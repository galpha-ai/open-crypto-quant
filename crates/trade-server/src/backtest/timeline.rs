//! Backtest timeline for merging and sequencing events.
//!
//! Merges orderbook snapshots and trade events chronologically,
//! grouping trades with their associated snapshots.

use popeyes_trading_types::{OrderbookSnapshotEvent, PolymarketTradeEvent, SpotPriceUpdate};
use tracing::{debug, info};

use super::error::BacktestError;
use super::types::BacktestTick;

/// Timeline of backtest events, providing iteration over ticks.
///
/// Merges orderbook snapshots and trade events chronologically.
/// Each tick contains a snapshot and all trades that occurred
/// between this snapshot and the next.
pub struct BacktestTimeline {
    /// All ticks in chronological order
    ticks: Vec<BacktestTick>,
    /// Current position in the timeline
    current_index: usize,
}

impl BacktestTimeline {
    /// Create a new timeline from snapshots, trades, and optional spot price updates.
    ///
    /// Snapshots, trades, and spot prices are merged chronologically. Trades and spot prices
    /// are assigned to the most recent snapshot (the last snapshot with timestamp <= event timestamp).
    ///
    /// # Arguments
    /// * `snapshots` - Orderbook snapshots (must be sorted by timestamp)
    /// * `trades` - Trade events (must be sorted by timestamp)
    /// * `spot_events` - Optional spot market price updates (must be sorted by timestamp)
    ///
    /// # Edge Cases
    /// - Trades before the first snapshot are discarded
    /// - Spot prices before the first snapshot are assigned to the first tick
    /// - Trades/spot prices after the last snapshot are included in the last tick
    pub fn new(
        mut snapshots: Vec<OrderbookSnapshotEvent>,
        mut trades: Vec<PolymarketTradeEvent>,
        spot_events: Option<Vec<SpotPriceUpdate>>,
    ) -> Result<Self, BacktestError> {
        if snapshots.is_empty() {
            return Err(BacktestError::EmptyDataError(
                "No snapshots provided for timeline".to_string(),
            ));
        }

        // Ensure sorted
        snapshots.sort_by_key(|s| s.timestamp);
        trades.sort_by_key(|t| t.timestamp);

        // Create ticks from snapshots
        let mut ticks: Vec<BacktestTick> = snapshots.into_iter().map(BacktestTick::new).collect();

        // Count trades before first snapshot (will be discarded)
        let first_snapshot_ts = ticks[0].timestamp();
        let discarded_count = trades
            .iter()
            .take_while(|t| t.timestamp < first_snapshot_ts)
            .count();

        if discarded_count > 0 {
            debug!(
                "Discarding {} trades before first snapshot (ts < {})",
                discarded_count, first_snapshot_ts
            );
        }

        // Assign trades to ticks
        // For each trade, find the tick it belongs to (last tick with ts <= trade.ts)
        let mut trade_iter = trades.into_iter().skip(discarded_count).peekable();

        for i in 0..ticks.len() {
            let current_ts = ticks[i].timestamp();
            let next_ts = ticks.get(i + 1).map(|t| t.timestamp()).unwrap_or(i64::MAX);

            // Collect all trades for this tick
            while let Some(trade) = trade_iter.peek() {
                if trade.timestamp >= current_ts && trade.timestamp < next_ts {
                    ticks[i].trades.push(trade_iter.next().unwrap());
                } else {
                    break;
                }
            }
        }

        // Any remaining trades go to the last tick
        let last_idx = ticks.len() - 1;
        for trade in trade_iter {
            ticks[last_idx].trades.push(trade);
        }

        // Assign spot price updates to ticks (similar to trades, but include early events)
        if let Some(mut spot_events) = spot_events {
            spot_events.sort_by_key(|e| e.timestamp);

            let mut spot_iter = spot_events.into_iter().peekable();

            for i in 0..ticks.len() {
                let current_ts = ticks[i].timestamp();
                let next_ts = ticks.get(i + 1).map(|t| t.timestamp()).unwrap_or(i64::MAX);

                // Collect all spot events for this tick
                // For the first tick, include ALL events before or at current_ts
                if i == 0 {
                    while let Some(spot) = spot_iter.peek() {
                        if spot.timestamp.timestamp_millis() < next_ts {
                            ticks[i].spot_prices.push(spot_iter.next().unwrap());
                        } else {
                            break;
                        }
                    }
                } else {
                    while let Some(spot) = spot_iter.peek() {
                        let spot_ts = spot.timestamp.timestamp_millis();
                        if spot_ts >= current_ts && spot_ts < next_ts {
                            ticks[i].spot_prices.push(spot_iter.next().unwrap());
                        } else {
                            break;
                        }
                    }
                }
            }

            // Any remaining spot events go to the last tick
            let last_idx = ticks.len() - 1;
            for spot in spot_iter {
                ticks[last_idx].spot_prices.push(spot);
            }
        }

        let total_spot_prices: usize = ticks.iter().map(|t| t.spot_prices.len()).sum();

        debug!(
            "Created timeline with {} ticks, total trades: {}, total spot prices: {}",
            ticks.len(),
            ticks.iter().map(|t| t.trades.len()).sum::<usize>(),
            total_spot_prices
        );

        // Log spot price distribution status
        info!(
            "Timeline created: {} total ticks, {} total trades, {} total spot prices across {} ticks with spot data",
            ticks.len(),
            ticks.iter().map(|t| t.trades.len()).sum::<usize>(),
            total_spot_prices,
            ticks.iter().filter(|t| !t.spot_prices.is_empty()).count()
        );

        Ok(Self {
            ticks,
            current_index: 0,
        })
    }

