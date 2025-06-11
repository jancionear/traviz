use crate::analyze_utils::{
    calculate_table_column_widths, collect_matching_spans, draw_clickable_right_aligned_text_cell,
    draw_left_aligned_text_cell, process_spans_for_analysis, span_search_ui,
    span_selection_list_ui, Statistics,
};
use crate::colors;
use crate::types::Span;
use crate::types::MILLISECONDS_PER_SECOND;
use eframe::egui::{
    self, Button, ComboBox, Grid, Id, Layout, Modal, RichText, ScrollArea, TextEdit, Ui, Vec2,
};
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

/// Structure to represent a dependency link between spans.
#[derive(Clone)]
pub struct DependencyLink {
    pub source_spans: Vec<Rc<Span>>,
    pub target_spans: Vec<Rc<Span>>,
    pub delay_seconds: f64,
}

/// Holds statistics and a list of identified dependency links where the target span resides on a specific node.
pub struct NodeDependencyMetrics {
    pub link_delay_statistics: Statistics,
    pub links: Vec<DependencyLink>,
    pub min_delay_link: Option<DependencyLink>,
    pub max_delay_link: Option<DependencyLink>,
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

/// Defines how the link delay is calculated when source spans are grouped.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub enum GroupAggregationStrategy {
    /// Link delay is based on the latest end time among all selected source spans from all groups.
    WaitForLastGroup,
    /// Link delay is based on the earliest end time among the latest selected source spans from each respective group.
    #[default]
    FirstCompletedGroup,
}

impl std::fmt::Display for GroupAggregationStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupAggregationStrategy::WaitForLastGroup => write!(f, "Wait For Last Group"),
            GroupAggregationStrategy::FirstCompletedGroup => write!(f, "First Completed Group"),
        }
    }
}

/// Defines the analysis cardinality mode.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub enum AnalysisCardinality {
    /// N-to-1: Find N source spans for each target span (existing mode).
    #[default]
    NToOne,
    /// 1-to-N: Find N target spans for each source span (new mode).
    OneToN,
}

impl std::fmt::Display for AnalysisCardinality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisCardinality::NToOne => write!(f, "N-to-1"),
            AnalysisCardinality::OneToN => write!(f, "1-to-N"),
        }
    }
}

pub struct DependencyAnalysisResult {
    pub source_span_name: String,
    pub target_span_name: String,
    pub threshold: usize,
    pub linking_attribute: String,
    pub source_scope: SourceScope,
    pub source_timing_strategy: SourceTimingStrategy,
    pub group_by_attribute: String,
    pub group_aggregation_strategy: GroupAggregationStrategy,
    pub analysis_cardinality: AnalysisCardinality,
    pub per_node_results: HashMap<String, NodeDependencyMetrics>,
    pub analysis_duration_ms: u128,
    pub overall_stats: Statistics,
    pub overall_min_delay_link: Option<DependencyLink>,
    pub overall_max_delay_link: Option<DependencyLink>,
}

/// Information needed to display the dependency link details popup.
pub struct LinkDetailsPopupInfo {
    pub link: DependencyLink,
    pub title: String,
    pub node_name: String,
    pub group_by_attribute_name: String,
    pub linking_attribute_name: String,
    pub delay_ms: f64,
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
    /// Optional attribute name used to match source and target spans for linking.
    linking_attribute: String,
    /// Scope for selecting source spans: "self" (same node as target) or "all nodes".
    source_scope: SourceScope,
    /// Strategy for selecting source spans (earliest or latest).
    source_timing_strategy: SourceTimingStrategy,
    /// Attribute to group source spans by.
    group_by_attribute: String,
    /// How to aggregate timings from multiple groups for link delay calculation.
    group_aggregation_strategy: GroupAggregationStrategy,
    /// Analysis cardinality mode: N-to-1 or 1-to-N.
    analysis_cardinality: AnalysisCardinality,
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
    /// If set, shows a popup with details of a specific dependency link.
    show_link_details_popup: Option<LinkDetailsPopupInfo>,
    /// Input for parsing analysis descriptions.
    description_input: String,
}

type PreparedAnalysisInput = (
    String,
    String,
    Vec<Rc<Span>>,
    Vec<Rc<Span>>,
    Option<HashSet<String>>,
);

type NodeSpanMap = HashMap<String, Vec<Rc<Span>>>;

