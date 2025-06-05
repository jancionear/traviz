use crate::relation::{
    make_uuid_from_seed, AttributeRelation, AttributeRelationOp, MatchType, Relation,
    RelationNodesConfig,
};
use crate::structured_modes::{MatchCondition, SpanSelector};

pub fn produce_block_on_head_to_preprocess_block_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("produce_block_on_head -> preprocess_block"),
        name: "produce_block_on_head -> preprocess_block".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("produce_block_on_head"),
        to_span_selector: SpanSelector::new_equal_name("preprocess_block"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::AllNodes,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn preprocess_block_to_postprocess_ready_block_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("pre-post-process block"),
        name: "preprocess_block -> postprocess_ready_block".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("preprocess_block"),
        to_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn postprocess_ready_block_to_produce_block_on_head_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("postprocess_ready_block -> produce_block_on_head"),
        name: "postprocess_ready_block -> produce_block_on_head".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        to_span_selector: SpanSelector::new_equal_name("produce_block_on_head"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::OneGreater,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn postprocess_ready_block_to_next_preprocess_block_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("post-pre-process block"),
        name: "postprocess_ready_block to next preprocess_block".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        to_span_selector: SpanSelector::new_equal_name("preprocess_block"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::OneGreater,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn preprocess_block_to_apply_new_chunk_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("preprocess_block -> apply_new_chunk"),
        name: "preprocess_block -> apply_new_chunk".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("preprocess_block"),
        to_span_selector: SpanSelector {
            span_name_condition: MatchCondition::equal_to("apply_new_chunk"),
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![
                (
                    "apply_reason".to_string(),
                    MatchCondition::equal_to("UpdateTrackedShard"),
                ),
                ("block_type".to_string(), MatchCondition::equal_to("Normal")),
            ],
        },
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn apply_new_chunk_normal_to_postprocess_ready_block_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("apply_new_chunk(normal) -> postprocess_ready_block"),
        name: "apply_new_chunk(normal) -> postprocess_ready_block".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector {
            span_name_condition: MatchCondition::equal_to("apply_new_chunk"),
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![
                (
                    "apply_reason".to_string(),
                    MatchCondition::equal_to("UpdateTrackedShard"),
                ),
                ("block_type".to_string(), MatchCondition::equal_to("Normal")),
            ],
        },
        to_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn apply_new_chunk_optimistic_to_postprocess_ready_block_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("apply_new_chunk(optimistic) -> postprocess_ready_block"),
        name: "apply_new_chunk(optimistic) -> postprocess_ready_block".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector {
            span_name_condition: MatchCondition::equal_to("apply_new_chunk"),
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![
                (
                    "apply_reason".to_string(),
                    MatchCondition::equal_to("UpdateTrackedShard"),
                ),
                (
                    "block_type".to_string(),
                    MatchCondition::equal_to("Optimistic"),
                ),
            ],
        },
        to_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn postprocess_ready_block_to_produce_chunk_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("postprocess_ready_block -> produce_chunk"),
        name: "postprocess_ready_block -> produce_chunk".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        to_span_selector: SpanSelector::new_equal_name("produce_chunk"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::OneGreater,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn produce_chunk_to_send_chunk_state_witness_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("produce_chunk -> send_chunk_state_witness"),
        name: "produce_chunk -> send_chunk_state_witness".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("produce_chunk"),
        to_span_selector: SpanSelector::new_equal_name("send_chunk_state_witness"),
        attribute_relations: vec![
            AttributeRelation {
                from_attribute: "height".to_string(),
                to_attribute: "height".to_string(),
                relation: AttributeRelationOp::Equal,
            },
            AttributeRelation {
                from_attribute: "shard_id".to_string(),
                to_attribute: "shard_id".to_string(),
                relation: AttributeRelationOp::Equal,
            },
        ],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn send_chunk_state_witness_to_validate_chunk_state_witness_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("send-validate witness"),
        name: "send_chunk_state_witness -> validate_chunk_state_witness".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("send_chunk_state_witness"),
        to_span_selector: SpanSelector::new_equal_name("validate_chunk_state_witness"),
        attribute_relations: vec![
            AttributeRelation {
                from_attribute: "height".to_string(),
                to_attribute: "height".to_string(),
                relation: AttributeRelationOp::Equal,
            },
            AttributeRelation {
                from_attribute: "shard_id".to_string(),
                to_attribute: "shard_id".to_string(),
                relation: AttributeRelationOp::Equal,
            },
        ],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::AllNodes,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn validate_chunk_state_witness_to_send_chunk_endorsement_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("validate_chunk_state_witness -> send_chunk_endorsement"),
        name: "validate_chunk_state_witness -> send_chunk_endorsement".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("validate_chunk_state_witness"),
        to_span_selector: SpanSelector::new_equal_name("send_chunk_endorsement"),
        attribute_relations: vec![
            AttributeRelation {
                from_attribute: "height".to_string(),
                to_attribute: "height".to_string(),
                relation: AttributeRelationOp::Equal,
            },
            AttributeRelation {
                from_attribute: "shard_id".to_string(),
                to_attribute: "shard_id".to_string(),
                relation: AttributeRelationOp::Equal,
            },
        ],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn send_chunk_endorsement_to_validate_chunk_endorsement_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("send-validate chunk endorsement"),
        name: "send_chunk_endorsement -> validate_chunk_endorsement".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("send_chunk_endorsement"),
        to_span_selector: SpanSelector::new_equal_name("validate_chunk_endorsement"),
        attribute_relations: vec![
            AttributeRelation {
                from_attribute: "height".to_string(),
                to_attribute: "height".to_string(),
                relation: AttributeRelationOp::Equal,
            },
            AttributeRelation {
                from_attribute: "shard_id".to_string(),
                to_attribute: "shard_id".to_string(),
                relation: AttributeRelationOp::Equal,
            },
            AttributeRelation {
                from_attribute: "validator".to_string(),
                to_attribute: "validator".to_string(),
                relation: AttributeRelationOp::Equal,
            },
        ],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::AllNodes,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}
pub fn validate_chunk_endorsement_to_produce_block_on_head_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("validate_chunk_endorsement -> produce_block_on_head"),
        name: "validate_chunk_endorsement -> produce_block_on_head".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("validate_chunk_endorsement"),
        to_span_selector: SpanSelector::new_equal_name("produce_block_on_head"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn postprocess_ready_block_to_produce_optimistic_block_on_head_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("postprocess_ready_block -> produce_optimistic_block_on_head"),
        name: "postprocess_ready_block -> produce_optimistic_block_on_head".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("postprocess_ready_block"),
        to_span_selector: SpanSelector::new_equal_name("produce_optimistic_block_on_head"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::OneGreater,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn produce_optimistic_block_on_head_to_process_optimistic_block_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("produce optimistic -> preprocess optimistic"),
        name: "produce_optimistic_block_on_head -> process_optimistic_block".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("produce_optimistic_block_on_head"),
        to_span_selector: SpanSelector::new_equal_name("process_optimistic_block"),
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::AllNodes,
        match_type: MatchType::MatchAll,
        min_time_diff: 0.0,
        is_builtin: true,
    }
}

pub fn process_optimistic_block_to_apply_new_chunk_optimistic_relation() -> Relation {
    Relation {
        id: make_uuid_from_seed("process_optimistic_block -> apply_new_chunk(optimistic)"),
        name: "process_optimistic_block -> apply_new_chunk(optimistic)".to_string(),
        description: "".to_string(),
        from_span_selector: SpanSelector::new_equal_name("process_optimistic_block"),
        to_span_selector: SpanSelector {
            span_name_condition: MatchCondition::equal_to("apply_new_chunk"),
            node_name_condition: MatchCondition::any(),
            attribute_conditions: vec![
                (
                    "apply_reason".to_string(),
                    MatchCondition::equal_to("UpdateTrackedShard"),
                ),
                (
                    "block_type".to_string(),
                    MatchCondition::equal_to("Optimistic"),
                ),
            ],
        },
        attribute_relations: vec![AttributeRelation {
            from_attribute: "height".to_string(),
            to_attribute: "height".to_string(),
            relation: AttributeRelationOp::Equal,
        }],
        max_time_diff: Some(5.0), // 5 seconds
        nodes_config: RelationNodesConfig::SameNode,
        match_type: MatchType::MatchAll,
        min_time_diff: -0.010, // apply_new_chunk sometimes happens a few ms before the process_optimistic_block that spawns it.
        is_builtin: true,
    }
}

pub fn builtin_relations() -> Vec<Relation> {
    vec![
        produce_block_on_head_to_preprocess_block_relation(),
        preprocess_block_to_postprocess_ready_block_relation(),
        postprocess_ready_block_to_produce_block_on_head_relation(),
        postprocess_ready_block_to_next_preprocess_block_relation(),
        preprocess_block_to_apply_new_chunk_relation(),
        apply_new_chunk_normal_to_postprocess_ready_block_relation(),
        apply_new_chunk_optimistic_to_postprocess_ready_block_relation(),
        postprocess_ready_block_to_produce_chunk_relation(),
        produce_chunk_to_send_chunk_state_witness_relation(),
        send_chunk_state_witness_to_validate_chunk_state_witness_relation(),
        validate_chunk_state_witness_to_send_chunk_endorsement_relation(),
        send_chunk_endorsement_to_validate_chunk_endorsement_relation(),
        validate_chunk_endorsement_to_produce_block_on_head_relation(),
        postprocess_ready_block_to_produce_optimistic_block_on_head_relation(),
        produce_optimistic_block_on_head_to_process_optimistic_block_relation(),
        process_optimistic_block_to_apply_new_chunk_optimistic_relation(),
    ]
}
