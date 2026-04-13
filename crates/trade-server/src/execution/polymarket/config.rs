//! Configuration for the Polymarket executor.

use serde::{Deserialize, Serialize};

/// Configuration for the Polymarket order executor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketConfig {
    /// CLOB API host URL (e.g., "https://clob.polymarket.com")
    pub host: String,

    /// Private key for signing orders (hex string, loaded from environment)
    #[serde(skip_serializing)]
    pub private_key: String,

    /// Chain ID (137 for Polygon mainnet, 80002 for Amoy testnet)
    #[serde(default = "default_chain_id")]
    pub chain_id: u64,

    // === Safe wallet configuration for redemption ===
    /// RPC URL for Safe wallet transactions (e.g., "https://polygon-rpc.com")
    pub safe_rpc_url: Option<String>,

    /// Safe wallet address (proxy wallet that holds the tokens)
    pub safe_address: Option<String>,

    // === Dry run mode ===
    /// Dry run mode: log orders without executing them
    #[serde(default)]
    pub dry_run: bool,
}

fn default_chain_id() -> u64 {
    137 // Polygon mainnet
}

impl Default for PolymarketConfig {
    fn default() -> Self {
        Self {
            host: "https://clob.polymarket.com".to_string(),
            private_key: String::new(),
            chain_id: default_chain_id(),
            safe_rpc_url: None,
            safe_address: None,
            dry_run: true, // Default to dry run for safety
        }
    }
}

impl PolymarketConfig {
    /// Create a new config with the given host and private key
    pub fn new(host: impl Into<String>, private_key: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            private_key: private_key.into(),
            ..Default::default()
        }
    }

    /// Check if Safe wallet is configured for redemption operations
    pub fn has_safe_config(&self) -> bool {
        self.safe_rpc_url.is_some() && self.safe_address.is_some()
    }

    /// Check if dry run mode is enabled
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}
