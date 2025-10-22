use std::time::Duration;

use super::timing;
use crate::theoretical::{RelationBuilder, SpanBuilder, TheoreticalModel};

pub fn very_optimistic_model() -> TheoreticalModel {
    let block_producer = "block_producer";
    let chunk_producer = "chunk_producer";
    let chunk_validator = "chunk_validator";
    let nodes = [block_producer, chunk_producer, chunk_validator];

    let mut model = TheoreticalModel::new(
        "very_optimistic",
        "Stateless validation with a lot of optimistic execution to achieve maximum throughput",
    );

    // Prepare transactions using earlier state, in parallel with chunk application
    let optimization_earlier_prepare_transactions = false;
    // Allow to produce optimistic block after preprocess_block instead of postprocess_block
    let optimization_produce_optimistic_block_after_preprocess = false;
    // Allow to produce optimistic block based on the previous optimistic block alone
    let optimization_produce_optimistic_block_after_optimistic_block = false;
    // Allow applying a chunk after the previous block has been preprocessed
    let optimization_apply_chunk_after_previous_block_preprocess = false;
    // Allow applying a chunk after the previous-previous chunk has been postprocessed
    let optimization_apply_chunk_after_previous_previous_preprocess = false;

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
        // Needs chunk endorsements for this height
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
        // Needs produced block
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

        // produce_optimistic_block - Create the optimistic block that contains data needed to apply optimistic chunks
        model.add_span(
            SpanBuilder::new(
                "produce_optimistic_block",
                block_producer,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        if optimization_produce_optimistic_block_after_optimistic_block {
            // Needs only previous optimistic block to be created
            model.add_relation(
                RelationBuilder::new("produce_optimistic_block", "produce_optimistic_block")
                    .attribute_one_greater("height")
                    .same_node(),
            );
            // This isn't strictly needed, but it puts the next produce_optimistic_block in a more reasonable place
            model.add_relation(
                RelationBuilder::new("preprocess_block", "produce_optimistic_block")
                    .attribute_two_greater("height")
                    .same_node(),
            );
        } else if optimization_produce_optimistic_block_after_preprocess {
            // Needs previous block to be postprocessed
            model.add_relation(
                RelationBuilder::new("preprocess_block", "produce_optimistic_block")
                    .attribute_one_greater("height")
                    .same_node(),
            );
        } else {
            // Needs previous block to be preprocessed
            model.add_relation(
                RelationBuilder::new("postprocess_block", "produce_optimistic_block")
                    .attribute_one_greater("height")
                    .same_node(),
            );
        }

        // process_optimistic_block - Receive optimistic block and start applying optimistic chunks
        model.add_span(
            SpanBuilder::new(
                "process_optimistic_block",
                chunk_producer,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs produce_optimistic_block
        model.add_relation(
            RelationBuilder::new("produce_optimistic_block", "process_optimistic_block")
                .attribute_equal("height"),
        );

        // prepare_transactions - Gather and validate transactions for the next chunk
        model.add_span(
            SpanBuilder::new(
                "prepare_transactions",
                chunk_producer,
                timing::PREPARE_TRANSACTIONS_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        if optimization_earlier_prepare_transactions {
            // Needs post-state of the previous-previous chunk
            model.add_relation(
                RelationBuilder::new("apply_chunk_optimistic", "prepare_transactions")
                    .attribute_two_greater("height")
                    .same_node(),
            );
        } else {
            // Needs post-state of the previous chunk
            model.add_relation(
                RelationBuilder::new("apply_chunk_optimistic", "prepare_transactions")
                    .attribute_one_greater("height")
                    .same_node(),
            );
        }

        // send_outgoing_receipts - Send outgoing receipts produced by optimistic chunk application
        model.add_span(
            SpanBuilder::new(
                "send_outgoing_receipts",
                chunk_producer,
                timing::SEND_OUTGOING_RECEIPTS_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs apply_chunk_optimistic
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "send_outgoing_receipts")
                .attribute_equal("height")
                .same_node(),
        );

        // produce_chunk - Produce the official chunk for this height
        model.add_span(
            SpanBuilder::new("produce_chunk", chunk_producer, Duration::from_millis(10))
                .with_attribute("height", height.to_string()),
        );
        // Needs previous block to be accepted and postprocessed
        model.add_relation(
            RelationBuilder::new("postprocess_block", "produce_chunk")
                .attribute_one_greater("height")
                .same_node(),
        );
        // Needs transactions to be prepared
        model.add_relation(
            RelationBuilder::new("prepare_transactions", "produce_chunk")
                .attribute_equal("height")
                .same_node(),
        );

        // produce_optimistic_chunk - Produce optimistic chunk, which contains only transactions
        model.add_span(
            SpanBuilder::new(
                "produce_optimistic_chunk",
                chunk_producer,
                Duration::from_millis(10),
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs prepare_transactions
        model.add_relation(
            RelationBuilder::new("prepare_transactions", "produce_optimistic_chunk")
                .attribute_equal("height")
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
        // Needs the optimistic block
        model.add_relation(
            RelationBuilder::new("process_optimistic_block", "apply_chunk_optimistic")
                .attribute_equal("height")
                .same_node(),
        );
        // Needs the optimistic chunk
        model.add_relation(
            RelationBuilder::new("produce_optimistic_chunk", "apply_chunk_optimistic")
                .attribute_equal("height"),
        );
        // Needs outgoing receipts from other chunks
        model.add_relation(
            RelationBuilder::new("send_outgoing_receipts", "apply_chunk_optimistic")
                .attribute_one_greater("height")
                .same_node(),
        );
        // Needs previous chunk application to be completed
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "apply_chunk_optimistic")
                .attribute_one_greater("height")
                .same_node(),
        );
        if optimization_apply_chunk_after_previous_previous_preprocess {
            // Needs previous-previous block to be preprocessed
            model.add_relation(
                RelationBuilder::new("preprocess_block", "apply_chunk_optimistic")
                    .attribute_two_greater("height")
                    .same_node(),
            );
        } else if optimization_apply_chunk_after_previous_block_preprocess {
            // Needs previous block to be preprocessed
            model.add_relation(
                RelationBuilder::new("preprocess_block", "apply_chunk_optimistic")
                    .attribute_one_greater("height")
                    .same_node(),
            );
        } else {
            // Needs previous block to be postprocessed
            model.add_relation(
                RelationBuilder::new("postprocess_block", "apply_chunk_optimistic")
                    .attribute_one_greater("height")
                    .same_node(),
            );
        }

        // send_optimistic_witness - send witness which has all the information needed to apply the optimistic chunk.
        // Doesn't contain information about the next chunk.
        model.add_span(
            SpanBuilder::new(
                "send_optimistic_witness",
                chunk_producer,
                timing::DISTRIBUTE_WITNESS_TIME,
            )
            .with_attribute("height", height),
        );
        // Needs apply_chunk_optimistic to get storage proof
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "send_optimistic_witness")
                .attribute_equal("height")
                .same_node(),
        );

        // send_chunk_state_witness - Send the official chunk state witness,
        // which contains the new chunk and all data needed to apply the previous chunk.
        model.add_span(
            SpanBuilder::new(
                "send_chunk_state_witness",
                chunk_producer,
                timing::DISTRIBUTE_WITNESS_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs the chunk to be produced
        model.add_relation(
            RelationBuilder::new("produce_chunk", "send_chunk_state_witness")
                .attribute_equal("height")
                .same_node(),
        );
        // Needs apply_chunk_optimistic to get storage proof
        model.add_relation(
            RelationBuilder::new("apply_chunk_optimistic", "send_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );

        // apply_optimistic_witness - apply the optimistic chunk using the optimistic witness
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
        // Needs send_optimistic_witness
        model.add_relation(
            RelationBuilder::new("send_optimistic_witness", "apply_optimistic_witness")
                .attribute_equal("height"),
        );

        // validate_chunk_state_witness - validate the official chunk state witness.
        // The previous chunk has already been applied in apply_optimistic_witness,
        // so we only need to validate data in the new chunk, which is fast.
        model.add_span(
            SpanBuilder::new(
                "validate_chunk_state_witness",
                chunk_validator,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs send_chunk_state_witness
        model.add_relation(
            RelationBuilder::new("send_chunk_state_witness", "validate_chunk_state_witness")
                .attribute_equal("height"),
        );
        // Needs previous block to be postprocessed
        model.add_relation(
            RelationBuilder::new("postprocess_block", "validate_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );
        // Needs apply_optimistic_witness to apply the previous chunk
        model.add_relation(
            RelationBuilder::new("apply_optimistic_witness", "validate_chunk_state_witness")
                .attribute_one_greater("height")
                .same_node(),
        );

        // send_chunk_endorsement - Send official chunk endorsement after validating the chunk state witness.
        model.add_span(
            SpanBuilder::new(
                "send_chunk_endorsement",
                chunk_validator,
                timing::SHORT_OPERATION_TIME,
            )
            .with_attribute("height", height.to_string()),
        );
        // Needs validate_chunk_state_witness
        model.add_relation(
            RelationBuilder::new("validate_chunk_state_witness", "send_chunk_endorsement")
                .attribute_equal("height")
                .same_node(),
        );
    }

    model
}
