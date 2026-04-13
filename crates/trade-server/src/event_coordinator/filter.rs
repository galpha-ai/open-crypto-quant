//! Event filtering for selective event collection.
//!
//! The `EventFilter` allows configuring which events are captured during
//! paper trading or backtest runs. This is useful for:
//! - Reducing noise by excluding high-volume market data events
//! - Focusing on trading-relevant events (signals, orders, fills)
//! - Managing storage/bandwidth for Redis-based event persistence

use serde::{Deserialize, Serialize};

use crate::domain::SystemEvent;

/// Configurable filter for event collection.
///
/// By default, excludes high-volume market data events and keeps
/// trading-relevant events like signals, orders, and position changes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventFilter {
    /// Exclude orderbook snapshot events (large, ~30KB each)
    #[serde(default = "default_true")]
    pub exclude_orderbook_snapshots: bool,

    /// Exclude orderbook update events
    #[serde(default = "default_true")]
    pub exclude_orderbook_updates: bool,

    /// Exclude Polymarket trade events (high volume)
    #[serde(default = "default_true")]
    pub exclude_polymarket_trades: bool,

    /// Exclude timer events
    #[serde(default = "default_true")]
    pub exclude_timer_events: bool,

    /// Exclude token events (Solana token trades)
    #[serde(default = "default_true")]
    pub exclude_token_events: bool,

    /// Exclude spot price events
    #[serde(default = "default_true")]
    pub exclude_spot_price_events: bool,
}

fn default_true() -> bool {
    true
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::paper_trading_default()
    }
}

impl From<crate::config::EventFilterConfig> for EventFilter {
    fn from(config: crate::config::EventFilterConfig) -> Self {
        Self {
            exclude_orderbook_snapshots: config.exclude_orderbook_snapshots,
            exclude_orderbook_updates: config.exclude_orderbook_updates,
            exclude_polymarket_trades: config.exclude_polymarket_trades,
            exclude_timer_events: config.exclude_timer_events,
            exclude_token_events: config.exclude_token_events,
            exclude_spot_price_events: config.exclude_spot_price_events,
        }
    }
}

impl EventFilter {
    /// Create a filter that allows all events through (no filtering).
    pub fn allow_all() -> Self {
        Self {
            exclude_orderbook_snapshots: false,
            exclude_orderbook_updates: false,
            exclude_polymarket_trades: false,
            exclude_timer_events: false,
            exclude_token_events: false,
            exclude_spot_price_events: false,
        }
    }

    /// Default filter for paper trading - excludes market data, keeps trading events.
    ///
    /// Keeps:
    /// - `Signal.*` - generated trading signals
    /// - `LimitOrder.*` - order lifecycle (placed, filled, cancelled, expired, rejected)
    /// - `Position.*` - position lifecycle (created, updated, closed)
    /// - `Execution.*` - order fills and rejections
    /// - `Redemption.*` - pair redemption events
    pub fn paper_trading_default() -> Self {
        Self {
            exclude_orderbook_snapshots: true,
            exclude_orderbook_updates: true,
            exclude_polymarket_trades: true,
            exclude_timer_events: true,
            exclude_token_events: true,
            exclude_spot_price_events: true,
        }
    }

    /// Check if an event should be recorded based on filter rules.
    pub fn should_record(&self, event: &SystemEvent) -> bool {
        match event {
            SystemEvent::Token(_) => !self.exclude_token_events,
            SystemEvent::MarketData(market_data) => {
                use popeyes_trading_types::MarketDataEvent;
                match market_data {
                    MarketDataEvent::OrderbookSnapshot(_) => !self.exclude_orderbook_snapshots,
                    MarketDataEvent::OrderbookUpdate(_) => !self.exclude_orderbook_updates,
                    MarketDataEvent::SpotPrice(_) => !self.exclude_spot_price_events,
                    MarketDataEvent::PolymarketTrade(_) => !self.exclude_polymarket_trades,
                }
            }
            SystemEvent::Timer(_) => !self.exclude_timer_events,
            // Always record trading events
            SystemEvent::Signal(_) => true,
            SystemEvent::Position(_) => true,
            SystemEvent::Execution(_) => true,
            SystemEvent::LimitOrder(_) => true,
            SystemEvent::Redemption(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TimerEvent;
    use crate::execution::{LimitOrderEvent, OrderSide};
    use chrono::Utc;
    use popeyes_trading_types::{
        MarketDataEvent, OrderSummary, OrderbookSnapshotEvent, OrderbookSource,
    };

    #[test]
    fn test_default_filter_excludes_market_data() {
        let filter = EventFilter::default();

        // Timer events should be excluded
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        assert!(!filter.should_record(&timer_event));

        // Orderbook snapshots should be excluded
        let snapshot = OrderbookSnapshotEvent {
            asset_id: "test".to_string(),
            market: "test".to_string(),
            bids: vec![OrderSummary {
                price: 0.5,
                size: 100.0,
            }],
            asks: vec![OrderSummary {
                price: 0.6,
                size: 100.0,
            }],
            hash: String::new(),
            timestamp: 1000,
            source: OrderbookSource::Polymarket,
            market_metadata: None,
            observed_at: Utc::now(),
        };
        assert!(!filter.should_record(&SystemEvent::MarketData(
            MarketDataEvent::OrderbookSnapshot(snapshot)
        )));
    }

    #[test]
    fn test_default_filter_keeps_trading_events() {
        let filter = EventFilter::default();

        // LimitOrder events should be kept
        let order_event = SystemEvent::LimitOrder(LimitOrderEvent::OrderPlaced {
            order_id: "test".to_string(),
            mint: "test".to_string(),
            market: Some("test".to_string()),
            price: 0.5,
            size: 100.0,
            side: OrderSide::Buy,
            timestamp: Utc::now(),
            signal_id: None,
            context: None,
        });
        assert!(filter.should_record(&order_event));
    }

    #[test]
    fn test_allow_all_filter() {
        let filter = EventFilter::allow_all();

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        assert!(filter.should_record(&timer_event));

        let snapshot = OrderbookSnapshotEvent {
            asset_id: "test".to_string(),
            market: "test".to_string(),
            bids: vec![],
            asks: vec![],
            hash: String::new(),
            timestamp: 1000,
            source: OrderbookSource::Polymarket,
            market_metadata: None,
            observed_at: Utc::now(),
        };
        assert!(filter.should_record(&SystemEvent::MarketData(
            MarketDataEvent::OrderbookSnapshot(snapshot)
        )));
    }
}
