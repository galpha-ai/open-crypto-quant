use std::env;

use anyhow::{Context, Result};

#[cfg(test)]
mod integration_tests {
    use tokio;
    use tracing::{info, warn};
    use yellowstone_grpc_proto::prelude::SubscribeUpdateTransaction;

    use super::*;
    use crate::redis_tx_retriever::RedisTxRetriever;

    // This test is ignored by default and must be run explicitly
    // Run with: cargo test -- --ignored redis_tx_retriever_test::integration_tests::test_get_transaction
    #[tokio::test]
    #[ignore]
    async fn test_get_transaction() -> Result<()> {
        // Initialize test logging
        if env::var("RUST_LOG").is_err() {
            unsafe {
                env::set_var("RUST_LOG", "debug");
            }
        }
        let _ = tracing_subscriber::fmt::try_init();

        // Read Redis connection information from environment variables
        let redis_url = env::var("REDIS_URL").unwrap_or("redis://localhost:6379".to_string());

        let key_prefix = env::var("REDIS_KEY_PREFIX").unwrap_or("/transactions/prod".to_string());

        let tx_signature = env::var("TX_SIGNATURE")
            .context("TX_SIGNATURE environment variable is required for this test")?;

        info!(
            redis_url = %redis_url,
            key_prefix = %key_prefix,
            tx_signature = %tx_signature,
            "Starting Redis transaction retriever integration test"
        );

        // Create the retriever
        let retriever = RedisTxRetriever::new(&redis_url, key_prefix).await?;

        // Try to retrieve the transaction
        let tx_result = retriever.get_transaction(&tx_signature).await?;

        // Check the result
        match tx_result {
            Some(tx) => {
                // Verify the transaction data
                verify_transaction_data(&tx, &tx_signature)?;
                info!("Successfully retrieved and verified transaction");
                Ok(())
            }
            None => {
                warn!(
                    signature = %tx_signature,
                    "Transaction not found in Redis. Test can't verify retrieval functionality."
                );
                // This is not necessarily a failure - the transaction might not be in Redis
                // but we should warn about it
                Ok(())
            }
        }
    }

    // Helper function to verify retrieved transaction data
    fn verify_transaction_data(
        tx: &SubscribeUpdateTransaction,
        expected_signature: &str,
    ) -> Result<()> {
        // Verify the slot is greater than 0
        assert!(tx.slot > 0, "Transaction slot should be greater than 0");

        // Verify the transaction contains information
        let tx_info = tx
            .transaction
            .as_ref()
            .context("Transaction should have transaction info")?;

        // If we have a signature in the transaction, verify it matches what we expect
        if !tx_info.signature.is_empty() {
            let signature = bs58::encode(&tx_info.signature).into_string();
            assert_eq!(
                signature, expected_signature,
                "Retrieved transaction signature does not match expected signature"
            );
        }

        info!(slot = tx.slot, "Transaction verification successful");

        Ok(())
    }
}
