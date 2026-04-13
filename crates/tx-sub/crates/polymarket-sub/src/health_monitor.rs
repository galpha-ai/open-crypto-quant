//! Health monitoring for detecting silent rate limiting
//!
//! This module tracks event rate over a rolling window and exits the process
//! if the rate drops below a threshold, triggering an orchestrator restart.

use anyhow::Result;
use popeyes_trading_types::PolymarketTradeEvent;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::config::HealthMonitoringConfig;
use crate::metrics::Metrics;
use crate::types::{EventRateTracker, ParsedPriceChangeEvent, SubscriptionState};

/// Health monitor for tracking event rate and detecting silent rate limiting
pub struct HealthMonitor {
    /// Event rate tracker (shared with event recording task)
    event_tracker: Arc<Mutex<EventRateTracker>>,
    /// Subscription state (to check if we have active subscriptions)
    subscription_state: Arc<Mutex<SubscriptionState>>,
    /// Health monitoring configuration
    config: HealthMonitoringConfig,
    /// Metrics for tracking health status
    metrics: Arc<Metrics>,
    /// When the monitor started (for grace period calculation)
    start_time: Instant,
}

impl HealthMonitor {
    /// Create a new health monitor
    ///
    /// # Arguments
    /// * `config` - Health monitoring configuration
    /// * `subscription_state` - Current subscription state (to check subscription count)
    /// * `metrics` - Metrics for tracking health status
    pub fn new(
        config: HealthMonitoringConfig,
        subscription_state: Arc<Mutex<SubscriptionState>>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            event_tracker: Arc::new(Mutex::new(EventRateTracker::new())),
            subscription_state,
            config,
            metrics,
            start_time: Instant::now(),
        }
    }

    /// Get a handle to the event tracker for recording events
    #[allow(dead_code)]
    pub fn event_tracker(&self) -> Arc<Mutex<EventRateTracker>> {
        Arc::clone(&self.event_tracker)
    }

    /// Run the health monitor
    ///
    /// This spawns three tasks:
    /// 1. Trade event recording task: subscribes to parsed trade events and records them
    /// 2. Price change event recording task: subscribes to parsed price change events and records them
    /// 3. Health check task: periodically checks event rate and exits if unhealthy
    ///
    /// # Arguments
    /// * `trade_rx` - Receiver for parsed trade events (clone of parser output channel)
    /// * `price_change_rx` - Receiver for parsed price change events (clone of parser output channel)
    /// * `cancellation_token` - Token to signal shutdown
    pub async fn run(
        self,
        trade_rx: broadcast::Receiver<PolymarketTradeEvent>,
        price_change_rx: broadcast::Receiver<ParsedPriceChangeEvent>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        info!(
            check_interval_secs = self.config.check_interval_secs,
            min_events_per_minute = self.config.min_events_per_minute,
            "Health monitor starting"
        );

        // Spawn trade event recording task
        let event_tracker = Arc::clone(&self.event_tracker);
        let metrics = Arc::clone(&self.metrics);
        let trade_recording_token = cancellation_token.child_token();

        tokio::spawn(async move {
            Self::trade_event_recording_task(
                trade_rx,
                event_tracker,
                metrics,
                trade_recording_token,
            )
            .await
        });

        // Spawn price change event recording task
        let event_tracker = Arc::clone(&self.event_tracker);
        let metrics = Arc::clone(&self.metrics);
        let price_change_recording_token = cancellation_token.child_token();

        tokio::spawn(async move {
            Self::price_change_event_recording_task(
                price_change_rx,
                event_tracker,
                metrics,
                price_change_recording_token,
            )
            .await
        });

        // Run health check task in this task
        self.health_check_task(cancellation_token).await
    }

    /// Trade event recording task: subscribe to trade events and record them in the tracker
    async fn trade_event_recording_task(
        mut trade_rx: broadcast::Receiver<PolymarketTradeEvent>,
        event_tracker: Arc<Mutex<EventRateTracker>>,
        metrics: Arc<Metrics>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        info!("Trade event recording task starting");

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Trade event recording task shutting down");
                    break;
                }
                result = trade_rx.recv() => {
                    match result {
                        Ok(_trade_event) => {
                            // Record event in tracker
                            let mut tracker = event_tracker.lock().await;
                            tracker.record_event();

                            // Update last event timestamp metric
                            metrics.last_event_timestamp.set(
                                chrono::Utc::now().timestamp() as f64
                            );
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped = skipped,
                                "Health monitor lagged behind parser, some trade events skipped"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            warn!("Trade event channel closed");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Price change event recording task: subscribe to price change events and record them in the tracker
    async fn price_change_event_recording_task(
        mut price_change_rx: broadcast::Receiver<ParsedPriceChangeEvent>,
        event_tracker: Arc<Mutex<EventRateTracker>>,
        metrics: Arc<Metrics>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        info!("Price change event recording task starting");

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Price change event recording task shutting down");
                    break;
                }
                result = price_change_rx.recv() => {
                    match result {
                        Ok(_price_change_event) => {
                            // Record event in tracker
                            let mut tracker = event_tracker.lock().await;
                            tracker.record_event();

                            // Update last event timestamp metric
                            metrics.last_event_timestamp.set(
                                chrono::Utc::now().timestamp() as f64
                            );
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(
                                skipped = skipped,
                                "Health monitor lagged behind parser, some price change events skipped"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            warn!("Price change event channel closed");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Health check task: periodically check event rate and exit if unhealthy
    async fn health_check_task(
        self,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        let mut check_interval = interval(Duration::from_secs(self.config.check_interval_secs));

        // Grace period: don't enforce health checks for first 60 seconds
        let grace_period = Duration::from_secs(60);

        info!("Health check task starting");

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Health check task shutting down");
                    break;
                }
                _ = check_interval.tick() => {
                    // Check if we're still in grace period
                    let uptime = self.start_time.elapsed();
                    if uptime < grace_period {
                        info!(
                            uptime_secs = uptime.as_secs(),
                            "Still in grace period, skipping health check"
                        );
                        continue;
                    }

                    // Get event rate
                    let events_per_minute = {
                        let tracker = self.event_tracker.lock().await;
                        tracker.get_rate_per_minute()
                    };

                    // Get subscription count
                    let subscription_count = {
                        let state = self.subscription_state.lock().await;
                        state.len()
                    };

                    // Update metrics
                    self.metrics.events_per_minute.set(events_per_minute as f64);

                    // Prune old entries from tracker
                    {
                        let mut tracker = self.event_tracker.lock().await;
                        let cutoff = chrono::Utc::now()
                            - chrono::Duration::seconds(self.config.event_tracking_window_secs as i64);
                        tracker.prune_old(cutoff);
                    }

                    // Check health threshold
                    if subscription_count > 0
                        && events_per_minute < self.config.min_events_per_minute
                    {
                        // Health check failed - possible silent rate limiting
                        error!(
                            events_per_minute = events_per_minute,
                            subscriptions = subscription_count,
                            threshold = self.config.min_events_per_minute,
                            "Event rate dropped below threshold, possible silent rate limiting detected - exiting for restart"
                        );

                        // Increment failure metric
                        self.metrics
                            .health_check_failures
                            .with_label_values(&["low_event_rate"])
                            .inc();

                        // Exit process to trigger orchestrator restart
                        std::process::exit(1);
                    } else {
                        // Health check passed
                        info!(
                            events_per_minute = events_per_minute,
                            subscriptions = subscription_count,
                            threshold = self.config.min_events_per_minute,
                            "Health check passed"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SubscriptionState;
    use std::collections::HashSet;

    #[tokio::test]
    async fn test_event_tracker_records_events() {
        let config = HealthMonitoringConfig {
            enabled: true,
            check_interval_secs: 60,
            min_events_per_minute: 1,
            event_tracking_window_secs: 120,
        };

        let subscription_state = Arc::new(Mutex::new(SubscriptionState::new()));
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(Metrics::new(&registry).unwrap());

        let monitor = HealthMonitor::new(config, subscription_state, metrics);
        let tracker = monitor.event_tracker();

        // Record some events
        {
            let mut t = tracker.lock().await;
            t.record_event();
            t.record_event();
            t.record_event();
        }

        // Check total events
        {
            let t = tracker.lock().await;
            assert_eq!(t.total_events, 3);
        }
    }

    #[tokio::test]
    async fn test_event_tracker_calculates_rate() {
        let config = HealthMonitoringConfig {
            enabled: true,
            check_interval_secs: 60,
            min_events_per_minute: 1,
            event_tracking_window_secs: 120,
        };

        let subscription_state = Arc::new(Mutex::new(SubscriptionState::new()));
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(Metrics::new(&registry).unwrap());

        let monitor = HealthMonitor::new(config, subscription_state, metrics);
        let tracker = monitor.event_tracker();

        // Record 10 events
        {
            let mut t = tracker.lock().await;
            for _ in 0..10 {
                t.record_event();
            }
        }

        // Check rate (should be 10 events in the last minute)
        {
            let t = tracker.lock().await;
            let rate = t.get_rate_per_minute();
            assert_eq!(rate, 10);
        }
    }

    #[tokio::test]
    async fn test_health_monitor_checks_subscription_count() {
        let _config = HealthMonitoringConfig {
            enabled: true,
            check_interval_secs: 1,
            min_events_per_minute: 5,
            event_tracking_window_secs: 120,
        };

        let subscription_state = Arc::new(Mutex::new(SubscriptionState::new()));

        // Initially no subscriptions - health check should pass even with 0 events
        {
            let state = subscription_state.lock().await;
            assert_eq!(state.len(), 0);
        }

        // Add subscriptions
        {
            let mut state = subscription_state.lock().await;
            let mut assets = HashSet::new();
            assets.insert("asset1".to_string());
            assets.insert("asset2".to_string());
            state.update(assets, 2);
        }

        // Now subscription_count > 0, so health check would fail if events < threshold
        {
            let state = subscription_state.lock().await;
            assert_eq!(state.len(), 2);
        }
    }
}
