use async_trait::async_trait;

use super::TradableSignal;

/// Trait for filtering trading signals
#[async_trait]
pub trait SignalFilter: Send + Sync {
    /// Evaluate whether a signal should pass the filter
    /// Returns true if the signal should be processed (notifications, orders)
    /// Returns false if the signal should only be persisted
    async fn should_pass(&self, signal: &dyn TradableSignal) -> bool;
}

/// A no-op filter that lets all signals pass
pub struct PassAllFilter;

#[async_trait]
impl SignalFilter for PassAllFilter {
    async fn should_pass(&self, _signal: &dyn TradableSignal) -> bool {
        true
    }
}

/// A filter that blocks all signals from passing
pub struct BlockAllFilter;

#[async_trait]
impl SignalFilter for BlockAllFilter {
    async fn should_pass(&self, _signal: &dyn TradableSignal) -> bool {
        false
    }
}