impl AnalyzeDependencyModal {
    pub fn new() -> Self {
        let initial_threshold = 1;
        Self {
            threshold: initial_threshold,
            threshold_edit_str: initial_threshold.to_string(),
            group_aggregation_strategy: GroupAggregationStrategy::default(),
            description_input: String::new(),
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

    /// Test function: Sets the source span name for testing purposes.
    pub fn set_source_span_name(&mut self, name: Option<String>) {
        self.source_span_name = name;
    }

    /// Test function: Sets the target span name for testing purposes.
    pub fn set_target_span_name(&mut self, name: Option<String>) {
        self.target_span_name = name;
    }

    /// Test function: Sets the threshold for testing purposes.
    pub fn set_threshold(&mut self, threshold: usize) {
        self.threshold = threshold;
        self.threshold_edit_str = threshold.to_string();
    }

    /// Test function: Sets the source scope for testing purposes.
    pub fn set_source_scope(&mut self, scope: SourceScope) {
        self.source_scope = scope;
    }

    /// Test function: Sets the analysis cardinality for testing purposes.
    pub fn set_analysis_cardinality(&mut self, cardinality: AnalysisCardinality) {
        self.analysis_cardinality = cardinality;
    }

    /// Test function: Sets the linking attribute for testing purposes.
    pub fn set_linking_attribute(&mut self, attribute: String) {
        self.linking_attribute = attribute;
    }

    /// Test function: Sets the group by attribute for testing purposes.
    pub fn set_group_by_attribute(&mut self, attribute: String) {
        self.group_by_attribute = attribute;
    }

    /// Test function: Sets the source timing strategy for testing purposes.
    pub fn set_source_timing_strategy(&mut self, strategy: SourceTimingStrategy) {
        self.source_timing_strategy = strategy;
    }

    /// Test function: Sets the group aggregation strategy for testing purposes.
    pub fn set_group_aggregation_strategy(&mut self, strategy: GroupAggregationStrategy) {
        self.group_aggregation_strategy = strategy;
    }

    /// Test function: Gets the error message for testing purposes.
    pub fn get_error_message(&self) -> Option<&String> {
        self.error_message.as_ref()
    }

    /// Test function: Gets the source span name for testing purposes.
    pub fn get_source_span_name(&self) -> Option<&String> {
        self.source_span_name.as_ref()
    }

    /// Test function: Gets the target span name for testing purposes.
    pub fn get_target_span_name(&self) -> Option<&String> {
        self.target_span_name.as_ref()
    }

    /// Test function: Gets the analysis cardinality for testing purposes.
    pub fn get_analysis_cardinality(&self) -> &AnalysisCardinality {
        &self.analysis_cardinality
    }

    /// Test function: Gets the threshold for testing purposes.
    pub fn get_threshold(&self) -> usize {
        self.threshold
    }

    /// Test function: Gets the linking attribute for testing purposes.
    pub fn get_linking_attribute(&self) -> &String {
        &self.linking_attribute
    }

    /// Test function: Gets the group by attribute for testing purposes.
    pub fn get_group_by_attribute(&self) -> &String {
        &self.group_by_attribute
    }

    /// Test function: Gets the source scope for testing purposes.
    pub fn get_source_scope(&self) -> &SourceScope {
        &self.source_scope
    }

    /// Test function: Gets the source timing strategy for testing purposes.
    pub fn get_source_timing_strategy(&self) -> &SourceTimingStrategy {
        &self.source_timing_strategy
    }

    /// Test function: Gets the group aggregation strategy for testing purposes.
    pub fn get_group_aggregation_strategy(&self) -> &GroupAggregationStrategy {
        &self.group_aggregation_strategy
    }

    /// Test function: Gets the source search text for testing purposes.
    pub fn get_source_search_text(&self) -> &String {
        &self.source_search_text
    }

    /// Test function: Gets the target search text for testing purposes.
    pub fn get_target_search_text(&self) -> &String {
        &self.target_search_text
    }

    /// Selects a subset of source spans based on the configured timing strategy and threshold.
    fn select_spans_for_link_formation(&self, available_spans: &[Rc<Span>]) -> Vec<Rc<Span>> {
        assert!(self.threshold >= 1);
        let num_to_take = self.threshold;

        match self.source_timing_strategy {
            SourceTimingStrategy::EarliestFirst => {
                available_spans.iter().take(num_to_take).cloned().collect()
            }
            SourceTimingStrategy::LatestFirst => {
                let skip_count = available_spans.len().saturating_sub(num_to_take);
                available_spans.iter().skip(skip_count).cloned().collect()
            }
        }
    }

    /// Records a successfully formed dependency link, updates statistics, and tracks min/max delay links.
    #[allow(clippy::too_many_arguments)]
    fn record_formed_link(
        &self,
        stats: &mut Statistics,
        node_links: &mut Vec<DependencyLink>,
        min_link_for_node: &mut Option<DependencyLink>,
        max_link_for_node: &mut Option<DependencyLink>,
        formed_link: &DependencyLink,
        link_delay: f64,
        used_spans: &mut HashSet<Vec<u8>>,
        span_id: &[u8],
    ) {
        stats.add_value(link_delay);
        node_links.push(formed_link.clone());
        used_spans.insert(span_id.to_vec());

        // Update min/max links
        // stats.count, stats.min, stats.max are updated by stats.add_value()
        if stats.count == 1 {
            // This means it's the first link added to these stats
            *min_link_for_node = Some(formed_link.clone());
            *max_link_for_node = Some(formed_link.clone());
        } else {
            if link_delay == stats.min {
                *min_link_for_node = Some(formed_link.clone());
            }
            // A link can be both min and max if it's the only one, or if multiple links share the same min/max delay.
            if link_delay == stats.max {
                *max_link_for_node = Some(formed_link.clone());
            }
        }
    }

    /// Marks the source spans of a formed link as used according to the source scope.
    fn mark_source_spans_used(
        &self,
        source_spans_in_link: &[Rc<Span>],
        global_used_source_span_ids_for_self_mode: &mut HashSet<Vec<u8>>,
        used_source_ids_for_current_node_all_scope: &mut HashSet<Vec<u8>>,
    ) {
        for linked_s_span in source_spans_in_link {
            if self.source_scope == SourceScope::SameNode {
                global_used_source_span_ids_for_self_mode.insert(linked_s_span.span_id.clone());
            } else {
                used_source_ids_for_current_node_all_scope.insert(linked_s_span.span_id.clone());
            }
        }
    }

    /// Checks if two spans have matching values for all specified linking attributes.
    fn spans_match_linking_attributes(
        &self,
        source_span: &Rc<Span>,
        target_span: &Rc<Span>,
    ) -> bool {
        if self.linking_attribute.is_empty() {
            return true;
        }

        let attribute_names: Vec<&str> = self
            .linking_attribute
            .split(',')
            .map(|s| s.trim())
            .collect();

        for attr_name in attribute_names {
            if attr_name.is_empty() {
                continue;
            }

            // Both spans must have the attribute
            if !source_span.attributes.contains_key(attr_name)
                || !target_span.attributes.contains_key(attr_name)
            {
                return false;
            }

            // Values must match
            let source_value = &source_span.attributes[attr_name];
            let target_value = &target_span.attributes[attr_name];
            if source_value != target_value {
                return false;
            }
        }

        true
    }

    pub fn analyze_dependencies(&mut self) {
        self.analysis_result = None;

        let analysis_start = Instant::now();

        let (source_name, target_name, source_spans, target_spans, expected_group_keys_set) =
            match self.prepare_analysis_inputs() {
                Ok(inputs) => inputs,
                Err(msg) => {
                    self.error_message = Some(msg);
                    return;
                }
            };

        let (source_spans_by_node, target_spans_by_node) =
            group_spans_by_node(&source_spans, &target_spans);

        // Per-node dependency analysis
        let mut per_node_results = HashMap::new();

        match self.analysis_cardinality {
            AnalysisCardinality::NToOne => {
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
                let mut global_used_source_span_ids_for_self_mode: HashSet<Vec<u8>> =
                    HashSet::new();

                for node_name_str in node_names {
                    let current_target_spans_for_node = target_spans_by_node
                        .get(&node_name_str)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);

                    if current_target_spans_for_node.is_empty() {
                        continue;
                    }

                    if let Some(metrics) = self.analyze_dependencies_for_single_node_n_to_one(
                        &node_name_str,
                        &source_spans_by_node,
                        current_target_spans_for_node,
                        &mut global_used_source_span_ids_for_self_mode,
                        &expected_group_keys_set,
                    ) {
                        per_node_results.insert(node_name_str.clone(), metrics);
                    }
                }
            }
            AnalysisCardinality::OneToN => {
                let node_names = if self.source_scope == SourceScope::SameNode {
                    // Only analyze nodes that have both source and target spans
                    source_spans_by_node
                        .keys()
                        .filter(|node_name| target_spans_by_node.contains_key(*node_name))
                        .cloned()
                        .collect::<Vec<String>>()
                } else {
                    // "all nodes" - analyze all source nodes
                    source_spans_by_node.keys().cloned().collect()
                };

                // For 1-to-N: each source span can link to multiple targets
                let mut global_used_target_span_ids_for_self_mode: HashSet<Vec<u8>> =
                    HashSet::new();

                for node_name_str in node_names {
                    let current_source_spans_for_node = source_spans_by_node
                        .get(&node_name_str)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);

                    if current_source_spans_for_node.is_empty() {
                        continue;
                    }

                    if let Some(metrics) = self.analyze_dependencies_for_single_node_one_to_n(
                        &node_name_str,
                        current_source_spans_for_node,
                        &target_spans_by_node,
                        &mut global_used_target_span_ids_for_self_mode,
                        &expected_group_keys_set,
                    ) {
                        per_node_results.insert(node_name_str.clone(), metrics);
                    }
                }
            }
        }

        // Measure analysis duration
        let analysis_duration = analysis_start.elapsed().as_millis();

        // Store the results
        self.analysis_result = Some(DependencyAnalysisResult {
            source_span_name: source_name,
            target_span_name: target_name,
            threshold: self.threshold,
            linking_attribute: self.linking_attribute.clone(),
            source_scope: self.source_scope.clone(),
            source_timing_strategy: self.source_timing_strategy.clone(),
            group_by_attribute: self.group_by_attribute.clone(),
            group_aggregation_strategy: self.group_aggregation_strategy.clone(),
            analysis_cardinality: self.analysis_cardinality.clone(),
            per_node_results,
            analysis_duration_ms: analysis_duration,
            overall_stats: Statistics::new(),
            overall_min_delay_link: None,
            overall_max_delay_link: None,
        });

        // Calculate overall statistics if there are results
        if let Some(res) = &mut self.analysis_result {
            if !res.per_node_results.is_empty() {
                let mut temp_overall_stats = Statistics::new();
                let mut temp_overall_min_link: Option<DependencyLink> = None;
                let mut temp_overall_max_link: Option<DependencyLink> = None;

                for node_metrics in res.per_node_results.values() {
                    for link in &node_metrics.links {
                        let current_delay = link.delay_seconds;
                        let is_first_overall_value = temp_overall_stats.count == 0;

                        // Update .min, .max, .count
                        temp_overall_stats.add_value(current_delay);

                        if is_first_overall_value {
                            temp_overall_min_link = Some(link.clone());
                            temp_overall_max_link = Some(link.clone());
                        } else {
                            if current_delay == temp_overall_stats.min {
                                temp_overall_min_link = Some(link.clone());
                            }
                            if current_delay == temp_overall_stats.max {
                                temp_overall_max_link = Some(link.clone());
                            }
                        }
                    }
                }
                res.overall_stats = temp_overall_stats;
                res.overall_min_delay_link = temp_overall_min_link;
                res.overall_max_delay_link = temp_overall_max_link;
            }
        }

        self.error_message = None;
    }

