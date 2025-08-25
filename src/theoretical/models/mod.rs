use super::TheoreticalModel;

mod optimistic_block;
mod optimistic_witness;
mod realistic;
mod stateless_validation;
mod timing;
mod very_optimistic;

pub fn all_models() -> Vec<TheoreticalModel> {
    vec![
        stateless_validation::stateless_validation_model(),
        optimistic_block::optimistic_block_model(),
        optimistic_witness::optimistic_witness_model(),
        very_optimistic::very_optimistic_model(),
        realistic::realistic_model(),
    ]
}
