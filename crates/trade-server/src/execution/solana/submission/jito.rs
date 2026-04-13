use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jito_sdk_rust::JitoJsonRpcSDK;
use solana_sdk::{signature::Signature, transaction::VersionedTransaction};
use tokio::time::{sleep, timeout};
use tracing::{debug, info, instrument};

use super::metrics::JitoMetrics;
use super::submitter::TransactionSubmitter;
use crate::execution::solana::confirmation::GrpcConfirmationMonitor;

struct BundleStatus {
    err: Option<serde_json::Value>,
    transactions: Option<Vec<String>>,
}

pub struct JitoTransactionSubmitter {
    jito_sdk: JitoJsonRpcSDK,
    grpc_confirmer: GrpcConfirmationMonitor, // Use the central monitor
    metrics: Option<JitoMetrics>,
    confirmation_timeout: Duration,
}

impl JitoTransactionSubmitter {
    pub fn new(
        jito_block_engine_url: String,
        jito_api_key: Option<String>,
        grpc_confirmer: GrpcConfirmationMonitor,
        metrics: Option<JitoMetrics>,
        confirmation_timeout: Duration,
    ) -> Self {
        info!(
            jito_block_engine_url = jito_block_engine_url.as_str(),
            ?confirmation_timeout,
            "Creating JitoTransactionSubmitter"
        );

        // Initialize Jito SDK with or without API key
        let jito_sdk = match jito_api_key {
            Some(key) => JitoJsonRpcSDK::new(&jito_block_engine_url, Some(key)),
            None => JitoJsonRpcSDK::new(&jito_block_engine_url, None),
        };

        Self {
            jito_sdk,
            grpc_confirmer,
            metrics,
            confirmation_timeout,
        }
    }

