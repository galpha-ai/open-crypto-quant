use solana_sdk::clock::Slot; // Correct import for Slot type used elsewhere

/// Error types for the LeaderMonitorService.
#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("RPC error during background refresh: {0}")]
    RpcError(#[from] solana_client::client_error::ClientError), // Note: Only used internally by worker

    #[error("Failed to acquire lock for internal state: {0}")]
    LockError(String),

    #[error("Invalid slot range: start_slot {0} > end_slot {1}")]
    InvalidSlotRange(Slot, Slot),

    #[error("Leader schedule cache is not yet populated")]
    CacheNotReady,
}
