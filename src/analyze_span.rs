use crate::types::Span;
use eframe::egui::{self, Button, Color32, Grid, Key, Layout, Modal, ScrollArea, TextEdit, Vec2};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Default)]
pub struct AnalyzeSpanModal {
    pub show: bool,
    pub search_text: String,
    pub selected_span_name: Option<String>,
    pub unique_span_names: Vec<String>,
    pub analyzer: SpanAnalyzer,
    pub spans_processed: bool,
    stored_spans: Vec<Rc<Span>>,
    // Track the span to display details for
    span_details: Option<Rc<Span>>,
}

// Separate struct for handling span analysis to avoid borrow issues
#[derive(Default)]
pub struct SpanAnalyzer {
    pub analyze_result: Option<String>,
    span_statistics: Option<SpanAnalysisResult>,
}

// Struct to hold duration statistics for spans
struct SpanStatistics {
    count: usize,
    max_duration: f64,
    min_duration: f64,
    total_duration: f64,
    durations: Vec<f64>,
    // Store references to the actual min and max spans
    min_span: Option<Rc<Span>>,
    max_span: Option<Rc<Span>>,
}

impl SpanStatistics {
    fn new() -> Self {
        Self {
            count: 0,
            max_duration: f64::MIN,
            min_duration: f64::MAX,
            total_duration: 0.0,
            durations: Vec::new(),
            min_span: None,
            max_span: None,
        }
    }

    fn add_span(&mut self, span: &Rc<Span>) {
        let duration = span.end_time - span.start_time;
        self.count += 1;

        // Update max duration and store the span if it's the new max
        if duration > self.max_duration {
            self.max_duration = duration;
            self.max_span = Some(span.clone());
        }

        // Update min duration and store the span if it's the new min
        if duration < self.min_duration {
            self.min_duration = duration;
            self.min_span = Some(span.clone());
        }

        self.total_duration += duration;
        self.durations.push(duration);
    }

    fn mean_duration(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.total_duration / self.count as f64
    }

