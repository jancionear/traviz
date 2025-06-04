use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use opentelemetry_proto::tonic::common::v1::any_value::Value;

use traviz::types::{DisplayLength, Node, Span, SpanDisplayConfig, TimePoint};

/// Helper to create a simple fake node
#[allow(dead_code)]
pub fn create_test_node(name: &str) -> Rc<Node> {
    Rc::new(Node {
        name: name.to_string(),
        attributes: BTreeMap::new(),
    })
}

/// Helper to create a fake span with minimal required fields
#[allow(dead_code)]
pub fn create_test_span(
    name: &str,
    node: Rc<Node>,
    start_time: TimePoint,
    end_time: TimePoint,
    span_id: &[u8],
) -> Rc<Span> {
    Rc::new(Span {
        name: name.to_string(),
        original_name: name.to_string(),
        span_id: span_id.to_vec(),
        trace_id: vec![1, 2, 3, 4],
        parent_span_id: vec![],
        start_time,
        end_time,
        attributes: BTreeMap::new(),
        events: vec![],
        node,
        scope: None,
        children: RefCell::new(vec![]),
        display_children: RefCell::new(vec![]),
        min_start_time: Cell::new(start_time),
        max_end_time: Cell::new(end_time),
        display_options: SpanDisplayConfig {
            display_length: DisplayLength::Time,
        },
        collapse_children: Cell::new(false),
        dont_collapse_this_span: Cell::new(false),
        parent_height_offset: Cell::new(0),
        display_start: Cell::new(0.0),
        display_length: Cell::new(0.0),
        time_display_length: Cell::new(0.0),
        incoming_relations: RefCell::new(vec![]),
        outgoing_relations: RefCell::new(vec![]),
    })
}

/// Helper to create a span with attributes
#[allow(dead_code)]
pub fn create_test_span_with_attributes(
    name: &str,
    node: Rc<Node>,
    start_time: TimePoint,
    end_time: TimePoint,
    span_id: &[u8],
    attributes: BTreeMap<String, Option<Value>>,
) -> Rc<Span> {
    Rc::new(Span {
        name: name.to_string(),
        original_name: name.to_string(),
        span_id: span_id.to_vec(),
        trace_id: vec![1, 2, 3, 4],
        parent_span_id: vec![],
        start_time,
        end_time,
        attributes,
        events: vec![],
        node,
        scope: None,
        children: RefCell::new(vec![]),
        display_children: RefCell::new(vec![]),
        min_start_time: Cell::new(start_time),
        max_end_time: Cell::new(end_time),
        display_options: SpanDisplayConfig {
            display_length: DisplayLength::Time,
        },
        collapse_children: Cell::new(false),
        dont_collapse_this_span: Cell::new(false),
        parent_height_offset: Cell::new(0),
        display_start: Cell::new(0.0),
        display_length: Cell::new(0.0),
        time_display_length: Cell::new(0.0),
        incoming_relations: RefCell::new(vec![]),
        outgoing_relations: RefCell::new(vec![]),
    })
}

/// Helper to create a string attribute value
pub fn string_attr(value: &str) -> Option<Value> {
    Some(Value::StringValue(value.to_string()))
}

/// Helper to create simple test scenario with source and target spans
pub struct SimpleTestScenario {
    #[allow(dead_code)]
    pub source_spans: Vec<Rc<Span>>,
    #[allow(dead_code)]
    pub target_spans: Vec<Rc<Span>>,
    pub all_spans: Vec<Rc<Span>>,
}

impl SimpleTestScenario {
    /// Creates a basic scenario:
    /// - Node A: source span "task" ending at time 1.0
    /// - Node A: target span "process" starting at time 2.0
    /// This should create a dependency with 1.0 second delay
    pub fn basic_dependency() -> Self {
        let node_a = create_test_node("node_a");

        let source_span = create_test_span("task", node_a.clone(), 0.0, 1.0, &[1]);

        let target_span = create_test_span("process", node_a.clone(), 2.0, 3.0, &[2]);

        let source_spans = vec![source_span.clone()];
        let target_spans = vec![target_span.clone()];
        let all_spans = vec![source_span, target_span];

        Self {
            source_spans,
            target_spans,
            all_spans,
        }
    }

    /// Creates a multi-node scenario:
    /// - Node A: source span "task" ending at time 1.0
    /// - Node B: target span "process" starting at time 2.5
    /// This tests cross-node dependencies
    pub fn cross_node_dependency() -> Self {
        let node_a = create_test_node("node_a");
        let node_b = create_test_node("node_b");

        let source_span = create_test_span("task", node_a, 0.0, 1.0, &[1]);

        let target_span = create_test_span("process", node_b, 2.5, 3.5, &[2]);

        let source_spans = vec![source_span.clone()];
        let target_spans = vec![target_span.clone()];
        let all_spans = vec![source_span, target_span];

        Self {
            source_spans,
            target_spans,
            all_spans,
        }
    }
}
