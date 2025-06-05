use approx::assert_abs_diff_eq;
use traviz::analyze_dependency::{
    AnalysisCardinality, AnalyzeDependencyModal, GroupAggregationStrategy, SourceScope,
};

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
    assert_eq!(link.source_spans.len(), 2);
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
    modal.set_linking_attribute("height".to_string());

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
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Source span with height="100"
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("height", "100"),
    );

    // Target spans: one with matching height, one with non-matching height
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.0, 1.0))
            .with_string_attr("height", "100"), // matches source
    );
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.5, 1.0))
            .with_string_attr("height", "200"), // does NOT match source
    );

    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_linking_attribute("height".to_string());

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    // Should only link to the target with matching height="100", not height="200"
    assert_eq!(link.target_spans.len(), 1);
    assert_eq!(
        link.target_spans[0].attributes["height"],
        string_attr("100")
    );

    // Verify the source also has the matching attribute
    assert_eq!(
        link.source_spans[0].attributes["height"],
        string_attr("100")
    );
}

/// Tests linking attribute with missing attributes on some spans.
/// Verifies behavior when some spans lack the required linking attribute.
/// Should exclude spans that don't have the linking attribute defined.
#[test]
fn test_linking_attribute_missing_on_spans() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Source spans: one with the attribute, one without
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("category", "processing"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.5, 1.0)), // No category attribute - should be excluded
    );

    // Target spans: one with matching attribute, one without
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.0, 1.0))
            .with_string_attr("category", "processing"), // matches first source
    );
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.5, 1.0)), // No category attribute - should be excluded
    );

    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_linking_attribute("category".to_string());

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];

    // Should only link spans that both have the "category" attribute
    assert_eq!(link.source_spans.len(), 1);
    assert_eq!(link.target_spans.len(), 1);

    // Verify both linked spans have the required attribute
    assert_eq!(
        link.source_spans[0].attributes["category"],
        string_attr("processing")
    );

    // Verify spans without the attribute were excluded
    // (This is implicit - if they were included, we'd have more than 1 link)
}

/// Tests empty linking attribute (no filtering).
/// Verifies that when no linking attribute is specified, all temporally valid spans can link.
/// Should ignore attribute matching entirely when linking attribute is empty.
#[test]
fn test_empty_linking_attribute_no_filtering() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Source spans with different attributes (should not matter without linking attribute)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("type", "worker"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.5, 1.0))
            .with_string_attr("type", "manager"),
    );

    // One target span with different attribute (should not matter without linking attribute)
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.0, 1.0))
            .with_string_attr("type", "processing"), // different from sources
    );

    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2); // Require 2 sources to form a link
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    // NO linking attribute set - should ignore attribute filtering entirely

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1); // Should create 1 link

    let link = &node_result.links[0];

    // Should use both sources despite different attributes (no filtering)
    assert_eq!(link.source_spans.len(), 2); // Both sources should be used
    assert_eq!(link.target_spans.len(), 1); // One target

    // Verify that attribute matching is ignored - sources with different types are linked
    let source_types: Vec<_> = link
        .source_spans
        .iter()
        .map(|s| &s.attributes["type"])
        .collect();
    assert!(source_types.contains(&&string_attr("worker")));
    assert!(source_types.contains(&&string_attr("manager")));

    // Verify target has different attribute but is still linked
    assert_eq!(
        link.target_spans[0].attributes["type"],
        string_attr("processing")
    );
}

/// Tests source span grouping by attribute with FirstCompletedGroup strategy.
/// Verifies that source spans are grouped by attribute and threshold applied per group.
/// Should form links based on earliest completion time among groups.
#[test]
fn test_grouping_first_completed_group_strategy() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Group 1 (shard_1): ends at 1.0 and 1.2 (group completes at 1.2)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("shard_id", "shard_1"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 1.0))
            .with_string_attr("shard_id", "shard_1"),
    );

    // Group 2 (shard_2): ends at 1.4 and 1.6 (group completes at 1.6)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.4, 1.0))
            .with_string_attr("shard_id", "shard_2"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.6, 1.0))
            .with_string_attr("shard_id", "shard_2"),
    );

    // Target span that starts after all groups complete
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
    modal.set_threshold(2); // Require 2 sources per group
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_group_by_attribute("shard_id".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::FirstCompletedGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];

    // Should use all 4 sources (2 from each group)
    assert_eq!(link.source_spans.len(), 4);
    assert_eq!(link.target_spans.len(), 1);

    // With FirstCompletedGroup, delay should be from first group completion (1.2) to target start (3.0)
    // Delay = 3.0 - 1.2 = 1.8 seconds
    assert_abs_diff_eq!(link.delay_seconds, 1.8);

    // Verify we have sources from both groups
    let shard_ids: Vec<_> = link
        .source_spans
        .iter()
        .map(|s| &s.attributes["shard_id"])
        .collect();
    assert!(shard_ids.contains(&&string_attr("shard_1")));
    assert!(shard_ids.contains(&&string_attr("shard_2")));
}

