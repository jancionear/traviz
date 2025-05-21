use crate::analyze_utils;
use crate::types::{NodeIdentifier, Span, MILLISECONDS_PER_SECOND};
use eframe::egui::{
    Align, Align2, Button, Color32, Context, Grid, Id, Key, Label, Layout, Modal, Order, RichText,
    ScrollArea, Sense, Ui, Vec2, Window,
};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub struct AnalyzeSpanModal {
    /// Whether the modal window is currently visible.
    pub show: bool,
    /// Text entered by the user in the span name search box.
    pub search_text: String,
    /// The name of the span currently selected by the user in the list.
    pub selected_span_name: Option<String>,
    /// A sorted list of unique span names found in the current trace data.
    pub unique_span_names: Vec<String>,
    /// Flag indicating if the span list for the modal has been processed from the current trace data.
    pub spans_processed: bool,
    /// All unique spans (including children) collected from the current trace, used for analysis.
    all_spans_for_analysis: Vec<Rc<Span>>,
    /// Stores the specific span whose details are to be shown in a separate popup, if any.
    span_details: Option<Rc<Span>>,
    /// An optional message summarizing the outcome of the last analysis (e.g., errors or warnings).
    pub analysis_summary_message: Option<String>,
    /// Stores the detailed results of the last span analysis performed.
    detailed_span_analysis: Option<SpanAnalysisResult>,
}

/// Struct to hold duration statistics for spans.
/// Also stores references to the spans with the min duration and max duration.
struct SpanStatistics {
    duration_stats: analyze_utils::Statistics,

    min_span: Option<Rc<Span>>,
    max_span: Option<Rc<Span>>,
}

impl SpanStatistics {
    fn new() -> Self {
        Self {
            duration_stats: analyze_utils::Statistics::new(),
            min_span: None,
            max_span: None,
        }
    }

    fn add_span(&mut self, span: &Rc<Span>) {
        let duration = span.end_time - span.start_time;
        self.duration_stats.add_value(duration);

        // Update max duration and store the span if it's the new max
        if self
            .max_span
            .as_ref()
            .map_or(true, |s| duration > (s.end_time - s.start_time))
        {
            self.max_span = Some(span.clone());
        }

        // Update min duration and store the span if it's the new min
        if self
            .min_span
            .as_ref()
            .map_or(true, |s| duration < (s.end_time - s.start_time))
        {
            self.min_span = Some(span.clone());
        }
    }

    // Get the stored min span if available
    fn get_min_span(&self) -> Option<Rc<Span>> {
        self.min_span.clone()
    }

    // Get the stored max span if available
    fn get_max_span(&self) -> Option<Rc<Span>> {
        self.max_span.clone()
    }
}

/// Struct to hold analysis results for all nodes.
struct SpanAnalysisResult {
    span_name: String,
    per_node_stats: HashMap<String, SpanStatistics>,
    overall_stats: SpanStatistics,
}

/// Enum to specify the type of statistic (Min or Max).
#[derive(Clone, Copy, Debug)]
enum StatType {
    Min,
    Max,
}

/// Struct to group parameters for drawing a clickable stat cell.
struct ClickableStatCellDrawParams<'a> {
    ui: &'a mut Ui,
    width: f32,
    value_str: &'a str,
    is_strong: bool,
    node_identifier: NodeIdentifier,
    stat_type: StatType,
}

// Helper function to draw a left-aligned node name cell.
fn draw_node_name_cell(ui: &mut Ui, width: f32, text: &str, is_strong: bool) {
    ui.scope(|cell_ui| {
        cell_ui.set_min_width(width);
        let rich_text = RichText::new(text).monospace();
        if is_strong {
            cell_ui.strong(rich_text);
        } else {
            cell_ui.label(rich_text);
        }
    });
}

// Helper function to draw a right-aligned statistics cell (non-clickable).
fn draw_right_stat_cell(ui: &mut Ui, width: f32, value_str: &str, is_strong: bool) {
    ui.scope(|cell_ui| {
        cell_ui.set_min_width(width);
        cell_ui.with_layout(Layout::right_to_left(Align::Center), |inner_ui| {
            let rich_text = RichText::new(value_str).monospace();
            if is_strong {
                inner_ui.strong(rich_text);
            } else {
                inner_ui.label(rich_text);
            }
        });
    });
}

