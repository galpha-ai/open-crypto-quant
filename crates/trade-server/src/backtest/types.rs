//! Types for backtest operations.

use chrono::{DateTime, Utc};
use popeyes_trading_types::{OrderbookSnapshotEvent, PolymarketTradeEvent, SpotPriceUpdate};

/// A single tick in the backtest timeline.
///
/// Groups an orderbook snapshot with all trades that occurred
/// between this snapshot and the next one.
#[derive(Debug, Clone)]
pub struct BacktestTick {
    /// Orderbook snapshot at this tick
    pub snapshot: OrderbookSnapshotEvent,

    /// Trades that occurred between this snapshot and the next
    pub trades: Vec<PolymarketTradeEvent>,

    /// Spot price updates (e.g., BTC/USDC) that occurred between this snapshot and the next
    pub spot_prices: Vec<SpotPriceUpdate>,
}

impl BacktestTick {
    /// Create a new backtest tick with a snapshot and no trades.
    pub fn new(snapshot: OrderbookSnapshotEvent) -> Self {
        Self {
            snapshot,
            trades: Vec::new(),
            spot_prices: Vec::new(),
        }
    }

    /// Get the timestamp of this tick (from the snapshot).
    pub fn timestamp(&self) -> i64 {
        self.snapshot.timestamp
    }

    /// Get the timestamp as a DateTime.
    pub fn datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.snapshot.timestamp)
    }
}
