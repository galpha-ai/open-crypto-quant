use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use super::DexType;

mod shadow;
mod tcp;
mod unix;

pub use shadow::ShadowTxnMakerClient;
pub use tcp::TcpTxnMakerClient;
pub use unix::UnixSocketTxnMakerClient;

#[derive(Debug, Deserialize)]
pub struct TxnMakerResponse {
    #[serde(rename = "encoded64")]
    pub encoded64: Option<String>,
    pub error: Option<String>,
}

#[async_trait]
pub trait TxnMakerClient: Send + Sync {
    async fn send_request(
        &self,
        request: &serde_json::Value,
        dex_type: DexType,
        timeout: Duration,
    ) -> Result<TxnMakerResponse>;
}
