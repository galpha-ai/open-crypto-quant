use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    signature::Signature,
    transaction::VersionedTransaction,
};
use tokio_retry2::{Retry, RetryError, strategy::FixedInterval};
use tracing::{info, instrument};

#[async_trait]
pub trait TransactionSubmitter: Send + Sync {
    /// Submits an already-signed transaction
    ///
    /// This method accepts a signed VersionedTransaction and submits it to the network.
    /// In multi-user deployments, signing happens separately via SigningService.
    ///
    /// # Arguments
    /// * `transaction` - The already-signed transaction
    /// * `skip_simulation` - Whether to skip pre-flight simulation
    ///
    /// # Returns
    /// * `Ok(Signature)` - Transaction signature if successful
    /// * `Err(anyhow::Error)` - Submission failed
    async fn submit_transaction(
        &self,
        transaction: &VersionedTransaction,
        skip_simulation: bool,
    ) -> Result<Signature>;

    /// Confirms a transaction by polling for its status
    ///
    /// # Arguments
    /// * `signature` - The transaction signature to confirm
    ///
    /// # Returns
    /// * `Ok(())` - Transaction confirmed
    /// * `Err(anyhow::Error)` - Confirmation failed or timeout
    async fn confirm_transaction(&self, signature: Signature) -> Result<()>;
}

pub struct SolanaTransactionSubmitter {
    rpc_client: RpcClient,
}

impl SolanaTransactionSubmitter {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_client: RpcClient::new(rpc_url),
        }
    }
}

#[async_trait]
impl TransactionSubmitter for SolanaTransactionSubmitter {
    #[instrument(skip(self, transaction), level = "info")]
    async fn submit_transaction(
        &self,
        transaction: &VersionedTransaction,
        skip_simulation: bool,
    ) -> Result<Signature> {
        info!("sending transaction");

        if skip_simulation {
            let config = RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: Some(CommitmentLevel::Confirmed),
                ..RpcSendTransactionConfig::default()
            };
            // Send the transaction without retries
            self.rpc_client
                .send_transaction_with_config(transaction, config)
                .context("send transaction")
        } else {
            // Send the transaction without retries
            self.rpc_client
                .send_transaction(transaction)
                .context("send transaction")
        }
    }

    #[instrument(skip(self), level = "info")]
    async fn confirm_transaction(&self, signature: Signature) -> Result<()> {
        let commitment_config = CommitmentConfig::confirmed();
        // Retry for up to 60 seconds
        // After 60 seconds, the transaction is considered lost.
        let retry_strategy = FixedInterval::from_millis(500).take(120);

        Retry::spawn_notify(
            retry_strategy,
            || async {
                match self
                    .rpc_client
                    .confirm_transaction_with_commitment(&signature, commitment_config)
                {
                    Ok(response) => {
                        if response.value {
                            Ok(())
                        } else {
                            Err(RetryError::transient(anyhow!(
                                "Transaction(sig={}) not yet confirmed",
                                signature.to_string(),
                            )))
                        }
                    }
                    Err(e) => Err(RetryError::transient(anyhow!(
                        "Error confirming transaction(sig={}): {}",
                        e,
                        signature.to_string(),
                    ))),
                }
            },
            |error: &anyhow::Error, duration: Duration| {
                tracing::debug!(
                    error = ?error,
                    retry_in = ?duration,
                    "Waiting for transaction confirmation"
                );
            },
        )
        .await
    }
}

/// Mock transaction submitter for testing
#[cfg(test)]
pub struct MockTransactionSubmitter {}

#[cfg(test)]
impl MockTransactionSubmitter {
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
#[async_trait]
impl TransactionSubmitter for MockTransactionSubmitter {
    async fn submit_transaction(
        &self,
        transaction: &VersionedTransaction,
        _skip_simulation: bool,
    ) -> Result<Signature> {
        // Return the first signature from the transaction
        Ok(transaction.signatures[0])
    }

    async fn confirm_transaction(&self, _signature: Signature) -> Result<()> {
        // Mock confirmation always succeeds
        Ok(())
    }
}