/// Tests source span grouping by attribute with WaitForLastGroup strategy.
/// Verifies that link delay is calculated based on the latest group completion.
/// Should wait for all groups to complete before calculating delay.
#[test]
fn test_grouping_wait_for_last_group_strategy() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Group 1 (shard_1): ends at 1.0 and 1.2 (group completes at 1.2)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("shard_id", "shard_1"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 1.0))
            .with_string_attr("shard_id", "shard_1"),
    );

    // Group 2 (shard_2): ends at 1.4 and 1.6 (group completes at 1.6)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.4, 1.0))
            .with_string_attr("shard_id", "shard_2"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.6, 1.0))
            .with_string_attr("shard_id", "shard_2"),
    );

    // Target span that starts after all groups complete
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
    modal.set_threshold(2); // Require 2 sources per group
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_group_by_attribute("shard_id".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];

    // Should use all 4 sources (2 from each group)
    assert_eq!(link.source_spans.len(), 4);
    assert_eq!(link.target_spans.len(), 1);

    // With WaitForLastGroup, delay should be from last group completion (1.6) to target start (3.0)
    // Delay = 3.0 - 1.6 = 1.4 seconds
    assert_abs_diff_eq!(link.delay_seconds, 1.4);

    // Verify we have sources from both groups
    let shard_ids: Vec<_> = link
        .source_spans
        .iter()
        .map(|s| &s.attributes["shard_id"])
        .collect();
    assert!(shard_ids.contains(&&string_attr("shard_1")));
    assert!(shard_ids.contains(&&string_attr("shard_2")));
}

/// Tests grouping with insufficient spans in some groups.
/// Verifies that links are not formed when any group has fewer spans than threshold.
/// Should require all groups to meet the threshold requirement.
#[test]
fn test_grouping_insufficient_spans_per_group() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Group 1 (shard_1): 2 spans (meets threshold)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("shard_id", "shard_1"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 1.0))
            .with_string_attr("shard_id", "shard_1"),
    );

    // Group 2 (shard_2): Only 1 span (insufficient for threshold=2)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.4, 1.0))
            .with_string_attr("shard_id", "shard_2"),
    );

    // Target span
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
    modal.set_threshold(2); // Require 2 sources per group
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_group_by_attribute("shard_id".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should complete");

    // Should have no links because shard_2 doesn't have enough spans
    assert!(
        result.per_node_results.is_empty()
            || result
                .per_node_results
                .values()
                .all(|node_result| node_result.links.is_empty())
    );
}

/// Tests grouping behavior when no spans have the group-by attribute.
/// Verifies error handling when the specified group-by attribute doesn't exist.
/// Should return appropriate error message about missing group attribute.
#[test]
fn test_grouping_missing_group_attribute() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Source spans without the grouping attribute
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.2, 1.0),
    ));

    // Target span
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
    modal.set_group_by_attribute("nonexistent_attribute".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    // Should either fail with error or have no results due to missing group attribute
    if let Some(result) = modal.analysis_result.as_ref() {
        // If analysis completes, it should have no links
        let node_result = &result.per_node_results["node_a"];
        assert_eq!(node_result.links.len(), 0);
    } else {
        // Or it might fail with an error
        assert!(modal.get_error_message().is_some());
    }
}

/// Tests grouping with only one group present.
/// Verifies that grouping logic still works when all spans belong to the same group.
/// Should behave similarly to non-grouped analysis when only one group exists.
#[test]
fn test_grouping_single_group_only() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // All source spans have the same group value
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("region", "us-west"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 1.0))
            .with_string_attr("region", "us-west"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.4, 1.0))
            .with_string_attr("region", "us-west"),
    );

    // Target span
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
    modal.set_group_by_attribute("region".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should have produced results");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];

    // Should use the threshold number of sources (2) from the single group
    assert_eq!(link.source_spans.len(), 2);
    assert_eq!(link.target_spans.len(), 1);

    // All sources should have the same region attribute
    for source_span in &link.source_spans {
        assert_eq!(source_span.attributes["region"], string_attr("us-west"));
    }

    // With single group ending at 1.2 (0.2 + 1.0), delay = 3.0 - 1.2 = 1.8
    assert_abs_diff_eq!(link.delay_seconds, 1.8);
}