    /// Get the number of ticks in the timeline.
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    /// Check if the timeline is empty.
    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    /// Get the current tick index.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Check if there are more ticks.
    pub fn has_more(&self) -> bool {
        self.current_index < self.ticks.len()
    }

    /// Get the next tick without advancing the position.
    pub fn peek(&self) -> Option<&BacktestTick> {
        self.ticks.get(self.current_index)
    }

    /// Advance to the next tick and return it.
    pub fn next_tick(&mut self) -> Option<&BacktestTick> {
        if self.current_index < self.ticks.len() {
            let tick = &self.ticks[self.current_index];
            self.current_index += 1;
            Some(tick)
        } else {
            None
        }
    }

    /// Reset the timeline to the beginning.
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Get all ticks in the timeline.
    pub fn ticks(&self) -> &[BacktestTick] {
        &self.ticks
    }

    /// Get the total number of trades across all ticks.
    pub fn total_trades(&self) -> usize {
        self.ticks.iter().map(|t| t.trades.len()).sum()
    }

    /// Get the timestamp range of the timeline.
    pub fn time_range(&self) -> Option<(i64, i64)> {
        if self.ticks.is_empty() {
            None
        } else {
            Some((
                self.ticks.first().unwrap().timestamp(),
                self.ticks.last().unwrap().timestamp(),
            ))
        }
    }
}

