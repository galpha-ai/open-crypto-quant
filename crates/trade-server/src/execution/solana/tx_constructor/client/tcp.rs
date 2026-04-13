use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use super::{super::DexType, TxnMakerClient, TxnMakerResponse};

pub struct TcpTxnMakerClient {
    server_url: String,
    client: reqwest::Client,
}

impl TcpTxnMakerClient {
    pub fn new(server_url: String, timeout: Duration) -> Result<Self> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;

        Ok(Self { server_url, client })
    }
}

#[async_trait]
impl TxnMakerClient for TcpTxnMakerClient {
    async fn send_request(
        &self,
        request: &serde_json::Value,
        dex_type: DexType,
        _timeout: Duration,
    ) -> Result<TxnMakerResponse> {
        let url = format!("{}{}", self.server_url, dex_type.endpoint());

        let response = self.client.post(&url).json(request).send().await?;

        let status = response.status();
        let response_text = response.text().await?;

        tracing::info!(
            status = status.as_u16(),
            response_text = response_text.as_str(),
            url = url.as_str(),
            "Received TxnMaker response"
        );

        if response_text.is_empty() {
            return Err(anyhow::anyhow!("Empty response from TxnMaker service"));
        }

        let parsed_response: TxnMakerResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse TxnMaker response: {:?}. Response text: {}",
                    e,
                    response_text
                )
            })?;

        Ok(parsed_response)
    }
}
