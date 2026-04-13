use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use serde::Deserialize;
use solana_sdk::{
    message::VersionedMessage, signature::Signature, transaction::VersionedTransaction,
};
use std::str::FromStr;
use tracing::{debug, info, warn};

// Import proto types
use galpha_proto::user_service::http::{
    GetCustodialAddressResponse, WalletCustodialSignRequest, WalletCustodialSignResponse,
};

/// User service client for remote signing operations
///
/// This client communicates with the user-service API to perform
/// transaction signing using custodial wallets.
///
/// Requirements:
/// - JWT token for authentication
/// - User service base URL
/// - Chain ID (CAIP-2 format, e.g., "solana:mainnet-beta")
#[derive(Clone)]
pub struct UserServiceClient {
    /// HTTP client for making requests
    http_client: reqwest::Client,
    /// Base URL of the user service (e.g., "https://api.example.com")
    base_url: String,
    /// JWT access token for authentication
    jwt_token: String,
    /// Chain ID in CAIP-2 format
    chain_id: String,
}

/// API response wrapper
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: T,
    #[allow(dead_code)]
    timestamp: String,
}

impl UserServiceClient {
    /// Creates a new user service client
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the user service (e.g., "https://api.example.com")
    /// * `jwt_token` - JWT access token for authentication
    /// * `chain_id` - Chain ID in CAIP-2 format (e.g., "solana:mainnet-beta", "solana:devnet")
    pub fn new(base_url: String, jwt_token: String, chain_id: String) -> Self {
        info!(
            base_url = %base_url,
            chain_id = %chain_id,
            "Created UserServiceClient"
        );

        Self {
            http_client: reqwest::Client::new(),
            base_url,
            jwt_token,
            chain_id,
        }
    }

    /// Signs a transaction message using the remote user service
    ///
    /// # Arguments
    /// * `message` - The unsigned transaction message
    ///
    /// # Returns
    /// * `Ok(VersionedTransaction)` - The signed transaction
    /// * `Err(anyhow::Error)` - If signing fails
    ///
    /// # Flow
    /// 1. Create an unsigned VersionedTransaction from the message
    /// 2. Serialize the transaction to bytes
    /// 3. Base64 encode the bytes
    /// 4. Send POST request to /wallet/custodial/sign with JWT auth
    /// 5. Parse the response and decode the signed transaction
    pub async fn sign_transaction(
        &self,
        message: &VersionedMessage,
    ) -> Result<VersionedTransaction> {
        // Step 1: Create unsigned VersionedTransaction
        // User service expects VersionedTransaction, not just VersionedMessage
        let unsigned_tx = VersionedTransaction {
            signatures: vec![], // Empty signatures array
            message: message.clone(),
        };

        // Step 2: Serialize transaction to bytes
        let transaction_bytes =
            bincode::serialize(&unsigned_tx).context("Failed to serialize VersionedTransaction")?;

        // Step 3: Base64 encode
        let transaction_bytes_b64 = general_purpose::STANDARD.encode(&transaction_bytes);

        debug!(
            transaction_size = transaction_bytes.len(),
            chain_id = %self.chain_id,
            "Preparing to sign transaction via user service"
        );

        // Step 4: Prepare request using proto type
        let request = WalletCustodialSignRequest {
            transaction_bytes: transaction_bytes_b64,
            chain_id: self.chain_id.clone(),
        };

        let url = format!("{}/wallet/custodial/sign", self.base_url);

        info!(
            url = %url,
            chain_id = %self.chain_id,
            "Sending transaction to user service for signing"
        );

        // Step 5: Send HTTP request
        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.jwt_token))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to user service")?;

        // Step 6: Handle response
        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            warn!(
                status = %status,
                error = %error_body,
                "User service signing request failed"
            );
            return Err(anyhow!(
                "User service signing failed with status {}: {}",
                status,
                error_body
            ));
        }

        // Step 7: Parse response using proto type
        let api_response: ApiResponse<WalletCustodialSignResponse> = response
            .json()
            .await
            .context("Failed to parse user service response")?;

        if !api_response.success {
            return Err(anyhow!("User service returned success=false"));
        }

        debug!(
            signature = %api_response.data.signature,
            "Received signed transaction from user service"
        );

        // Step 8: Decode the signed transaction
        let signed_tx_bytes = general_purpose::STANDARD
            .decode(&api_response.data.signed_transaction)
            .context("Failed to decode signed transaction from base64")?;

        let signed_tx = bincode::deserialize::<VersionedTransaction>(&signed_tx_bytes)
            .context("Failed to deserialize signed transaction")?;

        // Step 9: Verify signature matches
        let signature_from_response = Signature::from_str(&api_response.data.signature)
            .context("Failed to parse signature from response")?;

        if signed_tx.signatures.is_empty() {
            return Err(anyhow!("Signed transaction has no signatures"));
        }

        if signed_tx.signatures[0] != signature_from_response {
            warn!(
                tx_signature = %signed_tx.signatures[0],
                response_signature = %signature_from_response,
                "Signature mismatch between transaction and response"
            );
            return Err(anyhow!("Signature mismatch in user service response"));
        }

        info!(
            signature = %signed_tx.signatures[0],
            "Successfully signed transaction via user service"
        );

        Ok(signed_tx)
    }

    /// Gets the wallet address for a specific chain ID
    ///
    /// # Arguments
    /// * `chain_id` - Chain ID in CAIP-2 format (e.g., "solana:mainnet-beta", "solana:devnet")
    ///
    /// # Returns
    /// * `Ok(String)` - The wallet address for the specified chain
    /// * `Err(anyhow::Error)` - If request fails
    ///
    /// # Flow
    /// 1. Send GET request to /wallet/custodial/address?chain_id=xxx with JWT auth
    /// 2. Parse the response and return the address
    pub async fn get_wallet_address(&self, chain_id: &str) -> Result<String> {
        let url = format!("{}/wallet/custodial/address", self.base_url);

        debug!(
            url = %url,
            chain_id = %chain_id,
            "Fetching wallet address from user service"
        );

        // Send HTTP request
        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.jwt_token))
            .query(&[("chain_id", chain_id)])
            .send()
            .await
            .context("Failed to send request to user service")?;

        // Handle response
        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            warn!(
                status = %status,
                error = %error_body,
                chain_id = %chain_id,
                "User service get wallet address request failed"
            );
            return Err(anyhow!(
                "User service get wallet address failed with status {}: {}",
                status,
                error_body
            ));
        }

        // Parse response
        let api_response: GetCustodialAddressResponse = response
            .json()
            .await
            .context("Failed to parse user service response")?;

        if !api_response.success {
            return Err(anyhow!("User service returned success=false"));
        }

        let custodial_address = api_response
            .data
            .ok_or_else(|| anyhow!("Missing data in response"))?;

        info!(
            address = %custodial_address.address,
            chain_id = %custodial_address.chain_id,
            "Successfully retrieved wallet address from user service"
        );

        Ok(custodial_address.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = UserServiceClient::new(
            "https://api.example.com".to_string(),
            "test_jwt_token".to_string(),
            "solana:mainnet-beta".to_string(),
        );

        assert_eq!(client.base_url, "https://api.example.com");
        assert_eq!(client.jwt_token, "test_jwt_token");
        assert_eq!(client.chain_id, "solana:mainnet-beta");
    }
}
