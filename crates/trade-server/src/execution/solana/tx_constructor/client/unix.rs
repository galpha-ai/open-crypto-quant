use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use super::{super::DexType, TxnMakerClient, TxnMakerResponse};

pub struct UnixSocketTxnMakerClient {
    socket_path: String,
}

impl UnixSocketTxnMakerClient {
    pub fn new(socket_path: String) -> Self {
        Self { socket_path }
    }
}

#[async_trait]
impl TxnMakerClient for UnixSocketTxnMakerClient {
    async fn send_request(
        &self,
        request: &serde_json::Value,
        dex_type: DexType,
        _timeout: Duration,
    ) -> Result<TxnMakerResponse> {
        use http_client_unix_domain_socket::{ClientUnix, ErrorAndResponseJson, Method};

        let socket_path = self.socket_path.clone();
        let endpoint = dex_type.endpoint();

        tracing::info!(
            socket_path = socket_path.as_str(),
            endpoint = endpoint,
            "Sending request to Unix socket"
        );

        // Create a Unix socket client
        let mut client = ClientUnix::try_new(&socket_path).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to connect to Unix socket at '{}': {}",
                socket_path,
                e
            )
        })?;

        // Define error type for unsuccessful responses
        #[derive(serde::Deserialize, Debug)]
        struct ErrorResponse {
            error: Option<String>,
            message: Option<String>,
        }

        // Send JSON POST request
        match client
            .send_request_json::<serde_json::Value, TxnMakerResponse, ErrorResponse>(
                endpoint,
                Method::POST,
                &[("Host", "localhost")],
                Some(request),
            )
            .await
        {
            Ok((status_code, response)) => {
                tracing::info!(
                    status = status_code.as_u16(),
                    encoded64 = ?response.encoded64,
                    error = ?response.error,
                    socket_path = socket_path.as_str(),
                    "Received Unix socket response"
                );
                Ok(response)
            }
            Err(ErrorAndResponseJson::ResponseUnsuccessful(status_code, error_response)) => {
                let error_msg = error_response
                    .error
                    .or(error_response.message)
                    .unwrap_or_else(|| format!("HTTP {} error", status_code.as_u16()));

                tracing::error!(
                    status = status_code.as_u16(),
                    error = error_msg.as_str(),
                    socket_path = socket_path.as_str(),
                    "Unix socket request failed"
                );

                Err(anyhow::anyhow!(
                    "TxnMaker service returned error ({}): {}",
                    status_code.as_u16(),
                    error_msg
                ))
            }
            Err(ErrorAndResponseJson::InternalError(e)) => {
                tracing::error!(
                    error = %e,
                    socket_path = socket_path.as_str(),
                    "Internal error during Unix socket request"
                );

                Err(anyhow::anyhow!(
                    "Internal error during Unix socket request: {}",
                    e
                ))
            }
        }
    }
}
