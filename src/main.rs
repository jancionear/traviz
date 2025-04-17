use core::f32;
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Result;
use eframe::egui::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, Align2, Button, Color32, ComboBox, FontId, Key, Label, Modal, PointerButton, Pos2, Rect,
    ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder, Vec2, Widget,
};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use task_timer::TaskTimer;
use types::{
    time_point_to_utc_string, value_to_text, DisplayLength, Event, HeightLevel, Node, Span,
    TimePoint,
};

mod modes;
mod task_timer;
mod types;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
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
        self.visible_end = (self.visible_start + 1.0).min(max_time);

        self.selected_start = min_time;
        self.selected_end = (self.selected_start + 0.1).min(max_time);
    }
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
}

impl DisplayMode {
    pub fn all_modes() -> &'static [DisplayMode] {
        &[
            DisplayMode::Everything,
            DisplayMode::Doomslug,
            DisplayMode::Chain,
        ]
    }
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
                spans_time_points_height: 50.0,
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
                self.draw_spans(spans_area, ui);

                self.draw_clicked_span(ctx, window_width - 100.0, window_height - 100.0);

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
                    DisplayMode::Everything => self.apply_mode(modes::everything_mode),
                    DisplayMode::Doomslug => self.apply_mode(modes::doomslug_mode),
                    DisplayMode::Chain => self.apply_mode(modes::chain_mode),
                };
                if let Err(e) = res {
                    println!("Error applying mode: {}", e);
                    self.display_mode = DisplayMode::Everything;
                }
            }
        });
    }

    fn load_file(&mut self, path: &PathBuf) -> Result<()> {
        // Read json file
        let mut file_bytes = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut file_bytes)?;

        self.raw_data = parse_trace_file(&file_bytes)?;

        self.apply_mode(modes::everything_mode)?;

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

    fn draw_spans(&mut self, area: Rect, ui: &mut Ui) {
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

                    for (node_name, (_node, spans)) in node_spans {
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

                        set_display_children(&spans_in_range);
                        Self::set_display_params(
                            &spans_in_range,
                            self.timeline.selected_start,
                            self.timeline.selected_end,
                            under_time_points_area.min.x + self.layout.node_name_width,
                            under_time_points_area.max.x,
                            ui,
                        );
                        let t = TaskTimer::new("arrange_spans");
                        let bbox = arrange_spans(&spans_in_range, true);
                        t.stop();
                        ui.style_mut().visuals.override_text_color = Some(Color32::BLACK);
                        let t = TaskTimer::new("draw_arranged_spans");
                        self.draw_arranged_spans(&spans_in_range, ui, cur_height, span_height, 0);
                        t.stop();

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

    fn set_display_params(
        spans: &[Rc<Span>],
        start_time: TimePoint,
        end_time: TimePoint,
        start_pos: f32,
        end_pos: f32,
        ui: &Ui,
    ) {
        for span in spans {
            let start_x = time_to_screen(span.start_time, start_pos, end_pos, start_time, end_time);
            let time_display_len =
                time_to_screen(span.end_time, start_pos, end_pos, start_time, end_time) - start_x;

            let display_len = match span.display_options.display_length {
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

            Self::set_display_params(
                span.display_children.borrow().as_slice(),
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
    ) {
        for span in spans {
            let y = start_height
                + span.parent_height_offset.get() as f32 * (span_height + self.layout.span_margin);

            // Always recurse into children, regardless of parent's visibility
            self.draw_arranged_span(span, ui, y, span_height, level);
        }
    }

    fn draw_arranged_span(
        &mut self,
        span: &Rc<Span>,
        ui: &mut Ui,
        start_height: f32,
        span_height: f32,
        level: u64,
    ) {
        let visible_rect = ui.clip_rect();
        // Only draw if this span is visible
        if start_height <= visible_rect.max.y && (start_height + span_height) >= visible_rect.min.y
        {
            let start_x = span.display_start.get();
            let end_x = start_x + span.display_length.get();

            let name = if end_x - start_x > self.layout.span_name_threshold {
                span.name.as_str()
            } else {
                ""
            };

            let time_color = Color32::from_rgb(242, 176, 34);
            let base_color = Color32::from_rgb(242, 242, 217);
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

            if level == 0 {
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
                    (span.end_time - span.start_time) * 1000.0
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
                    (span.end_time - span.start_time) * 1000.0
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
        return SpanBoundingBox::default();
    }

    // Sort spans by start time
    let mut sorted_spans = Vec::with_capacity(input_spans.len());
    sorted_spans.extend_from_slice(input_spans);
    sorted_spans.sort_by(|a, b| {
        a.min_start_time
            .get()
            .partial_cmp(&b.min_start_time.get())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut span_bounding_boxes: Vec<SpanBoundingBox> = Vec::with_capacity(sorted_spans.len());
    let mut final_bbox = SpanBoundingBox {
        start: f32::INFINITY,
        end: f32::NEG_INFINITY,
        height: 0,
    };

    // Each entry is the index of a span in sorted_spans/span_bounding_boxes
    let mut active_spans: Vec<usize> = Vec::with_capacity(sorted_spans.len());

    for (i, span) in sorted_spans.iter().enumerate() {
        let mut span_bbox = arrange_span(span);
        if first_invocation && span_bbox.height > 0 {
            span_bbox.height += 1;
        }

        // Default to height 0, will be updated below
        span.parent_height_offset.set(0);

        // Remove spans that end before the current span starts
        active_spans.retain(|&j| {
            let other_bbox = &span_bounding_boxes[j];
            other_bbox.end >= span_bbox.start
        });

        // Find the first non-colliding height
        let mut height = 0;
        'height_search: loop {
            span.parent_height_offset.set(height);

            for &j in &active_spans {
                let other_span = &sorted_spans[j];
                let other_bbox: &SpanBoundingBox = &span_bounding_boxes[j];

                if do_spans_collide_in_y(
                    height,
                    span_bbox.height,
                    other_span.parent_height_offset.get(),
                    other_bbox.height,
                ) {
                    height += 1;
                    continue 'height_search;
                }
            }
            break;
        }

        // Update the bounding box for this span
        final_bbox.start = final_bbox.start.min(span_bbox.start);
        final_bbox.end = final_bbox.end.max(span_bbox.end);
        final_bbox.height = final_bbox.height.max(height + span_bbox.height);

        span_bounding_boxes.push(span_bbox);
        active_spans.push(i);
    }

    final_bbox
}

impl Default for SpanBoundingBox {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 0.0,
            height: 0,
        }
    }
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

        for child in span.children.borrow().iter() {
            set_min_max_time(&[child.clone()]);

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

fn set_display_children(spans: &[Rc<Span>]) {
    for s in spans {
        set_display_children_rec(s, false, &mut vec![]);
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

#[test]
fn test_time_dots() {
    println!("{:?}", get_time_dots(0.001234, 0.00235));
}
