use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;
use tracing::info;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::{
    geyser::{
        CommitmentLevel, SubscribeRequest, SubscribeRequestFilterTransactions,
        SubscribeRequestPing, subscribe_update::UpdateOneof,
    },
    prelude::SubscribeRequestFilterBlocksMeta,
};

use crate::{grpc::types::TransactionData, metrics::Metrics};

/// Configuration for the gRPC subscription manager
#[derive(Debug)]
pub struct GrpcSubscriptionConfig {
    /// Endpoint URL for the gRPC service
    pub endpoint: String,
    /// Optional authentication token
    pub x_token: Option<String>,
    /// Program IDs to monitor for transactions
    pub program_ids: Vec<Pubkey>,
}

impl GrpcSubscriptionConfig {
    pub fn from_app_config(app_config: &crate::config::AppConfig) -> Self {
        let mut program_ids = app_config
            .grpc
            .program_ids
            .iter()
            .map(|id_str| Pubkey::from_str(id_str))
            .collect::<Result<Vec<_>, _>>()
            .context("Invalid program ID in config")
            .unwrap();

        // Add Bonk program ID if configured
        if let Some(bonk_config) = &app_config.bonk {
            if let Ok(bonk_id) = Pubkey::from_str(&bonk_config.program_id) {
                program_ids.push(bonk_id);
            }
        }

        Self {
            endpoint: app_config.grpc.endpoint.clone(),
            x_token: app_config.grpc.x_token.clone(),
            program_ids,
        }
    }
}

/// Manages real-time data subscriptions using Solana's Geyser gRPC service
pub struct GrpcDataSubscriptionManager {
    transaction_tx: broadcast::Sender<TransactionData>,
    config: GrpcSubscriptionConfig,
    metrics: Arc<Metrics>,
}

impl GrpcDataSubscriptionManager {
    /// Creates a new GrpcDataSubscriptionManager instance
    pub fn new(
        config: GrpcSubscriptionConfig,
        transaction_tx: broadcast::Sender<TransactionData>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            transaction_tx,
            config,
            metrics,
        }
    }

    /// Starts the gRPC subscription
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting GrpcDataSubscriptionManager");
        info!(
            endpoint = %self.config.endpoint,
            has_auth_token = self.config.x_token.is_some(),
            auth_token = ?self.config.x_token.as_ref().map(|t| {
                if t.len() > 8 {
                    format!("{}...{}", &t[..4], &t[t.len()-4..])
                } else {
                    "***".to_string()
                }
            }),
            program_ids = ?self.config.program_ids,
            program_count = self.config.program_ids.len(),
            "Establishing gRPC connection"
        );

        let mut client = GeyserGrpcClient::build_from_shared(self.config.endpoint.clone())?
            .x_token(self.config.x_token.clone())?
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .connect()
            .await?;

        info!(
            endpoint = %self.config.endpoint,
            "Successfully connected to gRPC endpoint"
        );

        let mut transactions = HashMap::new();
        transactions.insert(
            "client".to_string(),
            SubscribeRequestFilterTransactions {
                account_include: self
                    .config
                    .program_ids
                    .iter()
                    .map(|pubkey| pubkey.to_string())
                    .collect(),
                vote: Some(false),
                failed: Some(false),
                ..Default::default()
            },
        );

        let mut blocks_meta = HashMap::new();
        blocks_meta.insert("client".to_string(), SubscribeRequestFilterBlocksMeta {});

        info!(
            account_filters = ?self.config.program_ids,
            vote_excluded = true,
            failed_excluded = true,
            commitment_level = "Processed",
            "Sending subscription request"
        );

        let request = SubscribeRequest {
            transactions,
            blocks_meta,
            commitment: Some(CommitmentLevel::Processed as i32),
            ..Default::default()
        };

        let (mut subscribe_tx, mut stream) = client.subscribe_with_request(Some(request)).await?;

        info!("gRPC subscription stream established successfully");

        let mut last_ping = tokio::time::Instant::now();
        let ping_interval = tokio::time::Duration::from_secs(30); // Ping every 30 seconds

        while let Some(message) = stream.next().await {
            // Send ping periodically to keep connection alive
            if last_ping.elapsed() >= ping_interval {
                if let Err(e) = subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await
                {
                    tracing::error!(error = ?e, "Failed to send ping");
                }
                last_ping = tokio::time::Instant::now();
            }

            match message {
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        "Error receiving gRPC message",
                    );
                    continue;
                }
                Ok(message) => match message.update_oneof {
                    Some(UpdateOneof::Transaction(tx)) => {
                        self.metrics.transactions_processed.inc();
                        // Send to broadcast channel. Error means no active receivers.
                        if self
                            .transaction_tx
                            .send(TransactionData::GrpcTransaction(tx))
                            .is_err()
                        {
                            self.metrics
                                .send_failures
                                .with_label_values(&["broadcast_error"])
                                .inc();
                        }
                    }
                    Some(UpdateOneof::BlockMeta(meta)) => match meta.block_time {
                        Some(timestamp) => {
                            // Convert Unix timestamp to milliseconds and get current time
                            let block_time_ms = timestamp.timestamp * 1000; // Convert seconds to milliseconds
                            let current_time_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                                as i64;

                            // Calculate latency in milliseconds
                            let latency_ms = current_time_ms - block_time_ms;

                            tracing::info!(latency_ms, "Calculated block time latency");

                            // Only record positive latencies (to handle clock skew)
                            if latency_ms > 0 {
                                self.metrics.block_time_latency.observe(latency_ms as f64);

                                if latency_ms > 5000 {
                                    tracing::warn!(
                                        block_timestamp = timestamp.timestamp,
                                        latency_ms = latency_ms,
                                        "High block processing latency detected"
                                    );
                                }
                            }
                        }
                        None => {
                            tracing::debug!("Received BlockMeta without timestamp");
                        }
                    },
                    _ => continue,
                },
            }
        }

        Ok(())
    }
}
