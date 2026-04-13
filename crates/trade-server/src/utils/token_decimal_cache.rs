use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use spl_token::state::Mint;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;
use tracing::{debug, info};

/// A thread-safe cache for token decimals that fetches from RPC when needed
#[derive(Debug)]
pub struct TokenDecimalCache {
    cache: Mutex<HashMap<String, u8>>,
}

impl TokenDecimalCache {
    /// Create a new empty token decimal cache
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Get token decimals for a given mint address
    /// Returns cached value if available, uses hardcoded values for known DEX tokens,
    /// otherwise fetches from RPC and caches the result
    pub async fn get_decimals(&self, mint: &str, rpc_client: &RpcClient) -> Result<u8> {
        // Check cache first
        if let Ok(cache) = self.cache.lock() {
            if let Some(&decimals) = cache.get(mint) {
                debug!(
                    mint = mint,
                    decimals = decimals,
                    "Retrieved token decimals from cache"
                );
                return Ok(decimals);
            }
        }

        // Use hardcoded decimals for known DEX tokens to avoid RPC calls
        let decimals = if mint.ends_with("bonk") || mint.ends_with("pump") {
            debug!(
                mint = mint,
                "Using hardcoded 6 decimals for bonk/pump token"
            );
            6
        } else {
            // Fetch from RPC for unknown tokens
            self.fetch_decimals_from_rpc(mint, rpc_client).await?
        };

        // Cache the result
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(mint.to_string(), decimals);
            info!(
                mint = mint,
                decimals = decimals,
                source = if mint.ends_with("bonk") || mint.ends_with("pump") {
                    "hardcoded"
                } else {
                    "rpc"
                },
                "Cached token decimals for mint"
            );
        }

        Ok(decimals)
    }

    /// Get the token multiplier (10^decimals) for converting between base and standard units
    pub async fn get_token_multiplier(&self, mint: &str, rpc_client: &RpcClient) -> Result<f64> {
        let decimals = self.get_decimals(mint, rpc_client).await?;
        Ok(10_f64.powi(decimals as i32))
    }

    /// Convert from base units to standard units using cached decimals
    pub async fn base_to_standard(
        &self,
        mint: &str,
        base_amount: u64,
        rpc_client: &RpcClient,
    ) -> Result<f64> {
        let multiplier = self.get_token_multiplier(mint, rpc_client).await?;
        Ok(base_amount as f64 / multiplier)
    }

    /// Convert from standard units to base units using cached decimals
    pub async fn standard_to_base(
        &self,
        mint: &str,
        standard_amount: f64,
        rpc_client: &RpcClient,
    ) -> Result<u64> {
        let multiplier = self.get_token_multiplier(mint, rpc_client).await?;
        Ok((standard_amount * multiplier) as u64)
    }

    /// Fetch token decimals from RPC
    async fn fetch_decimals_from_rpc(&self, mint: &str, rpc_client: &RpcClient) -> Result<u8> {
        let mint_pubkey = Pubkey::from_str(mint)
            .map_err(|e| anyhow::anyhow!("Invalid mint address '{}': {}", mint, e))?;

        let account = rpc_client
            .get_account(&mint_pubkey)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get mint account for '{}': {}", mint, e))?;

        let mint_data = Mint::unpack(&account.data)
            .map_err(|e| anyhow::anyhow!("Failed to unpack mint data for '{}': {}", mint, e))?;

        debug!(
            mint = mint,
            decimals = mint_data.decimals,
            "Fetched token decimals from RPC"
        );

        Ok(mint_data.decimals)
    }

    /// Clear the cache (useful for testing or if needed for cleanup)
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
            debug!("Cleared token decimal cache");
        }
    }

    /// Get cache size (useful for monitoring)
    #[allow(dead_code)]
    pub fn cache_size(&self) -> usize {
        if let Ok(cache) = self.cache.lock() {
            cache.len()
        } else {
            0
        }
    }
}

impl Default for TokenDecimalCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;

    mock! {
        RpcClient {
            fn get_account(&self, pubkey: &Pubkey) -> Result<solana_sdk::account::Account, solana_client::client_error::ClientError>;
        }
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let cache = TokenDecimalCache::new();

        // Test that cache starts empty
        assert_eq!(cache.cache_size(), 0);

        // Test clear cache
        cache.clear_cache();
        assert_eq!(cache.cache_size(), 0);
    }
}
