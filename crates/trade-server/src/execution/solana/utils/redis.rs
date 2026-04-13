use anyhow::{Context, Result};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_sub::redis_tx_retriever::RedisTxRetriever;
use yellowstone_grpc_proto::{
    prelude::TokenBalance, solana::storage::confirmed_block::Transaction,
};

use super::tx::{TokenTransactionError, TransactionDetails};
use crate::execution::OrderType;

/// Extracts transaction details from a Redis-cached transaction
pub async fn extract_token_transaction_details_from_redis(
    redis_tx_retriever: &RedisTxRetriever,
    signature: &Signature,
    mint: &str,
    wallet_pubkey: &str,
    order_type: &OrderType,
) -> Result<TransactionDetails, TokenTransactionError> {
    // Get transaction from Redis
    let tx_with_meta = redis_tx_retriever
        .get_transaction(&signature.to_string())
        .await
        .context("Failed to fetch transaction from Redis")?
        .ok_or_else(|| anyhow::anyhow!("Transaction not found in Redis"))?;

    // Get slot
    let slot = tx_with_meta.slot;

    // Extract transaction info
    let tx_info = tx_with_meta
        .transaction
        .as_ref()
        .context("Transaction info not available")?;

    // Extract metadata
    let meta = tx_info
        .meta
        .as_ref()
        .context("Transaction metadata not available")?;

    // Get token balance changes
    let token_amount = get_token_balance_delta(
        &meta.pre_token_balances,
        &meta.post_token_balances,
        mint,
        wallet_pubkey,
        order_type,
    )?;

    // Ensure necessary balance data is available
    if meta.pre_balances.is_empty() || meta.post_balances.is_empty() {
        return Err(TokenTransactionError::PrePostBalancesNotAvailable);
    }

    // Get the transaction message with account keys
    let transaction = tx_info
        .transaction
        .as_ref()
        .context("Transaction data not available")?;

    let account_keys =
        extract_account_keys(transaction).context("Failed to extract account keys")?;

    // Find wallet index
    let wallet_index = find_wallet_index(&account_keys, wallet_pubkey)?;

    // Get SOL balance delta
    let sol_amount = get_sol_balance_delta(&meta.pre_balances, &meta.post_balances, wallet_index)?;

    // Calculate price
    let price = calculate_price(sol_amount, token_amount);

    tracing::debug!(
        signature = signature.to_string().as_str(),
        mint = mint,
        token_amount,
        sol_amount,
        price,
        "Extracted transaction details from Redis"
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

// Extract account keys from transaction
fn extract_account_keys(transaction: &Transaction) -> Result<Vec<Pubkey>> {
    let message = transaction
        .message
        .as_ref()
        .context("Message not available")?;

    let account_keys: Result<Vec<Pubkey>, anyhow::Error> = message
        .account_keys
        .iter()
        .map(|key_bytes| Pubkey::try_from(&key_bytes[0..32]).context("Invalid public key"))
        .collect();

    account_keys
}

// Find wallet index in account keys
fn find_wallet_index(account_keys: &[Pubkey], wallet_pubkey: &str) -> Result<usize> {
    account_keys
        .iter()
        .position(|key| key.to_string() == wallet_pubkey)
        .context("Wallet pubkey not found in transaction accounts")
}

// Calculate SOL balance delta
fn get_sol_balance_delta(
    pre_balances: &[u64],
    post_balances: &[u64],
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

// Calculate price from SOL and token amounts
fn calculate_price(sol_amount: f64, token_amount: f64) -> f64 {
    if token_amount.abs() > 0.0 {
        // SOL has 9 decimals, and tokens on pumpfun have 6 decimals.
        sol_amount.abs() / token_amount.abs() * 1000.0
    } else {
        0.0
    }
}

// Process token balance changes
fn get_token_balance_delta(
    pre_token_balances: &[TokenBalance],
    post_token_balances: &[TokenBalance],
    mint: &str,
    wallet_pubkey: &str,
    order_type: &OrderType,
) -> Result<f64, TokenTransactionError> {
    let mut pre_token_amount = 0.0;
    let mut post_token_amount = 0.0;

    // Get pre token balances
    for balance in pre_token_balances {
        if balance.mint.to_string() == mint {
            let owner = &balance.owner;
            if owner == wallet_pubkey {
                // Handle buy and sell differently for missing ui_amount
                #[allow(deprecated)]
                let amount = match (
                    balance.ui_token_amount.as_ref().map(|a| a.ui_amount),
                    order_type,
                ) {
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

    // Get post token balances
    for balance in post_token_balances {
        if balance.mint.to_string() == mint {
            let owner = &balance.owner;
            if owner == wallet_pubkey {
                post_token_amount = balance
                    .ui_token_amount
                    .as_ref()
                    .map(|a| a.ui_amount)
                    .unwrap_or(0.0);
                break;
            }
        }
    }

    Ok(post_token_amount - pre_token_amount)
}
