//! This module contains different modes for displaying the trace data.
//! Each "Mode" is a function which takes the raw trace data and prepares the spans that should be displayed.
//! They can filter, modify, or re-arrange the spans as needed for each mode.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::Result;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;

use crate::task_timer::TaskTimer;
use crate::types::{
    stringify_span, time_point_from_unix_nano, value_to_text, DisplayLength, Event, Node, Scope,
    Span, SpanDisplayConfig,
};

pub fn everything_mode(trace_data: &[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>> {
    let spans = extract_spans(trace_data)?;
    // Shorten the name to make arranging more efficient in everything mode.
    // This is a bit of a hack, but that's what we have for now.
    let spans = map_spans(spans.as_slice(), &|mut s: Span| -> Span {
        if s.name == "validate_chunk_endorsement" {
            s.name = "VCE".to_string();
        }
        s.attributes.insert(
            "full-name".to_string(),
            Some(Value::StringValue("validate_chunk_endorsement".to_string())),
        );
        s
    });
    Ok(spans)
}

pub fn doomslug_mode(trace_data: &[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>> {
    let all_spans = extract_spans(trace_data)?;
    let mut new_spans = Vec::new();
    get_doomslug_spans(&all_spans, &mut new_spans);
    Ok(map_spans(&new_spans, &|mut s: Span| -> Span {
        s.display_options.display_length = DisplayLength::Text;

        if s.name.contains("set_tip") {
            if let Some(val) = s.attributes.get("block_height") {
                s.name = format!("{}({})", s.name, value_to_text(val));
            }
        }

        if s.name.contains("on_approval_message") {
            if let Some(val) = s.attributes.get("msg") {
                let t = value_to_text(val);

                if t.starts_with("Endorsement") {
                    if let Some(target_height) = s.attributes.get("target_height") {
                        s.name = format!(
                            "on_approval(Endorse(target_height={}))",
                            value_to_text(target_height)
                        );
                    }
                };
            }
        }

        s
    }))
}

fn is_doomslug_span(span: &Span) -> bool {
    let stringified = stringify_span(&Rc::new(span.clone()), false);
    stringified.contains("doomslug")
        || stringified.contains("Doomslug")
        || span.name.contains("produce_block")
}

fn get_doomslug_spans(spans: &[Rc<Span>], res: &mut Vec<Rc<Span>>) {
    for span in spans {
        if is_doomslug_span(span) {
            let mut doomslug_children = Vec::new();
            get_doomslug_spans(&span.children.borrow(), &mut doomslug_children);
            let mut new_span = (**span).clone();
            new_span.children = RefCell::new(doomslug_children);
            res.push(Rc::new(new_span));
        } else {
            get_doomslug_spans(&span.children.borrow(), res);
        }
    }
}

pub fn chain_mode(trace_data: &[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>> {
    chain_mode_one_shard(trace_data, None)
}

pub fn chain_shard0_mode(trace_data: &[ExportTraceServiceRequest]) -> Result<Vec<Rc<Span>>> {
    chain_mode_one_shard(trace_data, Some("0".to_string()))
}

// Chain mode, optionally filtered down to one shard
pub fn chain_mode_one_shard(
    trace_data: &[ExportTraceServiceRequest],
    one_shard: Option<String>,
) -> Result<Vec<Rc<Span>>> {
    let important_chain_spans = [
        "preprocess_optimistic_block",
        "process_optimistic_block",
        "postprocess_ready_block",
        "postprocess_optimistic_block",
        "preprocess_block",
        "apply_new_chunk",
        "apply_old_chunk",
        "produce_chunk_internal",
        "produce_block_on",
        "receive_optimistic_block",
        "validate_chunk_state_witness",
        "send_chunk_state_witness",
        "produce_optimistic_block_on_head",
        "validate_chunk_endorsement",
        "on_approval_message",
    ];

    let all_spans = extract_spans(trace_data)?;

    let is_important = |span: &Span| {
        if !important_chain_spans.contains(&span.name.as_str()) {
            return false;
        }

        if let Some(filter_shard) = &one_shard {
            if let Some(val) = span.attributes.get("shard_id") {
                if &value_to_text(val) != filter_shard {
                    return false;
                }
            }
        }

        true
    };

    let res = retain_important(all_spans, &is_important);
    let res = add_height_shard_id_to_name(res);

    Ok(res)
}

pub fn retain_important(
    spans: Vec<Rc<Span>>,
    is_important: &dyn Fn(&Span) -> bool,
) -> Vec<Rc<Span>> {
    let mut res = Vec::new();
    for span in spans {
        retain_important_rek(&span, is_important, &mut res);
    }

    for span in &res {
        collapse_unimportant(span, is_important);
    }

    res
}

fn retain_important_rek(
    span: &Rc<Span>,
    is_important: &dyn Fn(&Span) -> bool,
    res: &mut Vec<Rc<Span>>,
) {
    if is_important(span) {
        res.push(span.clone());
    } else {
        for child in span.children.borrow().iter() {
            retain_important_rek(child, is_important, res);
        }
    }
}

fn collapse_unimportant(span: &Span, is_important: &dyn Fn(&Span) -> bool) -> bool {
    let mut found_important = false;

    if is_important(span) {
        found_important = true;
        span.dont_collapse_this_span.set(true);
        span.collapse_children.set(true);
    }

    for child in span.children.borrow().iter() {
        if collapse_unimportant(child, is_important) {
            found_important = true;
        }
    }

    found_important
}

pub fn add_height_shard_id_to_name(spans: Vec<Rc<Span>>) -> Vec<Rc<Span>> {
    map_spans(&spans, &|mut s: Span| {
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
        if let Some(val) = s.attributes.get("shard_id") {
            s.name = format!("{} s={}", s.name, value_to_text(val));
        }
        s.display_options.display_length = DisplayLength::Text;
        s
    })
}

#[must_use]
pub fn map_spans(spans: &[Rc<Span>], f: &dyn Fn(Span) -> Span) -> Vec<Rc<Span>> {
    spans
        .iter()
        .map(|span| Rc::new(map_span((**span).clone(), f)))
        .collect()
}

#[must_use]
pub fn map_span(span: Span, f: &dyn Fn(Span) -> Span) -> Span {
    let mut new_span = f(span);

    let mut new_children = Vec::new();
    for child in new_span.children.borrow().iter() {
        new_children.push(Rc::new(map_span((**child).clone(), f)));
    }

    new_span.children = RefCell::new(new_children);

    new_span
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
