use crate::analyze_utils::{
    calculate_table_column_widths, collect_matching_spans, draw_left_aligned_text_cell,
    draw_right_aligned_text_cell, process_spans_for_analysis, span_search_ui,
    span_selection_list_ui, Statistics,
};
use crate::types::Span;
use crate::types::MILLISECONDS_PER_SECOND;
use eframe::egui::{
    self, Button, Color32, ComboBox, Grid, Layout, Modal, RichText, ScrollArea, TextEdit, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

/// Structure to represent a dependency link between spans.
pub struct DependencyLink {
    pub source_spans: Vec<Rc<Span>>,
    pub target_span: Rc<Span>,
}

/// Holds statistics and a list of identified dependency links where the target span resides on a specific node.
pub struct NodeDependencyMetrics {
    pub link_delay_statistics: Statistics,
    pub links: Vec<DependencyLink>,
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub enum SourceScope {
    #[default]
    SameNode,
    AllNodes,
}

impl std::fmt::Display for SourceScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceScope::SameNode => write!(f, "self"),
            SourceScope::AllNodes => write!(f, "all nodes"),
        }
    }
}

/// Defines the strategy for selecting source spans when multiple are available.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub enum SourceTimingStrategy {
    #[default]
    EarliestFirst,
    LatestFirst,
}

impl std::fmt::Display for SourceTimingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceTimingStrategy::EarliestFirst => write!(f, "Earliest First"),
            SourceTimingStrategy::LatestFirst => write!(f, "Latest First"),
        }
    }
}

pub struct DependencyAnalysisResult {
    pub source_span_name: String,
    pub target_span_name: String,
    pub threshold: usize,
    pub metadata_field: String,
    pub source_scope: SourceScope,
    pub source_timing_strategy: SourceTimingStrategy,
    pub per_node_results: HashMap<String, NodeDependencyMetrics>,
    pub analysis_duration_ms: u128,
}

#[derive(Default)]
pub struct AnalyzeDependencyModal {
    /// Whether the modal window is currently visible.
    pub show: bool,
    /// Text entered by the user in the source span name search box.
    source_search_text: String,
    /// Text entered by the user in the target span name search box.
    target_search_text: String,
    /// The name of the source span currently selected by the user.
    source_span_name: Option<String>,
    /// The name of the target span currently selected by the user.
    target_span_name: Option<String>,
    /// The minimum number of preceding source spans required to form a valid dependency link.
    threshold: usize,
    /// String representation of the threshold for editing in the UI.
    threshold_edit_str: String,
    /// Optional metadata field name used to match source and target spans.
    metadata_field: String,
    /// Scope for selecting source spans: "self" (same node as target) or "all nodes".
    source_scope: SourceScope,
    /// Strategy for selecting source spans (earliest or latest).
    source_timing_strategy: SourceTimingStrategy,
    /// A sorted list of unique span names found in the current trace data.
    unique_span_names: Vec<String>,
    /// Flag indicating if the span list for the modal has been processed from the current trace data.
    pub spans_processed: bool,
    /// Stores the detailed results of the last dependency analysis performed.
    pub analysis_result: Option<DependencyAnalysisResult>,
    /// An optional message describing an error encountered during analysis.
    error_message: Option<String>,
    /// All unique spans (including children) collected from the current trace, used for analysis.
    all_spans_for_analysis: Vec<Rc<Span>>,
    /// If set, indicates a specific node to focus on in the trace view after closing the modal.
    pub focus_node: Option<String>,
}

impl AnalyzeDependencyModal {
    pub fn new() -> Self {
        let initial_threshold = 1;
        Self {
            threshold: initial_threshold,
            threshold_edit_str: initial_threshold.to_string(),
            ..Default::default()
        }
    }

    pub fn open(&mut self, spans_for_analysis: &[Rc<Span>]) {
        self.show = true;
        self.source_search_text = String::new();
        self.target_search_text = String::new();
        self.update_span_list(spans_for_analysis);
        self.spans_processed = true;
    }

    pub fn clear_focus(&mut self) {
        self.focus_node = None;
    }

    pub fn get_links_for_node(&self, node_name: &str) -> Option<&Vec<DependencyLink>> {
        self.analysis_result.as_ref().and_then(|result| {
            result
                .per_node_results
                .get(node_name)
                .map(|metrics| &metrics.links)
        })
    }