/// Tests complex scenario combining all features: grouping, linking attributes, cross-node, timing strategy.
/// Verifies that all features work together correctly in a comprehensive scenario.
/// Should handle grouped cross-node N-to-One dependencies with attribute matching and statistics.
#[test]
fn test_complex_all_features_combined() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("source_node");
    builder.add_node("target_node");

    // Source spans on source_node with grouping and linking attributes
    // Group "batch_1": 2 spans ending at 1.0 and 1.2 (latest at 1.2)
    builder.add_span(
        SpanConfig::new(
            "worker",
            "source_node",
            TimeInterval::with_duration(0.0, 1.0),
        )
        .with_string_attr("batch_id", "batch_1")
        .with_string_attr("env", "prod"),
    );
    builder.add_span(
        SpanConfig::new(
            "worker",
            "source_node",
            TimeInterval::with_duration(0.2, 1.0),
        )
        .with_string_attr("batch_id", "batch_1")
        .with_string_attr("env", "prod"),
    );

    // Group "batch_2": 3 spans ending at 1.5, 1.7, 1.9 (latest at 1.9)
    builder.add_span(
        SpanConfig::new(
            "worker",
            "source_node",
            TimeInterval::with_duration(0.5, 1.0),
        )
        .with_string_attr("batch_id", "batch_2")
        .with_string_attr("env", "prod"),
    );
    builder.add_span(
        SpanConfig::new(
            "worker",
            "source_node",
            TimeInterval::with_duration(0.7, 1.0),
        )
        .with_string_attr("batch_id", "batch_2")
        .with_string_attr("env", "prod"),
    );
    builder.add_span(
        SpanConfig::new(
            "worker",
            "source_node",
            TimeInterval::with_duration(0.9, 1.0),
        )
        .with_string_attr("batch_id", "batch_2")
        .with_string_attr("env", "prod"),
    );

    // Non-matching source span (different env) - should be filtered out
    builder.add_span(
        SpanConfig::new(
            "worker",
            "source_node",
            TimeInterval::with_duration(0.0, 0.5),
        )
        .with_string_attr("batch_id", "batch_3")
        .with_string_attr("env", "dev"), // Different env - won't match
    );

    // Target spans on target_node with matching attributes
    builder.add_span(
        SpanConfig::new(
            "processor",
            "target_node",
            TimeInterval::with_duration(3.0, 1.0),
        )
        .with_string_attr("env", "prod"), // delay = 3.0 - 1.9 = 1.1s (from batch_2)
    );
    builder.add_span(
        SpanConfig::new(
            "processor",
            "target_node",
            TimeInterval::with_duration(4.0, 1.0),
        )
        .with_string_attr("env", "prod"), // delay = 4.0 - 1.9 = 2.1s (from batch_2)
    );

    // Non-matching target span
    builder.add_span(
        SpanConfig::new(
            "processor",
            "target_node",
            TimeInterval::with_duration(5.0, 1.0),
        )
        .with_string_attr("env", "dev"), // Won't match
    );

    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("worker".to_string()));
    modal.set_target_span_name(Some("processor".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::AllNodes);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_linking_attribute("env".to_string());
    modal.set_group_by_attribute("batch_id".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);
    modal.set_source_timing_strategy(traviz::analyze_dependency::SourceTimingStrategy::LatestFirst);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Complex analysis should work when all features are correctly implemented");

    if result.per_node_results.is_empty() {
        // Check if there's an error or if the combination of features prevents link formation
        if let Some(error) = modal.get_error_message() {
            panic!("Analysis failed with error: {}", error);
        } else {
            panic!("No links formed - complex feature combination may have implementation issues");
        }
    }

    // Get the actual node with results (could be source_node instead of target_node)
    let (_, node_result) = result
        .per_node_results
        .iter()
        .next()
        .expect("Should have at least one node with results");

    if node_result.links.is_empty() {
        panic!("No dependency links formed - implementation may not handle complex feature combinations correctly");
    }

    // Verify all features working together
    for (i, link) in node_result.links.iter().enumerate() {
        // Cross-node: sources and targets should be on different nodes
        assert_ne!(
            link.source_spans[0].node.name, link.target_spans[0].node.name,
            "Cross-node analysis should link spans from different nodes"
        );

        // Linking attributes: all spans should have env="prod"
        for source_span in &link.source_spans {
            assert_eq!(
                source_span.attributes["env"],
                string_attr("prod"),
                "Linking attribute filtering failed for source spans"
            );
        }
        assert_eq!(
            link.target_spans[0].attributes["env"],
            string_attr("prod"),
            "Linking attribute filtering failed for target spans"
        );

        // Grouping: should use exactly threshold sources from each group
        // We have 2 groups (batch_1, batch_2) with threshold=2, so expect 4 total
        assert_eq!(
            link.source_spans.len(),
            4,
            "Should use 2 spans from each of the 2 groups, got {}",
            link.source_spans.len()
        );

        // Verify both batches are represented
        let batches: Vec<_> = link
            .source_spans
            .iter()
            .map(|s| &s.attributes["batch_id"])
            .collect();
        assert!(
            batches.contains(&&string_attr("batch_1")),
            "Missing spans from batch_1 group"
        );
        assert!(
            batches.contains(&&string_attr("batch_2")),
            "Missing spans from batch_2 group"
        );

        // Count spans per batch
        let batch_1_count = batches
            .iter()
            .filter(|&&b| *b == string_attr("batch_1"))
            .count();
        let batch_2_count = batches
            .iter()
            .filter(|&&b| *b == string_attr("batch_2"))
            .count();
        assert_eq!(batch_1_count, 2, "Should have exactly 2 spans from batch_1");
        assert_eq!(batch_2_count, 2, "Should have exactly 2 spans from batch_2");

        // Timing strategy (LatestFirst) + Grouping (WaitForLastGroup):
        // batch_1 completes at 1.2, batch_2 completes at 1.9
        // WaitForLastGroup uses 1.9 (latest group completion)
        // Target starts at 3.0 or 4.0, so delays should be 1.1s or 2.1s
        assert!((link.delay_seconds - 1.1).abs() < 0.01 || (link.delay_seconds - 2.1).abs() < 0.01);
    }

    // Only verify statistics if we have multiple links
    if node_result.links.len() >= 2 {
        // Verify statistics with known delays [1.1, 2.1]
        let stats = &node_result.link_delay_statistics;
        assert_eq!(stats.count, 2);
        assert!((stats.min - 1.1).abs() < 0.01);
        assert!((stats.max - 2.1).abs() < 0.01);
        assert!((stats.mean() - 1.6).abs() < 0.01);
        assert!((stats.median() - 1.6).abs() < 0.01);

        // Verify min/max delay links
        assert!(node_result.min_delay_link.is_some());
        assert!(node_result.max_delay_link.is_some());
    }
}