    fn median_duration(&self) -> f64 {
        if self.durations.is_empty() {
            return 0.0;
        }

        let mut sorted_durations = self.durations.clone();
        sorted_durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mid = sorted_durations.len() / 2;
        if sorted_durations.len() % 2 == 0 {
            (sorted_durations[mid - 1] + sorted_durations[mid]) / 2.0
        } else {
            sorted_durations[mid]
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

// Struct to hold analysis results for all nodes
struct SpanAnalysisResult {
    span_name: String,
    per_node_stats: HashMap<String, SpanStatistics>,
    overall_stats: SpanStatistics,
}

impl SpanAnalyzer {
    // Analyze spans without requiring spans to be passed in each time
    pub fn analyze_spans(&mut self, stored_spans: &[Rc<Span>], target_span_name: &str) {
        // Collect all spans with the target name from all nodes
        let mut matching_spans = Vec::new();
        let target_name = target_span_name.to_string();

        // Collect matching spans
        collect_matching_spans(stored_spans, &target_name, &mut matching_spans);

        if matching_spans.is_empty() {
            self.analyze_result = Some(format!("No spans found with name '{}'", target_name));
            return;
        }

        // Group spans by node
        let mut per_node_stats: HashMap<String, SpanStatistics> = HashMap::new();
        let mut overall_stats = SpanStatistics::new();

        for span in matching_spans {
            // Add to overall statistics
            overall_stats.add_span(&span);

            // Add to per-node statistics
            let node_name = span.node.name.clone();
            per_node_stats
                .entry(node_name)
                .or_insert_with(SpanStatistics::new)
                .add_span(&span);
        }

        // Store the results
        self.span_statistics = Some(SpanAnalysisResult {
            span_name: target_name,
            per_node_stats,
            overall_stats,
        });
    }

    // Find span with minimum duration for a node - now uses the stored references
    pub fn find_min_span(&self, node_name: &str) -> Option<Rc<Span>> {
        if let Some(result) = &self.span_statistics {
            if node_name == "ALL NODES" {
                return result.overall_stats.get_min_span();
            } else if let Some(stats) = result.per_node_stats.get(node_name) {
                return stats.get_min_span();
            }
        }
        None
    }

    // Find span with maximum duration for a node - now uses the stored references
    pub fn find_max_span(&self, node_name: &str) -> Option<Rc<Span>> {
        if let Some(result) = &self.span_statistics {
            if node_name == "ALL NODES" {
                return result.overall_stats.get_max_span();
            } else if let Some(stats) = result.per_node_stats.get(node_name) {
                return stats.get_max_span();
            }
        }
        None
    }
}

impl AnalyzeSpanModal {
    // Update span list and store spans internally
    pub fn update_span_list(&mut self, spans: &[Rc<Span>]) {
        // Store all spans including children
        self.stored_spans.clear();

        // Collect all spans including children
        for span in spans {
            collect_all_spans(span, &mut self.stored_spans);
        }

        println!(
            "Stored {} total spans in the analyze modal",
            self.stored_spans.len()
        );

        // Create a set of unique span names
        let mut unique_span_names: HashSet<String> = HashSet::new();

        for span in &self.stored_spans {
            unique_span_names.insert(span.name.clone());
        }

        // Convert to sorted vector
        self.unique_span_names = unique_span_names.into_iter().collect();
        self.unique_span_names.sort_by_key(|a| a.to_lowercase());
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
                        ui.vertical(|ui| {
                            ui.label("Search span by name:");
                            let text_edit = TextEdit::singleline(&mut self.search_text)
                                .hint_text("Type to search")
                                .background_color(Color32::from_gray(40))
                                .desired_width(max_width * 0.65);
                            ui.add(text_edit);
                        });
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
                            if let Some(span_name) = &self.selected_span_name {
                                // Run the actual analysis when the button is clicked
                                // Use stored_spans instead of passing them from outside
                                self.analyzer.analyze_spans(&self.stored_spans, span_name);
                            }
                        }
                    });
                });

                ui.add_space(10.0);

                // Filter names by the search text
                let search_term = self.search_text.to_lowercase();
                let filtered_names: Vec<&String> = self
                    .unique_span_names
                    .iter()
                    .filter(|name| {
                        search_term.is_empty() || name.to_lowercase().contains(&search_term)
                    })
                    .collect();

                // Split the remaining space - List takes 30% of height
                let list_height = (max_height - 120.0) * 0.3;
                let results_height = (max_height - 120.0) * 0.7 - 20.0;

                // Show list of span names in a scrollable area
                ui.label(format!("Spans ({}):", filtered_names.len()));

                // Make sure we always have a scrollbar for the list
                ScrollArea::vertical()
                    .max_height(list_height)
                    .id_salt("spans_list_scroll_area")
                    .show_viewport(ui, |ui, _viewport| {
                        for name in &filtered_names {
                            let is_selected = self.selected_span_name.as_ref() == Some(name);

                            let response = ui.selectable_label(is_selected, *name);

                            if response.clicked() {
                                self.selected_span_name = Some((*name).clone());
                                self.analyzer.span_statistics = None;
                            }
                        }
                    });

                ui.separator();

                // Results area
                ui.label("Analysis Results:");

                if let Some(result) = &self.analyzer.span_statistics {
                    ui.label(format!("Analysis of span: '{}'", result.span_name));
                }

                // Create the grid headers first outside the scroll area (to keep them visible)
                if self.analyzer.span_statistics.is_some() {
                    ui.add_space(10.0);

                    // Calculate total available width
                    let total_available_width = ui.available_width();

                    // Define column widths based on available space while keeping minimums
                    // First column is for node names, others are for values
                    let col_widths = [
                        (total_available_width * 0.25).max(140.0),  // Node name (25% but min 140px)
                        (total_available_width * 0.1).max(60.0),    // Count (10% but min 60px)
                        (total_available_width * 0.15).max(80.0),   // Min (15% but min 80px)
                        (total_available_width * 0.15).max(80.0),   // Max (15% but min 80px)
                        (total_available_width * 0.15).max(80.0),   // Mean (15% but min 80px)
                        (total_available_width * 0.15).max(80.0),   // Median (15% but min 80px)
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
                }

                // Grid contents in a scrollable area
                ScrollArea::vertical()
                    .max_height(results_height)
                    .id_salt("analysis_results_scroll_area")
                    .show_viewport(ui, |ui, _viewport| {
                        if let Some(result) = &self.analyzer.span_statistics {
                            // Calculate total available width again (might be different inside the scroll area)
                            let total_available_width = ui.available_width();

                            // Define column widths based on available space while keeping minimums
                            let col_widths = [
                                (total_available_width * 0.25).max(140.0),  // Node name (25% but min 140px)
                                (total_available_width * 0.1).max(60.0),    // Count (10% but min 60px)
                                (total_available_width * 0.15).max(80.0),   // Min (15% but min 80px)
                                (total_available_width * 0.15).max(80.0),   // Max (15% but min 80px)
                                (total_available_width * 0.15).max(80.0),   // Mean (15% but min 80px)
                                (total_available_width * 0.15).max(80.0),   // Median (15% but min 80px)
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
                                                                stats.count
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
                                                        let min_text = format!("{:.3}", stats.min_duration * 1000.0);
                                                        let min_response = ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(min_text)
                                                                    .monospace()
                                                                    .color(Color32::from_rgb(50, 150, 200))
                                                            )
                                                            .sense(egui::Sense::click())
                                                        );

                                                        if min_response.clicked() {
                                                            if let Some(min_span) = self.analyzer.find_min_span(&node_name) {
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
                                                        let max_text = format!("{:.3}", stats.max_duration * 1000.0);
                                                        let max_response = ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(max_text)
                                                                    .monospace()
                                                                    .color(Color32::from_rgb(50, 150, 200))
                                                            )
                                                            .sense(egui::Sense::click())
                                                        );

                                                        if max_response.clicked() {
                                                            if let Some(max_span) = self.analyzer.find_max_span(&node_name) {
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
                                                                stats.mean_duration() * 1000.0
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
                                                                stats.median_duration() * 1000.0
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
                                                        overall.count
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
                                                let min_text = format!("{:.3}", overall.min_duration * 1000.0);
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
                                                    if let Some(min_span) = self.analyzer.find_min_span("ALL NODES") {
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
                                                let max_text = format!("{:.3}", overall.max_duration * 1000.0);
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
                                                    if let Some(max_span) = self.analyzer.find_max_span("ALL NODES") {
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
                                                        overall.mean_duration() * 1000.0
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
                                                        overall.median_duration() * 1000.0
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
            self.analyzer.span_statistics = None;
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

// Helper function to collect all existing spans.
fn collect_all_spans(span: &Rc<Span>, all_spans: &mut Vec<Rc<Span>>) {
    // Use a HashSet to track span IDs we've already collected
    let mut seen_span_ids: HashSet<Vec<u8>> = HashSet::new();
    collect_all_spans_with_deduplication(span, all_spans, &mut seen_span_ids);
}

fn collect_all_spans_with_deduplication(
    span: &Rc<Span>,
    all_spans: &mut Vec<Rc<Span>>,
    seen_span_ids: &mut HashSet<Vec<u8>>,
) {
    // Only add this span if we haven't seen its ID before
    if !seen_span_ids.contains(&span.span_id) {
        seen_span_ids.insert(span.span_id.clone());
        all_spans.push(span.clone());
        // Recursively add all children
        for child in span.children.borrow().iter() {
            collect_all_spans_with_deduplication(child, all_spans, seen_span_ids);
        }
    }
}

fn collect_matching_spans(
    spans: &[Rc<Span>],
    target_name: &str,
    matching_spans: &mut Vec<Rc<Span>>,
) {
    // Since the input spans are already deduplicated, we can simply filter
    // for spans with matching names
    for span in spans {
        if span.name == target_name {
            matching_spans.push(span.clone());
        }
    }
}
