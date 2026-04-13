use anyhow::{Context, Result};
use solana_sdk::{message::VersionedMessage, signature::Signature};
use std::sync::Arc;
use tracing::{debug, info};

use super::submitter::TransactionSubmitter;
use crate::execution::solana::signing::SigningService;

/// Composition layer that combines signing and submission.
///
/// This service orchestrates the two-step process of:
/// 1. Signing a transaction message (via SigningService)
/// 2. Submitting the signed transaction (via TransactionSubmitter)
///
/// This separation allows for:
/// - Single-user mode: LocalKeypairSigningService + any submitter
/// - Multi-user mode: RemoteSigningService + any submitter
/// - Flexible submitter strategies (RPC, Jito, BloxRoute, etc.)
pub struct TransactionService {
    /// Service responsible for signing transactions
    signing: Arc<dyn SigningService>,
    /// Service responsible for submitting signed transactions
    submitter: Arc<dyn TransactionSubmitter>,
}

impl TransactionService {
    /// Creates a new TransactionService
    ///
    /// # Arguments
    /// * `signing` - The signing service to use
    /// * `submitter` - The transaction submitter to use
    pub fn new(signing: Arc<dyn SigningService>, submitter: Arc<dyn TransactionSubmitter>) -> Self {
        info!(
            wallet_pubkey = %signing.wallet_pubkey(),
            submitter_type = std::any::type_name::<dyn TransactionSubmitter>(),
            "Created TransactionService"
        );

        Self { signing, submitter }
    }

    /// Signs and submits a transaction in one operation
    ///
    /// # Arguments
    /// * `message` - The unsigned transaction message
    /// * `skip_simulation` - Whether to skip pre-flight simulation
    ///
    /// # Returns
    /// * `Ok(Signature)` - Transaction signature if successful
    /// * `Err(anyhow::Error)` - Signing or submission failed
    ///
    /// # Flow
    /// 1. Sign the message using SigningService
    /// 2. Submit the signed transaction using TransactionSubmitter
    /// 3. Return the transaction signature
    pub async fn submit(
        &self,
        message: &VersionedMessage,
        skip_simulation: bool,
    ) -> Result<Signature> {
        debug!(
            wallet_pubkey = %self.signing.wallet_pubkey(),
            skip_simulation,
            "Submitting transaction"
        );

        // Step 1: Sign the transaction
        let signed_tx = self
            .signing
            .sign(message)
            .await
            .context("Failed to sign transaction")?;

        debug!(
            signature = %signed_tx.signatures[0],
            "Transaction signed successfully"
        );

        // Step 2: Submit the signed transaction
        let signature = self
            .submitter
            .submit_transaction(&signed_tx, skip_simulation)
            .await
            .context("Failed to submit transaction")?;

        info!(
            wallet_pubkey = %self.signing.wallet_pubkey(),
            signature = %signature,
            "Transaction submitted successfully"
        );

        Ok(signature)
    }

    /// Confirms a transaction
    ///
    /// This is a convenience wrapper around the submitter's confirm method.
    ///
    /// # Arguments
    /// * `signature` - The transaction signature to confirm
    ///
    /// # Returns
    /// * `Ok(())` - Transaction confirmed
    /// * `Err(anyhow::Error)` - Confirmation failed or timeout
    pub async fn confirm_transaction(&self, signature: Signature) -> Result<()> {
        self.submitter
            .confirm_transaction(signature)
            .await
            .context("Failed to confirm transaction")
    }

    /// Returns the wallet public key being used for signing
    pub fn wallet_pubkey(&self) -> solana_sdk::pubkey::Pubkey {
        self.signing.wallet_pubkey()
    }

    /// Returns a reference to the signing service (for advanced use cases)
    pub fn signing_service(&self) -> &Arc<dyn SigningService> {
        &self.signing
    }

    /// Returns a reference to the submitter (for advanced use cases)
    pub fn submitter(&self) -> &Arc<dyn TransactionSubmitter> {
        &self.submitter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{LocalKeypairSigningService, MockTransactionSubmitter};
    use solana_sdk::{
        hash::Hash,
        message::{MessageHeader, v0},
        signature::Keypair,
        signer::Signer,
    };

    #[tokio::test]
    async fn test_transaction_service_creation() {
        let keypair = Keypair::new();
        let signer = Arc::new(LocalKeypairSigningService::new(keypair));
        let submitter = Arc::new(MockTransactionSubmitter::new());

        let service = TransactionService::new(signer.clone(), submitter);
        assert_eq!(service.wallet_pubkey(), signer.wallet_pubkey());
    }

    #[tokio::test]
    async fn test_transaction_service_submit() {
        let keypair = Keypair::new();
        let wallet_pubkey = keypair.pubkey();
        let signer = Arc::new(LocalKeypairSigningService::new(keypair));
        let submitter = Arc::new(MockTransactionSubmitter::new());

        let service = TransactionService::new(signer, submitter);

        // Create a simple test message
        let message = VersionedMessage::V0(v0::Message {
            header: MessageHeader {
                num_required_signatures: 1,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 0,
            },
            account_keys: vec![wallet_pubkey],
            recent_blockhash: Hash::default(),
            instructions: vec![],
            address_table_lookups: vec![],
        });

        // Submit the transaction
        let result = service.submit(&message, false).await;
        assert!(result.is_ok());

        let signature = result.unwrap();
        assert_ne!(signature, Signature::default());
    }
}
