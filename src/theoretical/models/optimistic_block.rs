use super::timing;
use crate::theoretical::{RelationBuilder, SpanBuilder, TheoreticalModel};

pub fn optimistic_block_model() -> TheoreticalModel {
    let block_producer = "block_producer";
    let chunk_producer = "chunk_producer";
    let chunk_validator = "chunk_validator";
    let nodes = [block_producer, chunk_producer, chunk_validator];

    let mut model = TheoreticalModel::new(
        "optimistic_block",
        "Stateless validation with optimistic block",
    );

    for height in 1..32 {
        // produce_block - Produce the official block for this height
        model.add_span(
            SpanBuilder::new(
                "produce_block",
                block_producer,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs send_chunk_endorsement for this height
        model.add_relation(
            RelationBuilder::new("send_chunk_endorsement", "produce_block")
                .attribute_equal("height"),
        );

        // preprocess_block - Preprocess the official block
        for node in nodes {
            model.add_span(
                SpanBuilder::new("preprocess_block", node, timing::SHORT_OPERATION_TIME)
                    .with_attribute("height", height.to_string()),
            )
        }
        // Needs produce_block for this height
        model.add_relation(
            RelationBuilder::new("produce_block", "preprocess_block").attribute_equal("height"),
        );

        // postprocess_block - Postprocess the official block
        for node in nodes {
            model.add_span(
                SpanBuilder::new("postprocess_block", node, timing::POSTPROCESS_BLOCK_TIME)
                    .with_attribute("height", height.to_string()),
            );
        }
        // Needs preprocess_block
        model.add_relation(
            RelationBuilder::new("preprocess_block", "postprocess_block")
                .attribute_equal("height")
                .same_node(),
        );
        // Needs all chunks from this block have to be applied
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

        // validate_chunk_state_witness
        model.add_span(
            SpanBuilder::new(
                "validate_chunk_state_witness",
                chunk_validator,
                timing::APPLY_CHUNK_TIME,
            )
            .with_attribute("height", height.to_string())
            .with_child(
                SpanBuilder::new("apply_chunk", chunk_validator, timing::APPLY_CHUNK_TIME)
                    .with_attribute("height", height - 1),
            ),
        );
        // validate_chunk_state_witness depends on send_chunk_state_witness and postprocessing previous block
        model.add_relation(
            RelationBuilder::new("send_chunk_state_witness", "validate_chunk_state_witness")
                .attribute_equal("height"),
        );
        model.add_relation(
            RelationBuilder::new("postprocess_block", "validate_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );

        // send_chunk_endorsement
        model.add_span(
            SpanBuilder::new(
                "send_chunk_endorsement",
                chunk_validator,
                timing::SEND_CHUNK_ENDORSEMENT_TIME,
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
