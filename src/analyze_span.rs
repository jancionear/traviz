use crate::analyze_utils::{
    calculate_table_column_widths, collect_matching_spans, draw_left_aligned_text_cell,
    draw_right_aligned_text_cell, process_spans_for_analysis, show_span_details, span_search_ui,
    span_selection_list_ui, Statistics,
};
use crate::types::{NodeIdentifier, Span, MILLISECONDS_PER_SECOND};
use eframe::egui::{
    Align, Button, Color32, Context, Grid, Id, Label, Layout, Modal, RichText, ScrollArea, Sense,
    Ui, Vec2,
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
    duration_stats: Statistics,

    min_span: Option<Rc<Span>>,
    max_span: Option<Rc<Span>>,
}

impl SpanStatistics {
    fn new() -> Self {
        Self {
            duration_stats: Statistics::new(),
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

impl AnalyzeSpanModal {
    /// Helper method to draw a right-aligned, clickable statistics cell (for Min/Max).
    fn draw_clickable_stat_cell(
        &self,
        ui: &mut Ui,
        width: f32,
        value_str: &str,
        is_strong: bool,
        node_identifier: NodeIdentifier,
        stat_type: StatType,
        span_to_view: &mut Option<Rc<Span>>,
    ) {
        ui.scope(|cell_ui| {
            cell_ui.set_min_width(width);
            cell_ui.with_layout(Layout::right_to_left(Align::Center), |inner_ui| {
                let mut rich_text = RichText::new(value_str)
                    .monospace()
                    .color(Color32::from_rgb(50, 150, 200));
                if is_strong {
                    rich_text = rich_text.strong();
                }
                let response = inner_ui.add(Label::new(rich_text).sense(Sense::click()));

                if response.clicked() {
                    let found_span = match stat_type {
                        StatType::Min => self.find_min_span_for_node(&node_identifier),
                        StatType::Max => self.find_max_span_for_node(&node_identifier),
                    };
                    if let Some(s) = found_span {
                        *span_to_view = Some(s);
                    }
                }
                let hover_text = match stat_type {
                    StatType::Min => "Click to see details of span with minimum duration",
                    StatType::Max => "Click to see details of span with maximum duration",
                };
                if response.hovered() {
                    response.on_hover_text(hover_text);
                }
            });
        });
    }

    pub fn open(&mut self, spans_for_analysis: &[Rc<Span>]) {
        self.show = true;
        self.search_text = String::new();
        self.update_span_list(spans_for_analysis);
        self.spans_processed = true;
    }

    pub fn update_span_list(&mut self, spans: &[Rc<Span>]) {
        let (all_spans, unique_names) = process_spans_for_analysis(spans);
        self.all_spans_for_analysis = all_spans;
        self.unique_span_names = unique_names;
    }

    fn perform_span_analysis(&mut self, target_span_name: &str) {
        let mut matching_spans = Vec::new();
        let target_name = target_span_name.to_string();

        // Collect matching spans
        collect_matching_spans(
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
                        span_search_ui(
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
                let selection_changed = span_selection_list_ui(
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
                    let col_widths = calculate_table_column_widths(grid_width, &col_percentages);

                    // Header row - outside scrollable area to make it sticky
                    Grid::new("span_analysis_header_grid")
                        .num_columns(6)
                        .spacing([10.0, 6.0])
                        .striped(true)
                        .min_col_width(0.0)
                        .show(ui, |ui_grid| {
                            // First cell (Node) is left-aligned
                            draw_left_aligned_text_cell(ui_grid, col_widths[0], "Node", true);

                            // Subsequent cells are right-aligned
                            draw_right_aligned_text_cell(
                                ui_grid,
                                col_widths[1],
                                "Count",
                                true,
                                None,
                            );
                            draw_right_aligned_text_cell(
                                ui_grid,
                                col_widths[2],
                                "Min (ms)",
                                true,
                                None,
                            );
                            draw_right_aligned_text_cell(
                                ui_grid,
                                col_widths[3],
                                "Max (ms)",
                                true,
                                None,
                            );
                            draw_right_aligned_text_cell(
                                ui_grid,
                                col_widths[4],
                                "Mean (ms)",
                                true,
                                None,
                            );
                            draw_right_aligned_text_cell(
                                ui_grid,
                                col_widths[5],
                                "Median (ms)",
                                true,
                                None,
                            );

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
                                calculate_table_column_widths(grid_width, &col_percentages);

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
                                            draw_left_aligned_text_cell(
                                                ui_grid,
                                                col_widths[0],
                                                &node_name,
                                                false,
                                            );
                                            draw_right_aligned_text_cell(
                                                ui_grid,
                                                col_widths[1],
                                                &format!("{}", stats.duration_stats.count),
                                                false,
                                                None,
                                            );

                                            let min_text = format!(
                                                "{:.3}",
                                                stats.duration_stats.min * MILLISECONDS_PER_SECOND
                                            );
                                            self.draw_clickable_stat_cell(
                                                ui_grid,
                                                col_widths[2],
                                                &min_text,
                                                false,
                                                NodeIdentifier::Node(node_name.clone()),
                                                StatType::Min,
                                                &mut span_to_view,
                                            );

                                            let max_text = format!(
                                                "{:.3}",
                                                stats.duration_stats.max * MILLISECONDS_PER_SECOND
                                            );
                                            self.draw_clickable_stat_cell(
                                                ui_grid,
                                                col_widths[3],
                                                &max_text,
                                                false,
                                                NodeIdentifier::Node(node_name.clone()),
                                                StatType::Max,
                                                &mut span_to_view,
                                            );

                                            draw_right_aligned_text_cell(
                                                ui_grid,
                                                col_widths[4],
                                                &format!(
                                                    "{:.3}",
                                                    stats.duration_stats.mean()
                                                        * MILLISECONDS_PER_SECOND
                                                ),
                                                false,
                                                None,
                                            );
                                            draw_right_aligned_text_cell(
                                                ui_grid,
                                                col_widths[5],
                                                &format!(
                                                    "{:.3}",
                                                    stats.duration_stats.median()
                                                        * MILLISECONDS_PER_SECOND
                                                ),
                                                false,
                                                None,
                                            );

                                            ui_grid.end_row();
                                        }
                                    }

                                    // Overall statistics row
                                    let overall = &result.overall_stats;
                                    draw_left_aligned_text_cell(
                                        ui_grid,
                                        col_widths[0],
                                        &NodeIdentifier::AllNodes.to_string(),
                                        true,
                                    );
                                    draw_right_aligned_text_cell(
                                        ui_grid,
                                        col_widths[1],
                                        &format!("{}", overall.duration_stats.count),
                                        true,
                                        None,
                                    );

                                    let min_text_overall = format!(
                                        "{:.3}",
                                        overall.duration_stats.min * MILLISECONDS_PER_SECOND
                                    );
                                    self.draw_clickable_stat_cell(
                                        ui_grid,
                                        col_widths[2],
                                        &min_text_overall,
                                        true,
                                        NodeIdentifier::AllNodes,
                                        StatType::Min,
                                        &mut span_to_view,
                                    );

                                    let max_text_overall = format!(
                                        "{:.3}",
                                        overall.duration_stats.max * MILLISECONDS_PER_SECOND
                                    );
                                    self.draw_clickable_stat_cell(
                                        ui_grid,
                                        col_widths[3],
                                        &max_text_overall,
                                        true,
                                        NodeIdentifier::AllNodes,
                                        StatType::Max,
                                        &mut span_to_view,
                                    );

                                    draw_right_aligned_text_cell(
                                        ui_grid,
                                        col_widths[4],
                                        &format!(
                                            "{:.3}",
                                            overall.duration_stats.mean() * MILLISECONDS_PER_SECOND
                                        ),
                                        true,
                                        None,
                                    );
                                    draw_right_aligned_text_cell(
                                        ui_grid,
                                        col_widths[5],
                                        &format!(
                                            "{:.3}",
                                            overall.duration_stats.median()
                                                * MILLISECONDS_PER_SECOND
                                        ),
                                        true,
                                        None,
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

        // Reset fields if modal got closed
        if modal_closed {
            self.show = false;
            self.spans_processed = false;
            self.selected_span_name = None;
            self.search_text = String::new();
            self.detailed_span_analysis = None;
            self.analysis_summary_message = None;
        }

        // If a specific span was clicked for detailed view (e.g., min/max duration span),
        // or if a detail view was already open, show/keep it open.
        if let Some(span_rc) = span_to_view.or_else(|| self.span_details.take()) {
            if show_span_details(ctx, &span_rc, max_width, max_height) {
                self.span_details = None;
            } else {
                self.span_details = Some(span_rc);
            }
        }
    }
}
