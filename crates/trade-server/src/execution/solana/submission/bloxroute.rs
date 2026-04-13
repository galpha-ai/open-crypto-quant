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
use tracing::{debug, info, instrument, warn};

use super::submitter::TransactionSubmitter;
use crate::execution::solana::confirmation::GrpcConfirmationMonitor;

#[derive(Serialize, Debug)]
struct BloxrouteTransactionContent {
    content: String, // base64 encoded signed transaction bytes
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BloxrouteSubmitRequest {
    transaction: BloxrouteTransactionContent,
    skip_pre_flight: bool,          // Corresponds to skip_simulation
    front_running_protection: bool, // Always false for our use case
    #[serde(rename = "useStakedRPCs")]
    use_staked_rpcs: bool, // Always true for our use case
}

#[derive(Deserialize, Debug)]
struct BloxrouteSubmitResponse {
    signature: String,
}

pub struct BloxrouteTransactionSubmitter {
    bloxroute_api_url: String,     // e.g., "https://ny.solana.dex.blxrbdn.com"
    bloxroute_auth_header: String, // "Authorization-Header-Value"
    http_client: reqwest::Client,
    rpc_client: RpcClient,                   // For confirmation polling
    grpc_confirmer: GrpcConfirmationMonitor, // Use the central monitor
    confirmation_timeout: Duration,
}

impl BloxrouteTransactionSubmitter {
    pub fn new(
        bloxroute_api_url: String,
        bloxroute_auth_header: String,
        rpc_url_for_confirmation: String, // Needed for confirmation polling
        grpc_confirmer: GrpcConfirmationMonitor,
        confirmation_timeout: Duration,
    ) -> Self {
        info!(
            bloxroute_api_url = bloxroute_api_url.as_str(),
            rpc_url_for_confirmation = rpc_url_for_confirmation.as_str(),
            ?confirmation_timeout,
            "Creating BloxrouteTransactionSubmitter"
        );
        Self {
            bloxroute_api_url,
            bloxroute_auth_header,
            http_client: reqwest::Client::new(),
            rpc_client: RpcClient::new(rpc_url_for_confirmation),
            grpc_confirmer,
            confirmation_timeout,
        }
    }
}

#[async_trait]
impl TransactionSubmitter for BloxrouteTransactionSubmitter {
    #[instrument(skip(self, transaction), level = "info")]
    async fn submit_transaction(
        &self,
        transaction: &VersionedTransaction,
        skip_simulation: bool, // Maps to skipPreFlight
    ) -> Result<Signature> {
        // 1. Serialize and Base64 encode
        let serialized_tx =
            bincode::serialize(&transaction).context("Failed to serialize transaction")?;
        let encoded_tx = general_purpose::STANDARD.encode(&serialized_tx);

        // 3. Construct bloXroute request payload
        let payload = BloxrouteSubmitRequest {
            transaction: BloxrouteTransactionContent {
                content: encoded_tx,
            },
            skip_pre_flight: skip_simulation,
            front_running_protection: false,
            use_staked_rpcs: true,
        };

        let request_url = format!("{}/api/v2/submit", self.bloxroute_api_url);
        info!(
            url = request_url.as_str(),
            payload = ?payload,
            "Sending transaction via bloXroute submit endpoint"
        );

        // 4. Send request to bloXroute
        let response = self
            .http_client
            .post(&request_url)
            .header("Authorization", &self.bloxroute_auth_header)
            .json(&payload)
            .send()
            .await
            .context("Failed to send request to bloXroute")?;

        // 5. Handle response
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            warn!(status = %status, body = error_body, "bloXroute submission failed");
            return Err(anyhow!(
                "bloXroute submission failed with status {}: {}",
                status,
                error_body
            ));
        }

        let response_body = response
            .json::<BloxrouteSubmitResponse>()
            .await
            .context("Failed to parse bloXroute response JSON")?;

        debug!(response = ?response_body, "Received bloXroute submit response");

        // 6. Parse and return signature
        let signature = Signature::from_str(&response_body.signature)
            .context("Failed to parse signature from bloXroute response")?;

        info!(%signature, "Transaction submitted successfully via bloXroute");

        // --- Start Confirmation Logic (similar to Jito) ---
        let mut rx_confirm_receiver = self.grpc_confirmer.register_confirmation_channel(signature);
        tracing::info!(%signature, "Registered confirmation channel with GrpcConfirmationMonitor");

        // Confirmation parameters
        let poll_interval = Duration::from_millis(500);
        let commitment_config = CommitmentConfig::confirmed();

        match timeout(self.confirmation_timeout, async {
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
                        warn!(%signature, error = ?e, "Error polling confirmation status via RPC, continuing...");
                        // Continue loop
                    }
                }
                // Implicit timeout check is handled by the outer `timeout` future
            }
        }).await {
            Ok(Ok(sig)) => Ok(sig), // Inner Ok is from the async block logic
            Ok(Err(e)) => { // Inner Err should not happen in this setup unless RPC call itself panics
                self.grpc_confirmer.deregister_confirmation_channel(signature); // Deregister on unexpected error
                Err(anyhow!("Error during confirmation polling(signature='{}'): {}", signature, e))
            },
            Err(_) => { // Outer Err is from timeout
                self.grpc_confirmer.deregister_confirmation_channel(signature); // Deregister on timeout
                Err(anyhow!(
                    "Failed to confirm transaction {} via gRPC or RPC polling within {:?} timeout",
                    signature,
                    self.confirmation_timeout
                ))
            },
        }
        // --- End Confirmation Logic ---
    }

    #[instrument(skip(self), level = "info")]
    async fn confirm_transaction(&self, signature: Signature) -> Result<()> {
        info!(%signature, "bloXroute confirmation handled within submit_transaction. Assuming success if submit returned Ok.");
        Ok(())
    }
}
