use std::collections::BTreeMap;

use traviz::analyze_span::AnalyzeSpanModal;

mod test_helpers;
use test_helpers::*;

#[test]
fn test_attribute_filter_all_types() {
    let node = create_test_node("test_node");

    // Test cases: (attribute_name, attribute_value, filter_string, type)
    let test_cases = vec![
        ("status", string_attr("active"), "status=active", "string"),
        ("port", int_attr(8080), "port=8080", "int"),
        ("enabled", bool_attr(true), "enabled=true", "bool"),
        ("timeout", double_attr(30.5), "timeout=30.5", "double"),
    ];

    let mut analyzer = AnalyzeSpanModal::default();

    for (attr_name, attr_value, filter_string, type_name) in test_cases {
        let mut attrs = BTreeMap::new();
        attrs.insert(attr_name.to_string(), attr_value);
        let span =
            create_test_span_with_attributes("test_span", node.clone(), 0.0, 1.0, &[1], attrs);

        // Set up analyzer
        analyzer.open(std::slice::from_ref(&span));
        analyzer.set_attribute_filter(filter_string.to_string());

        // Test the filtering
        let matches = analyzer.span_matches_attribute_filter(&span);
        assert!(matches, "{type_name} attribute failed!");
    }
}

#[test]
fn test_group_by_attributes() {
    let node = create_test_node("test_node");

    // Span 1: H=1805, S=4 (matches with span 2)
    let mut attrs1 = BTreeMap::new();
    attrs1.insert("H".to_string(), int_attr(1805));
    attrs1.insert("S".to_string(), int_attr(4));
    let span1 = create_test_span_with_attributes("test_span", node.clone(), 1.0, 2.0, &[1], attrs1);

    // Span 2: S=4, H=1805 (matches with span 1, order doesn't matter)
    let mut attrs2 = BTreeMap::new();
    attrs2.insert("S".to_string(), int_attr(4));
    attrs2.insert("H".to_string(), int_attr(1805));
    let span2 = create_test_span_with_attributes("test_span", node.clone(), 3.0, 5.0, &[2], attrs2);

    // Span 3: H=1806, S=5 (different H and S - doesn't match)
    let mut attrs3 = BTreeMap::new();
    attrs3.insert("H".to_string(), int_attr(1806));
    attrs3.insert("S".to_string(), int_attr(5));
    let span3 = create_test_span_with_attributes("test_span", node.clone(), 6.0, 7.0, &[3], attrs3);

    // Span 4: H=1805, S=7 (same H, different S - doesn't match)
    let mut attrs4 = BTreeMap::new();
    attrs4.insert("H".to_string(), int_attr(1805));
    attrs4.insert("S".to_string(), int_attr(7));
    let span4 = create_test_span_with_attributes("test_span", node.clone(), 8.0, 9.0, &[4], attrs4);

    // Test single attribute grouping (H only)
    let mut analyzer = AnalyzeSpanModal::default();
    analyzer.set_group_by_attributes("H".to_string());

    let key1 = analyzer.get_grouping_key(&span1).expect("Should have key");
    let key2 = analyzer.get_grouping_key(&span2).expect("Should have key");
    let key3 = analyzer.get_grouping_key(&span3).expect("Should have key");
    let key4 = analyzer.get_grouping_key(&span4).expect("Should have key");

    assert_eq!(key1, vec!["1805"]);
    assert_eq!(key2, vec!["1805"]);
    assert_eq!(key3, vec!["1806"]);
    assert_eq!(key4, vec!["1805"]);

    // Test multiple attribute grouping (H and S)
    analyzer.set_group_by_attributes("H,S".to_string());

    let key1 = analyzer.get_grouping_key(&span1).expect("Should have key");
    let key2 = analyzer.get_grouping_key(&span2).expect("Should have key");
    let key3 = analyzer.get_grouping_key(&span3).expect("Should have key");
    let key4 = analyzer.get_grouping_key(&span4).expect("Should have key");

    assert_eq!(key1, vec!["1805", "4"]);
    assert_eq!(key2, vec!["1805", "4"]);
    assert_eq!(key3, vec!["1806", "5"]);
    assert_eq!(key4, vec!["1805", "7"]);

    // Test grouped span creation and statistics
    let grouped = AnalyzeSpanModal::create_grouped_span(&[span1.clone(), span2.clone()]);
    assert_eq!(grouped.start_time, 1.0);
    assert_eq!(grouped.end_time, 5.0);
    let duration = grouped.end_time - grouped.start_time;
    assert!(
        (duration - 4.0).abs() < 0.001,
        "Duration should be 4.0, got {}",
        duration
    );
}

#[test]
fn test_group_by_missing_attribute() {
    let node = create_test_node("test_node");

    let mut analyzer = AnalyzeSpanModal::default();
    analyzer.set_group_by_attributes("H,S".to_string());

    // Span missing S attribute should return None
    let mut attrs_missing = BTreeMap::new();
    attrs_missing.insert("H".to_string(), int_attr(1805));
    let span_missing =
        create_test_span_with_attributes("test_span", node.clone(), 3.0, 4.0, &[2], attrs_missing);

    let key_missing = analyzer.get_grouping_key(&span_missing);
    assert!(
        key_missing.is_none(),
        "Should return None when attribute is missing"
    );
}

#[test]
/// A single grouped span should be equivalent to the same non-grouped span.
fn test_single_span_group() {
    let node = create_test_node("test_node");

    let mut attrs = BTreeMap::new();
    attrs.insert("H".to_string(), int_attr(1805));

    let span = create_test_span_with_attributes("test_span", node.clone(), 0.5, 0.7, &[1], attrs);

    let spans = vec![span.clone()];

    let grouped = AnalyzeSpanModal::create_grouped_span(&spans);

    assert_eq!(
        grouped.start_time, span.start_time,
        "Single span group should preserve start time"
    );
    assert_eq!(
        grouped.end_time, span.end_time,
        "Single span group should preserve end time"
    );
    assert_eq!(
        grouped.name, span.name,
        "Single span group should preserve name"
    );
}