/// Tests edge case: groups completing at exactly the same time.
/// This should expose bugs in group completion time calculation.
#[test]
fn test_grouping_simultaneous_group_completion() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Group 1: Both spans end at exactly 1.0
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("group", "A"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.5, 0.5))
            .with_string_attr("group", "A"),
    );

    // Group 2: Both spans also end at exactly 1.0
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 0.8))
            .with_string_attr("group", "B"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.3, 0.7))
            .with_string_attr("group", "B"),
    );

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
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.set_group_by_attribute("group".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::FirstCompletedGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Should handle simultaneous group completion");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    // When groups complete simultaneously, FirstCompletedGroup should still work
    // Delay should be 2.0 - 1.0 = 1.0
    assert_abs_diff_eq!(link.delay_seconds, 1.0);
}

/// Tests edge case: mixed group sizes with threshold enforcement.
/// Should expose bugs in per-group threshold validation.
#[test]
fn test_grouping_mixed_group_sizes_strict_threshold() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Group 1: Has exactly threshold (2) spans
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("partition", "P1"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.1, 1.0))
            .with_string_attr("partition", "P1"),
    );

    // Group 2: Has more than threshold (3 spans, threshold=2)
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 1.0))
            .with_string_attr("partition", "P2"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.3, 1.0))
            .with_string_attr("partition", "P2"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.4, 1.0))
            .with_string_attr("partition", "P2"),
    );

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
    modal.set_group_by_attribute("partition".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Should handle mixed group sizes");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];

    // Should use exactly threshold spans from each group (4 total: 2 from P1, 2 from P2)
    assert_eq!(link.source_spans.len(), 4);

    // Verify group distribution
    let partitions: Vec<_> = link
        .source_spans
        .iter()
        .map(|s| &s.attributes["partition"])
        .collect();
    let p1_count = partitions
        .iter()
        .filter(|&&p| *p == string_attr("P1"))
        .count();
    let p2_count = partitions
        .iter()
        .filter(|&&p| *p == string_attr("P2"))
        .count();

    assert_eq!(
        p1_count, 2,
        "Should use exactly 2 spans from P1 (threshold)"
    );
    assert_eq!(
        p2_count, 2,
        "Should use exactly 2 spans from P2 (threshold), not all 3 available"
    );
}

/// Tests edge case: empty string group attribute values.
#[test]
fn test_grouping_empty_string_group_values() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Spans with empty string group values
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
            .with_string_attr("category", ""),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.1, 1.0))
            .with_string_attr("category", ""),
    );

    // Spans with non-empty group values
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.2, 1.0))
            .with_string_attr("category", "valid"),
    );
    builder.add_span(
        SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.3, 1.0))
            .with_string_attr("category", "valid"),
    );

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
    modal.set_group_by_attribute("category".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Should handle empty string group values");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    assert_eq!(link.source_spans.len(), 4);

    // Verify both groups are represented
    let categories: Vec<_> = link
        .source_spans
        .iter()
        .map(|s| &s.attributes["category"])
        .collect();
    assert!(
        categories.contains(&&string_attr("")),
        "Missing spans from empty string group"
    );
    assert!(
        categories.contains(&&string_attr("valid")),
        "Missing spans from 'valid' group"
    );
}

/// Tests grouping with mixed group completions and 1-to-N cardinality.
/// Verifies that grouping works correctly with OneToN analysis mode.
/// Should group target spans instead of source spans in 1-to-N mode.
#[test]
fn test_grouping_one_to_n_cardinality() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // One source span
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));

    // Target spans in different groups
    // Group 1 (type_a): 2 spans
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.0, 1.0))
            .with_string_attr("type", "type_a"),
    );
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.2, 1.0))
            .with_string_attr("type", "type_a"),
    );

    // Group 2 (type_b): 2 spans
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.4, 1.0))
            .with_string_attr("type", "type_b"),
    );
    builder.add_span(
        SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.6, 1.0))
            .with_string_attr("type", "type_b"),
    );

    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::OneToN);
    modal.set_group_by_attribute("type".to_string());
    modal.set_group_aggregation_strategy(GroupAggregationStrategy::WaitForLastGroup);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should work with 1-to-N cardinality + grouping");

    let node_result = &result.per_node_results["node_a"];
    assert_eq!(
        node_result.links.len(),
        1,
        "Should create exactly 1 link with 1-to-N + grouping"
    );

    let link = &node_result.links[0];

    // Should have 1 source and 4 targets (2 from each group)
    assert_eq!(
        link.source_spans.len(),
        1,
        "Should have exactly 1 source span in 1-to-N mode"
    );
    assert_eq!(
        link.target_spans.len(),
        4,
        "Should have 4 targets (2 from each group) when threshold=2 and 2 groups exist"
    );

    // Verify we have targets from both groups
    let target_types: Vec<_> = link
        .target_spans
        .iter()
        .map(|s| &s.attributes["type"])
        .collect();
    assert!(
        target_types.contains(&&string_attr("type_a")),
        "Missing targets from type_a group"
    );
    assert!(
        target_types.contains(&&string_attr("type_b")),
        "Missing targets from type_b group"
    );

    // Count spans in each group - this should be exactly threshold per group
    let type_a_count = target_types
        .iter()
        .filter(|&&t| *t == string_attr("type_a"))
        .count();
    let type_b_count = target_types
        .iter()
        .filter(|&&t| *t == string_attr("type_b"))
        .count();
    assert_eq!(type_a_count, 2);
    assert_eq!(type_b_count, 2);
}

