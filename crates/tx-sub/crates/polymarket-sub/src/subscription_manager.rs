//! Subscription Manager
//!
//! Manages WebSocket subscriptions by receiving market updates from the discovery service,
//! calculating subscription diffs, enforcing subscription limits, and coordinating
//! WebSocket reconnections.

use crate::api_client::DiscoveredMarket;
use crate::config::MarketDiscoveryConfig;
use crate::metrics::Metrics;
use crate::types::SubscriptionState;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Message sent to App to trigger WebSocket reconnection with new asset list
#[derive(Debug, Clone)]
pub struct SubscriptionUpdate {
    /// New set of asset IDs to subscribe to
    pub asset_ids: Vec<String>,
    /// Number of markets represented
    pub markets_count: usize,
}

/// Subscription Manager
///
/// Receives market updates from discovery service, calculates subscription diffs,
/// enforces limits, and triggers WebSocket reconnections when needed.
pub struct SubscriptionManager {
    /// Shared subscription state
    state: Arc<Mutex<SubscriptionState>>,
    /// Configuration
    config: MarketDiscoveryConfig,
    /// Broadcast sender for subscription updates to App
    tx: broadcast::Sender<SubscriptionUpdate>,
    /// Prometheus metrics
    metrics: Arc<Metrics>,
}

impl SubscriptionManager {
    /// Create a new Subscription Manager
    ///
    /// # Arguments
    /// * `config` - Market discovery configuration
    /// * `metrics` - Prometheus metrics
    ///
    /// # Returns
    /// A tuple of (manager, receiver) where receiver can be used to subscribe to updates
    pub fn new(
        config: MarketDiscoveryConfig,
        metrics: Arc<Metrics>,
    ) -> (Self, broadcast::Receiver<SubscriptionUpdate>) {
        let state = Arc::new(Mutex::new(SubscriptionState::new()));

        // Create broadcast channel (buffer size 1 - only latest update matters)
        let (tx, rx) = broadcast::channel(1);

        let manager = Self {
            state,
            config,
            tx,
            metrics,
        };

        (manager, rx)
    }

