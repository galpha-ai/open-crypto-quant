//! Builder for PolymarketOrderExecutor.

use std::sync::Arc;

use alloy_primitives::Address;
use anyhow::{Result, anyhow};
use polyfill_rs::{ClobClient, client::ApiCreds};
use tokio::sync::watch;
use tracing::info;

use crate::client::polymarket::{SafeClient, SafeClientConfig};
use crate::event_coordinator::EventCoordinator;
use crate::position::PositionManager;

use super::poller::{MergeConfig, MergeExecutor, OrderStatusPoller, PollerConfig};
use super::{config::PolymarketConfig, executor::PolymarketOrderExecutor};

/// Builder for creating a `PolymarketOrderExecutor`.
///
/// # Example
///
/// ```ignore
/// let executor = PolymarketOrderExecutorBuilder::new()
///     .host("https://clob.polymarket.com")
///     .private_key("0x...")
///     .chain_id(137)
///     .event_coordinator(coordinator)
///     .build()
///     .await?;
/// ```
pub struct PolymarketOrderExecutorBuilder {
    host: Option<String>,
    private_key: Option<String>,
    chain_id: u64,
    api_key: Option<String>,
    api_secret: Option<String>,
    api_passphrase: Option<String>,
    signature_type: Option<u8>,
    funder: Option<String>,
    event_coordinator: Option<Arc<dyn EventCoordinator>>,
    // Safe wallet for redemption
    safe_rpc_url: Option<String>,
    safe_address: Option<String>,
    // Dry run mode
    dry_run: bool,
    // Order status poller config
    poller_config: Option<PollerConfig>,
    // Merge executor config
    merge_config: Option<MergeConfig>,
    shutdown_rx: Option<watch::Receiver<bool>>,
    // Position manager for drift detection
    position_manager: Option<Arc<dyn PositionManager>>,
    // HTTP proxy URL for bypassing Cloudflare
    http_proxy_url: Option<String>,
}

impl Default for PolymarketOrderExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PolymarketOrderExecutorBuilder {
    /// Create a new builder with default values
    pub fn new() -> Self {
        Self {
            host: None,
            private_key: None,
            chain_id: 137, // Polygon mainnet
            api_key: None,
            api_secret: None,
            api_passphrase: None,
            signature_type: None,
            funder: None,
            event_coordinator: None,
            safe_rpc_url: None,
            safe_address: None,
            dry_run: true, // Default to dry run for safety
            poller_config: None,
            merge_config: None,
            shutdown_rx: None,
            position_manager: None,
            http_proxy_url: None,
        }
    }

    /// Set the CLOB API host URL
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Set the private key for signing orders
    pub fn private_key(mut self, key: impl Into<String>) -> Self {
        self.private_key = Some(key.into());
        self
    }