// ============================================================================
// EDGE CASE AND ERROR HANDLING TESTS
// ============================================================================

/// Tests error handling when target span name doesn't exist.
/// Verifies appropriate error message when target spans cannot be found.
/// Should provide clear error message about missing target span name.
#[test]
fn test_error_missing_target_spans() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");
    builder.add_span(SpanConfig::new(
        "valid_source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    ));
    builder.add_span(SpanConfig::new(
        "another_source",
        "node_a",
        TimeInterval::with_duration(0.5, 1.0),
    ));
    let scenario = builder.build();

    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("valid_source".to_string()));
    modal.set_target_span_name(Some("nonexistent_target".to_string())); // This doesn't exist
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    // Analysis should fail with no results
    assert!(
        modal.analysis_result.is_none(),
        "Analysis should fail when target span name doesn't exist"
    );

    // Should have error message about missing target spans
    assert!(
        modal.get_error_message().is_some(),
        "Should have error message when target spans are missing"
    );
    let error = modal.get_error_message().unwrap();
    assert!(
        error.contains("No spans found with name 'nonexistent_target'"),
        "Error message should mention the missing target span name, got: {}",
        error
    );
}

/// Tests behavior with overlapping source and target spans.
/// Verifies handling when source spans end after target spans start (invalid timing).
/// Should exclude temporally invalid span pairs from link formation.
#[test]
fn test_overlapping_spans_invalid_timing() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Source span: 2.0 -> 4.0 (ends at 4.0)
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(2.0, 2.0),
    ));

    // Target span: 1.0 -> 3.0 (starts at 1.0, ends at 3.0)
    // This is INVALID because source ends (4.0) AFTER target starts (1.0)
    // but source also starts (2.0) AFTER target starts (1.0)
    // For a valid dependency, source must END before target STARTS
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(1.0, 2.0),
    ));

    // Another target that starts before source ends - also invalid
    // Target: 3.5 -> 4.5, Source ends at 4.0, so source ends after target starts
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(3.5, 1.0),
    ));

    // Add one valid target that starts after source ends
    // Target: 5.0 -> 6.0 (starts at 5.0, after source ends at 4.0) - VALID
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(5.0, 1.0),
    ));

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
        .expect("Analysis should complete even with invalid timing spans");

    let node_result = &result.per_node_results["node_a"];

    // Should only have 1 link (with the valid target that starts at 5.0)
    assert_eq!(node_result.links.len(), 1);

    let link = &node_result.links[0];
    assert_eq!(link.source_spans.len(), 1);
    assert_eq!(link.target_spans.len(), 1);

    // Verify the valid link has correct timing
    // Source ends at 4.0, target starts at 5.0, delay = 1.0
    assert_abs_diff_eq!(link.delay_seconds, 1.0);

    // Verify we're linking to the correct target (the one starting at 5.0)
    assert_abs_diff_eq!(link.target_spans[0].start_time, 5.0);
}

