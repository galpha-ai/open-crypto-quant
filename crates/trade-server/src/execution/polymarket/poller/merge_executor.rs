//! Merge executor for handling redemption operations.
//!
//! This module provides a dedicated executor for merge operations,
//! separating the concern from order polling.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use polyfill_rs::ClobClient;
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, info, warn};

use crate::client::polymarket::{SafeClient, TransactionConfirmation};
use crate::domain::SystemEvent;
use crate::event_coordinator::EventCoordinator;
use crate::execution::events::RedemptionEvent;
use crate::signal::RedemptionAction;

use super::balance_api::refresh_clob_balance;
use super::data_api::wait_for_merge_indexed;
use super::merge_config::MergeConfig;
use super::merge_metrics::MergeMetrics;

/// Executor for merge (redemption) operations.
///
/// Manages a queue of pending merge requests and executes them on a
/// configurable interval. Handles the full merge lifecycle:
/// 1. Query on-chain quantities
/// 2. Execute merge via Safe wallet
/// 3. Wait for on-chain confirmation
/// 4. Wait for CLOB indexing
/// 5. Refresh CLOB balance
/// 6. Emit redemption event
pub struct MergeExecutor {
    /// Safe wallet client for merge transactions
    safe_client: SafeClient,

    /// Event coordinator for enqueueing redemption events
    event_coordinator: Arc<dyn EventCoordinator>,

    /// CLOB client for balance operations
    clob_client: Arc<ClobClient>,

    /// Pending merge requests (market -> RedemptionAction)
    /// Only keeps the latest request per market
    pending_merges: RwLock<HashMap<String, RedemptionAction>>,

    /// Configuration
    config: MergeConfig,

    /// Metrics collector
    metrics: MergeMetrics,

    /// HTTP client for Data API calls
    http_client: reqwest::Client,
}