    /// Set the chain ID (default: 137 for Polygon mainnet)
    pub fn chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }

    /// Set the API key
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set the API secret
    pub fn api_secret(mut self, secret: impl Into<String>) -> Self {
        self.api_secret = Some(secret.into());
        self
    }

    /// Set the API passphrase
    pub fn api_passphrase(mut self, passphrase: impl Into<String>) -> Self {
        self.api_passphrase = Some(passphrase.into());
        self
    }

    /// Set API credentials from a tuple (key, secret, passphrase)
    pub fn api_credentials(
        mut self,
        key: impl Into<String>,
        secret: impl Into<String>,
        passphrase: impl Into<String>,
    ) -> Self {
        self.api_key = Some(key.into());
        self.api_secret = Some(secret.into());
        self.api_passphrase = Some(passphrase.into());
        self
    }

    /// Set the signature type for proxy wallets
    /// - 0: EOA (default)
    /// - 1: PolyProxy (email/Magic wallet)
    /// - 2: PolyGnosisSafe (browser wallet proxy)
    pub fn signature_type(mut self, sig_type: u8) -> Self {
        self.signature_type = Some(sig_type);
        self
    }

    /// Set the funder address (proxy wallet that holds funds)
    pub fn funder(mut self, funder: impl Into<String>) -> Self {
        self.funder = Some(funder.into());
        self
    }

    /// Configure for proxy wallet (signature_type + funder)
    pub fn proxy_wallet(mut self, signature_type: u8, funder: impl Into<String>) -> Self {
        self.signature_type = Some(signature_type);
        self.funder = Some(funder.into());
        self
    }

    /// Set the event coordinator
    pub fn event_coordinator(mut self, coordinator: Arc<dyn EventCoordinator>) -> Self {
        self.event_coordinator = Some(coordinator);
        self
    }

    /// Set the Safe wallet RPC URL for redemption operations
    pub fn safe_rpc_url(mut self, url: impl Into<String>) -> Self {
        self.safe_rpc_url = Some(url.into());
        self
    }

    /// Set the Safe wallet address for redemption operations
    pub fn safe_address(mut self, address: impl Into<String>) -> Self {
        self.safe_address = Some(address.into());
        self
    }

    /// Configure Safe wallet for pair redemption
    pub fn safe_wallet(mut self, rpc_url: impl Into<String>, address: impl Into<String>) -> Self {
        self.safe_rpc_url = Some(rpc_url.into());
        self.safe_address = Some(address.into());
        self
    }

    /// Enable dry run mode (log orders without executing)
    pub fn dry_run(mut self, enabled: bool) -> Self {
        self.dry_run = enabled;
        self
    }

    /// Set the poller configuration
    pub fn poller_config(mut self, config: PollerConfig) -> Self {
        self.poller_config = Some(config);
        self
    }

    /// Set the merge executor configuration
    pub fn merge_config(mut self, config: MergeConfig) -> Self {
        self.merge_config = Some(config);
        self
    }

    /// Set the shutdown receiver for the poller
    pub fn shutdown_receiver(mut self, rx: watch::Receiver<bool>) -> Self {
        self.shutdown_rx = Some(rx);
        self
    }

    /// Set the position manager for drift detection
    pub fn position_manager(mut self, pm: Arc<dyn PositionManager>) -> Self {
        self.position_manager = Some(pm);
        self
    }

    /// Set the HTTP proxy URL for bypassing Cloudflare
    /// Format: "http://user:pass@host:port"
    pub fn http_proxy(mut self, proxy_url: impl Into<String>) -> Self {
        self.http_proxy_url = Some(proxy_url.into());
        self
    }

    /// Build the executor
    pub async fn build(self) -> Result<PolymarketOrderExecutor> {
        let host = self.host.ok_or_else(|| anyhow!("Host is required"))?;
        let private_key = self
            .private_key
            .ok_or_else(|| anyhow!("Private key is required"))?;
        let event_coordinator = self
            .event_coordinator
            .ok_or_else(|| anyhow!("Event coordinator is required"))?;
        let position_manager = self
            .position_manager
            .ok_or_else(|| anyhow!("Position manager is required"))?;

        // Create client with L1 headers first to derive API credentials if needed
        let temp_client = ClobClient::with_l1_headers(&host, &private_key, self.chain_id);

        // Set up API credentials
        let api_creds = if let (Some(key), Some(secret), Some(passphrase)) =
            (&self.api_key, &self.api_secret, &self.api_passphrase)
        {
            ApiCreds {
                api_key: key.clone(),
                secret: secret.clone(),
                passphrase: passphrase.clone(),
            }
        } else {
            // Create or derive API key
            info!("Creating or deriving API key...");
            temp_client
                .create_or_derive_api_key(None)
                .await
                .map_err(|e| anyhow!("Failed to create or derive API key: {}", e))?
        };

        // Save credentials for executor client before moving to poller client
        let executor_api_creds = api_creds.clone();

        // Create the actual client based on whether proxy wallet and HTTP proxy are configured
        let client = match (&self.signature_type, &self.funder, &self.http_proxy_url) {
            (Some(sig_type), Some(funder), Some(proxy_url)) => {
                info!(signature_type = sig_type, funder = %funder, proxy = %proxy_url, "Using proxy wallet mode with HTTP proxy");
                ClobClient::with_proxy_wallet_and_http_proxy(
                    &host,
                    &private_key,
                    self.chain_id,
                    api_creds,
                    *sig_type,
                    funder,
                    proxy_url,
                )
            }
            (Some(sig_type), Some(funder), None) => {
                info!(signature_type = sig_type, funder = %funder, "Using proxy wallet mode");
                ClobClient::with_proxy_wallet(
                    &host,
                    &private_key,
                    self.chain_id,
                    api_creds,
                    *sig_type,
                    funder,
                )
            }
            _ => {
                // EOA mode - create new client with L2 headers
                info!("Using EOA mode");
                ClobClient::with_l2_headers(&host, &private_key, self.chain_id, api_creds)
            }
        };

        // Prewarm connections
        if let Err(e) = client.prewarm_connections().await {
            tracing::warn!(error = %e, "Failed to prewarm connections");
        }

        // Build config
        let config = PolymarketConfig {
            host: host.clone(),
            private_key: private_key.clone(),
            chain_id: self.chain_id,
            safe_rpc_url: self.safe_rpc_url.clone(),
            safe_address: self.safe_address.clone(),
            dry_run: self.dry_run,
        };

        // Create Safe client (required)
        let safe_rpc_url = self
            .safe_rpc_url
            .ok_or_else(|| anyhow!("Safe RPC URL is required"))?;
        let safe_addr = self
            .safe_address
            .ok_or_else(|| anyhow!("Safe address is required"))?;

        let safe_address: Address = safe_addr
            .parse()
            .map_err(|e| anyhow!("Invalid Safe address: {}", e))?;

        let safe_config = SafeClientConfig {
            rpc_url: safe_rpc_url.clone(),
            chain_id: self.chain_id,
            safe_address,
            max_wait_secs: None,
        };

        let safe_client = SafeClient::new(safe_config, &private_key)
            .map_err(|e| anyhow!("Failed to create Safe client: {}", e))?;

        info!(
            safe_address = %safe_addr,
            "Safe wallet configured for pair redemption"
        );

        // Create order status poller (always required)
        let poller_config = self.poller_config.unwrap_or_default();
        let merge_config = self.merge_config.unwrap_or_default();
        let shutdown_rx = self.shutdown_rx.unwrap_or_else(|| {
            // Create a dummy shutdown receiver if none provided
            let (_, rx) = watch::channel(false);
            rx
        });

        let clob_client = Arc::new(client);

        let poller = Arc::new(OrderStatusPoller::new(
            clob_client.clone(),
            event_coordinator.clone(),
            position_manager.clone(),
            poller_config.clone(),
            shutdown_rx,
        ));

        // Create merge executor
        let merge_executor = Arc::new(MergeExecutor::new(
            safe_client.clone(),
            event_coordinator.clone(),
            clob_client,
            merge_config.clone(),
        ));

        info!(
            host = %config.host,
            chain_id = config.chain_id,
            poll_interval_ms = poller_config.poll_interval.as_millis(),
            merge_poll_interval_ms = merge_config.merge_poll_interval.as_millis(),
            "PolymarketOrderExecutor initialized with poller and merge executor"
        );

        // Create a new ClobClient for the executor since we moved the first one to poller
        let executor_client = match (&self.signature_type, &self.funder, &self.http_proxy_url) {
            (Some(sig_type), Some(funder), Some(proxy_url)) => {
                ClobClient::with_proxy_wallet_and_http_proxy(
                    &host,
                    &private_key,
                    self.chain_id,
                    executor_api_creds,
                    *sig_type,
                    funder,
                    proxy_url,
                )
            }
            (Some(sig_type), Some(funder), None) => ClobClient::with_proxy_wallet(
                &host,
                &private_key,
                self.chain_id,
                executor_api_creds,
                *sig_type,
                funder,
            ),
            _ => {
                ClobClient::with_l2_headers(&host, &private_key, self.chain_id, executor_api_creds)
            }
        };

        Ok(PolymarketOrderExecutor::new(
            executor_client,
            config,
            event_coordinator,
            safe_client,
            poller,
            merge_executor,
            position_manager,
        ))
    }
}
