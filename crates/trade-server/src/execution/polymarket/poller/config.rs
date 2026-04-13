//! Configuration for the order status poller.

use std::time::Duration;

/// Configuration for the order status poller.
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// Base polling interval between cycles.
    /// Default: 500ms
    pub poll_interval: Duration,

    /// Maximum orders to poll per cycle.
    /// Prevents starvation if many orders pending.
    /// Default: 20
    pub batch_size: usize,

    /// Rate limit: requests per second.
    /// Polymarket limit is ~10 req/sec for authenticated endpoints.
    /// Default: 8.0 (leave headroom)
    pub rate_limit_rps: f64,

    /// Enable position sync (reconciliation).
    /// Default: true
    pub enable_position_sync: bool,

    /// Interval for position sync.
    /// Default: 30s
    pub position_sync_interval: Duration,

    /// Maximum consecutive failures before giving up on an order.
    /// Order will be unmonitored and error logged.
    /// Default: 10
    pub max_consecutive_failures: u32,

    /// Backoff multiplier for failed polls.
    /// Delay = base_interval * backoff^failures
    /// Default: 1.5
    pub failure_backoff_multiplier: f64,

    /// Maximum age for monitored orders.
    /// Orders older than this are pruned (assumed expired/cancelled externally).
    /// Default: 24h
    pub max_order_age: Duration,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(500),
            batch_size: 20,
            rate_limit_rps: 8.0,
            enable_position_sync: true,
            position_sync_interval: Duration::from_secs(30),
            max_consecutive_failures: 10,
            failure_backoff_multiplier: 1.5,
            max_order_age: Duration::from_secs(86400),
        }
    }
}

impl PollerConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the polling interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Set the batch size.
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the rate limit in requests per second.
    pub fn with_rate_limit_rps(mut self, rps: f64) -> Self {
        self.rate_limit_rps = rps;
        self
    }

    /// Enable or disable position sync.
    pub fn with_position_sync(mut self, enabled: bool) -> Self {
        self.enable_position_sync = enabled;
        self
    }

    /// Set the position sync interval.
    pub fn with_position_sync_interval(mut self, interval: Duration) -> Self {
        self.position_sync_interval = interval;
        self
    }

    /// Set the maximum consecutive failures.
    pub fn with_max_consecutive_failures(mut self, max: u32) -> Self {
        self.max_consecutive_failures = max;
        self
    }

    /// Set the failure backoff multiplier.
    pub fn with_failure_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.failure_backoff_multiplier = multiplier;
        self
    }

    /// Set the maximum order age.
    pub fn with_max_order_age(mut self, age: Duration) -> Self {
        self.max_order_age = age;
        self
    }

    /// Calculate the backoff delay for a given number of consecutive failures.
    pub fn calculate_backoff(&self, consecutive_failures: u32) -> Duration {
        let multiplier = self
            .failure_backoff_multiplier
            .powi(consecutive_failures as i32);
        let delay_ms = self.poll_interval.as_millis() as f64 * multiplier;
        Duration::from_millis(delay_ms as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PollerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_millis(500));
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.rate_limit_rps, 8.0);
        assert!(config.enable_position_sync);
        assert_eq!(config.position_sync_interval, Duration::from_secs(30));
        assert_eq!(config.max_consecutive_failures, 10);
        assert_eq!(config.failure_backoff_multiplier, 1.5);
        assert_eq!(config.max_order_age, Duration::from_secs(86400));
    }

    #[test]
    fn test_builder_pattern() {
        let config = PollerConfig::new()
            .with_poll_interval(Duration::from_millis(1000))
            .with_batch_size(50)
            .with_rate_limit_rps(5.0);

        assert_eq!(config.poll_interval, Duration::from_millis(1000));
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.rate_limit_rps, 5.0);
    }

    #[test]
    fn test_calculate_backoff() {
        let config = PollerConfig::default();

        // 0 failures = base interval
        assert_eq!(config.calculate_backoff(0), Duration::from_millis(500));

        // 1 failure = 500 * 1.5 = 750ms
        assert_eq!(config.calculate_backoff(1), Duration::from_millis(750));

        // 2 failures = 500 * 1.5^2 = 1125ms
        assert_eq!(config.calculate_backoff(2), Duration::from_millis(1125));
    }
}