    async fn get_bundle_status(&self, bundle_uuid: &str) -> Result<BundleStatus> {
        let status_response = self
            .jito_sdk
            .get_bundle_statuses(vec![bundle_uuid.to_string()])
            .await?;

        status_response
            .get("result")
            .and_then(|result| result.get("value"))
            .and_then(|value| value.as_array())
            .and_then(|statuses| statuses.get(0))
            .ok_or_else(|| anyhow!("Failed to parse bundle status"))
            .map(|bundle_status| BundleStatus {
                err: bundle_status.get("err").cloned(),
                transactions: bundle_status
                    .get("transactions")
                    .and_then(|t| t.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    }),
            })
    }

    async fn get_in_flight_bundle_status(&self, bundle_uuid: &str) -> Result<Option<String>> {
        let status_response = self
            .jito_sdk
            .get_in_flight_bundle_statuses(vec![bundle_uuid.to_string()])
            .await?;

        if let Some(result) = status_response.get("result") {
            if let Some(value) = result.get("value") {
                if let Some(statuses) = value.as_array() {
                    if let Some(bundle_status) = statuses.get(0) {
                        if let Some(status) = bundle_status.get("status") {
                            return Ok(status.as_str().map(String::from));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn check_transaction_error(&self, bundle_status: &BundleStatus) -> Result<()> {
        if let Some(err) = &bundle_status.err {
            if err["Ok"].is_null() {
                debug!("Transaction executed without errors.");
                Ok(())
            } else {
                tracing::error!(?err, "Transaction encountered an error");
                Err(anyhow!("Transaction encountered an error"))
            }
        } else {
            Ok(())
        }
    }

    fn get_transaction_signature(&self, bundle_status: &BundleStatus) -> Result<Signature> {
        if let Some(transactions) = &bundle_status.transactions {
            if let Some(tx_id) = transactions.first() {
                debug!("Transaction ID: {}", tx_id);
                return Signature::from_str(tx_id).context("Failed to parse transaction signature");
            }
        }

        Err(anyhow!("No transactions found in the bundle status"))
    }
}

#[async_trait]
impl TransactionSubmitter for JitoTransactionSubmitter {
    #[instrument(skip(self, transaction))]
    async fn submit_transaction(
        &self,
        transaction: &VersionedTransaction,
        _skip_simulation: bool, // Ignored for Jito implementation
    ) -> Result<Signature> {
        // n.b., it is important to calculate the signature before
        // sending to Jito because we might receive a gRPC event
        // before the Jito response.
        let signature = transaction.signatures[0];

        // Increment the submission attempts counter
        if let Some(metrics) = &self.metrics {
            metrics.bundle_submission_attempts_total.inc();
        }
        tracing::info!(%signature, "Calculated transaction signature for Jito submission");

        // Serialize the transaction for Jito
        let serialized_tx = bs58::encode(bincode::serialize(&transaction)?).into_string();

        // Setup confirmation notification channel via the central monitor
        let mut rx_confirm_receiver = self.grpc_confirmer.register_confirmation_channel(signature);
        tracing::info!(%signature, "Registered confirmation channel with GrpcConfirmationMonitor");

        // Prepare bundle for submission (array of transactions)
        let bundle = serde_json::json!([serialized_tx]);
        let uuid = None;

        // Send bundle using Jito SDK
        info!("Sending transaction as bundle via Jito");
        let response = self
            .jito_sdk
            .send_bundle(Some(bundle), uuid)
            .await
            .with_context(|| format!("send_bundle(uuid={:?})", uuid))?;

        // Extract bundle UUID from response
        let bundle_uuid = response["result"]
            .as_str()
            .ok_or_else(|| anyhow!("Failed to get bundle UUID from response"))?;

        tracing::info!(bundle_uuid, "Bundle sent with UUID");

        // Poll for bundle status or wait for gRPC confirmation
        let max_retries = 10;
        let retry_delay = Duration::from_secs(2);

        match timeout(self.confirmation_timeout, async {
            let mut attempts = 0;
            loop {
                tokio::select! {
                    biased; // Check receiver first if a message is already waiting
                    _ = &mut rx_confirm_receiver => {
                        info!(%signature, "Transaction confirmed via gRPC stream");
                        if let Some(metrics) = &self.metrics {
                            metrics.bundle_landed_total.inc();
                            metrics.update_landing_rate();
                        }
                        // No need to deregister here, the monitor handles it
                        // when sending the notification.
                        tracing::debug!(%signature, "gRPC confirmation received, monitor handled deregistration.");
                        // Deregister since confirmation happened via gRPC
                        self.grpc_confirmer.deregister_confirmation_channel(signature);
                        return Ok(signature);
                    },
                    _ = sleep(retry_delay), if attempts > 0 => {
                        // Wait before next poll attempt, but not on the first try
                    }
                }

                // If we selected sleep or it's the first attempt, poll Jito
                if attempts >= max_retries {
                     return Err(anyhow!(
                        "Polling timed out after {} attempts waiting for bundle {}",
                        max_retries, bundle_uuid
                    ));
                }
                attempts += 1;

                debug!(
                    %signature,
                    bundle_uuid,
                    attempt = attempts,
                    max_attempts = max_retries,
                    "Polling Jito bundle status"
                );

                let status = self.get_in_flight_bundle_status(bundle_uuid).await?;

                match status.as_deref() {
                    Some("Landed") => {
                        info!(%signature, bundle_uuid, "Jito reported bundle landed. Checking final status.");
                        // Get final bundle status to check for errors
                        let bundle_status = self
                            .get_bundle_status(bundle_uuid)
                            .await
                            .context("get_bundle_status after landing")?;
                        self.check_transaction_error(&bundle_status)
                            .await
                            .context("check_transaction_error after landing")?;

                        // We already know the signature, but can verify if needed:
                        let landed_signature = self.get_transaction_signature(&bundle_status)?;
                        if landed_signature != signature {
                            return Err(anyhow!(
                                "Transaction signature mismatch: expected {}, got {}",
                                signature,
                                landed_signature
                            ));
                        }

                        info!(%signature, "Transaction executed successfully via Jito polling");
                        if let Some(metrics) = &self.metrics {
                            metrics.bundle_landed_total.inc();
                            metrics.update_landing_rate();
                        }
                        // Deregister since confirmation happened via polling
                        self.grpc_confirmer.deregister_confirmation_channel(signature);
                        return Ok(signature);
                    }
                    Some("Pending") | Some(_) | None => {
                        // Continue polling
                        debug!(%signature, bundle_uuid, ?status, "Bundle status not 'Landed', continuing poll");
                    }
                }
            }
        }).await {
            Ok(Ok(sig)) => Ok(sig), // Inner Ok is from the async block logic
            Ok(Err(e)) => { // Inner Err is from the async block logic (e.g., poll error)
                self.grpc_confirmer.deregister_confirmation_channel(signature); // Deregister on polling failure
                Err(e)
            },
            Err(_) => { // Outer Err is from timeout
                self.grpc_confirmer.deregister_confirmation_channel(signature); // Deregister on timeout
                Err(anyhow!("Failed to confirm bundle {} within {:?} timeout", bundle_uuid, self.confirmation_timeout))
            },
        }
    }

    #[instrument(skip(self), level = "info")]
    async fn confirm_transaction(&self, signature: Signature) -> Result<()> {
        info!(%signature, "Jito confirmation handled within submit_transaction. Assuming success if submit returned Ok.");
        // If submit_transaction succeeded, we assume it was confirmed either via polling or gRPC.
        // If it failed, this method wouldn't be called or the error would propagate earlier.
        Ok(())
    }
}
