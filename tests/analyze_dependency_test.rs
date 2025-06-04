use approx::assert_abs_diff_eq;
use traviz::analyze_dependency::{AnalysisCardinality, AnalyzeDependencyModal, SourceScope};

mod test_helpers;
use test_helpers::{string_attr, ScenarioBuilder, SpanConfig, TestScenario, TimeInterval};

/// Tests basic dependency analysis between spans on the same node.
/// Verifies that a dependency link is correctly identified when a source span
/// ends before a target span starts, and delay calculation is accurate.
#[test]
fn test_basic_dependency_analysis() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    // Source span: 0.0 -> 1.0 (ends at 1.0)
    builder.add_span(SpanConfig::new(
        "task",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    // Target span: 2.0 -> 3.0 (starts at 2.0, delay = 1.0 second)
    builder.add_span(SpanConfig::new(
        "process",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("task".to_string()));
    modal.set_target_span_name(Some("process".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    assert_eq!(result.source_span_name, "task");
    assert_eq!(result.target_span_name, "process");
    assert_eq!(result.threshold, 1);

    assert!(result.per_node_results.contains_key("node_a"));
    let node_result = &result.per_node_results["node_a"];

    assert_eq!(node_result.links.len(), 1);
    assert_eq!(node_result.link_delay_statistics.count, 1);

    // Source span ends at 1.0, target starts at 2.0, so delay should be 1.0 seconds
    let link = &node_result.links[0];
    assert_eq!(link.delay_seconds, 1.0);

    assert_eq!(link.source_spans.len(), 1);
    assert_eq!(link.target_spans.len(), 1);
    assert_eq!(link.source_spans[0].name, "task");
    assert_eq!(link.target_spans[0].name, "process");
}

/// Tests cross-node dependency analysis with "AllNodes" scope.
/// Verifies that dependencies can be found between spans on different nodes
/// when the source scope allows it.
#[test]
fn test_cross_node_dependency_with_all_nodes_scope() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_node("node_b");
    // Source span on node_a: 0.0 -> 1.0 (ends at 1.0)
    builder.add_span(SpanConfig::new(
        "task",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    // Target span on node_b: 2.5 -> 3.5 (starts at 2.5, delay = 1.5 seconds)
    builder.add_span(SpanConfig::new(
        "process",
        "node_b",
        TimeInterval::with_duration(2.5, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("task".to_string()));
    modal.set_target_span_name(Some("process".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::AllNodes);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    assert!(result.per_node_results.contains_key("node_b"));
    let node_result = &result.per_node_results["node_b"];

    assert_eq!(node_result.links.len(), 1);

    // Source ends at 1.0, target starts at 2.5, delay = 1.5
    let link = &node_result.links[0];
    assert_eq!(link.delay_seconds, 1.5);

    assert_eq!(link.source_spans[0].node.name, "node_a");
    assert_eq!(link.target_spans[0].node.name, "node_b");
}

/// Tests cross-node scenario with "SameNode" scope restriction.
/// Verifies that cross-node dependencies are NOT found when the source scope
/// is restricted to the same node only.
#[test]
fn test_cross_node_dependency_with_same_node_scope() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_node("node_b");
    builder.add_span(SpanConfig::new(
        "task",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "process",
        "node_b",
        TimeInterval::with_duration(2.5, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("task".to_string()));
    modal.set_target_span_name(Some("process".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should complete");

    assert!(
        result.per_node_results.is_empty()
            || result
                .per_node_results
                .values()
                .all(|node_result| node_result.links.is_empty())
    );
}

/// Tests error handling when searching for non-existent span names.
/// Verifies that the analysis fails gracefully and provides a meaningful
/// error message when the source span name doesn't exist.
#[test]
fn test_no_matching_spans() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_span(SpanConfig::new(
        "task",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "process",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("nonexistent".to_string()));
    modal.set_target_span_name(Some("process".to_string()));
    modal.set_threshold(1);

    modal.analyze_dependencies();

    assert!(modal.analysis_result.is_none());
    assert!(modal.get_error_message().is_some());
    let error = modal.get_error_message().unwrap();
    assert!(error.contains("No spans found with name 'nonexistent'"));
}

/// Tests 1-to-N analysis cardinality with same node scope.
/// Verifies that one source span can link to multiple target spans on the same node.
/// Should find N target spans for each source span based on timing and threshold.
#[test]
fn test_one_to_n_cardinality_same_node() {
    let scenario = TestScenario::one_to_n_same_node();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::OneToN);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    assert!(result.per_node_results.contains_key("node_a"));
    let node_result = &result.per_node_results["node_a"];

    assert_eq!(node_result.links.len(), 1);
    let link = &node_result.links[0];

    // Should have 1 source and 2 target spans (threshold=2)
    assert_eq!(link.source_spans.len(), 1);
    assert_eq!(link.target_spans.len(), 2);
    assert_eq!(link.source_spans[0].name, "source");

    // Verify all targets are "target" spans
    for target_span in &link.target_spans {
        assert_eq!(target_span.name, "target");
    }
}

/// Tests 1-to-N analysis cardinality with all nodes scope.
/// Verifies that one source span can link to multiple target spans across different nodes.
/// Should find target spans from any node that start after the source ends.
#[test]
fn test_one_to_n_cardinality_all_nodes() {
    let scenario = TestScenario::one_to_n_cross_node();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::AllNodes);
    modal.set_analysis_cardinality(AnalysisCardinality::OneToN);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    // Should have results for the source node (node_1)
    assert!(result.per_node_results.contains_key("node_1"));
    let node_result = &result.per_node_results["node_1"];

    assert_eq!(node_result.links.len(), 1);
    let link = &node_result.links[0];

    // Should have 1 source and 2 target spans across different nodes
    assert_eq!(link.source_spans.len(), 1);
    assert_eq!(link.target_spans.len(), 2);
    assert_eq!(link.source_spans[0].node.name, "node_1");

    // Verify targets are on different nodes
    assert!(
        link.target_spans[0].node.name == "node_2" || link.target_spans[0].node.name == "node_3"
    );
    assert!(
        link.target_spans[1].node.name == "node_2" || link.target_spans[1].node.name == "node_3"
    );
    assert_ne!(
        link.target_spans[0].node.name,
        link.target_spans[1].node.name
    );
}

/// Tests 1-to-N analysis with insufficient target spans.
/// Verifies that no links are formed when there aren't enough target spans to meet threshold.
#[test]
fn test_one_to_n_insufficient_targets() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0),
    )); // Only 1 target
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(3); // Require 3 targets, but only 1 available
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::OneToN);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should complete");

    // Should have no links due to insufficient targets
    assert!(
        result.per_node_results.is_empty()
            || result
                .per_node_results
                .values()
                .all(|node_result| node_result.links.is_empty())
    );
}

/// Tests EarliestFirst timing strategy with multiple source spans.
/// Verifies that the earliest preceding source spans are selected when multiple options exist.
/// Should prefer older source spans over newer ones when forming links.
#[test]
fn test_earliest_first_timing_strategy() {
    let scenario = TestScenario::multiple_sources_timing();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_source_timing_strategy(
        traviz::analyze_dependency::SourceTimingStrategy::EarliestFirst,
    );

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    assert_eq!(link.source_spans.len(), 2);

    // Should select the 2 earliest ending spans (end times: 0.5, 0.8)
    // NOT the latest ending span (end time: 1.2)
    let end_times: Vec<f64> = link.source_spans.iter().map(|s| s.end_time).collect();
    assert!(end_times.contains(&0.5)); // earliest
    assert!(end_times.contains(&0.8)); // middle
    assert!(!end_times.contains(&1.2)); // latest (should be excluded)
}

/// Tests LatestFirst timing strategy with multiple source spans.
/// Verifies that the latest preceding source spans are selected when multiple options exist.
/// Should prefer newer source spans over older ones when forming links.
#[test]
fn test_latest_first_timing_strategy() {
    let scenario = TestScenario::multiple_sources_timing();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2); // Select 2 out of 3 available sources
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_source_timing_strategy(traviz::analyze_dependency::SourceTimingStrategy::LatestFirst);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    assert_eq!(link.source_spans.len(), 2);

    // Based on multiple_sources_timing scenario:
    // Source timings: 0.0->0.5 (ends 0.5), 0.2->0.8 (ends 0.8), 0.4->1.2 (ends 1.2)
    // LatestFirst with threshold=2 should select the 2 spans ending latest: 0.8 and 1.2
    let mut end_times: Vec<f64> = link.source_spans.iter().map(|s| s.end_time).collect();
    end_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Should have exactly the middle and latest end times
    assert_eq!(end_times.len(), 2);
    assert_abs_diff_eq!(end_times[0], 0.8); // middle timing
    assert_abs_diff_eq!(end_times[1], 1.2); // latest timing
}

/// Tests timing strategy with exactly threshold number of source spans.
/// Verifies behavior when available source spans exactly match the threshold requirement.
#[test]
fn test_timing_strategy_exact_threshold_match() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.5, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(3.0, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    assert_eq!(link.source_spans.len(), 2); // Should use all available sources
}

/// Tests dependency analysis with threshold = 2.
/// Verifies that at least 2 source spans are required to form a dependency link.
/// Should reject scenarios with only 1 source span available.
#[test]
fn test_threshold_two_sources_required() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    )); // Only 1 source
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2); // Require 2 sources, but only 1 available
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should complete");

    // Should have no links due to insufficient sources
    assert!(
        result.per_node_results.is_empty()
            || result
                .per_node_results
                .values()
                .all(|node_result| node_result.links.is_empty())
    );
}

