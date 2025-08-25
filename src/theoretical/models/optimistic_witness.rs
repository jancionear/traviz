use std::time::Duration;

use super::timing;
use crate::theoretical::{RelationBuilder, SpanBuilder, TheoreticalModel};

pub fn optimistic_witness_model() -> TheoreticalModel {
    let block_producer = "block_producer";
    let chunk_producer = "chunk_producer";
    let chunk_validator = "chunk_validator";
    let nodes = [block_producer, chunk_producer, chunk_validator];

    let mut model = TheoreticalModel::new(
        "optimistic_witness",
        "Stateless validation with optimistic block and optimistic chunk witness",
    );

    for height in 1..32 {
        // produce_block
        model.add_span(
            SpanBuilder::new(
                "produce_block",
                block_producer,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // produce_block depends on send_chunk_endorsement
        model.add_relation(
            RelationBuilder::new("send_chunk_endorsement", "produce_block")
                .attribute_equal("height"),
        );

        // preprocess_block
        for node in nodes {
            model.add_span(
                SpanBuilder::new("preprocess_block", node, timing::SHORT_OPERATION_TIME)
                    .with_attribute("height", height.to_string()),
            )
        }
        // preprocess_block depends on produce_block
        model.add_relation(
            RelationBuilder::new("produce_block", "preprocess_block").attribute_equal("height"),
        );

        // postprocess_block
        for node in nodes {
            model.add_span(
                SpanBuilder::new("postprocess_block", node, timing::POSTPROCESS_BLOCK_TIME)
                    .with_attribute("height", height.to_string()),
            );
        }
        // postprocess_block depends on preprocess_block and apply_chunk_optimistic
        model.add_relation(
            RelationBuilder::new("preprocess_block", "postprocess_block")
                .attribute_equal("height")
                .same_node(),
        );
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "postprocess_block")
                .attribute_equal("height")
                .same_node(),
        );

        // produce_optimistic_block
        model.add_span(
            SpanBuilder::new(
                "produce_optimistic_block",
                block_producer,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // produce_optimistic_block depends on postprocess_block
        model.add_relation(
            RelationBuilder::new("postprocess_block", "produce_optimistic_block")
                .attribute_one_greater("height")
                .same_node(),
        );

        // process_optimistic_block
        model.add_span(
            SpanBuilder::new(
                "process_optimistic_block",
                chunk_producer,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // process_optimistic_block depends on produce_optimistic_block
        model.add_relation(
            RelationBuilder::new("produce_optimistic_block", "process_optimistic_block")
                .attribute_equal("height"),
        );

        model.add_span(
            SpanBuilder::new(
                "produce_chunk",
                chunk_producer,
                timing::PREPARE_TRANSACTIONS_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // produce_chunk depends on postprocess_block
        model.add_relation(
            RelationBuilder::new("postprocess_block", "produce_chunk")
                .attribute_one_greater("height")
                .same_node(),
        );

        // apply_chunk_optimistic on chunk producer
        model.add_span(
            SpanBuilder::new(
                "apply_chunk_optimistic",
                chunk_producer,
                timing::APPLY_CHUNK_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // apply_chunk_optimistic depends on process_optimistic_block and produce_chunk
        model.add_relation(
            RelationBuilder::new("process_optimistic_block", "apply_chunk_optimistic")
                .attribute_equal("height")
                .same_node(),
        );
        model.add_relation(
            RelationBuilder::new("produce_chunk", "apply_chunk_optimistic")
                .attribute_equal("height"),
        );

        // send_optimistic_witness
        model.add_span(
            SpanBuilder::new(
                "send_optimistic_witness",
                chunk_producer,
                timing::DISTRIBUTE_WITNESS_TIME,
            )
            .with_attribute("height", height),
        );
        // send_optimistic_witness depends on apply_chunk_optimistic
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "send_optimistic_witness")
                .attribute_equal("height")
                .same_node(),
        );

        // send chunk state witness
        model.add_span(
            SpanBuilder::new(
                "send_chunk_state_witness",
                chunk_producer,
                timing::DISTRIBUTE_WITNESS_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // send_chunk_state_witness depends on produce_chunk and apply previous chunk
        model.add_relation(
            RelationBuilder::new("produce_chunk", "send_chunk_state_witness")
                .attribute_equal("height")
                .same_node(),
        );
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "send_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );

        // apply_optimistic_witness
        model.add_span(
            SpanBuilder::new(
                "apply_optimistic_witness",
                chunk_validator,
                timing::APPLY_CHUNK_TIME,
            )
            .with_attribute("height", height.to_string())
            .with_child(
                SpanBuilder::new(
                    "apply_chunk_optimistic",
                    chunk_validator,
                    timing::APPLY_CHUNK_TIME,
                )
                .with_attribute("height", height.to_string()),
            ),
        );
        // apply_optimistic_witness depends on send_optimistic_witness
        model.add_relation(
            RelationBuilder::new("send_optimistic_witness", "apply_optimistic_witness")
                .attribute_equal("height"),
        );

        // validate_chunk_state_witness
        model.add_span(
            SpanBuilder::new(
                "validate_chunk_state_witness",
                chunk_validator,
                Duration::from_millis(30),
            )
            .with_attribute("height", height.to_string()),
        );
        // validate_chunk_state_witness depends on send_chunk_state_witness, postprocessing previous block and applying optimistic witness
        model.add_relation(
            RelationBuilder::new("send_chunk_state_witness", "validate_chunk_state_witness")
                .attribute_equal("height"),
        );
        model.add_relation(
            RelationBuilder::new("postprocess_block", "validate_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );
        model.add_relation(
            RelationBuilder::new("apply_optimistic_witness", "validate_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );

        // send_chunk_endorsement
        model.add_span(
            SpanBuilder::new(
                "send_chunk_endorsement",
                chunk_validator,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // send_chunk_endorsement depends on validate_chunk_state_witness
        model.add_relation(
            RelationBuilder::new("validate_chunk_state_witness", "send_chunk_endorsement")
                .attribute_equal("height")
                .same_node(),
        );
    }

    model
}
