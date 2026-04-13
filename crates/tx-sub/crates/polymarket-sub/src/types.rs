//! Data structures for market discovery, subscription state, health monitoring, and parsed events

use chrono::{DateTime, Utc};
use popeyes_trading_types::TradeSide;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Market metadata for a crypto binary prediction market
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MarketMetadata {
    /// Polymarket event ID
    pub event_id: String,
    /// Market ticker (e.g., "sol-updown-15m-1763489700")
    pub ticker: String,
    /// Market resolution time
    pub end_date: DateTime<Utc>,
    /// CLOB token IDs for this market (typically 2: "Up" and "Down")
    pub asset_ids: Vec<String>,
    /// When this market was first discovered
    pub discovered_at: DateTime<Utc>,
}

/// Subscription state tracking current WebSocket subscriptions
#[derive(Debug, Clone)]
pub struct SubscriptionState {
    /// Asset IDs currently subscribed to WebSocket
    pub current_assets: HashSet<String>,
    /// When subscription list was last refreshed
    pub last_updated: DateTime<Utc>,
    /// Number of markets represented by current subscriptions
    pub total_markets: usize,
}

impl SubscriptionState {
    /// Create a new empty subscription state
    pub fn new() -> Self {
        Self {
            current_assets: HashSet::new(),
            last_updated: Utc::now(),
            total_markets: 0,
        }
    }

    /// Calculate diff between current and new asset sets
    /// Returns (additions, removals)
    pub fn calculate_diff(&self, new_assets: &HashSet<String>) -> (Vec<String>, Vec<String>) {
        // Assets in new but not in current (additions)
        let additions: Vec<String> = new_assets
            .difference(&self.current_assets)
            .cloned()
            .collect();

        // Assets in current but not in new (removals)
        let removals: Vec<String> = self
            .current_assets
            .difference(new_assets)
            .cloned()
            .collect();

        (additions, removals)
    }

    /// Update subscription state with new asset set
    pub fn update(&mut self, new_assets: HashSet<String>, markets_count: usize) {
        self.current_assets = new_assets;
        self.last_updated = Utc::now();
        self.total_markets = markets_count;
    }

    /// Get current subscription count
    pub fn len(&self) -> usize {
        self.current_assets.len()
    }

    /// Check if subscription state is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.current_assets.is_empty()
    }
}

impl Default for SubscriptionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Event rate tracker for health monitoring
#[derive(Debug)]
pub struct EventRateTracker {
    /// Ring buffer of event counts per second (keyed by unix timestamp)
    events_per_second: HashMap<u64, u64>,
    /// Cumulative event counter
    pub total_events: u64,
}

impl EventRateTracker {
    /// Create a new event rate tracker
    pub fn new() -> Self {
        Self {
            events_per_second: HashMap::new(),
            total_events: 0,
        }
    }

    /// Record an event at current timestamp
    pub fn record_event(&mut self) {
        let now = Utc::now().timestamp() as u64;
        *self.events_per_second.entry(now).or_insert(0) += 1;
        self.total_events += 1;
    }

    /// Get event rate per minute over the last 60 seconds
    pub fn get_rate_per_minute(&self) -> u64 {
        let now = Utc::now().timestamp() as u64;
        let cutoff = now.saturating_sub(60);

        self.events_per_second
            .iter()
            .filter(|(timestamp, _)| **timestamp >= cutoff)
            .map(|(_, count)| count)
            .sum()
    }

    /// Prune old entries to prevent unbounded growth
    pub fn prune_old(&mut self, cutoff: DateTime<Utc>) {
        let cutoff_ts = cutoff.timestamp() as u64;
        self.events_per_second.retain(|timestamp, _| *timestamp >= cutoff_ts);
    }
}

impl Default for EventRateTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal parsed price change event (orderbook update)
/// This uses numeric types for performance, unlike the raw WebSocket format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedPriceChangeEvent {
    /// Market/condition identifier
    pub market: String,

    /// Array of price level changes in this update
    pub price_changes: Vec<ParsedPriceChange>,

    /// Unix timestamp in milliseconds (when the event occurred on the exchange)
    pub timestamp: i64,

    /// When the subscriber service received this event (UTC)
    /// Used for cross-feed event ordering and latency analysis
    pub observed_at: DateTime<Utc>,
}

/// Individual parsed price level change
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedPriceChange {
    /// Token identifier (256-bit integer as decimal string)
    pub asset_id: String,

    /// Price level (0-1 probability range)
    pub price: f64,

    /// Aggregate size at this price level
    pub size: f64,

    /// Order side (reuse existing TradeSide enum)
    pub side: TradeSide,

    /// Order hash identifier
    pub hash: String,

    /// Current best bid price (0-1 range)
    pub best_bid: f64,

    /// Current best ask price (0-1 range)
    pub best_ask: f64,
}