    pub fn update_span_list(&mut self, spans: &[Rc<Span>]) {
        let (all_spans, unique_names) = process_spans_for_analysis(spans);
        self.all_spans_for_analysis = all_spans;
        self.unique_span_names = unique_names;
    }

    fn analyze_dependencies(&mut self) {
        self.analysis_result = None;

        // Validate source and target span names
        let source_name = match &self.source_span_name {
            Some(name) => name.clone(),
            None => {
                self.error_message = Some("Source span not selected".to_string());
                return;
            }
        };
        let target_name = match &self.target_span_name {
            Some(name) => name.clone(),
            None => {
                self.error_message = Some("Target span not selected".to_string());
                return;
            }
        };

        // Validate that source and target spans are different
        if source_name == target_name {
            self.error_message = Some("Source and target spans must be different".to_string());
            return;
        }

        // Start timing the analysis
        let analysis_start = Instant::now();

        // Collect all source and target spans
        let mut source_spans = Vec::new();
        let mut target_spans = Vec::new();

        collect_matching_spans(
            &self.all_spans_for_analysis,
            &source_name,
            &mut source_spans,
        );
        collect_matching_spans(
            &self.all_spans_for_analysis,
            &target_name,
            &mut target_spans,
        );

        if source_spans.is_empty() {
            self.error_message = Some(format!("No spans found with name '{}'", source_name));
            return;
        }

        if target_spans.is_empty() {
            self.error_message = Some(format!("No spans found with name '{}'", target_name));
            return;
        }

        // Sort spans by start time
        source_spans.sort_by(|a, b| {
            a.start_time
                .partial_cmp(&b.start_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        target_spans.sort_by(|a, b| {
            a.start_time
                .partial_cmp(&b.start_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Group spans by node
        let mut source_spans_by_node: HashMap<String, Vec<Rc<Span>>> = HashMap::new();
        let mut target_spans_by_node: HashMap<String, Vec<Rc<Span>>> = HashMap::new();

        for span in source_spans {
            source_spans_by_node
                .entry(span.node.name.clone())
                .or_default()
                .push(span);
        }

        for span in target_spans {
            target_spans_by_node
                .entry(span.node.name.clone())
                .or_default()
                .push(span);
        }

        // Per-node dependency analysis
        let mut per_node_results = HashMap::new();
        let node_names = if self.source_scope == SourceScope::SameNode {
            // Only analyze nodes that have both source and target spans
            source_spans_by_node
                .keys()
                .filter(|node_name| target_spans_by_node.contains_key(*node_name))
                .cloned()
                .collect::<Vec<String>>()
        } else {
            // "all nodes"
            // Use source nodes and target nodes
            let mut all_nodes = HashSet::new();
            all_nodes.extend(source_spans_by_node.keys().cloned());
            all_nodes.extend(target_spans_by_node.keys().cloned());
            all_nodes.into_iter().collect()
        };

        // This set is for 'self' mode: if a source span is a potential candidate for any target, it's marked used globally
        let mut global_used_source_span_ids_for_self_mode: HashSet<Vec<u8>> = HashSet::new();

        for node_name in node_names {
            let current_source_node_spans = if self.source_scope == SourceScope::SameNode {
                // Only use spans from this node
                source_spans_by_node
                    .get(&node_name)
                    .cloned()
                    .unwrap_or_default()
            } else {
                // Use spans from all nodes, sorted by time
                let mut all_s_spans: Vec<Rc<Span>> =
                    source_spans_by_node.values().flatten().cloned().collect();
                all_s_spans.sort_by(|a, b| {
                    a.start_time
                        .partial_cmp(&b.start_time)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                all_s_spans
            };

            // Skip if no source spans for this node/scope
            if current_source_node_spans.is_empty() {
                continue;
            }

            // Get target spans (always from the current node being processed for targets)
            let current_target_node_spans = target_spans_by_node
                .get(&node_name)
                .cloned()
                .unwrap_or_default();

            // Skip if no target spans for this node
            if current_target_node_spans.is_empty() {
                continue;
            }

            // Find valid links
            let mut node_links = Vec::new();
            let mut statistics = Statistics::new();
            let mut used_target_spans: HashSet<Vec<u8>> = HashSet::new();
            // For "all nodes" mode:
            // tracks source spans that have successfully linked to a target on *this specific target_node*.
            // Reset for each target_node.
            let mut used_source_ids_for_current_node_all_scope: HashSet<Vec<u8>> = HashSet::new();

            for target_span in current_target_node_spans.iter() {
                if used_target_spans.contains(&target_span.span_id) {
                    // This target has already been linked by a source group
                    continue;
                }

                // Check metadata for target_span if specified
                if !self.metadata_field.is_empty()
                    && !target_span.attributes.contains_key(&self.metadata_field)
                {
                    // Target itself must have the metadata field to be a candidate
                    continue;
                }

                let mut potential_sources_for_this_target: Vec<Rc<Span>> = Vec::new();
                for s_span in current_source_node_spans.iter() {
                    // Check if source span already used based on mode
                    let mut skip_source = false;
                    if self.source_scope == SourceScope::SameNode {
                        if global_used_source_span_ids_for_self_mode.contains(&s_span.span_id) {
                            skip_source = true;
                        }
                    } else {
                        // "all nodes" mode
                        if used_source_ids_for_current_node_all_scope.contains(&s_span.span_id) {
                            skip_source = true;
                        }
                    }
                    if skip_source {
                        continue;
                    }

                    // Basic time validity
                    if s_span.end_time < target_span.start_time {
                        // Check metadata compatibility if field is specified
                        if !self.metadata_field.is_empty() {
                            // Source must also have the metadata field
                            if !s_span.attributes.contains_key(&self.metadata_field) {
                                continue;
                            }
                            // Values must match (target_span's field existence already checked)
                            let source_value = &s_span.attributes[&self.metadata_field];
                            let target_value = &target_span.attributes[&self.metadata_field];
                            if source_value != target_value {
                                continue;
                            }
                        }
                        potential_sources_for_this_target.push(s_span.clone());
                    }
                }

                if potential_sources_for_this_target.len() >= self.threshold && self.threshold > 0 {
                    let num_to_take = self.threshold;
                    let selected_source_spans_group: Vec<Rc<Span>> = match self
                        .source_timing_strategy
                    {
                        SourceTimingStrategy::EarliestFirst => potential_sources_for_this_target
                            .iter()
                            .take(num_to_take)
                            .cloned()
                            .collect(),
                        SourceTimingStrategy::LatestFirst => {
                            let skip_count = potential_sources_for_this_target
                                .len()
                                .saturating_sub(num_to_take);
                            potential_sources_for_this_target
                                .iter()
                                .skip(skip_count)
                                .cloned()
                                .collect()
                        }
                    };

                    if !selected_source_spans_group.is_empty() {
                        // Should always be Some() if num_to_take > 0 and len >= threshold
                        let last_source_in_group = selected_source_spans_group.last().unwrap();

                        if last_source_in_group.end_time < target_span.start_time {
                            let distance = target_span.start_time - last_source_in_group.end_time;

                            statistics.add_value(distance);
                            node_links.push(DependencyLink {
                                source_spans: selected_source_spans_group.clone(),
                                target_span: target_span.clone(),
                            });

                            used_target_spans.insert(target_span.span_id.clone());

                            // Mark the *actually linked* source spans as used for the appropriate scope
                            for linked_s_span in &selected_source_spans_group {
                                if self.source_scope == SourceScope::SameNode {
                                    // In 'self' mode, linked spans are added to the global set.
                                    // Note: potential_sources are also added below, preserving original broader consumption.
                                    global_used_source_span_ids_for_self_mode
                                        .insert(linked_s_span.span_id.clone());
                                } else {
                                    // "all nodes" mode
                                    // In 'all nodes' mode, mark this source as used for this specific target node.
                                    used_source_ids_for_current_node_all_scope
                                        .insert(linked_s_span.span_id.clone());
                                }
                            }
                        }
                    }
                }

                // For "self" mode: *all potential* sources for this target are marked globally used.
                // This runs after link formation attempt for the current target_span.
                if self.source_scope == SourceScope::SameNode {
                    for s_potential_for_this_target in &potential_sources_for_this_target {
                        global_used_source_span_ids_for_self_mode
                            .insert(s_potential_for_this_target.span_id.clone());
                    }
                }
            }

            // Add result for this node if any links were formed
            if !node_links.is_empty() || statistics.count > 0 {
                per_node_results.insert(
                    node_name.clone(),
                    NodeDependencyMetrics {
                        link_delay_statistics: statistics,
                        links: node_links,
                    },
                );
            }
        }

        // Measure analysis duration
        let analysis_duration = analysis_start.elapsed().as_millis();

        // Store the results
        self.analysis_result = Some(DependencyAnalysisResult {
            source_span_name: source_name,
            target_span_name: target_name,
            threshold: self.threshold,
            metadata_field: self.metadata_field.clone(),
            source_scope: self.source_scope.clone(),
            source_timing_strategy: self.source_timing_strategy.clone(),
            per_node_results,
            analysis_duration_ms: analysis_duration,
        });

        self.error_message = None;
    }

    // Show the modal
    pub fn show_modal(&mut self, ctx: &egui::Context, max_width: f32, max_height: f32) {
        if !self.show {
            return;
        }

        let mut modal_closed = false;

        Modal::new("analyze dependency".into()).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(max_width);
                ui.set_max_height(max_height);

                ui.heading("Analyze Dependency");
                ui.add_space(10.0);

                Grid::new("source_target_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // --- Row 1: Search Boxes ---
                        // Source span search
                        ui.vertical(|ui| {
                            ui.set_width(max_width * 0.45);
                            span_search_ui(
                                ui,
                                &mut self.source_search_text,
                                "Source Span:",
                                "Search source span",
                                ui.available_width()
                            );
                        });
                        // Target span search
                        ui.vertical(|ui| {
                            ui.set_width(max_width * 0.45);
                            span_search_ui(
                                ui,
                                &mut self.target_search_text,
                                "Target Span:",
                                "Search target span",
                                ui.available_width()
                            );
                        });
                        ui.end_row();

                        // --- Row 2: Span Lists ---
                        let list_height = 150.0;
                        // Source span list
                        ui.vertical(|ui| {
                            ui.set_width(max_width * 0.45);
                            span_selection_list_ui(
                                ui,
                                &self.unique_span_names,
                                &self.source_search_text,
                                &mut self.source_span_name,
                                list_height,
                                "source_spans_list"
                            );
                        });
                        // Target span list
                        ui.vertical(|ui| {
                            ui.set_width(max_width * 0.45);
                            span_selection_list_ui(
                                ui,
                                &self.unique_span_names,
                                &self.target_search_text,
                                &mut self.target_span_name,
                                list_height,
                                "target_spans_list"
                            );
                        });
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                // Configuration row
                ui.horizontal(|ui| {
                    // Threshold input
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Threshold:");
                            let response = ui.add(
                                TextEdit::singleline(&mut self.threshold_edit_str)
                                    .desired_width(50.0)
                            );

                            let mut commit_valid_input = false;
                            if response.lost_focus() {
                                commit_valid_input = true;
                            }

                            if commit_valid_input {
                                if let Ok(value) = self.threshold_edit_str.parse::<usize>() {
                                    // Ensure minimum of 1
                                    self.threshold = value.max(1);
                                }
                                // Always reset edit string from the (potentially updated) numeric value after commit attempt
                                self.threshold_edit_str = self.threshold.to_string();
                            } else if response.changed() {
                                // User is typing. If they type something invalid, allow it in the text box for now.
                                // It will be validated/reverted on commit (lost_focus/enter).
                                // Optionally, we could try to parse here and give immediate feedback (e.g. red text box)
                                // but the current approach is simpler: validate on commit.
                            }

                            if response.hovered() {
                                response.on_hover_text("Specifies which source span to use (e.g., 2 means use every second source span)");
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // Link Metadata Matcher input
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Link Metadata Matcher:");
                            let response = ui.add(
                                TextEdit::singleline(&mut self.metadata_field)
                                    .desired_width(120.0)
                                    .hint_text("field name")
                            );

                            if response.hovered() {
                                response.on_hover_text(
                                    "Optional. If provided, only spans with matching values for this metadata field can form links. \
                                    Leave empty to ignore metadata matching."
                                );
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // Source toggle dropdown
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Source:");
                            let combo_box_response = ComboBox::new(ui.id().with("source_scope"), "")
                                .selected_text(self.source_scope.to_string())
                                .width(80.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.source_scope,
                                        SourceScope::SameNode, SourceScope::SameNode.to_string());
                                    ui.selectable_value(&mut self.source_scope,
                                        SourceScope::AllNodes, SourceScope::AllNodes.to_string());
                                });

                            combo_box_response.response.on_hover_text("'self' only considers sources from the same node as target. 'all nodes' considers sources from any node.");
                        });
                    });

                    ui.add_space(10.0);

                    // Source Timing Strategy dropdown
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Timing:");
                            let timing_combo_response = ComboBox::new(ui.id().with("source_timing_strategy"), "")
                                .selected_text(self.source_timing_strategy.to_string())
                                .width(120.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.source_timing_strategy,
                                        SourceTimingStrategy::EarliestFirst,
                                        SourceTimingStrategy::EarliestFirst.to_string()
                                    );
                                    ui.selectable_value(
                                        &mut self.source_timing_strategy,
                                        SourceTimingStrategy::LatestFirst,
                                        SourceTimingStrategy::LatestFirst.to_string()
                                    );
                                });

                            if timing_combo_response.response.hovered() {
                                timing_combo_response.response.on_hover_text(
                                    "Determines which source spans are selected if multiple are available before the target: \
                                    'Earliest First' picks the oldest preceding source spans. \
                                    'Latest First' picks the most recent preceding source spans."
                                );
                            }
                        });
                    });

                    ui.add_space(20.0);

                    // Analyze button (right side)
                    ui.with_layout(Layout::right_to_left(eframe::emath::Align::Center), |ui| {
                        let is_ready = self.source_span_name.is_some() && self.target_span_name.is_some();
                        if ui
                            .add_enabled(
                                is_ready,
                                Button::new("Analyze").min_size(Vec2::new(100.0, 30.0)),
                            )
                            .clicked()
                        {
                            self.analyze_dependencies();
                        }
                    });
                });