    /// Analyzes dependencies for a single node.
    fn analyze_dependencies_for_single_node_n_to_one(
        &mut self,
        node_name: &str,
        source_spans_by_node: &HashMap<String, Vec<Rc<Span>>>,
        current_target_node_spans: &[Rc<Span>],
        global_used_source_span_ids_for_self_mode: &mut HashSet<Vec<u8>>,
        expected_group_keys_set: &Option<HashSet<String>>,
    ) -> Option<NodeDependencyMetrics> {
        let current_source_node_spans = if self.source_scope == SourceScope::SameNode {
            // Only use spans from this node
            source_spans_by_node
                .get(node_name)
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
            return None;
        }

        // Find valid links for the current node
        let mut node_links_for_current_node = Vec::new();
        let mut stats_for_current_node = Statistics::new();
        let mut min_link_for_current_node: Option<DependencyLink> = None;
        let mut max_link_for_current_node: Option<DependencyLink> = None;

        let mut used_target_spans: HashSet<Vec<u8>> = HashSet::new();
        // This is specific to the current node when in "all nodes" scope, ensuring sources are not reused for different targets *on this same node* within this call.
        let mut used_source_ids_for_current_node_all_scope: HashSet<Vec<u8>> = HashSet::new();

        for target_span_rc in current_target_node_spans.iter() {
            if used_target_spans.contains(&target_span_rc.span_id) {
                // This target has already been linked by a source group
                continue;
            }

            self.process_target_span_for_links(
                target_span_rc, // Pass as &Rc<Span>
                &current_source_node_spans,
                expected_group_keys_set,
                global_used_source_span_ids_for_self_mode,
                &mut used_source_ids_for_current_node_all_scope, // Pass as mutable ref
                &mut node_links_for_current_node,
                &mut stats_for_current_node,
                &mut min_link_for_current_node,
                &mut max_link_for_current_node,
                &mut used_target_spans,
            );
        }

        // Add result for this node if any links were formed
        if !node_links_for_current_node.is_empty() || stats_for_current_node.count > 0 {
            Some(NodeDependencyMetrics {
                link_delay_statistics: stats_for_current_node,
                links: node_links_for_current_node,
                min_delay_link: min_link_for_current_node,
                max_delay_link: max_link_for_current_node,
            })
        } else {
            None
        }
    }

    /// Processes a single target span to find and record dependency links.
    #[allow(clippy::too_many_arguments)]
    fn process_target_span_for_links(
        &self,
        target_span: &Rc<Span>,
        current_source_node_spans: &[Rc<Span>],
        expected_group_keys_set: &Option<HashSet<String>>,
        global_used_source_span_ids_for_self_mode: &mut HashSet<Vec<u8>>,
        used_source_ids_for_current_node_all_scope: &mut HashSet<Vec<u8>>,
        node_links_for_current_node: &mut Vec<DependencyLink>,
        stats_for_current_node: &mut Statistics,
        min_link_for_current_node: &mut Option<DependencyLink>,
        max_link_for_current_node: &mut Option<DependencyLink>,
        used_target_spans: &mut HashSet<Vec<u8>>,
    ) {
        // Common logic to find all temporally valid, attribute-matching, and not-yet-used source spans
        let mut eligible_sources_before_target: Vec<Rc<Span>> = Vec::new();
        for s_span in current_source_node_spans.iter() {
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
            if s_span.end_time <= target_span.start_time {
                // Check linking attribute compatibility using the new multi-attribute function
                if self.spans_match_linking_attributes(s_span, target_span) {
                    eligible_sources_before_target.push(s_span.clone());
                }
            }
        }

        // Branch based on grouping
        if !self.group_by_attribute.is_empty() && expected_group_keys_set.is_some() {
            // GROUPING LOGIC
            let mut grouped_potential_sources: HashMap<String, Vec<Rc<Span>>> = HashMap::new();
            for s_span in &eligible_sources_before_target {
                if let Some(Some(Value::StringValue(s_val))) =
                    s_span.attributes.get(&self.group_by_attribute)
                {
                    grouped_potential_sources
                        .entry(s_val.clone())
                        .or_default()
                        .push(s_span.clone());
                }
            }

            let mut all_groups_meet_threshold = true;
            let mut spans_for_this_grouped_link: Vec<Rc<Span>> = Vec::new();

            // Check if each group that exists in eligible sources meets threshold
            // (Don't require all originally expected groups - some may be filtered out by linking attributes)
            for group_spans in grouped_potential_sources.values() {
                if group_spans.len() >= self.threshold && self.threshold > 0 {
                    let selected_from_group = self.select_spans_for_link_formation(group_spans);
                    spans_for_this_grouped_link.extend(selected_from_group);
                } else {
                    all_groups_meet_threshold = false;
                    break;
                }
            }

            // Only proceed if we have at least some groups and all present groups meet threshold
            if all_groups_meet_threshold
                && !grouped_potential_sources.is_empty()
                && !spans_for_this_grouped_link.is_empty()
            {
                // Calculate link delay based on aggregation strategy for groups
                let link_delay = match self.group_aggregation_strategy {
                    GroupAggregationStrategy::WaitForLastGroup => {
                        spans_for_this_grouped_link
                            .iter()
                            .map(|s| s.end_time)
                            .fold(f64::NEG_INFINITY, f64::max)
                            - target_span.start_time
                    }
                    GroupAggregationStrategy::FirstCompletedGroup => {
                        let mut latest_end_time_per_group: HashMap<String, f64> = HashMap::new();
                        for s_span in &spans_for_this_grouped_link {
                            if let Some(Some(Value::StringValue(group_key))) =
                                s_span.attributes.get(&self.group_by_attribute)
                            {
                                latest_end_time_per_group
                                    .entry(group_key.clone())
                                    .and_modify(|e| *e = e.max(s_span.end_time))
                                    .or_insert(s_span.end_time);
                            }
                        }
                        latest_end_time_per_group
                            .values()
                            .fold(f64::INFINITY, |a, &b| a.min(b))
                            - target_span.start_time
                    }
                }
                .abs();

                let new_formed_link = DependencyLink {
                    source_spans: spans_for_this_grouped_link.clone(),
                    target_spans: vec![target_span.clone()],
                    delay_seconds: link_delay,
                };

                self.record_formed_link(
                    stats_for_current_node,
                    node_links_for_current_node,
                    min_link_for_current_node,
                    max_link_for_current_node,
                    &new_formed_link,
                    link_delay,
                    used_target_spans,
                    &target_span.span_id,
                );

                self.mark_source_spans_used(
                    &spans_for_this_grouped_link,
                    global_used_source_span_ids_for_self_mode,
                    used_source_ids_for_current_node_all_scope,
                );
            }
        } else {
            // NON-GROUPING LOGIC
            if eligible_sources_before_target.len() >= self.threshold && self.threshold > 0 {
                let selected_source_spans_group =
                    self.select_spans_for_link_formation(&eligible_sources_before_target);

                if !selected_source_spans_group.is_empty() {
                    let latest_end_time_of_selected_sources = selected_source_spans_group
                        .iter()
                        .map(|s| s.end_time)
                        .fold(f64::NEG_INFINITY, f64::max);

                    let link_distance =
                        (target_span.start_time - latest_end_time_of_selected_sources).abs();

                    let new_formed_link = DependencyLink {
                        source_spans: selected_source_spans_group.clone(),
                        target_spans: vec![target_span.clone()],
                        delay_seconds: link_distance,
                    };

                    self.record_formed_link(
                        stats_for_current_node,
                        node_links_for_current_node,
                        min_link_for_current_node,
                        max_link_for_current_node,
                        &new_formed_link,
                        link_distance,
                        used_target_spans,
                        &target_span.span_id,
                    );

                    self.mark_source_spans_used(
                        &selected_source_spans_group,
                        global_used_source_span_ids_for_self_mode,
                        used_source_ids_for_current_node_all_scope,
                    );
                }
            }
        }
    }