    /// Run the subscription manager
    ///
    /// Receives market updates from discovery service and manages subscriptions.
    /// Runs until cancellation token is triggered.
    ///
    /// # Arguments
    /// * `discovery_rx` - Receiver for market updates from discovery service
    /// * `cancellation_token` - Token to signal shutdown
    pub async fn run(
        self,
        mut discovery_rx: broadcast::Receiver<Vec<DiscoveredMarket>>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        info!(
            max_subscriptions = self.config.max_subscriptions,
            "Subscription manager starting"
        );

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Subscription manager shutting down");
                    break;
                }
                result = discovery_rx.recv() => {
                    match result {
                        Ok(markets) => {
                            if let Err(e) = self.process_market_update(markets).await {
                                warn!(error = %e, "Failed to process market update");
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped = skipped,
                                "Subscription manager lagged behind market discovery updates"
                            );
                            // Continue processing - we'll get the latest update next
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            warn!("Market discovery channel closed, shutting down");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Process a market update from the discovery service
    async fn process_market_update(&self, markets: Vec<DiscoveredMarket>) -> Result<()> {
        debug!(
            markets_count = markets.len(),
            "Processing market update from discovery service"
        );

        // Handle empty market list (possible API issue)
        if markets.is_empty() {
            warn!("Received empty market list from discovery service, keeping current subscriptions");
            return Ok(());
        }

        // Build new asset set from discovered markets
        let (new_assets, markets_count) = self.build_asset_set(&markets);

        debug!(
            asset_count = new_assets.len(),
            markets_count = markets_count,
            "Built new asset set from discovered markets"
        );

        // Get current state and calculate diff
        let mut state = self.state.lock().await;
        let (additions, removals) = state.calculate_diff(&new_assets);

        // Check if there are any changes
        if additions.is_empty() && removals.is_empty() {
            debug!("No subscription changes detected");
            return Ok(());
        }

        info!(
            current_count = state.len(),
            new_count = new_assets.len(),
            additions = additions.len(),
            removals = removals.len(),
            "Subscription changes detected"
        );

        // Log sample additions (first 3)
        if !additions.is_empty() {
            let sample: Vec<_> = additions.iter().take(3).collect();
            debug!(
                additions = additions.len(),
                sample = ?sample,
                "Adding new subscriptions"
            );
        }

        // Log sample removals (first 3)
        if !removals.is_empty() {
            let sample: Vec<_> = removals.iter().take(3).collect();
            debug!(
                removals = removals.len(),
                sample = ?sample,
                "Removing expired subscriptions"
            );
        }

        // Apply subscription limit if needed
        let final_assets = self.apply_subscription_limit(new_assets, &markets);

        // Update state
        state.update(final_assets.clone(), markets_count);

        // Update metrics
        self.metrics.assets_subscribed.set(state.len() as f64);
        self.metrics
            .subscription_updates
            .with_label_values(&["added"])
            .inc_by(additions.len() as f64);
        self.metrics
            .subscription_updates
            .with_label_values(&["removed"])
            .inc_by(removals.len() as f64);

        // Broadcast update to App for WebSocket reconnection
        let update = SubscriptionUpdate {
            asset_ids: final_assets.into_iter().collect(),
            markets_count,
        };

        if let Err(e) = self.tx.send(update) {
            // This is not critical - it just means no one is listening
            debug!(error = %e, "No subscribers for subscription updates");
        }

        info!(
            subscriptions = state.len(),
            markets = markets_count,
            "Updated subscriptions successfully"
        );

        Ok(())
    }

    /// Build a set of asset IDs from discovered markets
    ///
    /// Flattens all asset IDs from all markets and deduplicates.
    ///
    /// Returns: (asset_set, markets_count)
    fn build_asset_set(&self, markets: &[DiscoveredMarket]) -> (HashSet<String>, usize) {
        let mut asset_set = HashSet::new();

        for market in markets {
            for asset_id in &market.asset_ids {
                asset_set.insert(asset_id.clone());
            }
        }

        (asset_set, markets.len())
    }

    /// Apply subscription limit by prioritizing markets resolving soonest
    ///
    /// If the asset count exceeds max_subscriptions, we:
    /// 1. Sort markets by end_date ascending (already done by discovery service)
    /// 2. Take assets from markets resolving soonest until we hit the limit
    /// 3. Log warning about dropped markets
    fn apply_subscription_limit(
        &self,
        mut assets: HashSet<String>,
        markets: &[DiscoveredMarket],
    ) -> HashSet<String> {
        if assets.len() <= self.config.max_subscriptions {
            // Under limit, no action needed
            return assets;
        }

        warn!(
            current = assets.len(),
            max = self.config.max_subscriptions,
            "Asset count exceeds subscription limit, applying prioritization"
        );

        // Rebuild asset set by taking markets in order until we hit the limit
        assets.clear();
        let mut markets_included = 0;
        let mut markets_dropped = 0;

        for market in markets {
            // Check if adding this market would exceed the limit
            let new_total = assets.len() + market.asset_ids.len();

            if new_total <= self.config.max_subscriptions {
                // Add all assets from this market
                for asset_id in &market.asset_ids {
                    assets.insert(asset_id.clone());
                }
                markets_included += 1;
            } else {
                // Would exceed limit, stop adding markets
                markets_dropped += 1;
                debug!(
                    ticker = %market.ticker,
                    end_date = %market.end_date,
                    "Dropping market due to subscription limit"
                );
            }

            // Stop if we've hit the limit exactly
            if assets.len() >= self.config.max_subscriptions {
                markets_dropped += markets.len() - markets_included;
                break;
            }
        }

        warn!(
            final_count = assets.len(),
            markets_included = markets_included,
            markets_dropped = markets_dropped,
            "Applied subscription limit"
        );

        // Update metrics
        if markets_dropped > 0 {
            self.metrics
                .markets_dropped
                .with_label_values(&["limit_exceeded"])
                .inc_by(markets_dropped as f64);
        }

        assets
    }

    /// Get a clone of the current subscription state (for testing/debugging)
    #[allow(dead_code)]
    pub async fn get_state(&self) -> SubscriptionState {
        self.state.lock().await.clone()
    }

    /// Get a reference to the subscription state (for health monitoring)
    pub fn state_ref(&self) -> Arc<Mutex<SubscriptionState>> {
        Arc::clone(&self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use prometheus::Registry;

    fn create_test_config(max_subscriptions: usize) -> MarketDiscoveryConfig {
        MarketDiscoveryConfig {
            enabled: true,
            api_base_url: "https://gamma-api.polymarket.com".to_string(),
            tag_id: 21,
            ticker_patterns: vec!["btc-updown-15m-".to_string()],
            discovery_interval_secs: 300,
            max_subscriptions,
            api_timeout_secs: 30,
            api_retry_attempts: 3,
            api_retry_backoff_ms: 1000,
        }
    }

    fn create_test_metrics() -> Arc<Metrics> {
        let registry = Registry::new();
        Arc::new(Metrics::new(&registry).unwrap())
    }

    fn create_test_market(
        id: &str,
        ticker: &str,
        asset_count: usize,
        end_date_offset_secs: i64,
    ) -> DiscoveredMarket {
        let asset_ids: Vec<String> = (0..asset_count)
            .map(|i| format!("asset_{}_{}", id, i))
            .collect();

        DiscoveredMarket {
            event_id: id.to_string(),
            ticker: ticker.to_string(),
            title: format!("Test Market {}", id),
            end_date: Utc::now() + chrono::Duration::seconds(end_date_offset_secs),
            condition_id: format!("0xcond{}", id),
            asset_ids,
        }
    }

    #[test]
    fn test_build_asset_set() {
        let config = create_test_config(1000);
        let metrics = create_test_metrics();
        let (manager, _rx) = SubscriptionManager::new(config, metrics);

        let markets = vec![
            create_test_market("1", "btc-updown-15m-1", 2, 100),
            create_test_market("2", "btc-updown-15m-2", 2, 200),
            create_test_market("3", "btc-updown-15m-3", 2, 300),
        ];

        let (assets, count) = manager.build_asset_set(&markets);

        assert_eq!(count, 3);
        assert_eq!(assets.len(), 6); // 3 markets * 2 assets each
    }

    #[test]
    fn test_apply_subscription_limit_under_limit() {
        let config = create_test_config(100);
        let metrics = create_test_metrics();
        let (manager, _rx) = SubscriptionManager::new(config, metrics);

        let markets = vec![
            create_test_market("1", "btc-updown-15m-1", 2, 100),
            create_test_market("2", "btc-updown-15m-2", 2, 200),
        ];

        let (assets, _) = manager.build_asset_set(&markets);
        let final_assets = manager.apply_subscription_limit(assets.clone(), &markets);

        // Should keep all assets since under limit
        assert_eq!(final_assets.len(), 4);
        assert_eq!(final_assets, assets);
    }

    #[test]
    fn test_apply_subscription_limit_over_limit() {
        let config = create_test_config(3); // Very low limit
        let metrics = create_test_metrics();
        let (manager, _rx) = SubscriptionManager::new(config, metrics);

        // Create markets with different end dates (already sorted by discovery service)
        let markets = vec![
            create_test_market("1", "btc-updown-15m-1", 2, 100),  // Earlier, should be kept
            create_test_market("2", "btc-updown-15m-2", 2, 200),  // Later, should be dropped
            create_test_market("3", "btc-updown-15m-3", 2, 300),  // Even later, should be dropped
        ];

        let (assets, _) = manager.build_asset_set(&markets);
        assert_eq!(assets.len(), 6); // 3 markets * 2 assets each

        let final_assets = manager.apply_subscription_limit(assets, &markets);

        // Should only keep assets from first market (2 assets) to stay under limit of 3
        assert_eq!(final_assets.len(), 2);
        assert!(final_assets.contains("asset_1_0"));
        assert!(final_assets.contains("asset_1_1"));
    }

    #[test]
    fn test_apply_subscription_limit_exact_limit() {
        let config = create_test_config(4); // Exactly 2 markets worth
        let metrics = create_test_metrics();
        let (manager, _rx) = SubscriptionManager::new(config, metrics);

        let markets = vec![
            create_test_market("1", "btc-updown-15m-1", 2, 100),
            create_test_market("2", "btc-updown-15m-2", 2, 200),
            create_test_market("3", "btc-updown-15m-3", 2, 300),
        ];

        let (assets, _) = manager.build_asset_set(&markets);
        let final_assets = manager.apply_subscription_limit(assets, &markets);

        // Should keep first 2 markets (4 assets total)
        assert_eq!(final_assets.len(), 4);
    }

    #[tokio::test]
    async fn test_process_market_update_no_changes() {
        let config = create_test_config(1000);
        let metrics = create_test_metrics();
        let (manager, _rx) = SubscriptionManager::new(config, metrics);

        // Set initial state
        let mut initial_assets = HashSet::new();
        initial_assets.insert("asset1".to_string());
        initial_assets.insert("asset2".to_string());
        {
            let mut state = manager.state.lock().await;
            state.update(initial_assets.clone(), 1);
        }

        // Send same assets again
        let markets = vec![DiscoveredMarket {
            event_id: "1".to_string(),
            ticker: "btc-updown-15m-1".to_string(),
            title: "Bitcoin Up or Down - Test 1".to_string(),
            end_date: Utc::now() + chrono::Duration::seconds(100),
            condition_id: "0xtest1".to_string(),
            asset_ids: vec!["asset1".to_string(), "asset2".to_string()],
        }];

        manager.process_market_update(markets).await.unwrap();

        // State should be unchanged
        let state = manager.state.lock().await;
        assert_eq!(state.len(), 2);
    }

    #[tokio::test]
    async fn test_process_market_update_with_changes() {
        let config = create_test_config(1000);
        let metrics = create_test_metrics();
        let (manager, mut rx) = SubscriptionManager::new(config, metrics);

        // Set initial state
        let mut initial_assets = HashSet::new();
        initial_assets.insert("asset1".to_string());
        initial_assets.insert("asset2".to_string());
        {
            let mut state = manager.state.lock().await;
            state.update(initial_assets, 1);
        }

        // Send different assets
        let markets = vec![DiscoveredMarket {
            event_id: "2".to_string(),
            ticker: "btc-updown-15m-2".to_string(),
            title: "Bitcoin Up or Down - Test 2".to_string(),
            end_date: Utc::now() + chrono::Duration::seconds(200),
            condition_id: "0xtest2".to_string(),
            asset_ids: vec!["asset2".to_string(), "asset3".to_string()],
        }];

        manager.process_market_update(markets).await.unwrap();

        // State should be updated
        let state = manager.state.lock().await;
        assert_eq!(state.len(), 2);
        assert!(state.current_assets.contains("asset2"));
        assert!(state.current_assets.contains("asset3"));
        assert!(!state.current_assets.contains("asset1"));

        // Should have received update
        let update = rx.try_recv().unwrap();
        assert_eq!(update.asset_ids.len(), 2);
        assert_eq!(update.markets_count, 1);
    }

    #[tokio::test]
    async fn test_process_market_update_empty_list() {
        let config = create_test_config(1000);
        let metrics = create_test_metrics();
        let (manager, _rx) = SubscriptionManager::new(config, metrics);

        // Set initial state
        let mut initial_assets = HashSet::new();
        initial_assets.insert("asset1".to_string());
        {
            let mut state = manager.state.lock().await;
            state.update(initial_assets, 1);
        }

        // Send empty market list
        let markets = vec![];
        manager.process_market_update(markets).await.unwrap();

        // State should be unchanged (we keep current subscriptions)
        let state = manager.state.lock().await;
        assert_eq!(state.len(), 1);
        assert!(state.current_assets.contains("asset1"));
    }
}
