//! This is the module that defines Display modes available in Traviz.
//! Each "Mode" is a function which takes the raw trace data and prepares the spans that should be displayed.
//! They can filter, modify, transform, re-arrange the spans as needed for each mode.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use anyhow::Result;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;

use crate::structured_modes::{self, StructuredMode};
use crate::task_timer::TaskTimer;
use crate::types::{
    time_point_from_unix_nano, time_point_to_utc_string, value_to_text, DisplayLength, Event, Node,
    Scope, Span, SpanDisplayConfig, MILLISECONDS_PER_SECOND,
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
    let structured_modes: Vec<DisplayMode> = structured_modes::builtin_structured_modes()
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

    // Only apply grouping if any rule uses it
    if structured_mode
        .span_rules
        .iter()
        .any(|rule| rule.decision.group)
    {
        apply_grouping(&mut new_spans);
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

    let will_change_name = !decision.replace_name.is_empty()
        || decision.add_height_to_name
        || decision.add_shard_id_to_name;
    if will_change_name {
        modified_span.attributes.insert(
            "original.span.name".to_string(),
            Some(Value::StringValue(modified_span.name.clone())),
        );
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
    add_part_ord_to_name(&mut modified_span);
    modified_span.display_options.display_length = decision.display_length;

    // Optionally mark span for grouping
    if decision.group {
        modified_span.active_segments = Some(Vec::new());
    }

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

// TODO - make this configurable like shard_id and height. Maybe make name modification more generic.
pub fn add_part_ord_to_name(s: &mut Span) {
    if let Some(val) = s.attributes.get("part_ord") {
        s.name = format!("{} p={}", s.name, value_to_text(val));
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
                            original_name: span.name.clone(),
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

                            incoming_relations: RefCell::new(Vec::new()),
                            outgoing_relations: RefCell::new(Vec::new()),

                            active_segments: None,
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

fn apply_grouping(spans: &mut Vec<Rc<Span>>) {
    // Collect all spans that should be grouped (including children spans)
    let mut all_groupable_spans = Vec::new();
    let mut span_locations = HashMap::new();
    collect_groupable_spans_recursive(spans, &mut all_groupable_spans, &mut span_locations, None);

    // Separate top-level spans: preserve non-groupable spans, prepare groupable spans for merging
    let mut groups: HashMap<_, Vec<Rc<Span>>> = HashMap::new();
    let mut non_grouped_top_level = Vec::new();
    for span in spans.drain(..) {
        let should_be_grouped = span
            .active_segments
            .as_ref()
            .map(|segments| segments.is_empty())
            .unwrap_or(false);

        if !should_be_grouped {
            non_grouped_top_level.push(span);
        }
    }

    // Group all collected spans by (node, height, name)
    for span in all_groupable_spans {
        let original_name = span
            .attributes
            .get("original.span.name")
            .and_then(|v| v.as_ref())
            .and_then(|v| match v {
                Value::StringValue(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| span.name.clone());

        // TODO: make grouping attribute customizable
        let height_value_opt = span.attributes.get("height").cloned().unwrap_or(None);
        let height = value_to_text(&height_value_opt);
        if height == "empty" {
            println!(
                "WARN: grouping span '{}' on node '{}' without 'height' attribute",
                original_name, span.node.name
            );
        }

        let grouping_key = (span.node.name.clone(), height, original_name);
        groups.entry(grouping_key).or_default().push(span);
    }

    // Remove grouped spans from their original locations in hierarchy
    remove_spans_recursive(&mut non_grouped_top_level, &span_locations);

    // Add non-grouped spans back
    spans.extend(non_grouped_top_level);

    // Create grouped spans and add to top level
    for ((_node_name, _height, original_name), span_group) in groups {
        if span_group.len() > 1 {
            let grouped_span = create_grouped_span(original_name, span_group);
            spans.push(Rc::new(grouped_span));
        } else {
            let span_rc = span_group.into_iter().next().unwrap();
            let mut single_span = (*span_rc).clone();
            single_span.active_segments = None;
            spans.push(Rc::new(single_span));
        }
    }
}

fn collect_groupable_spans_recursive(
    spans: &[Rc<Span>],
    collector: &mut Vec<Rc<Span>>,
    locations: &mut HashMap<Vec<u8>, Option<Vec<u8>>>, // span_id -> parent_span_id
    parent_id: Option<Vec<u8>>,
) {
    for span in spans {
        let should_be_grouped = span
            .active_segments
            .as_ref()
            .map(|segments| segments.is_empty())
            .unwrap_or(false);

        if should_be_grouped {
            collector.push(span.clone());
            locations.insert(span.span_id.clone(), parent_id.clone());
        }

        let children = span.children.borrow();
        collect_groupable_spans_recursive(
            &children,
            collector,
            locations,
            Some(span.span_id.clone()),
        );
    }
}

fn remove_spans_recursive(
    spans: &mut Vec<Rc<Span>>,
    span_locations: &HashMap<Vec<u8>, Option<Vec<u8>>>,
) {
    spans.retain(|span| {
        if span_locations.contains_key(&span.span_id) && span_locations[&span.span_id].is_none() {
            return false;
        }
        let mut children = span.children.borrow_mut();
        children.retain(|child| {
            !span_locations.contains_key(&child.span_id)
                || span_locations[&child.span_id] != Some(span.span_id.clone())
        });
        true
    });
}

fn create_grouped_span(base_name: String, spans: Vec<Rc<Span>>) -> Span {
    let span_count = spans.len();
    let min_start = spans
        .iter()
        .map(|s| s.start_time)
        .fold(f64::INFINITY, f64::min);
    let max_end = spans
        .iter()
        .map(|s| s.end_time)
        .fold(f64::NEG_INFINITY, f64::max);

    // Create active segments from individual span time ranges and merge overlapping ones
    let mut raw_segments = spans
        .iter()
        .map(|s| (s.start_time, s.end_time))
        .collect::<Vec<_>>();

    raw_segments.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Merge overlapping segments
    let mut active_segments: Vec<(f64, f64)> = Vec::new();
    for (start, end) in raw_segments {
        if let Some(last) = active_segments.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
            } else {
                active_segments.push((start, end));
            }
        } else {
            active_segments.push((start, end));
        }
    }

    // Use the first span as the base
    let base_span = &spans[0];
    let mut grouped_span = (**base_span).clone();

    grouped_span.name = format!("{base_name} (total={span_count})");
    grouped_span.start_time = min_start;
    grouped_span.end_time = max_end;
    grouped_span.min_start_time.set(min_start);
    grouped_span.max_end_time.set(max_end);
    grouped_span.active_segments = Some(active_segments);

    // Store original spans information for hover tooltip
    let spans_info = spans
        .iter()
        .map(|s| {
            format!(
                "{}: {:.3}ms [{} - {}]",
                s.name,
                (s.end_time - s.start_time) * MILLISECONDS_PER_SECOND,
                time_point_to_utc_string(s.start_time),
                time_point_to_utc_string(s.end_time)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    grouped_span.attributes.insert(
        "grouped_spans_info".to_string(),
        Some(Value::StringValue(spans_info)),
    );

    // Remove all children for now, to keep things simple
    grouped_span.children = RefCell::new(Vec::new());
    grouped_span.display_children = RefCell::new(Vec::new());

    grouped_span
}
