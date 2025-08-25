use core::f32;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Result;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, Align2, Button, Color32, ComboBox, FontId, Key, Label, Modal, PointerButton, Pos2, Rect,
    Response, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2, Widget,
};
use eframe::epaint::PathShape;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;

#[cfg(feature = "profiling")]
use traviz::profiling;
use traviz::{
    analyze_dependency, analyze_span, builtin_relations, colors, edit_modes, edit_relations, modes,
    node_filter, persistent, relation, structured_modes, task_timer, types,
};

use analyze_dependency::{AnalyzeDependencyModal, DependencyLink};
use analyze_span::AnalyzeSpanModal;
use edit_modes::EditDisplayModes;
use edit_relations::{EditRelationViews, EditRelations};
use modes::structured_mode_transformation;
use node_filter::{EditNodeFilters, NodeFilter};
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use relation::{builtin_relation_views, find_relations, Relation, RelationInstance, RelationView};
use structured_modes::StructuredMode;
use task_timer::TaskTimer;
use types::{
    time_point_to_utc_string, value_to_text, DisplayLength, Event, HeightLevel, Node, Span,
    TimePoint, MILLISECONDS_PER_SECOND,
};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native("traviz", options, Box::new(|_cc| Ok(Box::<App>::default())))
}

#[derive(Debug)]
struct Timeline {
    absolute_start: TimePoint,
    absolute_end: TimePoint,

    visible_start: TimePoint,
    visible_end: TimePoint,

    selected_start: TimePoint,
    selected_end: TimePoint,
}

impl Timeline {
    fn init(&mut self, min_time: TimePoint, max_time: TimePoint) {
        self.absolute_start = min_time;
        self.absolute_end = max_time;

        self.visible_start = min_time;
        self.visible_end = (self.visible_start + 5.0).min(max_time);

        self.selected_start = min_time;
        self.selected_end = (self.selected_start + 1.0).min(max_time);
    }
}

/// Holds parameters for converting a `TimePoint` to a horizontal (X-axis) screen coordinate.
///
/// This struct encapsulates the current timeline view's mapping:
/// - `selected_start_time` and `selected_end_time`: Define the time range currently visible or selected.
/// - `visual_start_x` and `visual_end_x`: Define the corresponding pixel coordinates on the screen
///   where this time range begins and ends.
struct TimeToScreenParams {
    selected_start_time: TimePoint,
    selected_end_time: TimePoint,
    visual_start_x: f32,
    visual_end_x: f32,
}

fn screen_to_time(
    screen_x: f32,
    start_x: f32,
    end_x: f32,
    start_time: TimePoint,
    end_time: TimePoint,
) -> TimePoint {
    start_time + ((screen_x - start_x) / (end_x - start_x)) as f64 * (end_time - start_time)
}

fn time_to_screen(
    time: TimePoint,
    start_x: f32,
    end_x: f32,
    start_time: TimePoint,
    end_time: TimePoint,
) -> f32 {
    start_x + ((time - start_time) / (end_time - start_time)) as f32 * (end_x - start_x)
}

fn screen_change_to_time_change(
    screen_change: f32,
    screen_width: f32,
    start_time: TimePoint,
    end_time: TimePoint,
) -> TimePoint {
    let before = screen_to_time(0.0, 0.0, screen_width, start_time, end_time);
    let after = screen_to_time(screen_change, 0.0, screen_width, start_time, end_time);
    after - before
}

type NodeSpans = (Rc<Node>, Vec<Rc<Span>>);
/// A map from a node name to the node itself and a list of its associated spans.
type NodeSpansMap = BTreeMap<String, NodeSpans>;
/// Same as [NodeSpansMap] but in vector of pairs.
type NodeSpansVec = Vec<(String, NodeSpans)>;

struct App {
    layout: Layout,
    timeline: Timeline,
    raw_data: Vec<ExportTraceServiceRequest>,
    spans_to_display: Vec<Rc<Span>>,
    cached_node_spans: Option<NodeSpansMap>,
    timeline_bar1_time: TimePoint,
    timeline_bar2_time: TimePoint,
    clicked_span: Option<Rc<Span>>,
    include_children_events: bool,

    display_modes: Vec<StructuredMode>,
    current_display_mode_index: usize,

    node_filters: Vec<NodeFilter>,
    current_node_filter_index: usize,

    search: Search,
    edit_display_modes: EditDisplayModes,
    edit_node_filters: EditNodeFilters,
    edit_relations: EditRelations,
    edit_relation_views: EditRelationViews,

    // Analyze 'features'
    all_spans_for_analysis: Vec<Rc<Span>>,
    analyze_span_modal: AnalyzeSpanModal,
    analyze_dependency_modal: AnalyzeDependencyModal,

    // Spans highlighting
    highlighted_spans: Vec<Rc<Span>>,

    // Cache for span ID to root span lookup (for highlighted spans performance)
    span_id_to_root_cache: Option<HashMap<Vec<u8>, Rc<Span>>>,

    // Dependency arrow interactivity
    clicked_arrow_info: Option<ArrowInfo>,
    hovered_arrow_key: Option<ArrowKey>,

    cached_produce_block_starts: Option<Vec<(TimePoint, String)>>,

    defined_relations: Vec<Relation>,
    relation_views: Vec<RelationView>,
    current_relation_view_index: usize,
    active_relations: Vec<RelationInstance>,
}

struct Layout {
    top_bar_height: f32,
    timeline_height: f32,
    timeline_bar_width: f32,
    node_name_width: f32,
    span_name_threshold: f32,
    span_margin: f32,
    spans_time_points_height: f32,
    middle_bar_height: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ArrowKey {
    source_span_id: Vec<u8>,
    source_node_name: String,
    target_span_id: Vec<u8>,
    target_node_name: String,
}

#[derive(Clone, Debug)]
struct ArrowInfo {
    source_span_name: String,
    source_node_name: String,
    source_start_time: TimePoint,
    source_end_time: TimePoint,
    target_span_name: String,
    target_node_name: String,
    target_start_time: TimePoint,
    target_end_time: TimePoint,
    /// Duration of the link (target_start_time - source_end_time)
    duration: TimePoint,
}

impl Default for App {
    fn default() -> Self {
        let mut res = Self {
            layout: Layout {
                top_bar_height: 30.0,
                timeline_height: 90.0,
                timeline_bar_width: 10.0,
                node_name_width: 150.0,
                span_name_threshold: 100.0,
                span_margin: 3.0,
                spans_time_points_height: 80.0,
                middle_bar_height: 30.0,
            },
            timeline: Timeline {
                absolute_start: 0.0,
                absolute_end: 0.0,
                visible_start: 0.0,
                visible_end: 0.0,
                selected_start: 0.0,
                selected_end: 0.0,
            },
            raw_data: vec![],
            spans_to_display: vec![],
            timeline_bar1_time: 0.0,
            timeline_bar2_time: 0.0,
            clicked_span: None,
            include_children_events: true,
            display_modes: structured_modes::builtin_structured_modes(),
            current_display_mode_index: 0,
            node_filters: vec![NodeFilter::show_all(), NodeFilter::show_none()],
            current_node_filter_index: 0,
            search: Search::default(),
            edit_display_modes: EditDisplayModes::new(),
            edit_node_filters: EditNodeFilters::new(),
            edit_relations: EditRelations::new(),
            edit_relation_views: EditRelationViews::new(),
            all_spans_for_analysis: vec![],
            analyze_span_modal: AnalyzeSpanModal::default(),
            analyze_dependency_modal: AnalyzeDependencyModal::new(),
            highlighted_spans: Vec::new(),
            span_id_to_root_cache: None,
            clicked_arrow_info: None,
            hovered_arrow_key: None,
            cached_produce_block_starts: None,
            cached_node_spans: None,
            defined_relations: builtin_relations::builtin_relations(),
            relation_views: builtin_relation_views(),
            current_relation_view_index: 0,
            active_relations: vec![],
        };
        res.timeline.init(1.0, 3.0);
        res.set_timeline_end_bars_to_selected();
        res.search.search_term = "NOT IMPLEMENTED".to_string();

        res.load_peristent_data();

        // If a file path is provided as the first argument, try to load it.
        if let Some(first_arg) = std::env::args().nth(1) {
            println!("Trying to open file: {first_arg}");
            if let Err(err) = res.load_file(&PathBuf::from(first_arg)) {
                println!("Error loading file: {err}");
            } else {
                println!("File loaded successfully.");
            }
        }

        res
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(eframe::egui::Frame::new())
            .show(ctx, |ui| {
                ctx.options_mut(|o| o.line_scroll_speed = 100.0);
                ctx.style_mut(|s| {
                    s.interaction.tooltip_delay = 0.0;
                    s.interaction.tooltip_grace_time = 0.0;
                    s.interaction.show_tooltips_only_when_still = false;
                });

                let window_width = ui.max_rect().width();
                let window_height = ui.max_rect().height();

                let t = if false {
                    Some(TaskTimer::new("Drawing"))
                } else {
                    None
                };
                self.draw_top_bar(ui);

                let timeline_area = Rect::from_min_size(
                    Pos2::new(0.0, self.layout.top_bar_height),
                    Vec2::new(window_width, self.layout.timeline_height),
                );
                self.draw_timeline(timeline_area, ui, ctx);

                let middle_bar_area = Rect::from_min_size(
                    Pos2::new(0.0, timeline_area.max.y),
                    Vec2::new(window_width, self.layout.middle_bar_height),
                );
                self.draw_middle_bar(middle_bar_area, ui);

                let spans_area = Rect::from_min_size(
                    Pos2::new(0.0, middle_bar_area.max.y),
                    Vec2::new(window_width, window_height - middle_bar_area.max.y),
                );
                self.draw_spans(spans_area, ui, ctx);

                self.draw_clicked_span(ctx, window_width - 100.0, window_height - 100.0);

                if let Some(new_display_modes) =
                    self.edit_display_modes
                        .draw(ctx, window_width - 100.0, window_height - 100.0)
                {
                    self.display_modes = new_display_modes;
                    self.save_persistent_data();
                    if self.current_display_mode_index >= self.display_modes.len() {
                        self.current_display_mode_index = 0;
                    }
                    if let Err(e) = self.apply_current_mode() {
                        println!("Failed to apply display mode: {e}");
                    }
                }

                if let Some(new_node_filters) =
                    self.edit_node_filters
                        .draw(ctx, window_width - 100.0, window_height - 100.0)
                {
                    self.node_filters = new_node_filters;
                    self.save_persistent_data();
                    if self.current_node_filter_index >= self.node_filters.len() {
                        self.current_node_filter_index = 0;
                    }
                }

                if let Some((new_relations, new_relation_views)) =
                    self.edit_relations
                        .draw(window_width - 100.0, window_height - 100.0, ctx)
                {
                    self.defined_relations = new_relations;
                    self.relation_views = new_relation_views;
                    self.save_persistent_data();
                    self.apply_current_relations_view();
                }

                if let Some(new_relation_views) =
                    self.edit_relation_views
                        .draw(window_width - 100.0, window_height - 100.0, ctx)
                {
                    self.relation_views = new_relation_views;
                    self.save_persistent_data();
                    if self.current_relation_view_index >= self.relation_views.len() {
                        self.current_relation_view_index = 0;
                    }
                    self.apply_current_relations_view();
                }

                self.draw_analyze_span_modal(ctx, window_width - 200.0, window_height - 200.0);

                self.draw_analyze_dependency_modal(
                    ctx,
                    window_width - 200.0,
                    window_height - 200.0,
                );
                self.draw_clicked_arrow_popup(ctx, window_width - 150.0, window_height - 150.0);

                // If Ctrl+Q clicked, quit the app
                if ctx.input(|i| i.key_down(Key::Q) && i.modifiers.ctrl) {
                    std::process::exit(0);
                }

                t.inspect(|t| t.stop());
                #[cfg(feature = "profiling")]
                profiling::GLOBAL_PROFILER.increment_frame_count();
            });
    }
}

// TODO - implement search
#[allow(unused)]
#[derive(Default, Debug)]
struct Search {
    search_term: String,
    search_results: Vec<Rc<Span>>,
    matching_span_ids: HashSet<Vec<u8>>,
    // TODO - non functional for now
    hide_non_matching: bool,
}

impl App {
    fn draw_top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let open_file_button = ui.button("Open file");