                ui.add_space(10.0);

                // Display error message if any
                if let Some(error) = &self.error_message {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(220, 50, 50), error);
                    });
                }

                ui.separator();

                // Results area
                ui.label("Dependency Analysis Results:");

                // Initialize grid_width and col_percentages with defaults
                // These will be properly set if analysis_result is Some,
                // and the ScrollArea that uses them is only shown in that case.
                let mut grid_width = 0.0;
                // Define percentage-based column widths
                let col_percentages = [0.25, 0.14, 0.14, 0.14, 0.14, 0.14]; // Node, Count, Min, Max, Mean, Median

                if let Some(result) = &self.analysis_result {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Analysis of dependency: '{}' -> '{}' (threshold: {}, metadata: {}, scope: {}, timing: {})",
                            result.source_span_name,
                            result.target_span_name,
                            result.threshold,
                            if result.metadata_field.is_empty() { "none" } else { &result.metadata_field },
                            result.source_scope,
                            result.source_timing_strategy
                        ));

                        ui.label(format!("(Analysis took {} ms)", result.analysis_duration_ms));
                    });
                }

                // Create the grid headers outside the scroll area (to keep them visible)
                if self.analysis_result.is_some() {
                    ui.add_space(10.0);
                    grid_width = ui.available_width();

                    // Calculate pixel widths for columns based on percentages
                    let col_widths = calculate_table_column_widths(grid_width, &col_percentages);

                    // Header row - outside scrollable area to make it sticky
                    Grid::new("dependency_analysis_header_grid")
                        .num_columns(6)
                        .spacing([10.0, 6.0])
                        .striped(true)
                        .min_col_width(0.0)
                        .show(ui, |ui_grid| {
                            draw_left_aligned_text_cell(ui_grid, col_widths[0], "Node", true);
                            draw_right_aligned_text_cell(ui_grid, col_widths[1], "Count", true, None);
                            draw_right_aligned_text_cell(ui_grid, col_widths[2], "Min (ms)", true, None);
                            draw_right_aligned_text_cell(ui_grid, col_widths[3], "Max (ms)", true, None);
                            draw_right_aligned_text_cell(ui_grid, col_widths[4], "Mean (ms)", true, None);
                            draw_right_aligned_text_cell(ui_grid, col_widths[5], "Median (ms)", true, None);
                            ui_grid.end_row();
                        });

                    // Add a horizontal separator line
                    ui.separator();
                }

                // Grid contents in a scrollable area
                let results_height = if self.analysis_result.is_some() {
                    (max_height - 400.0).max(200.0)
                } else {
                    100.0
                };
                ScrollArea::vertical()
                    .max_height(results_height)
                    .id_salt("dependency_results_scroll_area")
                    .show_viewport(ui, |ui, _viewport| {
                        if let Some(result) = &self.analysis_result {
                            // Calculate column widths using the same grid width and percentages
                            let col_widths = calculate_table_column_widths(grid_width, &col_percentages);

                            Grid::new("dependency_analysis_grid")
                                .num_columns(6)
                                .spacing([10.0, 6.0])
                                .striped(true)
                                .min_col_width(0.0)
                                .show(ui, |ui| {
                                    // Get nodes and sort them alphabetically
                                    let mut node_names: Vec<String> =
                                        result.per_node_results.keys().cloned().collect();
                                    node_names.sort();

                                    // Rows for each node
                                    for node_name in node_names {
                                        if let Some(node_result) = result.per_node_results.get(&node_name) {
                                            let stats = &node_result.link_delay_statistics;

                                            // Node Name + Focus Button Column
                                            ui.scope(|ui_cell| {
                                                ui_cell.set_min_width(col_widths[0]);
                                                ui_cell.horizontal(|ui_horiz| {
                                                    ui_horiz.label(
                                                        RichText::new(&node_name).monospace(),
                                                    );
                                                    ui_horiz.add_space(5.0);
                                                    let focus_response = ui_horiz.button("🔍");
                                                    if focus_response.clicked() {
                                                        self.focus_node = Some(node_name.clone());
                                                        modal_closed = true;
                                                    }
                                                    if focus_response.hovered() {
                                                        focus_response.on_hover_text("Focus trace view on this node");
                                                    }
                                                });
                                            });

                                            // Stats columns
                                            if stats.count > 0 {
                                                // Count
                                                draw_right_aligned_text_cell(
                                                    ui,
                                                    col_widths[1],
                                                    &format!("{}", stats.count),
                                                    false,
                                                    None
                                                );
                                                // Min
                                                draw_right_aligned_text_cell(
                                                    ui,
                                                    col_widths[2],
                                                    &format!("{:.3}", stats.min * MILLISECONDS_PER_SECOND),
                                                    false,
                                                    Some(Color32::from_rgb(50, 150, 200))
                                                );
                                                // Max
                                                draw_right_aligned_text_cell(
                                                    ui,
                                                    col_widths[3],
                                                    &format!("{:.3}", stats.max * MILLISECONDS_PER_SECOND),
                                                    false,
                                                    Some(Color32::from_rgb(50, 150, 200))
                                                );
                                                // Mean
                                                draw_right_aligned_text_cell(
                                                    ui,
                                                    col_widths[4],
                                                    &format!("{:.3}", stats.mean() * MILLISECONDS_PER_SECOND),
                                                    false,
                                                    None
                                                );
                                                // Median
                                                draw_right_aligned_text_cell(
                                                    ui,
                                                    col_widths[5],
                                                    &format!("{:.3}", stats.median() * MILLISECONDS_PER_SECOND),
                                                    false,
                                                    None
                                                );
                                            } else {
                                                // No links for this node, display "-" for all stat columns (Count, Min, Max, Mean, Median)
                                                for width in col_widths.iter().skip(1) {
                                                    draw_right_aligned_text_cell(
                                                        ui,
                                                        *width,
                                                        "-",
                                                        false,
                                                        None
                                                    );
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    }

                                    // If no results found
                                    if result.per_node_results.is_empty() {
                                        draw_left_aligned_text_cell(ui, col_widths[0], "No matching dependencies found", false);
                                        for width in col_widths.iter().skip(1) {
                                             draw_right_aligned_text_cell(ui, *width, "", false, None);
                                        }
                                        ui.end_row();
                                    }
                                });
                        } else {
                            ui.label("Select source and target spans, then click 'Analyze' to see dependency statistics.");
                        }
                    });

                ui.separator();

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        modal_closed = true;
                    }
                });
            });
        });

        // Reset fields if modal got closed
        if modal_closed {
            self.show = false;
            self.spans_processed = false;
            self.source_span_name = None;
            self.target_span_name = None;
            self.source_search_text = String::new();
            self.target_search_text = String::new();
            self.threshold = 1;
            self.threshold_edit_str = self.threshold.to_string();
            self.metadata_field = String::new();
            self.source_scope = SourceScope::default();
            self.source_timing_strategy = SourceTimingStrategy::default();
            self.error_message = None;
        }
    }
}
