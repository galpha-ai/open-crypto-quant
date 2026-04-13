use anyhow::{Result, anyhow};
use async_trait::async_trait;
use solana_sdk::{message::VersionedMessage, pubkey::Pubkey, transaction::VersionedTransaction};
use std::str::FromStr;
use tracing::debug;

use super::service::SigningService;
use crate::client::UserServiceClient;

/// Remote signing service for multi-user mode.
///
/// This implementation uses UserServiceClient to call the user-service API
/// for transaction signing. It's designed for multi-tenant deployments where:
/// - Users don't want to expose their keypairs to the trading server
/// - Multiple users share the same trading infrastructure
/// - Keypairs are stored in a separate, secure signing service (user-service)
///
/// # Usage
/// Create via `new()` which automatically fetches the wallet address:
/// ```no_run
/// # use trade_server::execution::RemoteSigningService;
/// # async fn example() -> anyhow::Result<()> {
/// let signer = RemoteSigningService::new(
///     "jwt_token".to_string(),
///     "https://api.example.com".to_string(),
///     "solana:mainnet-beta".to_string()
/// ).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Security
/// - JWT tokens are stored in memory only
/// - Tokens should be rotated regularly
/// - The remote signing service (user-service) handles rate-limiting
///
/// # Architecture
/// - Uses UserServiceClient for HTTP communication
/// - Automatically fetches wallet address from user service
/// - Validates signatures from remote service
pub struct RemoteSigningService {
    /// User service client for making signing requests
    user_service_client: UserServiceClient,
    /// Wallet public key for this user
    wallet_pubkey: Pubkey,
}

impl RemoteSigningService {
    /// Creates a new remote signing service by fetching wallet address from user service
    ///
    /// This constructor automatically fetches the wallet address for the given chain_id
    /// from the user service.
    ///
    /// # Arguments
    /// * `jwt_token` - JWT token for authentication
    /// * `user_service_url` - User service base URL (e.g., "https://api.example.com")
    /// * `chain_id` - Chain ID in CAIP-2 format (e.g., "solana:mainnet-beta")
    ///
    /// # Returns
    /// * `Ok(RemoteSigningService)` - Successfully created signing service
    /// * `Err(anyhow::Error)` - If fetching wallet address fails
    pub async fn new(
        jwt_token: String,
        user_service_url: String,
        chain_id: String,
    ) -> Result<Self> {
        let user_service_client =
            UserServiceClient::new(user_service_url, jwt_token, chain_id.clone());

        // Fetch wallet address from user service
        let wallet_address = user_service_client.get_wallet_address(&chain_id).await?;

        let wallet_pubkey = Pubkey::from_str(&wallet_address)
            .map_err(|e| anyhow!("Invalid wallet address from user service: {}", e))?;

        debug!(
            wallet_pubkey = %wallet_pubkey,
            chain_id = %chain_id,
            "Created RemoteSigningService by fetching wallet address from user service"
        );

        Ok(Self {
            user_service_client,
            wallet_pubkey,
        })
    }
}

#[async_trait]
impl SigningService for RemoteSigningService {
    fn wallet_pubkey(&self) -> Pubkey {
        self.wallet_pubkey
    }

    async fn sign(&self, message: &VersionedMessage) -> Result<VersionedTransaction> {
        debug!(
            wallet_pubkey = %self.wallet_pubkey,
            "Requesting remote signature from user-service"
        );

        // Use UserServiceClient to sign the transaction
        let signed_tx = self.user_service_client.sign_transaction(message).await?;

        debug!(
            wallet_pubkey = %self.wallet_pubkey,
            signature = %signed_tx.signatures[0],
            "Successfully received signed transaction from user-service"
        );

        Ok(signed_tx)
    }
}
