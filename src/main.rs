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
    ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2, Widget,
};
use eframe::epaint::PathShape;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use task_timer::TaskTimer;
use types::{
    time_point_to_utc_string, value_to_text, DisplayLength, Event, HeightLevel, Node, Span,
    TimePoint, MILLISECONDS_PER_SECOND,
};

mod analyze_dependency;
mod analyze_span;
mod analyze_utils;
mod modes;
mod task_timer;
mod types;

use analyze_dependency::AnalyzeDependencyModal;
use analyze_span::AnalyzeSpanModal;
use modes::{chain_mode, chain_shard0_mode, doomslug_mode, everything_mode};

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

struct App {
    layout: Layout,
    timeline: Timeline,
    raw_data: Vec<ExportTraceServiceRequest>,
    spans_to_display: Vec<Rc<Span>>,

    timeline_bar1_time: TimePoint,
    timeline_bar2_time: TimePoint,
    clicked_span: Option<Rc<Span>>,
    include_children_events: bool,
    display_mode: DisplayMode,
    search: Search,

    // Analyze 'features'
    all_spans_for_analysis: Vec<Rc<Span>>,
    analyze_span_modal: AnalyzeSpanModal,
    analyze_dependency_modal: AnalyzeDependencyModal,

    // Spans highlighting
    highlighted_spans: Vec<Rc<Span>>,
    dependency_focus_target_node: Option<String>,

