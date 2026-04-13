//! Event parser for Polymarket WebSocket messages
//!
//! This module parses JSON messages from the WebSocket and converts them
//! into structured event types.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use popeyes_trading_types::{PolymarketTradeEvent, TradeSide};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::market_metadata_cache::MarketMetadataCache;
use crate::metrics::Metrics;
use crate::types::{ParsedBookEvent, ParsedPriceChangeEvent};
use crate::ws_client::ObservedMessage;

use super::{book, price_change};

/// Event parser that processes WebSocket messages
pub struct EventParser {
    /// Receiver for raw WebSocket messages (with observed_at timestamp)
    message_rx: broadcast::Receiver<ObservedMessage>,
    /// Sender for parsed trade events
    trade_tx: broadcast::Sender<PolymarketTradeEvent>,
    /// Sender for parsed price change events
    price_change_tx: broadcast::Sender<ParsedPriceChangeEvent>,
    /// Sender for parsed book (orderbook snapshot) events
    book_tx: broadcast::Sender<ParsedBookEvent>,
    /// Metrics for tracking parsing
    metrics: Arc<Metrics>,
    /// Market metadata cache for enriching trade events
    market_metadata_cache: Arc<MarketMetadataCache>,
}

impl EventParser {
    /// Create a new event parser
    pub fn new(
        message_rx: broadcast::Receiver<ObservedMessage>,
        trade_tx: broadcast::Sender<PolymarketTradeEvent>,
        price_change_tx: broadcast::Sender<ParsedPriceChangeEvent>,
        book_tx: broadcast::Sender<ParsedBookEvent>,
        metrics: Arc<Metrics>,
        market_metadata_cache: Arc<MarketMetadataCache>,
    ) -> Self {
        Self {
            message_rx,
            trade_tx,
            price_change_tx,
            book_tx,
            metrics,
            market_metadata_cache,
        }
    }

    /// Run the parser (receive messages, parse, broadcast events)
    ///
    /// # Arguments
    /// * `cancellation_token` - Token to signal shutdown
    pub async fn run(mut self, cancellation_token: CancellationToken) -> Result<()> {
        tracing::info!("Event parser starting");

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    tracing::info!("Event parser shutting down");
                    break;
                }
                result = self.message_rx.recv() => {
                    match result {
                Ok(observed_msg) => {
                    let json_value = &observed_msg.json;
                    let observed_at = observed_msg.observed_at;

                    // Extract event type
                    let event_type = json_value
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    // Route to event-type-specific parser
                    match event_type {
                        "last_trade_price" => {
                            match self.parse_last_trade_price(json_value, observed_at).await {
                                Ok(trade_event) => {
                                    // Broadcast parsed event
                                    if let Err(e) = self.trade_tx.send(trade_event) {
                                        tracing::warn!(
                                            error = %e,
                                            "Failed to broadcast trade event"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        event_type = event_type,
                                        "Failed to parse event"
                                    );
                                    self.metrics
                                        .parsing_errors
                                        .with_label_values(&[event_type])
                                        .inc();
                                }
                            }
                        }
                        "price_change" => {
                            match price_change::parse_price_change(json_value, observed_at) {
                                Ok(price_change_event) => {
                                    // Broadcast parsed event
                                    if let Err(e) = self.price_change_tx.send(price_change_event) {
                                        tracing::warn!(
                                            error = %e,
                                            "Failed to broadcast price change event"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        event_type = event_type,
                                        "Failed to parse price_change event"
                                    );
                                    self.metrics
                                        .parsing_errors
                                        .with_label_values(&[event_type])
                                        .inc();
                                }
                            }
                        }
                        "book" => {
                            match book::parse_book(json_value, observed_at) {
                                Ok(book_event) => {
                                    // Broadcast parsed event
                                    if let Err(e) = self.book_tx.send(book_event) {
                                        tracing::warn!(
                                            error = %e,
                                            "Failed to broadcast book event"
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        event_type = event_type,
                                        "Failed to parse book event"
                                    );
                                    self.metrics
                                        .parsing_errors
                                        .with_label_values(&[event_type])
                                        .inc();
                                }
                            }
                        }
                        _ => {
                            tracing::debug!(
                                event_type = event_type,
                                "Skipping unsupported event type"
                            );
                        }
                    }
                }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped = skipped,
                                "Parser lagged behind WebSocket, some messages skipped"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("WebSocket channel closed, parser shutting down");
                            break;
                        }
                    }
                }
            }
        }