/// Tests single attribute matching between source and target spans.
/// Verifies that only spans with matching attribute values can form links.
/// Should filter out spans that don't have matching attribute values.
#[test]
fn test_single_linking_attribute_matching() {
    let scenario = TestScenario::with_linking_attributes();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_linking_attribute("height".to_string()); // Match on height attribute

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    // Should only link spans with matching height="100"
    assert_eq!(
        link.source_spans[0].attributes["height"],
        string_attr("100")
    );
    assert_eq!(
        link.target_spans[0].attributes["height"],
        string_attr("100")
    );
}

/// Tests multiple attribute matching using comma-separated attribute names.
/// Verifies that spans must match ALL specified attributes to form links.
#[test]
fn test_multiple_linking_attributes_matching() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("height", "100")
            .with_string_attr("width", "200"),
    );
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.0, 1.0))
            .with_string_attr("height", "100")
            .with_string_attr("width", "200"), // Both attributes match
    );
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.5, 1.0))
            .with_string_attr("height", "100")
            .with_string_attr("width", "300"), // Only height matches
    );
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_linking_attribute("height,width".to_string());

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    // Should only link the target with both matching attributes
    assert_eq!(
        link.target_spans[0].attributes["height"],
        string_attr("100")
    );
    assert_eq!(link.target_spans[0].attributes["width"], string_attr("200"));
}

