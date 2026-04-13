use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time;
use tracing::info;

use crate::metrics::Metrics;

pub struct StatsMonitor {
    events_by_dex: Arc<Mutex<DexEventWindow>>,
    log_interval: Duration,
    enabled: bool,
    metrics: Option<Arc<Metrics>>,
}

impl StatsMonitor {
    pub fn new(window_duration: Duration, log_interval: Duration, enabled: bool) -> Arc<Self> {
        let monitor = Self {
            events_by_dex: Arc::new(Mutex::new(DexEventWindow::new(window_duration))),
            log_interval,
            enabled,
            metrics: None,
        };
        Arc::new(monitor)
    }

    pub fn new_with_metrics(
        window_duration: Duration,
        log_interval: Duration,
        enabled: bool,
        metrics: Arc<Metrics>,
    ) -> Arc<Self> {
        let monitor = Self {
            events_by_dex: Arc::new(Mutex::new(DexEventWindow::new(window_duration))),
            log_interval,
            enabled,
            metrics: Some(metrics),
        };
        Arc::new(monitor)
    }

    pub async fn record_dex_event(&self, dex: &str) {
        if !self.enabled {
            return;
        }

        let mut events = self.events_by_dex.lock().await;
        events.record_event(dex);
    }

    pub fn spawn_logging_task(self: Arc<Self>) {
        if !self.enabled {
            return;
        }

        tokio::spawn(async move {
            let mut interval = time::interval(self.log_interval);
            interval.tick().await; // Skip first tick

            loop {
                interval.tick().await;
                let stats = self.get_stats().await;
                Self::log_stats(&stats);
            }
        });
    }

    pub async fn is_inactive(&self, threshold: Duration) -> bool {
        let stats = self.get_stats().await;

        match stats.last_event_age_secs {
            Some(age) => age >= threshold.as_secs_f64(),
            None => false, // No events yet - not considered inactive during startup
        }
    }

    pub fn spawn_inactivity_watchdog(self: Arc<Self>, timeout: Duration) {
        if !self.enabled {
            return;
        }

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(1));
            interval.tick().await; // Skip first tick

            loop {
                interval.tick().await;

                if self.is_inactive(timeout).await {
                    let stats = self.get_stats().await;
                    let age = stats
                        .last_event_age_secs
                        .map(|a| format!("{:.1}", a))
                        .unwrap_or_else(|| "none".to_string());

                    tracing::error!(
                        timeout_seconds = timeout.as_secs(),
                        last_event_age_secs = age,
                        "Inactivity timeout exceeded - no trade events observed within threshold, exiting"
                    );

                    // Increment metrics counter if available
                    if let Some(metrics) = &self.metrics {
                        metrics.inactivity_exits_total.inc();
                    }

                    std::process::exit(1);
                }
            }
        });
    }

    async fn get_stats(&self) -> DexStats {
        let mut events = self.events_by_dex.lock().await;
        events.get_stats()
    }

    fn log_stats(stats: &DexStats) {
        let rate = stats.total_count as f64 / stats.window_seconds;

        // Format DEX breakdown
        let mut dex_breakdown = String::new();
        for (dex, count) in &stats.by_dex {
            if !dex_breakdown.is_empty() {
                dex_breakdown.push_str(", ");
            }
            dex_breakdown.push_str(&format!("{}={}", dex.to_lowercase(), count));
        }

        info!(
            total_events = stats.total_count,
            window_secs = stats.window_seconds,
            events_per_sec = format!("{:.1}", rate),
            last_event_age_secs = stats
                .last_event_age_secs
                .map(|age| format!("{:.1}", age))
                .unwrap_or_else(|| "none".to_string()),
            dex_breakdown = dex_breakdown,
            "dex_stats_monitor"
        );
    }
}

struct DexEventWindow {
    events_by_dex: HashMap<String, VecDeque<Instant>>,
    window_duration: Duration,
}

impl DexEventWindow {
    fn new(window_duration: Duration) -> Self {
        Self {
            events_by_dex: HashMap::new(),
            window_duration,
        }
    }

    fn record_event(&mut self, dex: &str) {
        let now = Instant::now();
        let events = self
            .events_by_dex
            .entry(dex.to_string())
            .or_insert_with(VecDeque::new);
        events.push_back(now);

        // Cleanup old events for this DEX
        self.cleanup_old_events(now);
    }

    fn cleanup_old_events(&mut self, now: Instant) {
        let cutoff = now - self.window_duration;

        for events in self.events_by_dex.values_mut() {
            while let Some(&front_time) = events.front() {
                if front_time < cutoff {
                    events.pop_front();
                } else {
                    break;
                }
            }
        }

        // Remove empty DEX entries
        self.events_by_dex.retain(|_, events| !events.is_empty());
    }

