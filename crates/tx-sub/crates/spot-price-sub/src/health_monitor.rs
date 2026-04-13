//! Health monitoring for spot price subscriber
//!
//! Monitors the rate of incoming spot price updates and exits the process
//! if the rate drops below a configured threshold, triggering container
//! orchestrator restart.

use anyhow::Result;
use chrono::Utc;
use popeyes_trading_types::SpotPriceUpdate;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};
use tokio::time::interval;

use crate::config::HealthMonitoringConfig;
use crate::metrics::Metrics;

/// Grace period after startup before enforcing health checks (seconds)
const STARTUP_GRACE_PERIOD_SECS: u64 = 60;

/// Event rate tracker for health monitoring
struct EventRateTracker {
    /// Events per second (keyed by timestamp)
    events_per_second: HashMap<i64, usize>,
    /// Total events received
    total_events: usize,
    /// Last event timestamp
    last_event_timestamp: i64,
}

impl EventRateTracker {
    fn new() -> Self {
        Self {
            events_per_second: HashMap::new(),
            total_events: 0,
            last_event_timestamp: 0,
        }
    }

    /// Record an event
    fn record_event(&mut self) {
        let now = Utc::now().timestamp();
        *self.events_per_second.entry(now).or_insert(0) += 1;
        self.total_events += 1;
        self.last_event_timestamp = now;
    }

    /// Calculate rolling event rate over the last N seconds
    fn calculate_rate(&self, window_secs: u64) -> f64 {
        let now = Utc::now().timestamp();
        let cutoff = now - window_secs as i64;

        let events_in_window: usize = self
            .events_per_second
            .iter()
            .filter(|(timestamp, _)| **timestamp >= cutoff)
            .map(|(_, count)| count)
            .sum();

        // Convert to events per minute
        (events_in_window as f64 / window_secs as f64) * 60.0
    }

    /// Prune old entries to prevent unbounded growth
    fn prune(&mut self, keep_last_secs: u64) {
        let now = Utc::now().timestamp();
        let cutoff = now - keep_last_secs as i64;

        self.events_per_second.retain(|timestamp, _| *timestamp >= cutoff);
    }
}

/// Health monitor for spot price subscriber
pub struct HealthMonitor {
    /// Spot price update receiver
    spot_price_rx: broadcast::Receiver<SpotPriceUpdate>,
    /// Health monitoring configuration
    config: HealthMonitoringConfig,
    /// Event rate tracker (shared between tasks)
    event_tracker: Arc<Mutex<EventRateTracker>>,
    /// Metrics
    metrics: Arc<Metrics>,
    /// Startup timestamp
    startup_time: i64,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(
        spot_price_rx: broadcast::Receiver<SpotPriceUpdate>,
        config: HealthMonitoringConfig,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            spot_price_rx,
            config,
            event_tracker: Arc::new(Mutex::new(EventRateTracker::new())),
            metrics,
            startup_time: Utc::now().timestamp(),
        }
    }

    /// Start the health monitor (consumes self)
    pub async fn start(self) -> Result<()> {
        if !self.config.enabled {
            tracing::info!("Health monitoring disabled, monitor not started");
            return Ok(());
        }

        tracing::info!(
            check_interval_secs = self.config.check_interval_secs,
            min_updates_per_minute = self.config.min_updates_per_minute,
            "Health monitor starting"
        );

        // Spawn event recording task
        let recording_task = self.spawn_event_recording_task();

        // Spawn health check task
        let checking_task = self.spawn_health_check_task();

        tokio::select! {
            _ = recording_task => {
                tracing::info!("Event recording task ended");
            }
            _ = checking_task => {
                tracing::info!("Health check task ended");
            }
        }

        Ok(())
    }

    /// Spawn task that records incoming events
    fn spawn_event_recording_task(&self) -> tokio::task::JoinHandle<()> {
        let mut rx = self.spot_price_rx.resubscribe();
        let tracker = Arc::clone(&self.event_tracker);
        let metrics = Arc::clone(&self.metrics);

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(_update) => {
                        let mut tracker = tracker.lock().await;
                        tracker.record_event();

                        // Update metrics
                        metrics
                            .last_update_timestamp
                            .set(tracker.last_event_timestamp);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(lagged = n, "Health monitor lagged behind updates");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Update channel closed, event recording task ending");
                        break;
                    }
                }
            }
        })
    }

    /// Spawn task that periodically checks health
    fn spawn_health_check_task(&self) -> tokio::task::JoinHandle<()> {
        let check_interval_secs = self.config.check_interval_secs;
        let min_updates_per_minute = self.config.min_updates_per_minute;
        let tracking_window_secs = self.config.event_tracking_window_secs;
        let tracker = Arc::clone(&self.event_tracker);
        let metrics = Arc::clone(&self.metrics);
        let startup_time = self.startup_time;

        tokio::spawn(async move {
            let mut check_interval = interval(Duration::from_secs(check_interval_secs));

            loop {
                check_interval.tick().await;

                let now = Utc::now().timestamp();
                let uptime_secs = now - startup_time;

                // Skip health checks during grace period
                if uptime_secs < STARTUP_GRACE_PERIOD_SECS as i64 {
                    tracing::debug!(
                        uptime_secs = uptime_secs,
                        grace_period_secs = STARTUP_GRACE_PERIOD_SECS,
                        "In grace period, skipping health check"
                    );
                    continue;
                }

                let mut tracker = tracker.lock().await;

                // Calculate current rate
                let current_rate = tracker.calculate_rate(tracking_window_secs);

                // Update metrics
                metrics.updates_per_minute.set(current_rate as i64);

                tracing::debug!(
                    rate = current_rate,
                    threshold = min_updates_per_minute,
                    "Health check"
                );

                // Check if rate is below threshold
                if current_rate < min_updates_per_minute as f64 {
                    tracing::error!(
                        rate = current_rate,
                        threshold = min_updates_per_minute,
                        uptime_secs = uptime_secs,
                        "Health check failed: event rate too low, exiting"
                    );

                    metrics.health_check_failures.inc();

                    // Exit process with error code
                    std::process::exit(1);
                }

                // Prune old events (keep 2x window for safety)
                tracker.prune(tracking_window_secs * 2);
            }
        })
    }
}
