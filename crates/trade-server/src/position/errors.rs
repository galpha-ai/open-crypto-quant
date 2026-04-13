use thiserror::Error;

#[derive(Debug, Error)]
pub enum PositionError {
    #[error("Insufficient SOL balance: {available:.2} < {required:.2}")]
    InsufficientSolBalance { available: f64, required: f64 },

    #[error("No position found for mint: {0}")]
    PositionNotFound(String),

    #[error("Invalid event type for position update")]
    InvalidEventType,

    #[error("Order rejected: {0}")]
    OrderRejected(String),

    #[error("Maximum open positions limit reached: {0}")]
    MaxOpenPositionsReached(u32),

    // === Intent-based reconciliation errors ===
    #[error("Reconciliation failed: {0}")]
    ReconciliationFailed(String),

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Invalid intent: {0}")]
    InvalidIntent(String),
}