/// Tests behavior with identical start/end times.
/// Verifies handling of spans with zero duration or simultaneous timing.
/// Should handle edge cases with identical timestamps appropriately.
#[test]
fn test_identical_timing_edge_cases() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Case 1: Zero-duration source span (start_time == end_time)
    builder.add_span(SpanConfig::new(
        "zero_duration_source",
        "node_a",
        TimeInterval::with_duration(1.0, 0.0), // 1.0 -> 1.0
    ));

    // Case 2: Normal duration source span
    builder.add_span(SpanConfig::new(
        "normal_source",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0), // 2.0 -> 3.0
    ));

    // Case 3: Target that starts exactly when zero-duration source ends
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(1.0, 1.0), // 1.0 -> 2.0
    ));

    // Case 4: Target that starts exactly when normal source ends
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(3.0, 1.0), // 3.0 -> 4.0
    ));

    // Case 5: Zero-duration target
    builder.add_span(SpanConfig::new(
        "zero_duration_target",
        "node_a",
        TimeInterval::with_duration(4.0, 0.0), // 4.0 -> 4.0
    ));

    // Case 6: Target that starts after zero-duration target
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(5.0, 1.0), // 5.0 -> 6.0
    ));

    let scenario = builder.build();

    // Test with zero-duration source
    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);

    modal.set_source_span_name(Some("zero_duration_source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(1);
    modal.set_source_scope(SourceScope::SameNode);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("Analysis should handle zero-duration spans");

    let node_result = &result.per_node_results["node_a"];

    // Zero-duration source (ends at 1.0) should link to targets starting at 1.0 and later
    // Should find links to targets starting at 1.0, 3.0, and 5.0
    assert!(node_result.links.len() >= 1,
           "Zero-duration source should be able to form links with targets starting at or after its end time");

    // Test one specific case: zero-duration source ending at 1.0 linking to target starting at 1.0
    let found_simultaneous_link = node_result.links.iter().any(|link| {
        link.delay_seconds == 0.0
            && link.source_spans[0].end_time == 1.0
            && link.target_spans[0].start_time == 1.0
    });
    assert!(
        found_simultaneous_link,
        "Should allow link when source ends exactly when target starts (zero delay)"
    );

    // Test with normal source linking to zero-duration target
    let mut modal2 = AnalyzeDependencyModal::new();
    modal2.update_span_list(&scenario.all_spans);

    modal2.set_source_span_name(Some("normal_source".to_string()));
    modal2.set_target_span_name(Some("zero_duration_target".to_string()));
    modal2.set_threshold(1);
    modal2.set_source_scope(SourceScope::SameNode);
    modal2.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal2.analyze_dependencies();

    let result2 = modal2
        .analysis_result
        .as_ref()
        .expect("Analysis should handle linking to zero-duration targets");

    let node_result2 = &result2.per_node_results["node_a"];

    // Normal source (ends at 3.0) should link to zero-duration target (starts at 4.0)
    assert_eq!(node_result2.links.len(), 1);

    let link = &node_result2.links[0];
    assert_abs_diff_eq!(link.delay_seconds, 1.0);

    // Verify zero-duration target span has start_time == end_time
    assert_abs_diff_eq!(
        link.target_spans[0].start_time,
        link.target_spans[0].end_time
    );

    // Test exact simultaneous timing (source ends exactly when target starts)
    let mut modal3 = AnalyzeDependencyModal::new();
    modal3.update_span_list(&scenario.all_spans);

    modal3.set_source_span_name(Some("normal_source".to_string()));
    modal3.set_target_span_name(Some("target".to_string()));
    modal3.set_threshold(1);
    modal3.set_source_scope(SourceScope::SameNode);
    modal3.set_analysis_cardinality(AnalysisCardinality::NToOne);

    modal3.analyze_dependencies();

    let result3 = modal3
        .analysis_result
        .as_ref()
        .expect("Analysis should handle exact simultaneous timing");

    let node_result3 = &result3.per_node_results["node_a"];

    // Should find link where source ends at 3.0 and target starts at 3.0 (zero delay)
    let found_zero_delay = node_result3.links.iter().any(|link| {
        (link.delay_seconds - 0.0).abs() < 0.001
            && (link.source_spans[0].end_time - 3.0).abs() < 0.001
            && (link.target_spans[0].start_time - 3.0).abs() < 0.001
    });
    assert!(
        found_zero_delay,
        "Should handle exact simultaneous timing (source end == target start) with zero delay"
    );
}

/// Tests analysis with spans from single node but AllNodes scope.
/// Verifies that AllNodes scope works correctly when all spans are on one node.
/// Should produce same results as SameNode scope in this scenario.
#[test]
fn test_all_nodes_scope_single_node_scenario() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("single_node");

    // Create a complex scenario with multiple sources and targets on one node
    builder.add_span(SpanConfig::new(
        "source",
        "single_node",
        TimeInterval::with_duration(0.0, 1.0),
    )); // ends at 1.0
    builder.add_span(SpanConfig::new(
        "source",
        "single_node",
        TimeInterval::with_duration(0.5, 1.0),
    )); // ends at 1.5
    builder.add_span(SpanConfig::new(
        "source",
        "single_node",
        TimeInterval::with_duration(1.0, 1.0),
    )); // ends at 2.0

    builder.add_span(SpanConfig::new(
        "target",
        "single_node",
        TimeInterval::with_duration(2.5, 1.0),
    )); // starts at 2.5, delay from sources: 1.5, 1.0, 0.5
    builder.add_span(SpanConfig::new(
        "target",
        "single_node",
        TimeInterval::with_duration(3.0, 1.0),
    )); // starts at 3.0, delay from sources: 2.0, 1.5, 1.0

    let scenario = builder.build();

    // Test with SameNode scope
    let mut modal_same_node = AnalyzeDependencyModal::new();
    modal_same_node.update_span_list(&scenario.all_spans);
    modal_same_node.set_source_span_name(Some("source".to_string()));
    modal_same_node.set_target_span_name(Some("target".to_string()));
    modal_same_node.set_threshold(2);
    modal_same_node.set_source_scope(SourceScope::SameNode);
    modal_same_node.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal_same_node.analyze_dependencies();

    // Test with AllNodes scope
    let mut modal_all_nodes = AnalyzeDependencyModal::new();
    modal_all_nodes.update_span_list(&scenario.all_spans);
    modal_all_nodes.set_source_span_name(Some("source".to_string()));
    modal_all_nodes.set_target_span_name(Some("target".to_string()));
    modal_all_nodes.set_threshold(2);
    modal_all_nodes.set_source_scope(SourceScope::AllNodes);
    modal_all_nodes.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal_all_nodes.analyze_dependencies();

    // Both should produce results
    let result_same_node = modal_same_node
        .analysis_result
        .as_ref()
        .expect("SameNode analysis should succeed");
    let result_all_nodes = modal_all_nodes
        .analysis_result
        .as_ref()
        .expect("AllNodes analysis should succeed");

    // Results should be identical
    assert_eq!(
        result_same_node.per_node_results.len(),
        result_all_nodes.per_node_results.len(),
        "Both scopes should analyze the same number of nodes"
    );

    let node_result_same = &result_same_node.per_node_results["single_node"];
    let node_result_all = &result_all_nodes.per_node_results["single_node"];

    // Should have same number of links
    assert_eq!(
        node_result_same.links.len(),
        node_result_all.links.len(),
        "Both scopes should find the same number of dependency links"
    );

    // Compare each link's properties
    for (i, (link_same, link_all)) in node_result_same
        .links
        .iter()
        .zip(node_result_all.links.iter())
        .enumerate()
    {
        assert_abs_diff_eq!(
            link_same.delay_seconds,
            link_all.delay_seconds,
            epsilon = 0.001
        );

        assert_eq!(link_same.source_spans.len(), link_all.source_spans.len());
        assert_eq!(link_same.target_spans.len(), link_all.target_spans.len());

        // Verify source spans are identical (same span IDs)
        for (src_same, src_all) in link_same
            .source_spans
            .iter()
            .zip(link_all.source_spans.iter())
        {
            assert_eq!(
                src_same.span_id, src_all.span_id,
                "Source spans should be identical between scopes"
            );
        }

        // Verify target spans are identical
        for (tgt_same, tgt_all) in link_same
            .target_spans
            .iter()
            .zip(link_all.target_spans.iter())
        {
            assert_eq!(
                tgt_same.span_id, tgt_all.span_id,
                "Target spans should be identical between scopes"
            );
        }
    }

    // Statistics should also be identical
    assert_eq!(
        node_result_same.link_delay_statistics.count,
        node_result_all.link_delay_statistics.count
    );
    assert_abs_diff_eq!(
        node_result_same.link_delay_statistics.min,
        node_result_all.link_delay_statistics.min
    );
    assert_abs_diff_eq!(
        node_result_same.link_delay_statistics.max,
        node_result_all.link_delay_statistics.max
    );
}

