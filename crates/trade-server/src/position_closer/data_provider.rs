use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use solana_account_decoder_client_types::{
    ParsedAccount, UiAccount, UiAccountData, UiAccountEncoding,
};
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_request::TokenAccountsFilter};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    program_pack::Pack,
    pubkey::Pubkey,
};
use spl_token::state::{Account as TokenAccount, AccountState};
use tracing::{debug, error, instrument, warn};

use super::traits::{RpcDataProvider, TokenAccountInfo};

/// Reason for skipping an account during processing.
#[derive(Debug)]
enum SkipReason {
    UnsupportedEncoding,
    DecodingError,
    NonSplTokenJson,
    NotAccountTypeJson,
    MissingFieldJson, // Indicates a field was expected but missing in JSON
    ParseErrorJson,   // General JSON parsing/format error
    InvalidStateJson, // Account state in JSON was not 'initialized'
    UnpackError,      // Failed to unpack binary data using spl_token::state::Account::unpack
    WrongDataLength,
    ZeroBalance,
    NotInitialized,
}

/// An implementation of `RpcDataProvider` that uses a Solana `RpcClient`.
pub struct SolanaRpcDataProvider {
    rpc_client: Arc<RpcClient>,
}

impl SolanaRpcDataProvider {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self { rpc_client }
    }
}

#[async_trait]
impl RpcDataProvider for SolanaRpcDataProvider {
    /// Gets the latest slot using `get_slot_with_commitment`.
    async fn get_latest_slot(&self, commitment: CommitmentLevel) -> Result<u64> {
        self.rpc_client
            .get_slot_with_commitment(CommitmentConfig { commitment })
            .await
            .context("Failed to get latest slot from RPC")
    }

    async fn get_account_token_balances(
        &self,
        account_pubkey: &Pubkey,
        commitment: CommitmentLevel,
    ) -> Result<Vec<TokenAccountInfo>> {
        debug!(%account_pubkey, ?commitment, "Fetching token accounts by owner");
        let commitment_config = CommitmentConfig { commitment };

        // Fetch accounts
        let response = self
            .rpc_client
            .get_token_accounts_by_owner_with_commitment(
                account_pubkey,
                TokenAccountsFilter::ProgramId(spl_token::id()), // Only SPL Token accounts
                commitment_config,
            )
            .await
            .context("Failed to get token accounts by owner from RPC")?;

        let mut balances = Vec::new();
        for keyed_account in response.value {
            // Parse the pubkey string from RpcKeyedAccount
            let token_account_pubkey = match keyed_account.pubkey.parse::<Pubkey>() {
                Ok(pk) => pk,
                Err(e) => {
                    error!(pubkey_str = %keyed_account.pubkey, error = %e, "Failed to parse token account pubkey string, skipping");
                    continue; // Skip this account
                }
            };
            process_rpc_account(
                &token_account_pubkey,
                keyed_account.account,
                account_pubkey,
                &mut balances,
            );
        }

        tracing::debug!(count = balances.len(), %account_pubkey, "Finished processing token balances");
        Ok(balances)
    }
}

/// Decodes binary account data based on the encoding.
fn decode_binary_data(
    encoded_data: &str,
    encoding: UiAccountEncoding,
    pubkey: &Pubkey,
) -> Result<Vec<u8>, SkipReason> {
    match encoding {
        UiAccountEncoding::Base64 => base64::engine::general_purpose::STANDARD
            .decode(encoded_data)
            .map_err(|e| {
                error!(%pubkey, ?encoding, error = %e, "Base64 decode error");
                SkipReason::DecodingError
            }),
        UiAccountEncoding::Base58 => bs58::decode(encoded_data).into_vec().map_err(|e| {
            error!(%pubkey, ?encoding, error = %e, "Base58 decode error");
            SkipReason::DecodingError
        }),
        _ => {
            error!(%pubkey, ?encoding, "Unsupported binary account data encoding encountered");
            Err(SkipReason::UnsupportedEncoding)
        }
    }
}

