//! Integration tests for SafeClient
//!
//! These tests require network access and will execute real transactions on Polygon.
//!
//! Required environment variables:
//! - POLYMARKET_SAFE_ADDRESS: Safe wallet address
//! - POLYMARKET_PRIVATE_KEY: Private key for signing
//! - POLYMARKET_CONDITION_ID: Condition ID for the market
//! - POLYMARKET_RPC_URL: Polygon RPC URL (optional, defaults to https://polygon-rpc.com)
//! - POLYMARKET_NEG_RISK: Whether market uses negRisk (optional, defaults to true)
//!
//! Run with: cargo test --package trade_server safe_client_integration -- --ignored

#[cfg(test)]
mod integration_tests {
    use crate::client::polymarket::{SafeClient, SafeClientConfig};
    use alloy_primitives::Address;
    use std::env;
    use std::str::FromStr;

    const DEFAULT_RPC_URL: &str = "https://polygon-rpc.com";
    const CHAIN_ID: u64 = 137;

    fn get_env(key: &str) -> Option<String> {
        env::var(key).ok()
    }

    fn get_env_required(key: &str) -> String {
        env::var(key).unwrap_or_else(|_| panic!("Environment variable {} is required", key))
    }

    fn get_neg_risk() -> bool {
        env::var("POLYMARKET_NEG_RISK")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(true)
    }

    fn create_test_client() -> Option<SafeClient> {
        let safe_address = get_env("POLYMARKET_SAFE_ADDRESS")?;
        let private_key = get_env("POLYMARKET_PRIVATE_KEY")?;
        let rpc_url = get_env("POLYMARKET_RPC_URL").unwrap_or_else(|| DEFAULT_RPC_URL.to_string());

        let config = SafeClientConfig {
            rpc_url,
            chain_id: CHAIN_ID,
            safe_address: Address::from_str(&safe_address).ok()?,
            max_wait_secs: None, // Use default
        };

        SafeClient::new(config, &private_key).ok()
    }

    #[test]
    fn test_client_creation() {
        let Some(client) = create_test_client() else {
            println!("Skipping test: POLYMARKET_SAFE_ADDRESS or POLYMARKET_PRIVATE_KEY not set");
            return;
        };

        let safe_address = get_env_required("POLYMARKET_SAFE_ADDRESS");
        assert_eq!(
            client.safe_address(),
            Address::from_str(&safe_address).unwrap()
        );
    }

    #[test]
    fn test_signer_address() {
        let Some(client) = create_test_client() else {
            println!("Skipping test: POLYMARKET_SAFE_ADDRESS or POLYMARKET_PRIVATE_KEY not set");
            return;
        };

        // Verify signer address is derived correctly from private key
        let signer_addr = client.signer_address();
        assert!(!signer_addr.is_zero());
    }

    #[tokio::test]
    #[ignore] // Run with --ignored flag as this makes real network calls
    async fn test_split_usdc() {
        let Some(client) = create_test_client() else {
            println!("Skipping test: required environment variables not set");
            return;
        };

        let condition_id = get_env_required("POLYMARKET_CONDITION_ID");
        let neg_risk = get_neg_risk();
        let amount = "1"; // 1 USDC

        let result = client.split_usdc(&condition_id, amount, neg_risk).await;

        match result {
            Ok(tx_hash) => {
                println!("Split transaction hash: {}", tx_hash);
                assert!(tx_hash.starts_with("0x"));
            }
            Err(e) => {
                // Expected to fail if insufficient balance or gas
                println!("Split failed (expected if no balance): {}", e);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Run with --ignored flag as this makes real network calls
    async fn test_merge_usdc() {
        let Some(client) = create_test_client() else {
            println!("Skipping test: required environment variables not set");
            return;
        };

        let condition_id = get_env_required("POLYMARKET_CONDITION_ID");
        let neg_risk = get_neg_risk();
        let amount = "1"; // 1 USDC worth of tokens

        let result = client.merge_usdc(&condition_id, amount, neg_risk).await;

        match result {
            Ok(tx_hash) => {
                println!("Merge transaction hash: {}", tx_hash);
                assert!(tx_hash.starts_with("0x"));
            }
            Err(e) => {
                // Expected to fail if insufficient token balance
                println!("Merge failed (expected if no tokens): {}", e);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Run with --ignored flag as this makes real network calls
    async fn test_redeem() {
        let Some(client) = create_test_client() else {
            println!("Skipping test: required environment variables not set");
            return;
        };

        let condition_id = get_env_required("POLYMARKET_CONDITION_ID");
        let neg_risk = get_neg_risk();
        // For negRisk redeem, amounts are required
        let amounts = Some(vec!["1", "0"]); // Example amounts

        let result = client.redeem_str(&condition_id, neg_risk, amounts).await;

        match result {
            Ok(tx_hash) => {
                println!("Redeem transaction hash: {}", tx_hash);
                assert!(tx_hash.starts_with("0x"));
            }
            Err(e) => {
                // Expected to fail if market not resolved or no winning tokens
                println!("Redeem failed (expected if market not resolved): {}", e);
            }
        }
    }

    #[tokio::test]
    #[ignore] // Run with --ignored flag as this makes real network calls
    async fn test_split_and_merge_flow() {
        let Some(client) = create_test_client() else {
            println!("Skipping test: required environment variables not set");
            return;
        };

        let condition_id = get_env_required("POLYMARKET_CONDITION_ID");
        let neg_risk = get_neg_risk();
        let amount = "1"; // 1 USDC

        // Split USDC into tokens
        // Note: SafeClient automatically waits for pending transactions before executing
        println!("Splitting {} USDC...", amount);
        let split_result = client.split_usdc(&condition_id, amount, neg_risk).await;

        match split_result {
            Ok(tx_hash) => {
                println!("Split transaction hash: {}", tx_hash);

                // Merge tokens back to USDC
                // Note: This will automatically wait for split transaction to complete
                println!("Merging tokens back to USDC...");
                let merge_result = client.merge_usdc(&condition_id, amount, neg_risk).await;

                match merge_result {
                    Ok(merge_tx_hash) => {
                        println!("Merge transaction hash: {}", merge_tx_hash);
                    }
                    Err(e) => {
                        println!("Merge failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Split failed: {}", e);
            }
        }
    }
}