    // Dependency arrow interactivity
    clicked_arrow_info: Option<ArrowInfo>,
    hovered_arrow_key: Option<ArrowKey>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum DisplayMode {
    Everything,
    Doomslug,
    Chain,
    ChainShard0,
}

impl DisplayMode {
    pub fn all_modes() -> &'static [DisplayMode] {
        &[
            DisplayMode::Everything,
            DisplayMode::Doomslug,
            DisplayMode::Chain,
            DisplayMode::ChainShard0,
        ]
    }
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
            display_mode: DisplayMode::Everything,
            search: Search::default(),
            all_spans_for_analysis: vec![],
            analyze_span_modal: AnalyzeSpanModal::default(),
            analyze_dependency_modal: AnalyzeDependencyModal::new(),
            highlighted_spans: Vec::new(),
            dependency_focus_target_node: None,
            clicked_arrow_info: None,
            hovered_arrow_key: None,
        };
        res.timeline.init(1.0, 3.0);
        res.set_timeline_end_bars_to_selected();
        res.search.search_term = "NOT IMPLEMENTED".to_string();
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
                    println!("Loading file: {:?}...", path);
                    match self.load_file(&path) {
                        Ok(()) => println!("Successfully loaded file."),
                        Err(e) => println!("Error loading file: {}", e),
                    }
                }
            }

            let display_mode_before = self.display_mode;
            ComboBox::new("mode chooser", "")
                .selected_text(format!("Display mode: {:?}", self.display_mode))
                .show_ui(ui, |ui| {
                    for mode in DisplayMode::all_modes() {
                        ui.selectable_value(&mut self.display_mode, *mode, format!("{:?}", mode));
                    }
                });
            if display_mode_before != self.display_mode {
                let res = match self.display_mode {
                    DisplayMode::Everything => self.apply_mode(everything_mode),
                    DisplayMode::Doomslug => self.apply_mode(doomslug_mode),
                    DisplayMode::Chain => self.apply_mode(chain_mode),
                    DisplayMode::ChainShard0 => self.apply_mode(chain_shard0_mode),
                };
                if let Err(e) = res {
                    println!("Error applying mode: {}", e);
                    self.display_mode = DisplayMode::Everything;
                }
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
                    let clear_button = ui.add_enabled(
                        has_highlights,
                        Button::new("Clear Highlights").fill(if has_highlights {
                            Color32::from_rgb(220, 230, 245)
                        } else {
                            Color32::from_rgb(180, 180, 180)
                        }),
                    );
                    if clear_button.clicked() {
                        println!(
                            "Clearing {} highlighted spans",
                            self.highlighted_spans.len()
                        );
                        self.highlighted_spans.clear();
                        self.analyze_dependency_modal.clear_focus();
                        self.dependency_focus_target_node = None;
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
        self.dependency_focus_target_node = None;
        self.analyze_span_modal.reset_processed_flag();
        self.analyze_dependency_modal.reset_processed_flag();

        // Extract all spans for analysis purposes immediately after loading raw_data
        self.all_spans_for_analysis = everything_mode(&self.raw_data)?;
        println!(
            "Stored {} parent spans from everything_mode for analysis after file load.",
            self.all_spans_for_analysis.len()
        );

        match self.display_mode {
            DisplayMode::Everything => {
                // Optimization: If current mode is Everything, reuse the already computed all_spans_for_analysis.
                self.spans_to_display = self.all_spans_for_analysis.clone();
                set_min_max_time(&self.spans_to_display);
            }
            DisplayMode::Doomslug => self.apply_mode(doomslug_mode)?,
            DisplayMode::Chain => self.apply_mode(chain_mode)?,
            DisplayMode::ChainShard0 => self.apply_mode(chain_shard0_mode)?,
        }

        let (min_time, max_time) = get_min_max_time(&self.spans_to_display).unwrap();
        self.timeline.init(min_time, max_time);
        self.set_timeline_end_bars_to_selected();

        Ok(())
    }

    fn apply_mode(
        &mut self,
        mode_fn: impl Fn(&[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>>,
    ) -> Result<()> {
        self.spans_to_display = mode_fn(&self.raw_data)?;
        set_min_max_time(&self.spans_to_display);
        //let (min_time, max_time) = get_min_max_time(&self.spans_to_display);
        Ok(())
    }

    // TODO - make this better. Time points should shift when the timeline is moved, not stay in place.
    // Would give better visual feedback. Something like `get_time_dots`.
    fn draw_timeline(&mut self, area: Rect, ui: &mut Ui, ctx: &egui::Context) {
        let background_button = ui.put(
            area,
            Button::new("")
                .fill(Color32::from_rgb(55, 127, 153))
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
                .fill(Color32::from_rgb(134, 202, 227)),
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
            Color32::from_rgb(50, 50, 50),
            ui,
            &self.spans_to_display,
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
        spans: &[Rc<Span>],
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
        let produce_block_starts = collect_produce_block_starts_with_nodes(spans);
        for (t, node_name) in produce_block_starts {
            if (t >= start_time) && (t <= end_time) {
                let x = time_to_screen(t, area.min.x, area.max.x, start_time, end_time);
                let marker_height = 20.0;
                ui.painter().line_segment(
                    [
                        Pos2::new(x, area.max.y),
                        Pos2::new(x, area.max.y - marker_height),
                    ],
                    Stroke::new(2.0, Color32::RED),
                );

                // Remove "neard:" prefix if present
                let short_node_name = node_name.strip_prefix("neard:").unwrap_or(&node_name);

                // Draw node name
                let small_font_id =
                    FontId::proportional(0.6 * egui::TextStyle::Body.resolve(ui.style()).size);
                ui.painter().text(
                    Pos2::new(x + 4.0, area.max.y - 10.0),
                    Align2::LEFT_TOP,
                    short_node_name,
                    small_font_id,
                    Color32::RED,
                );

                // Draw indicator for produce_block
                let label_rect = Rect::from_min_size(
                    Pos2::new(area.min.x - 15.0, area.max.y - 16.0),
                    Vec2::new(90.0, 18.0),
                );
                ui.painter().rect_filled(
                    label_rect,
                    3.0,
                    Color32::from_rgba_unmultiplied(60, 0, 0, 0),
                );
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
        ui.painter().rect_filled(area, 0.0, Color32::from_gray(10));

        let top_margin = 5;
        let ui_area = Rect::from_min_max(
            Pos2::new(area.min.x, area.min.y + top_margin as f32),
            area.max,
        );
        ui.allocate_new_ui(UiBuilder::new().max_rect(ui_area), |ui| {
            ui.horizontal(|ui| {
                TextEdit::singleline(&mut self.search.search_term)
                    .background_color(Color32::from_gray(40))
                    .ui(ui);
                ui.button("Search").clicked();
                ui.button("Next").clicked();
                ui.checkbox(&mut self.search.hide_non_matching, "Hide non-matching")
                    .clicked();
            });
        });
    }

    fn draw_spans(&mut self, area: Rect, ui: &mut Ui, ctx: &egui::Context) {
        let mut node_spans: BTreeMap<String, (Rc<Node>, Vec<Rc<Span>>)> = BTreeMap::new();
        for span in &self.spans_to_display {
            node_spans
                .entry(span.node.name.clone())
                .or_insert((span.node.clone(), vec![]))
                .1
                .push(span.clone());
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
            Color32::from_rgb(60, 60, 70),
        );
        self.draw_time_points(
            self.timeline.selected_start,
            self.timeline.selected_end,
            self.timeline.absolute_start,
            time_points_area,
            Color32::from_gray(240),
            ui,
            &self.spans_to_display,
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

        if self.spans_to_display.is_empty() {
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
                    ui.style_mut().visuals.override_text_color = Some(Color32::BLACK);

                    // TODO - a button for background feels hacky x.x
                    let background_button = ui.put(
                        under_time_points_area,
                        Button::new("")
                            .fill(Color32::from_rgb(30, 30, 30))
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
                        fs.layout_no_wrap("A".to_string(), FontId::default(), Color32::BLACK)
                            .rect
                            .height()
                    }) * 1.2;
                    self.layout.span_name_threshold = ui.fonts(|fs| {
                        fs.layout_no_wrap("...".to_string(), FontId::default(), Color32::BLACK)
                            .rect
                            .width()
                    });

                    let mut cur_height = under_time_points_area.min.y - visible_rect.min.y;

                    // Create a mapping from span_id to (node_name, y-position) for dependency arrows
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

                    for (node_name, (_node, spans)) in &node_spans {
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
                        Self::set_display_params_with_highlights(
                            &spans_in_range,
                            &highlighted_span_ids_set,
                            self.timeline.selected_start,
                            self.timeline.selected_end,
                            under_time_points_area.min.x + self.layout.node_name_width,
                            under_time_points_area.max.x,
                            ui,
                        );
                        let bbox = arrange_spans(&spans_in_range, true);

                        if !highlighted_span_ids_set.is_empty() {
                            self.collect_span_positions(
                                &spans_in_range,
                                cur_height,
                                span_height,
                                &mut span_positions,
                                &highlighted_span_ids_set,
                            );
                        }

                        ui.style_mut().visuals.override_text_color = Some(Color32::BLACK);
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
                        ui.style_mut().visuals.override_text_color = Some(Color32::WHITE);

                        let line_color = Color32::from_gray(230);
                        ui.put(
                            Rect::from_min_max(
                                Pos2::new(node_names_area.min.x, cur_height),
                                Pos2::new(node_names_area.max.x, next_height),
                            ),
                            Button::new(node_name)
                                .fill(Color32::from_rgb(10, 10, 20))
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
                        let time_params = TimeToScreenParams {
                            selected_start_time: self.timeline.selected_start,
                            selected_end_time: self.timeline.selected_end,
                            visual_start_x: under_time_points_area.min.x
                                + self.layout.node_name_width,
                            visual_end_x: under_time_points_area.max.x,
                        };
                        self.draw_dependency_links(ui, &span_positions, &time_params, ctx);
                    }

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
                        fs.layout_no_wrap(span.name.to_string(), FontId::default(), Color32::BLACK)
                            .rect
                            .width()
                    });
                    text_len.max(time_display_len)
                }
            };
            span.display_start.set(start_x);
            span.display_length.set(display_len);
            span.time_display_length.set(time_display_len);

            Self::set_display_params_with_highlights(
                span.display_children.borrow().as_slice(),
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
        for span in spans {
            let next_start_height = start_height
                + span.parent_height_offset.get() as f32 * (span_height + self.layout.span_margin);
            self.draw_arranged_span(
                span,
                ui,
                next_start_height,
                span_height,
                level,
                highlighted_span_ids,
            );
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
        let is_highlighted = highlighted_span_ids.contains(&span.span_id);

        // Only draw if this span is visible
        let visible_rect = ui.clip_rect();
        if start_height <= visible_rect.max.y && (start_height + span_height) >= visible_rect.min.y
        {
            let start_x = span.display_start.get();
            let end_x = start_x + span.display_length.get();

            let name = if end_x - start_x > self.layout.span_name_threshold {
                span.name.as_str()
            } else {
                ""
            };

            // Set colors based on whether it's a highlighted span
            let (time_color, base_color) = if is_highlighted {
                // Use blue color for highlighted spans
                (
                    Color32::from_rgb(50, 150, 220),
                    Color32::from_rgb(200, 220, 240),
                )
            } else {
                // Use yellow/gold colors for normal spans
                (
                    Color32::from_rgb(242, 176, 34),
                    Color32::from_rgb(242, 242, 217),
                )
            };

            let time_rect = Rect::from_min_max(
                Pos2::new(start_x, start_height),
                Pos2::new(
                    start_x + span.time_display_length.get(),
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
                let border_stroke = Stroke::new(2.5, Color32::from_rgb(0, 110, 230));
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
                    Stroke::new(2.0, Color32::from_rgb(255, 51, 0)),
                );
            }

            let span_button = ui.put(
                display_rect,
                Button::new(name)
                    .truncate()
                    .fill(Color32::from_rgba_unmultiplied(242, 176, 34, 1)),
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

        // Always recurse into children
        self.draw_arranged_spans(
            span.display_children.borrow().as_slice(),
            ui,
            start_height + span_height + self.layout.span_margin,
            span_height,
            level + 1,
            highlighted_span_ids,
        );
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

        // Get focus_node_name by taking it from the modal and store it in App
        let focus_node_name = self.analyze_dependency_modal.focus_node.take().unwrap();
        self.dependency_focus_target_node = Some(focus_node_name.clone());

        // Clear previous highlights
        self.highlighted_spans.clear();

        // Get the analysis result
        let analysis = match &self.analyze_dependency_modal.analysis_result {
            Some(res) => res,
            None => {
                println!("No analysis result available!");
                return;
            }
        };

        let node_result = match analysis.per_node_results.get(&focus_node_name) {
            Some(res) => res,
            None => {
                println!("Node result not found for: {}", focus_node_name);
                return;
            }
        };

        println!("Found node result with {} links", node_result.links.len());

        let mut spans_to_highlight = Vec::new();
        let mut unique_span_ids_to_highlight: HashSet<Vec<u8>> = HashSet::new();

        // Iterate through links and directly collect unique spans
        for (i, link) in node_result.links.iter().enumerate() {
            // Process source spans
            for (s_idx, source_s) in link.source_spans.iter().enumerate() {
                println!(
                    "[Link {}][Source {}/{}] Name: {} (node: {}, ID: {:?})",
                    i,
                    s_idx + 1,
                    link.source_spans.len(),
                    source_s.name,
                    source_s.node.name,
                    hex::encode(&source_s.span_id)
                );
                if unique_span_ids_to_highlight.insert(source_s.span_id.clone()) {
                    spans_to_highlight.push(source_s.clone());
                }
            }

            // Process target span
            let target_s = &link.target_span;
            println!(
                "[Link {}][Target] Name: {} (node: {}, ID: {:?})",
                i,
                target_s.name,
                target_s.node.name,
                hex::encode(&target_s.span_id)
            );
            if unique_span_ids_to_highlight.insert(target_s.span_id.clone()) {
                spans_to_highlight.push(target_s.clone());
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

        // Limit the range to 4 seconds maximum
        let desired_max_time = min_time + 4.0;
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

    // Collect positions of spans in a node, including children
    fn collect_span_positions(
        &self,
        spans: &[Rc<Span>],
        start_height_param: f32,
        span_height: f32,
        positions: &mut HashMap<Vec<u8>, f32>,
        highlighted_ids_to_store: &HashSet<Vec<u8>>,
    ) {
        for span in spans {
            let y_pos = start_height_param
                + span.parent_height_offset.get() as f32 * (span_height + self.layout.span_margin)
                + span_height / 2.0;

            // Only store position if this span's ID is in the highlighted set
            if highlighted_ids_to_store.contains(&span.span_id) {
                positions.insert(span.span_id.clone(), y_pos);
            }

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
                    highlighted_ids_to_store,
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

        let analysis = match &self.analyze_dependency_modal.analysis_result {
            Some(res) => res,
            None => return,
        };
        let focused_target_node_name = match self.dependency_focus_target_node.as_ref() {
            Some(name) => name,
            None => return,
        };
        let node_result = match analysis.per_node_results.get(focused_target_node_name) {
            Some(res) => res,
            None => return,
        };

        let arrow_color = Color32::from_rgb(50, 150, 220);
        let base_arrow_stroke = Stroke::new(2.0, arrow_color);

        for link in node_result.links.iter() {
            if link.source_spans.is_empty() {
                continue;
            }

            let target_span = &link.target_span;

            // Ensure target span y_center_on_screen is available
            let target_y_center_on_screen = match span_positions.get(&target_span.span_id) {
                Some(&y_center) => y_center,
                None => continue, // Target span not found in positions, skip this link
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

                let distance_ms =
                    (target_span.start_time - source_span.end_time) * MILLISECONDS_PER_SECOND;

                if distance_ms < 0.0 {
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
                let should_draw_highlighted = self.hovered_arrow_key.as_ref() == Some(&arrow_key);

                let arrow_interaction_result = draw_arrow(
                    ui,
                    from_pos,
                    to_pos,
                    base_arrow_stroke,
                    format!("{:.2} ms", distance_ms),
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

        // After checking all arrows, update the persistent hover state
        // and request repaint if it changed.
        if self.hovered_arrow_key != new_hovered_arrow_key {
            self.hovered_arrow_key = new_hovered_arrow_key;
            ctx.request_repaint();
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

/// Sets relative_display_pos for all spans and their children
fn arrange_spans(input_spans: &[Rc<Span>], first_invocation: bool) -> SpanBoundingBox {
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
        let mut span_bbox = arrange_span(span);
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

fn arrange_span(span: &Rc<Span>) -> SpanBoundingBox {
    let span_start = span.display_start.get();
    let span_end = span_start + span.display_length.get();

    if span.display_children.borrow().is_empty() {
        SpanBoundingBox {
            start: span_start,
            end: span_end,
            height: 1,
        }
    } else {
        let children_bbox = arrange_spans(span.display_children.borrow().as_slice(), false);
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
            println!(
                "WARN: get_time_dots looped!: start_time: {}, end_time: {}",
                start_time, end_time
            );
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

fn draw_arrow(
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
        Stroke::new(base_stroke.width + 1.5, Color32::from_rgb(220, 240, 255))
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
    let text_color = Color32::from_rgb(240, 240, 240);

    // Measure text for background
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(label.clone(), font_id.clone(), text_color));
    let padding = Vec2::new(6.0, 4.0);
    let text_rect = Rect::from_min_size(
        label_pos - Vec2::new(galley.rect.width() / 2.0, galley.rect.height() / 2.0),
        galley.rect.size() + padding,
    );

    // Draw text background for better visibility
    ui.painter().rect_filled(
        text_rect,
        4.0,
        Color32::from_rgba_premultiplied(30, 30, 30, 230),
    );

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
