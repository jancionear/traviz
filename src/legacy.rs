//! Old code that is being kept for backwards compatibility.

use uuid::Uuid;

use crate::relation::{AttributeRelation, MatchType, Relation, RelationNodesConfig};
use crate::structured_modes::SpanSelector;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RelationV0 {
    pub id: Uuid,

    pub name: String,
    pub from_span_name: String,
    pub to_span_name: String,
    pub attribute_relations: Vec<AttributeRelation>,
    pub max_time_diff: Option<f64>,

    pub nodes_config: RelationNodesConfig,
    pub match_type: MatchType,

    pub is_builtin: bool,
}

impl From<RelationV0> for Relation {
    fn from(relation: RelationV0) -> Self {
        Relation {
            id: relation.id,
            name: relation.name,
            description: String::new(),
            from_span_selector: SpanSelector::new_equal_name(&relation.from_span_name),
            to_span_selector: SpanSelector::new_equal_name(&relation.to_span_name),
            attribute_relations: relation.attribute_relations,
            max_time_diff: relation.max_time_diff,
            nodes_config: relation.nodes_config,
            match_type: relation.match_type,
            is_builtin: relation.is_builtin,
        }
    }
}
