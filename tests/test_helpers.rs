#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use opentelemetry_proto::tonic::common::v1::any_value::Value;

use traviz::types::{DisplayLength, Node, Span, SpanDisplayConfig, TimePoint};

/// Helper to create a simple fake node
pub fn create_test_node(name: &str) -> Rc<Node> {
    Rc::new(Node {
        name: name.to_string(),
        attributes: BTreeMap::new(),
    })
}

/// Helper to create a fake span with minimal required fields
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

/// Helper to create an integer attribute value
pub fn int_attr(value: i64) -> Option<Value> {
    Some(Value::IntValue(value))
}

/// Helper to create a boolean attribute value
pub fn bool_attr(value: bool) -> Option<Value> {
    Some(Value::BoolValue(value))
}

/// Helper to create a double attribute value
pub fn double_attr(value: f64) -> Option<Value> {
    Some(Value::DoubleValue(value))
}

/// Represents a time interval for spans
#[derive(Debug, Clone)]
pub struct TimeInterval {
    pub start: TimePoint,
    pub end: TimePoint,
}

impl TimeInterval {
    /// Creates an interval with specified duration
    pub fn with_duration(start: TimePoint, duration: f64) -> Self {
        Self {
            start,
            end: start + duration,
        }
    }
}

/// Configuration for creating a span with attributes and timing
#[derive(Debug, Clone)]
pub struct SpanConfig {
    pub name: String,
    pub node_name: String,
    pub timing: TimeInterval,
    pub attributes: BTreeMap<String, Option<Value>>,
    pub span_id: Vec<u8>,
}

impl SpanConfig {
    pub fn new(name: &str, node_name: &str, timing: TimeInterval) -> Self {
        static mut SPAN_ID_COUNTER: u8 = 1;
        let span_id = unsafe {
            let id = SPAN_ID_COUNTER;
            SPAN_ID_COUNTER += 1;
            vec![id]
        };

        Self {
            name: name.to_string(),
            node_name: node_name.to_string(),
            timing,
            attributes: BTreeMap::new(),
            span_id,
        }
    }

    /// Add an attribute to this span
    pub fn with_attr(mut self, key: &str, value: Option<Value>) -> Self {
        self.attributes.insert(key.to_string(), value);
        self
    }

    /// Add a string attribute
    pub fn with_string_attr(self, key: &str, value: &str) -> Self {
        self.with_attr(key, string_attr(value))
    }

    /// Add an integer attribute
    pub fn with_int_attr(self, key: &str, value: i64) -> Self {
        self.with_attr(key, int_attr(value))
    }

    /// Add a double attribute
    pub fn with_double_attr(self, key: &str, value: f64) -> Self {
        self.with_attr(key, double_attr(value))
    }
}

/// Builder for creating comprehensive test scenarios
pub struct ScenarioBuilder {
    nodes: Vec<Rc<Node>>,
    spans: Vec<SpanConfig>,
    next_node_counter: usize,
}

impl ScenarioBuilder {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            spans: Vec::new(),
            next_node_counter: 1,
        }
    }

    /// Add a node to the scenario
    pub fn add_node(&mut self, name: &str) -> &mut Self {
        self.nodes.push(create_test_node(name));
        self
    }

    /// Add nodes with automatic naming (node_1, node_2, etc.)
    pub fn add_nodes(&mut self, count: usize) -> &mut Self {
        for _ in 0..count {
            let node_name = format!("node_{}", self.next_node_counter);
            self.next_node_counter += 1;
            self.add_node(&node_name);
        }
        self
    }

    /// Add a span configuration
    pub fn add_span(&mut self, config: SpanConfig) -> &mut Self {
        self.spans.push(config);
        self
    }

    /// Add multiple spans with the same name but different timing
    pub fn add_spans_with_timing(
        &mut self,
        name: &str,
        node_name: &str,
        timings: &[TimeInterval],
    ) -> &mut Self {
        for timing in timings {
            self.add_span(SpanConfig::new(name, node_name, timing.clone()));
        }
        self
    }

    /// Build the scenario into a TestScenario
    pub fn build(self) -> TestScenario {
        let mut node_map: BTreeMap<String, Rc<Node>> = BTreeMap::new();

        // Ensure all referenced nodes exist
        for span_config in &self.spans {
            if !node_map.contains_key(&span_config.node_name) {
                // Find existing node or create new one
                if let Some(existing_node) =
                    self.nodes.iter().find(|n| n.name == span_config.node_name)
                {
                    node_map.insert(span_config.node_name.clone(), existing_node.clone());
                } else {
                    node_map.insert(
                        span_config.node_name.clone(),
                        create_test_node(&span_config.node_name),
                    );
                }
            }
        }

        // Create spans
        let mut all_spans = Vec::new();
        let mut source_spans = Vec::new();
        let mut target_spans = Vec::new();

        for span_config in self.spans {
            let node = node_map.get(&span_config.node_name).unwrap().clone();
            let span = create_test_span_with_attributes(
                &span_config.name,
                node,
                span_config.timing.start,
                span_config.timing.end,
                &span_config.span_id,
                span_config.attributes,
            );
            all_spans.push(span.clone());

            // Categorize as source or target based on naming convention
            if span_config.name.contains("source") || span_config.name.contains("task") {
                source_spans.push(span);
            } else if span_config.name.contains("target") || span_config.name.contains("process") {
                target_spans.push(span);
            }
        }

        TestScenario {
            source_spans,
            target_spans,
            all_spans,
        }
    }
}