/// Tests linking attribute filtering with non-matching values.
/// Verifies that spans with different attribute values cannot form links.
/// Should exclude spans that don't have the required attribute match.
#[test]
fn test_linking_attribute_mismatch_filtering() {
    // TODO: Implement test where some spans have different attribute values
}

/// Tests linking attribute with missing attributes on some spans.
/// Verifies behavior when some spans lack the required linking attribute.
/// Should exclude spans that don't have the linking attribute defined.
#[test]
fn test_linking_attribute_missing_on_spans() {
    // TODO: Implement test where some spans lack the linking attribute
}

/// Tests empty linking attribute (no filtering).
/// Verifies that when no linking attribute is specified, all temporally valid spans can link.
/// Should ignore attribute matching entirely when linking attribute is empty.
#[test]
fn test_empty_linking_attribute_no_filtering() {
    // TODO: Implement test with no linking attribute restrictions
}

/// Tests source span grouping by attribute with FirstCompletedGroup strategy.
/// Verifies that source spans are grouped by attribute and threshold applied per group.
/// Should form links based on earliest completion time among groups.
#[test]
fn test_grouping_first_completed_group_strategy() {
    // TODO: Implement test with grouped sources and FirstCompletedGroup timing
}

/// Tests source span grouping by attribute with WaitForLastGroup strategy.
/// Verifies that link delay is calculated based on the latest group completion.
/// Should wait for all groups to complete before calculating delay.
#[test]
fn test_grouping_wait_for_last_group_strategy() {
    // TODO: Implement test with grouped sources and WaitForLastGroup timing
}

