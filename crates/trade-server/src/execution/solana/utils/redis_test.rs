use std::{env, str::FromStr};

use anyhow::{Context, Result};

#[cfg(test)]
mod integration_tests {
    use solana_sdk::signature::Signature;
    use solana_sub::redis_tx_retriever::RedisTxRetriever;
    use tokio;
    use tracing::{info, warn};

    use super::*;
    use crate::execution::{
        OrderType, TransactionDetails, extract_token_transaction_details_from_redis,
    };

    // This test is ignored by default and must be run explicitly
    // Run with: cargo test -- --ignored redis_tx_utils_test::integration_tests::test_extract_transaction_details
    #[tokio::test]
    #[ignore]
    async fn test_extract_transaction_details() -> Result<()> {
        // Initialize test logging
        if env::var("RUST_LOG").is_err() {
            unsafe {
                env::set_var("RUST_LOG", "debug");
            }
        }
        let _ = tracing_subscriber::fmt::try_init();

        // Read Redis connection information from environment variables
        let redis_url = env::var("REDIS_URL").unwrap_or("redis://localhost:6379".to_string());
        let key_prefix = env::var("REDIS_KEY_PREFIX").unwrap_or("/transactions/prod".to_string());
        let tx_signature = env::var("TX_SIGNATURE")
            .context("TX_SIGNATURE environment variable is required for this test")?;
        let mint = env::var("TOKEN_MINT")
            .context("TOKEN_MINT environment variable is required for this test")?;
        let wallet_pubkey = env::var("WALLET_PUBKEY")
            .context("WALLET_PUBKEY environment variable is required for this test")?;
        let is_buy = env::var("IS_BUY").unwrap_or("true".to_string()) == "true";

        info!(
            redis_url = %redis_url,
            key_prefix = %key_prefix,
            tx_signature = %tx_signature,
            mint = %mint,
            wallet_pubkey = %wallet_pubkey,
            is_buy = is_buy,
            "Starting Redis transaction extraction integration test"
        );

        // Create the retriever
        let retriever = RedisTxRetriever::new(&redis_url, key_prefix).await?;

        // Create the appropriate order type
        let order_type = if is_buy {
            OrderType::MarketBuy { quote_amount: 0.0 }
        } else {
            OrderType::MarketSell {
                token_amount: 0.0,
                clear_position: false,
            }
        };

        // Try to extract the transaction details
        let tx_details = extract_token_transaction_details_from_redis(
            &retriever,
            &Signature::from_str(&tx_signature)?,
            &mint,
            &wallet_pubkey,
            &order_type,
        )
        .await;

        // Check the result
        match tx_details {
            Ok(details) => {
                // Verify the transaction details
                verify_transaction_details(&details, &tx_signature, &mint, &wallet_pubkey, is_buy)?;
                info!("Successfully extracted and verified transaction details");
                Ok(())
            }
            Err(err) => {
                warn!(
                    signature = %tx_signature,
                    error = %err,
                    "Failed to extract transaction details"
                );
                // We'll consider this a failure of the test
                Err(err.into())
            }
        }
    }

    // Helper function to verify transaction details
    fn verify_transaction_details(
        details: &TransactionDetails,
        expected_signature: &str,
        expected_mint: &str,
        expected_wallet: &str,
        is_buy: bool,
    ) -> Result<()> {
        // Verify signature, mint, and wallet match
        assert_eq!(
            details.signature, expected_signature,
            "Transaction signature does not match"
        );
        assert_eq!(details.mint, expected_mint, "Token mint does not match");
        assert_eq!(
            details.wallet_pubkey, expected_wallet,
            "Wallet pubkey does not match"
        );

        // Verify slot is greater than 0
        assert!(
            details.slot > 0,
            "Transaction slot should be greater than 0"
        );

        // Verify price is greater than 0
        assert!(details.price > 0.0, "Price should be greater than 0");

        // Verify token and SOL amounts based on transaction type
        if is_buy {
            assert!(
                details.token_amount > 0.0,
                "Buy transaction should have positive token amount"
            );
            assert!(
                details.sol_amount < 0.0,
                "Buy transaction should have negative SOL amount (spent)"
            );
        } else {
            assert!(
                details.token_amount < 0.0,
                "Sell transaction should have negative token amount"
            );
            assert!(
                details.sol_amount > 0.0,
                "Sell transaction should have positive SOL amount (received)"
            );
        }

        info!(
            signature = %details.signature,
            mint = %details.mint,
            token_amount = details.token_amount,
            sol_amount = details.sol_amount,
            price = details.price,
            "Transaction details verification successful"
        );

        Ok(())
    }
}