    /// Analyzes dependencies for a single node in 1-to-N mode.
    fn analyze_dependencies_for_single_node_one_to_n(
        &mut self,
        node_name: &str,
        current_source_node_spans: &[Rc<Span>],
        target_spans_by_node: &HashMap<String, Vec<Rc<Span>>>,
        global_used_target_span_ids_for_self_mode: &mut HashSet<Vec<u8>>,
        expected_group_keys_set: &Option<HashSet<String>>,
    ) -> Option<NodeDependencyMetrics> {
        // Get all potential target spans based on source scope
        let current_target_spans = if self.source_scope == SourceScope::SameNode {
            // Only use targets from this same node
            target_spans_by_node
                .get(node_name)
                .cloned()
                .unwrap_or_default()
        } else {
            // Use targets from all nodes, sorted by time
            let mut all_t_spans: Vec<Rc<Span>> =
                target_spans_by_node.values().flatten().cloned().collect();
            all_t_spans.sort_by(|a, b| {
                a.start_time
                    .partial_cmp(&b.start_time)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            all_t_spans
        };

        // Skip if no target spans for this scope
        if current_target_spans.is_empty() {
            return None;
        }

        // Find valid links for the current source node
        let mut node_links_for_current_node = Vec::new();
        let mut stats_for_current_node = Statistics::new();
        let mut min_link_for_current_node: Option<DependencyLink> = None;
        let mut max_link_for_current_node: Option<DependencyLink> = None;

        let mut used_source_spans: HashSet<Vec<u8>> = HashSet::new();
        // This is specific to the current node when in "all nodes" scope
        let mut used_target_ids_for_current_node_all_scope: HashSet<Vec<u8>> = HashSet::new();

        for source_span_rc in current_source_node_spans.iter() {
            if used_source_spans.contains(&source_span_rc.span_id) {
                // This source has already been processed
                continue;
            }

            self.process_source_span_for_one_to_n_links(
                source_span_rc,
                &current_target_spans,
                expected_group_keys_set,
                global_used_target_span_ids_for_self_mode,
                &mut used_target_ids_for_current_node_all_scope,
                &mut node_links_for_current_node,
                &mut stats_for_current_node,
                &mut min_link_for_current_node,
                &mut max_link_for_current_node,
                &mut used_source_spans,
            );
        }

        // Add result for this node if any links were formed
        if !node_links_for_current_node.is_empty() || stats_for_current_node.count > 0 {
            Some(NodeDependencyMetrics {
                link_delay_statistics: stats_for_current_node,
                links: node_links_for_current_node,
                min_delay_link: min_link_for_current_node,
                max_delay_link: max_link_for_current_node,
            })
        } else {
            None
        }
    }

    /// Processes a single source span to find and record dependency links in 1-to-N mode.
    #[allow(clippy::too_many_arguments)]
    fn process_source_span_for_one_to_n_links(
        &self,
        source_span: &Rc<Span>,
        current_target_spans: &[Rc<Span>],
        expected_group_keys_set: &Option<HashSet<String>>,
        global_used_target_span_ids_for_self_mode: &mut HashSet<Vec<u8>>,
        used_target_ids_for_current_node_all_scope: &mut HashSet<Vec<u8>>,
        node_links_for_current_node: &mut Vec<DependencyLink>,
        stats_for_current_node: &mut Statistics,
        min_link_for_current_node: &mut Option<DependencyLink>,
        max_link_for_current_node: &mut Option<DependencyLink>,
        used_source_spans: &mut HashSet<Vec<u8>>,
    ) {
        // Common logic to find all temporally valid, attribute-matching, and not-yet-used target spans
        let mut eligible_targets_after_source: Vec<Rc<Span>> = Vec::new();
        for t_span in current_target_spans.iter() {
            let mut skip_target = false;
            if self.source_scope == SourceScope::SameNode {
                if global_used_target_span_ids_for_self_mode.contains(&t_span.span_id) {
                    skip_target = true;
                }
            } else {
                // "all nodes" mode
                if used_target_ids_for_current_node_all_scope.contains(&t_span.span_id) {
                    skip_target = true;
                }
            }
            if skip_target {
                continue;
            }

            // Basic time validity: target must start after source ends
            if t_span.start_time >= source_span.end_time
                && self.spans_match_linking_attributes(source_span, t_span)
            {
                eligible_targets_after_source.push(t_span.clone());
            }
        }

        // Branch based on grouping
        if !self.group_by_attribute.is_empty() && expected_group_keys_set.is_some() {
            // GROUPING LOGIC
            let mut grouped_potential_targets: HashMap<String, Vec<Rc<Span>>> = HashMap::new();
            for t_span in &eligible_targets_after_source {
                if let Some(Some(Value::StringValue(t_val))) =
                    t_span.attributes.get(&self.group_by_attribute)
                {
                    grouped_potential_targets
                        .entry(t_val.clone())
                        .or_default()
                        .push(t_span.clone());
                }
            }

            let mut all_groups_meet_threshold = true;
            let mut targets_for_this_grouped_link: Vec<Rc<Span>> = Vec::new();

            // Check if each group that exists in eligible targets meets threshold
            // (Don't require all originally expected groups - some may be filtered out by linking attributes)
            for group_targets in grouped_potential_targets.values() {
                if group_targets.len() >= self.threshold && self.threshold > 0 {
                    let selected_from_group =
                        self.select_targets_for_one_to_n_link_formation(group_targets);
                    targets_for_this_grouped_link.extend(selected_from_group);
                } else {
                    all_groups_meet_threshold = false;
                    break;
                }
            }

            // Only proceed if we have at least some groups and all present groups meet threshold
            if all_groups_meet_threshold
                && !grouped_potential_targets.is_empty()
                && !targets_for_this_grouped_link.is_empty()
            {
                // Create a single link with one source and multiple targets
                // For consistency with N-to-1 mode, always use the LATEST target start time
                // (representing "how long until ALL targets have started")
                let link_delay = (targets_for_this_grouped_link
                    .iter()
                    .map(|t| t.start_time)
                    .fold(f64::NEG_INFINITY, f64::max)
                    - source_span.end_time)
                    .abs();

                let new_formed_link = DependencyLink {
                    source_spans: vec![source_span.clone()],
                    target_spans: targets_for_this_grouped_link.clone(),
                    delay_seconds: link_delay,
                };

                self.record_formed_link(
                    stats_for_current_node,
                    node_links_for_current_node,
                    min_link_for_current_node,
                    max_link_for_current_node,
                    &new_formed_link,
                    link_delay,
                    used_source_spans,
                    &source_span.span_id,
                );

                // Mark all targets as used
                for target_span in &targets_for_this_grouped_link {
                    if self.source_scope == SourceScope::SameNode {
                        global_used_target_span_ids_for_self_mode
                            .insert(target_span.span_id.clone());
                    } else {
                        used_target_ids_for_current_node_all_scope
                            .insert(target_span.span_id.clone());
                    }
                }
            }
        } else {
            // NON-GROUPING LOGIC
            if eligible_targets_after_source.len() >= self.threshold && self.threshold > 0 {
                let selected_target_spans_group =
                    self.select_targets_for_one_to_n_link_formation(&eligible_targets_after_source);

                if !selected_target_spans_group.is_empty() {
                    // Create a single link with one source and multiple targets
                    // For consistency with N-to-1 mode, always use the LATEST target start time
                    // (representing "how long until ALL targets have started")
                    let link_delay = (selected_target_spans_group
                        .iter()
                        .map(|t| t.start_time)
                        .fold(f64::NEG_INFINITY, f64::max)
                        - source_span.end_time)
                        .abs();

                    let new_formed_link = DependencyLink {
                        source_spans: vec![source_span.clone()],
                        target_spans: selected_target_spans_group.clone(),
                        delay_seconds: link_delay,
                    };

                    self.record_formed_link(
                        stats_for_current_node,
                        node_links_for_current_node,
                        min_link_for_current_node,
                        max_link_for_current_node,
                        &new_formed_link,
                        link_delay,
                        used_source_spans,
                        &source_span.span_id,
                    );

                    // Mark all targets as used
                    for target_span in &selected_target_spans_group {
                        if self.source_scope == SourceScope::SameNode {
                            global_used_target_span_ids_for_self_mode
                                .insert(target_span.span_id.clone());
                        } else {
                            used_target_ids_for_current_node_all_scope
                                .insert(target_span.span_id.clone());
                        }
                    }
                }
            }
        }
    }

    /// Selects a subset of target spans based on the configured timing strategy and threshold for 1-to-N analysis.
    fn select_targets_for_one_to_n_link_formation(
        &self,
        available_targets: &[Rc<Span>],
    ) -> Vec<Rc<Span>> {
        assert!(self.threshold >= 1);
        let num_to_take = self.threshold;

        match self.source_timing_strategy {
            SourceTimingStrategy::EarliestFirst => available_targets
                .iter()
                .take(num_to_take)
                .cloned()
                .collect(),
            SourceTimingStrategy::LatestFirst => {
                let skip_count = available_targets.len().saturating_sub(num_to_take);
                available_targets.iter().skip(skip_count).cloned().collect()
            }
        }
    }

    /// Validates inputs and prepares initial span lists for dependency analysis.
    /// Returns a tuple of (source_name, target_name, source_spans, target_spans, expected_group_keys_set)
    /// or an error message string if validation fails.
    fn prepare_analysis_inputs(&mut self) -> Result<PreparedAnalysisInput, String> {
        // Validate source and target span names
        let source_name = match &self.source_span_name {
            Some(name) => name.clone(),
            None => return Err("Source span not selected".to_string()),
        };
        let target_name = match &self.target_span_name {
            Some(name) => name.clone(),
            None => return Err("Target span not selected".to_string()),
        };

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
            return Err(format!("No spans found with name \'{}\'", source_name));
        }

        if target_spans.is_empty() {
            return Err(format!("No spans found with name \'{}\'", target_name));
        }

        // Determine expected_group_keys_set if grouping is active
        let mut expected_group_keys_set: Option<HashSet<String>> = None;
        if !self.group_by_attribute.is_empty() {
            let mut keys_found = HashSet::new();

            // Choose which spans to check based on cardinality mode
            let (spans_to_check, span_type_name) = match self.analysis_cardinality {
                AnalysisCardinality::NToOne => (&source_spans, &source_name),
                AnalysisCardinality::OneToN => (&target_spans, &target_name),
            };

            for span in spans_to_check {
                if let Some(Some(Value::StringValue(s_val))) =
                    span.attributes.get(&self.group_by_attribute)
                {
                    keys_found.insert(s_val.clone());
                }
            }

            if keys_found.is_empty() {
                let error_message = match self.analysis_cardinality {
                    AnalysisCardinality::NToOne => {
                        format!(
                            "The \'Group By Attribute\' (\'{}\') was not found in any source spans named \'{}\', or no such source spans have this attribute.",
                            self.group_by_attribute, span_type_name
                        )
                    }
                    AnalysisCardinality::OneToN => {
                        format!(
                            "The \'Group By Attribute\' (\'{}\') was not found in any target spans named \'{}\', or no such target spans have this attribute.",
                            self.group_by_attribute, span_type_name
                        )
                    }
                };
                return Err(error_message);
            }
            expected_group_keys_set = Some(keys_found);
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
        Ok((
            source_name,
            target_name,
            source_spans,
            target_spans,
            expected_group_keys_set,
        ))
    }

    // Show the modal
    pub fn show_modal(&mut self, ctx: &egui::Context, max_width: f32, max_height: f32) {
        if !self.show {
            return;
        }

        let mut modal_closed = false;

        Modal::new("analyze dependency".into()).show(ctx, |ui_modal_area| {
            ui_modal_area.vertical(|ui_main_column| {
                ui_main_column.set_max_width(max_width);
                ui_main_column.set_max_height(max_height);

                ui_main_column.heading("Analyze Dependency");
                ui_main_column.add_space(10.0);

                // Quick setup section for parsing analysis descriptions
                self.show_quick_setup_parsing_ui(ui_main_column);

                ui_main_column.add_space(10.0);

                Grid::new("source_target_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .striped(true)
                    .show(ui_main_column, |ui_grid_for_search| {
                        ui_grid_for_search.vertical(|ui| {
                            ui.set_width(max_width * 0.45);
                            span_search_ui(
                                ui,
                                &mut self.source_search_text,
                                "Source Span:",
                                "Search source span",
                                ui.available_width()
                            );
                        });
                        ui_grid_for_search.vertical(|ui| {
                            ui.set_width(max_width * 0.45);
                            span_search_ui(
                                ui,
                                &mut self.target_search_text,
                                "Target Span:",
                                "Search target span",
                                ui.available_width()
                            );
                        });
                        ui_grid_for_search.end_row();
                        let list_height = 150.0;
                        ui_grid_for_search.vertical(|ui| {
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
                        ui_grid_for_search.vertical(|ui| {
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
                        ui_grid_for_search.end_row();
                    });

                ui_main_column.add_space(10.0);
                ui_main_column.separator();
                ui_main_column.add_space(10.0);

                ui_main_column.vertical(|ui_config_rows_container| {
                    ui_config_rows_container.horizontal(|ui_row1| {
                        ui_row1.vertical(|ui| {
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
                                        self.threshold = value.max(1);
                                    }
                                    self.threshold_edit_str = self.threshold.to_string();
                                }
                                if response.hovered() {
                                    response.on_hover_text(concat!(
                                        "Minimum number of source spans required to form a link. ",
                                        "This count of spans (per group, if grouping is active) will be selected ",
                                        "based on the chosen timing strategy."
                                    ));
                                }
                            });
                        });
                        ui_row1.add_space(10.0);
                        ui_row1.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Cardinality:");
                                let card_resp = ComboBox::new(ui.id().with("analysis_cardinality"), "")
                                    .selected_text(self.analysis_cardinality.to_string())
                                    .width(80.0)
                                    .show_ui(ui, |ui_combo_card| {
                                        ui_combo_card.selectable_value(&mut self.analysis_cardinality, AnalysisCardinality::NToOne, AnalysisCardinality::NToOne.to_string());
                                        ui_combo_card.selectable_value(&mut self.analysis_cardinality, AnalysisCardinality::OneToN, AnalysisCardinality::OneToN.to_string());
                                    });
                                card_resp.response.on_hover_text(concat!(
                                    "N-to-1: Find N source spans for each target span (existing mode). ",
                                    "1-to-N: Find N target spans for each source span (new mode)."
                                ));
                            });
                        });
                        ui_row1.add_space(10.0);
                        ui_row1.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Link by Attribute:");
                                let response = ui.add(
                                    TextEdit::singleline(&mut self.linking_attribute)
                                        .desired_width(100.0)
                                        .hint_text("field name")
                                );
                                if response.hovered() {
                                    response.on_hover_text(concat!(
                                        "Optional. If provided, only spans with matching values for this ",
                                        "attribute field can form links. Leave empty to ignore attribute matching. ",
                                        "Use comma-separated names (e.g., 'height,shard_id') to match multiple attributes."
                                    ));
                                }
                            });
                        });
                        ui_row1.add_space(10.0);
                        ui_row1.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Group By Attribute:");
                                let group_by_response = ui.add(
                                    TextEdit::singleline(&mut self.group_by_attribute)
                                        .desired_width(100.0)
                                        .hint_text("field name")
                                );
                                if group_by_response.hovered() {
                                    group_by_response.on_hover_text(concat!(
                                        "Optional. If provided, source spans will be grouped by this attribute, ",
                                        "and the threshold will be applied per group. Leave empty to disable grouping."
                                    ));
                                }
                            });
                        });
                    });

                    ui_config_rows_container.add_space(8.0);

                    ui_config_rows_container.horizontal(|ui_row2| {
                        ui_row2.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Source Scope:");
                                let resp = ComboBox::new(ui.id().with("source_scope"), "")
                                    .selected_text(self.source_scope.to_string())
                                    .width(80.0)
                                    .show_ui(ui, |ui_combo_scope| {
                                        ui_combo_scope.selectable_value(&mut self.source_scope, SourceScope::SameNode, SourceScope::SameNode.to_string());
                                        ui_combo_scope.selectable_value(&mut self.source_scope, SourceScope::AllNodes, SourceScope::AllNodes.to_string());
                                    });
                                resp.response.on_hover_text(concat!(
                                    "'self' only considers sources from the same node as target. ",
                                    "'all nodes' considers sources from any node."
                                ));
                            });
                        });
                        ui_row2.add_space(10.0);
                        ui_row2.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Source Timing:");
                                let resp = ComboBox::new(ui.id().with("source_timing_strategy"), "")
                                    .selected_text(self.source_timing_strategy.to_string())
                                    .width(120.0)
                                    .show_ui(ui, |ui_combo_timing| {
                                        ui_combo_timing.selectable_value(&mut self.source_timing_strategy, SourceTimingStrategy::EarliestFirst, SourceTimingStrategy::EarliestFirst.to_string());
                                        ui_combo_timing.selectable_value(&mut self.source_timing_strategy, SourceTimingStrategy::LatestFirst, SourceTimingStrategy::LatestFirst.to_string());
                                    });
                                if resp.response.hovered() {
                                    resp.response.on_hover_text(concat!(
                                        "Determines which source spans are selected if multiple are available ",
                                        "before the target: 'Earliest First' picks the oldest preceding source spans. ",
                                        "'Latest First' picks the most recent preceding source spans."
                                    ));
                                }
                            });
                        });
                        ui_row2.add_space(10.0);
                        ui_row2.vertical(|ui| {
                            let is_grouping_active = !self.group_by_attribute.is_empty();
                            ui.add_enabled_ui(is_grouping_active, |ui_enabled_agg| {
                                ui_enabled_agg.horizontal(|ui_agg_horiz| {
                                    ui_agg_horiz.label("Group Aggregation:");
                                    let agg_resp = ComboBox::new(ui_agg_horiz.id().with("group_aggregation_strategy"), "")
                                        .selected_text(self.group_aggregation_strategy.to_string())
                                        .width(160.0)
                                        .show_ui(ui_agg_horiz, |ui_combo_agg| {
                                            ui_combo_agg.selectable_value(&mut self.group_aggregation_strategy, GroupAggregationStrategy::WaitForLastGroup, GroupAggregationStrategy::WaitForLastGroup.to_string());
                                            ui_combo_agg.selectable_value(&mut self.group_aggregation_strategy, GroupAggregationStrategy::FirstCompletedGroup, GroupAggregationStrategy::FirstCompletedGroup.to_string());
                                        });
                                    if agg_resp.response.hovered() {
                                        let hover_text = if is_grouping_active {
                                            match self.group_aggregation_strategy {
                                                GroupAggregationStrategy::WaitForLastGroup =>
                                                    concat!("Link delay is based on the latest end time among all selected source spans ",
                                                            "from all groups."), 
                                                GroupAggregationStrategy::FirstCompletedGroup =>
                                                    concat!("Link delay is based on the earliest end time among the latest selected ",
                                                            "source spans from each respective group."),
                                            }
                                        } else { "Only applicable when 'Group By Attribute' is used." };
                                        agg_resp.response.on_hover_text(hover_text);
                                    }
                                });
                            });
                        });
                        ui_row2.add_space(20.0);
                        ui_row2.with_layout(Layout::right_to_left(eframe::emath::Align::Center), |ui_analyze_button_area| {
                            if ui_analyze_button_area.add_enabled(self.source_span_name.is_some() && self.target_span_name.is_some(), Button::new("Analyze").min_size(Vec2::new(100.0, 30.0))).clicked() {
                                self.analyze_dependencies();
                            }
                        });
                    });
                });

                ui_main_column.add_space(10.0);
                if let Some(error) = &self.error_message {
                    ui_main_column.horizontal(|ui_err_msg| {
                        ui_err_msg.colored_label(colors::MILD_RED, error);
                    });
                }
                ui_main_column.separator();
                ui_main_column.label("Dependency Analysis Results:");

                let mut grid_width = 0.0;
                let col_percentages = [0.25, 0.11, 0.11, 0.11, 0.11, 0.11, 0.11];

                if let Some(result) = &self.analysis_result {
                    ui_main_column.horizontal_wrapped(|ui_summary_wrap| {
                        ui_summary_wrap.label(format!(
                            "Analysis of dependency: '{}' -> '{}' (cardinality: {}, threshold: {}, linking by: {}, group by: {}, scope: {}, timing: {}, group aggregation: {})",
                            result.source_span_name, result.target_span_name, result.analysis_cardinality, result.threshold,
                            if result.linking_attribute.is_empty() { "none" } else { &result.linking_attribute },
                            if result.group_by_attribute.is_empty() { "none" } else { &result.group_by_attribute },
                            result.source_scope, result.source_timing_strategy, result.group_aggregation_strategy
                        ));
                        ui_summary_wrap.label(format!("(Analysis took {} ms)", result.analysis_duration_ms));
                    });
                }

                if self.analysis_result.is_some() {
                    ui_main_column.add_space(10.0);
                    grid_width = ui_main_column.available_width();
                    let col_widths = calculate_table_column_widths(grid_width, &col_percentages);
                    Grid::new("dependency_analysis_header_grid")
                        .num_columns(7)
                        .spacing([10.0, 6.0])
                        .striped(true)
                        .min_col_width(0.0)
                        .show(ui_main_column, |ui_header_grid| {
                            let node_header = match self.analysis_result.as_ref().map(|r| &r.analysis_cardinality) {
                                Some(AnalysisCardinality::OneToN) => "Source Node",
                                _ => "Node",
                            };
                            draw_left_aligned_text_cell(ui_header_grid, col_widths[0], node_header, true);
                            draw_clickable_right_aligned_text_cell(ui_header_grid, col_widths[1], "Count", true, None, false);
                            draw_clickable_right_aligned_text_cell(ui_header_grid, col_widths[2], "Min (ms)", true, None, false);
                            draw_clickable_right_aligned_text_cell(ui_header_grid, col_widths[3], "Max (ms)", true, None, false);
                            draw_clickable_right_aligned_text_cell(ui_header_grid, col_widths[4], "Mean (ms)", true, None, false);
                            draw_clickable_right_aligned_text_cell(ui_header_grid, col_widths[5], "Median (ms)", true, None, false);
                            draw_clickable_right_aligned_text_cell(ui_header_grid, col_widths[6], "Std Dev (ms)", true, None, false);
                            ui_header_grid.end_row();
                        });
                    ui_main_column.separator();
                }

                let results_height = if self.analysis_result.is_some() { (max_height - 340.0).max(230.0) } else { 115.0 };
                ScrollArea::vertical()
                    .max_height(results_height)
                    .id_salt("dependency_results_scroll_area")
                    .show_viewport(ui_main_column, |ui_scroll_content, _viewport| {
                        if let Some(result) = &self.analysis_result {
                            let col_widths = calculate_table_column_widths(grid_width, &col_percentages);
                            Grid::new("dependency_analysis_grid")
                                .num_columns(7)
                                .spacing([10.0, 6.0])
                                .striped(true)
                                .min_col_width(0.0)
                                .show(ui_scroll_content, |ui_data_grid| {
                                    let mut node_names: Vec<String> = result.per_node_results.keys().cloned().collect();
                                    node_names.sort();
                                    for node_name in node_names {
                                        if let Some(node_result) = result.per_node_results.get(&node_name) {
                                            let stats = &node_result.link_delay_statistics;
                                            ui_data_grid.scope(|ui_cell| {
                                                ui_cell.set_min_width(col_widths[0]);
                                                ui_cell.horizontal(|ui_horiz| {
                                                    ui_horiz.label(RichText::new(&node_name).monospace());
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
                                            if stats.count > 0 {
                                                draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[1], &format!("{}", stats.count), false, None, false);
                                                let min_val_str = format!("{:.3}", stats.min * MILLISECONDS_PER_SECOND);
                                                if let Some(resp) = draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[2], &min_val_str, false, Some(colors::MILD_BLUE2), node_result.min_delay_link.is_some()) {
                                                    if resp.clicked() {
                                                        if let Some(link) = &node_result.min_delay_link {
                                                            self.show_link_details_popup = Some(LinkDetailsPopupInfo {
                                                                link: link.clone(),
                                                                title: format!("Minimum Delay Link Details ({})", node_name),
                                                                node_name: node_name.clone(),
                                                                group_by_attribute_name: result.group_by_attribute.clone(),
                                                                linking_attribute_name: result.linking_attribute.clone(),
                                                                delay_ms: stats.min * MILLISECONDS_PER_SECOND,
                                                            });
                                                        }
                                                    }
                                                }
                                                let max_val_str = format!("{:.3}", stats.max * MILLISECONDS_PER_SECOND);
                                                if let Some(resp) = draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[3], &max_val_str, false, Some(colors::MILD_BLUE2), node_result.max_delay_link.is_some()) {
                                                    if resp.clicked() {
                                                        if let Some(link) = &node_result.max_delay_link {
                                                            self.show_link_details_popup = Some(LinkDetailsPopupInfo {
                                                                link: link.clone(),
                                                                title: format!("Maximum Delay Link Details ({})", node_name),
                                                                node_name: node_name.clone(),
                                                                group_by_attribute_name: result.group_by_attribute.clone(),
                                                                linking_attribute_name: result.linking_attribute.clone(),
                                                                delay_ms: stats.max * MILLISECONDS_PER_SECOND,
                                                            });
                                                        }
                                                    }
                                                }
                                                draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[4], &format!("{:.3}", stats.mean() * MILLISECONDS_PER_SECOND), false, None, false);
                                                draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[5], &format!("{:.3}", stats.median() * MILLISECONDS_PER_SECOND), false, None, false);
                                                draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[6], &format!("{:.3}", stats.std_dev() * MILLISECONDS_PER_SECOND), false, None, false);
                                            } else {
                                                for &col_width_val in col_widths.iter().skip(1) {
                                                    draw_clickable_right_aligned_text_cell(ui_data_grid, col_width_val, "-", false, None, false);
                                                }
                                            }
                                            ui_data_grid.end_row();
                                        }
                                    }
                                    if result.per_node_results.is_empty() {
                                        draw_left_aligned_text_cell(ui_data_grid, col_widths[0], "No matching dependencies found", false);
                                        for &col_width_val in col_widths.iter().skip(1) {
                                            draw_clickable_right_aligned_text_cell(ui_data_grid, col_width_val, "", false, None, false);
                                        }
                                        ui_data_grid.end_row();
                                    }

                                    // Overall statistics row
                                    if result.overall_stats.count > 0 {
                                        let overall_label_text = RichText::new("All Nodes").strong();
                                        ui_data_grid.scope(|ui_cell| {
                                            ui_cell.set_min_width(col_widths[0]);
                                            ui_cell.label(overall_label_text);
                                        });

                                        draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[1], &format!("{}", result.overall_stats.count), true, None, false);

                                        let min_val_str = format!("{:.3}", result.overall_stats.min * MILLISECONDS_PER_SECOND);
                                        if let Some(resp) = draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[2], &min_val_str, true, Some(colors::MILD_BLUE2), result.overall_min_delay_link.is_some()) {
                                            if resp.clicked() {
                                                if let Some(link) = &result.overall_min_delay_link {
                                                    self.show_link_details_popup = Some(LinkDetailsPopupInfo {
                                                        link: link.clone(),
                                                        title: format!("Overall Minimum Delay Link Details (Node: {})", link.target_spans[0].node.name),
                                                        node_name: link.target_spans[0].node.name.clone(),
                                                        group_by_attribute_name: result.group_by_attribute.clone(),
                                                        linking_attribute_name: result.linking_attribute.clone(),
                                                        delay_ms: result.overall_stats.min * MILLISECONDS_PER_SECOND,
                                                    });
                                                }
                                            }
                                        }

                                        let max_val_str = format!("{:.3}", result.overall_stats.max * MILLISECONDS_PER_SECOND);
                                        if let Some(resp) = draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[3], &max_val_str, true, Some(colors::MILD_BLUE2), result.overall_max_delay_link.is_some()) {
                                            if resp.clicked() {
                                                if let Some(link) = &result.overall_max_delay_link {
                                                    self.show_link_details_popup = Some(LinkDetailsPopupInfo {
                                                        link: link.clone(),
                                                        title: format!("Overall Maximum Delay Link Details (Node: {})", link.target_spans[0].node.name),
                                                        node_name: link.target_spans[0].node.name.clone(),
                                                        group_by_attribute_name: result.group_by_attribute.clone(),
                                                        linking_attribute_name: result.linking_attribute.clone(),
                                                        delay_ms: result.overall_stats.max * MILLISECONDS_PER_SECOND,
                                                    });
                                                }
                                            }
                                        }
                                        draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[4], &format!("{:.3}", result.overall_stats.mean() * MILLISECONDS_PER_SECOND), true, None, false);
                                        draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[5], &format!("{:.3}", result.overall_stats.median() * MILLISECONDS_PER_SECOND), true, None, false);
                                        draw_clickable_right_aligned_text_cell(ui_data_grid, col_widths[6], &format!("{:.3}", result.overall_stats.std_dev() * MILLISECONDS_PER_SECOND), true, None, false);
                                        ui_data_grid.end_row();
                                    }
                                });
                        } else {
                            ui_scroll_content.label("Select source and target spans, then click 'Analyze' to see dependency statistics.");
                        }
                    });

                ui_main_column.separator();
                ui_main_column.add_space(10.0);
                ui_main_column.horizontal(|ui_close_button_row| {
                    if ui_close_button_row.button("Close").clicked() {
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
            self.linking_attribute = String::new();
            self.group_by_attribute = String::new();
            self.source_scope = SourceScope::default();
            self.source_timing_strategy = SourceTimingStrategy::default();
            self.group_aggregation_strategy = GroupAggregationStrategy::default();
            self.analysis_cardinality = AnalysisCardinality::default();
            self.error_message = None;
            self.description_input = String::new();
        }

        // Show the link details popup if requested
        self.show_dependency_link_details_modal_ui(ctx, max_width * 0.92, max_height * 0.8);
    }

    // Method to show the dependency link details modal
    fn show_dependency_link_details_modal_ui(
        &mut self,
        ctx: &egui::Context,
        max_width: f32,
        max_height: f32,
    ) {
        let mut close_requested = false;

        if let Some(details_info) = &self.show_link_details_popup {
            let popup_id = Id::new(&details_info.title);

            Modal::new(popup_id).show(ctx, |ui| {
                ui.set_max_width(max_width);
                ui.set_max_height(max_height);
                ui.set_min_size(Vec2::new(max_width * 0.5, max_height * 0.5));

                ui.heading(&details_info.title);
                ui.add_space(5.0);
                ui.label(format!("Node: {}", details_info.node_name));
                ui.label(format!("Delay: {:.3} ms", details_info.delay_ms));
                if !details_info.linking_attribute_name.is_empty() {
                    ui.label(format!(
                        "Linked by attribute: {}",
                        details_info.linking_attribute_name
                    ));
                }
                if !details_info.group_by_attribute_name.is_empty() {
                    ui.label(format!(
                        "Grouped by attribute: {}",
                        details_info.group_by_attribute_name
                    ));
                }
                ui.separator();

                draw_link_visualization_ui_impl(ui, details_info);

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                if ui.button("Close").clicked() {
                    close_requested = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close_requested = true;
                }
            });
        }

        if close_requested {
            self.show_link_details_popup = None;
        }
    }

    /// EXPERIMENTAL FEATURE: Quick setup from analysis description parsing
    ///
    /// Parse an analysis description string and populate the modal fields
    pub fn parse_and_fill_from_description(&mut self, description: &str) -> Result<(), String> {
        // Clear any previous parse error
        self.error_message = None;

        // Trim whitespace
        let desc = description.trim();

        // Check if it starts with the expected prefix
        if !desc.starts_with("Analysis of dependency:") {
            return Err("Description must start with 'Analysis of dependency:'".to_string());
        }

        // Parse source and target span names
        let after_prefix = desc.strip_prefix("Analysis of dependency:").unwrap().trim();

        // Find the arrow pattern 'source' -> 'target'
        let arrow_regex = regex::Regex::new(r"'([^']+)'\s*->\s*'([^']+)'").unwrap();
        let arrow_captures = arrow_regex
            .captures(after_prefix)
            .ok_or("Could not find 'source' -> 'target' pattern in quotes")?;
        let source_name = arrow_captures.get(1).unwrap().as_str().to_string();
        let target_name = arrow_captures.get(2).unwrap().as_str().to_string();

        // Extract the parameters part (everything in parentheses)
        let params_start = after_prefix
            .find('(')
            .ok_or("Could not find opening parenthesis for parameters")?;
        let params_end = after_prefix
            .rfind(')')
            .ok_or("Could not find closing parenthesis for parameters")?;
        let params_str = &after_prefix[params_start + 1..params_end];

        // Parse individual parameters
        let mut cardinality = None;
        let mut threshold = None;
        let mut linking_by = None;
        let mut group_by = None;
        let mut scope = None;
        let mut timing = None;
        let mut group_aggregation = None;

        // Define the expected parameter names in order
        let param_names = [
            "cardinality:",
            "threshold:",
            "linking by:",
            "group by:",
            "scope:",
            "timing:",
            "group aggregation:",
        ];

        // Parse parameters by finding each parameter name and taking everything until the next parameter name
        let mut remaining = params_str;

        for (i, &param_name) in param_names.iter().enumerate() {
            if let Some(start_pos) = remaining.find(param_name) {
                // Move past the parameter name
                let value_start = start_pos + param_name.len();
                let value_part = &remaining[value_start..];

                // Find the next parameter name to determine where this value ends
                let mut value_end = value_part.len();
                for &next_param_name in &param_names[i + 1..] {
                    if let Some(next_pos) = value_part.find(next_param_name) {
                        value_end = value_end.min(next_pos);
                    }
                }

                // Extract and trim the value
                let value = value_part[..value_end].trim().trim_end_matches(',').trim();

                // Parse the specific parameter
                match param_name {
                    "cardinality:" => {
                        cardinality = Some(match value {
                            "N-to-1" => AnalysisCardinality::NToOne,
                            "1-to-N" => AnalysisCardinality::OneToN,
                            _ => return Err(format!("Unknown cardinality: {}", value)),
                        });
                    }
                    "threshold:" => {
                        threshold = Some(
                            value
                                .parse::<usize>()
                                .map_err(|_| format!("Invalid threshold: {}", value))?,
                        );
                    }
                    "linking by:" => {
                        linking_by = Some(if value == "none" {
                            String::new()
                        } else {
                            value.to_string()
                        });
                    }
                    "group by:" => {
                        group_by = Some(if value == "none" {
                            String::new()
                        } else {
                            value.to_string()
                        });
                    }
                    "scope:" => {
                        scope = Some(match value {
                            "self" => SourceScope::SameNode,
                            "all nodes" => SourceScope::AllNodes,
                            _ => return Err(format!("Unknown scope: {}", value)),
                        });
                    }
                    "timing:" => {
                        timing = Some(match value {
                            "Earliest First" => SourceTimingStrategy::EarliestFirst,
                            "Latest First" => SourceTimingStrategy::LatestFirst,
                            _ => return Err(format!("Unknown timing strategy: {}", value)),
                        });
                    }
                    "group aggregation:" => {
                        group_aggregation = Some(match value {
                            "Wait For Last Group" => GroupAggregationStrategy::WaitForLastGroup,
                            "First Completed Group" => {
                                GroupAggregationStrategy::FirstCompletedGroup
                            }
                            _ => {
                                return Err(format!(
                                    "Unknown group aggregation strategy: {}",
                                    value
                                ))
                            }
                        });
                    }
                    _ => {}
                }

                // Move the remaining string forward to avoid processing the same parameter again
                remaining = &remaining[value_start + value_end..];
            }
        }

        // Apply the parsed values
        self.source_span_name = Some(source_name);
        self.target_span_name = Some(target_name);

        if let Some(card) = cardinality {
            self.analysis_cardinality = card;
        }

        if let Some(thresh) = threshold {
            self.threshold = thresh.max(1);
            self.threshold_edit_str = self.threshold.to_string();
        }

        if let Some(linking) = linking_by {
            self.linking_attribute = linking;
        }

        if let Some(grouping) = group_by {
            self.group_by_attribute = grouping;
        }

        if let Some(sc) = scope {
            self.source_scope = sc;
        }

        if let Some(tim) = timing {
            self.source_timing_strategy = tim;
        }

        if let Some(agg) = group_aggregation {
            self.group_aggregation_strategy = agg;
        }

        // Update search text to match the selected spans
        if let Some(ref source) = self.source_span_name {
            self.source_search_text = source.clone();
        }
        if let Some(ref target) = self.target_span_name {
            self.target_search_text = target.clone();
        }

        Ok(())
    }

    /// EXPERIMENTAL FEATURE: Quick setup from analysis description parsing
    fn show_quick_setup_parsing_ui(&mut self, ui_main_column: &mut Ui) {
        ui_main_column.collapsing("Quick Setup from Analysis Description", |ui_quick_setup| {
            ui_quick_setup.label("Paste an analysis description to automatically fill all fields:");
            ui_quick_setup.add_space(5.0);

            ui_quick_setup.horizontal(|ui_input_row| {
                ui_input_row.add(
                    TextEdit::multiline(&mut self.description_input)
                        .desired_width(ui_input_row.available_width() - 120.0)
                        .desired_rows(3)
                        .hint_text("Analysis of dependency: 'source_span' -> 'target_span' (cardinality: 1-to-N, threshold: 1, linking by: none, group by: none, scope: all nodes, timing: Earliest First, group aggregation: First Completed Group)")
                );

                ui_input_row.vertical(|ui_button_col| {
                    if ui_button_col.button("Parse, Fill and Analyze").clicked() {
                        if let Err(err) = self.parse_and_fill_from_description(&self.description_input.clone()) {
                            self.error_message = Some(format!("Parse error: {}", err));
                        } else {
                            // Clear parse errors but keep other error messages
                            if let Some(ref msg) = self.error_message {
                                if msg.starts_with("Parse error:") {
                                    self.error_message = None;
                                }
                            }
                            // After successful parsing, run the analysis
                            self.analyze_dependencies();
                        }
                    }
                });
            });

            if let Some(ref error) = self.error_message {
                if error.starts_with("Parse error:") {
                    ui_quick_setup.add_space(5.0);
                    ui_quick_setup.colored_label(colors::MILD_RED, error);
                }
            }
        });
    }
}

