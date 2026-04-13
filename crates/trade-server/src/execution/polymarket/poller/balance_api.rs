//! Balance API utilities for querying and refreshing CLOB balances.
//!
//! This module provides functions for interacting with the Polymarket CLOB
//! balance APIs.

use anyhow::Result;
use polyfill_rs::ClobClient;
use polyfill_rs::types::{AssetType, BalanceAllowanceParams};
use tracing::{debug, info, warn};

/// Get USDC balance from the CLOB API.
///
/// # Arguments
/// * `client` - CLOB client for API calls
///
/// # Returns
/// USDC balance as f64 (converted from 6 decimal places)
pub async fn get_usdc_balance(client: &ClobClient) -> Result<f64> {
    let params = BalanceAllowanceParams {
        asset_type: Some(AssetType::COLLATERAL),
        token_id: None,
        signature_type: None,
    };

    let balance = client.get_balance_allowance(Some(params)).await?;

    // Balance is returned as a string in the smallest unit (6 decimals for USDC)
    let balance_raw = balance
        .get("balance")
        .and_then(|b| b.as_str())
        .and_then(|b| b.parse::<u64>().ok())
        .unwrap_or(0);

    // Convert from 6 decimal places to f64
    Ok(balance_raw as f64 / 1_000_000.0)
}

/// Get token balance from the CLOB API.
///
/// # Arguments
/// * `client` - CLOB client for API calls
/// * `token_id` - Token/asset ID to query
///
/// # Returns
/// Token balance as f64 (converted from 6 decimal places)
pub async fn get_token_balance(client: &ClobClient, token_id: &str) -> Result<f64> {
    let params = BalanceAllowanceParams {
        asset_type: Some(AssetType::CONDITIONAL),
        token_id: Some(token_id.to_string()),
        signature_type: None,
    };

    let balance = client.get_balance_allowance(Some(params)).await?;

    // Balance is returned as a string in the smallest unit (6 decimals)
    let balance_raw = balance
        .get("balance")
        .and_then(|b| b.as_str())
        .and_then(|b| b.parse::<u64>().ok())
        .unwrap_or(0);

    // Convert from 6 decimal places to f64
    Ok(balance_raw as f64 / 1_000_000.0)
}

/// Refresh CLOB balance by calling updateBalanceAllowance API.
///
/// This forces the CLOB to recognize on-chain balance changes (e.g., after merge).
/// The CLOB caches balance state, so this is necessary after on-chain operations.
///
/// # Arguments
/// * `client` - CLOB client for API calls
pub async fn refresh_clob_balance(client: &ClobClient) -> Result<()> {
    debug!("Refreshing CLOB collateral balance");

    let params = BalanceAllowanceParams {
        asset_type: Some(AssetType::COLLATERAL),
        token_id: None,
        signature_type: None,
    };

    match client.update_balance_allowance(Some(params)).await {
        Ok(_) => {
            info!("CLOB collateral balance refreshed successfully");
            Ok(())
        }
        Err(e) => {
            warn!(error = %e, "Failed to refresh CLOB balance");
            Err(anyhow::anyhow!("Failed to refresh CLOB balance: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would require a real ClobClient
    // Unit tests for balance parsing logic are covered in the main poller tests
}
