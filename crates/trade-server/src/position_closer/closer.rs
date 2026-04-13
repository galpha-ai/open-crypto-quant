use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentLevel, pubkey::Pubkey};
use tokio::time;
use tracing::{error, info};

use super::traits::{PositionCloser, RpcDataProvider};
use crate::position::PositionManager;
use crate::utils::token_decimal_cache::TokenDecimalCache;

#[derive(Debug, Clone)]
pub struct TokenFilter {
    allowed_suffixes: HashSet<String>,
}

impl TokenFilter {
    pub fn all() -> Self {
        Self {
            allowed_suffixes: HashSet::new(),
        }
    }

    pub fn by_suffixes<'a>(suffixes: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            allowed_suffixes: suffixes.into_iter().map(String::from).collect(),
        }
    }

    pub fn should_process(&self, mint: &str) -> bool {
        self.allowed_suffixes.is_empty()
            || self
                .allowed_suffixes
                .iter()
                .any(|suffix| mint.ends_with(suffix))
    }
}

pub struct SimplePositionCloserBuilder {
    position_manager: Option<Arc<dyn PositionManager>>,
    rpc_data_provider: Option<Arc<dyn RpcDataProvider>>,
    wallet_pubkey: Option<Pubkey>,
    check_interval: Option<Duration>,
    position_age_threshold_slots: Option<u64>,
    rpc_commitment_level: Option<CommitmentLevel>,
    token_filter: Option<TokenFilter>,
    decimal_cache: Option<Arc<TokenDecimalCache>>,
    rpc_client: Option<Arc<RpcClient>>,
}

impl SimplePositionCloserBuilder {
    pub fn new() -> Self {
        Self {
            position_manager: None,
            rpc_data_provider: None,
            wallet_pubkey: None,
            check_interval: None,
            position_age_threshold_slots: None,
            rpc_commitment_level: None,
            token_filter: None,
            decimal_cache: None,
            rpc_client: None,
        }
    }

    pub fn position_manager(mut self, position_manager: Arc<dyn PositionManager>) -> Self {
        self.position_manager = Some(position_manager);
        self
    }

    pub fn rpc_data_provider(mut self, rpc_data_provider: Arc<dyn RpcDataProvider>) -> Self {
        self.rpc_data_provider = Some(rpc_data_provider);
        self
    }

    pub fn wallet_pubkey(mut self, wallet_pubkey: Pubkey) -> Self {
        self.wallet_pubkey = Some(wallet_pubkey);
        self
    }

    pub fn check_interval(mut self, check_interval: Duration) -> Self {
        self.check_interval = Some(check_interval);
        self
    }

    pub fn position_age_threshold_slots(mut self, position_age_threshold_slots: u64) -> Self {
        self.position_age_threshold_slots = Some(position_age_threshold_slots);
        self
    }

    pub fn rpc_commitment_level(mut self, rpc_commitment_level: CommitmentLevel) -> Self {
        self.rpc_commitment_level = Some(rpc_commitment_level);
        self
    }

    pub fn token_filter(mut self, token_filter: TokenFilter) -> Self {
        self.token_filter = Some(token_filter);
        self
    }

    pub fn decimal_cache(mut self, decimal_cache: Arc<TokenDecimalCache>) -> Self {
        self.decimal_cache = Some(decimal_cache);
        self
    }

    pub fn rpc_client(mut self, rpc_client: Arc<RpcClient>) -> Self {
        self.rpc_client = Some(rpc_client);
        self
    }

    pub fn build(self) -> Result<SimplePositionCloser> {
        let position_manager = self
            .position_manager
            .ok_or_else(|| anyhow::anyhow!("position_manager is required"))?;

        let rpc_data_provider = self
            .rpc_data_provider
            .ok_or_else(|| anyhow::anyhow!("rpc_data_provider is required"))?;

        let wallet_pubkey = self
            .wallet_pubkey
            .ok_or_else(|| anyhow::anyhow!("wallet_pubkey is required"))?;

        let check_interval = self
            .check_interval
            .unwrap_or_else(|| Duration::from_secs(3));

        let position_age_threshold_slots = self.position_age_threshold_slots.unwrap_or(5);

        let rpc_commitment_level = self
            .rpc_commitment_level
            .unwrap_or(CommitmentLevel::Confirmed);

        let token_filter = self.token_filter.unwrap_or_else(TokenFilter::all);

        let decimal_cache = self
            .decimal_cache
            .unwrap_or_else(|| Arc::new(TokenDecimalCache::new()));

        let rpc_client = self
            .rpc_client
            .ok_or_else(|| anyhow::anyhow!("rpc_client is required"))?;

        Ok(SimplePositionCloser::new(
            position_manager,
            rpc_data_provider,
            wallet_pubkey,
            check_interval,
            position_age_threshold_slots,
            rpc_commitment_level,
            token_filter,
            decimal_cache,
            rpc_client,
        ))
    }
}

