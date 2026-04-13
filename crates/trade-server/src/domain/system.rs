use chrono::{DateTime, Utc};
use popeyes_trading_types::{MarketDataEvent, TokenEvent};

use crate::{
    domain::TimerEvent,
    execution::{ExecutionEvent, LimitOrderEvent, RedemptionEvent},
    position::PositionEvent,
    signal::TradableSignal,
};

#[derive(Debug)]
pub enum SystemEvent {
    Token(TokenEvent),
    /// Market data events (orderbook updates, snapshots, spot prices)
    MarketData(MarketDataEvent),
    Timer(TimerEvent),
    Signal(Box<dyn TradableSignal>),
    Position(PositionEvent),
    Execution(ExecutionEvent),
    /// Limit order lifecycle events (placed, filled, cancelled, etc.)
    LimitOrder(LimitOrderEvent),
    /// Pair redemption events for binary markets
    Redemption(RedemptionEvent),
}

impl SystemEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            SystemEvent::Token(_) => "Token",
            SystemEvent::MarketData(_) => "MarketData",
            SystemEvent::Timer(_) => "Timer",
            SystemEvent::Signal(_) => "Signal",
            SystemEvent::Position(_) => "Position",
            SystemEvent::Execution(_) => "Execution",
            SystemEvent::LimitOrder(_) => "LimitOrder",
            SystemEvent::Redemption(_) => "Redemption",
        }
    }

    /// Get the event timestamp in milliseconds, if available.
    /// Returns None for events that don't have a timestamp.
    pub fn timestamp_ms(&self) -> Option<i64> {
        match self {
            SystemEvent::MarketData(market_data) => match market_data {
                MarketDataEvent::OrderbookSnapshot(snapshot) => Some(snapshot.timestamp),
                MarketDataEvent::OrderbookUpdate(update) => Some(update.timestamp),
                MarketDataEvent::PolymarketTrade(trade) => Some(trade.timestamp),
                MarketDataEvent::SpotPrice(spot) => Some(spot.timestamp.timestamp_millis()),
            },
            SystemEvent::Token(token_event) => match token_event {
                TokenEvent::Buy(trade) | TokenEvent::Sell(trade) | TokenEvent::Swap(trade) => {
                    Some(trade.timestamp().timestamp_millis())
                }
                TokenEvent::Create(creation) => Some(creation.timestamp.timestamp_millis()),
            },
            SystemEvent::LimitOrder(limit_order) => match limit_order {
                LimitOrderEvent::OrderPlaced { timestamp, .. }
                | LimitOrderEvent::OrderPartiallyFilled { timestamp, .. }
                | LimitOrderEvent::OrderCancelled { timestamp, .. }
                | LimitOrderEvent::OrderExpired { timestamp, .. } => {
                    Some(timestamp.timestamp_millis())
                }
                LimitOrderEvent::OrderRejected { .. } => None,
            },
            // Timer, Signal, Position, Execution, Redemption don't have meaningful source timestamps
            _ => None,
        }
    }

    /// Calculate the latency (in milliseconds) from event timestamp to now.
    /// Returns None if the event doesn't have a timestamp.
    pub fn latency_ms(&self) -> Option<i64> {
        self.timestamp_ms().map(|ts| {
            let now_ms = Utc::now().timestamp_millis();
            now_ms - ts
        })
    }

    /// Extract the timestamp from the event as a DateTime, if available.
    ///
    /// Returns the logical timestamp for market data events (orderbook snapshots,
    /// trades, etc.) which is useful for signal generators to track simulation time
    /// in backtest mode.
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.timestamp_ms()
            .and_then(DateTime::from_timestamp_millis)
    }
}
