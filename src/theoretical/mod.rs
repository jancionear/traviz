use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use opentelemetry_proto::tonic::common::v1::any_value::Value;
use uuid::Uuid;

use crate::relation::{
    find_relations, make_uuid_from_seed, AttributeRelation, AttributeRelationOp, MatchType,
    Relation, RelationNodesConfig, RelationView,
};
use crate::structured_modes::{MatchCondition, SpanSelector};
use crate::types::{DisplayLength, SpanDisplayConfig};
use crate::{Node, Span};

pub mod models;

pub use models::all_models;

pub struct TheoreticalModel {
    pub name: String,
    pub description: String,
    spans: Vec<Span>,
    relations: Vec<Relation>,
}

impl TheoreticalModel {
    pub fn new(name: &str, description: &str) -> Self {
        TheoreticalModel {
            name: name.to_string(),
            description: description.to_string(),
            spans: Vec::new(),
            relations: Vec::new(),
        }
    }

    pub fn add_span(&mut self, span: impl Into<Span>) {
        self.spans.push(span.into());
    }

    pub fn add_relation(&mut self, relation: impl Into<Relation>) {
        let relation = relation.into();

        for rel in &self.relations {
            let mut rel_clone = rel.clone();
            rel_clone.id = relation.id.clone();
            if rel_clone == relation {
                return;
            }
        }

        self.relations.push(relation);
    }

    pub fn finalize(self) -> (Vec<Span>, Vec<Relation>) {
        let spans = set_span_times_from_relations(self.spans, self.relations.clone());
        (spans, self.relations)
    }
}

struct SpanBuilder {
    name: String,
    node: String,
    attributes: Vec<(String, String)>,
    length: Duration,
    children: Vec<Span>,
}

impl SpanBuilder {
    fn new(name: impl Into<String>, node: impl Into<String>, length: Duration) -> Self {
        SpanBuilder {
            name: name.into(),
            node: node.into(),
            attributes: Vec::new(),
            length,
            children: Vec::new(),
        }
        .with_attribute("tag_block_production", true) // enable tag_block_production to be able to use the display mode
    }

    fn with_attribute(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.attributes.push((key.to_string(), value.to_string()));
        self
    }

    // Warning - children are not taken into account when calculating relation dependencies.
    // They are only as additional information for the display.
    fn with_child(mut self, child: impl Into<Span>) -> Self {
        self.children.push(child.into());
        self
    }

    fn build(mut self) -> Span {
        let span_id = Uuid::new_v4().as_bytes().to_vec();
        let trace_id = Uuid::new_v4().as_bytes().to_vec();

        for child in &mut self.children {
            child.parent_span_id = span_id.clone();
        }

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
            children: RefCell::new(self.children.into_iter().map(Rc::new).collect()),
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

impl From<SpanBuilder> for Span {
    fn from(builder: SpanBuilder) -> Self {
        builder.build()
    }
}

struct RelationBuilder {
    relation: Relation,
}

impl RelationBuilder {
    pub fn new(from_span: &str, to_span: &str) -> Self {
        RelationBuilder {
            relation: Relation {
                id: Uuid::from_bytes([0u8; 16]),
                name: format!("{from_span} -> {to_span}"),
                description: "".to_string(),
                from_span_selector: SpanSelector::new_equal_name(from_span),
                to_span_selector: SpanSelector::new_equal_name(to_span),
                attribute_relations: Vec::new(),
                max_time_diff: None,
                nodes_config: RelationNodesConfig::AllNodes,
                match_type: MatchType::MatchAll,
                min_time_diff: 0.0,
                is_builtin: true,
            },
        }
    }

    pub fn attribute_equal(mut self, attr_name: &str) -> Self {
        self.relation.attribute_relations.push(AttributeRelation {
            from_attribute: attr_name.to_string(),
            to_attribute: attr_name.to_string(),
            relation: AttributeRelationOp::Equal,
        });
        self
    }

    pub fn attribute_one_greater(mut self, attr_name: &str) -> Self {
        self.relation.attribute_relations.push(AttributeRelation {
            from_attribute: attr_name.to_string(),
            to_attribute: attr_name.to_string(),
            relation: AttributeRelationOp::OneGreater,
        });
        self
    }

    pub fn attribute_two_greater(mut self, attr_name: &str) -> Self {
        self.relation.attribute_relations.push(AttributeRelation {
            from_attribute: attr_name.to_string(),
            to_attribute: attr_name.to_string(),
            relation: AttributeRelationOp::TwoGreater,
        });
        self
    }

    #[allow(unused)]
    pub fn from_attribute_equal(mut self, attr_name: &str, value: impl ToString) -> Self {
        self.relation.from_span_selector.attribute_conditions.push((
            attr_name.to_string(),
            MatchCondition::equal_to(&value.to_string()),
        ));
        self
    }

    #[allow(unused)]
    pub fn to_attribute_equal(mut self, attr_name: &str, value: impl ToString) -> Self {
        self.relation.to_span_selector.attribute_conditions.push((
            attr_name.to_string(),
            MatchCondition::equal_to(&value.to_string()),
        ));
        self
    }

    pub fn same_node(mut self) -> Self {
        self.relation.nodes_config = RelationNodesConfig::SameNode;
        self
    }

    pub fn build(mut self) -> Relation {
        self.relation.id = make_uuid_from_seed(&serde_json::to_string(&self.relation).unwrap());
        self.relation
    }
}

impl From<RelationBuilder> for Relation {
    fn from(builder: RelationBuilder) -> Self {
        builder.build()
    }
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
        println!("Processing relation: {:?}", relation);

        let Some(from_index) = span_id_to_index.get(&relation.from_span.upgrade().unwrap().span_id)
        else {
            continue; // child spans are not in span_id_to_index, ignore them
        };
        let Some(to_index) = span_id_to_index.get(&relation.to_span.upgrade().unwrap().span_id)
        else {
            continue; // ignore children
        };
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

    for span in &spans {
        set_children_start_to_parent_start(span);
    }

    spans
}

fn set_children_start_to_parent_start(span: &Span) {
    let parent_start = span.start_time;

    let mut new_children = Vec::new();
    for old_child in span.children.borrow().iter() {
        let mut new_child = (**old_child).clone();
        let child_duration = new_child.end_time - new_child.start_time;
        new_child.start_time = parent_start;
        new_child.end_time = new_child.start_time + child_duration;
        new_children.push(Rc::new(new_child));
    }

    span.children.replace(new_children);

    for child in span.children.borrow().iter() {
        set_children_start_to_parent_start(child);
    }
}