            if open_file_button.clicked() {
                println!(
                    "Opened file picker. sometimes the file picker opens behind the main window :/"
                );
                // TODO - fix file picker

                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    println!("Loading file: {path:?}...");
                    match self.load_file(&path) {
                        Ok(()) => println!("Successfully loaded file."),
                        Err(e) => println!("Error loading file: {e}"),
                    }
                }
            }

            let previous_display_mode_index = self.current_display_mode_index;
            let current_mode_name = self
                .display_modes
                .get(self.current_display_mode_index)
                .map_or("Deleted".to_string(), |mode| mode.name.clone());
            ComboBox::new("mode chooser", "")
                .selected_text(format!("Display mode: {current_mode_name}"))
                .show_ui(ui, |ui| {
                    for (i, mode) in self.display_modes.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.current_display_mode_index,
                            i,
                            mode.name.to_string(),
                        );
                    }
                });
            if previous_display_mode_index != self.current_display_mode_index {
                if let Err(e) = self.apply_current_mode() {
                    println!("Failed to apply display mode: {e}");
                    // Go back to the previous mode
                    self.current_display_mode_index = previous_display_mode_index;
                }
            }

            let current_node_filter_name = self
                .node_filters
                .get(self.current_node_filter_index)
                .map_or("Deleted".to_string(), |filter| filter.name.clone());
            ComboBox::new("node filter chooser", "")
                .selected_text(format!("Node filter: {current_node_filter_name}"))
                .show_ui(ui, |ui| {
                    for (i, filter) in self.node_filters.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.current_node_filter_index,
                            i,
                            filter.name.to_string(),
                        );
                    }
                });

            let previous_relations_view_idx = self.current_relation_view_index;
            let current_relations_view_name = self
                .relation_views
                .get(self.current_relation_view_index)
                .map_or("Deleted".to_string(), |view| view.name.clone());
            ComboBox::new("relation view chooser", "")
                .selected_text(format!("Relation view: {current_relations_view_name}"))
                .show_ui(ui, |ui| {
                    for (i, view) in self.relation_views.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.current_relation_view_index,
                            i,
                            view.name.to_string(),
                        );
                    }
                });
            if previous_relations_view_idx != self.current_relation_view_index {
                self.apply_current_relations_view();
            }

            if ui.button("Edit display modes").clicked() {
                self.load_peristent_data();
                self.edit_display_modes.open(self.display_modes.clone());
            }

            if ui.button("Edit node filters").clicked() {
                self.load_peristent_data();
                self.edit_node_filters.open(self.node_filters.clone());
            }

            if ui.button("Edit relations").clicked() {
                self.load_peristent_data();
                self.edit_relations
                    .open(self.defined_relations.clone(), self.relation_views.clone());
            }

            if ui.button("Edit relation views").clicked() {
                self.load_peristent_data();
                self.edit_relation_views
                    .open(self.defined_relations.clone(), self.relation_views.clone());
            }

            // Analyze Span button, disabled if no spans are loaded
            let has_spans = !self.spans_to_display.is_empty();
            let analyze_button = ui.add_enabled(has_spans, Button::new("Analyze Span"));
            if analyze_button.clicked() {
                self.analyze_span_modal.open(&self.all_spans_for_analysis);
            }

            // Analyze Dependency button, disabled if no spans are loaded
            let analyze_dep_button = ui.add_enabled(has_spans, Button::new("Analyze Dependency"));
            if analyze_dep_button.clicked() {
                self.analyze_dependency_modal
                    .open(&self.all_spans_for_analysis);
            }

            // Clear Highlights button, only enabled when there are highlighted spans
            let has_highlights = !self.highlighted_spans.is_empty();
            ui.with_layout(
                egui::Layout::right_to_left(eframe::emath::Align::RIGHT),
                |ui| {
                    let clear_button =
                        ui.add_enabled(has_highlights, Button::new("Clear Highlights"));
                    if clear_button.clicked() {
                        println!(
                            "Clearing {} highlighted spans",
                            self.highlighted_spans.len()
                        );
                        self.highlighted_spans.clear();
                        self.analyze_dependency_modal.clear_focus();
                    }
                },
            );
        });
    }

    fn load_file(&mut self, path: &PathBuf) -> Result<()> {
        // Read json file
        let mut file_bytes = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut file_bytes)?;

        self.raw_data = parse_trace_file(&file_bytes)?;

        // Clear old data before loading new traces
        self.all_spans_for_analysis.clear();
        self.spans_to_display.clear();
        self.clicked_span = None;
        self.highlighted_spans.clear();
        self.span_id_to_root_cache = None;
        self.analyze_span_modal = AnalyzeSpanModal::default();
        self.analyze_dependency_modal = AnalyzeDependencyModal::new();
        self.cached_produce_block_starts = None;

        let everything_mode = self
            .display_modes
            .iter()
            .find(|m| m.name == "Everything")
            .expect("'Everything' display mode not found during initialization.");

        // Populate all_spans_for_analysis using the transformation from the "Everything" mode.
        self.all_spans_for_analysis =
            structured_mode_transformation(&self.raw_data, everything_mode)
                .expect("Failed to transform data using 'Everything' mode.");

        println!(
            "Stored {} spans from 'Everything' mode for analysis after file load.",
            self.all_spans_for_analysis.len()
        );

        // Populate the cache for produce_block_starts
        self.cached_produce_block_starts = Some(collect_produce_block_starts_with_nodes(
            &self.all_spans_for_analysis,
        ));

        self.apply_current_mode()?;
        let (min_time, max_time) = get_min_max_time(&self.spans_to_display).unwrap();
        self.timeline.init(min_time, max_time);
        self.set_timeline_end_bars_to_selected();

        Ok(())
    }

    fn apply_current_mode(&mut self) -> Result<()> {
        let mode = self
            .display_modes
            .get(self.current_display_mode_index)
            .ok_or_else(|| anyhow::anyhow!("Invalid display mode index"))?;

        self.spans_to_display = structured_mode_transformation(&self.raw_data, mode)?;
        set_min_max_time(&self.spans_to_display);
        self.cached_node_spans = None;

        self.apply_current_relations_view();

        Ok(())
    }

    fn apply_current_relations_view(&mut self) {
        let Some(view) = self.relation_views.get(self.current_relation_view_index) else {
            println!(
                "WARN: No relation view found at index {}",
                self.current_relation_view_index
            );
            return;
        };

        self.active_relations =
            find_relations(&self.defined_relations, view, &self.spans_to_display);
    }

    // TODO - make this better. Time points should shift when the timeline is moved, not stay in place.
    // Would give better visual feedback. Something like `get_time_dots`.
    fn draw_timeline(&mut self, area: Rect, ui: &mut Ui, ctx: &egui::Context) {
        let background_button = ui.put(
            area,
            Button::new("")
                .fill(colors::MILD_BLUE)
                .sense(Sense::click_and_drag()),
        );

        let timeline_bar1_pos = time_to_screen(
            self.timeline_bar1_time,
            area.min.x,
            area.max.x,
            self.timeline.visible_start,
            self.timeline.visible_end,
        );
        let bar1_button = ui.put(
            Rect::from_min_size(
                Pos2::new(
                    timeline_bar1_pos - self.layout.timeline_bar_width / 2.0,
                    area.min.y,
                ),
                Vec2::new(self.layout.timeline_bar_width, area.height()),
            ),
            Button::new("").sense(Sense::drag()),
        );
        let timeline_bar2_pos = time_to_screen(
            self.timeline_bar2_time,
            area.min.x,
            area.max.x,
            self.timeline.visible_start,
            self.timeline.visible_end,
        );
        let bar2_button = ui.put(
            Rect::from_min_size(
                Pos2::new(
                    timeline_bar2_pos - self.layout.timeline_bar_width / 2.0,
                    area.min.y,
                ),
                Vec2::new(self.layout.timeline_bar_width, area.height()),
            ),
            Button::new("").sense(Sense::drag()),
        );

        let mut middle_rect = Rect::from_min_max(
            Pos2::new(
                timeline_bar1_pos.min(timeline_bar2_pos) + self.layout.timeline_bar_width / 2.0,
                area.min.y,
            ),
            Pos2::new(
                timeline_bar1_pos.max(timeline_bar2_pos) - self.layout.timeline_bar_width / 2.0,
                area.max.y,
            ),
        );
        if middle_rect.min.x >= middle_rect.max.x || middle_rect.width() <= 8.0 {
            // TODO - for some reason egui crashes if middle_rect gets too small.
            middle_rect = Rect::from_pos(Pos2::new(-10000.0, -10000.0));
        }
        let middle_button = ui.put(
            middle_rect,
            Button::new("")
                .sense(Sense::drag())
                .fill(colors::LIGHT_BLUE),
        );

        // Dragging end of selected area should adjust the selected area
        if bar1_button.dragged_by(PointerButton::Primary) {
            self.timeline_bar1_time += screen_change_to_time_change(
                bar1_button.drag_delta().x,
                area.width(),
                self.timeline.visible_start,
                self.timeline.visible_end,
            );
            self.set_timeline_selected_to_end_bars();
        }
        if bar2_button.dragged_by(PointerButton::Primary) {
            self.timeline_bar2_time += screen_change_to_time_change(
                bar2_button.drag_delta().x,
                area.width(),
                self.timeline.visible_start,
                self.timeline.visible_end,
            );
            self.set_timeline_selected_to_end_bars();
        }

        // Dragging middle of selected area should shift the selected area
        if middle_button.dragged_by(PointerButton::Primary) {
            let time_shift = screen_change_to_time_change(
                middle_button.drag_delta().x,
                area.width(),
                self.timeline.visible_start,
                self.timeline.visible_end,
            );
            self.timeline_bar1_time += time_shift;
            self.timeline_bar2_time += time_shift;
            self.set_timeline_selected_to_end_bars();
        }

        // Dragging with RMB should shift the whole timeline
        let maybe_shift_x: Option<f32> = if background_button.dragged_by(PointerButton::Secondary) {
            Some(background_button.drag_delta().x)
        } else if bar1_button.dragged_by(PointerButton::Secondary) {
            Some(bar1_button.drag_delta().x)
        } else if bar2_button.dragged_by(PointerButton::Secondary) {
            Some(bar2_button.drag_delta().x)
        } else if middle_button.dragged_by(PointerButton::Secondary) {
            Some(middle_button.drag_delta().x)
        } else {
            None
        };
        if let Some(shift_x) = maybe_shift_x {
            let time_shift = screen_change_to_time_change(
                shift_x,
                area.width(),
                self.timeline.visible_start,
                self.timeline.visible_end,
            );
            self.timeline.visible_start -= time_shift;
            self.timeline.visible_end -= time_shift;
        }

        // Handle scrolling to zoom in/out
        ctx.input(|input| {
            if (background_button.hovered()
                || bar1_button.hovered()
                || bar2_button.hovered()
                || middle_button.hovered())
                && input.raw_scroll_delta.y != 0.0
            {
                let scale = 1.0 - input.raw_scroll_delta.y / 200.0;
                let Some(latest_pos) = input.pointer.latest_pos() else {
                    return; // latest_pos can sometimes be None here.
                };
                let mouse_time = screen_to_time(
                    latest_pos.x,
                    area.min.x,
                    area.max.x,
                    self.timeline.visible_start,
                    self.timeline.visible_end,
                );
                let len_before = self.timeline.visible_end - self.timeline.visible_start;
                let len_after = len_before * scale as f64;
                let new_start =
                    mouse_time - (mouse_time - self.timeline.visible_start) * scale as f64;
                let new_end = new_start + len_after;
                self.timeline.visible_start = new_start;
                self.timeline.visible_end = new_end;
            }
        });

        self.draw_time_points(
            self.timeline.visible_start,
            self.timeline.visible_end,
            self.timeline.absolute_start,
            area,
            colors::GRAY_50,
            ui,
        );
    }

    fn set_timeline_selected_to_end_bars(&mut self) {
        self.timeline.selected_start = self.timeline_bar1_time.min(self.timeline_bar2_time);
        self.timeline.selected_end = self.timeline_bar1_time.max(self.timeline_bar2_time);
    }

    fn set_timeline_end_bars_to_selected(&mut self) {
        self.timeline_bar1_time = self.timeline.selected_start;
        self.timeline_bar2_time = self.timeline.selected_end;
    }

    fn draw_time_points(
        &self,
        start_time: TimePoint,
        end_time: TimePoint,
        absolute_start: TimePoint,
        area: Rect,
        color: Color32,
        ui: &mut Ui,
    ) {
        for dot in get_time_dots(start_time, end_time) {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(
                        time_to_screen(dot, area.min.x, area.max.x, start_time, end_time),
                        (area.min.y + area.max.y) / 2.0,
                    ),
                    Vec2::new(2.0, 2.0),
                ),
                1.0,
                color,
            );
        }

        let mut cur_pos = area.min.x;
        while cur_pos < area.max.x {
            let cur_time = screen_to_time(cur_pos, area.min.x, area.max.x, start_time, end_time);
            let time_str = time_point_to_utc_string(cur_time);
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(cur_pos, area.min.y), Vec2::new(2.0, 30.0)),
                0.0,
                color,
            );
            let text_rect = ui.painter().text(
                Pos2::new(cur_pos + 4.0, area.min.y),
                Align2::LEFT_TOP,
                time_str,
                FontId::default(),
                color,
            );
            let time_since_start_str = format!("{:.3} s", cur_time - absolute_start);
            ui.painter().text(
                Pos2::new(text_rect.min.x, text_rect.max.y + 4.0),
                Align2::LEFT_TOP,
                time_since_start_str,
                FontId::default(),
                color,
            );
            cur_pos += text_rect.width() + 50.0;
        }

        // Draw red lines for produce_block
        let produce_block_starts_data = self
            .cached_produce_block_starts
            .as_ref()
            .cloned()
            .unwrap_or_else(|| {
                if self.all_spans_for_analysis.is_empty() {
                    // If there are no spans, no need to reconstruct, return empty.
                    Vec::new()
                } else {
                    println!(
                        "Reconstructing produce_block_starts in draw_time_points (cache was None)..."
                    );
                    collect_produce_block_starts_with_nodes(&self.all_spans_for_analysis)
                }
            });

        for (t_ref, node_name) in &produce_block_starts_data {
            let t = *t_ref;
            if (t >= start_time) && (t <= end_time) {
                let x = time_to_screen(t, area.min.x, area.max.x, start_time, end_time);
                let marker_height = 20.0;
                ui.painter().line_segment(
                    [
                        Pos2::new(x, area.max.y),
                        Pos2::new(x, area.max.y - marker_height),
                    ],
                    Stroke::new(2.0, colors::RED),
                );

                // Remove "neard:" prefix if present
                let short_node_name = node_name.strip_prefix("neard:").unwrap_or(node_name);

                // Draw node name
                let small_font_id =
                    FontId::proportional(0.6 * egui::TextStyle::Body.resolve(ui.style()).size);
                ui.painter().text(
                    Pos2::new(x + 4.0, area.max.y - 10.0),
                    Align2::LEFT_TOP,
                    short_node_name,
                    small_font_id,
                    colors::RED,
                );

                // Draw indicator for produce_block
                let label_rect = Rect::from_min_size(
                    Pos2::new(area.min.x - 15.0, area.max.y - 16.0),
                    Vec2::new(90.0, 18.0),
                );
                ui.painter()
                    .rect_filled(label_rect, 3.0, colors::TRANSPARENT);
                let font_id =
                    FontId::proportional(0.7 * egui::TextStyle::Body.resolve(ui.style()).size);
                ui.painter().text(
                    label_rect.center(),
                    Align2::CENTER_CENTER,
                    "produce_block",
                    font_id,
                    color,
                );
            }
        }
    }

    fn draw_middle_bar(&mut self, area: Rect, ui: &mut Ui) {
        ui.painter().rect_filled(area, 0.0, colors::GRAY_10);

        let top_margin = 5;
        let ui_area = Rect::from_min_max(
            Pos2::new(area.min.x, area.min.y + top_margin as f32),
            area.max,
        );
        ui.allocate_new_ui(UiBuilder::new().max_rect(ui_area), |ui| {
            ui.horizontal(|ui| {
                TextEdit::singleline(&mut self.search.search_term)
                    .background_color(colors::GRAY_40)
                    .ui(ui);
                ui.button("Search").clicked();
                ui.button("Next").clicked();
                ui.checkbox(&mut self.search.hide_non_matching, "Hide non-matching")
                    .clicked();
            });
        });
    }

    fn draw_spans(&mut self, area: Rect, ui: &mut Ui, ctx: &egui::Context) {
        #[cfg(feature = "profiling")]
        let _timing_guard = profiling::GLOBAL_PROFILER.start_timing("draw_spans");

        let mut final_spans_for_drawing_owned: Option<Vec<Rc<Span>>> = None;
        if !self.highlighted_spans.is_empty() {
            #[cfg(feature = "profiling")]
            let _timing_guard_highlight =
                profiling::GLOBAL_PROFILER.start_timing("highlighted_spans_processing");

            // Build the cache if it doesn't exist
            if self.span_id_to_root_cache.is_none() {
                #[cfg(feature = "profiling")]
                let _timing_guard_cache_build =
                    profiling::GLOBAL_PROFILER.start_timing("build_span_id_cache");

                let mut cache = HashMap::new();
                for root_span in &self.all_spans_for_analysis {
                    populate_span_cache_recursive(root_span, root_span, &mut cache);
                }
                self.span_id_to_root_cache = Some(cache);
            }

            // Use the cache for fast lookups
            let cache = self.span_id_to_root_cache.as_ref().unwrap();
            let mut roots_to_add_if_highlighted: Vec<Rc<Span>> = Vec::new();
            let mut current_display_plus_new_root_ids: HashSet<Vec<u8>> = self
                .spans_to_display
                .iter()
                .map(|s| s.span_id.clone())
                .collect();

            for highlighted_span_rc in &self.highlighted_spans {
                if let Some(root_span) = cache.get(&highlighted_span_rc.span_id) {
                    if current_display_plus_new_root_ids.insert(root_span.span_id.clone()) {
                        roots_to_add_if_highlighted.push(root_span.clone());
                    }
                }
            }

            if !roots_to_add_if_highlighted.is_empty() {
                let mut temp_spans = self.spans_to_display.clone();
                temp_spans.extend(roots_to_add_if_highlighted);
                final_spans_for_drawing_owned = Some(temp_spans);
            }
        }

        let spans_to_render = final_spans_for_drawing_owned
            .as_ref()
            .unwrap_or(&self.spans_to_display);

        let node_spans_items_for_loop: NodeSpansVec;

        if final_spans_for_drawing_owned.is_some() {
            #[cfg(feature = "profiling")]
            let _timing_guard_node_map =
                profiling::GLOBAL_PROFILER.start_timing("build_temp_node_map_for_highlights");

            // Highlights are active and modified the span list, build a temporary map
            let mut temp_map_for_highlight: NodeSpansMap = BTreeMap::new();
            for span_ref in spans_to_render {
                temp_map_for_highlight
                    .entry(span_ref.node.name.clone())
                    .or_insert((span_ref.node.clone(), vec![]))
                    .1
                    .push(span_ref.clone());
            }
            node_spans_items_for_loop = temp_map_for_highlight.into_iter().collect();
        } else {
            // No active highlights modifying the list, or no highlights at all. Use cache.
            if self.cached_node_spans.is_none() {
                let mut node_spans_map: NodeSpansMap = BTreeMap::new();
                for span in &self.spans_to_display {
                    node_spans_map
                        .entry(span.node.name.clone())
                        .or_insert((span.node.clone(), vec![]))
                        .1
                        .push(span.clone());
                }
                self.cached_node_spans = Some(node_spans_map);
            }
            node_spans_items_for_loop = self
                .cached_node_spans
                .as_ref()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }

        let time_points_area = Rect::from_min_max(
            Pos2::new(area.min.x + self.layout.node_name_width, area.min.y),
            Pos2::new(
                area.max.x,
                area.min.y + self.layout.spans_time_points_height,
            ),
        );

        ui.painter().rect_filled(
            Rect::from_min_max(area.min, time_points_area.max),
            0.0,
            colors::BLUE_DARK_GRAY,
        );
        self.draw_time_points(
            self.timeline.selected_start,
            self.timeline.selected_end,
            self.timeline.absolute_start,
            time_points_area,
            colors::GRAY_240,
            ui,
        );

        let under_time_points_area =
            Rect::from_two_pos(Pos2::new(area.min.x, time_points_area.max.y), area.max);

        let node_names_area = Rect::from_min_max(
            Pos2::new(area.min.x, time_points_area.max.y),
            Pos2::new(
                area.min.x + self.layout.node_name_width,
                under_time_points_area.max.y,
            ),
        );

        if spans_to_render.is_empty() {
            ui.put(
                Rect::from_center_size(under_time_points_area.center(), Vec2::new(200.0, 200.0)),
                Label::new("No spans to display.\nOpen a file or change filters."),
            );
            return;
        }

        ui.allocate_new_ui(UiBuilder::new().max_rect(under_time_points_area), |ui| {
            ScrollArea::vertical()
                .max_height(under_time_points_area.height())
                .max_width(under_time_points_area.width())
                .auto_shrink(false)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .animated(false)
                .show_viewport(ui, |ui, visible_rect| {
                    ui.style_mut().spacing.button_padding = Vec2::ZERO;
                    ui.style_mut().visuals.override_text_color = Some(colors::BLACK);

                    // TODO - a button for background feels hacky x.x
                    let background_button = ui.put(
                        under_time_points_area,
                        Button::new("")
                            .fill(colors::GRAY_30)
                            .sense(Sense::click_and_drag()),
                    );
                    if background_button.dragged_by(PointerButton::Secondary) {
                        self.shift_selected_time(screen_change_to_time_change(
                            -background_button.drag_delta().x,
                            under_time_points_area.width() - self.layout.node_name_width,
                            self.timeline.selected_start,
                            self.timeline.selected_end,
                        ));
                    }

                    let span_height = ui.fonts(|fs| {
                        fs.layout_no_wrap("A".to_string(), FontId::default(), colors::BLACK)
                            .rect
                            .height()
                    }) * 1.2;
                    self.layout.span_name_threshold = ui.fonts(|fs| {
                        fs.layout_no_wrap("...".to_string(), FontId::default(), colors::BLACK)
                            .rect
                            .width()
                    });

                    let mut cur_height = under_time_points_area.min.y - visible_rect.min.y;

                    let mut span_positions: HashMap<Vec<u8>, f32> = HashMap::new();

                    let highlighted_span_ids_set: HashSet<Vec<u8>> =
                        if !self.highlighted_spans.is_empty() {
                            self.highlighted_spans
                                .iter()
                                .map(|s| s.span_id.clone())
                                .collect()
                        } else {
                            HashSet::new()
                        };

                    let time_params = TimeToScreenParams {
                        selected_start_time: self.timeline.selected_start,
                        selected_end_time: self.timeline.selected_end,
                        visual_start_x: under_time_points_area.min.x + self.layout.node_name_width,
                        visual_end_x: under_time_points_area.max.x,
                    };
                    for (node_name, (_node, spans)) in node_spans_items_for_loop {
                        // TODO - filter spans before displaying, like display modes. It'd work better with search etc.
                        if let Some(current_node_filter) =
                            self.node_filters.get(self.current_node_filter_index)
                        {
                            if !current_node_filter.should_show_span(&node_name) {
                                continue;
                            }
                        }

                        let spans_in_range: Vec<Rc<Span>> = spans
                            .iter()
                            .filter(|s| {
                                is_intersecting(
                                    s.min_start_time.get(),
                                    s.max_end_time.get(),
                                    self.timeline.selected_start,
                                    self.timeline.selected_end,
                                )
                            })
                            .cloned()
                            .collect();

                        set_display_children_with_highlights(
                            &spans_in_range,
                            &self.highlighted_spans,
                        );

                        #[cfg(feature = "profiling")]
                        let _timing_guard_display_params = profiling::GLOBAL_PROFILER
                            .start_timing("set_display_params_with_highlights");

                        Self::set_display_params_with_highlights(
                            &spans_in_range,
                            &highlighted_span_ids_set,
                            self.timeline.selected_start,
                            self.timeline.selected_end,
                            under_time_points_area.min.x + self.layout.node_name_width,
                            under_time_points_area.max.x,
                            ui,
                        );

                        #[cfg(feature = "profiling")]
                        let _timing_guard_arrange =
                            profiling::GLOBAL_PROFILER.start_timing("arrange_spans");

                        let bbox = arrange_spans_with_viewport(
                            &spans_in_range,
                            true,
                            self.timeline.selected_start,
                            self.timeline.selected_end,
                        );

                        if !highlighted_span_ids_set.is_empty() || !self.active_relations.is_empty()
                        {
                            #[cfg(feature = "profiling")]
                            let _timing_guard_positions =
                                profiling::GLOBAL_PROFILER.start_timing("collect_span_positions");

                            self.collect_span_positions(
                                &spans_in_range,
                                cur_height,
                                span_height,
                                &mut span_positions,
                            );
                        }

                        ui.style_mut().visuals.override_text_color = Some(colors::BLACK);
                        self.draw_arranged_spans(
                            &spans_in_range,
                            ui,
                            cur_height,
                            span_height,
                            0,
                            &highlighted_span_ids_set,
                        );

                        let next_height = cur_height
                            + bbox.height as f32 * (span_height + self.layout.span_margin);
                        ui.style_mut().visuals.override_text_color = Some(colors::WHITE);

                        let line_color = colors::GRAY_230;
                        ui.put(
                            Rect::from_min_max(
                                Pos2::new(node_names_area.min.x, cur_height),
                                Pos2::new(node_names_area.max.x, next_height),
                            ),
                            Button::new(node_name)
                                .fill(colors::ALMOST_BLACK)
                                .stroke(Stroke::new(1.0, line_color)),
                        );
                        ui.painter().line(
                            vec![
                                Pos2::new(area.min.x, next_height),
                                Pos2::new(area.max.x, next_height),
                            ],
                            Stroke::new(1.0, line_color),
                        );
                        cur_height = next_height;
                    }

                    // Draw dependency arrows if needed
                    if !highlighted_span_ids_set.is_empty() {
                        #[cfg(feature = "profiling")]
                        let _timing_guard_arrows =
                            profiling::GLOBAL_PROFILER.start_timing("draw_dependency_links");

                        self.draw_dependency_links(ui, &span_positions, &time_params, ctx);
                    }

                    self.draw_relation_links(&span_positions, &time_params, ui, ctx);

                    ui.input(|i| {
                        if i.zoom_delta() != 1.0 {
                            let diff = i.zoom_delta() - 1.0;
                            let delta = (1.0 - diff * 0.3) as f64;
                            let Some(latest_pos) = i.pointer.latest_pos() else {
                                return; // latest_pos can sometimes be None here.
                            };

                            let mouse_time = screen_to_time(
                                latest_pos.x,
                                area.min.x + self.layout.node_name_width,
                                area.max.x,
                                self.timeline.selected_start,
                                self.timeline.selected_end,
                            );
                            let selected_len_before =
                                self.timeline.selected_end - self.timeline.selected_start;
                            let selected_len_after = selected_len_before * delta;
                            let new_selected_start =
                                mouse_time - (mouse_time - self.timeline.selected_start) * delta;
                            let new_selected_end = new_selected_start + selected_len_after;

                            let visible_len_before =
                                self.timeline.visible_end - self.timeline.visible_start;
                            let visible_len_after = visible_len_before * delta;
                            let new_visible_start =
                                mouse_time - (mouse_time - self.timeline.visible_start) * delta;
                            let new_visible_end = new_visible_start + visible_len_after;

                            self.timeline.selected_start = new_selected_start;
                            self.timeline.selected_end = new_selected_end;
                            self.timeline.visible_start = new_visible_start;
                            self.timeline.visible_end = new_visible_end;
                            self.set_timeline_end_bars_to_selected();
                        }
                    })
                });
        });
    }

    fn shift_selected_time(&mut self, shift: TimePoint) {
        self.timeline.selected_start += shift;
        self.timeline.selected_end += shift;
        self.timeline.visible_start += shift;
        self.timeline.visible_end += shift;
        self.timeline_bar1_time += shift;
        self.timeline_bar2_time += shift;
    }

    fn set_display_params_with_highlights(
        spans: &[Rc<Span>],
        highlighted_span_ids: &HashSet<Vec<u8>>,
        start_time: TimePoint,
        end_time: TimePoint,
        start_pos: f32,
        end_pos: f32,
        ui: &Ui,
    ) {
        for span in spans {
            let is_highlighted = highlighted_span_ids.contains(&span.span_id);

            let start_x = time_to_screen(span.start_time, start_pos, end_pos, start_time, end_time);
            let time_display_len =
                time_to_screen(span.end_time, start_pos, end_pos, start_time, end_time) - start_x;

            let display_mode = if is_highlighted {
                DisplayLength::Text
            } else {
                span.display_options.display_length
            };

            let display_len = match display_mode {
                DisplayLength::Time => time_display_len,
                DisplayLength::Text => {
                    let text_len = ui.fonts(|fs| {
                        fs.layout_no_wrap(span.name.to_string(), FontId::default(), colors::BLACK)
                            .rect
                            .width()
                    });
                    text_len.max(time_display_len)
                }
            };
            span.display_start.set(start_x);
            span.display_length.set(display_len);
            span.time_display_length.set(time_display_len);

            // Filter children to only include those that intersect with the viewport
            let all_children = span.display_children.borrow();
            let viewport_culled_children: Vec<Rc<Span>> = all_children
                .iter()
                .filter(|child| {
                    is_intersecting(
                        child.min_start_time.get(),
                        child.max_end_time.get(),
                        start_time,
                        end_time,
                    )
                })
                .cloned()
                .collect();

            Self::set_display_params_with_highlights(
                &viewport_culled_children,
                highlighted_span_ids,
                start_time,
                end_time,
                start_pos,
                end_pos,
                ui,
            );
        }
    }

    fn draw_arranged_spans(
        &mut self,
        spans: &[Rc<Span>],
        ui: &mut Ui,
        start_height: f32,
        span_height: f32,
        level: u64,
        highlighted_span_ids: &HashSet<Vec<u8>>,
    ) {
        #[cfg(feature = "profiling")]
        let _timing_guard = profiling::GLOBAL_PROFILER.start_timing("draw_arranged_spans");

        for span_rc in spans {
            let current_span_draw_y = start_height
                + span_rc.parent_height_offset.get() as f32
                    * (span_height + self.layout.span_margin);

            if current_span_draw_y <= ui.clip_rect().max.y
                && (current_span_draw_y + span_height) >= ui.clip_rect().min.y
            {
                self.draw_arranged_span(
                    span_rc,
                    ui,
                    current_span_draw_y,
                    span_height,
                    level,
                    highlighted_span_ids,
                );
            }

            // Recurse for children, applying culling if the entire children's block is off-screen.
            let children_block_start_y =
                current_span_draw_y + span_height + self.layout.span_margin;

            if children_block_start_y <= ui.clip_rect().max.y {
                let display_children = span_rc.display_children.borrow();
                if !display_children.is_empty() {
                    self.draw_arranged_spans(
                        display_children.as_slice(),
                        ui,
                        children_block_start_y,
                        span_height,
                        level + 1,
                        highlighted_span_ids,
                    );
                }
            }
        }
    }

    fn draw_arranged_span(
        &mut self,
        span: &Rc<Span>,
        ui: &mut Ui,
        start_height: f32,
        span_height: f32,
        level: u64,
        highlighted_span_ids: &HashSet<Vec<u8>>,
    ) {
        if span.active_segments.is_some() {
            self.draw_grouped_span(
                span,
                ui,
                start_height,
                span_height,
                level,
                highlighted_span_ids,
            );
            return;
        }

        let is_highlighted = highlighted_span_ids.contains(&span.span_id);
        let visible_rect = ui.clip_rect();

        // Check if the current span itself is visible and draw it
        if start_height <= visible_rect.max.y && (start_height + span_height) >= visible_rect.min.y
        {
            let start_x = span.display_start.get();
            // Ensure display_length is not negative
            let end_x = start_x + span.display_length.get().max(0.0);

            let name = if end_x - start_x > self.layout.span_name_threshold {
                span.name.as_str()
            } else {
                ""
            };

            // Set colors based on whether it's a highlighted span
            let (time_color, base_color) = if is_highlighted {
                // Use blue color for highlighted spans
                (colors::INTENSE_BLUE, colors::VERY_LIGHT_BLUE)
            } else {
                // Use yellow/gold colors for normal spans
                (colors::DARK_YELLOW, colors::VERY_LIGHT_YELLOW)
            };

            let time_rect = Rect::from_min_max(
                Pos2::new(start_x, start_height),
                Pos2::new(
                    start_x + span.time_display_length.get().max(0.0),
                    start_height + span_height,
                ),
            );
            let display_rect = Rect::from_min_max(
                Pos2::new(start_x, start_height),
                Pos2::new(end_x, start_height + span_height),
            );
            ui.painter().rect_filled(display_rect, 0, base_color);
            ui.painter().rect_filled(time_rect, 0, time_color);

            // Highlighted spans also have a nice border around them
            if is_highlighted {
                let border_stroke = Stroke::new(2.5, colors::INTENSE_BLUE2);
                let points = vec![
                    display_rect.min,
                    Pos2::new(display_rect.max.x, display_rect.min.y),
                    display_rect.max,
                    Pos2::new(display_rect.min.x, display_rect.max.y),
                ];
                let border_shape = PathShape::closed_line(points, border_stroke);
                ui.painter().add(border_shape);
            }

            if level == 0 {
                // Top level spans get a color line at the top
                ui.painter().line(
                    vec![
                        Pos2::new(start_x, start_height),
                        Pos2::new(end_x, start_height),
                    ],
                    Stroke::new(2.0, colors::INTENSE_RED),
                );
            }

            let span_button = ui.put(
                display_rect,
                Button::new(name)
                    .truncate()
                    .fill(colors::transparent_yellow()),
            );

            if span_button.clicked_by(PointerButton::Primary) {
                self.clicked_span = Some(span.clone());
            }

            if span_button.clicked_by(PointerButton::Middle) {
                span.collapse_children.set(!span.collapse_children.get());
            }

            span_button.on_hover_ui_at_pointer(|ui| {
                ui.label(span.name.clone());
                ui.separator();
                ui.label(format!(
                    "{:.3} ms",
                    (span.end_time - span.start_time) * MILLISECONDS_PER_SECOND
                ));
                ui.label(format!(
                    "{} - {}",
                    time_point_to_utc_string(span.start_time),
                    time_point_to_utc_string(span.end_time)
                ));
                ui.label(format!("span_id: {}", hex::encode(&span.span_id)));
                ui.label(format!(
                    "parent_span_id: {}",
                    hex::encode(&span.parent_span_id)
                ));
                ui.separator();
                for (name, value) in &span.attributes {
                    ui.label(format!("{}: {}", name, value_to_text(value)));
                }
                ui.separator();
                let num_events = count_events(span);
                ui.label(format!(
                    "Events: (this span: {}) (including children: {})",
                    span.events.len(),
                    num_events
                ));
            });
        }
    }

    fn add_grouped_span_hover_tooltip(&self, span_button: Response, span: &Span) {
        span_button.on_hover_ui_at_pointer(|ui| {
            ui.label(span.name.clone().to_string());
            ui.separator();

            ui.label(format!(
                "{:.3} ms",
                (span.end_time - span.start_time) * MILLISECONDS_PER_SECOND
            ));
            ui.label(format!(
                "{} - {}",
                time_point_to_utc_string(span.start_time),
                time_point_to_utc_string(span.end_time)
            ));
            ui.separator();

            if let Some(Some(Value::StringValue(spans_info))) =
                span.attributes.get("grouped_spans_info")
            {
                ui.label("Individual Spans:");
                for line in spans_info.lines() {
                    ui.label(format!("- {line}"));
                }
            } else {
                ui.label("(No individual span info found)");
            }
        });
    }

    fn grouped_span_segment_to_rect(
        segment_start: f64,
        segment_end: f64,
        span: &Span,
        start_x: f32,
        end_x: f32,
        start_height: f32,
        span_height: f32,
    ) -> Rect {
        let time_range = span.end_time - span.start_time;
        let screen_width = end_x - start_x;
        let segment_start_x =
            start_x + ((segment_start - span.start_time) / time_range * screen_width as f64) as f32;
        let segment_end_x =
            start_x + ((segment_end - span.start_time) / time_range * screen_width as f64) as f32;

        // Minimum width for grouped span segments
        const MIN_SEGMENT_WIDTH: f32 = 4.0;

        let calculated_width = segment_end_x - segment_start_x;
        let (actual_start_x, actual_end_x) = if calculated_width < MIN_SEGMENT_WIDTH {
            // Center the minimum width segment on the original segment center
            let center_x = (segment_start_x + segment_end_x) / 2.0;
            let half_min_width = MIN_SEGMENT_WIDTH / 2.0;
            let new_start_x = (center_x - half_min_width).max(start_x); // Don't go before span start
            let new_end_x = (center_x + half_min_width).min(end_x); // Don't go after span end

            // If centering would exceed bounds, adjust to fit within span
            if new_end_x - new_start_x < MIN_SEGMENT_WIDTH {
                if new_start_x == start_x {
                    (new_start_x, (new_start_x + MIN_SEGMENT_WIDTH).min(end_x))
                } else {
                    ((new_end_x - MIN_SEGMENT_WIDTH).max(start_x), new_end_x)
                }
            } else {
                (new_start_x, new_end_x)
            }
        } else {
            (segment_start_x, segment_end_x)
        };

        Rect::from_min_max(
            Pos2::new(actual_start_x, start_height),
            Pos2::new(actual_end_x, start_height + span_height),
        )
    }

    fn draw_grouped_span(
        &mut self,
        span: &Rc<Span>,
        ui: &mut Ui,
        start_height: f32,
        span_height: f32,
        level: u64,
        highlighted_span_ids: &HashSet<Vec<u8>>,
    ) {
        let visible_rect = ui.clip_rect();

        // Exit early if not visible
        if start_height > visible_rect.max.y || (start_height + span_height) < visible_rect.min.y {
            return;
        }

        let is_highlighted = highlighted_span_ids.contains(&span.span_id);
        let start_x = span.display_start.get();
        let end_x = start_x + span.display_length.get().max(0.0);
        // Time-accurate end X for mapping active segments (stripes) within true time range
        let time_end_x = start_x + span.time_display_length.get().max(0.0);

        let name = if end_x - start_x > self.layout.span_name_threshold {
            span.name.as_str()
        } else {
            ""
        };

        let (active_color, gap_color) = if is_highlighted {
            (colors::INTENSE_BLUE, colors::VERY_LIGHT_BLUE)
        } else {
            (colors::DARK_YELLOW, colors::VERY_LIGHT_YELLOW)
        };

        // Draw the full span range with gap color
        let full_rect = Rect::from_min_max(
            Pos2::new(start_x, start_height),
            Pos2::new(end_x, start_height + span_height),
        );
        ui.painter().rect_filled(full_rect, 0, gap_color);

        // Draw active segments with active color and collect rects for interaction
        let mut segment_rects = Vec::new();
        if let Some(active_segments) = &span.active_segments {
            // Note: Empty vector means span is marked for grouping but not yet processed
            // (this shouldn't happen!)
            for (segment_start, segment_end) in active_segments {
                let segment_rect = Self::grouped_span_segment_to_rect(
                    *segment_start,
                    *segment_end,
                    span,
                    start_x,
                    time_end_x,
                    start_height,
                    span_height,
                );
                ui.painter().rect_filled(segment_rect, 0, active_color);
                segment_rects.push(segment_rect);
            }
        }

        // Highlighted spans get a border
        if is_highlighted {
            let border_stroke = Stroke::new(2.5, colors::INTENSE_BLUE2);
            let points = vec![
                full_rect.min,
                Pos2::new(full_rect.max.x, full_rect.min.y),
                full_rect.max,
                Pos2::new(full_rect.min.x, full_rect.max.y),
            ];
            let border_shape = PathShape::closed_line(points, border_stroke);
            ui.painter().add(border_shape);
        }

        if level == 0 {
            // Grouped spans get a color line at the top
            ui.painter().line(
                vec![
                    Pos2::new(start_x, start_height),
                    Pos2::new(end_x, start_height),
                ],
                Stroke::new(2.0, colors::INTENSE_GREEN),
            );
        }

        let full_span_button = ui.put(
            full_rect,
            Button::new(name).truncate().fill(Color32::TRANSPARENT),
        );

        if full_span_button.clicked_by(PointerButton::Primary) {
            self.clicked_span = Some(span.clone());
        }

        self.add_grouped_span_hover_tooltip(full_span_button, span);
    }

    fn draw_clicked_span(&mut self, ctx: &egui::Context, max_width: f32, max_height: f32) {
        if self.clicked_span.is_none() {
            return;
        }

        Modal::new("clicked span".into()).show(ctx, |ui| {
            ui.vertical(|ui| {
                let span = self.clicked_span.as_ref().unwrap();
                ui.set_max_width(max_width);
                ui.set_max_height(max_height);

                let draw_separator = |ui: &mut Ui| {
                    ui.set_max_width(10.0);
                    ui.separator();
                    ui.set_max_width(max_width);
                };

                let close_button = ui.button("Close");
                draw_separator(ui);
                ui.label(span.name.clone());
                ui.label("");
                ui.label(format!(
                    "{:.3} ms",
                    (span.end_time - span.start_time) * MILLISECONDS_PER_SECOND
                ));
                ui.label(format!(
                    "{} - {}",
                    time_point_to_utc_string(span.start_time),
                    time_point_to_utc_string(span.end_time)
                ));

                if span.active_segments.is_some() {
                    // Grouped span
                    draw_separator(ui);
                    if let Some(Some(Value::StringValue(spans_info))) =
                        span.attributes.get("grouped_spans_info")
                    {
                        ui.label("Individual Spans:");
                        ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for line in spans_info.lines() {
                                ui.label(format!("- {line}"));
                            }
                        });
                    }
                } else {
                    // Regular span
                    ui.label(format!("span_id: {}", hex::encode(&span.span_id)));
                    ui.label(format!(
                        "parent_span_id: {}",
                        hex::encode(&span.parent_span_id)
                    ));
                    draw_separator(ui);
                    for (name, value) in &span.attributes {
                        ui.label(format!("{}: {}", name, value_to_text(value)));
                    }
                    draw_separator(ui);

                    let mut events = if self.include_children_events {
                        collect_events(span)
                    } else {
                        span.events.clone()
                    };
                    events.sort_by(|e1, e2| {
                        e1.time
                            .partial_cmp(&e2.time)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    ui.label("");
                    ui.label(format!("Events ({})", events.len()));
                    ui.checkbox(
                        &mut self.include_children_events,
                        "Include events from children spans",
                    );
                    draw_separator(ui);
                    ScrollArea::vertical().show(ui, |ui| {
                        for event in events {
                            draw_separator(ui);
                            ui.label(time_point_to_utc_string(event.time));
                            ui.label(event.name);
                            ui.label("");
                            for (name, value) in event.attributes {
                                ui.label(format!("{}: {}", name, value_to_text(&value)));
                            }
                        }
                    });
                }

                if close_button.clicked() {
                    self.clicked_span = None;
                }
            })
        });

        // Esc closes the popup
        ctx.input(|i| {
            if i.key_down(Key::Escape) {
                self.clicked_span = None;
            }
        })
    }

    fn draw_analyze_span_modal(&mut self, ctx: &egui::Context, max_width: f32, max_height: f32) {
        if !self.analyze_span_modal.show {
            return;
        }
        let modal = &mut self.analyze_span_modal;
        modal.show_modal(ctx, max_width, max_height);
    }

    fn draw_analyze_dependency_modal(
        &mut self,
        ctx: &egui::Context,
        max_width: f32,
        max_height: f32,
    ) {
        if self.analyze_dependency_modal.show {
            let modal = &mut self.analyze_dependency_modal;
            modal.show_modal(ctx, max_width, max_height);
            return;
        }
        // Modal is not set to be shown. Check if it was just closed by a selection.
        if self.analyze_dependency_modal.focus_node.is_none() {
            // If modal.show is false and focus_node is None, we do nothing further.
            return;
        }

        let focus_node_name = self.analyze_dependency_modal.focus_node.take().unwrap();

        // Clear previous highlights
        self.highlighted_spans.clear();

        // Get all dependency links and filter for those involving the focused node
        let links_to_highlight = match &self.analyze_dependency_modal.analysis_result {
            Some(analysis) => {
                let mut relevant_links = Vec::new();

                // Look through all nodes' dependency results
                for node_result in analysis.per_node_results.values() {
                    for link in &node_result.links {
                        // Check if this link involves the focused node (either as source or target)
                        let involves_focused_node = link
                            .source_spans
                            .iter()
                            .any(|s| s.node.name == focus_node_name)
                            || link
                                .target_spans
                                .iter()
                                .any(|s| s.node.name == focus_node_name);

                        if involves_focused_node {
                            relevant_links.push(link.clone());
                        }
                    }
                }

                println!(
                    "Found {} links involving focused node '{}'",
                    relevant_links.len(),
                    focus_node_name
                );
                relevant_links
            }
            None => {
                println!("No analysis result available!");
                return;
            }
        };

        self.highlight_spans_for_dependency_links(&links_to_highlight);
    }

    // Collect positions of spans in a node, including children
    fn collect_span_positions(
        &self,
        spans: &[Rc<Span>],
        start_height_param: f32,
        span_height: f32,
        positions: &mut HashMap<Vec<u8>, f32>,
    ) {
        for span in spans {
            let y_pos = start_height_param
                + span.parent_height_offset.get() as f32 * (span_height + self.layout.span_margin)
                + span_height / 2.0;

            // Only store position if this span's ID is in the highlighted set
            positions.insert(span.span_id.clone(), y_pos);

            let children = span.display_children.borrow();
            if !children.is_empty() {
                let this_span_visual_top_y = start_height_param
                    + span.parent_height_offset.get() as f32
                        * (span_height + self.layout.span_margin);
                let children_area_start_y =
                    this_span_visual_top_y + span_height + self.layout.span_margin;

                self.collect_span_positions(
                    &children,
                    children_area_start_y,
                    span_height,
                    positions,
                );
            }
        }
    }

    /// Draws arrows between related spans.
    fn draw_dependency_links(
        &mut self,
        ui: &mut Ui,
        span_positions: &HashMap<Vec<u8>, f32>,
        time_params: &TimeToScreenParams,
        ctx: &egui::Context,
    ) {
        if self.highlighted_spans.is_empty() {
            return;
        }
        let mut new_hovered_arrow_key = None;

        // Get all highlighted span IDs for efficient lookup
        let highlighted_span_ids_set: HashSet<Vec<u8>> = self
            .highlighted_spans
            .iter()
            .map(|s| s.span_id.clone())
            .collect();

        // Find the focused node from the highlighted spans
        let focused_node_names: HashSet<String> = self
            .highlighted_spans
            .iter()
            .map(|s| s.node.name.clone())
            .collect();

        // Look through all nodes that have dependency analysis results
        // and find links that involve both highlighted spans AND the focused node
        let analysis_result = match &self.analyze_dependency_modal.analysis_result {
            Some(result) => result,
            None => return,
        };

        let mut all_links_to_draw = Vec::new();
        for node_metrics in analysis_result.per_node_results.values() {
            for link in &node_metrics.links {
                // Check if this link involves any highlighted spans
                let link_involves_highlighted = link
                    .source_spans
                    .iter()
                    .any(|s| highlighted_span_ids_set.contains(&s.span_id))
                    || link
                        .target_spans
                        .iter()
                        .any(|s| highlighted_span_ids_set.contains(&s.span_id));

                // Also check if this link involves any of the focused nodes
                let link_involves_focused_node = link
                    .source_spans
                    .iter()
                    .any(|s| focused_node_names.contains(&s.node.name))
                    || link
                        .target_spans
                        .iter()
                        .any(|s| focused_node_names.contains(&s.node.name));

                if link_involves_highlighted && link_involves_focused_node {
                    all_links_to_draw.push(link);
                }
            }
        }

        if all_links_to_draw.is_empty() {
            return;
        }

        let arrow_color = colors::INTENSE_BLUE;
        let base_arrow_stroke = Stroke::new(2.0, arrow_color);

        for link in all_links_to_draw.iter() {
            if link.source_spans.is_empty() || link.target_spans.is_empty() {
                continue;
            }

            // Draw arrows from each source to each target
            for source_span in &link.source_spans {
                // Ensure source span y_center_on_screen is available
                let source_y_center_on_screen = match span_positions.get(&source_span.span_id) {
                    Some(&y_center) => y_center,
                    None => continue, // Source span not found, skip this source
                };

                // Calculate the x-coordinate for the arrow origin based on the source span's end time
                let source_x_for_arrow_origin = time_to_screen(
                    source_span.end_time,
                    time_params.visual_start_x,
                    time_params.visual_end_x,
                    time_params.selected_start_time,
                    time_params.selected_end_time,
                );
                // The arrow originates from the vertical center of the source span
                let from_pos = Pos2::new(source_x_for_arrow_origin, source_y_center_on_screen);

                for target_span in &link.target_spans {
                    // Ensure target span y_center_on_screen is available
                    let target_y_center_on_screen = match span_positions.get(&target_span.span_id) {
                        Some(&y_center) => y_center,
                        None => continue, // Target span not found in positions, skip this target
                    };

                    // Calculate the x-coordinate for the arrow tip based on the target span's start time
                    let target_x_for_arrow_tip = time_to_screen(
                        target_span.start_time,
                        time_params.visual_start_x,
                        time_params.visual_end_x,
                        time_params.selected_start_time,
                        time_params.selected_end_time,
                    );
                    // The arrow points to the vertical center of the target span
                    let to_pos = Pos2::new(target_x_for_arrow_tip, target_y_center_on_screen);

                    let distance_ms =
                        (target_span.start_time - source_span.end_time) * MILLISECONDS_PER_SECOND;

                    if distance_ms <= 0.0 {
                        // Ensure source ends before target starts
                        continue;
                    }

                    let arrow_key = ArrowKey {
                        source_span_id: source_span.span_id.clone(),
                        source_node_name: source_span.node.name.clone(),
                        target_span_id: target_span.span_id.clone(),
                        target_node_name: target_span.node.name.clone(),
                    };

                    // Draw the arrow based on the hover state from the PREVIOUS frame
                    // (or the most recent state if multiple repaints happened quickly)
                    let should_draw_highlighted =
                        self.hovered_arrow_key.as_ref() == Some(&arrow_key);

                    let arrow_interaction_result = draw_dependency_arrow(
                        ui,
                        from_pos,
                        to_pos,
                        base_arrow_stroke,
                        format!("{distance_ms:.2} ms"),
                        should_draw_highlighted,
                        &arrow_key,
                    );

                    if arrow_interaction_result.is_precisely_hovered {
                        // This arrow is being hovered in the current frame's input processing pass
                        new_hovered_arrow_key = Some(arrow_key.clone());
                    }

                    if arrow_interaction_result.response.clicked() {
                        self.clicked_arrow_info = Some(ArrowInfo {
                            source_span_name: source_span.name.clone(),
                            source_node_name: source_span.node.name.clone(),
                            source_start_time: source_span.start_time,
                            source_end_time: source_span.end_time,
                            target_span_name: target_span.name.clone(),
                            target_node_name: target_span.node.name.clone(),
                            target_start_time: target_span.start_time,
                            target_end_time: target_span.end_time,
                            duration: target_span.start_time - source_span.end_time,
                        });
                    }
                }
            }
        }

        // After checking all arrows, update the persistent hover state
        // and request repaint if it changed.
        if self.hovered_arrow_key != new_hovered_arrow_key {
            self.hovered_arrow_key = new_hovered_arrow_key;
            ctx.request_repaint();
        }
    }

    fn draw_relation_links(
        &mut self,
        span_positions: &HashMap<Vec<u8>, f32>,
        time_params: &TimeToScreenParams,
        ui: &mut Ui,
        _ctx: &egui::Context,
    ) {
        let arrow_color = colors::INTENSE_BLUE;
        let base_arrow_stroke = Stroke::new(2.0, arrow_color);

        for relation in &self.active_relations {
            let from_span = relation.from_span.upgrade().unwrap();
            let to_span = relation.to_span.upgrade().unwrap();

            let from_span_x_position = time_to_screen(
                from_span.end_time,
                time_params.visual_start_x,
                time_params.visual_end_x,
                time_params.selected_start_time,
                time_params.selected_end_time,
            );

            let from_span_y_position = match span_positions.get(&from_span.span_id) {
                Some(&pos) => pos,
                None => {
                    continue; // Skip if the span position is not found
                }
            };

            let to_span_x_position = time_to_screen(
                to_span.start_time,
                time_params.visual_start_x,
                time_params.visual_end_x,
                time_params.selected_start_time,
                time_params.selected_end_time,
            );

            let to_span_y_position = match span_positions.get(&to_span.span_id) {
                Some(&pos) => pos,
                None => continue, // Skip if the span position is not found
            };

            let distance_ms = (to_span.start_time - from_span.end_time) * MILLISECONDS_PER_SECOND;

            let arrow_key = ArrowKey {
                source_span_id: from_span.span_id.clone(),
                source_node_name: from_span.node.name.clone(),
                target_span_id: to_span.span_id.clone(),
                target_node_name: to_span.node.name.clone(),
            };

            let should_draw_highlighted = self.hovered_arrow_key.as_ref() == Some(&arrow_key);

            let from_pos = Pos2::new(from_span_x_position, from_span_y_position);
            let to_pos = Pos2::new(to_span_x_position, to_span_y_position);

            draw_dependency_arrow(
                ui,
                from_pos,
                to_pos,
                base_arrow_stroke,
                format!("{distance_ms:.2} ms"),
                should_draw_highlighted,
                &arrow_key,
            );
        }
    }

    fn draw_clicked_arrow_popup(&mut self, ctx: &egui::Context, max_width: f32, max_height: f32) {
        if self.clicked_arrow_info.is_none() {
            return;
        }

        let mut open = true;
        let info = self.clicked_arrow_info.as_ref().unwrap().clone();

        egui::Window::new("Dependency Link Information")
            .id(egui::Id::new("clicked_arrow_modal_window")) // Unique ID for the window
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(max_width * 0.8);
                ui.set_max_height(max_height * 0.6);

                ui.vertical_centered(|ui| {
                    ui.heading("Dependency Link Information"); // Already centered by vertical_centered
                });
                ui.add_space(5.0);
                ui.separator();
                ui.add_space(5.0);

                egui::Grid::new("arrow_info_grid")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Source Node:");
                        ui.label(&info.source_node_name);
                        ui.end_row();

                        ui.strong("Source Span:");
                        ui.label(&info.source_span_name);
                        ui.end_row();

                        ui.strong("Source Time:");
                        ui.label(format!(
                            "{} - {}",
                            time_point_to_utc_string(info.source_start_time),
                            time_point_to_utc_string(info.source_end_time)
                        ));
                        ui.end_row();

                        ui.separator();
                        ui.end_row();

                        ui.strong("Target Node:");
                        ui.label(&info.target_node_name);
                        ui.end_row();

                        ui.strong("Target Span:");
                        ui.label(&info.target_span_name);
                        ui.end_row();

                        ui.strong("Target Time:");
                        ui.label(format!(
                            "{} - {}",
                            time_point_to_utc_string(info.target_start_time),
                            time_point_to_utc_string(info.target_end_time)
                        ));
                        ui.end_row();

                        ui.separator();
                        ui.end_row();

                        ui.strong("Link Start Time:");
                        ui.label(time_point_to_utc_string(info.source_end_time));
                        ui.end_row();

                        ui.strong("Link End Time:");
                        ui.label(time_point_to_utc_string(info.target_start_time));
                        ui.end_row();

                        ui.strong("Link Duration:");
                        ui.label(format!("{:.3} ms", info.duration * MILLISECONDS_PER_SECOND));
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    if ui.button("Close").clicked() {
                        self.clicked_arrow_info = None;
                    }
                });
            });

        if !open {
            self.clicked_arrow_info = None;
        }

        // Esc closes the popup
        if ctx.input(|i| i.key_down(Key::Escape)) {
            self.clicked_arrow_info = None;
        }
    }

    fn load_peristent_data(&mut self) {
        if let Err(err) = persistent::load_persistent_data(
            &mut self.display_modes,
            &mut self.node_filters,
            &mut self.defined_relations,
            &mut self.relation_views,
        ) {
            eprintln!("Failed to load persistent data: {err}");
        }
    }

    fn save_persistent_data(&self) {
        if let Err(err) = persistent::save_persistent_data(
            &self.display_modes,
            &self.node_filters,
            &self.defined_relations,
            &self.relation_views,
        ) {
            eprintln!("Failed to save persistent data: {err}");
        }
    }

    fn highlight_spans_for_dependency_links(&mut self, links: &[DependencyLink]) {
        #[cfg(feature = "profiling")]
        let _timing_guard =
            profiling::GLOBAL_PROFILER.start_timing("highlight_spans_for_dependency_links");

        let mut spans_to_highlight = Vec::new();
        let mut unique_span_ids_to_highlight = HashSet::new();

        for (i, link) in links.iter().enumerate() {
            // Process source spans
            for (s_idx, source_s) in link.source_spans.iter().enumerate() {
                println!(
                    "[Link {}][Source {}/{}] Name: {} (node: {}, ID: {:?})",
                    i,
                    s_idx + 1,
                    link.source_spans.len(),
                    source_s.original_name,
                    source_s.node.name,
                    hex::encode(&source_s.span_id)
                );
                if unique_span_ids_to_highlight.insert(source_s.span_id.clone()) {
                    spans_to_highlight.push(source_s.clone());
                }
            }

            // Process target spans
            for (t_idx, target_s) in link.target_spans.iter().enumerate() {
                println!(
                    "[Link {}][Target {}/{}] Name: {} (node: {}, ID: {:?})",
                    i,
                    t_idx + 1,
                    link.target_spans.len(),
                    target_s.original_name,
                    target_s.node.name,
                    hex::encode(&target_s.span_id)
                );
                if unique_span_ids_to_highlight.insert(target_s.span_id.clone()) {
                    spans_to_highlight.push(target_s.clone());
                }
            }
        }

        // Assign the collected unique spans
        self.highlighted_spans = spans_to_highlight;

        // Adjust timeline to show these spans if needed
        if self.highlighted_spans.is_empty() {
            println!("No spans were highlighted!");
            return;
        }

        let mut min_time = f64::MAX;
        let mut max_time = f64::MIN;

        for span in &self.highlighted_spans {
            min_time = min_time.min(span.start_time);
            max_time = max_time.max(span.end_time);
        }

        // Limit the range to 3 seconds maximum
        const MAX_HIGHLIGHT_SPAN_TIMELINE_ZOOM: f64 = 3.0;
        let desired_max_time = min_time + MAX_HIGHLIGHT_SPAN_TIMELINE_ZOOM;
        if max_time > desired_max_time {
            max_time = desired_max_time;
        }

        // Add padding around the time range
        let padding = (max_time - min_time) * 0.2;
        min_time -= padding;
        max_time += padding;

        // Update timeline if needed
        if min_time < self.timeline.selected_start || max_time > self.timeline.selected_end {
            self.timeline.selected_start = min_time;
            self.timeline.selected_end = max_time;
            self.set_timeline_end_bars_to_selected();
        }
    }
}

