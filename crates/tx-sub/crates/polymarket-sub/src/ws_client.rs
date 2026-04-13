//! WebSocket client for Polymarket CLOB API
//!
//! This module handles the WebSocket connection, subscription, and message receiving.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

use crate::metrics::Metrics;

/// A WebSocket message with its observed timestamp
/// The observed_at timestamp is captured immediately when the message is received,
/// before any parsing or processing, for accurate cross-feed event ordering.
#[derive(Debug, Clone)]
pub struct ObservedMessage {
    /// The parsed JSON message
    pub json: serde_json::Value,
    /// When the message was received by the subscriber service
    pub observed_at: DateTime<Utc>,
}

/// WebSocket client for Polymarket subscriptions
pub struct PolymarketWebSocketClient {
    /// WebSocket endpoint URL
    endpoint: String,
    /// Asset IDs to subscribe to
    asset_ids: Vec<String>,
    /// Broadcast channel for sending messages to parser (with observed timestamp)
    message_tx: broadcast::Sender<ObservedMessage>,
    /// Metrics for tracking connection and events
    metrics: Arc<Metrics>,
}

impl PolymarketWebSocketClient {
    /// Create a new WebSocket client
    pub fn new(
        endpoint: String,
        asset_ids: Vec<String>,
        message_tx: broadcast::Sender<ObservedMessage>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            endpoint,
            asset_ids,
            message_tx,
            metrics,
        }
    }

    /// Run the WebSocket client (connect, subscribe, receive messages)
    ///
    /// # Arguments
    /// * `cancellation_token` - Token to signal shutdown
    pub async fn run(&self, cancellation_token: CancellationToken) -> Result<()> {
        tracing::info!(endpoint = %self.endpoint, "WebSocket client starting");

        // Connect to WebSocket with TLS
        let (ws_stream, response) = connect_async(&self.endpoint)
            .await
            .context("Failed to connect to WebSocket")?;

        tracing::info!(
            status = response.status().as_u16(),
            "WebSocket connected successfully"
        );

        // Set connection status to connected
        self.metrics.ws_connection_status.set(1.0);

        // Split the WebSocket stream
        let (mut write, mut read) = ws_stream.split();

        // Send subscription message
        let subscription = self.create_subscription_message();
        tracing::info!(
            asset_count = self.asset_ids.len(),
            "Sending subscription message"
        );

        write
            .send(Message::Text(subscription))
            .await
            .context("Failed to send subscription message")?;

        tracing::info!(
            asset_count = self.asset_ids.len(),
            "Subscribed to market channel"
        );

        // Spawn ping task
        let mut write_clone = write;
        let metrics_clone = Arc::clone(&self.metrics);
        let cancellation_token_clone = cancellation_token.clone();
        let ping_task = tokio::spawn(async move {
            let mut ping_interval = interval(Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = cancellation_token_clone.cancelled() => {
                        tracing::debug!("Ping task cancelled");
                        break;
                    }
                    _ = ping_interval.tick() => {
                        // Polymarket expects plain text "PING", not JSON
                        if let Err(e) = write_clone.send(Message::Text("PING".to_string())).await {
                            tracing::error!(error = %e, "Failed to send ping, connection lost");
                            metrics_clone.ws_connection_status.set(0.0);
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
                    tracing::info!("WebSocket client shutting down gracefully");
                    ping_task.abort();
                    metrics.ws_connection_status.set(0.0);
                    break;
                }
                msg = read.next() => {
                    // Capture observed_at timestamp IMMEDIATELY on message receipt
                    // This is the earliest possible point for accurate cross-feed ordering
                    let observed_at = chrono::Utc::now();

                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // Handle PONG text messages (server sends "PONG" as text, not WebSocket Pong frame)
                            if text.trim() == "PONG" {
                                tracing::debug!("Received PONG");
                                continue;
                            }

                            // Parse JSON message
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(json_value) => {
                                    // Calculate and log message latency
                                    let latency_ms = Self::calculate_latency(&json_value);

                                    // Extract event type for metrics
                                    if let Some(event_type) = json_value.get("event_type").and_then(|v| v.as_str()) {
                                        metrics.ws_events_received
                                            .with_label_values(&[event_type])
                                            .inc();

                                        if let Some(latency) = latency_ms {
                                            tracing::debug!(
                                                event_type = event_type,
                                                latency_ms = latency,
                                                "Received event"
                                            );
                                        } else {
                                            tracing::debug!(event_type = event_type, "Received event (no timestamp)");
                                        }
                                    }

                                    // Broadcast to parser with observed_at timestamp
                                    let observed_msg = ObservedMessage {
                                        json: json_value,
                                        observed_at,
                                    };
                                    if let Err(e) = message_tx.send(observed_msg) {
                                        tracing::warn!(error = %e, "Failed to broadcast message to parser");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, text = %text, "Failed to parse JSON message");
                                }
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            tracing::warn!(?frame, "WebSocket connection closed by server");
                            metrics.ws_connection_status.set(0.0);
                            ping_task.abort();
                            std::process::exit(1);
                        }
                        Some(Ok(Message::Pong(_))) => {
                            tracing::debug!("Received pong");
                        }
                        Some(Ok(msg)) => {
                            tracing::debug!(msg_type = ?msg, "Received non-text message");
                        }
                        Some(Err(e)) => {
                            tracing::error!(error = %e, "WebSocket error");
                            metrics.ws_connection_status.set(0.0);
                            ping_task.abort();
                            std::process::exit(1);
                        }
                        None => {
                            tracing::warn!("WebSocket stream ended");
                            metrics.ws_connection_status.set(0.0);
                            ping_task.abort();
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        tracing::info!("WebSocket client shut down successfully");
        Ok(())
    }

    /// Create subscription message for market channel
    fn create_subscription_message(&self) -> String {
        let subscription = json!({
            "assets_ids": self.asset_ids,
            "type": "market"
        });

        subscription.to_string()
    }

    /// Calculate message latency from timestamp field
    /// Returns latency in milliseconds, or None if timestamp not found
    fn calculate_latency(json_value: &serde_json::Value) -> Option<i64> {
        // Try to extract timestamp (can be string or number, in milliseconds)
        let timestamp_ms = if let Some(ts_str) = json_value.get("timestamp").and_then(|v| v.as_str()) {
            ts_str.parse::<i64>().ok()
        } else {
            json_value.get("timestamp").and_then(|v| v.as_i64())
        };

        timestamp_ms.map(|ts| {
            let now_ms = chrono::Utc::now().timestamp_millis();
            now_ms - ts
        })
    }
}
