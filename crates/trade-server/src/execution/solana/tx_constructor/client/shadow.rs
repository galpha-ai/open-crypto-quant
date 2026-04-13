use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tracing::{error, info, warn};

use super::{super::DexType, TxnMakerClient, TxnMakerResponse};

pub struct ShadowTxnMakerClient {
    tcp_client: Box<dyn TxnMakerClient>,
    unix_client: Box<dyn TxnMakerClient>,
}

impl ShadowTxnMakerClient {
    pub fn new(tcp_client: Box<dyn TxnMakerClient>, unix_client: Box<dyn TxnMakerClient>) -> Self {
        Self {
            tcp_client,
            unix_client,
        }
    }
}

#[async_trait]
impl TxnMakerClient for ShadowTxnMakerClient {
    async fn send_request(
        &self,
        request: &serde_json::Value,
        dex_type: DexType,
        timeout: Duration,
    ) -> Result<TxnMakerResponse> {
        // Send requests to both endpoints in parallel
        let tcp_future = self.tcp_client.send_request(request, dex_type, timeout);
        let unix_future = self.unix_client.send_request(request, dex_type, timeout);

        let (tcp_result, unix_result) = tokio::join!(tcp_future, unix_future);

        // Log comparison results
        match (&tcp_result, &unix_result) {
            (Ok(tcp_resp), Ok(unix_resp)) => {
                // Compare responses
                let tcp_encoded = tcp_resp.encoded64.as_deref().unwrap_or("");
                let unix_encoded = unix_resp.encoded64.as_deref().unwrap_or("");
                let tcp_error = tcp_resp.error.as_deref();
                let unix_error = unix_resp.error.as_deref();

                if tcp_encoded == unix_encoded && tcp_error == unix_error {
                    info!(
                        dex_type = ?dex_type,
                        mode = "shadow",
                        result = "match",
                        "Shadow mode responses match"
                    );
                } else {
                    warn!(
                        dex_type = ?dex_type,
                        mode = "shadow",
                        result = "mismatch",
                        tcp_has_encoded = !tcp_encoded.is_empty(),
                        unix_has_encoded = !unix_encoded.is_empty(),
                        tcp_error = ?tcp_error,
                        unix_error = ?unix_error,
                        "Shadow mode response mismatch"
                    );
                }
            }
            (Ok(_), Err(e)) => {
                error!(
                    dex_type = ?dex_type,
                    mode = "shadow",
                    failed_client = "unix",
                    error = %e,
                    "Shadow mode Unix socket request failed"
                );
            }
            (Err(e), Ok(_)) => {
                error!(
                    dex_type = ?dex_type,
                    mode = "shadow",
                    failed_client = "tcp",
                    error = %e,
                    "Shadow mode TCP request failed"
                );
            }
            (Err(tcp_e), Err(unix_e)) => {
                error!(
                    dex_type = ?dex_type,
                    mode = "shadow",
                    failed_clients = "both",
                    tcp_error = %tcp_e,
                    unix_error = %unix_e,
                    "Shadow mode both requests failed"
                );
            }
        }

        // Always return TCP result to maintain production behavior
        tcp_result
    }
}
