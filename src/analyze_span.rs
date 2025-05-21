use crate::analyze_utils;
use crate::types::Span;
use crate::types::MILLISECONDS_PER_SECOND;
use eframe::egui::{self, Button, Color32, Grid, Key, Layout, Modal, ScrollArea, Vec2};
use std::collections::{HashMap, HashSet};
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

impl AnalyzeSpanModal {
    /// Update span list and store spans internally.
    pub fn update_span_list(&mut self, spans: &[Rc<Span>]) {
        self.all_spans_for_analysis.clear();

        // Collect all spans including children
        for span in spans {
            analyze_utils::collect_span_tree_with_deduplication(
                span,
                &mut self.all_spans_for_analysis,
            );
        }

        // Create a set of unique span names
        let mut unique_span_names: HashSet<String> = HashSet::new();
        for span in &self.all_spans_for_analysis {
            unique_span_names.insert(span.name.clone());
        }

        // Convert to sorted vector
        self.unique_span_names = unique_span_names.into_iter().collect();
        self.unique_span_names.sort_by_key(|a| a.to_lowercase());
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
    fn find_min_span_for_node(&self, node_name: &str) -> Option<Rc<Span>> {
        if let Some(result) = &self.detailed_span_analysis {
            if node_name == "ALL NODES" {
                return result.overall_stats.get_min_span();
            } else if let Some(stats) = result.per_node_stats.get(node_name) {
                return stats.get_min_span();
            }
        }
        None
    }

    /// Returns the span with the maximum duration.
    fn find_max_span_for_node(&self, node_name: &str) -> Option<Rc<Span>> {
        if let Some(result) = &self.detailed_span_analysis {
            if node_name == "ALL NODES" {
                return result.overall_stats.get_max_span();
            } else if let Some(stats) = result.per_node_stats.get(node_name) {
                return stats.get_max_span();
            }
        }
        None
    }

    // Show the modal without requiring spans to be passed each time
    pub fn show_modal(
        &mut self,
        ctx: &egui::Context,
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
                            max_width * 0.65
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
                                let span_name_cloned = span_name_ref.clone(); // Clone here
                                // Run the actual analysis when the button is clicked
                                self.perform_span_analysis(&span_name_cloned); // Use the cloned value
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
                    "analyze_span_list"
                );

                if selection_changed {
                    self.detailed_span_analysis = None;
                    self.analysis_summary_message = None; // Also clear summary message
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
                    let col_percentages = [0.25, 0.1, 0.15, 0.2, 0.15, 0.15]; // Sums to 1.0

                    let grid_width = available_width;

                    // Calculate pixel widths for columns based on percentages of the full width
                    let col_widths = [
                        (grid_width * col_percentages[0]).max(140.0), // Node
                        (grid_width * col_percentages[1]).max(60.0),  // Count
                        (grid_width * col_percentages[2]).max(80.0),  // Min
                        (grid_width * col_percentages[3]).max(80.0),  // Max
                        (grid_width * col_percentages[4]).max(80.0),  // Mean
                        (grid_width * col_percentages[5]).max(80.0),  // Median
                    ];

                    // Header row - outside scrollable area to make it sticky
                    Grid::new("span_analysis_header_grid")
                        .num_columns(6)
                        .spacing([10.0, 6.0])
                        .striped(true)
                        .min_col_width(0.0) // Let the explicit width settings handle sizing
                        .show(ui, |ui| {
                            // Create header cells with consistent width and borders
                            ui.scope(|ui| {
                                ui.set_min_width(col_widths[0]);
                                ui.strong(egui::RichText::new("Node").monospace());
                            });
                            ui.scope(|ui| {
                                ui.set_min_width(col_widths[1]);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.strong(egui::RichText::new("Count").monospace());
                                    },
                                );
                            });
                            ui.scope(|ui| {
                                ui.set_min_width(col_widths[2]);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.strong(egui::RichText::new("Min (ms)").monospace());
                                    },
                                );
                            });
                            ui.scope(|ui| {
                                ui.set_min_width(col_widths[3]);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.strong(egui::RichText::new("Max (ms)").monospace());
                                    },
                                );
                            });
                            ui.scope(|ui| {
                                ui.set_min_width(col_widths[4]);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.strong(egui::RichText::new("Mean (ms)").monospace());
                                    },
                                );
                            });
                            ui.scope(|ui| {
                                ui.set_min_width(col_widths[5]);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.strong(egui::RichText::new("Median (ms)").monospace());
                                    },
                                );
                            });
                            ui.end_row();
                        });

                    // Add a horizontal separator line
                    ui.separator();

                    // Store the grid width and column percentages for the data grid
                    ui.memory_mut(|mem| {
                        mem.data.insert_temp(egui::Id::new("analyze_grid_width"), grid_width);
                        mem.data.insert_temp(egui::Id::new("analyze_col_percentages"), col_percentages);
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
                                let width = mem.data.get_temp::<f32>(egui::Id::new("analyze_grid_width"))
                                    .unwrap_or_else(|| ui.available_width());
                                let percentages = mem.data.get_temp::<[f32; 6]>(egui::Id::new("analyze_col_percentages"))
                                    .unwrap_or([0.25, 0.1, 0.15, 0.2, 0.15, 0.15]);
                                (width, percentages)
                            });

                            // Calculate column widths using the same grid width and percentages
                            let col_widths = [
                                (grid_width * col_percentages[0]).max(140.0), // Node
                                (grid_width * col_percentages[1]).max(60.0),  // Count
                                (grid_width * col_percentages[2]).max(80.0),  // Min
                                (grid_width * col_percentages[3]).max(80.0),  // Max
                                (grid_width * col_percentages[4]).max(80.0),  // Mean
                                (grid_width * col_percentages[5]).max(80.0),  // Median
                            ];

                            // Use Grid for tabular data (without headers)
                            Grid::new("span_analysis_grid")
                                .num_columns(6)
                                .spacing([10.0, 6.0])
                                .striped(true)
                                .min_col_width(0.0) // Let the explicit width settings handle sizing
                                .show(ui, |ui| {
                                    // Get nodes and sort them alphabetically
                                    let mut node_names: Vec<String> =
                                        result.per_node_stats.keys().cloned().collect();
                                    node_names.sort();

                                    // Rows for each node
                                    for node_name in node_names {
                                        if let Some(stats) = result.per_node_stats.get(&node_name) {
                                            // Apply consistent width to each cell with monospace font
                                            ui.scope(|ui| {
                                                ui.set_min_width(col_widths[0]);
                                                ui.label(
                                                    egui::RichText::new(&node_name).monospace(),
                                                );
                                            });
                                            ui.scope(|ui| {
                                                ui.set_min_width(col_widths[1]);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{}",
                                                                stats.duration_stats.count
                                                            ))
                                                            .monospace(),
                                                        );
                                                    },
                                                );
                                            });
                                            ui.scope(|ui| {
                                                ui.set_min_width(col_widths[2]);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let min_text = format!("{:.3}", stats.duration_stats.min * MILLISECONDS_PER_SECOND);
                                                        let min_response = ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(min_text)
                                                                    .monospace()
                                                                    .color(Color32::from_rgb(50, 150, 200))
                                                            )
                                                            .sense(egui::Sense::click())
                                                        );

                                                        if min_response.clicked() {
                                                            if let Some(min_span) = self.find_min_span_for_node(&node_name) {
                                                                span_to_view = Some(min_span);
                                                            }
                                                        }
                                                        if min_response.hovered() {
                                                            min_response.on_hover_text("Click to see details of span with minimum duration");
                                                        }
                                                    },
                                                );
                                            });
                                            ui.scope(|ui| {
                                                ui.set_min_width(col_widths[3]);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let max_text = format!("{:.3}", stats.duration_stats.max * MILLISECONDS_PER_SECOND);
                                                        let max_response = ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(max_text)
                                                                    .monospace()
                                                                    .color(Color32::from_rgb(50, 150, 200))
                                                            )
                                                            .sense(egui::Sense::click())
                                                        );

                                                        if max_response.clicked() {
                                                            if let Some(max_span) = self.find_max_span_for_node(&node_name) {
                                                                span_to_view = Some(max_span);
                                                            }
                                                        }

                                                        if max_response.hovered() {
                                                            max_response.on_hover_text("Click to see details of span with maximum duration");
                                                        }
                                                    },
                                                );
                                            });
                                            ui.scope(|ui| {
                                                ui.set_min_width(col_widths[4]);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{:.3}",
                                                                stats.duration_stats.mean() * MILLISECONDS_PER_SECOND
                                                            ))
                                                            .monospace(),
                                                        );
                                                    },
                                                );
                                            });
                                            ui.scope(|ui| {
                                                ui.set_min_width(col_widths[5]);
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{:.3}",
                                                                stats.duration_stats.median() * MILLISECONDS_PER_SECOND
                                                            ))
                                                            .monospace(),
                                                        );
                                                    },
                                                );
                                            });
                                            ui.end_row();
                                        }
                                    }

                                    // Overall statistics row
                                    let overall = &result.overall_stats;
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[0]);
                                        ui.strong(egui::RichText::new("ALL NODES").monospace());
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[1]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.strong(
                                                    egui::RichText::new(format!(
                                                        "{}",
                                                        overall.duration_stats.count
                                                    ))
                                                    .monospace(),
                                                );
                                            },
                                        );
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[2]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let min_text = format!("{:.3}", overall.duration_stats.min * MILLISECONDS_PER_SECOND);
                                                let min_response = ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(min_text)
                                                            .monospace()
                                                            .color(Color32::from_rgb(50, 150, 200))
                                                            .strong()
                                                    )
                                                    .sense(egui::Sense::click())
                                                );

                                                if min_response.clicked() {
                                                    if let Some(min_span) = self.find_min_span_for_node("ALL NODES") {
                                                        span_to_view = Some(min_span);
                                                    }
                                                }

                                                if min_response.hovered() {
                                                    min_response.on_hover_text("Click to see details of span with minimum duration");
                                                }
                                            },
                                        );
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[3]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let max_text = format!("{:.3}", overall.duration_stats.max * 1000.0);
                                                let max_response = ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(max_text)
                                                            .monospace()
                                                            .color(Color32::from_rgb(50, 150, 200))
                                                            .strong()
                                                    )
                                                    .sense(egui::Sense::click())
                                                );

                                                if max_response.clicked() {
                                                    if let Some(max_span) = self.find_max_span_for_node("ALL NODES") {
                                                        span_to_view = Some(max_span);
                                                    }
                                                }

                                                if max_response.hovered() {
                                                    max_response.on_hover_text("Click to see details of span with maximum duration");
                                                }
                                            },
                                        );
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[4]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{:.3}",
                                                        overall.duration_stats.mean() * 1000.0
                                                    ))
                                                    .monospace(),
                                                );
                                            },
                                        );
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[5]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{:.3}",
                                                        overall.duration_stats.median() * 1000.0
                                                    ))
                                                    .monospace(),
                                                );
                                            },
                                        );
                                    });
                                    ui.end_row();
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

    // Show details of a specific span
    fn show_span_details(
        &self,
        ctx: &egui::Context,
        span: &Rc<Span>,
        max_width: f32,
        max_height: f32,
    ) -> bool {
        let mut should_close = false;

        // Create a modal dialog for the span details
        let mut open = true;
        egui::Window::new("Span Details")
            .fixed_size([max_width * 0.8, max_height * 0.6])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Foreground)
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
                        egui::Grid::new("span_details_attributes")
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