impl MergeExecutor {
    /// Create a new merge executor.
    pub fn new(
        safe_client: SafeClient,
        event_coordinator: Arc<dyn EventCoordinator>,
        clob_client: Arc<ClobClient>,
        config: MergeConfig,
    ) -> Self {
        Self {
            safe_client,
            event_coordinator,
            clob_client,
            pending_merges: RwLock::new(HashMap::new()),
            config,
            metrics: MergeMetrics::default(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Create a new merge executor with custom metrics.
    pub fn with_metrics(
        safe_client: SafeClient,
        event_coordinator: Arc<dyn EventCoordinator>,
        clob_client: Arc<ClobClient>,
        config: MergeConfig,
        metrics: MergeMetrics,
    ) -> Self {
        Self {
            safe_client,
            event_coordinator,
            clob_client,
            pending_merges: RwLock::new(HashMap::new()),
            config,
            metrics,
            http_client: reqwest::Client::new(),
        }
    }

    /// Request a merge for a specific market.
    /// Called by executor when redemption is requested.
    /// The actual merge will be executed in the next poll cycle.
    pub async fn request_merge(&self, action: RedemptionAction) {
        let market = action.market.clone();
        let mut pending = self.pending_merges.write().await;
        pending.insert(market.clone(), action);
        debug!(market = %market, "Merge requested for market");
    }

    /// Check if there are pending merge requests.
    pub async fn has_pending_merges(&self) -> bool {
        !self.pending_merges.read().await.is_empty()
    }

    /// Get count of pending merge requests.
    pub async fn pending_merge_count(&self) -> usize {
        self.pending_merges.read().await.len()
    }

    /// Run the merge polling loop (spawned as background task).
    pub async fn run(self: Arc<Self>, shutdown: watch::Receiver<bool>) {
        info!(
            interval_ms = self.config.merge_poll_interval.as_millis(),
            "Starting merge executor loop"
        );

        let mut interval = tokio::time::interval(self.config.merge_poll_interval);

        loop {
            interval.tick().await;

            // Check for shutdown
            if *shutdown.borrow() {
                info!("Merge executor loop shutting down");
                break;
            }

            // Process pending merge requests
            self.process_pending_merges().await;
        }
    }

    /// Process all pending merge requests once.
    pub async fn process_pending_merges(&self) {
        let pending_actions: Vec<RedemptionAction> = {
            let mut pending = self.pending_merges.write().await;
            pending.drain().map(|(_, action)| action).collect()
        };

        if !pending_actions.is_empty() {
            info!(
                count = pending_actions.len(),
                "Processing pending merge requests"
            );
            for action in pending_actions {
                if let Err(e) = self.execute_merge(&action).await {
                    warn!(error = %e, market = %action.market, "Merge for market failed");
                }
            }
        }
    }

    /// Execute a merge for a specific market based on RedemptionAction.
    ///
    /// Queries token balances from CLOB API (reflects on-chain state),
    /// then merges the minimum redeemable amount.
    async fn execute_merge(&self, action: &RedemptionAction) -> Result<()> {
        let market = &action.market;
        let up_asset_id = &action.up_asset_id;
        let down_asset_id = &action.down_asset_id;

        // Query on-chain token balances directly from CTF contract
        let up_qty = match self.safe_client.get_ctf_balance(up_asset_id).await {
            Ok(qty) => qty,
            Err(e) => {
                error!(error = %e, asset = %up_asset_id, market = %market, "Failed to query CTF balance for up asset");
                return Err(e);
            }
        };
        let down_qty = match self.safe_client.get_ctf_balance(down_asset_id).await {
            Ok(qty) => qty,
            Err(e) => {
                error!(error = %e, asset = %down_asset_id, market = %market, "Failed to query CTF balance for down asset");
                return Err(e);
            }
        };

        // Calculate the minimum amount that can be merged
        let min_amount = up_qty.min(down_qty);

        // Skip if below threshold
        if min_amount < self.config.auto_merge_min_amount {
            debug!(
                market = %market,
                up_qty = up_qty,
                down_qty = down_qty,
                min_amount = min_amount,
                threshold = self.config.auto_merge_min_amount,
                "Skipping merge: below threshold"
            );
            return Ok(());
        }

        info!(
            market = %market,
            up_asset = %up_asset_id,
            down_asset = %down_asset_id,
            up_qty = up_qty,
            down_qty = down_qty,
            merge_amount = min_amount,
            "Executing merge"
        );

        // Query neg_risk from API (fail if query fails, don't use default)
        let neg_risk = self
            .clob_client
            .get_neg_risk(up_asset_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query neg_risk for {}: {}", up_asset_id, e))?;
        debug!(neg_risk = neg_risk, "Queried neg_risk from API");

        // Format amount for merge (6 decimals for USDC)
        let usdc_amount = format!("{:.6}", min_amount);

        // Execute merge via Safe wallet (retry logic is handled inside safe_client)
        let tx_hash = match self
            .safe_client
            .merge_usdc(market, &usdc_amount, neg_risk)
            .await
        {
            Ok(hash) => hash,
            Err(e) => {
                error!(
                    error = %e,
                    market = %market,
                    "Merge transaction failed to submit"
                );
                self.metrics.merge_failures.inc();
                return Ok(());
            }
        };

        info!(
            tx_hash = %tx_hash,
            market = %market,
            quantity = min_amount,
            "Merge transaction submitted, waiting for confirmation..."
        );

        // Wait for transaction to be confirmed on chain
        let confirmation = self
            .safe_client
            .wait_for_transaction(&tx_hash, Some(self.config.merge_confirmation_timeout_secs))
            .await;

        self.handle_merge_confirmation(
            confirmation,
            &tx_hash,
            market,
            up_asset_id,
            down_asset_id,
            min_amount,
        )
        .await;

        Ok(())
    }

    /// Handle merge transaction confirmation result.
    async fn handle_merge_confirmation(
        &self,
        confirmation: TransactionConfirmation,
        _tx_hash: &str,
        market: &str,
        up_asset_id: &str,
        down_asset_id: &str,
        quantity: f64,
    ) {
        match confirmation {
            TransactionConfirmation::Success {
                tx_hash,
                block_number,
            } => {
                info!(
                    tx_hash = %tx_hash,
                    block_number = block_number,
                    market = %market,
                    quantity = quantity,
                    "Merge transaction confirmed on-chain, waiting for CLOB indexing..."
                );

                // Wait for merge to be indexed in Data API before refreshing balance
                let wallet = format!("{:?}", self.safe_client.safe_address());
                let indexed = wait_for_merge_indexed(
                    &self.http_client,
                    &wallet,
                    &tx_hash,
                    self.config.merge_indexing_max_attempts,
                )
                .await;

                if indexed {
                    // Force CLOB to recognize the new USDC balance from merge
                    if let Err(e) = refresh_clob_balance(&self.clob_client).await {
                        warn!(error = %e, "Failed to refresh CLOB balance after merge");
                    }

                    // Emit RedemptionCompleted event after CLOB has indexed the merge
                    self.emit_redemption_completed(market, up_asset_id, down_asset_id, quantity)
                        .await;

                    self.metrics.merges_executed.inc();
                } else {
                    // Merge confirmed on-chain but not indexed by CLOB yet
                    warn!(
                        tx_hash = %tx_hash,
                        market = %market,
                        "Merge confirmed on-chain but not indexed by CLOB, proceeding anyway"
                    );

                    if let Err(e) = refresh_clob_balance(&self.clob_client).await {
                        warn!(error = %e, "Failed to refresh CLOB balance");
                    }

                    self.emit_redemption_completed(market, up_asset_id, down_asset_id, quantity)
                        .await;

                    self.metrics.merges_executed.inc();
                }
            }
            TransactionConfirmation::Failed {
                tx_hash,
                block_number,
            } => {
                warn!(
                    tx_hash = %tx_hash,
                    block_number = block_number,
                    market = %market,
                    quantity = quantity,
                    "Merge transaction confirmed but reverted on chain"
                );
                self.metrics.merge_failures.inc();
            }
            TransactionConfirmation::Timeout { tx_hash } => {
                warn!(
                    tx_hash = %tx_hash,
                    market = %market,
                    quantity = quantity,
                    "Timeout waiting for merge transaction confirmation"
                );
                // Don't count as failure - transaction might still confirm later
                // Also don't emit RedemptionCompleted to avoid premature position update
            }
        }
    }

    /// Emit a RedemptionCompleted event.
    async fn emit_redemption_completed(
        &self,
        market: &str,
        up_asset_id: &str,
        down_asset_id: &str,
        quantity: f64,
    ) {
        let event = RedemptionEvent::RedemptionCompleted {
            market: market.to_string(),
            up_asset_id: up_asset_id.to_string(),
            down_asset_id: down_asset_id.to_string(),
            quantity,
            quote_received: quantity, // 1 pair = $1 USDC
            timestamp: Utc::now(),
        };

        if let Err(e) = self
            .event_coordinator
            .enqueue_event(SystemEvent::Redemption(event))
            .await
        {
            error!(error = %e, "Failed to enqueue redemption event");
        }
    }

    /// Get the config (for testing).
    #[cfg(test)]
    pub fn config(&self) -> &MergeConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_config_default() {
        let config = MergeConfig::default();
        assert_eq!(config.auto_merge_min_amount, 1.0);
    }
}