impl AnalyzeSpanModal {
    /// Helper function to calculate column widths based on percentages.
    fn calculate_column_widths(grid_width: f32, col_percentages: &[f32; 6]) -> [f32; 6] {
        [
            (grid_width * col_percentages[0]).max(140.0), // Node
            (grid_width * col_percentages[1]).max(60.0),  // Count
            (grid_width * col_percentages[2]).max(80.0),  // Min
            (grid_width * col_percentages[3]).max(80.0),  // Max
            (grid_width * col_percentages[4]).max(80.0),  // Mean
            (grid_width * col_percentages[5]).max(80.0),  // Median
        ]
    }

    /// Helper method to draw a right-aligned, clickable statistics cell (for Min/Max).
    fn draw_clickable_stat_cell(
        &self,
        params: ClickableStatCellDrawParams,
        span_to_view: &mut Option<Rc<Span>>,
    ) {
        params.ui.scope(|cell_ui| {
            cell_ui.set_min_width(params.width);
            cell_ui.with_layout(Layout::right_to_left(Align::Center), |inner_ui| {
                let mut rich_text = RichText::new(params.value_str)
                    .monospace()
                    .color(Color32::from_rgb(50, 150, 200));
                if params.is_strong {
                    rich_text = rich_text.strong();
                }
                let response = inner_ui.add(Label::new(rich_text).sense(Sense::click()));

                if response.clicked() {
                    let found_span = match params.stat_type {
                        StatType::Min => self.find_min_span_for_node(&params.node_identifier),
                        StatType::Max => self.find_max_span_for_node(&params.node_identifier),
                    };
                    if let Some(s) = found_span {
                        *span_to_view = Some(s);
                    }
                }
                let hover_text = match params.stat_type {
                    StatType::Min => "Click to see details of span with minimum duration",
                    StatType::Max => "Click to see details of span with maximum duration",
                };
                if response.hovered() {
                    response.on_hover_text(hover_text);
                }
            });
        });
    }

    pub fn update_span_list(&mut self, spans: &[Rc<Span>]) {
        let (all_spans, unique_names) = analyze_utils::process_spans_for_analysis(spans);
        self.all_spans_for_analysis = all_spans;
        self.unique_span_names = unique_names;
    }

    fn perform_span_analysis(&mut self, target_span_name: &str) {
        let mut matching_spans = Vec::new();
        let target_name = target_span_name.to_string();

        // Collect matching spans
        analyze_utils::collect_matching_spans(
            &self.all_spans_for_analysis,
            &target_name,
            &mut matching_spans,
        );

        if matching_spans.is_empty() {
            self.analysis_summary_message =
                Some(format!("No spans found with name '{}'", target_name));
            // Clear previous results
            self.detailed_span_analysis = None;
            return;
        }

        // Group spans by node
        let mut per_node_stats: HashMap<String, SpanStatistics> = HashMap::new();
        let mut overall_stats = SpanStatistics::new();

        for span in matching_spans {
            overall_stats.add_span(&span);
            let node_name = span.node.name.clone();
            per_node_stats
                .entry(node_name)
                .or_insert_with(SpanStatistics::new)
                .add_span(&span);
        }

        // Store analysis results
        self.detailed_span_analysis = Some(SpanAnalysisResult {
            span_name: target_name,
            per_node_stats,
            overall_stats,
        });
        self.analysis_summary_message = None;
    }

    /// Returns the span with the minimum duration.
    fn find_min_span_for_node(&self, node_identifier: &NodeIdentifier) -> Option<Rc<Span>> {
        self.detailed_span_analysis
            .as_ref()
            .and_then(|result| match node_identifier {
                NodeIdentifier::AllNodes => result.overall_stats.get_min_span(),
                NodeIdentifier::Node(node_name) => result
                    .per_node_stats
                    .get(node_name)
                    .and_then(|stats| stats.get_min_span()),
            })
    }

    /// Returns the span with the maximum duration.
    fn find_max_span_for_node(&self, node_identifier: &NodeIdentifier) -> Option<Rc<Span>> {
        self.detailed_span_analysis
            .as_ref()
            .and_then(|result| match node_identifier {
                NodeIdentifier::AllNodes => result.overall_stats.get_max_span(),
                NodeIdentifier::Node(node_name) => result
                    .per_node_stats
                    .get(node_name)
                    .and_then(|stats| stats.get_max_span()),
            })
    }