/// Unpacks token account data from bytes and filters based on state and balance.
#[instrument(level = "debug", skip(data_bytes), fields(pubkey=%account_pubkey))]
fn unpack_token_account(
    data_bytes: Vec<u8>,
    account_pubkey: &Pubkey,
    owner_pubkey: &Pubkey,
) -> Result<TokenAccountInfo, SkipReason> {
    if data_bytes.len() != TokenAccount::LEN {
        error!(
            data_len = data_bytes.len(),
            expected_len = TokenAccount::LEN,
            "Decoded account data length mismatch for SPL Token account"
        );
        return Err(SkipReason::WrongDataLength);
    }

    match TokenAccount::unpack(&data_bytes) {
        Ok(token_account) => {
            if token_account.state != AccountState::Initialized {
                debug!(mint = %token_account.mint, owner = %owner_pubkey, state = ?token_account.state, "Skipping token account (not initialized)");
                Err(SkipReason::NotInitialized)
            } else if token_account.amount == 0 {
                debug!(mint = %token_account.mint, owner = %owner_pubkey, "Skipping token account with zero balance");
                Err(SkipReason::ZeroBalance)
            } else {
                Ok(TokenAccountInfo {
                    mint: token_account.mint,
                    amount: token_account.amount,
                })
            }
        }
        Err(e) => {
            warn!(error = ?e, "Failed to unpack token account data from decoded bytes");
            Err(SkipReason::UnpackError)
        }
    }
}

/// Parses JSON token account data and filters based on state and balance.
#[instrument(level = "debug", skip(json_data), fields(pubkey=%account_pubkey))]
fn parse_json_token_account_data(
    json_data: ParsedAccount,
    account_pubkey: &Pubkey,
    owner_pubkey: &Pubkey,
) -> Result<TokenAccountInfo, SkipReason> {
    if json_data.program != "spl-token" {
        debug!(program = %json_data.program, "Skipping non-SPL-token JSON account");
        return Err(SkipReason::NonSplTokenJson);
    }

    let parsed_data = match json_data.parsed {
        serde_json::Value::Object(map) => map,
        _ => {
            error!("Parsed JSON data is not an object");
            return Err(SkipReason::ParseErrorJson);
        }
    };

    if parsed_data.get("type").and_then(|v| v.as_str()) != Some("account") {
        debug!("Skipping JSON account - not of type 'account'");
        return Err(SkipReason::NotAccountTypeJson);
    }

    let info_val = parsed_data
        .get("info")
        .ok_or(SkipReason::MissingFieldJson)?;

    let mint_str = info_val
        .get("mint")
        .and_then(|v| v.as_str())
        .ok_or(SkipReason::MissingFieldJson)?;
    let amount_str = info_val
        .get("tokenAmount")
        .and_then(|a| a.get("amount"))
        .and_then(|v| v.as_str())
        .ok_or(SkipReason::MissingFieldJson)?;
    let state_str = info_val
        .get("state")
        .and_then(|v| v.as_str())
        .ok_or(SkipReason::MissingFieldJson)?;

    let mint = mint_str
        .parse::<Pubkey>()
        .map_err(|_| SkipReason::ParseErrorJson)?; // Error logged below if needed
    let amount = amount_str
        .parse::<u64>()
        .map_err(|_| SkipReason::ParseErrorJson)?; // Error logged below if needed

    if state_str != "initialized" {
        debug!(%mint, owner = %owner_pubkey, state=%state_str, "Skipping token account (from JSON) (not initialized)");
        Err(SkipReason::InvalidStateJson)
    } else if amount == 0 {
        debug!(%mint, owner = %owner_pubkey, "Skipping token account (from JSON) with zero balance");
        Err(SkipReason::ZeroBalance)
    } else {
        Ok(TokenAccountInfo { mint, amount })
    }
}

