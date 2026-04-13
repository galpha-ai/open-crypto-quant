//! RTDS message parser for crypto_prices events

use anyhow::{anyhow, Result};
use chrono::DateTime;
use popeyes_trading_types::SpotPriceUpdate;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::metrics::Metrics;

/// Parser for RTDS crypto_prices messages
pub struct SpotPriceParser {
    /// WebSocket message receiver
    ws_rx: broadcast::Receiver<serde_json::Value>,
    /// Spot price update broadcaster
    spot_price_tx: broadcast::Sender<SpotPriceUpdate>,
    /// Metrics
    metrics: Arc<Metrics>,
}

impl SpotPriceParser {
    /// Create a new spot price parser
    pub fn new(
        ws_rx: broadcast::Receiver<serde_json::Value>,
        spot_price_tx: broadcast::Sender<SpotPriceUpdate>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            ws_rx,
            spot_price_tx,
            metrics,
        }
    }

    /// Start the parser (consumes self)
    pub async fn start(mut self) -> Result<()> {
        tracing::info!("Spot price parser starting");

        loop {
            match self.ws_rx.recv().await {
                Ok(msg) => {
                    if let Err(e) = self.parse_message(msg) {
                        tracing::warn!(error = %e, "Parse error");
                        self.metrics
                            .parsing_errors
                            .with_label_values(&["parse_error"])
                            .inc();
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(lagged = n, "Parser lagged behind WebSocket messages");
                    self.metrics
                        .parsing_errors
                        .with_label_values(&["lagged"])
                        .inc();
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("WebSocket message channel closed, parser shutting down");
                    break;
                }
            }
        }

        tracing::info!("Spot price parser shut down");
        Ok(())
    }

    /// Parse a single RTDS message
    ///
    /// Expected format:
    /// ```json
    /// {
    ///   "topic": "crypto_prices",
    ///   "type": "update",
    ///   "timestamp": 1732617600000,
    ///   "payload": {
    ///     "symbol": "BTCUSDT",
    ///     "timestamp": 1732617595000,
    ///     "value": 98750.25
    ///   }
    /// }
    /// ```
    fn parse_message(&self, msg: serde_json::Value) -> Result<()> {
        // Log the raw message for debugging
        tracing::debug!(msg = %msg, "Received message");

        // Check topic
        let topic = msg
            .get("topic")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing topic field"))?;

        if topic != "crypto_prices" {
            // Ignore non-crypto_prices messages
            return Ok(());
        }

        // Check message type - only process "update" messages
        let msg_type = msg
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing type field"))?;

        if msg_type != "update" {
            // Ignore non-update messages (e.g., "subscribe" response with historical data)
            tracing::debug!(msg_type = msg_type, "Ignoring non-update message");
            return Ok(());
        }

        // Extract payload
        let payload = msg
            .get("payload")
            .ok_or_else(|| anyhow!("Missing payload field"))?;

        // Parse symbol
        let symbol = payload
            .get("symbol")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing or invalid symbol"))?
            .to_string();

        // Parse price (from "value" field)
        let price = payload
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!("Missing or invalid price value"))?;

        // Validate price is positive and finite
        if !price.is_finite() || price <= 0.0 {
            return Err(anyhow!("Invalid price: {}", price));
        }

        // Parse timestamp (milliseconds)
        let timestamp_ms = payload
            .get("timestamp")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow!("Missing or invalid timestamp"))?;

        let timestamp = DateTime::from_timestamp_millis(timestamp_ms)
            .ok_or_else(|| anyhow!("Invalid timestamp: {}", timestamp_ms))?;

        // Create SpotPriceUpdate
        let update = SpotPriceUpdate {
            symbol,
            price,
            timestamp,
            source: "binance".to_string(),
        };

        tracing::debug!(
            symbol = %update.symbol,
            price = update.price,
            timestamp_ms = timestamp_ms,
            "Parsed spot price update"
        );

        // Broadcast to Redis publisher
        if let Err(e) = self.spot_price_tx.send(update) {
            tracing::warn!(error = %e, "Failed to send spot price update");
            self.metrics
                .parsing_errors
                .with_label_values(&["send_failed"])
                .inc();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_valid_message() {
        let msg = json!({
            "topic": "crypto_prices",
            "type": "update",
            "timestamp": 1732617600000i64,
            "payload": {
                "symbol": "BTCUSDT",
                "timestamp": 1732617595000i64,
                "value": 98750.25
            }
        });

        let (spot_price_tx, _spot_price_rx) = broadcast::channel(100);
        let (_ws_tx, ws_rx) = broadcast::channel(100);
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(Metrics::new(&registry).unwrap());

        let parser = SpotPriceParser::new(ws_rx, spot_price_tx, metrics);

        assert!(parser.parse_message(msg).is_ok());
    }

    #[test]
    fn test_parse_missing_topic() {
        let msg = json!({
            "type": "update",
            "payload": {
                "symbol": "BTCUSDT",
                "value": 98750.25
            }
        });

        let (spot_price_tx, _spot_price_rx) = broadcast::channel(100);
        let (_ws_tx, ws_rx) = broadcast::channel(100);
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(Metrics::new(&registry).unwrap());

        let parser = SpotPriceParser::new(ws_rx, spot_price_tx, metrics);

        assert!(parser.parse_message(msg).is_err());
    }

    #[test]
    fn test_parse_invalid_price() {
        let msg = json!({
            "topic": "crypto_prices",
            "type": "update",
            "timestamp": 1732617600000i64,
            "payload": {
                "symbol": "BTCUSDT",
                "timestamp": 1732617595000i64,
                "value": -100.0
            }
        });

        let (spot_price_tx, _spot_price_rx) = broadcast::channel(100);
        let (_ws_tx, ws_rx) = broadcast::channel(100);
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(Metrics::new(&registry).unwrap());

        let parser = SpotPriceParser::new(ws_rx, spot_price_tx, metrics);

        assert!(parser.parse_message(msg).is_err());
    }

    #[test]
    fn test_parse_non_crypto_prices_topic() {
        let msg = json!({
            "topic": "other_topic",
            "type": "update",
            "payload": {}
        });

        let (spot_price_tx, _spot_price_rx) = broadcast::channel(100);
        let (_ws_tx, ws_rx) = broadcast::channel(100);
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(Metrics::new(&registry).unwrap());

        let parser = SpotPriceParser::new(ws_rx, spot_price_tx, metrics);

        // Should return Ok but not parse anything
        assert!(parser.parse_message(msg).is_ok());
    }
}
