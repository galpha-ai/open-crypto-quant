//! Orderbook Tracker Component
//!
//! Maintains orderbook state per market, applies incremental updates to produce
//! normalized snapshots, and evicts inactive markets after a configurable threshold.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use popeyes_trading_types::{
    OrderSummary, OrderbookSnapshotEvent, OrderbookUpdateEvent, TradeSide,
};
use tracing::debug;

/// Default eviction threshold (1 hour)
const DEFAULT_EVICTION_THRESHOLD: Duration = Duration::from_secs(3600);
const PRICE_EPSILON: f64 = 1e-9;

/// Tracks orderbook state for a single market
#[derive(Debug, Clone)]
struct TrackedOrderbook {
    /// Current orderbook snapshot
    snapshot: OrderbookSnapshotEvent,
    /// Last update timestamp for eviction tracking
    last_update: DateTime<Utc>,
}

/// Maintains orderbook state per asset, applies incremental updates,
/// and evicts inactive assets.
#[derive(Debug)]
pub struct OrderbookTracker {
    /// Current orderbook state per asset (keyed by asset_id, NOT market ID)
    /// NOTE: Binary markets like Polymarket have two assets (Up/Down) per market,
    /// each with its own orderbook.
    orderbooks: HashMap<String, TrackedOrderbook>,
    /// Eviction threshold for inactive markets
    eviction_threshold: Duration,
}

impl Default for OrderbookTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderbookTracker {
    /// Create a new OrderbookTracker with the default eviction threshold (1 hour)
    pub fn new() -> Self {
        Self {
            orderbooks: HashMap::new(),
            eviction_threshold: DEFAULT_EVICTION_THRESHOLD,
        }
    }

    /// Create a new OrderbookTracker with a custom eviction threshold
    pub fn with_eviction_threshold(eviction_threshold: Duration) -> Self {
        Self {
            orderbooks: HashMap::new(),
            eviction_threshold,
        }
    }

    /// Apply an orderbook update event and return the resulting snapshot.
    ///
    /// Returns `None` if there's no existing snapshot for the asset (need snapshot first).
    pub fn apply_update(
        &mut self,
        update: &OrderbookUpdateEvent,
    ) -> Option<OrderbookSnapshotEvent> {
        let asset_id = &update.asset_id;

        let tracked = match self.orderbooks.get_mut(asset_id) {
            Some(tracked) => tracked,
            None => {
                debug!(
                    asset_id = %asset_id,
                    market = %update.market,
                    "Received orderbook update without prior snapshot, ignoring"
                );
                return None;
            }
        };

        // Apply the update to the orderbook
        let snapshot = &mut tracked.snapshot;

        match update.side {
            TradeSide::Buy => {
                Self::apply_level_update(&mut snapshot.bids, update.price, update.size, true);
            }
            TradeSide::Sell => {
                Self::apply_level_update(&mut snapshot.asks, update.price, update.size, false);
            }
        }

        // Align top-of-book levels with the update-provided BBO.
        Self::apply_best_bid_ask(snapshot, update);

        // Update timestamp and hash
        snapshot.timestamp = update.timestamp;
        snapshot.hash = update.hash.clone();
        snapshot.observed_at = update.observed_at;

        // Update last_update for eviction tracking
        tracked.last_update =
            DateTime::from_timestamp_millis(update.timestamp).unwrap_or_else(Utc::now);

        debug!(
            asset_id = %asset_id,
            market = %update.market,
            side = ?update.side,
            price = update.price,
            size = update.size,
            bids_count = snapshot.bids.len(),
            asks_count = snapshot.asks.len(),
            "Applied orderbook update"
        );

        Some(snapshot.clone())
    }

    fn apply_best_bid_ask(snapshot: &mut OrderbookSnapshotEvent, update: &OrderbookUpdateEvent) {
        Self::align_best_level(&mut snapshot.bids, update.best_bid, update, TradeSide::Buy);
        Self::align_best_level(&mut snapshot.asks, update.best_ask, update, TradeSide::Sell);
    }