    fn get_stats(&mut self) -> DexStats {
        let now = Instant::now();

        // Clean up before calculating stats
        self.cleanup_old_events(now);

        let total: usize = self.events_by_dex.values().map(|events| events.len()).sum();

        // Find most recent event across all DEXs
        let last_event_age_secs = self
            .events_by_dex
            .values()
            .filter_map(|events| events.back())
            .max()
            .map(|&last_time| now.duration_since(last_time).as_secs_f64());

        let by_dex: HashMap<String, usize> = self
            .events_by_dex
            .iter()
            .map(|(dex, events)| (dex.clone(), events.len()))
            .collect();

        DexStats {
            total_count: total,
            by_dex,
            window_seconds: self.window_duration.as_secs_f64(),
            last_event_age_secs,
        }
    }
}

pub struct DexStats {
    pub total_count: usize,
    pub by_dex: HashMap<String, usize>,
    pub window_seconds: f64,
    pub last_event_age_secs: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_dex_event_window() {
        let mut window = DexEventWindow::new(Duration::from_secs(2));

        window.record_event("pumpfun");
        window.record_event("rayammv4");
        window.record_event("pumpfun");
        window.record_event("bonk");

        let stats = window.get_stats();
        assert_eq!(stats.total_count, 4);
        assert_eq!(stats.by_dex.get("pumpfun"), Some(&2));
        assert_eq!(stats.by_dex.get("rayammv4"), Some(&1));
        assert_eq!(stats.by_dex.get("bonk"), Some(&1));
    }

    #[tokio::test]
    async fn test_sliding_window_cleanup() {
        let mut window = DexEventWindow::new(Duration::from_secs(1));

        // Add events
        window.record_event("pumpfun");
        window.record_event("rayammv4");

        // Wait for window to expire
        sleep(Duration::from_millis(1100)).await;

        // Add new event which triggers cleanup
        window.record_event("pumpfun");

        let stats = window.get_stats();
        assert_eq!(stats.total_count, 1); // Only the recent event
        assert_eq!(stats.by_dex.get("pumpfun"), Some(&1));
        assert_eq!(stats.by_dex.get("rayammv4"), None); // Cleaned up
    }

    #[tokio::test]
    async fn test_stats_monitor_disabled() {
        let monitor = StatsMonitor::new(
            Duration::from_secs(10),
            Duration::from_secs(5),
            false, // disabled
        );

        // Should not record when disabled
        monitor.record_dex_event("pumpfun").await;

        let stats = monitor.get_stats().await;
        assert_eq!(stats.total_count, 0);
    }

    #[tokio::test]
    async fn test_is_inactive_with_no_events() {
        let monitor = StatsMonitor::new(Duration::from_secs(10), Duration::from_secs(5), true);

        // Should return false when no events have occurred yet (startup case)
        assert!(!monitor.is_inactive(Duration::from_secs(5)).await);
    }

    #[tokio::test]
    async fn test_is_inactive_returns_false_when_events_within_threshold() {
        let monitor = StatsMonitor::new(Duration::from_secs(10), Duration::from_secs(5), true);

        // Record an event
        monitor.record_dex_event("pumpfun").await;

        // Should return false immediately after event
        assert!(!monitor.is_inactive(Duration::from_secs(5)).await);

        // Wait a bit but still within threshold
        sleep(Duration::from_millis(500)).await;
        assert!(!monitor.is_inactive(Duration::from_secs(5)).await);
    }

    #[tokio::test]
    async fn test_is_inactive_returns_true_after_timeout() {
        let monitor = StatsMonitor::new(Duration::from_secs(10), Duration::from_secs(5), true);

        // Record an event
        monitor.record_dex_event("pumpfun").await;

        // Should return false immediately
        assert!(!monitor.is_inactive(Duration::from_millis(500)).await);

        // Wait past the timeout threshold
        sleep(Duration::from_millis(600)).await;

        // Should return true now that timeout has expired
        assert!(monitor.is_inactive(Duration::from_millis(500)).await);
    }

    #[tokio::test]
    async fn test_is_inactive_with_multiple_dex_events() {
        let monitor = StatsMonitor::new(Duration::from_secs(10), Duration::from_secs(5), true);

        // Record events from different DEXs
        monitor.record_dex_event("pumpfun").await;
        sleep(Duration::from_millis(100)).await;
        monitor.record_dex_event("rayammv4").await;

        // Most recent event should determine inactivity status
        assert!(!monitor.is_inactive(Duration::from_millis(200)).await);

        // Wait past threshold - should be inactive based on most recent event
        sleep(Duration::from_millis(250)).await;
        assert!(monitor.is_inactive(Duration::from_millis(200)).await);
    }
}
