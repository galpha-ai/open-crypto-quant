//! WebSocket client for Polymarket RTDS API (crypto_prices topic)

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

use crate::metrics::Metrics;

/// WebSocket client for RTDS crypto prices
pub struct RtdsWebSocketClient {
    /// WebSocket endpoint URL
    endpoint: String,
    /// Cryptocurrency symbols to subscribe to
    symbols: Vec<String>,
    /// Broadcast channel for sending messages to parser
    message_tx: broadcast::Sender<serde_json::Value>,
    /// Metrics for tracking connection and events
    metrics: Arc<Metrics>,
}

impl RtdsWebSocketClient {
    /// Create a new RTDS WebSocket client
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

    /// Run the WebSocket client (connect, subscribe, receive messages)
    pub async fn run(&self, cancellation_token: CancellationToken) -> Result<()> {
        tracing::info!(endpoint = %self.endpoint, "RTDS WebSocket client starting");

        // Connect to WebSocket with TLS
        let (ws_stream, response) = connect_async(&self.endpoint)
            .await
            .context("Failed to connect to RTDS WebSocket")?;

        tracing::info!(
            status = response.status().as_u16(),
            "RTDS WebSocket connected successfully"
        );

        // Set connection status to connected
        self.metrics.ws_connection_status.set(1);

        // Split the WebSocket stream
        let (mut write, mut read) = ws_stream.split();

        // Send subscription message
        let subscription = self.create_subscription_message();
        tracing::info!(
            symbols = ?self.symbols,
            subscription = %subscription,
            "Sending RTDS subscription message"
        );

        write
            .send(Message::Text(subscription))
            .await
            .context("Failed to send subscription message")?;

        tracing::info!(
            symbol_count = self.symbols.len(),
            "Subscribed to crypto_prices topic"
        );

        // Spawn ping task
        let mut write_clone = write;
        let metrics_clone = Arc::clone(&self.metrics);
        let cancellation_token_clone = cancellation_token.clone();
        let ping_task = tokio::spawn(async move {
            let mut ping_interval = interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = cancellation_token_clone.cancelled() => {
                        tracing::debug!("Ping task cancelled");
                        break;
                    }
                    _ = ping_interval.tick() => {
                        // Official RTDS client sends raw "ping" string, not JSON
                        if let Err(e) = write_clone.send(Message::Text("ping".to_string())).await {
                            tracing::error!(error = %e, "Failed to send ping, connection lost");
                            metrics_clone.ws_connection_status.set(0);
                            std::process::exit(1);
                        }

                        tracing::debug!("Ping sent");
                    }
                }
            }
        });

        // Message receiving loop
        let message_tx = self.message_tx.clone();
        let metrics = Arc::clone(&self.metrics);

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    tracing::info!("RTDS WebSocket client shutting down gracefully");
                    ping_task.abort();
                    metrics.ws_connection_status.set(0);
                    break;
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Handle PONG text messages
                            if text.trim() == "PONG" {
                                tracing::debug!("Received PONG");
                                continue;
                            }

                            // Skip empty messages (likely keepalives)
                            if text.is_empty() {
                                tracing::debug!("Received empty message, skipping");
                                continue;
                            }

                            // Parse JSON message
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(json_value) => {
                                    // Extract topic for metrics
                                    if let Some(topic) = json_value.get("topic").and_then(|v| v.as_str()) {
                                        metrics.ws_messages_received
                                            .with_label_values(&[topic])
                                            .inc();
                                        tracing::debug!(topic = topic, "Received message");
                                    }

                                    // Broadcast to parser
                                    if let Err(e) = message_tx.send(json_value) {
                                        tracing::warn!(error = %e, "Failed to broadcast message to parser");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        text_len = text.len(),
                                        raw_text = %text,
                                        "Failed to parse JSON message"
                                    );
                                }
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            tracing::warn!(?frame, "WebSocket connection closed by server");
                            metrics.ws_connection_status.set(0);
                            ping_task.abort();
                            std::process::exit(1);
                        }
                        Some(Ok(Message::Pong(_))) => {
                            tracing::debug!("Received pong frame");
                        }
                        Some(Ok(msg)) => {
                            tracing::debug!(msg_type = ?msg, "Received non-text message");
                        }
                        Some(Err(e)) => {
                            tracing::error!(error = %e, "WebSocket error");
                            metrics.ws_connection_status.set(0);
                            ping_task.abort();
                            std::process::exit(1);
                        }
                        None => {
                            tracing::warn!("WebSocket stream ended");
                            metrics.ws_connection_status.set(0);
                            ping_task.abort();
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        tracing::info!("RTDS WebSocket client shut down successfully");
        Ok(())
    }

    /// Create subscription message for crypto_prices topic (Binance source)
    ///
    /// Each symbol requires a separate subscription entry with uppercase symbol.
    /// Example format:
    /// ```json
    /// {
    ///   "action": "subscribe",
    ///   "subscriptions": [
    ///     {"topic": "crypto_prices", "type": "update", "filters": "{\"symbol\":\"BTCUSDT\"}"},
    ///     {"topic": "crypto_prices", "type": "update", "filters": "{\"symbol\":\"ETHUSDT\"}"}
    ///   ]
    /// }
    /// ```
    fn create_subscription_message(&self) -> String {
        // Each symbol needs its own subscription entry with uppercase symbol
        let subscriptions: Vec<serde_json::Value> = self
            .symbols
            .iter()
            .map(|s| {
                let filters = format!(r#"{{"symbol":"{}"}}"#, s.to_uppercase());
                json!({
                    "topic": "crypto_prices",
                    "type": "update",
                    "filters": filters
                })
            })
            .collect();

        let subscription = json!({
            "action": "subscribe",
            "subscriptions": subscriptions
        });

        subscription.to_string()
    }
}