    fn align_best_level(
        levels: &mut Vec<OrderSummary>,
        best_price: f64,
        update: &OrderbookUpdateEvent,
        side: TradeSide,
    ) {
        if side == TradeSide::Buy {
            levels.retain(|l| l.price <= best_price + PRICE_EPSILON);
        } else {
            levels.retain(|l| l.price >= best_price - PRICE_EPSILON);
        }

        let has_best = levels
            .iter()
            .any(|l| (l.price - best_price).abs() < PRICE_EPSILON);
        if !has_best {
            if let Some(top) = levels.first_mut() {
                let size = Self::best_level_size(best_price, update, &side);
                top.price = best_price;
                if size > 0.0 {
                    top.size = size;
                }
            } else {
                levels.push(OrderSummary {
                    price: best_price,
                    size: Self::best_level_size(best_price, update, &side),
                });
            }
        }

        if side == TradeSide::Buy {
            levels.sort_by(|a, b| {
                b.price
                    .partial_cmp(&a.price)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            levels.sort_by(|a, b| {
                a.price
                    .partial_cmp(&b.price)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    fn best_level_size(best_price: f64, update: &OrderbookUpdateEvent, side: &TradeSide) -> f64 {
        if update.side == *side && (update.price - best_price).abs() < PRICE_EPSILON {
            update.size
        } else {
            0.0
        }
    }

    /// Apply a price level update to a side of the orderbook
    fn apply_level_update(levels: &mut Vec<OrderSummary>, price: f64, size: f64, is_bids: bool) {
        // Find the index of the existing level at this price
        let existing_idx = levels
            .iter()
            .position(|l| (l.price - price).abs() < f64::EPSILON);

        if size == 0.0 {
            // Remove the price level
            if let Some(idx) = existing_idx {
                levels.remove(idx);
            }
        } else if let Some(idx) = existing_idx {
            // Update existing level
            levels[idx].size = size;
        } else {
            // Insert new level
            levels.push(OrderSummary { price, size });
            // Re-sort: bids descending, asks ascending
            if is_bids {
                levels.sort_by(|a, b| {
                    b.price
                        .partial_cmp(&a.price)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            } else {
                levels.sort_by(|a, b| {
                    a.price
                        .partial_cmp(&b.price)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
    }

    /// Replace state with a new snapshot and return it.
    pub fn apply_snapshot(&mut self, snapshot: &OrderbookSnapshotEvent) -> OrderbookSnapshotEvent {
        let asset_id = snapshot.asset_id.clone();
        let now = DateTime::from_timestamp_millis(snapshot.timestamp).unwrap_or_else(Utc::now);

        // Log input snapshot details for debugging
        let input_best_bid = snapshot.bids.first().map(|b| b.price);
        let input_best_ask = snapshot.asks.first().map(|a| a.price);
        let outcome = snapshot
            .market_metadata
            .as_ref()
            .and_then(|m| m.outcome.as_ref());
        let ticker = snapshot.market_metadata.as_ref().map(|m| m.ticker.as_str());

        tracing::debug!(
            asset_id = %asset_id,
            market = %snapshot.market,
            ticker = ?ticker,
            outcome = ?outcome,
            input_best_bid = ?input_best_bid,
            input_best_ask = ?input_best_ask,
            bids_count = snapshot.bids.len(),
            asks_count = snapshot.asks.len(),
            "Applied orderbook snapshot"
        );

        self.orderbooks.insert(
            asset_id,
            TrackedOrderbook {
                snapshot: snapshot.clone(),
                last_update: now,
            },
        );

        snapshot.clone()
    }

    /// Get current snapshot for an asset (read-only).
    pub fn get_orderbook(&self, asset_id: &str) -> Option<&OrderbookSnapshotEvent> {
        self.orderbooks.get(asset_id).map(|t| &t.snapshot)
    }

    /// Get the number of tracked markets.
    pub fn market_count(&self) -> usize {
        self.orderbooks.len()
    }

    /// Evict inactive markets (those not updated within the eviction threshold).
    ///
    /// Returns a list of evicted market IDs.
    pub fn evict_inactive(&mut self) -> Vec<String> {
        let now = Utc::now();
        let threshold = chrono::Duration::from_std(self.eviction_threshold)
            .unwrap_or_else(|_| chrono::Duration::hours(1));

        let evicted: Vec<String> = self
            .orderbooks
            .iter()
            .filter_map(|(market, tracked)| {
                if now - tracked.last_update > threshold {
                    Some(market.clone())
                } else {
                    None
                }
            })
            .collect();

        for market in &evicted {
            self.orderbooks.remove(market);
            debug!(market = %market, "Evicted inactive orderbook");
        }

        if !evicted.is_empty() {
            debug!(
                evicted_count = evicted.len(),
                remaining_count = self.orderbooks.len(),
                "Evicted inactive orderbooks"
            );
        }

        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use popeyes_trading_types::OrderbookSource;

    fn create_test_snapshot(
        market: &str,
        asset_id: &str,
        bids: Vec<(f64, f64)>,
        asks: Vec<(f64, f64)>,
    ) -> OrderbookSnapshotEvent {
        OrderbookSnapshotEvent {
            asset_id: asset_id.to_string(),
            market: market.to_string(),
            bids: bids
                .into_iter()
                .map(|(price, size)| OrderSummary { price, size })
                .collect(),
            asks: asks
                .into_iter()
                .map(|(price, size)| OrderSummary { price, size })
                .collect(),
            hash: "test_hash".to_string(),
            timestamp: Utc::now().timestamp_millis(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
            observed_at: Utc::now(),
        }
    }

    fn create_test_update(
        market: &str,
        asset_id: &str,
        side: TradeSide,
        price: f64,
        size: f64,
        best_bid: f64,
        best_ask: f64,
    ) -> OrderbookUpdateEvent {
        OrderbookUpdateEvent {
            asset_id: asset_id.to_string(),
            market: market.to_string(),
            price,
            size,
            side,
            hash: "update_hash".to_string(),
            best_bid,
            best_ask,
            timestamp: Utc::now().timestamp_millis(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
            observed_at: Utc::now(),
        }
    }

    #[test]
    fn test_new_tracker_is_empty() {
        let tracker = OrderbookTracker::new();
        assert_eq!(tracker.market_count(), 0);
    }

    #[test]
    fn test_apply_update_without_snapshot_returns_none() {
        let mut tracker = OrderbookTracker::new();
        let update = create_test_update("market1", "asset1", TradeSide::Buy, 0.5, 100.0, 0.5, 0.6);

        let result = tracker.apply_update(&update);

        assert!(result.is_none());
        assert_eq!(tracker.market_count(), 0);
    }

    #[test]
    fn test_apply_snapshot_stores_orderbook() {
        let mut tracker = OrderbookTracker::new();
        let snapshot = create_test_snapshot(
            "market1",
            "asset1",
            vec![(0.5, 100.0), (0.49, 200.0)],
            vec![(0.51, 150.0), (0.52, 250.0)],
        );

        let result = tracker.apply_snapshot(&snapshot);

        assert_eq!(tracker.market_count(), 1);
        assert_eq!(result.market, "market1");
        assert_eq!(result.bids.len(), 2);
        assert_eq!(result.asks.len(), 2);

        let stored = tracker.get_orderbook("asset1").unwrap();
        assert_eq!(stored.bids.len(), 2);
        assert_eq!(stored.asks.len(), 2);
    }

    #[test]
    fn test_apply_update_after_snapshot() {
        let mut tracker = OrderbookTracker::new();
        let mut snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        snapshot.timestamp = 1_000;
        snapshot.observed_at = Utc.timestamp_millis_opt(900).single().unwrap();
        tracker.apply_snapshot(&snapshot);

        // Add a new bid level
        let mut update =
            create_test_update("market1", "asset1", TradeSide::Buy, 0.49, 200.0, 0.5, 0.51);
        update.timestamp = 2_000;
        update.observed_at = Utc.timestamp_millis_opt(1_900).single().unwrap();
        let result = tracker.apply_update(&update).unwrap();

        assert_eq!(result.bids.len(), 2);
        // Bids should be sorted descending
        assert_eq!(result.bids[0].price, 0.5);
        assert_eq!(result.bids[1].price, 0.49);
        assert_eq!(result.bids[1].size, 200.0);
        assert_eq!(result.timestamp, update.timestamp);
        assert_eq!(result.observed_at, update.observed_at);
    }

    #[test]
    fn test_apply_update_modifies_existing_level() {
        let mut tracker = OrderbookTracker::new();
        let snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        tracker.apply_snapshot(&snapshot);

        // Update existing bid level
        let update = create_test_update("market1", "asset1", TradeSide::Buy, 0.5, 300.0, 0.5, 0.51);
        let result = tracker.apply_update(&update).unwrap();

        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].price, 0.5);
        assert_eq!(result.bids[0].size, 300.0);
    }

    #[test]
    fn test_apply_update_removes_level_on_zero_size() {
        let mut tracker = OrderbookTracker::new();
        let snapshot = create_test_snapshot(
            "market1",
            "asset1",
            vec![(0.5, 100.0), (0.49, 200.0)],
            vec![(0.51, 150.0)],
        );
        tracker.apply_snapshot(&snapshot);

        // Remove bid level with size=0
        let update = create_test_update("market1", "asset1", TradeSide::Buy, 0.5, 0.0, 0.49, 0.51);
        let result = tracker.apply_update(&update).unwrap();

        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].price, 0.49);
    }

    #[test]
    fn test_apply_update_ask_side() {
        let mut tracker = OrderbookTracker::new();
        let snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        tracker.apply_snapshot(&snapshot);

        // Add a new ask level
        let update =
            create_test_update("market1", "asset1", TradeSide::Sell, 0.52, 250.0, 0.5, 0.51);
        let result = tracker.apply_update(&update).unwrap();

        assert_eq!(result.asks.len(), 2);
        // Asks should be sorted ascending
        assert_eq!(result.asks[0].price, 0.51);
        assert_eq!(result.asks[1].price, 0.52);
        assert_eq!(result.asks[1].size, 250.0);
    }

    #[test]
    fn test_multiple_markets() {
        let mut tracker = OrderbookTracker::new();

        let snapshot1 =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        let snapshot2 =
            create_test_snapshot("market2", "asset2", vec![(0.6, 200.0)], vec![(0.61, 250.0)]);

        tracker.apply_snapshot(&snapshot1);
        tracker.apply_snapshot(&snapshot2);

        assert_eq!(tracker.market_count(), 2);
        assert!(tracker.get_orderbook("asset1").is_some());
        assert!(tracker.get_orderbook("asset2").is_some());
    }

    #[test]
    fn test_evict_inactive_no_evictions() {
        let mut tracker = OrderbookTracker::new();
        let snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        tracker.apply_snapshot(&snapshot);

        let evicted = tracker.evict_inactive();

        assert!(evicted.is_empty());
        assert_eq!(tracker.market_count(), 1);
    }

    #[test]
    fn test_evict_inactive_with_short_threshold() {
        let mut tracker = OrderbookTracker::with_eviction_threshold(Duration::from_millis(1));

        // Create a snapshot with an old timestamp
        let mut snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        snapshot.timestamp = (Utc::now() - chrono::Duration::hours(2)).timestamp_millis();
        tracker.apply_snapshot(&snapshot);

        // Wait a tiny bit to ensure time passes
        std::thread::sleep(Duration::from_millis(5));

        let evicted = tracker.evict_inactive();

        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], "asset1");
        assert_eq!(tracker.market_count(), 0);
    }

    #[test]
    fn test_get_orderbook_nonexistent() {
        let tracker = OrderbookTracker::new();
        assert!(tracker.get_orderbook("nonexistent").is_none());
    }

    #[test]
    fn test_snapshot_overwrites_previous() {
        let mut tracker = OrderbookTracker::new();

        let snapshot1 =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        tracker.apply_snapshot(&snapshot1);

        let snapshot2 =
            create_test_snapshot("market1", "asset1", vec![(0.6, 200.0)], vec![(0.61, 250.0)]);
        tracker.apply_snapshot(&snapshot2);

        assert_eq!(tracker.market_count(), 1);
        let stored = tracker.get_orderbook("asset1").unwrap();
        assert_eq!(stored.bids[0].price, 0.6);
        assert_eq!(stored.asks[0].price, 0.61);
    }

    #[test]
    fn test_update_preserves_metadata() {
        let mut tracker = OrderbookTracker::new();

        let mut snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.5, 100.0)], vec![(0.51, 150.0)]);
        snapshot.market_metadata = Some(popeyes_trading_types::PolymarketMarketMetadata {
            event_id: "event123".to_string(),
            ticker: "ticker123".to_string(),
            title: "Test Market".to_string(),
            end_date: "2025-12-31".to_string(),
            outcome: Some("Yes".to_string()),
        });
        tracker.apply_snapshot(&snapshot);

        let update =
            create_test_update("market1", "asset1", TradeSide::Buy, 0.49, 200.0, 0.5, 0.51);
        let result = tracker.apply_update(&update).unwrap();

        // Metadata should be preserved from the snapshot
        assert!(result.market_metadata.is_some());
        assert_eq!(result.market_metadata.unwrap().event_id, "event123");
    }

    #[test]
    fn test_update_overrides_best_bid_ask() {
        let mut tracker = OrderbookTracker::new();
        let snapshot =
            create_test_snapshot("market1", "asset1", vec![(0.6, 100.0)], vec![(0.61, 150.0)]);
        tracker.apply_snapshot(&snapshot);

        let update =
            create_test_update("market1", "asset1", TradeSide::Buy, 0.55, 100.0, 0.55, 0.65);
        let result = tracker.apply_update(&update).unwrap();

        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].price, 0.55);
        assert_eq!(result.asks.len(), 1);
        assert_eq!(result.asks[0].price, 0.65);
        assert_eq!(result.asks[0].size, 0.0);
    }
}