/// Tests grouping with insufficient spans in some groups.
/// Verifies that links are not formed when any group has fewer spans than threshold.
/// Should require all groups to meet the threshold requirement.
#[test]
fn test_grouping_insufficient_spans_per_group() {
    // TODO: Implement test where some groups don't meet threshold
}

/// Tests grouping with mixed group completions and 1-to-N cardinality.
/// Verifies that grouping works correctly with OneToN analysis mode.
/// Should group target spans instead of source spans in 1-to-N mode.
#[test]
fn test_grouping_one_to_n_cardinality() {
    // TODO: Implement test combining grouping with 1-to-N analysis
}

/// Tests grouping behavior when no spans have the group-by attribute.
/// Verifies error handling when the specified group-by attribute doesn't exist.
/// Should return appropriate error message about missing group attribute.
#[test]
fn test_grouping_missing_group_attribute() {
    // TODO: Implement test where group-by attribute is not found on any spans
}

/// Tests grouping with only one group present.
/// Verifies that grouping logic still works when all spans belong to the same group.
/// Should behave similarly to non-grouped analysis when only one group exists.
#[test]
fn test_grouping_single_group_only() {
    // TODO: Implement test where all spans have the same group attribute value
}

/// Tests complex scenario combining all features: grouping, linking attributes, cross-node, 1-to-N.
/// Verifies that all features work together correctly in a comprehensive scenario.
/// Should handle grouped cross-node 1-to-N dependencies with attribute matching.
#[test]
fn test_complex_all_features_combined() {
    // TODO: Implement comprehensive test combining all major features
}

// ============================================================================
// EDGE CASE AND ERROR HANDLING TESTS
// ============================================================================

/// Tests error handling when target span name doesn't exist.
/// Verifies appropriate error message when target spans cannot be found.
/// Should provide clear error message about missing target span name.
#[test]
fn test_error_missing_target_spans() {
    // TODO: Implement test with non-existent target span name
}

/// Tests behavior with overlapping source and target spans.
/// Verifies handling when source spans end after target spans start (invalid timing).
/// Should exclude temporally invalid span pairs from link formation.
#[test]
fn test_overlapping_spans_invalid_timing() {
    // TODO: Implement test with source spans that don't precede targets
}

/// Tests behavior with identical start/end times.
/// Verifies handling of spans with zero duration or simultaneous timing.
/// Should handle edge cases with identical timestamps appropriately.
#[test]
fn test_identical_timing_edge_cases() {
    // TODO: Implement test with spans having identical or zero-duration timing
}

/// Tests analysis with empty span list.
/// Verifies appropriate error handling when no spans are provided for analysis.
/// Should return clear error message about empty input data.
#[test]
fn test_empty_span_list_error() {
    // TODO: Implement test with no input spans
}

/// Tests analysis with spans from single node but AllNodes scope.
/// Verifies that AllNodes scope works correctly when all spans are on one node.
/// Should produce same results as SameNode scope in this scenario.
#[test]
fn test_all_nodes_scope_single_node_scenario() {
    // TODO: Implement test verifying AllNodes scope with single-node spans
}

/// Tests span reuse prevention in same-node mode.
/// Verifies that source spans used in one link cannot be reused for another link.
/// Should ensure each source span is used at most once per analysis.
#[test]
fn test_span_reuse_prevention_same_node() {
    // TODO: Implement test verifying source spans aren't reused in SameNode mode
}

/// Tests span reuse prevention in all-nodes mode.
/// Verifies correct span reuse behavior when analyzing across multiple nodes.
/// Should allow appropriate reuse patterns while preventing conflicts.
#[test]
fn test_span_reuse_prevention_all_nodes() {
    // TODO: Implement test verifying span reuse rules in AllNodes mode
}

// ============================================================================
// STATISTICS AND RESULTS TESTS
// ============================================================================

