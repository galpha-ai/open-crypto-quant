//! Periodic position reconciliation with exchange.
//!
//! This module provides a background task that periodically queries the exchange
//! for actual position state and reconciles it with the local position manager.
//! This catches any drift between expected and actual positions (e.g., from
//! missed fill events, manual trades, etc.).

use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tracing::{error, info, warn};

use crate::domain::SystemEvent;
use crate::event_coordinator::EventCoordinator;
use crate::position::{
    PositionManager,
    position_query::{ExchangePosition, PositionQuerier, PositionQueryError},
};

/// Configuration for the position reconciler.
#[derive(Debug, Clone)]
pub struct PositionReconcilerConfig {
    /// How often to reconcile (e.g., 30 seconds).
    pub interval: Duration,
    /// Minimum drift to trigger an update event (avoid noise).
    /// Positions with drift below this threshold are considered in sync.
    pub drift_threshold: f64,
    /// Optional list of assets to reconcile.
    /// If None, reconciles all positions tracked by the position manager.
    pub assets: Option<Vec<String>>,
}

impl Default for PositionReconcilerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            drift_threshold: 1e-6,
            assets: None,
        }
    }
}

impl PositionReconcilerConfig {
    /// Create a new config with the specified interval.
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Set the drift threshold.
    pub fn with_drift_threshold(mut self, threshold: f64) -> Self {
        self.drift_threshold = threshold;
        self
    }

    /// Limit reconciliation to specific assets.
    pub fn with_assets(mut self, assets: Vec<String>) -> Self {
        self.assets = Some(assets);
        self
    }
}

/// Background task that periodically reconciles position state with exchange.
pub struct PositionReconciler<PM, PQ, EC>
where
    PM: PositionManager,
    PQ: PositionQuerier,
    EC: EventCoordinator,
{
    config: PositionReconcilerConfig,
    position_querier: Arc<PQ>,
    position_manager: Arc<PM>,
    event_coordinator: Arc<EC>,
}

impl<PM, PQ, EC> PositionReconciler<PM, PQ, EC>
where
    PM: PositionManager + 'static,
    PQ: PositionQuerier + 'static,
    EC: EventCoordinator + 'static,
{
    /// Create a new position reconciler.
    pub fn new(
        config: PositionReconcilerConfig,
        position_querier: Arc<PQ>,
        position_manager: Arc<PM>,
        event_coordinator: Arc<EC>,
    ) -> Self {
        Self {
            config,
            position_querier,
            position_manager,
            event_coordinator,
        }
    }

    /// Start the reconciliation loop.
    ///
    /// This spawns a background task that runs until dropped.
    /// Returns a JoinHandle that can be used to await completion.
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run_loop().await;
        })
    }

    async fn run_loop(&self) {
        let mut ticker = interval(self.config.interval);

        info!(
            exchange = self.position_querier.exchange_name(),
            interval_secs = self.config.interval.as_secs(),
            drift_threshold = self.config.drift_threshold,
            "Starting position reconciliation loop"
        );

        loop {
            ticker.tick().await;

            if let Err(e) = self.reconcile().await {
                error!(
                    error = ?e,
                    exchange = self.position_querier.exchange_name(),
                    "Position reconciliation failed"
                );
            }
        }
    }

    async fn reconcile(&self) -> Result<(), PositionQueryError> {
        let exchange_positions = self.position_querier.query_all_positions().await?;

        for exchange_pos in exchange_positions {
            // Skip if we're filtering to specific assets
            if let Some(ref assets) = self.config.assets {
                if !assets.contains(&exchange_pos.asset_id) {
                    continue;
                }
            }

            if let Err(e) = self.reconcile_single_position(exchange_pos).await {
                warn!(
                    error = ?e,
                    "Failed to reconcile single position, continuing with others"
                );
            }
        }

        Ok(())
    }

    async fn reconcile_single_position(
        &self,
        exchange_pos: ExchangePosition,
    ) -> Result<(), PositionQueryError> {
        // Get our tracked position
        let tracked = self
            .position_manager
            .get_position(&exchange_pos.asset_id)
            .await;
        let tracked_amount = tracked.as_ref().map(|p| p.amount).unwrap_or(0.0);
        let drift = exchange_pos.amount - tracked_amount;

        // Skip if drift is below threshold
        if drift.abs() < self.config.drift_threshold {
            return Ok(());
        }

        warn!(
            asset_id = %exchange_pos.asset_id,
            tracked_amount = tracked_amount,
            exchange_amount = exchange_pos.amount,
            drift = drift,
            exchange = self.position_querier.exchange_name(),
            "Position drift detected, emitting reconciliation event"
        );

        // Update position manager with authoritative exchange data
        let position_event = self
            .position_manager
            .reconcile_position(&exchange_pos)
            .await
            .map_err(|e| PositionQueryError::ApiError(e.to_string()))?;

        // Emit event for strategies to consume
        self.event_coordinator
            .enqueue_event(SystemEvent::Position(position_event))
            .await
            .map_err(|e| PositionQueryError::ApiError(e.to_string()))?;

        Ok(())
    }
}

