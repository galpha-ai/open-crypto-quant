use anyhow::{Context, Result};
use bs58;
use redis::{AsyncCommands, Client, aio::ConnectionManager};
use tokio::sync::broadcast;
use tracing::{debug, error, warn};
use yellowstone_grpc_proto::prost::Message;

use crate::{config::PersistTxRedisConfig, grpc::TransactionData};

const DEFAULT_TX_TTL_SECONDS: u64 = 600; // 10 minutes

pub struct RedisTxConsumer {
    conn_manager: ConnectionManager,
    config: PersistTxRedisConfig,
    tx_rx: broadcast::Receiver<TransactionData>,
}

impl RedisTxConsumer {
    pub async fn new(
        redis_url: &str,
        config: PersistTxRedisConfig,
        tx_rx: broadcast::Receiver<TransactionData>,
    ) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client)
            .await
            .context("Failed to create Redis connection manager for persister")?;
        Ok(Self {
            conn_manager,
            config,
            tx_rx,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            warn!("Redis transaction persistence is disabled via config");
            return Ok(());
        }

        debug!(
            environment = %self.config.environment,
            ttl = self.config.ttl_seconds.unwrap_or(DEFAULT_TX_TTL_SECONDS),
            "Starting Redis transaction persister"
        );

        loop {
            match self.tx_rx.recv().await {
                Ok(tx_data) => {
                    if let TransactionData::GrpcTransaction(tx) = tx_data {
                        let signature = tx
                            .transaction
                            .as_ref()
                            .map(|info| bs58::encode(&info.signature).into_string())
                            .unwrap_or_else(|| "unknown_signature".to_string());

                        let key =
                            format!("/transactions/{}/{}", self.config.environment, signature);

                        let data = tx.encode_to_vec();
                        let ttl = self.config.ttl_seconds.unwrap_or(DEFAULT_TX_TTL_SECONDS);

                        let mut conn = self.conn_manager.clone();
                        match conn.set_ex::<_, _, ()>(&key, data, ttl).await {
                            Ok(_) => {
                                debug!(key = %key, ttl = ttl, "Persisted transaction to Redis")
                            }
                            Err(e) => {
                                error!(error = ?e, key = %key, "Failed to persist transaction")
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        count = n,
                        "Redis persister lagged behind transaction stream"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    warn!("Transaction broadcast channel closed");
                    break;
                }
            }
        }
        Ok(())
    }
}