pub struct SimplePositionCloser {
    position_manager: Arc<dyn PositionManager>,
    rpc_data_provider: Arc<dyn RpcDataProvider>,
    wallet_pubkey: Pubkey, // The wallet address to check balances for
    check_interval: Duration,
    position_age_threshold_slots: u64,
    rpc_commitment_level: CommitmentLevel,
    token_filter: TokenFilter,
    decimal_cache: Arc<TokenDecimalCache>,
    rpc_client: Arc<RpcClient>,
}

impl SimplePositionCloser {
    pub fn builder() -> SimplePositionCloserBuilder {
        SimplePositionCloserBuilder::new()
    }

    pub fn new(
        position_manager: Arc<dyn PositionManager>,
        rpc_data_provider: Arc<dyn RpcDataProvider>,
        wallet_pubkey: Pubkey,
        check_interval: Duration,
        position_age_threshold_slots: u64,
        rpc_commitment_level: CommitmentLevel,
        token_filter: TokenFilter,
        decimal_cache: Arc<TokenDecimalCache>,
        rpc_client: Arc<RpcClient>,
    ) -> Self {
        info!(?check_interval, position_age_threshold_slots, ?rpc_commitment_level, %wallet_pubkey, ?token_filter, "Creating SimplePositionCloser");
        Self {
            position_manager,
            rpc_data_provider,
            wallet_pubkey,
            check_interval,
            position_age_threshold_slots,
            rpc_commitment_level,
            token_filter,
            decimal_cache,
            rpc_client,
        }
    }

