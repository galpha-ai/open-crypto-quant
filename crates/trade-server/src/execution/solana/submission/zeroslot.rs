use std::{str::FromStr, time::Duration};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, signature::Signature, transaction::VersionedTransaction,
};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use super::submitter::TransactionSubmitter;
use crate::execution::solana::confirmation::GrpcConfirmationMonitor;

#[derive(Serialize, Debug)]
struct ZeroSlotSendTxParams {
    encoding: String, // "base64"
}

#[derive(Serialize, Debug)]
struct ZeroSlotRequest {
    jsonrpc: String,
    id: String,
    method: String,                         // "sendTransaction"
    params: (String, ZeroSlotSendTxParams), // [base64_encoded_tx, params_object]
}

#[derive(Deserialize, Debug)]
struct ZeroSlotError {
    code: i64,
    message: String,
}

#[derive(Deserialize, Debug)]
struct ZeroSlotResponse {
    result: Option<String>, // Signature as string
    error: Option<ZeroSlotError>,
}

pub struct ZeroSlotTransactionSubmitter {
    zeroslot_api_url: String, // e.g., "http://ny.0slot.trade"
    zeroslot_api_key: String,
    http_client: reqwest::Client,
    rpc_client: RpcClient, // For confirmation polling fallback
    grpc_confirmer: GrpcConfirmationMonitor, // Use the central monitor
    confirmation_timeout: Duration, // Configurable confirmation timeout
}

impl ZeroSlotTransactionSubmitter {
    pub fn new(
        zeroslot_api_url: String,
        zeroslot_api_key: String,
        rpc_url_for_confirmation: String, // Needed for confirmation polling
        grpc_confirmer: GrpcConfirmationMonitor,
        confirmation_timeout: Duration,
    ) -> Self {
        info!(
            zeroslot_api_url = zeroslot_api_url.as_str(),
            rpc_url_for_confirmation = rpc_url_for_confirmation.as_str(),
            ?confirmation_timeout,
            "Creating ZeroSlotTransactionSubmitter"
        );
        Self {
            zeroslot_api_url,
            zeroslot_api_key,
            http_client: reqwest::Client::new(),
            rpc_client: RpcClient::new(rpc_url_for_confirmation),
            grpc_confirmer,
            confirmation_timeout,
        }
    }
}

