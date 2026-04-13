use anyhow::{Context, Result};
use redis::{Client, aio::ConnectionManager};
use tracing::{debug, error};
use yellowstone_grpc_proto::{prelude::SubscribeUpdateTransaction, prost::Message};

/// Client for retrieving transaction details from Redis by transaction signature
pub struct RedisTxRetriever {
    /// Redis connection manager
    conn_manager: ConnectionManager,
    /// Redis key prefix for stored transactions
    key_prefix: String,
}

impl RedisTxRetriever {
    /// Creates a new RedisTxRetriever instance
    pub async fn new(redis_url: &str, key_prefix: String) -> Result<Self> {
        let redis_client = Client::open(redis_url)
            .context("Failed to create Redis client for transaction retriever")?;

        let conn_manager = ConnectionManager::new(redis_client)
            .await
            .context("Failed to create Redis connection manager for transaction retriever")?;

        Ok(Self {
            conn_manager,
            key_prefix,
        })
    }

    /// Retrieves a transaction by its signature
    pub async fn get_transaction(
        &self,
        signature: &str,
    ) -> Result<Option<SubscribeUpdateTransaction>> {
        debug!(signature = %signature, "Retrieving transaction from Redis");

        let key = format!("{}/{}", self.key_prefix, signature);
        let mut conn = self.conn_manager.clone();

        // Retrieve the protobuf-encoded transaction directly
        let binary_data: Option<Vec<u8>> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .context("Failed to retrieve transaction from Redis")?;

        // If key doesn't exist, return None
        match binary_data {
            None => {
                debug!(signature = %signature, "Transaction not found in Redis");
                Ok(None)
            }
            Some(data) => {
                // Decode the transaction using Prost
                match SubscribeUpdateTransaction::decode(&data[..]) {
                    Ok(grpc_tx) => {
                        debug!(signature = %signature, "Successfully retrieved transaction from Redis");
                        Ok(Some(grpc_tx))
                    }
                    Err(e) => {
                        error!(
                            signature = %signature,
                            error = %e,
                            "Failed to decode transaction data from Redis"
                        );
                        Err(e.into())
                    }
                }
            }
        }
    }
}
