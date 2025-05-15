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
}

impl SpanStatistics {
    fn new() -> Self {
        Self {
            count: 0,
            max_duration: f64::MIN,
            min_duration: f64::MAX,
            total_duration: 0.0,
            durations: Vec::new(),
        }
    }

    fn add_span(&mut self, span: &Rc<Span>) {
        let duration = span.end_time - span.start_time;
        self.count += 1;
        self.max_duration = self.max_duration.max(duration);
        self.min_duration = self.min_duration.min(duration);
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
            "Stored {} spans in the analyze modal",
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

                // Define consistent column widths
                let col_widths = [140.0, 60.0, 100.0, 100.0, 100.0, 100.0];

                // Create the grid headers first outside the scroll area (to keep them visible)
                if self.analyzer.span_statistics.is_some() {
                    ui.add_space(10.0);

                    // Header row - outside scrollable area to make it sticky
                    Grid::new("span_analysis_header_grid")
                        .num_columns(6)
                        .spacing([10.0, 6.0])
                        .striped(true)
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
                            // Use Grid for tabular data (without headers)
                            Grid::new("span_analysis_grid")
                                .num_columns(6)
                                .spacing([10.0, 6.0])
                                .striped(true)
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
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{:.3}",
                                                                stats.min_duration * 1000.0
                                                            ))
                                                            .monospace(),
                                                        );
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
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{:.3}",
                                                                stats.max_duration * 1000.0
                                                            ))
                                                            .monospace(),
                                                        );
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
                                                ui.strong(
                                                    egui::RichText::new(format!(
                                                        "{:.3}",
                                                        overall.min_duration * 1000.0
                                                    ))
                                                    .monospace(),
                                                );
                                            },
                                        );
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[3]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.strong(
                                                    egui::RichText::new(format!(
                                                        "{:.3}",
                                                        overall.max_duration * 1000.0
                                                    ))
                                                    .monospace(),
                                                );
                                            },
                                        );
                                    });
                                    ui.scope(|ui| {
                                        ui.set_min_width(col_widths[4]);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.strong(
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
                                                ui.strong(
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

                // Only Close button at the bottom
                if ui.button("Close").clicked() {
                    modal_closed = true;
                }
            });
        });

        // Close with Escape key
        ctx.input(|i| {
            if i.key_down(Key::Escape) {
                modal_closed = true;
            }
        });

        // Apply changes if modal closed
        // Reset all selections
        if modal_closed {
            self.show = false;
            self.spans_processed = false;
            self.selected_span_name = None;
            self.search_text = String::new();
            self.analyzer.span_statistics = None;
        }
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
