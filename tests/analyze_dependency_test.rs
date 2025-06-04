use traviz::analyze_dependency::{AnalysisCardinality, AnalyzeDependencyModal, SourceScope};

mod test_helpers;
use test_helpers::SimpleTestScenario;

/// Tests basic dependency analysis between spans on the same node.
/// Verifies that a dependency link is correctly identified when a source span
/// ends before a target span starts, and delay calculation is accurate.
#[test]
fn test_basic_dependency_analysis() {
    let scenario = SimpleTestScenario::basic_dependency();

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
    let scenario = SimpleTestScenario::cross_node_dependency();

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
    let scenario = SimpleTestScenario::cross_node_dependency();

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
    let scenario = SimpleTestScenario::basic_dependency();

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
