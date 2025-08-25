use std::time::Duration;

use rand::seq::IndexedRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::theoretical::{RelationBuilder, SpanBuilder, TheoreticalModel};

pub fn realistic_model() -> TheoreticalModel {
    let num_nodes = 20;
    let nodes = (0..num_nodes)
        .map(|i| format!("node{}", i))
        .collect::<Vec<_>>();

    let mut model = TheoreticalModel::new(
        "realistic",
        "Matches current 20 shard/node forknet benchmark",
    );
    let mut rng = ChaCha20Rng::from_seed([0; 32]);
    let enable_randomness = true;

    for height in 1..32 {
        let block_producer = nodes.choose(&mut rng).unwrap();

        model.add_span(
            SpanBuilder::new(
                "produce_block_on_head",
                block_producer,
                Duration::from_millis(1),
            )
            .with_attribute("height", height),
        );
        model.add_relation(
            RelationBuilder::new("postprocess_ready_block", "produce_block_on_head")
                .attribute_one_greater("height"),
        );
        // TODO - properly model the chunk endorsement dependency
        model.add_relation(
            RelationBuilder::new("send_chunk_endorsement", "produce_block_on_head")
                .attribute_equal("height"),
        );

        model.add_span(
            SpanBuilder::new(
                "produce_optimistic_block_on_head",
                block_producer,
                Duration::from_micros(200),
            )
            .with_attribute("height", height),
        );
        model.add_relation(
            RelationBuilder::new(
                "postprocess_ready_block",
                "produce_optimistic_block_on_head",
            )
            .attribute_one_greater("height"),
        );
        model.add_relation(
            RelationBuilder::new("produce_chunk_internal", "produce_optimistic_block_on_head")
                .attribute_equal("height")
                .same_node(),
        );
        model.add_relation(
            RelationBuilder::new(
                "persist_and_distribute_encoded_chunk",
                "produce_optimistic_block_on_head",
            )
            .attribute_equal("height")
            .same_node(),
        );

        // (tracked_shard, node) node_i tracks shard i
        for (shard_id, node) in nodes.iter().enumerate() {
            let is_slow = if enable_randomness {
                rng.random_range(0..5) == 0
            } else {
                node == "node1"
            };

            // receive_block
            if node != block_producer {
                model.add_span(
                    SpanBuilder::new("receive_block", node, Duration::from_millis(5))
                        .with_attribute("height", height),
                );
                model.add_relation(
                    RelationBuilder::new("produce_block_on_head", "receive_block")
                        .attribute_equal("height"),
                );
            }

            // preprocess_block
            model.add_span(
                SpanBuilder::new("preprocess_block", node, Duration::from_millis(35))
                    .with_attribute("height", height),
            );
            model.add_relation(
                RelationBuilder::new("produce_block_on_head", "preprocess_block")
                    .attribute_equal("height"),
            );
            model.add_relation(
                RelationBuilder::new("receive_block", "preprocess_block")
                    .attribute_equal("height")
                    .same_node(),
            );
            model.add_relation(
                RelationBuilder::new("postprocess_ready_block", "preprocess_block")
                    .attribute_one_greater("height")
                    .same_node(),
            );

            // postprocess_ready_block
            model.add_span(
                SpanBuilder::new("postprocess_ready_block", node, Duration::from_millis(270))
                    .with_attribute("height", height),
            );
            model.add_relation(
                RelationBuilder::new("preprocess_block", "postprocess_ready_block")
                    .attribute_equal("height")
                    .same_node(),
            );
            model.add_relation(
                RelationBuilder::new("apply_new_chunk", "postprocess_ready_block")
                    .attribute_equal("height")
                    .same_node(),
            );

            // produce_chunk
            model.add_span(
                SpanBuilder::new("produce_chunk_internal", node, Duration::from_millis(170))
                    .with_attribute("height", height)
                    .with_attribute("shard_id", shard_id),
            );
            model.add_relation(
                RelationBuilder::new("postprocess_ready_block", "produce_chunk_internal")
                    .attribute_one_greater("height")
                    .same_node(),
            );

            // send_chunk_state_witness
            model.add_span(
                SpanBuilder::new("send_chunk_state_witness", node, Duration::from_millis(30))
                    .with_attribute("height", height)
                    .with_attribute("shard_id", shard_id),
            );
            model.add_relation(
                RelationBuilder::new("produce_chunk_internal", "send_chunk_state_witness")
                    .attribute_equal("height")
                    .attribute_equal("shard_id")
                    .same_node(),
            );

            // persist_and_distribute_encoded_chunk
            model.add_span(
                SpanBuilder::new(
                    "persist_and_distribute_encoded_chunk",
                    node,
                    Duration::from_millis(160),
                )
                .with_attribute("height", height)
                .with_attribute("shard_id", shard_id),
            );
            model.add_relation(
                RelationBuilder::new(
                    "produce_chunk_internal",
                    "persist_and_distribute_encoded_chunk",
                )
                .attribute_equal("height")
                .attribute_equal("shard_id")
                .same_node(),
            );
            model.add_relation(
                RelationBuilder::new(
                    "send_chunk_state_witness",
                    "persist_and_distribute_encoded_chunk",
                )
                .attribute_equal("height")
                .attribute_equal("shard_id")
                .same_node(),
            );

            for shard_id in 0..num_nodes {
                // receive_chunk
                model.add_span(
                    SpanBuilder::new("receive_chunk", node, Duration::from_millis(80))
                        .with_attribute("height", height)
                        .with_attribute("shard_id", shard_id),
                );
                model.add_relation(
                    RelationBuilder::new("persist_and_distribute_encoded_chunk", "receive_chunk")
                        .attribute_equal("height")
                        .attribute_equal("shard_id"),
                );

                // chunk_completed
                model.add_span(
                    SpanBuilder::new("chunk_completed", node, Duration::from_millis(1))
                        .with_attribute("height", height)
                        .with_attribute("shard_id", shard_id),
                );
                model.add_relation(
                    RelationBuilder::new("persist_and_distribute_encoded_chunk", "chunk_completed")
                        .attribute_equal("height")
                        .attribute_equal("shard_id"),
                );
                model.add_relation(
                    RelationBuilder::new("receive_chunk", "chunk_completed")
                        .attribute_equal("height")
                        .attribute_equal("shard_id")
                        .same_node(),
                );
            }

            // receive_optimistic_block
            if node != block_producer {
                model.add_span(
                    SpanBuilder::new("receive_optimistic_block", node, Duration::from_millis(30))
                        .with_attribute("height", height),
                );
                model.add_relation(
                    RelationBuilder::new(
                        "produce_optimistic_block_on_head",
                        "receive_optimistic_block",
                    )
                    .attribute_equal("height"),
                );
            }

            // process_optimistic_block
            model.add_span(
                SpanBuilder::new("process_optimistic_block", node, Duration::from_millis(70))
                    .with_attribute("height", height),
            );
            model.add_relation(
                RelationBuilder::new("receive_optimistic_block", "process_optimistic_block")
                    .attribute_equal("height")
                    .same_node(),
            );
            model.add_relation(
                RelationBuilder::new("postprocess_ready_block", "process_optimistic_block")
                    .attribute_one_greater("height")
                    .same_node(),
            );
            model.add_relation(
                RelationBuilder::new(
                    "produce_optimistic_block_on_head",
                    "process_optimistic_block",
                )
                .attribute_equal("height"),
            );
            model.add_relation(
                RelationBuilder::new("chunk_completed", "process_optimistic_block")
                    .attribute_equal("height")
                    .same_node(),
            );

            // apply_new_chunk
            let apply_new_chunk_time = if is_slow { 600 } else { 475 };
            model.add_span(
                SpanBuilder::new(
                    "apply_new_chunk",
                    node,
                    Duration::from_millis(apply_new_chunk_time),
                )
                .with_attribute("height", height)
                .with_attribute("shard_id", shard_id),
            );
            model.add_relation(
                RelationBuilder::new("process_optimistic_block", "apply_new_chunk")
                    .attribute_equal("height")
                    .same_node(),
            );

            // Chunk validation
            for validated_shard_id in (0..num_nodes).cycle().skip(6 * shard_id).take(6) {
                // receive_witness
                let receive_witness_time = if is_slow { 350 } else { 220 };
                model.add_span(
                    SpanBuilder::new(
                        "receive_witness",
                        node,
                        Duration::from_millis(receive_witness_time),
                    )
                    .with_attribute("height", height)
                    .with_attribute("shard_id", validated_shard_id),
                );
                model.add_relation(
                    RelationBuilder::new("send_chunk_state_witness", "receive_witness")
                        .attribute_equal("height")
                        .attribute_equal("shard_id"),
                );

                // validate_chunk_state_witness
                let validate_chunk_state_witness_time = if is_slow { 800 } else { 600 };
                model.add_span(
                    SpanBuilder::new(
                        "validate_chunk_state_witness",
                        node,
                        Duration::from_millis(validate_chunk_state_witness_time),
                    )
                    .with_attribute("height", height)
                    .with_attribute("shard_id", validated_shard_id),
                );
                model.add_relation(
                    RelationBuilder::new("receive_witness", "validate_chunk_state_witness")
                        .attribute_equal("height")
                        .attribute_equal("shard_id")
                        .same_node(),
                );
                model.add_relation(
                    RelationBuilder::new("postprocess_ready_block", "validate_chunk_state_witness")
                        .attribute_one_greater("height")
                        .attribute_equal("shard_id")
                        .same_node(),
                );

                // send_chunk_endorsement
                model.add_span(
                    SpanBuilder::new("send_chunk_endorsement", node, Duration::from_micros(100))
                        .with_attribute("height", height)
                        .with_attribute("shard_id", validated_shard_id),
                );
                model.add_relation(
                    RelationBuilder::new("validate_chunk_state_witness", "send_chunk_endorsement")
                        .attribute_equal("height")
                        .attribute_equal("shard_id")
                        .same_node(),
                );
            }
        }
    }

    model
}