    pub fn show_modal(
        &mut self,
        ctx: &Context,
        spans: &[Rc<Span>],
        max_width: f32,
        max_height: f32,
    ) {
        if !self.show {
            return;
        }

        // Only update the span list if we have new spans and they're not empty
        if !self.spans_processed && !spans.is_empty() {
            self.update_span_list(spans);
            self.spans_processed = true;
        }

        // Track if we need to view a span's details after modal closes
        let mut span_to_view: Option<Rc<Span>> = None;

        let mut modal_closed = false;

        Modal::new("analyze span".into()).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(max_width);
                ui.set_max_height(max_height);

                ui.heading("Analyze Span");

                // Top row with search and analyze button
                ui.horizontal(|ui| {
                    // Search field (left side, takes 70% of width)
                    ui.with_layout(Layout::left_to_right(eframe::emath::Align::Center), |ui| {
                        ui.set_max_width(max_width * 0.7);
                        analyze_utils::span_search_ui(
                            ui,
                            &mut self.search_text,
                            "Search span by name:",
                            "Type to search",
                            max_width * 0.65,
                        );
                    });

                    // Analyze button (right side)
                    ui.with_layout(Layout::right_to_left(eframe::emath::Align::Center), |ui| {
                        let is_span_selected = self.selected_span_name.is_some();
                        if ui
                            .add_enabled(
                                is_span_selected,
                                Button::new("Analyze").min_size(Vec2::new(120.0, 40.0)),
                            )
                            .clicked()
                        {
                            if let Some(span_name_ref) = &self.selected_span_name {
                                self.perform_span_analysis(&span_name_ref.clone());
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // Split the remaining space - List takes 30% of height
                let list_height = (max_height - 120.0) * 0.3;
                let results_height = (max_height - 120.0) * 0.7 - 20.0;

                // Show list of span names in a scrollable area
                let selection_changed = analyze_utils::span_selection_list_ui(
                    ui,
                    &self.unique_span_names,
                    &self.search_text,
                    &mut self.selected_span_name,
                    list_height,
                    "analyze_span_list",
                );

                if selection_changed {
                    self.detailed_span_analysis = None;
                    self.analysis_summary_message = None;
                }

                ui.separator();

                // Results area
                ui.label("Analysis Results:");

                if let Some(result) = &self.detailed_span_analysis {
                    ui.label(format!("Analysis of span: '{}'", result.span_name));
                }
                if let Some(message) = &self.analysis_summary_message {
                    ui.colored_label(Color32::YELLOW, message);
                }

                // Create the grid headers first outside the scroll area (to keep them visible)
                if self.detailed_span_analysis.is_some() {
                    ui.add_space(10.0);

                    let available_width = ui.available_width();

                    // Define percentage-based column widths that sum exactly to 100%
                    // This ensures the full width is used in both header and data grid
                    let col_percentages = [0.25, 0.1, 0.15, 0.2, 0.15, 0.15];

                    let grid_width = available_width;

                    // Calculate pixel widths for columns based on percentages of the full width
                    let col_widths = Self::calculate_column_widths(grid_width, &col_percentages);

                    // Header row - outside scrollable area to make it sticky
                    Grid::new("span_analysis_header_grid")
                        .num_columns(6)
                        .spacing([10.0, 6.0])
                        .striped(true)
                        .min_col_width(0.0)
                        .show(ui, |ui_grid| {
                            // First cell (Node) is left-aligned
                            draw_node_name_cell(ui_grid, col_widths[0], "Node", true);

                            // Subsequent cells are right-aligned
                            draw_right_stat_cell(ui_grid, col_widths[1], "Count", true);
                            draw_right_stat_cell(ui_grid, col_widths[2], "Min (ms)", true);
                            draw_right_stat_cell(ui_grid, col_widths[3], "Max (ms)", true);
                            draw_right_stat_cell(ui_grid, col_widths[4], "Mean (ms)", true);
                            draw_right_stat_cell(ui_grid, col_widths[5], "Median (ms)", true);

                            ui_grid.end_row();
                        });

                    ui.separator();

                    // Store the grid width and column percentages for the data grid
                    ui.memory_mut(|mem| {
                        mem.data
                            .insert_temp(Id::new("analyze_grid_width"), grid_width);
                        mem.data
                            .insert_temp(Id::new("analyze_col_percentages"), col_percentages);
                    });
                }

                // Grid contents in a scrollable area
                ScrollArea::vertical()
                    .max_height(results_height)
                    .id_salt("analysis_results_scroll_area")
                    .show_viewport(ui, |ui, _viewport| {
                        if let Some(result) = &self.detailed_span_analysis {
                            // Retrieve the stored grid width and column percentages
                            let (grid_width, col_percentages) = ui.memory(|mem| {
                                let width = mem
                                    .data
                                    .get_temp::<f32>(Id::new("analyze_grid_width"))
                                    .unwrap_or_else(|| ui.available_width());
                                let percentages = mem
                                    .data
                                    .get_temp::<[f32; 6]>(Id::new("analyze_col_percentages"))
                                    .unwrap_or([0.25, 0.1, 0.15, 0.2, 0.15, 0.15]);
                                (width, percentages)
                            });

                            // Calculate column widths using the same grid width and percentages
                            let col_widths =
                                Self::calculate_column_widths(grid_width, &col_percentages);

                            // Use Grid for tabular data (without headers)
                            Grid::new("span_analysis_grid")
                                .num_columns(6)
                                .spacing([10.0, 6.0])
                                .striped(true)
                                .min_col_width(0.0)
                                .show(ui, |ui_grid| {
                                    // Get nodes and sort them alphabetically
                                    let mut node_names: Vec<String> =
                                        result.per_node_stats.keys().cloned().collect();
                                    node_names.sort();

                                    // Rows for each node
                                    for node_name in node_names {
                                        if let Some(stats) = result.per_node_stats.get(&node_name) {
                                            draw_node_name_cell(
                                                ui_grid,
                                                col_widths[0],
                                                &node_name,
                                                false,
                                            );
                                            draw_right_stat_cell(
                                                ui_grid,
                                                col_widths[1],
                                                &format!("{}", stats.duration_stats.count),
                                                false,
                                            );

                                            let min_text = format!(
                                                "{:.3}",
                                                stats.duration_stats.min * MILLISECONDS_PER_SECOND
                                            );
                                            self.draw_clickable_stat_cell(
                                                ClickableStatCellDrawParams {
                                                    ui: ui_grid,
                                                    width: col_widths[2],
                                                    value_str: &min_text,
                                                    is_strong: false,
                                                    node_identifier: NodeIdentifier::Node(
                                                        node_name.clone(),
                                                    ),
                                                    stat_type: StatType::Min,
                                                },
                                                &mut span_to_view,
                                            );

                                            let max_text = format!(
                                                "{:.3}",
                                                stats.duration_stats.max * MILLISECONDS_PER_SECOND
                                            );
                                            self.draw_clickable_stat_cell(
                                                ClickableStatCellDrawParams {
                                                    ui: ui_grid,
                                                    width: col_widths[3],
                                                    value_str: &max_text,
                                                    is_strong: false,
                                                    node_identifier: NodeIdentifier::Node(
                                                        node_name.clone(),
                                                    ),
                                                    stat_type: StatType::Max,
                                                },
                                                &mut span_to_view,
                                            );

                                            draw_right_stat_cell(
                                                ui_grid,
                                                col_widths[4],
                                                &format!(
                                                    "{:.3}",
                                                    stats.duration_stats.mean()
                                                        * MILLISECONDS_PER_SECOND
                                                ),
                                                false,
                                            );
                                            draw_right_stat_cell(
                                                ui_grid,
                                                col_widths[5],
                                                &format!(
                                                    "{:.3}",
                                                    stats.duration_stats.median()
                                                        * MILLISECONDS_PER_SECOND
                                                ),
                                                false,
                                            );

                                            ui_grid.end_row();
                                        }
                                    }

                                    // Overall statistics row
                                    let overall = &result.overall_stats;
                                    draw_node_name_cell(
                                        ui_grid,
                                        col_widths[0],
                                        &NodeIdentifier::AllNodes.to_string(),
                                        true,
                                    );
                                    draw_right_stat_cell(
                                        ui_grid,
                                        col_widths[1],
                                        &format!("{}", overall.duration_stats.count),
                                        true,
                                    );

                                    let min_text_overall = format!(
                                        "{:.3}",
                                        overall.duration_stats.min * MILLISECONDS_PER_SECOND
                                    );
                                    self.draw_clickable_stat_cell(
                                        ClickableStatCellDrawParams {
                                            ui: ui_grid,
                                            width: col_widths[2],
                                            value_str: &min_text_overall,
                                            is_strong: true,
                                            node_identifier: NodeIdentifier::AllNodes,
                                            stat_type: StatType::Min,
                                        },
                                        &mut span_to_view,
                                    );

                                    let max_text_overall = format!(
                                        "{:.3}",
                                        overall.duration_stats.max * MILLISECONDS_PER_SECOND
                                    );
                                    self.draw_clickable_stat_cell(
                                        ClickableStatCellDrawParams {
                                            ui: ui_grid,
                                            width: col_widths[3],
                                            value_str: &max_text_overall,
                                            is_strong: true,
                                            node_identifier: NodeIdentifier::AllNodes,
                                            stat_type: StatType::Max,
                                        },
                                        &mut span_to_view,
                                    );

                                    draw_right_stat_cell(
                                        ui_grid,
                                        col_widths[4],
                                        &format!(
                                            "{:.3}",
                                            overall.duration_stats.mean() * MILLISECONDS_PER_SECOND
                                        ),
                                        true,
                                    );
                                    draw_right_stat_cell(
                                        ui_grid,
                                        col_widths[5],
                                        &format!(
                                            "{:.3}",
                                            overall.duration_stats.median()
                                                * MILLISECONDS_PER_SECOND
                                        ),
                                        true,
                                    );

                                    ui_grid.end_row();
                                });
                        } else if self.selected_span_name.is_some() {
                            ui.label("Click 'Analyze' to see statistics for the selected span.");
                        } else {
                            ui.label("Select a span from the list to analyze.");
                        }
                    });

                ui.separator();

                ui.add_space(10.0);
                if ui.button("Close").clicked() {
                    modal_closed = true;
                }
            });
        });

        // Apply changes if modal closed
        if modal_closed {
            self.show = false;
            self.spans_processed = false;
            self.selected_span_name = None;
            self.search_text = String::new();
            self.detailed_span_analysis = None;
            self.analysis_summary_message = None;
            self.span_details = None;
        }

        if let Some(span) = span_to_view {
            self.span_details = Some(span);
            ctx.request_repaint();
        }

        if let Some(span) = &self.span_details {
            if self.show_span_details(ctx, span, max_width, max_height) {
                self.span_details = None;
                ctx.request_repaint();
            }
        }
    }

    /// Show details of a specific span.
    fn show_span_details(
        &self,
        ctx: &Context,
        span: &Rc<Span>,
        max_width: f32,
        max_height: f32,
    ) -> bool {
        let mut should_close = false;

        // Create a modal dialog for the span details
        let mut open = true;
        Window::new("Span Details")
            .fixed_size([max_width * 0.8, max_height * 0.6])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .order(Order::Foreground)
            .show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| {
                    ui.heading(&span.name);
                    ui.add_space(10.0);

                    // Display timing information
                    ui.strong(format!(
                        "Duration: {:.3} ms",
                        (span.end_time - span.start_time) * 1000.0
                    ));
                    ui.label(format!(
                        "Time: {} - {}",
                        crate::types::time_point_to_utc_string(span.start_time),
                        crate::types::time_point_to_utc_string(span.end_time)
                    ));

                    // Display span identification
                    ui.add_space(5.0);
                    ui.label(format!("Node: {}", span.node.name));
                    ui.label(format!("Span ID: {}", hex::encode(&span.span_id)));
                    ui.label(format!(
                        "Parent Span ID: {}",
                        hex::encode(&span.parent_span_id)
                    ));

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Display attributes
                    ui.heading("Attributes");
                    if span.attributes.is_empty() {
                        ui.label("No attributes");
                    } else {
                        Grid::new("span_details_attributes")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (name, value) in &span.attributes {
                                    ui.strong(name);
                                    ui.label(crate::types::value_to_text(value));
                                    ui.end_row();
                                }
                            });
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Display events
                    ui.heading("Events");
                    ScrollArea::vertical()
                        .max_height(max_height * 0.3)
                        .show(ui, |ui| {
                            if span.events.is_empty() {
                                ui.label("No events");
                            } else {
                                for event in &span.events {
                                    ui.collapsing(event.name.clone(), |ui| {
                                        ui.label(format!(
                                            "Time: {}",
                                            crate::types::time_point_to_utc_string(event.time)
                                        ));

                                        for (name, value) in &event.attributes {
                                            ui.label(format!(
                                                "{}: {}",
                                                name,
                                                crate::types::value_to_text(value)
                                            ));
                                        }
                                    });
                                }
                            }
                        });

                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        should_close = true;
                    }
                });
            });

        // Check for ESC key to close the span details window
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) {
                should_close = true;
            }
        });

        !open || should_close
    }
}