/// Recursively populates the span ID to root span cache
fn populate_span_cache_recursive(
    current_span: &Rc<Span>,
    root_span: &Rc<Span>,
    cache: &mut HashMap<Vec<u8>, Rc<Span>>,
) {
    cache.insert(current_span.span_id.clone(), root_span.clone());

    for child in current_span.children.borrow().iter() {
        populate_span_cache_recursive(child, root_span, cache);
    }
}

fn get_min_max_time(spans: &[Rc<Span>]) -> Option<(TimePoint, TimePoint)> {
    let mut min_max: Option<(TimePoint, TimePoint)> = None;

    for span in spans {
        match &mut min_max {
            Some((min_time, max_time)) => {
                *min_time = min_time.min(span.min_start_time.get());
                *max_time = max_time.max(span.max_end_time.get());
            }
            None => {
                min_max = Some((span.min_start_time.get(), span.max_end_time.get()));
            }
        }
    }

    min_max
}

#[derive(Debug, Clone, Copy)]
struct SpanBoundingBox {
    start: f32,
    end: f32,
    height: HeightLevel,
}

fn arrange_spans_with_viewport(
    input_spans: &[Rc<Span>],
    first_invocation: bool,
    viewport_start: f64,
    viewport_end: f64,
) -> SpanBoundingBox {
    #[cfg(feature = "profiling")]
    let _timing_guard = profiling::GLOBAL_PROFILER.start_timing("arrange_spans");

    if input_spans.is_empty() {
        return SpanBoundingBox {
            start: 0.0,
            end: 0.0,
            height: 0,
        };
    }

    let mut sorted_spans = input_spans.to_vec();
    sorted_spans.sort_by(|a, b| {
        if let Some(start_ordering) = a.min_start_time.partial_cmp(&b.min_start_time) {
            return start_ordering;
        }
        a.max_end_time
            .partial_cmp(&b.max_end_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut span_bounding_boxes: Vec<SpanBoundingBox> = Vec::with_capacity(sorted_spans.len());

    // Spans that can collide with new spans (holds indexes to span_bounding_boxes).
    let mut active_spans: Vec<usize> = Vec::new();

    for (i, span) in sorted_spans.iter().enumerate() {
        let mut span_bbox = arrange_span_with_viewport(span, viewport_start, viewport_end);
        if first_invocation && span_bbox.height > 0 {
            span_bbox.height += 1; // Top-level spans have one unit of padding below them
        }

        // Default to height 0, will be updated below
        span.parent_height_offset.set(0);

        // Remove spans that for sure won't collide with this span or any future ones. Spans are
        // sorted by start time, so we can be sure that for futures ones the start time will be
        // larger than the end of the bounding box.
        active_spans.retain(|&j| span_bounding_boxes[j].end >= span_bbox.start);

        loop {
            let mut is_colliding = false;

            for &j in &active_spans {
                let other_span = &sorted_spans[j];
                let other_span_bbox = &span_bounding_boxes[j];

                if is_intersecting(
                    span_bbox.start,
                    span_bbox.end,
                    other_span_bbox.start,
                    other_span_bbox.end,
                ) && do_spans_collide_in_y(
                    span.parent_height_offset.get(),
                    span_bbox.height,
                    other_span.parent_height_offset.get(),
                    other_span_bbox.height,
                ) {
                    is_colliding = true;
                    break;
                }

                if span_bbox.start < other_span_bbox.end && span_bbox.end < other_span_bbox.start {
                    assert!(is_colliding);
                }
            }

            if is_colliding {
                span.parent_height_offset
                    .set(span.parent_height_offset.get() + 1);
            } else {
                break;
            }
        }

        span_bounding_boxes.push(span_bbox);
        active_spans.push(i);
    }

    let mut final_bbox = SpanBoundingBox {
        start: f32::INFINITY,
        end: f32::NEG_INFINITY,
        height: 0,
    };

    for i in 0..sorted_spans.len() {
        let span = &sorted_spans[i];
        let span_bbox = &span_bounding_boxes[i];

        final_bbox.start = final_bbox.start.min(span_bbox.start);
        final_bbox.end = final_bbox.end.max(span_bbox.end);
        final_bbox.height = final_bbox
            .height
            .max(span.parent_height_offset.get() + span_bbox.height);
    }

    final_bbox
}

fn arrange_span_with_viewport(
    span: &Rc<Span>,
    viewport_start: f64,
    viewport_end: f64,
) -> SpanBoundingBox {
    let span_start = span.display_start.get();
    let span_end = span_start + span.display_length.get();

    if span.display_children.borrow().is_empty() {
        SpanBoundingBox {
            start: span_start,
            end: span_end,
            height: 1,
        }
    } else {
        // Filter children to only include those that intersect with the viewport
        let all_children = span.display_children.borrow();
        let viewport_culled_children: Vec<Rc<Span>> = all_children
            .iter()
            .filter(|child| {
                is_intersecting(
                    child.min_start_time.get(),
                    child.max_end_time.get(),
                    viewport_start,
                    viewport_end,
                )
            })
            .cloned()
            .collect();

        let children_bbox = arrange_spans_with_viewport(
            &viewport_culled_children,
            false,
            viewport_start,
            viewport_end,
        );
        SpanBoundingBox {
            start: span_start.min(children_bbox.start),
            end: span_end.max(children_bbox.end),
            height: children_bbox.height + 1,
        }
    }
}

fn parse_trace_file(file_bytes: &[u8]) -> Result<Vec<ExportTraceServiceRequest>> {
    let t = TaskTimer::new("Parsing trace file");

    let file_str =
        std::str::from_utf8(file_bytes).map_err(|e| anyhow::anyhow!("File is not UTF8!: {}", e))?;
    let traces: Vec<ExportTraceServiceRequest> = serde_json::from_str(file_str)?;

    t.stop();
    Ok(traces)
}

fn set_min_max_time(spans: &[Rc<Span>]) {
    for span in spans {
        let mut min_start_time = span.start_time;
        let mut max_end_time = span.end_time;

        let children = span.children.borrow();
        set_min_max_time(children.as_slice());

        for child in children.iter() {
            min_start_time = min_start_time.min(child.min_start_time.get());
            max_end_time = max_end_time.max(child.max_end_time.get());
        }

        span.min_start_time.set(min_start_time);
        span.max_end_time.set(max_end_time);
    }
}

fn is_between<T: PartialOrd + Copy>(x: T, a: T, b: T) -> bool {
    a <= x && x <= b
}

fn is_intersecting<T: PartialOrd + Copy>(a: T, b: T, c: T, d: T) -> bool {
    is_between(a, c, d) || is_between(b, c, d) || is_between(c, a, b) || is_between(d, a, b)
}

fn do_spans_collide_in_y(y1: u64, height1: u64, y2: u64, height2: u64) -> bool {
    if !is_intersecting(y1, y1 + height1, y2, y2 + height2) {
        return false;
    }

    // Spans can touch vertically, that's ok
    if y1 + height1 == y2 || y2 + height2 == y1 {
        return false;
    }

    true
}

fn count_events(span: &Span) -> usize {
    let mut count = span.events.len();
    for child in span.children.borrow().iter() {
        count += count_events(child);
    }
    count
}

fn collect_events(span: &Span) -> Vec<Event> {
    let mut result = span.events.clone();
    for c in span.children.borrow().iter() {
        result.extend(collect_events(c).into_iter());
    }
    result
}

fn get_time_dots(start_time: TimePoint, end_time: TimePoint) -> Vec<TimePoint> {
    let mut delta = 10.0f64.powf((end_time - start_time).log10().ceil() + 10.0);
    let mut iterations: usize = 0;
    loop {
        let num_points = (end_time - start_time) / delta;
        if (5.0..100.0).contains(&num_points) {
            break;
        }
        delta /= 10.0;
        iterations += 1;
        if iterations > 10000 {
            println!("WARN: get_time_dots looped!: start_time: {start_time}, end_time: {end_time}");
            return vec![];
        }
    }
    let rounded_start = (start_time / delta).round() * delta;

    let mut dots = vec![];
    let mut cur_time = rounded_start;
    while cur_time < end_time {
        dots.push(cur_time);
        cur_time += delta;
    }
    dots
}

fn set_display_children_with_highlights(spans: &[Rc<Span>], highlighted_spans: &[Rc<Span>]) {
    #[cfg(feature = "profiling")]
    let _timing_guard =
        profiling::GLOBAL_PROFILER.start_timing("set_display_children_with_highlights");

    // First, set dont_collapse_this_span for all highlighted spans
    for span in highlighted_spans {
        span.dont_collapse_this_span.set(true);
    }

    // Now run the regular set_display_children
    for s in spans {
        set_display_children_rec(s, false, &mut vec![]);
    }

    // After layout is done, reset the flags to avoid affecting future layouts
    for span in highlighted_spans {
        span.dont_collapse_this_span.set(false);
    }
}

fn set_display_children_rec(
    span: &Rc<Span>,
    mut collapse_children_active: bool,
    cur_children: &mut Vec<Rc<Span>>,
) {
    if span.dont_collapse_this_span.get() {
        collapse_children_active = false;
    }

    let display_this_span = !collapse_children_active;

    if display_this_span {
        cur_children.push(span.clone());

        if span.collapse_children.get() {
            collapse_children_active = true;
        }

        span.display_children.borrow_mut().clear();
        for c in span.children.borrow().iter() {
            set_display_children_rec(
                c,
                collapse_children_active,
                &mut span.display_children.borrow_mut(),
            );
        }
    } else {
        for c in span.children.borrow().iter() {
            set_display_children_rec(c, collapse_children_active, cur_children);
        }
    }
}

fn collect_produce_block_starts_with_nodes(spans: &[Rc<Span>]) -> Vec<(TimePoint, String)> {
    let mut result = Vec::new();
    for span in spans {
        if span.name.starts_with("produce_block") {
            result.push((span.start_time, span.node.name.clone()));
        }
        let children = span.children.borrow();
        result.extend(collect_produce_block_starts_with_nodes(children.as_slice()));
    }
    result
}

fn draw_dependency_arrow(
    ui: &mut Ui,
    from: Pos2,
    to: Pos2,
    base_stroke: Stroke,
    label: String,
    is_hovered: bool,
    arrow_key: &ArrowKey,
) -> ArrowInteractionOutput {
    // Calculate the vector and its length
    let vec = to - from;
    let length = vec.length();

    // If the arrow is too short, allocate a minimal response for its space and don't draw anything
    if length < 0.001 {
        return ArrowInteractionOutput {
            response: ui.allocate_rect(Rect::from_min_max(from, to), Sense::hover()),
            is_precisely_hovered: false,
        };
    }

    // Normalize the vector for direction
    let normalized = vec.normalized();

    // Calculate the perpendicular (normal) vector for arrow heads and label placement
    let normal = Vec2::new(-normalized.y, normalized.x);

    // Draw the main line
    let line_stroke = if is_hovered {
        Stroke::new(base_stroke.width + 1.5, colors::VERY_LIGHT_BLUE2)
    } else {
        base_stroke
    };
    ui.painter().line_segment([from, to], line_stroke);

    // Arrow head size should be proportional to line length but capped
    let arrow_size = (length * 0.1).clamp(6.0, 12.0);
    let arrow_point = to - normalized * arrow_size;

    // Draw arrow head with two lines
    ui.painter()
        .line_segment([to, arrow_point + normal * arrow_size * 0.5], line_stroke);
    ui.painter()
        .line_segment([to, arrow_point - normal * arrow_size * 0.5], line_stroke);

    // Position the label at a fixed offset perpendicular to the line
    let label_offset = normal * 15.0;
    let label_pos = from + vec * 0.5 + label_offset;
    let font_id = FontId::proportional(12.0);
    let text_color = colors::GRAY_240;

    // Measure text for background
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(label.clone(), font_id.clone(), text_color));
    let padding = Vec2::new(6.0, 4.0);
    let text_rect = Rect::from_min_size(
        label_pos - Vec2::new(galley.rect.width() / 2.0, galley.rect.height() / 2.0),
        galley.rect.size() + padding,
    );

    // Draw text background for better visibility
    ui.painter()
        .rect_filled(text_rect, 4.0, colors::TRANSPARENT_GRAY);

    // Draw the text
    ui.painter()
        .text(label_pos, Align2::CENTER_CENTER, label, font_id, text_color);

    // Use a unique ID derived from the arrow_key to avoid conflicts
    let interact_id = ui.id().with(arrow_key);

    // Define the interactive area for the line itself
    let hover_padding = base_stroke.width + 4.0;
    let line_bounding_box = Rect::from_min_max(from.min(to), from.max(to));
    let hoverable_line_rect = line_bounding_box.expand(hover_padding);

    let interactive_rect = hoverable_line_rect;

    let egui_response = ui.interact(interactive_rect, interact_id, Sense::click());

    let mut is_precisely_hovered = false;
    if egui_response.hovered() {
        if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
            // Use the actual stroke width being drawn for precision check
            let current_stroke_width = if is_hovered {
                base_stroke.width + 1.5
            } else {
                base_stroke.width
            };
            // Check distance to the line segment (from -> to)
            // Add a small buffer to the stroke width for easier hovering
            let hover_threshold = current_stroke_width / 2.0 + 2.0;
            if distance_sq_to_segment(mouse_pos, from, to) < hover_threshold * hover_threshold {
                is_precisely_hovered = true;
            }
        }
    }

    ArrowInteractionOutput {
        response: egui_response,
        is_precisely_hovered,
    }
}

/// Describes the interaction result for a drawn arrow, including precise hover detection.
struct ArrowInteractionOutput {
    /// The raw Egui response from interacting with the arrow's bounding box.
    response: egui::Response,
    /// True if the mouse pointer is close to the actual arrow line segment.
    is_precisely_hovered: bool,
}

/// Calculates the square of the shortest distance from point `p` to the line segment `a`-`b`.
fn distance_sq_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let l2 = a.distance_sq(b); // Squared length of the segment
    if l2 == 0.0 {
        return p.distance_sq(a); // a and b are the same point
    }
    // Consider the line extending the segment, parameterized as a + t * (b - a).
    // Project point p onto this line.
    // t = [(p - a) . (b - a)] / |b - a|^2
    let t = ((p - a).dot(b - a)) / l2;
    if t < 0.0 {
        p.distance_sq(a) // Beyond the 'a' end of the segment
    } else if t > 1.0 {
        p.distance_sq(b) // Beyond the 'b' end of the segment
    } else {
        // Projection falls on the segment
        let projection = a + (b - a) * t;
        p.distance_sq(projection)
    }
}

#[test]
fn test_time_dots() {
    println!("{:?}", get_time_dots(0.001234, 0.00235));
}