        tracing::info!("Event parser shut down successfully");
        Ok(())
    }

    /// Parse a last_trade_price event
    ///
    /// # Arguments
    /// * `data` - The JSON data from the WebSocket message
    /// * `observed_at` - When the message was received by the subscriber service
    async fn parse_last_trade_price(
        &self,
        data: &serde_json::Value,
        observed_at: DateTime<Utc>,
    ) -> Result<PolymarketTradeEvent> {
        // Extract required fields
        let asset_id = data
            .get("asset_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing or invalid asset_id"))?
            .to_string();

        let market = data
            .get("market")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing or invalid market"))?
            .to_string();

        // Parse price (0-1 probability range)
        let price = data
            .get("price")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing price"))?
            .parse::<f64>()
            .context("Failed to parse price")?;

        if !(0.0..=1.0).contains(&price) {
            return Err(anyhow!("Price out of range (0-1): {}", price));
        }

        // Parse size
        let size = data
            .get("size")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing size"))?
            .parse::<f64>()
            .context("Failed to parse size")?;

        // Parse side
        let side_str = data
            .get("side")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing or invalid side"))?;

        let side = match side_str {
            "BUY" => TradeSide::Buy,
            "SELL" => TradeSide::Sell,
            _ => return Err(anyhow!("Invalid side value: {}", side_str)),
        };

        // Parse timestamp (milliseconds) - can be string or number
        let timestamp = if let Some(ts_str) = data.get("timestamp").and_then(|v| v.as_str()) {
            ts_str
                .parse::<i64>()
                .context("Failed to parse timestamp string")?
        } else if let Some(ts_num) = data.get("timestamp").and_then(|v| v.as_i64()) {
            ts_num
        } else {
            return Err(anyhow!("Missing or invalid timestamp"));
        };

        // Parse fee rate (basis points) - can be string or number
        let fee_rate_bps = if let Some(fee_str) = data.get("fee_rate_bps").and_then(|v| v.as_str()) {
            fee_str
                .parse::<u32>()
                .context("Failed to parse fee_rate_bps string")?
        } else if let Some(fee_num) = data.get("fee_rate_bps").and_then(|v| v.as_u64()) {
            fee_num as u32
        } else {
            return Err(anyhow!("Missing or invalid fee_rate_bps"));
        };

        // Lookup market metadata from cache using asset_id
        // Cache is now keyed by asset_id (not condition_id) to enable outcome lookup
        let market_metadata = self.market_metadata_cache.get(&asset_id).await;

        if market_metadata.is_none() {
            tracing::debug!(
                asset_id = %asset_id,
                condition_id = %market,
                "Market metadata not found in cache for trade event"
            );
        }

        Ok(PolymarketTradeEvent {
            asset_id,
            market,
            price,
            size,
            side,
            timestamp,
            observed_at,
            fee_rate_bps,
            market_metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;
    use serde_json::json;

    fn create_test_metrics() -> Arc<Metrics> {
        let registry = Registry::new();
        Arc::new(Metrics::new(&registry).unwrap())
    }

    fn create_test_parser() -> EventParser {
        let (_msg_tx, msg_rx) = broadcast::channel(10);
        let (trade_tx, _trade_rx) = broadcast::channel(10);
        let (price_change_tx, _price_change_rx) = broadcast::channel(10);
        let (book_tx, _book_rx) = broadcast::channel(10);
        let metrics = create_test_metrics();
        let cache = Arc::new(MarketMetadataCache::new());

        EventParser::new(msg_rx, trade_tx, price_change_tx, book_tx, metrics, cache)
    }

    fn test_observed_at() -> DateTime<Utc> {
        Utc::now()
    }

    fn valid_trade_event() -> serde_json::Value {
        json!({
            "event_type": "last_trade_price",
            "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "price": "0.456",
            "size": "219.217767",
            "side": "BUY",
            "timestamp": 1699564800123i64,
            "fee_rate_bps": 10u64
        })
    }

    #[tokio::test]
    async fn test_parse_valid_buy_event() {
        let parser = create_test_parser();
        let data = valid_trade_event();
        let observed_at = test_observed_at();

        let result = parser.parse_last_trade_price(&data, observed_at).await;
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.asset_id, "109681959945973300464568698402968596289258214226684818748321941747028805721376");
        assert_eq!(event.market, "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999");
        assert_eq!(event.price, 0.456);
        assert_eq!(event.size, 219.217767);
        assert!(matches!(event.side, TradeSide::Buy));
        assert_eq!(event.timestamp, 1699564800123);
        assert_eq!(event.observed_at, observed_at);
        assert_eq!(event.fee_rate_bps, 10);
        assert_eq!(event.market_metadata, None);
    }

    #[tokio::test]
    async fn test_parse_valid_sell_event() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["side"] = json!("SELL");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_ok());

        let event = result.unwrap();
        assert!(matches!(event.side, TradeSide::Sell));
    }

    #[tokio::test]
    async fn test_parse_missing_asset_id() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("asset_id");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("asset_id"));
    }

    #[tokio::test]
    async fn test_parse_missing_market() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("market");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("market"));
    }

    #[tokio::test]
    async fn test_parse_missing_price() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("price");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("price"));
    }

    #[tokio::test]
    async fn test_parse_missing_size() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("size");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("size"));
    }

    #[tokio::test]
    async fn test_parse_missing_side() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("side");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("side"));
    }

    #[tokio::test]
    async fn test_parse_missing_timestamp() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("timestamp");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timestamp"));
    }

    #[tokio::test]
    async fn test_parse_missing_fee_rate_bps() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data.as_object_mut().unwrap().remove("fee_rate_bps");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("fee_rate_bps"));
    }

    #[tokio::test]
    async fn test_parse_invalid_price_format() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["price"] = json!("invalid");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse"));
    }

    #[tokio::test]
    async fn test_parse_price_out_of_range_high() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["price"] = json!("1.5");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[tokio::test]
    async fn test_parse_price_out_of_range_low() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["price"] = json!("-0.1");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[tokio::test]
    async fn test_parse_price_boundary_values() {
        let parser = create_test_parser();

        // Test 0.0
        let mut data = valid_trade_event();
        data["price"] = json!("0.0");
        assert!(parser.parse_last_trade_price(&data, test_observed_at()).await.is_ok());

        // Test 1.0
        data["price"] = json!("1.0");
        assert!(parser.parse_last_trade_price(&data, test_observed_at()).await.is_ok());
    }

    #[tokio::test]
    async fn test_parse_invalid_side_value() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["side"] = json!("INVALID");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid side"));
    }

    #[tokio::test]
    async fn test_parse_invalid_size_format() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["size"] = json!("not_a_number");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_wrong_type_timestamp() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["timestamp"] = json!("not_an_integer");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_wrong_type_fee_rate_bps() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["fee_rate_bps"] = json!("not_a_number");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_large_asset_id() {
        let parser = create_test_parser();
        let data = valid_trade_event();

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_ok());

        // Verify the 256-bit asset ID is preserved as string
        let event = result.unwrap();
        assert_eq!(event.asset_id.len(), 78); // 256-bit number as decimal string
    }

    #[tokio::test]
    async fn test_parse_zero_size() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["size"] = json!("0.0");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.size, 0.0);
    }

    #[tokio::test]
    async fn test_parse_very_small_price() {
        let parser = create_test_parser();
        let mut data = valid_trade_event();
        data["price"] = json!("0.001");

        let result = parser.parse_last_trade_price(&data, test_observed_at()).await;
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.price, 0.001);
    }
}
