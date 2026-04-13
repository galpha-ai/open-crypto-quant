//! Configuration for the merge executor.

use std::time::Duration;

/// Configuration for the merge executor.
#[derive(Debug, Clone)]
pub struct MergeConfig {
    /// Minimum redeemable pairs before triggering auto-merge.
    /// Default: 1.0 (merge any redeemable amount >= 1 USDC)
    pub auto_merge_min_amount: f64,

    /// Interval for merge polling (independent from order polling).
    /// Default: 3s
    pub merge_poll_interval: Duration,

    /// Maximum time to wait for merge transaction confirmation in seconds.
    /// Default: 60s
    pub merge_confirmation_timeout_secs: u64,

    /// Maximum attempts to check if merge is indexed in Data API.
    /// Default: 60
    pub merge_indexing_max_attempts: u32,
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            auto_merge_min_amount: 1.0,
            merge_poll_interval: Duration::from_secs(3),
            merge_confirmation_timeout_secs: 60,
            merge_indexing_max_attempts: 60,
        }
    }
}

impl MergeConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the minimum amount for auto-merge.
    pub fn with_auto_merge_min_amount(mut self, amount: f64) -> Self {
        self.auto_merge_min_amount = amount;
        self
    }

    /// Set the merge poll interval.
    pub fn with_merge_poll_interval(mut self, interval: Duration) -> Self {
        self.merge_poll_interval = interval;
        self
    }

    /// Set the merge confirmation timeout.
    pub fn with_merge_confirmation_timeout_secs(mut self, secs: u64) -> Self {
        self.merge_confirmation_timeout_secs = secs;
        self
    }

    /// Set the merge indexing max attempts.
    pub fn with_merge_indexing_max_attempts(mut self, attempts: u32) -> Self {
        self.merge_indexing_max_attempts = attempts;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MergeConfig::default();
        assert_eq!(config.auto_merge_min_amount, 1.0);
        assert_eq!(config.merge_poll_interval, Duration::from_secs(3));
        assert_eq!(config.merge_confirmation_timeout_secs, 60);
        assert_eq!(config.merge_indexing_max_attempts, 60);
    }

    #[test]
    fn test_builder_pattern() {
        let config = MergeConfig::new()
            .with_auto_merge_min_amount(5.0)
            .with_merge_poll_interval(Duration::from_secs(10));

        assert_eq!(config.auto_merge_min_amount, 5.0);
        assert_eq!(config.merge_poll_interval, Duration::from_secs(10));
    }
}