/// Processes a single RPC account response, handling different encodings and parsing.
#[instrument(level = "debug", skip(account_data, balances), fields(pubkey=%token_account_pubkey))]
fn process_rpc_account(
    token_account_pubkey: &Pubkey, // The pubkey of the token account being processed
    account_data: UiAccount,       // The account data itself
    owner_pubkey: &Pubkey,         // Owner pubkey for context in logs
    balances: &mut Vec<TokenAccountInfo>,
) {
    let parse_result: Result<TokenAccountInfo, SkipReason> = match account_data.data {
        UiAccountData::Binary(encoded_data, encoding) => {
            match decode_binary_data(&encoded_data, encoding, token_account_pubkey) {
                Ok(bytes) => unpack_token_account(bytes, token_account_pubkey, owner_pubkey),
                Err(reason) => Err(reason),
            }
        }
        UiAccountData::Json(json_data) => {
            parse_json_token_account_data(json_data, token_account_pubkey, owner_pubkey)
        }
        UiAccountData::LegacyBinary(encoded_data) => {
            // Treat LegacyBinary as Base58
            match decode_binary_data(
                &encoded_data,
                UiAccountEncoding::Base58,
                token_account_pubkey,
            ) {
                Ok(bytes) => unpack_token_account(bytes, token_account_pubkey, owner_pubkey),
                Err(reason) => Err(reason),
            }
        }
    };

    match parse_result {
        Ok(info) => {
            // Filter out accounts with exactly 1 token (considered dust/leftover)
            // Zero balance accounts are already filtered by unpack/parse functions.
            if info.amount == 1 {
                debug!(mint = %info.mint, owner = %owner_pubkey, amount = info.amount, "Skipping token account with balance of 1 (dust)");
            } else {
                balances.push(info);
            }
        }
        Err(reason) => {
            // Log the skip reason if it hasn't been logged already by sub-functions at error level
            match reason {
                SkipReason::ZeroBalance
                | SkipReason::NotInitialized
                | SkipReason::NonSplTokenJson
                | SkipReason::NotAccountTypeJson
                | SkipReason::InvalidStateJson => {
                    // These are logged at debug level within parsing functions
                }
                _ => {
                    warn!(reason=?reason, "Skipping account due to processing error");
                } // ParseErrorJson, InvalidStateJson, UnpackError fall here if triggered
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, str::FromStr, sync::Arc};

    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_sdk::{commitment_config::CommitmentLevel, pubkey::Pubkey};
    use tracing_test::traced_test; // Use tracing_test for capturing logs

    use super::{RpcDataProvider, SolanaRpcDataProvider};

    const TEST_RPC_ENDPOINT_ENV: &str = "TEST_RPC_ENDPOINT";
    const TEST_ACCOUNT_PUBKEY_ENV: &str = "TEST_ACCOUNT_PUBKEY";

    #[tokio::test]
    #[ignore] // Skip by default, run with `cargo test -- --ignored` or enable feature
    #[traced_test] // Initialize tracing for this test
    async fn test_fetch_token_accounts_integration() {
        let rpc_url = env::var(TEST_RPC_ENDPOINT_ENV);
        let account_pk_str = env::var(TEST_ACCOUNT_PUBKEY_ENV);

        if rpc_url.is_err() || account_pk_str.is_err() {
            println!(
                "Skipping integration test: Set {} and {} environment variables.",
                TEST_RPC_ENDPOINT_ENV, TEST_ACCOUNT_PUBKEY_ENV
            );
            return;
        }

        let rpc_url = rpc_url.unwrap();
        let account_pk =
            Pubkey::from_str(&account_pk_str.unwrap()).expect("Invalid public key in env var");

        println!("Using RPC Endpoint: {}", rpc_url);
        println!("Fetching token accounts for: {}", account_pk);

        let rpc_client = Arc::new(RpcClient::new(rpc_url));
        let data_provider = SolanaRpcDataProvider::new(rpc_client);

        let result = data_provider
            .get_account_token_balances(&account_pk, CommitmentLevel::Confirmed)
            .await;

        println!("Result: {:?}", result);
        assert!(result.is_ok(), "RPC call failed: {:?}", result.err()); // Basic check that the call succeeded
    }
}