/// Draws the visualization for a dependency link, including source and target spans.
fn draw_link_visualization_ui_impl(ui: &mut Ui, details: &LinkDetailsPopupInfo) {
    let mut sorted_source_spans = details.link.source_spans.clone();
    // Sort by end time primarily, then by name as a secondary criterion for stable sort
    sorted_source_spans.sort_by(|a, b| {
        a.end_time
            .partial_cmp(&b.end_time)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });

    // For calculating time to target, use the earliest target start time
    let earliest_target_start_time = details
        .link
        .target_spans
        .iter()
        .map(|t| t.start_time)
        .fold(f64::INFINITY, f64::min);

    ScrollArea::vertical()
        .id_salt("link_details_scroll_area")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.strong("Source Spans:");
            ui.add_space(2.0);

            if !details.group_by_attribute_name.is_empty() {
                let mut grouped_sources_map: BTreeMap<String, Vec<Rc<Span>>> = BTreeMap::new();
                let ungrouped_key = "(Ungrouped/N/A)".to_string();

                for s_span in &sorted_source_spans {
                    if let Some(Some(Value::StringValue(group_val))) =
                        s_span.attributes.get(&details.group_by_attribute_name)
                    {
                        grouped_sources_map
                            .entry(group_val.clone())
                            .or_default()
                            .push(s_span.clone());
                    } else {
                        grouped_sources_map
                            .entry(ungrouped_key.clone())
                            .or_default()
                            .push(s_span.clone());
                    }
                }

                let mut sorted_groups: Vec<(String, Vec<Rc<Span>>, f64)> = Vec::new();
                for (group_name, spans_in_group) in grouped_sources_map {
                    let latest_end_time_in_group = spans_in_group
                        .iter()
                        .map(|s| s.end_time)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let time_to_target_group_ms = (earliest_target_start_time
                        - latest_end_time_in_group)
                        * MILLISECONDS_PER_SECOND;
                    sorted_groups.push((group_name, spans_in_group, time_to_target_group_ms));
                }

                sorted_groups
                    .sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

                for (group_name, spans_in_group, time_to_target_group_ms) in sorted_groups {
                    ui.horizontal(|ui| {
                        ui.strong(format!(
                            "  Group ({}): {}",
                            details.group_by_attribute_name, group_name
                        ));
                        if time_to_target_group_ms.is_finite() {
                            ui.strong(format!(
                                " (Time to Target: {:.3}ms)",
                                time_to_target_group_ms.max(0.0)
                            ));
                        }
                    });

                    for s_span in spans_in_group {
                        ui.indent("source_span_indent_group", |ui| {
                            ui.horizontal(|ui| {
                                let distance_to_target_ms = (earliest_target_start_time
                                    - s_span.end_time)
                                    * MILLISECONDS_PER_SECOND;
                                let time_to_target_display =
                                    format!("{:.3}ms", distance_to_target_ms.max(0.0));
                                let end_timestamp_str =
                                    crate::types::time_point_to_utc_string(s_span.end_time);

                                ui.strong("Time to Target: ");
                                let ttt_label_response = ui.monospace(time_to_target_display);
                                ttt_label_response
                                    .on_hover_text(format!("End: {}", end_timestamp_str));

                                ui.strong(" Node: ");
                                ui.monospace(&s_span.node.name);
                                ui.strong(" Name: ");
                                ui.monospace(&s_span.name);
                                ui.strong(" ID: ");
                                ui.monospace(hex::encode(&s_span.span_id));
                            });
                        });
                        ui.add_space(1.0);
                    }
                    ui.add_space(4.0);
                }
            } else {
                for (idx, s_span) in sorted_source_spans.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}. ", idx + 1));

                        let distance_to_target_ms = (earliest_target_start_time - s_span.end_time)
                            * MILLISECONDS_PER_SECOND;
                        let time_to_target_display =
                            format!("{:.3}ms", distance_to_target_ms.max(0.0));
                        let end_timestamp_str =
                            crate::types::time_point_to_utc_string(s_span.end_time);

                        ui.strong("Time to Target: ");
                        let ttt_label_response = ui.monospace(time_to_target_display);
                        ttt_label_response.on_hover_text(format!("End: {}", end_timestamp_str));

                        ui.strong(" Node: ");
                        ui.monospace(&s_span.node.name);
                        ui.strong(" Name: ");
                        ui.monospace(&s_span.name);
                        ui.strong(" ID: ");
                        ui.monospace(hex::encode(&s_span.span_id));
                    });
                    ui.add_space(1.0);
                }
            }

            ui.add_space(8.0);
            ui.strong("Target Spans:");
            ui.add_space(2.0);
            let t_spans = &details.link.target_spans;
            for (idx, t_span) in t_spans.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}. ", idx + 1));
                    ui.strong("Start: ");
                    ui.monospace(crate::types::time_point_to_utc_string(t_span.start_time));
                    ui.strong(" Node: ");
                    ui.monospace(&t_span.node.name);
                    ui.strong(" Name: ");
                    ui.monospace(&t_span.name);
                    ui.strong(" ID: ");
                    ui.monospace(hex::encode(&t_span.span_id));
                });
                ui.add_space(1.0);
            }
        });
}

fn group_spans_by_node(
    source_spans_list: &[Rc<Span>],
    target_spans_list: &[Rc<Span>],
) -> (NodeSpanMap, NodeSpanMap) {
    let mut source_spans_by_node: NodeSpanMap = HashMap::new();
    let mut target_spans_by_node: NodeSpanMap = HashMap::new();

    for span in source_spans_list {
        source_spans_by_node
            .entry(span.node.name.clone())
            .or_default()
            .push(span.clone());
    }

    for span in target_spans_list {
        target_spans_by_node
            .entry(span.node.name.clone())
            .or_default()
            .push(span.clone());
    }
    (source_spans_by_node, target_spans_by_node)
}
