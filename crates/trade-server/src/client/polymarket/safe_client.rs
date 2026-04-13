//! Safe wallet client for Polymarket CTF operations (split, merge, redeem)

use alloy_network::EthereumWallet;
use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolCall;
use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

use super::abi::{IConditionalTokens, ISafe};
use super::constants::{CTF_ADDRESS, NEG_RISK_ADAPTER_ADDRESS, USDC_ADDRESS, USDC_DECIMALS};
use super::encode::{
    OperationType, SafeTransaction, encode_merge, encode_merge_neg_risk, encode_redeem,
    encode_redeem_neg_risk, encode_split, encode_split_neg_risk,
};

/// Default maximum wait time in seconds
const DEFAULT_MAX_WAIT_SECS: u64 = 120;
/// Poll interval in seconds
const POLL_INTERVAL_SECS: u64 = 2;
/// HTTP client timeout in seconds
const HTTP_TIMEOUT_SECS: u64 = 30;

/// Configuration for the Safe client
#[derive(Debug, Clone)]
pub struct SafeClientConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    pub safe_address: Address,
    /// Maximum time to wait for pending transactions in seconds
    pub max_wait_secs: Option<u64>,
}

/// Safe wallet client for CTF operations
#[derive(Clone)]
pub struct SafeClient {
    config: SafeClientConfig,
    signer: PrivateKeySigner,
    http_client: Client,
}

/// Response from eth_call
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    result: Option<String>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    message: String,
}

/// Response from eth_getTransactionReceipt
#[derive(Debug, Deserialize)]
struct TransactionReceiptResponse {
    result: Option<TransactionReceipt>,
    error: Option<JsonRpcError>,
}

/// Transaction receipt from eth_getTransactionReceipt
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionReceipt {
    /// Block number (hex)
    block_number: Option<String>,
    /// Status: 0x1 for success, 0x0 for failure
    status: Option<String>,
}

/// Result of waiting for a transaction
#[derive(Debug, Clone)]
pub enum TransactionConfirmation {
    /// Transaction was confirmed successfully
    Success { tx_hash: String, block_number: u64 },
    /// Transaction was confirmed but failed (reverted)
    Failed { tx_hash: String, block_number: u64 },
    /// Timeout waiting for confirmation
    Timeout { tx_hash: String },
}

impl TransactionConfirmation {
    /// Returns true if the transaction was confirmed successfully
    pub fn is_success(&self) -> bool {
        matches!(self, TransactionConfirmation::Success { .. })
    }
}