/// Tests statistical calculations (min, max, mean, median) for dependency delays.
/// Verifies that link delay statistics are calculated correctly.
/// Should produce accurate statistical measures for multiple dependency links.
#[test]
fn test_dependency_delay_statistics_accuracy() {
    let scenario = TestScenario::for_statistics_testing();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 5); // 5 dependency links

    let stats = &node_result.link_delay_statistics;
    assert_eq!(stats.count, 5);

    // Known delays: 0.5s, 1.0s, 1.5s, 2.0s, 3.0s
    assert_eq!(stats.min, 0.5); // minimum delay
    assert_eq!(stats.max, 3.0); // maximum delay

    // Mean = (0.5 + 1.0 + 1.5 + 2.0 + 3.0) / 5 = 8.0 / 5 = 1.6
    assert_abs_diff_eq!(stats.mean(), 1.6);

    // Median of [0.5, 1.0, 1.5, 2.0, 3.0] = 1.5
    assert_eq!(stats.median(), 1.5);

    // Verify min/max links are tracked correctly
    assert!(node_result.min_delay_link.is_some());
    assert!(node_result.max_delay_link.is_some());
    assert_abs_diff_eq!(
        node_result.min_delay_link.as_ref().unwrap().delay_seconds,
        0.5
    );
    assert_abs_diff_eq!(
        node_result.max_delay_link.as_ref().unwrap().delay_seconds,
        3.0
    );
}

/// Tests overall statistics aggregation across multiple nodes.
/// Verifies that per-node statistics are correctly aggregated into overall stats.
/// Should combine statistics from all nodes into accurate overall measures.
#[test]
fn test_overall_statistics_aggregation() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_node("node_b");

    // Node A: delays of 1.0s and 2.0s
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    )); // ends at 1.0
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(1.0, 1.0),
    )); // ends at 2.0
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0),
    )); // delay = 1.0s
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(4.0, 1.0),
    )); // delay = 2.0s

    // Node B: delays of 0.5s and 3.0s
    builder.add_span(SpanConfig::new(
        "source",
        "node_b",
        TimeInterval::with_duration(0.0, 1.0),
    )); // ends at 1.0
    builder.add_span(SpanConfig::new(
        "source",
        "node_b",
        TimeInterval::with_duration(2.0, 1.0),
    )); // ends at 3.0
    builder.add_span(SpanConfig::new(
        "target",
        "node_b",
        TimeInterval::with_duration(1.5, 1.0),
    )); // delay = 0.5s
    builder.add_span(SpanConfig::new(
        "target",
        "node_b",
        TimeInterval::with_duration(6.0, 1.0),
    )); // delay = 3.0s

    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    // Verify per-node results
    assert_eq!(result.per_node_results.len(), 2);
    assert!(result.per_node_results.contains_key("node_a"));
    assert!(result.per_node_results.contains_key("node_b"));

    // Verify overall statistics aggregation
    // Combined delays: [0.5, 1.0, 2.0, 3.0]
    let overall_stats = &result.overall_stats;
    assert_eq!(overall_stats.count, 4);
    assert_eq!(overall_stats.min, 0.5);
    assert_eq!(overall_stats.max, 3.0);

    // Mean = (0.5 + 1.0 + 2.0 + 3.0) / 4 = 6.5 / 4 = 1.625
    assert_abs_diff_eq!(overall_stats.mean(), 1.625);

    // Median of [0.5, 1.0, 2.0, 3.0] = (1.0 + 2.0) / 2 = 1.5
    assert_eq!(overall_stats.median(), 1.5);
}

/// Tests min/max delay link identification.
/// Verifies that the links with minimum and maximum delays are correctly identified.
/// Should track and report the specific links representing statistical extremes.
#[test]
fn test_min_max_delay_link_identification() {
    let scenario = TestScenario::for_statistics_testing();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];

    // Verify node-level min/max links
    let min_link = node_result.min_delay_link.as_ref().unwrap();
    let max_link = node_result.max_delay_link.as_ref().unwrap();

    assert_abs_diff_eq!(min_link.delay_seconds, 0.5);
    assert_abs_diff_eq!(max_link.delay_seconds, 3.0);

    // Verify these are actual dependency links with correct source/target
    assert_eq!(min_link.source_spans.len(), 1);
    assert_eq!(min_link.target_spans.len(), 1);
    assert_eq!(min_link.source_spans[0].name, "source");
    assert_eq!(min_link.target_spans[0].name, "target");

    // Verify overall min/max links
    let overall_min_link = result.overall_min_delay_link.as_ref().unwrap();
    let overall_max_link = result.overall_max_delay_link.as_ref().unwrap();

    assert_abs_diff_eq!(overall_min_link.delay_seconds, 0.5);
    assert_abs_diff_eq!(overall_max_link.delay_seconds, 3.0);
}
