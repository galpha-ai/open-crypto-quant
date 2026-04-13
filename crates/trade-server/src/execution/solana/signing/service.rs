use anyhow::Result;
use async_trait::async_trait;
use solana_sdk::{message::VersionedMessage, pubkey::Pubkey, transaction::VersionedTransaction};

/// Trait for signing Solana transactions.
///
/// This abstraction allows trade_server to work in both single-user and multi-user modes:
/// - Single-user: LocalKeypairSigningService signs locally with an in-process keypair
/// - Multi-user: RemoteSigningService signs via remote API with JWT authentication
#[async_trait]
pub trait SigningService: Send + Sync {
    /// Returns the wallet public key that this signing service controls
    fn wallet_pubkey(&self) -> Pubkey;

    /// Signs a transaction message and returns a signed VersionedTransaction
    ///
    /// # Arguments
    /// * `message` - The unsigned transaction message to sign
    ///
    /// # Returns
    /// * `Ok(VersionedTransaction)` - Successfully signed transaction
    /// * `Err(anyhow::Error)` - Signing failed (local error or remote API error)
    async fn sign(&self, message: &VersionedMessage) -> Result<VersionedTransaction>;
}
