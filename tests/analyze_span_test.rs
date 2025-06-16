use std::collections::BTreeMap;
use std::rc::Rc;

use opentelemetry_proto::tonic::common::v1::any_value::Value;
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
        analyzer.open(&[span.clone()]);
        analyzer.set_attribute_filter(filter_string.to_string());

        // Test the filtering
        let matches = analyzer.span_matches_attribute_filter(&span);
        assert!(matches, "{type_name} attribute failed!");
    }
}
