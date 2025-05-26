//! This is the module that defines Display modes available in Traviz.
//! Each "Mode" is a function which takes the raw trace data and prepares the spans that should be displayed.
//! They can filter, modify, transform, re-arrange the spans as needed for each mode.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::Result;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;

use crate::structured_modes::{self, StructuredMode};
use crate::task_timer::TaskTimer;
use crate::types::{
    time_point_from_unix_nano, value_to_text, DisplayLength, Event, Node, Scope, Span,
    SpanDisplayConfig,
};

#[allow(unused)]
pub type DisplayModeTransform = Box<dyn Fn(&[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>>>;

#[allow(unused)]
pub struct DisplayMode {
    pub name: String,
    /// Function which takes raw data and outputs spans that should be displayed.
    pub transformation: DisplayModeTransform,
}

#[allow(unused)]
/// Get all of the built-in display modes.
pub fn get_all_modes() -> Vec<DisplayMode> {
    // Include all structured modes in the display modes.
    // For now those are all of the modes, but in the future we may add more.
    let structured_modes: Vec<DisplayMode> = structured_modes::get_all_structured_modes()
        .into_iter()
        .map(|mode| DisplayMode {
            name: mode.name.clone(),
            transformation: Box::new(move |trace_data| {
                structured_mode_transformation(trace_data, &mode)
            }),
        })
        .collect();
    structured_modes
}

/// Perform the transformation to the spans based on the structured mode.
/// - for each span hierarchy, find the first (highest) span that should be visible and make it a
///   top level span, discard the ones above it.
/// - collapse spans that are not visible.
/// - modify each span as needed - replace name, add height to name, set display length, etc.
pub fn structured_mode_transformation(
    trace_data: &[ExportTraceServiceRequest],
    structured_mode: &StructuredMode,
) -> Result<Vec<Rc<Span>>> {
    let all_spans = extract_spans(trace_data)?;
    let mut new_spans = Vec::new();

    for span in all_spans {
        structured_mode_transformation_rek(structured_mode, &span, &mut new_spans, false);
    }

    Ok(new_spans)
}

fn structured_mode_transformation_rek(
    mode: &StructuredMode,
    span: &Rc<Span>,
    visible_top_level_spans: &mut Vec<Rc<Span>>,
    under_visible_top_level_span: bool,
) -> Option<Rc<Span>> {
    let decision = mode.get_decision_for_span(span);

    if !decision.visible && !under_visible_top_level_span {
        for child in span.children.borrow().iter() {
            structured_mode_transformation_rek(mode, child, visible_top_level_spans, false);
        }
        return None;
    }

    let taken_children = std::mem::take(&mut *span.children.borrow_mut());

    let mut modified_span: Span = (**span).clone();
    if decision.visible {
        modified_span.dont_collapse_this_span.set(true);
        modified_span.collapse_children.set(true);
    }
    if !decision.replace_name.is_empty() {
        modified_span.name = decision.replace_name;
    }
    if decision.add_height_to_name {
        add_height_to_name(&mut modified_span);
    }
    if decision.add_shard_id_to_name {
        add_shard_id_to_name(&mut modified_span);
    }
    modified_span.display_options.display_length = decision.display_length;

    let mut new_children = Vec::new();
    for child in taken_children.iter() {
        if let Some(new_child) =
            structured_mode_transformation_rek(mode, child, visible_top_level_spans, true)
        {
            new_children.push(new_child);
        }
    }
    modified_span.children = RefCell::new(new_children);

    if !under_visible_top_level_span {
        visible_top_level_spans.push(Rc::new(modified_span));
        None
    } else {
        Some(Rc::new(modified_span))
    }
}

pub fn add_height_to_name(s: &mut Span) {
    if let Some(val) = s.attributes.get("height") {
        s.name = format!("{} H={}", s.name, value_to_text(val));
    }
    if let Some(val) = s.attributes.get("block_height") {
        s.name = format!("{} H={}", s.name, value_to_text(val));
    }
    if let Some(val) = s.attributes.get("height_created") {
        s.name = format!("{} HC={}", s.name, value_to_text(val));
    }
    if let Some(val) = s.attributes.get("next_height") {
        s.name = format!("{} NH={}", s.name, value_to_text(val));
    }
}

pub fn add_shard_id_to_name(s: &mut Span) {
    if let Some(val) = s.attributes.get("shard_id") {
        s.name = format!("{} s={}", s.name, value_to_text(val));
    }
}

// Parse the raw OTel data into a tree of spans/
fn extract_spans(requests: &[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>> {
    let t = TaskTimer::new("Extracting spans");

    let mut spans_by_id = BTreeMap::new();
    for request in requests {
        for rs in &request.resource_spans {
            let resource = match &rs.resource {
                Some(r) => {
                    let mut attributes = BTreeMap::new();
                    for attribute in &r.attributes {
                        attributes.insert(
                            attribute.key.clone(),
                            attribute.value.clone().and_then(|v| v.value),
                        );
                    }

                    let name = match attributes.get("service.name") {
                        Some(Some(Value::StringValue(service_name))) => service_name.clone(),
                        _ => "unknown".to_string(),
                    };

                    Rc::new(Node { name, attributes })
                }
                None => Rc::new(Node {
                    name: "no resource".to_string(),
                    attributes: BTreeMap::new(),
                }),
            };

            for ss in &rs.scope_spans {
                let scope = ss.scope.as_ref().map(|s| {
                    let mut attributes = BTreeMap::new();
                    for attribute in &s.attributes {
                        attributes.insert(
                            attribute.key.clone(),
                            attribute.value.clone().and_then(|v| v.value),
                        );
                    }
                    Rc::new(Scope {
                        name: s.name.clone(),
                        version: s.version.clone(),
                        attributes,
                    })
                });

                for span in &ss.spans {
                    let mut attributes = BTreeMap::new();
                    for attribute in &span.attributes {
                        attributes.insert(
                            attribute.key.clone(),
                            attribute.value.clone().and_then(|v| v.value),
                        );
                    }

                    let start_time = time_point_from_unix_nano(span.start_time_unix_nano);
                    let end_time = time_point_from_unix_nano(span.end_time_unix_nano);

                    let mut events = vec![];
                    for event in &span.events {
                        let mut attributes = BTreeMap::new();
                        for attribute in &event.attributes {
                            attributes.insert(
                                attribute.key.clone(),
                                attribute.value.clone().and_then(|v| v.value),
                            );
                        }

                        events.push(Event {
                            name: event.name.clone(),
                            time: time_point_from_unix_nano(event.time_unix_nano),
                            attributes,
                        });
                    }

                    spans_by_id.insert(
                        span.span_id.clone(),
                        Rc::new(Span {
                            name: span.name.clone(),
                            span_id: span.span_id.clone(),
                            trace_id: span.trace_id.clone(),
                            parent_span_id: span.parent_span_id.clone(),
                            start_time,
                            end_time,
                            attributes,
                            events,
                            node: resource.clone(),
                            scope: scope.clone(),
                            children: RefCell::new(Vec::new()),
                            display_children: RefCell::new(Vec::new()),
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
                        }),
                    );
                }
            }
        }
    }

    let mut top_level_spans = vec![];
    for span in spans_by_id.values() {
        if let Some(parent_span) = spans_by_id.get(&span.parent_span_id) {
            parent_span.children.borrow_mut().push(span.clone());
        } else {
            top_level_spans.push(span.clone());
        }
    }

    t.stop();

    Ok(top_level_spans)
}
