//! Event collector trait and implementations.
//!
//! The `EventCollector` trait provides a unified interface for capturing events
//! during both live trading and backtests. The `InMemoryEventCollector` implementation
//! stores events in memory for later export to JSONL format.

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use popeyes_trading_types::MarketDataEvent;
use serde_json::json;
use tracing::debug;

use crate::{
    domain::SystemEvent,
    execution::{ExecutionEvent, LimitOrderEvent, RedemptionEvent},
    position::PositionEvent,
};

/// Collected event for JSONL output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectedEvent {
    /// Session identifier for correlating events from different trading sessions.
    /// When using a shared queue name, this field distinguishes events from different sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Monotonically increasing sequence number within this collector instance.
    /// Used as secondary sort key for deterministic event ordering.
    pub sequence_id: u64,

    /// Event timestamp in ISO 8601 format
    pub timestamp: String,

    /// Logical simulation time (ISO 8601), populated during backtests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_time: Option<String>,

    /// Event type identifier
    pub event_type: String,

    /// Full event payload
    pub data: serde_json::Value,
}

/// Trait for collecting events during trading or backtests.
///
/// Implementations of this trait can record events for later analysis,
/// export them to files, or stream them to external systems.
pub trait EventCollector: Send + Sync {
    /// Record an event for later analysis.
    fn record(&self, event: &SystemEvent);

    /// Record an event with associated logical time (for backtests).
    ///
    /// The logical time represents the simulation time from historical data,
    /// as opposed to the wall-clock time when the backtest runs.
    fn record_with_logical_time(&self, event: &SystemEvent, logical_time: Option<DateTime<Utc>>) {
        let _ = logical_time; // Default: ignore logical time
        self.record(event);
    }

    /// Get all collected events.
    fn events(&self) -> Vec<CollectedEvent>;

    /// Export collected events to a file in JSONL format.
    fn export(&self, path: &Path) -> Result<()>;

    /// Get the number of collected events.
    fn len(&self) -> usize;

    /// Check if no events have been collected.
    fn is_empty(&self) -> bool;
}

/// In-memory event collector for recording events during trading or backtests.
///
/// Events are stored in memory and can be exported to a JSONL file
/// after the session completes.
pub struct InMemoryEventCollector {
    /// Collected events
    events: Mutex<Vec<CollectedEvent>>,

    /// Next sequence number to assign
    next_sequence: AtomicU64,
}

impl Default for InMemoryEventCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventCollector {
    /// Create a new in-memory event collector.
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            next_sequence: AtomicU64::new(0),
        }
    }
}

/// Streaming JSONL file event collector.
///
/// Writes each event to a JSONL file as it is recorded to avoid unbounded
/// memory growth when capturing large backtest sessions.
pub struct JsonlFileEventCollector {
    output_path: PathBuf,
    writer: Mutex<BufWriter<File>>,
    next_sequence: AtomicU64,
    event_count: AtomicU64,
}

