use anyhow::Result;
use async_trait::async_trait;
use solana_sdk::{commitment_config::CommitmentLevel, pubkey::Pubkey};

#[async_trait]
pub trait PositionCloser: Send + Sync {
    /// Runs the reconciliation loop indefinitely.
    async fn run(&self) -> Result<()>;
}

/// Information about a token account
#[derive(Debug)]
pub struct TokenAccountInfo {
    /// The mint address of the token
    pub mint: Pubkey,
    /// The token balance
    pub amount: u64,
}

#[async_trait]
pub trait RpcDataProvider: Send + Sync {
    /// Gets the latest slot at the specified commitment level
    async fn get_latest_slot(&self, commitment: CommitmentLevel) -> Result<u64>;

    /// Gets token account balances for a given account
    async fn get_account_token_balances(
        &self,
        account_pubkey: &Pubkey,
        commitment: CommitmentLevel,
    ) -> Result<Vec<TokenAccountInfo>>;
}
