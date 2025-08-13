use std::time::Duration;

// Time for a very short operation (produce block, validate endorsement, etc)
pub const SHORT_OPERATION_TIME: Duration = Duration::from_millis(10);
pub const APPLY_CHUNK_TIME: Duration = Duration::from_millis(400);
pub const PREPARE_TRANSACTIONS_TIME: Duration = Duration::from_millis(100);
pub const POSTPROCESS_BLOCK_TIME: Duration = Duration::from_millis(100);
pub const DISTRIBUTE_WITNESS_TIME: Duration = Duration::from_millis(150);
pub const SEND_CHUNK_ENDORSEMENT_TIME: Duration = Duration::from_millis(10);
pub const SEND_OUTGOING_RECEIPTS_TIME: Duration = Duration::from_millis(50);