impl SafeClient {
    /// Create a new Safe client
    pub fn new(config: SafeClientConfig, private_key: &str) -> Result<Self> {
        let signer: PrivateKeySigner = private_key
            .parse()
            .map_err(|e| anyhow!("Invalid private key: {}", e))?;

        let http_client = Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()
            .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            config,
            signer,
            http_client,
        })
    }

    /// Get the signer's address
    pub fn signer_address(&self) -> Address {
        self.signer.address()
    }

    /// Get the Safe address
    pub fn safe_address(&self) -> Address {
        self.config.safe_address
    }

    /// Get CTF token balance for the Safe wallet from on-chain
    ///
    /// # Arguments
    /// * `token_id` - The token ID (as decimal string)
    ///
    /// # Returns
    /// Token balance as f64 (converted from 6 decimal places)
    pub async fn get_ctf_balance(&self, token_id: &str) -> Result<f64> {
        let token_id_u256 =
            U256::from_str_radix(token_id, 10).map_err(|e| anyhow!("Invalid token ID: {}", e))?;

        let call = IConditionalTokens::balanceOfCall {
            owner: self.config.safe_address,
            id: token_id_u256,
        };

        let data = Bytes::from(call.abi_encode());
        let result = self.eth_call(CTF_ADDRESS, data).await?;

        if result.len() < 32 {
            return Err(anyhow!("Invalid balance response"));
        }

        let balance = U256::from_be_slice(&result[..32]);
        // Convert from 6 decimals to f64
        Ok(balance.to::<u128>() as f64 / 1_000_000.0)
    }

    /// Make an eth_call to the RPC
    async fn eth_call(&self, to: Address, data: Bytes) -> Result<Bytes> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": format!("{:?}", to),
                "data": format!("0x{}", hex::encode(&data)),
            }, "latest"],
            "id": 1
        });

        let response: JsonRpcResponse = self
            .http_client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        if let Some(error) = response.error {
            return Err(anyhow!("RPC error: {}", error.message));
        }

        let result = response
            .result
            .ok_or_else(|| anyhow!("No result in response"))?;
        let bytes = hex::decode(result.trim_start_matches("0x"))?;
        Ok(Bytes::from(bytes))
    }

    /// Get the current nonce of the Safe
    async fn get_safe_nonce(&self) -> Result<U256> {
        let call = ISafe::nonceCall {};
        let data = Bytes::from(call.abi_encode());
        let result = self.eth_call(self.config.safe_address, data).await?;

        // Decode the result (uint256)
        if result.len() < 32 {
            return Err(anyhow!("Invalid nonce response"));
        }
        let nonce = U256::from_be_slice(&result[..32]);
        Ok(nonce)
    }

    /// Get the transaction hash from the Safe contract
    async fn get_transaction_hash(
        &self,
        tx: &SafeTransaction,
        nonce: U256,
    ) -> Result<FixedBytes<32>> {
        let call = ISafe::getTransactionHashCall {
            to: tx.to,
            value: tx.value,
            data: tx.data.clone(),
            operation: tx.operation as u8,
            safeTxGas: U256::ZERO,
            baseGas: U256::ZERO,
            gasPrice: U256::ZERO,
            gasToken: Address::ZERO,
            refundReceiver: Address::ZERO,
            _nonce: nonce,
        };

        let data = Bytes::from(call.abi_encode());
        let result = self.eth_call(self.config.safe_address, data).await?;

        if result.len() < 32 {
            return Err(anyhow!("Invalid transaction hash response"));
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(FixedBytes::from(hash))
    }

    /// Sign a Safe transaction hash
    fn sign_safe_tx_hash(&self, tx_hash: FixedBytes<32>) -> Result<Bytes> {
        // Sign as ETH signed message (Safe expects this format)
        let signature = self
            .signer
            .sign_message_sync(tx_hash.as_slice())
            .map_err(|e| anyhow!("Failed to sign: {}", e))?;

        // Adjust v value for Safe contract (add 4 for eth_sign)
        let mut sig_bytes = signature.as_bytes().to_vec();
        let v = sig_bytes[64];
        sig_bytes[64] = match v {
            0 | 1 => v + 31,
            27 | 28 => v + 4,
            _ => return Err(anyhow!("Invalid signature v value: {}", v)),
        };

        Ok(Bytes::from(sig_bytes))
    }

    /// Build and send a Safe transaction with automatic retry on nonce errors.
    ///
    /// Each attempt: get latest nonce → sign → send. Retry on GS026/GS013.
    async fn execute_safe_transaction(&self, tx: SafeTransaction) -> Result<String> {
        const MAX_RETRIES: u32 = 10;
        const RETRY_DELAY_MS: u64 = 2000;

        info!(
            target_contract = %format!("{:?}", tx.to),
            value = %tx.value,
            data_len = tx.data.len(),
            operation = ?tx.operation,
            "Executing Safe transaction"
        );

        for attempt in 1..=MAX_RETRIES {
            // Get latest Safe nonce
            let nonce = self.get_safe_nonce().await?;
            info!(
                safe_nonce = %nonce,
                attempt = attempt,
                "Got Safe nonce for transaction"
            );

            // Compute transaction hash and sign
            let tx_hash = self.get_transaction_hash(&tx, nonce).await?;
            let signature = self.sign_safe_tx_hash(tx_hash)?;

            // Build execTransaction call
            let call = ISafe::execTransactionCall {
                to: tx.to.clone(),
                value: tx.value,
                data: tx.data.clone(),
                operation: tx.operation as u8,
                safeTxGas: U256::ZERO,
                baseGas: U256::ZERO,
                gasPrice: U256::ZERO,
                gasToken: Address::ZERO,
                refundReceiver: Address::ZERO,
                signatures: signature,
            };

            let calldata = Bytes::from(call.abi_encode());

            // Try to send
            match self
                .send_transaction(self.config.safe_address, calldata)
                .await
            {
                Ok(tx_hash) => return Ok(tx_hash),
                Err(e) => {
                    let err_str = e.to_string();
                    // Retry on nonce-related errors
                    let is_retryable = err_str.contains("GS013")
                        || err_str.contains("GS026")
                        || err_str.contains("nonce");

                    if is_retryable && attempt < MAX_RETRIES {
                        warn!(
                            error = %e,
                            attempt = attempt,
                            "Safe transaction failed, retrying..."
                        );
                        sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Err(anyhow!(
            "Safe transaction failed after {} retries",
            MAX_RETRIES
        ))
    }

    /// Send a transaction using alloy provider (handles nonce, gas automatically)
    async fn send_transaction(&self, to: Address, data: Bytes) -> Result<String> {
        // Create wallet and provider
        let wallet = EthereumWallet::from(self.signer.clone());
        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http(self.config.rpc_url.parse()?);

        // Build transaction request - provider handles nonce and gas automatically
        let tx = TransactionRequest::default().to(to).input(data.into());

        // Send transaction
        let pending = provider
            .send_transaction(tx)
            .await
            .map_err(|e| anyhow!("Failed to send transaction: {}", e))?;

        let tx_hash = format!("{:?}", pending.tx_hash());
        info!(tx_hash = %tx_hash, "Transaction submitted");
        Ok(tx_hash)
    }
    /// Wait for a transaction to be confirmed on chain
    ///
    /// Polls eth_getTransactionReceipt until the transaction is mined or timeout.
    /// Returns the confirmation status (success, failed, or timeout).
    ///
    /// # Arguments
    /// * `tx_hash` - The transaction hash to wait for
    /// * `max_wait_secs` - Maximum time to wait in seconds (optional, defaults to config)
    pub async fn wait_for_transaction(
        &self,
        tx_hash: &str,
        max_wait_secs: Option<u64>,
    ) -> TransactionConfirmation {
        let max_wait =
            max_wait_secs.unwrap_or(self.config.max_wait_secs.unwrap_or(DEFAULT_MAX_WAIT_SECS));
        let max_iterations = max_wait / POLL_INTERVAL_SECS;

        tracing::info!(
            tx_hash = %tx_hash,
            max_wait_secs = max_wait,
            "Waiting for transaction confirmation"
        );

        for i in 0..max_iterations {
            match self.get_transaction_receipt(tx_hash).await {
                Ok(Some(receipt)) => {
                    let block_number = receipt
                        .block_number
                        .as_ref()
                        .and_then(|b| u64::from_str_radix(b.trim_start_matches("0x"), 16).ok())
                        .unwrap_or(0);

                    // Check status: 0x1 = success, 0x0 = failed
                    let is_success = receipt.status.as_ref().map(|s| s == "0x1").unwrap_or(false);

                    if is_success {
                        tracing::info!(
                            tx_hash = %tx_hash,
                            block_number = block_number,
                            "Transaction confirmed successfully"
                        );
                        return TransactionConfirmation::Success {
                            tx_hash: tx_hash.to_string(),
                            block_number,
                        };
                    } else {
                        tracing::warn!(
                            tx_hash = %tx_hash,
                            block_number = block_number,
                            status = ?receipt.status,
                            "Transaction confirmed but failed (reverted)"
                        );
                        return TransactionConfirmation::Failed {
                            tx_hash: tx_hash.to_string(),
                            block_number,
                        };
                    }
                }
                Ok(None) => {
                    // Transaction not yet mined, continue polling
                    tracing::debug!(
                        tx_hash = %tx_hash,
                        iteration = i + 1,
                        max_iterations = max_iterations,
                        "Transaction not yet mined, waiting..."
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        tx_hash = %tx_hash,
                        error = %e,
                        "Error getting transaction receipt, retrying..."
                    );
                }
            }

            sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        }

        tracing::warn!(
            tx_hash = %tx_hash,
            max_wait_secs = max_wait,
            "Timeout waiting for transaction confirmation"
        );
        TransactionConfirmation::Timeout {
            tx_hash: tx_hash.to_string(),
        }
    }

    /// Get transaction receipt from RPC
    async fn get_transaction_receipt(&self, tx_hash: &str) -> Result<Option<TransactionReceipt>> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getTransactionReceipt",
            "params": [tx_hash],
            "id": 1
        });

        let response: TransactionReceiptResponse = self
            .http_client
            .post(&self.config.rpc_url)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        if let Some(error) = response.error {
            return Err(anyhow!("RPC error: {}", error.message));
        }

        Ok(response.result)
    }

    /// Split USDC into conditional tokens (YES/NO)
    ///
    /// # Arguments
    /// * `condition_id` - The condition ID of the market
    /// * `amount` - Amount in USDC (with 6 decimals)
    /// * `neg_risk` - Whether this is a negRisk market
    pub async fn split(
        &self,
        condition_id: FixedBytes<32>,
        amount: U256,
        neg_risk: bool,
    ) -> Result<String> {
        let amount_usdc = amount.to::<u128>() as f64 / 1_000_000.0;
        info!(
            condition_id = %format!("0x{}", hex::encode(condition_id)),
            amount_raw = %amount,
            amount_usdc = %format!("{:.6}", amount_usdc),
            neg_risk = neg_risk,
            safe_address = %format!("{:?}", self.config.safe_address),
            "Preparing split transaction"
        );

        let (to, data) = if neg_risk {
            info!(
                target_contract = %format!("{:?}", NEG_RISK_ADAPTER_ADDRESS),
                "Using NegRisk adapter for split"
            );
            (
                NEG_RISK_ADAPTER_ADDRESS,
                encode_split_neg_risk(condition_id, amount),
            )
        } else {
            info!(
                target_contract = %format!("{:?}", CTF_ADDRESS),
                collateral = %format!("{:?}", USDC_ADDRESS),
                "Using CTF for split"
            );
            (
                CTF_ADDRESS,
                encode_split(USDC_ADDRESS, condition_id, amount),
            )
        };

        let tx = SafeTransaction {
            to,
            value: U256::ZERO,
            data,
            operation: OperationType::Call,
        };

        self.execute_safe_transaction(tx).await
    }

    /// Split USDC amount (in human-readable format) into conditional tokens
    ///
    /// # Arguments
    /// * `condition_id` - The condition ID of the market (hex string with 0x prefix)
    /// * `amount` - Amount in USDC (e.g., "1.5" for 1.5 USDC)
    /// * `neg_risk` - Whether this is a negRisk market
    pub async fn split_usdc(
        &self,
        condition_id: &str,
        amount: &str,
        neg_risk: bool,
    ) -> Result<String> {
        let condition_id = parse_bytes32(condition_id)?;
        let amount = parse_usdc_amount(amount)?;
        self.split(condition_id, amount, neg_risk).await
    }

    /// Merge conditional tokens back into USDC
    ///
    /// # Arguments
    /// * `condition_id` - The condition ID of the market
    /// * `amount` - Amount to merge (with 6 decimals)
    /// * `neg_risk` - Whether this is a negRisk market
    pub async fn merge(
        &self,
        condition_id: FixedBytes<32>,
        amount: U256,
        neg_risk: bool,
    ) -> Result<String> {
        let amount_usdc = amount.to::<u128>() as f64 / 1_000_000.0;
        info!(
            condition_id = %format!("0x{}", hex::encode(condition_id)),
            amount_raw = %amount,
            amount_usdc = %format!("{:.6}", amount_usdc),
            neg_risk = neg_risk,
            safe_address = %format!("{:?}", self.config.safe_address),
            "Preparing merge transaction"
        );

        let (to, data) = if neg_risk {
            info!(
                target_contract = %format!("{:?}", NEG_RISK_ADAPTER_ADDRESS),
                "Using NegRisk adapter for merge"
            );
            (
                NEG_RISK_ADAPTER_ADDRESS,
                encode_merge_neg_risk(condition_id, amount),
            )
        } else {
            info!(
                target_contract = %format!("{:?}", CTF_ADDRESS),
                collateral = %format!("{:?}", USDC_ADDRESS),
                "Using CTF for merge"
            );
            (
                CTF_ADDRESS,
                encode_merge(USDC_ADDRESS, condition_id, amount),
            )
        };

        let tx = SafeTransaction {
            to,
            value: U256::ZERO,
            data,
            operation: OperationType::Call,
        };

        self.execute_safe_transaction(tx).await
    }

    /// Merge conditional tokens (human-readable amount) back into USDC
    pub async fn merge_usdc(
        &self,
        condition_id: &str,
        amount: &str,
        neg_risk: bool,
    ) -> Result<String> {
        let condition_id = parse_bytes32(condition_id)?;
        let amount = parse_usdc_amount(amount)?;
        self.merge(condition_id, amount, neg_risk).await
    }

    /// Redeem winning conditional tokens for USDC (after market resolution)
    ///
    /// # Arguments
    /// * `condition_id` - The condition ID of the market
    /// * `neg_risk` - Whether this is a negRisk market
    /// * `amounts` - For negRisk markets, the amounts to redeem for each outcome
    pub async fn redeem(
        &self,
        condition_id: FixedBytes<32>,
        neg_risk: bool,
        amounts: Option<Vec<U256>>,
    ) -> Result<String> {
        let (to, data) = if neg_risk {
            let amounts = amounts.ok_or_else(|| anyhow!("Amounts required for negRisk redeem"))?;
            (
                NEG_RISK_ADAPTER_ADDRESS,
                encode_redeem_neg_risk(condition_id, amounts),
            )
        } else {
            (CTF_ADDRESS, encode_redeem(USDC_ADDRESS, condition_id))
        };

        let tx = SafeTransaction {
            to,
            value: U256::ZERO,
            data,
            operation: OperationType::Call,
        };

        self.execute_safe_transaction(tx).await
    }

    /// Redeem with string condition ID
    pub async fn redeem_str(
        &self,
        condition_id: &str,
        neg_risk: bool,
        amounts: Option<Vec<&str>>,
    ) -> Result<String> {
        let condition_id = parse_bytes32(condition_id)?;
        let amounts = amounts
            .map(|a| {
                a.iter()
                    .map(|s| parse_usdc_amount(s))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?;
        self.redeem(condition_id, neg_risk, amounts).await
    }
}

/// Parse a hex string (with or without 0x prefix) into FixedBytes<32>
fn parse_bytes32(s: &str) -> Result<FixedBytes<32>> {
    let s = s.trim_start_matches("0x");
    if s.len() != 64 {
        return Err(anyhow!("Invalid bytes32 length: expected 64 hex chars"));
    }
    let bytes = hex::decode(s)?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(FixedBytes::from(arr))
}

/// Parse a USDC amount string (e.g., "1.5") into U256 with 6 decimals
fn parse_usdc_amount(s: &str) -> Result<U256> {
    let parts: Vec<&str> = s.split('.').collect();
    let (whole, frac) = match parts.len() {
        1 => (parts[0], ""),
        2 => (parts[0], parts[1]),
        _ => return Err(anyhow!("Invalid amount format")),
    };

    let whole: u64 = whole.parse().map_err(|_| anyhow!("Invalid whole part"))?;

    // Pad or truncate fractional part to 6 digits
    let frac = format!("{:0<6}", frac);
    let frac: u64 = frac[..6]
        .parse()
        .map_err(|_| anyhow!("Invalid fractional part"))?;

    let amount = whole * 10u64.pow(USDC_DECIMALS as u32) + frac;
    Ok(U256::from(amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bytes32() {
        let hex = "0x35cc41270f5cdfd59b45e68ab85dc51b0b900d286a6c74ea006b8572d9ff5934";
        let result = parse_bytes32(hex).unwrap();
        assert_eq!(result[0], 0x35);
        assert_eq!(result[31], 0x34);
    }

    #[test]
    fn test_parse_usdc_amount() {
        assert_eq!(parse_usdc_amount("1").unwrap(), U256::from(1_000_000u64));
        assert_eq!(parse_usdc_amount("1.5").unwrap(), U256::from(1_500_000u64));
        assert_eq!(parse_usdc_amount("0.1").unwrap(), U256::from(100_000u64));
        assert_eq!(
            parse_usdc_amount("100").unwrap(),
            U256::from(100_000_000u64)
        );
    }
}
