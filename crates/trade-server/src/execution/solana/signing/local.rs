use anyhow::{Context, Result};
use async_trait::async_trait;
use solana_sdk::{
    message::VersionedMessage,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use tracing::debug;

use super::service::SigningService;

/// Local keypair-based signing service for single-user mode.
///
/// This implementation holds an in-process Keypair and signs transactions locally.
/// It's the simplest and most performant signing method, suitable for:
/// - Single-user deployments
/// - Development and testing
/// - Scenarios where the keypair can be securely stored in memory
pub struct LocalKeypairSigningService {
    keypair: Keypair,
}

impl LocalKeypairSigningService {
    /// Creates a new local signing service with the given keypair
    pub fn new(keypair: Keypair) -> Self {
        debug!(
            wallet_pubkey = %keypair.pubkey(),
            "Created LocalKeypairSigningService"
        );
        Self { keypair }
    }

    /// Returns a reference to the underlying keypair
    ///
    /// This is useful for legacy code that still needs direct keypair access
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }
}

#[async_trait]
impl SigningService for LocalKeypairSigningService {
    fn wallet_pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    async fn sign(&self, message: &VersionedMessage) -> Result<VersionedTransaction> {
        // Sign the message with our keypair
        let transaction = VersionedTransaction::try_new(message.clone(), &[&self.keypair])
            .context("Failed to sign transaction with local keypair")?;

        debug!(
            wallet_pubkey = %self.keypair.pubkey(),
            "Successfully signed transaction locally"
        );

        Ok(transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        hash::Hash,
        message::{MessageHeader, v0},
        signature::Keypair,
    };

    #[tokio::test]
    async fn test_local_signer_wallet_pubkey() {
        let keypair = Keypair::new();
        let expected_pubkey = keypair.pubkey();
        let signer = LocalKeypairSigningService::new(keypair);

        assert_eq!(signer.wallet_pubkey(), expected_pubkey);
    }

    #[tokio::test]
    async fn test_local_signer_sign_transaction() {
        let keypair = Keypair::new();
        let signer = LocalKeypairSigningService::new(keypair);

        // Create a simple test message
        let message = VersionedMessage::V0(v0::Message {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 0,
            },
            account_keys: vec![signer.wallet_pubkey()],
            recent_blockhash: Hash::default(),
            instructions: vec![],
            address_table_lookups: vec![],
        });

        // Sign the message
        let result = signer.sign(&message).await;
        assert!(result.is_ok());

        let signed_tx = result.unwrap();
        assert_eq!(signed_tx.signatures.len(), 1);
        assert_eq!(signed_tx.message, message);
    }
}
