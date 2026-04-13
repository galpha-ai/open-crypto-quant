use std::{collections::HashMap, sync::Arc, time::Duration};

use solana_client::nonblocking::rpc_client::RpcClient;
use tokio::sync::{RwLock, watch};
use tracing::{error, warn};

use crate::leader_monitor::{CacheState, MonitorError};

const LEADER_SLOT_LOOKAHEAD: u64 = 200; // Number of slots ahead to fetch leaders for

/// Background worker responsible for fetching and updating the leader schedule cache.
pub struct MonitorWorker {
    pub rpc_client: Arc<RpcClient>,
    pub cache_state: Arc<RwLock<CacheState>>,
    pub refresh_interval: Duration,
    pub shutdown_rx: watch::Receiver<bool>,
}

impl MonitorWorker {
    /// Runs the worker loop until a shutdown signal is received.
    pub async fn run(mut self) {
        tracing::info!("Leader monitor background worker started.");
        let mut interval_timer = tokio::time::interval(self.refresh_interval);

        loop {
            tokio::select! {
                _ = interval_timer.tick() => {
                    if let Err(e) = self.refresh_cache().await {
                        error!("Failed to refresh leader schedule cache: {}", e);
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        tracing::info!("Leader monitor background worker received shutdown signal.");
                        break;
                    }
                }
            }
        }
        tracing::info!("Leader monitor background worker stopped.");
    }

    /// Fetches the latest epoch info and leader schedules for the current and next epochs.
    pub async fn refresh_cache(&self) -> Result<(), MonitorError> {
        tracing::debug!("Refreshing leader schedule cache...");

        // 1. Get current slot
        let current_slot = self.rpc_client.get_slot().await?;
        tracing::debug!(current_slot, "Fetched current slot");

        // 2. Fetch slot leaders for a lookahead window
        let leaders = self
            .rpc_client
            .get_slot_leaders(current_slot, LEADER_SLOT_LOOKAHEAD)
            .await?;

        if leaders.is_empty() {
            warn!(
                "Received empty leader list from RPC for slot range starting at {}, limit {}",
                current_slot, LEADER_SLOT_LOOKAHEAD
            );
            // Decide whether to clear the cache or keep the old one. Let's keep the old one for now.
            return Ok(());
        }

        // 3. Construct the new schedule map
        let mut new_schedule = HashMap::with_capacity(leaders.len());
        for (i, leader) in leaders.into_iter().enumerate() {
            let slot = current_slot + i as u64;
            new_schedule.insert(slot, leader);
        }

        // 4. Update cache state
        let mut cache_state_guard = self.cache_state.write().await; // Acquire write lock
        cache_state_guard.latest_schedule = Some(Arc::new(new_schedule));

        tracing::debug!(
            "Successfully refreshed leader schedule cache with {} slots starting from {}",
            cache_state_guard
                .latest_schedule
                .as_ref()
                .map(|s| s.len())
                .unwrap_or(0),
            current_slot
        );
        Ok(()) // Lock is released when cache_state_guard goes out of scope
    }
}
