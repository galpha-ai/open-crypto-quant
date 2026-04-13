use anyhow::Result;
use solana_sdk::message::VersionedMessage;

use crate::{execution::order::Order, signal::TradableSignal};

#[async_trait::async_trait]
pub trait TransactionConstructor: Send + Sync {
    /// Construct a transaction message for the given order
    async fn construct_transaction(
        &self,
        order: &Order,
        no_later_than_slot: Option<u64>,
        log_str: Option<String>,
    ) -> Result<VersionedMessage>;

    /// Cache signal information for future transaction construction
    fn cache_signal(&self, _signal: &dyn TradableSignal) {
        // Default implementation does nothing
    }
}