    async fn perform_ghost_check(&self) -> Result<()> {
        tracing::debug!("Starting ghost position check cycle...");

        // 1. Get Ground Truth Slot
        let current_confirmed_slot = self
            .rpc_data_provider
            .get_latest_slot(self.rpc_commitment_level)
            .await
            .map_err(|e| {
                error!("Failed to get latest confirmed slot: {}", e);
                e
            })?;

        // 2. Get Ground Truth Balances and convert to map for efficient lookup
        let token_balances = self
            .rpc_data_provider
            .get_account_token_balances(&self.wallet_pubkey, self.rpc_commitment_level)
            .await
            .map_err(|e| {
                error!("Failed to get account token balances: {}", e);
                e
            })?;
        let on_chain_balances: std::collections::HashMap<_, _> = token_balances
            .into_iter()
            .map(|info| (info.mint, info.amount))
            .collect();
        tracing::debug!(
            current_confirmed_slot,
            on_chain_balance_count = on_chain_balances.len(),
            "Fetched ground truth state"
        );

        // 3. Get Optimistic State
        let manager_positions = self.position_manager.get_all_open_positions().await;
        tracing::debug!(
            manager_position_count = manager_positions.len(),
            "Fetched optimistic state from PositionManager"
        );

        // 4. Compare and Reconcile (Ghost Positions Only)
        for position in &manager_positions {
            // Check if we should process this token based on the token filter
            if !self.token_filter.should_process(&position.mint) {
                tracing::debug!(mint = position.mint, "Skipping token due to filter setting");
                continue;
            }

            // Calculate position age (for both orphan and normal positions)
            let position_age_slots = if position.entry_slot == 0 {
                // For orphan positions (entry_slot = 0), consider them as old enough
                tracing::info!(
                    mint = position.mint,
                    "Orphan position detected (entry_slot = 0) - proceeding with verification"
                );
                // Use a value greater than threshold to ensure verification
                self.position_age_threshold_slots + 1
            } else {
                // For normal positions, calculate actual age
                current_confirmed_slot.saturating_sub(position.entry_slot)
            };

            // Check if position is old enough to verify
            let should_verify = position_age_slots > self.position_age_threshold_slots;

            if should_verify {
                tracing::debug!(
                    mint = position.mint,
                    entry_slot = position.entry_slot,
                    position_age_slots,
                    threshold = self.position_age_threshold_slots,
                    "Position is old enough for verification"
                );

                // Check if it exists on-chain (using the map)
                let mint_pubkey = match position.mint.parse() {
                    Ok(pk) => pk,
                    Err(e) => {
                        error!(
                            mint = position.mint,
                            error = ?e,
                            "Failed to parse mint string as Pubkey"
                        );
                        continue;
                    }
                };
                if !on_chain_balances.contains_key(&mint_pubkey) {
                    error!(
                        mint = position.mint,
                        entry_slot = position.entry_slot,
                        entry_time = ?position.entry_time,
                        current_confirmed_slot,
                        "Ghost position detected! Position held by manager does not exist on-chain (or has zero balance)."
                    );

                    // Attempt to remove from manager
                    match self.position_manager.remove_position(&position.mint).await {
                        Ok(Some(_removed_pos)) => {
                            info!(
                                mint = position.mint,
                                "Successfully removed ghost position from manager."
                            );
                            // In future: Emit ReconciliationNeeded event here
                        }
                        Ok(None) => {
                            // This might happen if another process/thread removed it between get_all_open_positions and remove_position
                            error!(
                                mint = position.mint,
                                "Attempted to remove ghost position, but it was already gone from manager."
                            );
                        }
                        Err(e) => {
                            error!(mint = position.mint, error = ?e, "Failed to remove ghost position from manager.");
                        }
                    }
                } else {
                    tracing::debug!(mint = position.mint, "Verified position exists on-chain.");
                }
            } else {
                // Position not old enough, skip check
                tracing::debug!(
                    mint = position.mint,
                    entry_slot = position.entry_slot,
                    position_age_slots,
                    threshold = self.position_age_threshold_slots,
                    "Position not old enough for verification yet."
                );
            }
        }

        // 5. Check for orphan positions (on-chain balances not tracked by manager)
        let manager_position_mints: std::collections::HashSet<String> = manager_positions
            .iter()
            .map(|pos| pos.mint.clone())
            .collect();

        for (mint_pubkey, amount) in on_chain_balances {
            // Skip small amounts that might be dust
            if amount <= 1 {
                continue;
            }

            let mint_str = mint_pubkey.to_string();

            // Check if we should process this token based on the token filter
            if !self.token_filter.should_process(&mint_str) {
                tracing::debug!(
                    mint = %mint_str,
                    "Skipping token orphan due to filter setting"
                );
                continue;
            }

            if !manager_position_mints.contains(&mint_str) {
                // Convert amount from base units to standard units using dynamic decimals
                let standard_amount = match self
                    .decimal_cache
                    .base_to_standard(&mint_str, amount, &self.rpc_client)
                    .await
                {
                    Ok(converted_amount) => converted_amount.floor() as u64,
                    Err(e) => {
                        error!(
                            mint = %mint_str,
                            error = ?e,
                            "Failed to convert orphan position amount from base to standard units"
                        );
                        continue;
                    }
                };

                // Skip dust amounts after conversion (less than or equal to 1 standard unit)
                if standard_amount <= 1 {
                    continue;
                }

                // Only log if amount is more than dust
                info!(
                    mint = %mint_str,
                    amount = amount,
                    standard_amount = standard_amount,
                    "Orphan position detected! Position exists on-chain but not tracked by manager."
                );

                // Add to position manager with converted amount
                match self
                    .position_manager
                    .add_orphan_position(mint_str.clone(), standard_amount)
                    .await
                {
                    Ok(()) => {
                        info!(
                            mint = %mint_str,
                            base_amount = amount,
                            standard_amount = standard_amount,
                            "Successfully added orphan position to manager. Will be handled normally."
                        );
                    }
                    Err(e) => {
                        error!(
                            mint = %mint_str,
                            error = ?e,
                            "Failed to add orphan position to manager."
                        );
                    }
                }
            }
        }

        tracing::debug!("Finished ghost position check cycle.");
        Ok(())
    }
}

#[async_trait]
impl PositionCloser for SimplePositionCloser {
    async fn run(&self) -> Result<()> {
        info!("Starting PositionCloser run loop...");
        let mut interval = time::interval(self.check_interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.perform_ghost_check().await {
                // Log error from the check cycle but continue the loop
                error!(
                    err = ?e,
                    "Error during position closer check cycle. Will retry on next interval.",
                );
            }
        }
    }
}