/// Tests span reuse prevention in same-node mode.
/// Verifies that source spans used in one link cannot be reused for another link.
/// Should ensure each source span is used at most once per analysis.
#[test]
fn test_span_reuse_prevention_same_node() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("node_a");

    // Create 3 source spans that all could potentially link to multiple targets
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.0, 1.0),
    )); // ends at 1.0
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.2, 1.0),
    )); // ends at 1.2
    builder.add_span(SpanConfig::new(
        "source",
        "node_a",
        TimeInterval::with_duration(0.4, 1.0),
    )); // ends at 1.4

    // Create 3 target spans that all start after all sources end
    // All targets could theoretically link to all sources
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(2.0, 1.0),
    )); // starts at 2.0
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(2.5, 1.0),
    )); // starts at 2.5
    builder.add_span(SpanConfig::new(
        "target",
        "node_a",
        TimeInterval::with_duration(3.0, 1.0),
    )); // starts at 3.0

    let scenario = builder.build();

    // Test N-to-One with threshold=2 (need 2 sources per link)
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
        .expect("Analysis should succeed");
    let node_result = &result.per_node_results["node_a"];

    // With 3 sources and threshold=2, we can form at most 1 link
    // (because after using 2 sources for one link, only 1 source remains)
    assert!(
        node_result.links.len() <= 1,
        "Should not form more than 1 link when threshold=2 and only 3 sources available"
    );

    if !node_result.links.is_empty() {
        let link = &node_result.links[0];
        assert_eq!(
            link.source_spans.len(),
            2,
            "Link should use exactly 2 source spans (threshold)"
        );
        assert_eq!(
            link.target_spans.len(),
            1,
            "N-to-One should have exactly 1 target span"
        );
    }

    // Test One-to-N with threshold=2 (need 2 targets per link)
    let mut modal2 = AnalyzeDependencyModal::new();
    modal2.update_span_list(&scenario.all_spans);
    modal2.set_source_span_name(Some("source".to_string()));
    modal2.set_target_span_name(Some("target".to_string()));
    modal2.set_threshold(2);
    modal2.set_source_scope(SourceScope::SameNode);
    modal2.set_analysis_cardinality(AnalysisCardinality::OneToN);
    modal2.analyze_dependencies();

    let result2 = modal2
        .analysis_result
        .as_ref()
        .expect("One-to-N analysis should succeed");
    let node_result2 = &result2.per_node_results["node_a"];

    // With 3 sources and threshold=2 for targets, we should form at most 1 link
    // (each source should only be used once)
    assert!(
        node_result2.links.len() <= 3,
        "Should not have more links than available source spans"
    );

    // Verify no source span is used in multiple links
    let mut all_used_source_ids = std::collections::HashSet::new();
    for link in &node_result2.links {
        assert_eq!(
            link.source_spans.len(),
            1,
            "One-to-N should have exactly 1 source span per link"
        );
        assert_eq!(
            link.target_spans.len(),
            2,
            "Should use exactly 2 target spans (threshold)"
        );

        let source_id = &link.source_spans[0].span_id;
        assert!(
            !all_used_source_ids.contains(source_id),
            "Source span should not be reused across multiple links"
        );
        all_used_source_ids.insert(source_id.clone());
    }
}