impl Default for ScenarioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a complete test scenario with source and target spans
pub struct TestScenario {
    pub source_spans: Vec<Rc<Span>>,
    pub target_spans: Vec<Rc<Span>>,
    pub all_spans: Vec<Rc<Span>>,
}

impl TestScenario {
    /// Creates a scenario for testing 1-to-N cardinality on same node
    pub fn one_to_n_same_node() -> Self {
        let mut builder = ScenarioBuilder::new();
        builder.add_node("node_a");

        // One source span
        let source_timing = TimeInterval::with_duration(0.0, 1.0);
        builder.add_span(SpanConfig::new("source", "node_a", source_timing));

        // Multiple target spans that start after source ends
        let target_timings = vec![
            TimeInterval::with_duration(2.0, 1.0), // starts 1.0 after source ends
            TimeInterval::with_duration(2.5, 1.0), // starts 1.5 after source ends
            TimeInterval::with_duration(3.0, 1.0), // starts 2.0 after source ends
        ];
        builder.add_spans_with_timing("target", "node_a", &target_timings);

        builder.build()
    }

    /// Creates a scenario for testing 1-to-N cardinality across nodes
    pub fn one_to_n_cross_node() -> Self {
        let mut builder = ScenarioBuilder::new();
        builder.add_nodes(3); // node_1, node_2, node_3

        // One source span on node_1
        let source_timing = TimeInterval::with_duration(0.0, 1.0);
        builder.add_span(SpanConfig::new("source", "node_1", source_timing));

        // Target spans on different nodes
        builder.add_span(SpanConfig::new(
            "target",
            "node_2",
            TimeInterval::with_duration(2.0, 1.0),
        ));
        builder.add_span(SpanConfig::new(
            "target",
            "node_3",
            TimeInterval::with_duration(2.5, 1.0),
        ));

        builder.build()
    }

    /// Creates a scenario for testing timing strategies
    pub fn multiple_sources_timing() -> Self {
        let mut builder = ScenarioBuilder::new();
        builder.add_node("node_a");

        // Multiple source spans with different end times
        let source_timings = vec![
            TimeInterval::with_duration(0.0, 0.5), // ends at 0.5 (earliest)
            TimeInterval::with_duration(0.2, 0.6), // ends at 0.8 (middle)
            TimeInterval::with_duration(0.4, 0.8), // ends at 1.2 (latest)
        ];
        builder.add_spans_with_timing("source", "node_a", &source_timings);

        // One target span that starts after all sources
        builder.add_span(SpanConfig::new(
            "target",
            "node_a",
            TimeInterval::with_duration(2.0, 1.0),
        ));

        builder.build()
    }

    /// Creates a scenario for testing attribute linking
    pub fn with_linking_attributes() -> Self {
        let mut builder = ScenarioBuilder::new();
        builder.add_node("node_a");

        // Source spans with different height attributes
        builder.add_span(
            SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.0, 1.0))
                .with_string_attr("height", "100"),
        );
        builder.add_span(
            SpanConfig::new("source", "node_a", TimeInterval::with_duration(0.5, 1.0))
                .with_string_attr("height", "200"),
        );

        // Target spans with matching and non-matching height attributes
        builder.add_span(
            SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.0, 1.0))
                .with_string_attr("height", "100"), // matches first source
        );
        builder.add_span(
            SpanConfig::new("target", "node_a", TimeInterval::with_duration(2.5, 1.0))
                .with_string_attr("height", "300"), // no match
        );

        builder.build()
    }

    /// Creates a scenario specifically for testing statistical calculations.
    /// Multiple source-target pairs with known delays for validating min, max, mean, median.
    /// Delays: 0.5s, 1.0s, 1.5s, 2.0s, 3.0s (mean: 1.6s, median: 1.5s, min: 0.5s, max: 3.0s)
    pub fn for_statistics_testing() -> Self {
        let mut builder = ScenarioBuilder::new();
        builder.add_node("node_a");

        // Create source spans that end at different times
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
            "source",
            "node_a",
            TimeInterval::with_duration(2.0, 1.0),
        )); // ends at 3.0
        builder.add_span(SpanConfig::new(
            "source",
            "node_a",
            TimeInterval::with_duration(3.0, 1.0),
        )); // ends at 4.0
        builder.add_span(SpanConfig::new(
            "source",
            "node_a",
            TimeInterval::with_duration(4.0, 1.0),
        )); // ends at 5.0

        // Create target spans with precisely calculated delays
        builder.add_span(SpanConfig::new(
            "target",
            "node_a",
            TimeInterval::with_duration(1.5, 1.0),
        )); // delay = 0.5s (from source ending at 1.0)
        builder.add_span(SpanConfig::new(
            "target",
            "node_a",
            TimeInterval::with_duration(3.0, 1.0),
        )); // delay = 1.0s (from source ending at 2.0)
        builder.add_span(SpanConfig::new(
            "target",
            "node_a",
            TimeInterval::with_duration(4.5, 1.0),
        )); // delay = 1.5s (from source ending at 3.0)
        builder.add_span(SpanConfig::new(
            "target",
            "node_a",
            TimeInterval::with_duration(6.0, 1.0),
        )); // delay = 2.0s (from source ending at 4.0)
        builder.add_span(SpanConfig::new(
            "target",
            "node_a",
            TimeInterval::with_duration(8.0, 1.0),
        )); // delay = 3.0s (from source ending at 5.0)

        builder.build()
    }
}