#[async_trait]
impl TransactionSubmitter for ZeroSlotTransactionSubmitter {
    #[instrument(skip(self, transaction), level = "info")]
    async fn submit_transaction(
        &self,
        transaction: &VersionedTransaction,
        _skip_simulation: bool, // 0slot doesn't have this option in sendTransaction
    ) -> Result<Signature> {
        // 1. Serialize and Base64 encode
        let serialized_tx =
            bincode::serialize(&transaction).context("Failed to serialize transaction")?;
        let encoded_tx = general_purpose::STANDARD.encode(&serialized_tx);

        // 3. Construct 0slot request payload
        let request_id = Uuid::new_v4().to_string();
        let payload = ZeroSlotRequest {
            jsonrpc: "2.0".to_string(),
            id: request_id.clone(),
            method: "sendTransaction".to_string(),
            params: (
                encoded_tx,
                ZeroSlotSendTxParams {
                    encoding: "base64".to_string(),
                },
            ),
        };

        let request_url = format!(
            "{}?api-key={}",
            self.zeroslot_api_url, self.zeroslot_api_key
        );
        info!(
            url = self.zeroslot_api_url.as_str(), // Don't log full URL with key
            id = request_id,
            payload = ?payload.params.1, // Log params only, not the full tx
            "Sending transaction via 0slot sendTransaction endpoint"
        );

        // 4. Send request to 0slot
        let response = self
            .http_client
            .post(&request_url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send request to 0slot")?;

        // 5. Handle response
        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            error!(status = %status, body = error_body, "0slot submission failed (HTTP)");
            return Err(anyhow!(
                "0slot submission failed with HTTP status {}: {}",
                status,
                error_body
            ));
        }

        let response_body = response
            .json::<ZeroSlotResponse>()
            .await
            .context("Failed to parse 0slot response JSON")?;

        debug!(response = ?response_body, "Received 0slot response");

        // Check for JSON-RPC level errors
        if let Some(error) = response_body.error {
            error!(
                code = error.code,
                message = error.message,
                "0slot submission failed (API error)"
            );
            return Err(anyhow!(
                "0slot submission failed with API error {}: {}",
                error.code,
                error.message
            ));
        }

        // 6. Parse and return signature
        let signature_str = response_body
            .result
            .ok_or_else(|| anyhow!("Missing 'result' (signature) in 0slot response"))?;
        let signature = Signature::from_str(&signature_str)
            .context("Failed to parse signature from 0slot response")?;

        info!(%signature, "Transaction submitted successfully via 0slot");

        // --- Start Confirmation Logic ---
        let mut rx_confirm_receiver = self.grpc_confirmer.register_confirmation_channel(signature);
        tracing::debug!(%signature, "Registered confirmation channel with GrpcConfirmationMonitor");

        // Confirmation parameters
        let poll_interval = Duration::from_millis(500);
        let total_timeout = self.confirmation_timeout;
        let commitment_config = CommitmentConfig::confirmed();

        match timeout(total_timeout, async {
            let mut attempts = 0;
            loop {
                tokio::select! {
                    biased; // Check receiver first if a message is already waiting
                    _ = &mut rx_confirm_receiver => {
                        info!(%signature, "Transaction confirmed via gRPC stream");
                        // No need to deregister, monitor handles it.
                        tracing::debug!(%signature, "gRPC confirmation received, monitor handled deregistration.");
                        return Ok::<_, anyhow::Error>(signature);
                    },
                    _ = sleep(poll_interval), if attempts > 0 => {
                        // Wait before next poll attempt, but not on the first try
                    }
                }
                attempts += 1;
                debug!(%signature, attempt = attempts, "Polling RPC for confirmation");

                // Poll RPC
                match self
                    .rpc_client
                    .confirm_transaction_with_commitment(&signature, commitment_config)
                {
                    Ok(response) => {
                        if response.value {
                            info!(%signature, "Transaction confirmed via RPC poll");
                            // Deregister since confirmation happened via polling
                            self.grpc_confirmer.deregister_confirmation_channel(signature);
                            return Ok(signature);
                        } else {
                            debug!(%signature, "Transaction not yet confirmed via RPC poll, continuing...");
                            // Continue loop
                        }
                    }
                    Err(e) => {
                        // Log RPC errors but keep trying until timeout
                        error!(%signature, error = ?e, "Error polling confirmation status via RPC, continuing...");
                        // Continue loop
                    }
                }
                // Implicit timeout check is handled by the outer `timeout` future
            }
        }).await {
            Ok(Ok(sig)) => Ok(sig), // Inner Ok is from the async block logic
            Ok(Err(e)) => { // Inner Err should not happen in this setup unless RPC call itself panics
                self.grpc_confirmer.deregister_confirmation_channel(signature); // Deregister on unexpected error
                Err(anyhow!("Error during confirmation polling (signature='{}'): {}", signature, e))
            },
            Err(_) => { // Outer Err is from timeout
                self.grpc_confirmer.deregister_confirmation_channel(signature); // Deregister on timeout
                Err(anyhow!(
                    "Failed to confirm transaction {} via gRPC or RPC polling within {:?} timeout",
                    signature,
                    total_timeout
                ))
            },
        }
        // --- End Confirmation Logic ---
    }

    #[instrument(skip(self), level = "info")]
    async fn confirm_transaction(&self, signature: Signature) -> Result<()> {
        info!(%signature, "0slot confirmation handled within submit_transaction. Assuming success if submit returned Ok.");
        Ok(())
    }
}
