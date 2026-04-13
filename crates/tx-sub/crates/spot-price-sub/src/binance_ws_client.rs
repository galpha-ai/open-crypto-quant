///! Binance WebSocket client for real-time spot price updates

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

use crate::metrics::Metrics;

/// Binance aggTrade WebSocket message
#[derive(Debug, Deserialize)]
struct BinanceAggTrade {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "T")]
    trade_time: i64,
}

/// Binance WebSocket client for spot prices
pub struct BinanceWebSocketClient {
    /// WebSocket endpoint
    endpoint: String,
    /// Symbols to subscribe (e.g., ["BTCUSDT", "ETHUSDT"])
    symbols: Vec<String>,
    /// Broadcast channel for messages
    message_tx: broadcast::Sender<serde_json::Value>,
    /// Metrics
    metrics: Arc<Metrics>,
}

impl BinanceWebSocketClient {
    /// Create new Binance WebSocket client
    pub fn new(
        endpoint: String,
        symbols: Vec<String>,
        message_tx: broadcast::Sender<serde_json::Value>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            endpoint,
            symbols,
            message_tx,
            metrics,
        }
    }

    /// Run the WebSocket client
    pub async fn run(&self, cancellation_token: CancellationToken) -> Result<()> {
        tracing::info!(endpoint = %self.endpoint, "Binance WebSocket client starting");

        // Connect
        let (ws_stream, response) = connect_async(&self.endpoint)
            .await
            .context("Failed to connect to Binance WebSocket")?;

        tracing::info!(
            status = response.status().as_u16(),
            "Binance WebSocket connected"
        );

        self.metrics.ws_connection_status.set(1);

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to aggTrade streams
        let streams: Vec<String> = self.symbols
            .iter()
            .map(|s| format!("{}@aggTrade", s.to_lowercase()))
            .collect();

        let subscription = json!({
            "method": "SUBSCRIBE",
            "params": streams,
            "id": 1
        });

        tracing::info!(
            symbols = ?self.symbols,
            "Subscribing to Binance aggTrade streams"
        );

        write
            .send(Message::Text(subscription.to_string()))
            .await
            .context("Failed to send subscription")?;

        // Spawn keepalive task
        let cancellation_clone = cancellation_token.clone();
        tokio::spawn(async move {
            let mut keepalive = interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    _ = cancellation_clone.cancelled() => break,
                    _ = keepalive.tick() => {
                        tracing::debug!("Binance connection alive");
                    }
                }
            }
        });

        // Read messages
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    tracing::info!("Binance WebSocket client shutting down");
                    self.metrics.ws_connection_status.set(0);
                    break;
                }
                message = read.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            self.handle_message(&text).await;
                        }
                        Some(Ok(Message::Ping(_))) => {
                            // Binance sends pings, respond with pong
                            if let Err(e) = write.send(Message::Pong(vec![])).await {
                                tracing::error!(error = %e, "Failed to send pong");
                                self.metrics.ws_connection_status.set(0);
                                return Err(e.into());
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::warn!("Binance WebSocket closed by server");
                            self.metrics.ws_connection_status.set(0);
                            break;
                        }
                        Some(Err(e)) => {
                            tracing::error!(error = %e, "Binance WebSocket error");
                            self.metrics.ws_connection_status.set(0);
                            return Err(e.into());
                        }
                        None => {
                            tracing::warn!("Binance WebSocket stream ended");
                            self.metrics.ws_connection_status.set(0);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_message(&self, text: &str) {
        // Parse aggTrade message
        match serde_json::from_str::<BinanceAggTrade>(text) {
            Ok(trade) => {
                // Only process aggTrade events
                if trade.event_type != "aggTrade" {
                    return;
                }
                // Convert to spot price update format
                let price: f64 = match trade.price.parse() {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse price");
                        return;
                    }
                };

                // Convert to RTDS format for parser compatibility
                let spot_update = json!({
                    "topic": "crypto_prices",
                    "type": "update",
                    "timestamp": trade.trade_time,
                    "payload": {
                        "symbol": trade.symbol,
                        "timestamp": trade.trade_time,
                        "value": price
                    }
                });

                self.metrics.ws_messages_received.with_label_values(&["binance"]).inc();

                // Send to parser
                if let Err(e) = self.message_tx.send(spot_update) {
                    tracing::warn!(error = %e, "Failed to send message to parser");
                }
            }
            Err(_) => {
                // Might be subscription response or other message, ignore
                tracing::debug!(message = text, "Non-trade message received");
            }
        }
    }
}
