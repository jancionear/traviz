use std::collections::HashMap;
use std::rc::{Rc, Weak};

use sha2::Digest;
use uuid::Uuid;

use crate::builtin_relations;
use crate::structured_modes::SpanSelector;
use crate::task_timer::TaskTimer;
use crate::types::{value_to_text, Span};

pub fn make_uuid_from_seed(seed: &str) -> Uuid {
    let digest_bytes: [u8; 32] = sha2::Sha256::digest(seed).into();
    let uuid_bytes: [u8; 16] = digest_bytes[0..16].try_into().unwrap();
    Uuid::from_bytes(uuid_bytes)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Relation {
    pub id: Uuid,

    pub name: String,
    pub description: String,
    pub from_span_selector: SpanSelector,
    pub to_span_selector: SpanSelector,
    pub attribute_relations: Vec<AttributeRelation>,
    pub max_time_diff: Option<f64>,

    pub nodes_config: RelationNodesConfig,
    pub match_type: MatchType,

    pub is_builtin: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttributeRelation {
    pub from_attribute: String,
    pub to_attribute: String,
    pub relation: AttributeRelationOp,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AttributeRelationOp {
    Equal,
    OneGreater,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelationNodesConfig {
    SameNode,
    DifferentNode,
    AllNodes,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MatchType {
    MatchAll,
    MatchClosest,
}

impl Relation {
    pub fn matches(&self, from_span: &Span, to_span: &Span) -> bool {
        if !self.from_span_selector.matches(from_span) {
            return false;
        }
        if !self.to_span_selector.matches(to_span) {
            return false;
        }

        for attribute_relation in &self.attribute_relations {
            if !attribute_relation.matches(from_span, to_span) {
                return false; // If any attribute relation does not match, the relation does not match
            }
        }

        match &self.nodes_config {
            RelationNodesConfig::SameNode => {
                if from_span.node.name != to_span.node.name {
                    return false; // Spans must be in the same node
                }
            }
            RelationNodesConfig::DifferentNode => {
                if from_span.node.name == to_span.node.name {
                    return false; // Spans must be in different nodes
                }
            }
            RelationNodesConfig::AllNodes => {
                // No restriction on nodes
            }
        }

        true
    }
}

impl AttributeRelation {
    fn matches(&self, from_span: &Span, to_span: &Span) -> bool {
        let Some(from_value) = from_span.attributes.get(&self.from_attribute) else {
            return false;
        };
        let Some(to_value) = to_span.attributes.get(&self.to_attribute) else {
            return false;
        };

        match self.relation {
            AttributeRelationOp::Equal => value_to_text(from_value) == value_to_text(to_value),
            AttributeRelationOp::OneGreater => {
                if let (Ok(from_num), Ok(to_num)) = (
                    value_to_text(from_value).parse::<i64>(),
                    value_to_text(to_value).parse::<i64>(),
                ) {
                    from_num.checked_add(1) == Some(to_num)
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationView {
    pub enabled_relations: Vec<Uuid>,
    pub name: String,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationViews {
    pub views: Vec<RelationView>,
}

#[derive(Debug, Clone)]
pub struct RelationInstance {
    pub from_span: Weak<Span>,
    pub to_span: Weak<Span>,
    #[allow(unused)]
    pub relation: Rc<Relation>,
}

pub fn find_relations(
    all_relations: &[Relation],
    view: &RelationView,
    spans: &[Rc<Span>],
) -> Vec<RelationInstance> {
    #[cfg(feature = "profiling")]
    let _timing_guard = crate::profiling::GLOBAL_PROFILER.start_timing("find_relations");

    let task_timer = TaskTimer::new("Finding relations");

    let mut res = Vec::new();

    // Spans grouped by name, sorted by start time.
    let mut spans_by_name: HashMap<String, Vec<Rc<Span>>> = HashMap::new();
    for span in spans {
        gather_spans_by_name(span, &mut spans_by_name);
    }
    for (_name, spans) in spans_by_name.iter_mut() {
        // Sort spans by start time
        spans.sort_by(|a, b| {
            a.start_time
                .partial_cmp(&b.start_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Clear outgoing and incoming relations for each span
        for span in spans {
            span.outgoing_relations.borrow_mut().clear();
            span.incoming_relations.borrow_mut().clear();
        }
    }

    for enabled_relation_id in &view.enabled_relations {
        let Some(relation) = all_relations.iter().find(|r| &r.id == enabled_relation_id) else {
            continue;
        };
        let relation = Rc::new(relation.clone());

        let matching_from_span_names = spans_by_name
            .keys()
            .filter(|name| {
                relation
                    .from_span_selector
                    .span_name_condition
                    .matches(name)
            })
            .collect::<Vec<_>>();
        let matching_to_span_names = spans_by_name
            .keys()
            .filter(|name| relation.to_span_selector.span_name_condition.matches(name))
            .collect::<Vec<_>>();

        for from_span_name in &matching_from_span_names {
            let Some(from_spans) = spans_by_name.get(from_span_name.as_str()) else {
                continue;
            };
            for to_span_name in &matching_to_span_names {
                let Some(to_spans) = spans_by_name.get(to_span_name.as_str()) else {
                    continue;
                };

                for from_span in from_spans {
                    let first_to_span_index = find_first_span_after(to_spans, from_span.end_time);
                    for to_span in &to_spans[first_to_span_index..] {
                        if let Some(max_time_diff) = relation.max_time_diff {
                            if to_span.start_time - from_span.start_time > max_time_diff {
                                break;
                            }
                        }

                        if !relation.matches(from_span, to_span) {
                            continue;
                        }

                        let instance = RelationInstance {
                            from_span: Rc::<Span>::downgrade(from_span),
                            to_span: Rc::<Span>::downgrade(to_span),
                            relation: relation.clone(),
                        };

                        from_span
                            .outgoing_relations
                            .borrow_mut()
                            .push(instance.clone());
                        to_span
                            .incoming_relations
                            .borrow_mut()
                            .push(instance.clone());
                        res.push(instance);

                        match relation.match_type {
                            MatchType::MatchAll => {
                                // For MatchAll, we continue to find more matches
                                continue;
                            }
                            MatchType::MatchClosest => {
                                // For MatchClosest, we break after the first match
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    task_timer.stop();
    println!("Found {} relations", res.len());

    res
}

fn gather_spans_by_name(span: &Rc<Span>, spans_by_name: &mut HashMap<String, Vec<Rc<Span>>>) {
    spans_by_name
        .entry(span.original_name().to_string())
        .or_default()
        .push(span.clone());
    for child in span.children.borrow().iter() {
        gather_spans_by_name(child, spans_by_name);
    }
}

fn find_first_span_after(spans: &[Rc<Span>], start_time: f64) -> usize {
    // TODO - this could be optimized with binary search
    spans
        .iter()
        .position(|span| span.start_time >= start_time)
        .unwrap_or(spans.len())
}

pub fn builtin_relation_views() -> Vec<RelationView> {
    vec![
        RelationView {
            name: "No relations".to_string(),
            enabled_relations: vec![],
            is_builtin: true,
        },
        RelationView {
            name: "Pre-Post Process Block".to_string(),
            enabled_relations: vec![
                crate::builtin_relations::preprocess_block_to_postprocess_ready_block_relation().id,
            ],
            is_builtin: true,
        },
        RelationView {
            name: "Send-Receive Witness".to_string(),
            enabled_relations: vec![builtin_relations::send_chunk_state_witness_to_validate_chunk_state_witness_relation().id],
            is_builtin: true,
        },
        RelationView {
            name: "Send-Validate Chunk Endorsement".to_string(),
            enabled_relations: vec![builtin_relations::send_chunk_endorsement_to_validate_chunk_endorsement_relation().id],
            is_builtin: true,
        },
        RelationView {
            name: "Block production without witness and endorsement distribution".to_string(),
            enabled_relations: vec![
                builtin_relations::produce_block_on_head_to_preprocess_block_relation().id,
                builtin_relations::preprocess_block_to_postprocess_ready_block_relation().id,
                builtin_relations::postprocess_ready_block_to_produce_block_on_head_relation().id,
                builtin_relations::postprocess_ready_block_to_next_preprocess_block_relation().id,
                builtin_relations::preprocess_block_to_apply_new_chunk_relation().id,
                builtin_relations::apply_new_chunk_normal_to_postprocess_ready_block_relation().id,
                builtin_relations::apply_new_chunk_optimistic_to_postprocess_ready_block_relation().id,
                builtin_relations::postprocess_ready_block_to_produce_chunk_relation().id,
                builtin_relations::produce_chunk_to_send_chunk_state_witness_relation().id,
                builtin_relations::validate_chunk_state_witness_to_send_chunk_endorsement_relation().id,
                builtin_relations::validate_chunk_endorsement_to_produce_block_on_head_relation().id,
                builtin_relations::postprocess_ready_block_to_produce_optimistic_block_on_head_relation().id,
                builtin_relations::produce_optimistic_block_on_head_to_process_optimistic_block_relation().id,
                builtin_relations::process_optimistic_block_to_apply_new_chunk_optimistic_relation().id,
            ],
            is_builtin: true,
        },
        RelationView {
            name: "All builtin Relations".to_string(),
            enabled_relations: builtin_relations::builtin_relations()
                .iter()
                .map(|r| r.id)
                .collect(),
            is_builtin: true,
        },
    ]
}
