// Create spans
// Create relations
// Assign span times

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use opentelemetry_proto::tonic::common::v1::any_value::Value;
use uuid::Uuid;

use crate::builtin_relations::builtin_relations;
use crate::relation::{find_relations, Relation, RelationView};
use crate::types::{DisplayLength, SpanDisplayConfig};
use crate::{Node, Span};

struct SpanBuilder {
    name: String,
    node: String,
    attributes: Vec<(String, String)>,
    length: Duration,
}

impl SpanBuilder {
    fn new(name: impl Into<String>, node: impl Into<String>, length: Duration) -> Self {
        SpanBuilder {
            name: name.into(),
            node: node.into(),
            attributes: Vec::new(),
            length,
        }
        .with_attribute("tag_block_production", true) // enable tag_block_production to be able to use the display mode
    }

    fn with_attribute(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.attributes.push((key.to_string(), value.to_string()));
        self
    }

    fn build(self) -> Span {
        let span_id = Uuid::new_v4().as_bytes().to_vec();
        let trace_id = Uuid::new_v4().as_bytes().to_vec();

        let node = Rc::new(Node {
            name: self.node.clone(),
            attributes: BTreeMap::new(),
        });

        Span {
            name: self.name.clone(),
            original_name: self.name,
            span_id,
            trace_id,
            parent_span_id: Vec::new(),
            start_time: Self::default_start_time(),
            end_time: Self::default_start_time() + self.length.as_secs_f64(),
            attributes: self
                .attributes
                .into_iter()
                .map(|(attr, value)| (attr, Some(Value::StringValue(value))))
                .collect(),
            events: Vec::new(),
            node,
            scope: None,
            children: RefCell::new(Vec::new()),
            display_children: RefCell::new(Vec::new()),
            min_start_time: Cell::new(0.0),
            max_end_time: Cell::new(0.0),
            display_options: SpanDisplayConfig {
                display_length: DisplayLength::Text,
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
        }
    }

    fn default_start_time() -> f64 {
        1.0
    }
}

const PRODUCE_BLOCK_NAME: &str = "produce_block_on_head";
const PRODUCE_BLOCK_TIME: Duration = Duration::from_millis(20);

const PREPROCESS_BLOCK_NAME: &str = "preprocess_block";
const PREPROCESS_BLOCK_TIME: Duration = Duration::from_millis(100);

const POSTPROCESS_BLOCK_NAME: &str = "postprocess_ready_block";
const POSTPROCESS_BLOCK_TIME: Duration = Duration::from_millis(100);

const PRODUCE_OPTIMISTIC_BLOCK_NAME: &str = "produce_optimistic_block_on_head";
const PRODUCE_OPTIMISTIC_BLOCK_TIME: Duration = Duration::from_millis(20);

const PROCESS_OPTIMISTIC_BLOCK_NAME: &str = "process_optimistic_block";
const PROCESS_OPTIMISTIC_BLOCK_TIME: Duration = Duration::from_millis(20);

const APPLY_CHUNK_NAME: &str = "apply_new_chunk";
const APPLY_CHUNK_TIME: Duration = Duration::from_millis(400);

const PRODUCE_CHUNK_NAME: &str = "produce_chunk";
const PRODUCE_CHUNK_TIME: Duration = Duration::from_millis(50);

const SEND_CHUNK_ENDORSEMENT_NAME: &str = "send_chunk_endorsement";
const SEND_CHUNK_ENDORSEMENT_TIME: Duration = Duration::from_millis(20);

const SEND_CHUNK_STATE_WITNESS_NAME: &str = "send_chunk_state_witness";
const SEND_CHUNK_STATE_WITNESS_TIME: Duration = Duration::from_millis(150);

const NODE_BLOCK_PRODUCER_NAME: &str = "block_producer";
const NODE_CHUNK_PRODUCER_NAME: &str = "chunk_producer";
const NODE_CHUNK_VALIDATOR_NAME: &str = "chunk_validator";

pub fn theory1() -> Vec<Span> {
    let mut spans = Vec::new();

    for height in 0..5 {
        // produce block
        spans.push(
            SpanBuilder::new(
                PRODUCE_BLOCK_NAME,
                NODE_BLOCK_PRODUCER_NAME,
                PRODUCE_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );

        // preprocess block
        spans.push(
            SpanBuilder::new(
                PREPROCESS_BLOCK_NAME,
                NODE_BLOCK_PRODUCER_NAME,
                PREPROCESS_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );
        spans.push(
            SpanBuilder::new(
                PREPROCESS_BLOCK_NAME,
                NODE_CHUNK_PRODUCER_NAME,
                PREPROCESS_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );
        spans.push(
            SpanBuilder::new(
                PREPROCESS_BLOCK_NAME,
                NODE_CHUNK_VALIDATOR_NAME,
                PREPROCESS_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );

        // postprocess block
        spans.push(
            SpanBuilder::new(
                POSTPROCESS_BLOCK_NAME,
                NODE_BLOCK_PRODUCER_NAME,
                POSTPROCESS_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );
        spans.push(
            SpanBuilder::new(
                POSTPROCESS_BLOCK_NAME,
                NODE_CHUNK_PRODUCER_NAME,
                POSTPROCESS_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );
        spans.push(
            SpanBuilder::new(
                POSTPROCESS_BLOCK_NAME,
                NODE_CHUNK_VALIDATOR_NAME,
                POSTPROCESS_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );

        // apply chunk (validate witness)
        spans.push(
            SpanBuilder::new(
                APPLY_CHUNK_NAME,
                NODE_CHUNK_VALIDATOR_NAME,
                APPLY_CHUNK_TIME,
            )
            .with_attribute("height", height)
            .with_attribute("shard_id", 0)
            .with_attribute("apply_reason", "ValidateChunkStateWitness")
            .with_attribute("block_type", "Normal")
            .build(),
        );

        if height == 0 {
            continue;
        }

        // produce optimistic block
        spans.push(
            SpanBuilder::new(
                PRODUCE_OPTIMISTIC_BLOCK_NAME,
                NODE_BLOCK_PRODUCER_NAME,
                PRODUCE_OPTIMISTIC_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );

        // process optimistic block
        spans.push(
            SpanBuilder::new(
                PROCESS_OPTIMISTIC_BLOCK_NAME,
                NODE_CHUNK_PRODUCER_NAME,
                PROCESS_OPTIMISTIC_BLOCK_TIME,
            )
            .with_attribute("height", height)
            .build(),
        );

        // produce chunk
        spans.push(
            SpanBuilder::new(
                PRODUCE_CHUNK_NAME,
                NODE_CHUNK_PRODUCER_NAME,
                PRODUCE_CHUNK_TIME,
            )
            .with_attribute("height", height)
            .with_attribute("shard_id", 0)
            .build(),
        );

        // apply chunk (optimistic)
        spans.push(
            SpanBuilder::new(APPLY_CHUNK_NAME, NODE_CHUNK_PRODUCER_NAME, APPLY_CHUNK_TIME)
                .with_attribute("height", height)
                .with_attribute("shard_id", 0)
                .with_attribute("apply_reason", "UpdateTrackedShard")
                .with_attribute("block_type", "Optimistic")
                .build(),
        );

        // send chunk state witness
        spans.push(
            SpanBuilder::new(
                SEND_CHUNK_STATE_WITNESS_NAME,
                NODE_CHUNK_PRODUCER_NAME,
                SEND_CHUNK_STATE_WITNESS_TIME,
            )
            .with_attribute("height", height)
            .with_attribute("shard_id", 0)
            .build(),
        );

        // send chunk endorsement
        spans.push(
            SpanBuilder::new(
                SEND_CHUNK_ENDORSEMENT_NAME,
                NODE_CHUNK_VALIDATOR_NAME,
                SEND_CHUNK_ENDORSEMENT_TIME,
            )
            .with_attribute("height", height)
            .with_attribute("shard_id", 0)
            .build(),
        );
    }

    let relations = builtin_relations();

    let spans_with_times = set_span_times_from_relations(spans, relations);

    // There are a few spans at the end without dependencies, which causes them to end up at the front.
    // Delete them.
    let result = spans_with_times
        .into_iter()
        .filter(|s| {
            if let Some(Some(Value::StringValue(height_str))) = s.attributes.get("height") {
                if height_str != "0" && s.start_time == SpanBuilder::default_start_time() {
                    return false;
                }
            }
            true
        })
        .collect();

    result
}

// Find relations between the spans and set their start time so that they are ordered by their relations
fn set_span_times_from_relations(mut spans: Vec<Span>, mut relations: Vec<Relation>) -> Vec<Span> {
    for relation in &mut relations {
        relation.min_time_diff = -10.0;
    }
    let relation_view = RelationView {
        enabled_relations: relations.iter().map(|r| r.id).collect(),
        name: "tmp".to_string(),
        is_builtin: false,
    };
    let rcd_spans = spans.iter().cloned().map(Rc::new).collect::<Vec<_>>();
    let active_relations = find_relations(&relations, &relation_view, &rcd_spans);

    let span_id_to_index = spans
        .iter()
        .enumerate()
        .map(|(i, span)| (span.span_id.clone(), i))
        .collect::<BTreeMap<_, _>>();

    let mut outgoing_relations: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

    for relation in active_relations.iter() {
        let from_index = span_id_to_index
            .get(&relation.from_span.upgrade().unwrap().span_id)
            .unwrap();
        let to_index = span_id_to_index
            .get(&relation.to_span.upgrade().unwrap().span_id)
            .unwrap();
        outgoing_relations
            .entry(*from_index)
            .or_default()
            .push(*to_index);
    }

    let mut was_update = true;
    while was_update {
        was_update = false;
        for cur_span_idx in 0..spans.len() {
            for &other_span_idx in outgoing_relations.get(&cur_span_idx).unwrap_or(&Vec::new()) {
                if spans[other_span_idx].start_time < spans[cur_span_idx].end_time {
                    let other_span_len =
                        spans[other_span_idx].end_time - spans[other_span_idx].start_time;
                    spans[other_span_idx].start_time = spans[cur_span_idx].end_time;
                    spans[other_span_idx].end_time =
                        spans[other_span_idx].start_time + other_span_len;
                    was_update = true;
                }
            }
        }
    }
    spans
}