impl Iterator for BacktestTimeline {
    type Item = BacktestTick;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.ticks.len() {
            let tick = self.ticks[self.current_index].clone();
            self.current_index += 1;
            Some(tick)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use popeyes_trading_types::{OrderSummary, OrderbookSource, TradeSide};

    use super::*;

    fn create_snapshot(timestamp: i64) -> OrderbookSnapshotEvent {
        OrderbookSnapshotEvent {
            asset_id: "test-asset".to_string(),
            market: "test-market".to_string(),
            bids: vec![OrderSummary {
                price: 0.50,
                size: 100.0,
            }],
            asks: vec![OrderSummary {
                price: 0.55,
                size: 100.0,
            }],
            hash: String::new(),
            timestamp,
            source: OrderbookSource::Polymarket,
            market_metadata: None,
            observed_at: chrono::Utc::now(),
        }
    }

    fn create_trade(timestamp: i64, price: f64) -> PolymarketTradeEvent {
        PolymarketTradeEvent {
            asset_id: "test-asset".to_string(),
            market: "test-market".to_string(),
            price,
            size: 10.0,
            side: TradeSide::Buy,
            timestamp,
            fee_rate_bps: 0,
            market_metadata: None,
            observed_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_timeline_basic() {
        let snapshots = vec![create_snapshot(1000), create_snapshot(2000)];
        let trades = vec![
            create_trade(1100, 0.51),
            create_trade(1200, 0.52),
            create_trade(2100, 0.53),
        ];

        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.ticks()[0].trades.len(), 2);
        assert_eq!(timeline.ticks()[1].trades.len(), 1);
    }

    #[test]
    fn test_timeline_trades_before_first_snapshot() {
        let snapshots = vec![create_snapshot(1000)];
        let trades = vec![
            create_trade(500, 0.50),  // Before first snapshot - should be discarded
            create_trade(1100, 0.51), // After snapshot - should be included
        ];

        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.ticks()[0].trades.len(), 1);
        assert_eq!(timeline.ticks()[0].trades[0].timestamp, 1100);
    }

    #[test]
    fn test_timeline_trades_after_last_snapshot() {
        let snapshots = vec![create_snapshot(1000)];
        let trades = vec![
            create_trade(1100, 0.51),
            create_trade(2000, 0.52), // After all snapshots - should go to last tick
            create_trade(3000, 0.53), // After all snapshots - should go to last tick
        ];

        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.ticks()[0].trades.len(), 3);
    }

    #[test]
    fn test_timeline_no_trades() {
        let snapshots = vec![create_snapshot(1000), create_snapshot(2000)];
        let trades = vec![];

        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.ticks()[0].trades.len(), 0);
        assert_eq!(timeline.ticks()[1].trades.len(), 0);
    }

    #[test]
    fn test_timeline_empty_snapshots() {
        let snapshots = vec![];
        let trades = vec![create_trade(1000, 0.50)];

        let result = BacktestTimeline::new(snapshots, trades, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_timeline_iteration() {
        let snapshots = vec![create_snapshot(1000), create_snapshot(2000)];
        let trades = vec![];

        let mut timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        let tick1 = timeline.next_tick();
        assert!(tick1.is_some());
        assert_eq!(tick1.unwrap().timestamp(), 1000);

        let tick2 = timeline.next_tick();
        assert!(tick2.is_some());
        assert_eq!(tick2.unwrap().timestamp(), 2000);

        let tick3 = timeline.next_tick();
        assert!(tick3.is_none());
    }

    #[test]
    fn test_timeline_reset() {
        let snapshots = vec![create_snapshot(1000), create_snapshot(2000)];
        let trades = vec![];

        let mut timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        timeline.next_tick();
        timeline.next_tick();
        assert!(!timeline.has_more());

        timeline.reset();
        assert!(timeline.has_more());
        assert_eq!(timeline.current_index(), 0);
    }

    #[test]
    fn test_timeline_time_range() {
        let snapshots = vec![
            create_snapshot(1000),
            create_snapshot(2000),
            create_snapshot(3000),
        ];

        let timeline = BacktestTimeline::new(snapshots, vec![], None).unwrap();

        let range = timeline.time_range();
        assert!(range.is_some());
        assert_eq!(range.unwrap(), (1000, 3000));
    }

    #[test]
    fn test_timeline_total_trades() {
        let snapshots = vec![create_snapshot(1000), create_snapshot(2000)];
        let trades = vec![
            create_trade(1100, 0.51),
            create_trade(1200, 0.52),
            create_trade(2100, 0.53),
        ];

        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        assert_eq!(timeline.total_trades(), 3);
    }

    #[test]
    fn test_timeline_unsorted_input() {
        // Test that the timeline handles unsorted input correctly
        let snapshots = vec![create_snapshot(2000), create_snapshot(1000)]; // Out of order
        let trades = vec![
            create_trade(2100, 0.53),
            create_trade(1100, 0.51), // Out of order
        ];

        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        // Should be sorted correctly
        assert_eq!(timeline.ticks()[0].timestamp(), 1000);
        assert_eq!(timeline.ticks()[1].timestamp(), 2000);
        assert_eq!(timeline.ticks()[0].trades[0].timestamp, 1100);
        assert_eq!(timeline.ticks()[1].trades[0].timestamp, 2100);
    }
}