/// Builder for creating a PositionReconciler with fluent API.
pub struct PositionReconcilerBuilder<PM, PQ, EC>
where
    PM: PositionManager,
    PQ: PositionQuerier,
    EC: EventCoordinator,
{
    config: PositionReconcilerConfig,
    position_querier: Option<Arc<PQ>>,
    position_manager: Option<Arc<PM>>,
    event_coordinator: Option<Arc<EC>>,
}

impl<PM, PQ, EC> Default for PositionReconcilerBuilder<PM, PQ, EC>
where
    PM: PositionManager,
    PQ: PositionQuerier,
    EC: EventCoordinator,
{
    fn default() -> Self {
        Self {
            config: PositionReconcilerConfig::default(),
            position_querier: None,
            position_manager: None,
            event_coordinator: None,
        }
    }
}

impl<PM, PQ, EC> PositionReconcilerBuilder<PM, PQ, EC>
where
    PM: PositionManager + 'static,
    PQ: PositionQuerier + 'static,
    EC: EventCoordinator + 'static,
{
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the configuration.
    pub fn config(mut self, config: PositionReconcilerConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the position querier.
    pub fn position_querier(mut self, querier: Arc<PQ>) -> Self {
        self.position_querier = Some(querier);
        self
    }

    /// Set the position manager.
    pub fn position_manager(mut self, manager: Arc<PM>) -> Self {
        self.position_manager = Some(manager);
        self
    }

    /// Set the event coordinator.
    pub fn event_coordinator(mut self, coordinator: Arc<EC>) -> Self {
        self.event_coordinator = Some(coordinator);
        self
    }

    /// Build the reconciler.
    ///
    /// # Panics
    ///
    /// Panics if any required component is not set.
    pub fn build(self) -> Arc<PositionReconciler<PM, PQ, EC>> {
        Arc::new(PositionReconciler {
            config: self.config,
            position_querier: self.position_querier.expect("position_querier is required"),
            position_manager: self.position_manager.expect("position_manager is required"),
            event_coordinator: self
                .event_coordinator
                .expect("event_coordinator is required"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = PositionReconcilerConfig::default();
        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.drift_threshold, 1e-6);
        assert!(config.assets.is_none());
    }

    #[test]
    fn test_config_builder() {
        let config = PositionReconcilerConfig::default()
            .with_interval(Duration::from_secs(60))
            .with_drift_threshold(0.01)
            .with_assets(vec!["token1".to_string(), "token2".to_string()]);

        assert_eq!(config.interval, Duration::from_secs(60));
        assert_eq!(config.drift_threshold, 0.01);
        assert_eq!(
            config.assets,
            Some(vec!["token1".to_string(), "token2".to_string()])
        );
    }
}
