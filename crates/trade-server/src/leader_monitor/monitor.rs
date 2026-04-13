use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
    time::Duration,
};

use solana_client::nonblocking::rpc_client::RpcClient; // Use non-blocking client
use solana_sdk::{clock::Slot, pubkey::Pubkey};
use tokio::{
    sync::{RwLock, watch},
    task::JoinHandle,
};

use crate::leader_monitor::{MonitorError, MonitorWorker};

const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(60); // Refresh cache every minute

#[derive(Debug, Default)]
pub struct CacheState {
    /// Cache holding the latest fetched leader schedule.
    /// HashMap<Slot, Pubkey>
    pub latest_schedule: Option<Arc<HashMap<Slot, Pubkey>>>,
}

/// Service to monitor the leader schedule for specific target validators based on a cached schedule.
/// Schedule updates are handled by a background task.
pub struct LeaderMonitorService {
    /// Set of target validator public keys to monitor.
    target_validators: Arc<RwLock<HashSet<Pubkey>>>,
    /// Shared cache state updated by the background worker.
    cache_state: Arc<RwLock<CacheState>>,
    /// Handle to the background worker task.
    _worker_handle: JoinHandle<()>,
    /// Sender to signal shutdown to the background worker.
    shutdown_tx: watch::Sender<bool>,
}

// Manual impl to avoid Debug requirement on RpcClient/JoinHandle if we included all fields
// Also allows controlling which fields are shown.
impl fmt::Debug for LeaderMonitorService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Attempt to read the target validators. If the lock is poisoned, indicate that.
        // We use try_read here to avoid blocking in a synchronous Debug impl.
        let validators_dbg = match self.target_validators.try_read() {
            Ok(guard) => format!("{:?}", *guard),
            Err(_) => "<lock poisoned>".to_string(),
        };

        f.debug_struct("LeaderMonitorService")
            .field("target_validators", &validators_dbg)
            .field("worker_running", &(!*self.shutdown_tx.borrow())) // Show if worker signaled to stop
            .finish_non_exhaustive() // Indicate not all fields are shown
    }
}

impl LeaderMonitorService {
    /// Creates a new instance of the LeaderMonitorService and starts the background refresh task.
    ///
    /// # Arguments
    /// * `rpc_client`: An Arc-wrapped RpcClient connected to a Solana node.
    /// * `initial_target_validators`: The initial set of validator keys to monitor.
    /// * `refresh_interval`: Optional custom refresh interval for the background task.
    pub fn new(
        rpc_client: Arc<RpcClient>,
        initial_target_validators: HashSet<Pubkey>,
        refresh_interval: Option<Duration>,
    ) -> Self {
        let target_validators = Arc::new(RwLock::new(initial_target_validators));
        let cache_state = Arc::new(RwLock::new(CacheState::default()));
        let (shutdown_tx, shutdown_rx) = watch::channel(false); // Shutdown signal channel

        let worker = MonitorWorker {
            rpc_client: rpc_client.clone(), // Clone Arc for the worker
            cache_state: cache_state.clone(),
            refresh_interval: refresh_interval.unwrap_or(DEFAULT_REFRESH_INTERVAL),
            shutdown_rx,
        };

        // Spawn the background worker task
        let worker_handle = tokio::spawn(worker.run());

        Self {
            target_validators,
            cache_state,
            _worker_handle: worker_handle, // Store handle (e.g., for joining on drop)
            shutdown_tx,
        }
    }

    /// Checks if any internally tracked target validator is scheduled
    /// to be the leader for any slot within the range [start_slot, end_slot] (inclusive),
    /// based *only* on the currently cached schedule.
    ///
    /// This method does NOT perform any network requests. It returns results
    /// based on the data fetched periodically by the background task.
    ///
    /// # Arguments
    /// * `start_slot`: The starting slot number of the range to check.
    /// * `end_slot`: The ending slot number of the range to check (inclusive).
    ///
    /// # Returns
    /// * `Ok(true)`: If a target validator is found as a leader in the cached schedule for the range.
    /// * `Ok(false)`: If no target validators are found in the cache for the range, or if the required epoch/slot data is not cached.
    /// * `Err(MonitorError::InvalidSlotRange)`: If `start_slot > end_slot`.
    /// * `Err(MonitorError::CacheNotReady)`: If the cache hasn't been populated with epoch info yet.
    /// * `Err(MonitorError::LockError)`: If internal locks cannot be acquired.
    pub async fn is_target_validator_scheduled_leader(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<bool, MonitorError> {
        if start_slot > end_slot {
            return Err(MonitorError::InvalidSlotRange(start_slot, end_slot));
        }

        // 1. Acquire read locks on shared state (read locks allow concurrency)
        let target_validators_guard = self.target_validators.read().await;
        let cache_state_guard = self.cache_state.read().await;

        // 2. Check if cache is ready and get the schedule
        let schedule = match cache_state_guard.latest_schedule {
            Some(ref schedule_arc) => schedule_arc,
            None => return Err(MonitorError::CacheNotReady),
        };

        // 3. Iterate through the slot range and check against the cache
        for slot in start_slot..=end_slot {
            // Check if the specific slot is in the cached schedule
            if let Some(leader) = schedule.get(&slot) {
                // Check if the leader is one of the targets
                if target_validators_guard.contains(leader) {
                    return Ok(true); // Found a target validator as leader
                }
            }
            // If slot is not in the map, continue checking other slots.
            // This implicitly handles cases where schedule has gaps or is incomplete.
            // This means the slot is outside the currently cached range (e.g., too far past or future).
        }

        // 4. No target validator found in the cached range
        Ok(false)
    }

    /// Updates the entire set of target validators to monitor.
    pub async fn set_target_validators(
        &self,
        validators: HashSet<Pubkey>,
    ) -> Result<(), MonitorError> {
        let mut lock = self.target_validators.write().await;
        *lock = validators;
        Ok(())
    }

    /// Adds a single validator to the set of target validators.
    pub async fn add_target_validator(&self, validator: Pubkey) -> Result<(), MonitorError> {
        let mut lock = self.target_validators.write().await;
        lock.insert(validator);
        Ok(())
    }

    /// Removes a single validator from the set of target validators.
    pub async fn remove_target_validator(&self, validator: &Pubkey) -> Result<(), MonitorError> {
        let mut lock = self.target_validators.write().await;
        lock.remove(validator);
        Ok(())
    }

    /// Signals the background worker to stop.
    /// Does not wait for the worker to fully exit. Consider adding a method
    /// like `stop_and_wait()` if synchronous shutdown is needed.
    pub fn stop_worker(&self) {
        if self.shutdown_tx.send(true).is_err() {
            tracing::warn!(
                "Leader monitor worker already stopped or shutdown channel receiver dropped."
            );
        }
    }
}

/// Optional: Implement Drop to automatically stop the worker when the service goes out of scope.
impl Drop for LeaderMonitorService {
    fn drop(&mut self) {
        tracing::info!("Dropping LeaderMonitorService, signaling worker to stop.");
        self.stop_worker();
        // Note: We don't join the handle here in drop because drop cannot be async.
        // The handle `_worker_handle` will be dropped, detaching the task.
        // If guaranteed cleanup is needed, provide an explicit async `shutdown()` method.
    }
}
