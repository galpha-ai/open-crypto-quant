//! Metrics for the merge executor.

use prometheus::{Counter, Opts, Registry};

/// Metrics for the merge executor.
#[derive(Clone)]
pub struct MergeMetrics {
    /// Auto-merge operations executed
    pub merges_executed: Counter,

    /// Auto-merge failures
    pub merge_failures: Counter,
}

impl MergeMetrics {
    /// Create new metrics and register with the given registry.
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let merges_executed = Counter::with_opts(Opts::new(
            "polymarket_merge_executor_merges_executed_total",
            "Total number of merge operations executed",
        ))?;
        registry.register(Box::new(merges_executed.clone()))?;

        let merge_failures = Counter::with_opts(Opts::new(
            "polymarket_merge_executor_merge_failures_total",
            "Total number of merge failures",
        ))?;
        registry.register(Box::new(merge_failures.clone()))?;

        Ok(Self {
            merges_executed,
            merge_failures,
        })
    }

    /// Create metrics without registration (for testing).
    pub fn unregistered() -> Self {
        Self {
            merges_executed: Counter::new(
                "polymarket_merge_executor_merges_executed_total",
                "Total number of merge operations executed",
            )
            .unwrap(),
            merge_failures: Counter::new(
                "polymarket_merge_executor_merge_failures_total",
                "Total number of merge failures",
            )
            .unwrap(),
        }
    }
}

impl Default for MergeMetrics {
    fn default() -> Self {
        Self::unregistered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_metrics_creation() {
        let metrics = MergeMetrics::unregistered();
        assert_eq!(metrics.merges_executed.get(), 0.0);
        assert_eq!(metrics.merge_failures.get(), 0.0);
    }

    #[test]
    fn test_merge_metrics_increment() {
        let metrics = MergeMetrics::unregistered();
        metrics.merges_executed.inc();
        metrics.merge_failures.inc();
        assert_eq!(metrics.merges_executed.get(), 1.0);
        assert_eq!(metrics.merge_failures.get(), 1.0);
    }
}
