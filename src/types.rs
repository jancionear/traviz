#![allow(unused)]
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use opentelemetry_proto::tonic::common::v1::any_value::Value;

/// Seconds since epoch
/// TODO: make nicer, f64 isn't great for this
pub type TimePoint = f64;

pub fn time_point_from_unix_nano(unix_nano: u64) -> TimePoint {
    unix_nano as f64 / 1_000_000_000.0
}

pub fn time_point_to_utc_string(time: TimePoint) -> String {
    let date_time = chrono::DateTime::from_timestamp_nanos((time * 1e9) as i64);
    date_time.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

pub type HeightLevel = u64;

#[derive(Debug, Clone)]
pub struct Span {
    pub name: String,
    pub span_id: Vec<u8>,
    pub trace_id: Vec<u8>,
    pub parent_span_id: Vec<u8>,
    pub start_time: TimePoint,
    pub end_time: TimePoint,
    pub attributes: BTreeMap<String, Option<Value>>,
    pub events: Vec<Event>,
    pub node: Rc<Node>,
    pub scope: Option<Rc<Scope>>,

    pub children: RefCell<Vec<Rc<Span>>>,
    pub min_start_time: Cell<TimePoint>,
    pub max_end_time: Cell<TimePoint>,

    pub display_options: SpanDisplayConfig,
    pub collapse_children: Cell<bool>,

    // How much to offset from the parent when displaying the span
    pub parent_height_offset: Cell<HeightLevel>,
    pub display_start: Cell<f32>,
    pub display_length: Cell<f32>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub name: String,
    pub time: TimePoint,
    pub attributes: BTreeMap<String, Option<Value>>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub attributes: BTreeMap<String, Option<Value>>,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
    pub version: String,
    pub attributes: BTreeMap<String, Option<Value>>,
}

#[derive(Debug, Clone, Copy)]
pub struct SpanDisplayConfig {
    pub display_length: DisplayLength,
}

#[derive(Debug, Clone, Copy)]
pub enum DisplayLength {
    /// Span is displayed from start time to end time, length is equal to length of the interval
    Time,
    /// Displayed span is always long enough to fully display span name. Span starts at `start_time` and is long enough to display the name.
    Text,
}

pub fn value_to_text(value_opt: &Option<Value>) -> String {
    let Some(value) = value_opt else {
        return "empty".to_string();
    };

    match value {
        Value::StringValue(s) => s.clone(),
        Value::BoolValue(b) => b.to_string(),
        Value::IntValue(i) => i.to_string(),
        Value::DoubleValue(d) => d.to_string(),
        Value::ArrayValue(a) => format!(
            "[{}]",
            a.values
                .iter()
                .map(|v| value_to_text(&v.value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::KvlistValue(kv) => format!(
            "{{{}}}",
            kv.values
                .iter()
                .map(|v| format!(
                    "{}: {}",
                    v.key,
                    value_to_text(match &v.value {
                        Some(opt) => &opt.value,
                        None => &None,
                    })
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::BytesValue(b) => format!("{:?}", b), // TODO - hex? base64? maximum length?
    }
}

/// Convert span to a string that can be used for text-based search/filtering etc.
/// Not necessarily human readable.
pub fn stringify_span(span: &Rc<Span>, include_children: bool) -> String {
    let mut s = format!(
        "Span {{\n name: {}\n, span_id: {:x?}\n, trace_id: {:x?}\n, parent_span_id: {:x?}\n, start_time: {}\n, end_time: {}\n, attributes: {}\n",
        span.name,
        span.span_id,
        span.trace_id,
        span.parent_span_id,
        time_point_to_utc_string(span.start_time),
        time_point_to_utc_string(span.end_time),
        stringify_attributes(&span.attributes),
    );

    s.push_str(" events: [");
    for event in &span.events {
        s.push_str(&format!(
            "\n  event {} {}\n attributes: {}",
            event.name,
            time_point_to_utc_string(event.time),
            stringify_attributes(&event.attributes),
        ));
    }
    s.push_str("],\n");

    s.push_str(&format!(
        " node: Node {{\n  name: {}\n  attributes: {}\n }},\n",
        span.node.name,
        stringify_attributes(&span.node.attributes),
    ));
    // scope
    s.push_str(&format!(
        " scope: {:?},\n",
        span.scope.as_ref().map(|scope| {
            format!(
                "Scope {{\n  name: {}\n  version: {}\n  attributes: {}\n }}",
                scope.name,
                scope.version,
                stringify_attributes(&scope.attributes),
            )
        })
    ));

    if include_children {
        s.push_str(" children: [");
        for child in span.children.borrow().iter() {
            s.push_str(&format!("\n  {}", stringify_span(child, true)));
        }
        s.push_str("],\n");
    }

    s
}

pub fn stringify_attributes(attributes: &BTreeMap<String, Option<Value>>) -> String {
    let mut s = "{".to_string();
    for (key, value) in attributes {
        s.push_str(&format!("\n {} = {},", key, value_to_text(value)));
    }
    s.push('}');
    s
}
