use std::str::FromStr;

use anyhow::{Context, Result};
use serde::Serialize;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcTransactionConfig};
use solana_transaction_status_client_types::UiTransactionTokenBalance;
use thiserror::Error;

use crate::execution::OrderType;

#[derive(Debug, Error)]
pub enum TokenTransactionError {
    #[error("Token balance not available, position likely already cleared")]
    PositionAlreadyCleared,
    #[error("Not enough tokens to sell")]
    InsufficientPosition,
    #[error("Pre/post balances not available in transaction metadata")]
    PrePostBalancesNotAvailable,
    #[error("Unsupported transaction encoding")]
    UnsupportedTransactionEncoding,
    #[error("Other transaction error: {0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Serialize)]
pub struct TransactionDetails {
    pub token_amount: f64,
    pub sol_amount: f64,
    pub price: f64,
    pub wallet_pubkey: String,
    pub mint: String,
    pub signature: String,
    pub slot: u64,
}
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status_client_types::{
    UiTransactionEncoding, option_serializer::OptionSerializer,
};

#[allow(deprecated)]
fn get_token_balance_delta(
    pre_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
    post_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
    mint: &str,
    wallet_pubkey: &str,
    order_type: &OrderType,
) -> Result<f64, TokenTransactionError> {
    let mut pre_token_amount = 0.0;
    let mut post_token_amount = 0.0;

    // Get pre token balances
    if let OptionSerializer::Some(pre_token_balances) = pre_token_balances {
        for balance in pre_token_balances {
            if balance.mint == mint {
                let owner = balance.owner.as_ref().map(|s| s.as_str()).unwrap_or("");
                if owner == wallet_pubkey {
                    // Handle buy and sell differently for missing ui_amount
                    #[allow(deprecated)]
                    let amount = match (balance.ui_token_amount.ui_amount, order_type) {
                        (Some(amount), _) => amount,             // Normal case - amount exists
                        (None, _) if order_type.is_buy() => 0.0, // For buy, missing amount is treated as 0
                        (None, _) if order_type.is_sell() => {
                            // For sell, missing amount is an error
                            return Err(TokenTransactionError::PositionAlreadyCleared);
                        }
                        (None, _) => 0.0, // Default for any other order type
                    };
                    pre_token_amount = amount;
                    break;
                }
            }
        }
    }

    // Get post token balances
    if let OptionSerializer::Some(post_token_balances) = post_token_balances {
        for balance in post_token_balances {
            if balance.mint == mint {
                let owner = balance.owner.as_ref().map(|s| s.as_str()).unwrap_or("");
                if owner == wallet_pubkey {
                    post_token_amount = balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    break;
                }
            }
        }
    }

    Ok(post_token_amount - pre_token_amount)
}

fn get_sol_balance_delta(
    pre_balances: Vec<u64>,
    post_balances: Vec<u64>,
    wallet_index: usize,
) -> Result<f64> {
    // Ensure wallet index is within bounds
    if wallet_index >= pre_balances.len() || wallet_index >= post_balances.len() {
        return Err(anyhow::anyhow!(
            "Wallet index out of bounds in balance arrays"
        ));
    }

    let pre_sol = pre_balances[wallet_index] as f64 / 1_000_000_000.0;
    let post_sol = post_balances[wallet_index] as f64 / 1_000_000_000.0;
    Ok(post_sol - pre_sol)
}

fn find_wallet_index(
    account_keys: &[solana_sdk::pubkey::Pubkey],
    wallet_pubkey: &str,
) -> Result<usize> {
    account_keys
        .iter()
        .position(|key| key.to_string() == wallet_pubkey)
        .context("Wallet pubkey not found in transaction accounts")
}

fn calculate_price(sol_amount: f64, token_amount: f64) -> f64 {
    if token_amount.abs() > 0.0 {
        // SOL has 9 decimals, and tokens on pumpfun have 6 decimals.
        sol_amount.abs() / token_amount.abs() * 1000.0
    } else {
        0.0
    }
}

/// Extracts transaction details including token and SOL amounts
pub async fn extract_token_transaction_details(
    rpc_client: &RpcClient,
    signature: &Signature,
    mint: &str,
    wallet_pubkey: &str,
    order_type: &OrderType,
) -> Result<TransactionDetails, TokenTransactionError> {
    let config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let tx_with_meta = rpc_client
        .get_transaction_with_config(&signature, config)
        .context("Failed to fetch transaction details")?;

    // Find slot
    let slot = tx_with_meta.slot;

    let meta = tx_with_meta
        .transaction
        .meta
        .context("Transaction metadata not available")?;

    let token_amount = get_token_balance_delta(
        meta.pre_token_balances,
        meta.post_token_balances,
        mint,
        wallet_pubkey,
        order_type,
    )?;

    // Ensure necessary balance data is available
    if meta.pre_balances.is_empty() || meta.post_balances.is_empty() {
        return Err(TokenTransactionError::PrePostBalancesNotAvailable);
    }

    // Get the decoded transaction
    let account_keys = match &tx_with_meta.transaction.transaction {
        solana_transaction_status_client_types::EncodedTransaction::Json(ui_tx) => {
            match &ui_tx.message {
                solana_transaction_status_client_types::UiMessage::Raw(raw_msg) => {
                    let keys: Result<Vec<Pubkey>, _> = raw_msg
                        .account_keys
                        .iter()
                        .map(|acct| Pubkey::from_str(&acct))
                        .collect();

                    keys.context("Failed to parse account keys")?
                }
                solana_transaction_status_client_types::UiMessage::Parsed(parsed_msg) => {
                    let keys: Result<Vec<Pubkey>, _> = parsed_msg
                        .account_keys
                        .iter()
                        .map(|acct| Pubkey::from_str(&acct.pubkey))
                        .collect();

                    keys.context("Failed to parse account keys")?
                }
            }
        }
        _ => return Err(TokenTransactionError::UnsupportedTransactionEncoding),
    };

    // Find wallet index
    let wallet_index = find_wallet_index(&account_keys, wallet_pubkey)?;

    // Get SOL balance delta
    let sol_amount = get_sol_balance_delta(meta.pre_balances, meta.post_balances, wallet_index)?;

    // Calculate price
    let price = calculate_price(sol_amount, token_amount);

    tracing::debug!(
        signature = signature.to_string().as_str(),
        mint = mint,
        token_amount,
        sol_amount,
        price,
        "Extracted transaction details"
    );

    Ok(TransactionDetails {
        token_amount,
        sol_amount,
        price,
        wallet_pubkey: wallet_pubkey.to_string(),
        mint: mint.to_string(),
        signature: signature.to_string(),
        slot,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use solana_client::rpc_client::RpcClient;
    use solana_sdk::signature::Signature;

    use crate::execution::{OrderType, extract_token_transaction_details};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore] // This test is ignored by default, run with --ignored flag
    async fn test_extract_sell_transaction_details() {
        let rpc_url =
            std::env::var("HELIUS_RPC_URL").expect("HELIUS_RPC_URL must be set for integration tests").as_str();
        let rpc_client = RpcClient::new(rpc_url.to_string());

        let sell_tx_signature = "3YRafSwHoTH7EZXKmRwMHrJjXcREypSvAoATomi6ibLCHWGKKXCSm2fDdtwpqin1f48FPX2M7MpQhK4yWArJjLwV";
        let sell_mint = "2aUSdZLQ27erVWMtgwzL64LLwY78vMxPQHYE2WJdpump";
        let sell_wallet_pubkey = "DiQyRaHXBJB2AtRsT9XjZHca3soozfCqPoxAXbyWWQPd";

        let sell_details = extract_token_transaction_details(
            &rpc_client,
            &Signature::from_str(sell_tx_signature).unwrap(),
            sell_mint,
            sell_wallet_pubkey,
            &OrderType::MarketSell {
                token_amount: 0.0,
                clear_position: false,
            },
        )
        .await
        .unwrap();

        println!("Sell transaction details: {:#?}", sell_details);
        assert!(
            sell_details.token_amount < 0.0,
            "Sell should have negative token amount"
        );
        assert!(
            sell_details.sol_amount > 0.0,
            "Sell should have positive SOL amount (received)"
        );
        assert!(sell_details.price > 0.0, "Price should be positive");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore] // This test is ignored by default, run with --ignored flag
    async fn test_extract_buy_transaction_details() {
        let rpc_url =
            std::env::var("HELIUS_RPC_URL").expect("HELIUS_RPC_URL must be set for integration tests").as_str();
        let rpc_client = RpcClient::new(rpc_url.to_string());

        let buy_tx_signature = "4iV7zEobGn7pG1b8NLiw2ETHpRVHxUK2cP2vig4iTwB1RdHcggpuwpZjTg4DB74V5Ee8gYkdmmdwtcY1wxLvsf58";
        let buy_mint = "Aq2ExPe5E8CZce4sLsJk4dJcDewYyoEJFBjh427Zpump";
        let buy_wallet_pubkey = "7eATLQaK7ZWQ4LCGzhbAdYLdvAYK2XpxeEevemVbLmFG";

        let buy_details = extract_token_transaction_details(
            &rpc_client,
            &Signature::from_str(buy_tx_signature).unwrap(),
            buy_mint,
            buy_wallet_pubkey,
            &OrderType::MarketBuy { quote_amount: 0.0 }, // use dummy quote amount
        )
        .await
        .unwrap();

        println!("Buy transaction details: {:#?}", buy_details);
        assert!(
            buy_details.token_amount > 0.0,
            "Buy should have positive token amount"
        );
        assert!(
            buy_details.sol_amount < 0.0,
            "Buy should have negative SOL amount (spent)"
        );
        assert!(buy_details.price > 0.0, "Price should be positive");
    }
}