impl JsonlFileEventCollector {
    pub fn new(output_path: PathBuf) -> Result<Self> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::create(&output_path)?;
        Ok(Self {
            output_path,
            writer: Mutex::new(BufWriter::new(file)),
            next_sequence: AtomicU64::new(0),
            event_count: AtomicU64::new(0),
        })
    }

    fn write_event(&self, event: &SystemEvent, logical_time: Option<DateTime<Utc>>) {
        let sequence_id = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let collected = convert_system_event_to_collected(event, logical_time, None, sequence_id);

        if let Ok(line) = serde_json::to_string(&collected) {
            if let Ok(mut writer) = self.writer.lock() {
                if writer.write_all(line.as_bytes()).is_ok() && writer.write_all(b"\n").is_ok() {
                    self.event_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

/// Convert a SystemEvent to a CollectedEvent.
///
/// This is a shared implementation used by both `InMemoryEventCollector`
/// and `RedisEventCollector` to ensure consistent event serialization.
///
/// # Arguments
/// * `event` - The system event to convert
/// * `logical_time` - Optional logical simulation time (for backtests)
/// * `session_id` - Optional session identifier for event correlation
/// * `sequence_id` - Monotonically increasing sequence number for ordering
pub fn convert_system_event_to_collected(
    event: &SystemEvent,
    logical_time: Option<DateTime<Utc>>,
    session_id: Option<&str>,
    sequence_id: u64,
) -> CollectedEvent {
    use popeyes_trading_types::TokenEvent;
    use serde_json::json;

    let (timestamp, event_type, data) = match event {
        SystemEvent::Token(token_event) => {
            let ts = match token_event {
                TokenEvent::Create(e) => e.timestamp.to_rfc3339(),
                TokenEvent::Buy(e) | TokenEvent::Sell(e) | TokenEvent::Swap(e) => e
                    .base()
                    .map(|b| b.timestamp.to_rfc3339())
                    .unwrap_or_else(|| Utc::now().to_rfc3339()),
            };
            let data = match token_event {
                TokenEvent::Create(e) => json!({
                    "type": "Create",
                    "mint": e.mint,
                    "symbol": e.symbol,
                    "name": e.name,
                    "timestamp": e.timestamp.to_rfc3339()
                }),
                TokenEvent::Buy(e) | TokenEvent::Sell(e) | TokenEvent::Swap(e) => {
                    let event_type = match token_event {
                        TokenEvent::Buy(_) => "Buy",
                        TokenEvent::Sell(_) => "Sell",
                        TokenEvent::Swap(_) => "Swap",
                        _ => "Unknown",
                    };
                    let timestamp = e
                        .base()
                        .map(|b| b.timestamp.to_rfc3339())
                        .unwrap_or_else(|| Utc::now().to_rfc3339());
                    json!({
                        "type": event_type,
                        "mint": e.base_token_mint(),
                        "timestamp": timestamp,
                        "price": e.spot_price()
                    })
                }
            };
            (ts, "Token".to_string(), data)
        }
        SystemEvent::MarketData(market_data) => {
            let (ts, subtype, data) = match market_data {
                MarketDataEvent::OrderbookSnapshot(snap) => {
                    let ts = DateTime::from_timestamp_millis(snap.timestamp)
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default();
                    let data = json!({
                        "asset_id": snap.asset_id,
                        "market": snap.market,
                        "timestamp": snap.timestamp,
                        "bids": snap.bids,
                        "asks": snap.asks
                    });
                    (ts, "OrderbookSnapshot", data)
                }
                MarketDataEvent::OrderbookUpdate(update) => {
                    let ts = DateTime::from_timestamp_millis(update.timestamp)
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default();
                    let data = json!({
                        "asset_id": update.asset_id,
                        "market": update.market,
                        "timestamp": update.timestamp,
                        "best_bid": update.best_bid,
                        "best_ask": update.best_ask
                    });
                    (ts, "OrderbookUpdate", data)
                }
                MarketDataEvent::SpotPrice(price) => {
                    let ts = price.timestamp.to_rfc3339();
                    let data = json!({
                        "symbol": price.symbol,
                        "price": price.price,
                        "timestamp": ts
                    });
                    (ts, "SpotPrice", data)
                }
                MarketDataEvent::PolymarketTrade(trade) => {
                    let ts = DateTime::from_timestamp_millis(trade.timestamp)
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default();
                    let data = json!({
                        "asset_id": trade.asset_id,
                        "market": trade.market,
                        "price": trade.price,
                        "size": trade.size,
                        "side": format!("{:?}", trade.side),
                        "timestamp": trade.timestamp
                    });
                    (ts, "PolymarketTrade", data)
                }
            };
            (ts, format!("MarketData.{}", subtype), data)
        }
        SystemEvent::Timer(timer_event) => {
            let ts = timer_event.timestamp.to_rfc3339();
            let data = json!({
                "timestamp": ts
            });
            (ts, "Timer".to_string(), data)
        }
        SystemEvent::Signal(signal) => {
            let ts = signal
                .get_timestamp()
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            let data = signal.to_json().unwrap_or(json!({}));
            (ts, format!("Signal.{}", signal.signal_type()), data)
        }
        SystemEvent::Position(pos_event) => {
            let ts = Utc::now().to_rfc3339();
            let data = match pos_event {
                PositionEvent::PositionCreated {
                    position,
                    available_quote,
                } => json!({
                    "type": "PositionCreated",
                    "mint": position.mint,
                    "entry_price": position.entry_price,
                    "amount": position.amount,
                    "available_quote": available_quote
                }),
                PositionEvent::PositionUpdated {
                    position,
                    source,
                    available_quote,
                } => json!({
                    "type": "PositionUpdated",
                    "mint": position.mint,
                    "amount": position.amount,
                    "source": format!("{:?}", source),
                    "available_quote": available_quote
                }),
                PositionEvent::PositionClosed {
                    position,
                    realized_pnl_sol,
                    pnl_pct,
                    exit_reason,
                    available_quote,
                    ..
                } => json!({
                    "type": "PositionClosed",
                    "mint": position.mint,
                    "realized_pnl_sol": realized_pnl_sol,
                    "pnl_pct": pnl_pct,
                    "exit_reason": format!("{:?}", exit_reason),
                    "available_quote": available_quote
                }),
            };
            (ts, "Position".to_string(), data)
        }
        SystemEvent::Execution(exec_event) => match exec_event {
            ExecutionEvent::OrderFilled {
                mint,
                token_amount_change,
                price,
                timestamp,
                slippage,
                clear_position,
                ..
            } => {
                let ts = timestamp.to_rfc3339();
                let data = json!({
                    "type": "OrderFilled",
                    "mint": mint,
                    "token_amount_change": token_amount_change,
                    "price": price,
                    "slippage": slippage,
                    "clear_position": clear_position
                });
                (ts, "Execution".to_string(), data)
            }
            ExecutionEvent::OrderRejected { mint, reason, .. } => {
                let ts = Utc::now().to_rfc3339();
                let data = json!({
                    "type": "OrderRejected",
                    "mint": mint,
                    "reason": reason
                });
                (ts, "Execution".to_string(), data)
            }
        },
        SystemEvent::LimitOrder(limit_event) => {
            let (ts, subtype, data) = match limit_event {
                LimitOrderEvent::OrderPlaced {
                    order_id,
                    mint,
                    market,
                    price,
                    size,
                    side,
                    timestamp,
                    signal_id,
                    context,
                } => {
                    let ts = timestamp.to_rfc3339();
                    let data = json!({
                        "order_id": order_id,
                        "mint": mint,
                        "market": market,
                        "price": price,
                        "size": size,
                        "side": format!("{:?}", side),
                        "signal_id": signal_id,
                        "context": context
                    });
                    (ts, "OrderPlaced", data)
                }
                LimitOrderEvent::OrderPartiallyFilled {
                    order_id,
                    mint,
                    side,
                    filled_size,
                    remaining_size,
                    fill_price,
                    timestamp,
                    signal_id,
                    exit_mode,
                } => {
                    let ts = timestamp.to_rfc3339();
                    let data = json!({
                        "order_id": order_id,
                        "mint": mint,
                        "side": format!("{:?}", side),
                        "filled_size": filled_size,
                        "remaining_size": remaining_size,
                        "fill_price": fill_price,
                        "signal_id": signal_id,
                        "exit_mode": exit_mode.map(|e| format!("{:?}", e))
                    });
                    (ts, "OrderPartiallyFilled", data)
                }
                LimitOrderEvent::OrderCancelled {
                    order_id,
                    reason,
                    timestamp,
                } => {
                    let ts = timestamp.to_rfc3339();
                    let data = json!({
                        "order_id": order_id,
                        "reason": reason
                    });
                    (ts, "OrderCancelled", data)
                }
                LimitOrderEvent::OrderExpired {
                    order_id,
                    timestamp,
                } => {
                    let ts = timestamp.to_rfc3339();
                    let data = json!({
                        "order_id": order_id
                    });
                    (ts, "OrderExpired", data)
                }
                LimitOrderEvent::OrderRejected { reason } => {
                    let ts = Utc::now().to_rfc3339();
                    let data = json!({
                        "reason": reason
                    });
                    (ts, "OrderRejected", data)
                }
            };
            (ts, format!("LimitOrder.{}", subtype), data)
        }
        SystemEvent::Redemption(redemption_event) => {
            let (ts, subtype, data) = match redemption_event {
                RedemptionEvent::RedemptionCompleted {
                    market,
                    up_asset_id,
                    down_asset_id,
                    quantity,
                    quote_received,
                    timestamp,
                } => {
                    let ts = timestamp.to_rfc3339();
                    let data = json!({
                        "market": market,
                        "up_asset_id": up_asset_id,
                        "down_asset_id": down_asset_id,
                        "quantity": quantity,
                        "quote_received": quote_received
                    });
                    (ts, "RedemptionCompleted", data)
                }
                RedemptionEvent::RedemptionFailed {
                    market,
                    reason,
                    timestamp,
                } => {
                    let ts = timestamp.to_rfc3339();
                    let data = json!({
                        "market": market,
                        "reason": reason
                    });
                    (ts, "RedemptionFailed", data)
                }
            };
            (ts, format!("Redemption.{}", subtype), data)
        }
    };

    CollectedEvent {
        session_id: session_id.map(|s| s.to_string()),
        sequence_id,
        timestamp,
        logical_time: logical_time.map(|t| t.to_rfc3339()),
        event_type,
        data,
    }
}

impl EventCollector for InMemoryEventCollector {
    fn record(&self, event: &SystemEvent) {
        let sequence_id = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let collected = convert_system_event_to_collected(event, None, None, sequence_id);
        let mut events = self.events.lock().unwrap();
        events.push(collected);
    }

    fn record_with_logical_time(&self, event: &SystemEvent, logical_time: Option<DateTime<Utc>>) {
        let sequence_id = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let collected = convert_system_event_to_collected(event, logical_time, None, sequence_id);
        let mut events = self.events.lock().unwrap();
        events.push(collected);
    }

    fn events(&self) -> Vec<CollectedEvent> {
        self.events.lock().unwrap().clone()
    }

    fn export(&self, path: &Path) -> Result<()> {
        let events = self.events.lock().unwrap();

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        for event in events.iter() {
            let json = serde_json::to_string(event)?;
            writeln!(writer, "{}", json)?;
        }

        writer.flush()?;

        debug!(
            path = %path.display(),
            event_count = events.len(),
            "Wrote events to JSONL file"
        );

        Ok(())
    }

    fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }
}

impl EventCollector for JsonlFileEventCollector {
    fn record(&self, event: &SystemEvent) {
        self.write_event(event, None);
    }

    fn record_with_logical_time(&self, event: &SystemEvent, logical_time: Option<DateTime<Utc>>) {
        self.write_event(event, logical_time);
    }

    fn events(&self) -> Vec<CollectedEvent> {
        Vec::new()
    }

    fn export(&self, path: &Path) -> Result<()> {
        if path != self.output_path {
            return Err(anyhow::anyhow!(
                "JsonlFileEventCollector export path mismatch: opened={}, requested={}",
                self.output_path.display(),
                path.display()
            ));
        }
        if let Ok(mut writer) = self.writer.lock() {
            writer.flush()?;
        }
        Ok(())
    }

    fn len(&self) -> usize {
        self.event_count.load(Ordering::Relaxed) as usize
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use popeyes_trading_types::{OrderSummary, OrderbookSnapshotEvent, OrderbookSource};
    use std::io::Read;
    use tempfile::NamedTempFile;

    use crate::{domain::TimerEvent, execution::OrderSide};

    use super::*;

    #[test]
    fn test_collector_record_and_retrieve() {
        let collector = InMemoryEventCollector::new();
        assert!(collector.is_empty());

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);

        assert_eq!(collector.len(), 1);
        assert!(!collector.is_empty());

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "Timer");
    }

    #[test]
    fn test_collector_export() {
        let collector = InMemoryEventCollector::new();

        // Add some events
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);

        let snapshot = OrderbookSnapshotEvent {
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
            timestamp: 1000,
            observed_at: Utc::now(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
        };
        collector.record(&SystemEvent::MarketData(
            MarketDataEvent::OrderbookSnapshot(snapshot),
        ));

        // Write to temp file
        let temp_file = NamedTempFile::new().unwrap();
        collector.export(temp_file.path()).unwrap();

        // Read back and verify
        let mut file = File::open(temp_file.path()).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        // Verify each line is valid JSON
        for line in lines {
            let parsed: CollectedEvent = serde_json::from_str(line).unwrap();
            assert!(!parsed.event_type.is_empty());
        }
    }

    #[test]
    fn test_collector_limit_order_event() {
        let collector = InMemoryEventCollector::new();

        let event = SystemEvent::LimitOrder(LimitOrderEvent::OrderPlaced {
            order_id: "test-order".to_string(),
            mint: "test-mint".to_string(),
            market: Some("test-market".to_string()),
            price: 0.55,
            size: 100.0,
            side: OrderSide::Buy,
            timestamp: Utc::now(),
            signal_id: None,
            context: None,
        });
        collector.record(&event);

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "LimitOrder.OrderPlaced");
        assert_eq!(events[0].data["order_id"], "test-order");
    }

    #[test]
    fn test_collector_execution_event() {
        let collector = InMemoryEventCollector::new();

        let event = SystemEvent::Execution(ExecutionEvent::OrderFilled {
            mint: "test-mint".to_string(),
            token_amount_change: 100.0,
            quote_amount_change: Some(-50.0),
            price: Some(0.50),
            timestamp: Utc::now(),
            slippage: Some(0.01),
            clear_position: false,
            force_position_clear: false,
            execution_latency_in_slots: None,
            signal_id: None,
            confirmed_slot: None,
            confirmed_signature: None,
            exit_mode: None,
        });
        collector.record(&event);

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "Execution");
        assert_eq!(events[0].data["type"], "OrderFilled");
    }

    #[test]
    fn test_collector_with_logical_time() {
        let collector = InMemoryEventCollector::new();

        let logical_time = DateTime::parse_from_rfc3339("2025-11-24T14:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record_with_logical_time(&timer_event, Some(logical_time));

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "Timer");
        assert!(events[0].logical_time.is_some());
        assert!(
            events[0]
                .logical_time
                .as_ref()
                .unwrap()
                .contains("2025-11-24")
        );
    }

    #[test]
    fn test_collector_without_logical_time() {
        let collector = InMemoryEventCollector::new();

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "Timer");
        assert!(events[0].logical_time.is_none());
    }

    #[test]
    fn test_logical_time_serialization_skips_none() {
        let collector = InMemoryEventCollector::new();

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);

        let events = collector.events();
        let json = serde_json::to_string(&events[0]).unwrap();

        // logical_time should not be present when None
        assert!(!json.contains("logical_time"));
    }

    #[test]
    fn test_logical_time_serialization_includes_value() {
        let collector = InMemoryEventCollector::new();

        let logical_time = DateTime::parse_from_rfc3339("2025-11-24T14:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record_with_logical_time(&timer_event, Some(logical_time));

        let events = collector.events();
        let json = serde_json::to_string(&events[0]).unwrap();

        // logical_time should be present when Some
        assert!(json.contains("logical_time"));
        assert!(json.contains("2025-11-24"));
    }

    #[test]
    fn test_collector_redemption_event() {
        let collector = InMemoryEventCollector::new();

        let event = SystemEvent::Redemption(RedemptionEvent::RedemptionCompleted {
            market: "test-market".to_string(),
            up_asset_id: "up-asset".to_string(),
            down_asset_id: "down-asset".to_string(),
            quantity: 50.0,
            quote_received: 50.0,
            timestamp: Utc::now(),
        });
        collector.record(&event);

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "Redemption.RedemptionCompleted");
        assert_eq!(events[0].data["market"], "test-market");
        assert_eq!(events[0].data["quantity"], 50.0);
    }

    #[test]
    fn test_collector_redemption_failed_event() {
        let collector = InMemoryEventCollector::new();

        let event = SystemEvent::Redemption(RedemptionEvent::RedemptionFailed {
            market: "test-market".to_string(),
            reason: "insufficient balance".to_string(),
            timestamp: Utc::now(),
        });
        collector.record(&event);

        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "Redemption.RedemptionFailed");
        assert_eq!(events[0].data["market"], "test-market");
        assert_eq!(events[0].data["reason"], "insufficient balance");
    }

    #[test]
    fn test_sequence_id_increments() {
        let collector = InMemoryEventCollector::new();

        // Record multiple events
        for _ in 0..5 {
            let timer_event = SystemEvent::Timer(TimerEvent {
                timestamp: Utc::now(),
            });
            collector.record(&timer_event);
        }

        let events = collector.events();
        assert_eq!(events.len(), 5);

        // Verify sequence IDs are monotonically increasing from 0
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.sequence_id, i as u64, "sequence_id should be {}", i);
        }
    }

    #[test]
    fn test_sequence_id_resets_for_new_collector() {
        // First collector
        let collector1 = InMemoryEventCollector::new();
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector1.record(&timer_event);
        collector1.record(&timer_event);

        let events1 = collector1.events();
        assert_eq!(events1[0].sequence_id, 0);
        assert_eq!(events1[1].sequence_id, 1);

        // Second collector should start from 0 again
        let collector2 = InMemoryEventCollector::new();
        collector2.record(&timer_event);

        let events2 = collector2.events();
        assert_eq!(
            events2[0].sequence_id, 0,
            "new collector should start at sequence 0"
        );
    }

    #[test]
    fn test_sequence_id_in_jsonl_export() {
        let collector = InMemoryEventCollector::new();

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record(&timer_event);
        collector.record(&timer_event);

        // Write to temp file
        let temp_file = NamedTempFile::new().unwrap();
        collector.export(temp_file.path()).unwrap();

        // Read back and verify sequence_id is present
        let mut file = File::open(temp_file.path()).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);

        for (i, line) in lines.iter().enumerate() {
            let parsed: CollectedEvent = serde_json::from_str(line).unwrap();
            assert_eq!(parsed.sequence_id, i as u64);
        }
    }

    #[test]
    fn test_sequence_id_with_logical_time() {
        let collector = InMemoryEventCollector::new();

        let logical_time = DateTime::parse_from_rfc3339("2025-11-24T14:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        collector.record_with_logical_time(&timer_event, Some(logical_time));
        collector.record_with_logical_time(&timer_event, Some(logical_time));
        collector.record(&timer_event); // Mix of record_with_logical_time and record

        let events = collector.events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence_id, 0);
        assert_eq!(events[1].sequence_id, 1);
        assert_eq!(events[2].sequence_id, 2);
    }
}