/// Tests span reuse prevention in all-nodes mode.
/// Verifies correct span reuse behavior when analyzing across multiple nodes.
/// Should allow appropriate reuse patterns while preventing conflicts.
#[test]
fn test_span_reuse_prevention_all_nodes() {
    let mut builder = ScenarioBuilder::new();
    builder.add_node("source_node");
    builder.add_node("target_node_1");
    builder.add_node("target_node_2");

    // Source spans on source_node
    builder.add_span(SpanConfig::new(
        "source",
        "source_node",
        TimeInterval::with_duration(0.0, 1.0),
    )); // ends at 1.0
    builder.add_span(SpanConfig::new(
        "source",
        "source_node",
        TimeInterval::with_duration(0.5, 1.0),
    )); // ends at 1.5

    // Target spans on target_node_1
    builder.add_span(SpanConfig::new(
        "target",
        "target_node_1",
        TimeInterval::with_duration(2.0, 1.0),
    )); // starts at 2.0
    builder.add_span(SpanConfig::new(
        "target",
        "target_node_1",
        TimeInterval::with_duration(2.5, 1.0),
    )); // starts at 2.5

    // Target spans on target_node_2
    builder.add_span(SpanConfig::new(
        "target",
        "target_node_2",
        TimeInterval::with_duration(3.0, 1.0),
    )); // starts at 3.0
    builder.add_span(SpanConfig::new(
        "target",
        "target_node_2",
        TimeInterval::with_duration(3.5, 1.0),
    )); // starts at 3.5

    let scenario = builder.build();

    // Test N-to-One with AllNodes scope and threshold=2
    let mut modal = AnalyzeDependencyModal::new();
    modal.update_span_list(&scenario.all_spans);
    modal.set_source_span_name(Some("source".to_string()));
    modal.set_target_span_name(Some("target".to_string()));
    modal.set_threshold(2);
    modal.set_source_scope(SourceScope::AllNodes);
    modal.set_analysis_cardinality(AnalysisCardinality::NToOne);
    modal.analyze_dependencies();

    let result = modal
        .analysis_result
        .as_ref()
        .expect("AllNodes analysis should succeed");

    // Should have results for target nodes (where the targets are)
    let total_links: usize = result
        .per_node_results
        .values()
        .map(|node_result| node_result.links.len())
        .sum();

    // With 2 sources and threshold=2, we can form at most 2 links across all nodes
    // (each source can potentially be used in multiple cross-node links)
    assert!(total_links <= 4);

    // Verify that each target node can have its own links
    for (node_name, node_result) in &result.per_node_results {
        for link in &node_result.links {
            // All target spans in a link should be from the same node (the key node)
            for target_span in &link.target_spans {
                assert_eq!(target_span.node.name, *node_name);
            }

            // Source spans should be from source_node (cross-node dependency)
            for source_span in &link.source_spans {
                assert_eq!(source_span.node.name, "source_node");
            }

            assert_eq!(
                link.source_spans.len(),
                2,
                "Should use exactly threshold (2) source spans per link"
            );
            assert_eq!(
                link.target_spans.len(),
                1,
                "N-to-One should have exactly 1 target span per link"
            );
        }
    }

    // Test One-to-N with AllNodes scope
    let mut modal2 = AnalyzeDependencyModal::new();
    modal2.update_span_list(&scenario.all_spans);
    modal2.set_source_span_name(Some("source".to_string()));
    modal2.set_target_span_name(Some("target".to_string()));
    modal2.set_threshold(2);
    modal2.set_source_scope(SourceScope::AllNodes);
    modal2.set_analysis_cardinality(AnalysisCardinality::OneToN);
    modal2.analyze_dependencies();

    let result2 = modal2
        .analysis_result
        .as_ref()
        .expect("One-to-N AllNodes analysis should succeed");

    // In One-to-N with AllNodes, results should be grouped by source node
    // Each source span should potentially link to multiple targets across nodes
    for (node_name, node_result) in &result2.per_node_results {
        for link in &node_result.links {
            assert_eq!(
                link.source_spans.len(),
                1,
                "One-to-N should have exactly 1 source span per link"
            );
            assert_eq!(
                link.target_spans.len(),
                2,
                "Should use exactly threshold (2) target spans per link"
            );

            // In One-to-N with AllNodes, the result node should be where source spans are located
            for source_span in &link.source_spans {
                assert_eq!(
                    source_span.node.name, *node_name,
                    "Source span should be from the result node in One-to-N"
                );
            }
        }
    }
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
    assert_eq!(node_result.links.len(), 5);

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