/// Parsed book (orderbook snapshot) event
/// Emitted when first subscribed to a market or when a trade affects the book
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedBookEvent {
    /// Token identifier (256-bit integer as decimal string)
    pub asset_id: String,

    /// Market/condition identifier
    pub market: String,

    /// Bid price levels (buyers)
    pub bids: Vec<ParsedOrderSummary>,

    /// Ask price levels (sellers)
    pub asks: Vec<ParsedOrderSummary>,

    /// Hash summary of the orderbook content
    pub hash: String,

    /// Unix timestamp in milliseconds (when the event occurred on the exchange)
    pub timestamp: i64,

    /// When the subscriber service received this event (UTC)
    /// Used for cross-feed event ordering and latency analysis
    pub observed_at: DateTime<Utc>,
}

/// Individual price level in an orderbook snapshot
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedOrderSummary {
    /// Price level (0-1 probability range)
    pub price: f64,

    /// Size available at this price level
    pub size: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_state_diff_additions_only() {
        let mut state = SubscriptionState::new();
        state.update(HashSet::new(), 0);

        let mut new_assets = HashSet::new();
        new_assets.insert("asset1".to_string());
        new_assets.insert("asset2".to_string());
        new_assets.insert("asset3".to_string());

        let (additions, removals) = state.calculate_diff(&new_assets);

        assert_eq!(additions.len(), 3);
        assert_eq!(removals.len(), 0);
        assert!(additions.contains(&"asset1".to_string()));
        assert!(additions.contains(&"asset2".to_string()));
        assert!(additions.contains(&"asset3".to_string()));
    }

    #[test]
    fn test_subscription_state_diff_removals_only() {
        let mut state = SubscriptionState::new();
        let mut current = HashSet::new();
        current.insert("asset1".to_string());
        current.insert("asset2".to_string());
        current.insert("asset3".to_string());
        state.update(current, 3);

        let new_assets = HashSet::new();
        let (additions, removals) = state.calculate_diff(&new_assets);

        assert_eq!(additions.len(), 0);
        assert_eq!(removals.len(), 3);
        assert!(removals.contains(&"asset1".to_string()));
        assert!(removals.contains(&"asset2".to_string()));
        assert!(removals.contains(&"asset3".to_string()));
    }

    #[test]
    fn test_subscription_state_diff_mixed() {
        let mut state = SubscriptionState::new();
        let mut current = HashSet::new();
        current.insert("asset1".to_string());
        current.insert("asset2".to_string());
        current.insert("asset3".to_string());
        current.insert("asset4".to_string());
        current.insert("asset5".to_string());
        state.update(current, 5);

        let mut new_assets = HashSet::new();
        new_assets.insert("asset1".to_string()); // unchanged
        new_assets.insert("asset2".to_string()); // unchanged
        new_assets.insert("asset6".to_string()); // added
        new_assets.insert("asset7".to_string()); // added
        new_assets.insert("asset8".to_string()); // added
        new_assets.insert("asset9".to_string()); // added
        // asset3, asset4, asset5 removed

        let (additions, removals) = state.calculate_diff(&new_assets);

        assert_eq!(additions.len(), 4);
        assert_eq!(removals.len(), 3);
        assert!(additions.contains(&"asset6".to_string()));
        assert!(additions.contains(&"asset7".to_string()));
        assert!(additions.contains(&"asset8".to_string()));
        assert!(additions.contains(&"asset9".to_string()));
        assert!(removals.contains(&"asset3".to_string()));
        assert!(removals.contains(&"asset4".to_string()));
        assert!(removals.contains(&"asset5".to_string()));
    }

    #[test]
    fn test_subscription_state_update() {
        let mut state = SubscriptionState::new();
        assert_eq!(state.len(), 0);
        assert!(state.is_empty());

        let mut assets = HashSet::new();
        assets.insert("asset1".to_string());
        assets.insert("asset2".to_string());

        state.update(assets, 2);
        assert_eq!(state.len(), 2);
        assert!(!state.is_empty());
        assert_eq!(state.total_markets, 2);
    }

    #[test]
    fn test_event_rate_tracker_record_event() {
        let mut tracker = EventRateTracker::new();
        assert_eq!(tracker.total_events, 0);

        tracker.record_event();
        assert_eq!(tracker.total_events, 1);

        tracker.record_event();
        tracker.record_event();
        assert_eq!(tracker.total_events, 3);
    }

    #[test]
    fn test_event_rate_tracker_get_rate_per_minute() {
        let mut tracker = EventRateTracker::new();

        // Record 10 events
        for _ in 0..10 {
            tracker.record_event();
        }

        let rate = tracker.get_rate_per_minute();
        assert_eq!(rate, 10);
    }

    #[test]
    fn test_event_rate_tracker_prune_old() {
        let mut tracker = EventRateTracker::new();

        // Record events at old timestamp
        let old_ts = Utc::now().timestamp() as u64 - 200;
        tracker.events_per_second.insert(old_ts, 5);
        tracker.total_events = 5;

        assert_eq!(tracker.events_per_second.len(), 1);

        // Prune entries older than 120 seconds
        let cutoff = Utc::now() - chrono::Duration::seconds(120);
        tracker.prune_old(cutoff);

        // Old entry should be removed
        assert_eq!(tracker.events_per_second.len(), 0);
        // Total events counter should not be affected
        assert_eq!(tracker.total_events, 5);
    }
}
