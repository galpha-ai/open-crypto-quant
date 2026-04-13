use prometheus::{
    CounterVec, Gauge, Histogram, Opts, Registry, histogram_opts,
    register_counter_vec_with_registry, register_gauge_with_registry,
    register_histogram_with_registry,
};

/// Prometheus metrics for the paper trading executor.
///
/// These metrics provide observability into paper trading operations:
/// - Order placement, cancellation, and fill counts
/// - Fill latency (time from order placement to fill)
/// - Current pending order count
#[derive(Clone, Debug)]
pub struct PaperTradingMetrics {
    /// Counter for limit orders placed, labeled by side (buy/sell)
    pub orders_placed: CounterVec,
    /// Counter for orders cancelled
    pub orders_cancelled: CounterVec,
    /// Counter for orders filled (partially or fully), labeled by side
    pub orders_filled: CounterVec,
    /// Counter for total fill volume in base units, labeled by side
    pub fill_volume: CounterVec,
    /// Histogram for fill latency in milliseconds (time from placement to fill)
    pub fill_latency_ms: Histogram,
    /// Gauge for current number of pending orders
    pub pending_orders_count: Gauge,
}

impl PaperTradingMetrics {
    /// Create and register paper trading metrics with the provided registry.
    ///
    /// # Arguments
    /// * `registry` - The Prometheus registry to register metrics with
    ///
    /// # Returns
    /// A new `PaperTradingMetrics` instance, or an error if registration fails
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let orders_placed_opts = Opts::new(
            "paper_trading_orders_placed_total",
            "Total number of paper trading limit orders placed",
        );
        let orders_placed =
            register_counter_vec_with_registry!(orders_placed_opts, &["side"], registry)?;

        let orders_cancelled_opts = Opts::new(
            "paper_trading_orders_cancelled_total",
            "Total number of paper trading orders cancelled",
        );
        let orders_cancelled =
            register_counter_vec_with_registry!(orders_cancelled_opts, &[], registry)?;

        let orders_filled_opts = Opts::new(
            "paper_trading_orders_filled_total",
            "Total number of paper trading order fills (partial or full)",
        );
        let orders_filled =
            register_counter_vec_with_registry!(orders_filled_opts, &["side"], registry)?;

        let fill_volume_opts = Opts::new(
            "paper_trading_fill_volume_total",
            "Total fill volume in base units",
        );
        let fill_volume =
            register_counter_vec_with_registry!(fill_volume_opts, &["side"], registry)?;

        // Buckets suitable for market making latencies (sub-second to minutes)
        let fill_latency_ms = register_histogram_with_registry!(
            histogram_opts!(
                "paper_trading_fill_latency_ms",
                "Latency from order placement to fill in milliseconds",
                vec![
                    10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0, 30000.0, 60000.0, 300000.0
                ]
            ),
            registry
        )?;

        let pending_orders_opts = Opts::new(
            "paper_trading_pending_orders",
            "Current number of pending paper trading orders",
        );
        let pending_orders_count = register_gauge_with_registry!(pending_orders_opts, registry)?;

        Ok(Self {
            orders_placed,
            orders_cancelled,
            orders_filled,
            fill_volume,
            fill_latency_ms,
            pending_orders_count,
        })
    }

    /// Record a limit order placement.
    ///
    /// # Arguments
    /// * `side` - "buy" or "sell"
    pub fn record_order_placed(&self, side: &str) {
        self.orders_placed.with_label_values(&[side]).inc();
    }

    /// Record an order cancellation.
    pub fn record_order_cancelled(&self) {
        self.orders_cancelled.with_label_values(&[]).inc();
    }

    /// Record an order fill.
    ///
    /// # Arguments
    /// * `side` - "buy" or "sell"
    /// * `fill_size` - The size of the fill in base units
    /// * `latency_ms` - Time from placement to fill in milliseconds
    pub fn record_fill(&self, side: &str, fill_size: f64, latency_ms: f64) {
        self.orders_filled.with_label_values(&[side]).inc();
        self.fill_volume
            .with_label_values(&[side])
            .inc_by(fill_size);
        self.fill_latency_ms.observe(latency_ms);
    }

    /// Update the pending orders gauge.
    ///
    /// # Arguments
    /// * `count` - Current number of pending orders
    pub fn set_pending_orders(&self, count: usize) {
        self.pending_orders_count.set(count as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let registry = Registry::new();
        let metrics = PaperTradingMetrics::new(&registry).unwrap();

        // Verify metrics were registered
        assert!(registry.gather().len() > 0);

        // Test recording operations don't panic
        metrics.record_order_placed("buy");
        metrics.record_order_placed("sell");
        metrics.record_order_cancelled();
        metrics.record_fill("buy", 100.0, 50.0);
        metrics.set_pending_orders(5);
    }

    #[test]
    fn test_metrics_cannot_be_registered_twice() {
        let registry = Registry::new();

        let _metrics1 = PaperTradingMetrics::new(&registry).unwrap();
        let result = PaperTradingMetrics::new(&registry);

        // Second registration should fail
        assert!(result.is_err());
    }
}
