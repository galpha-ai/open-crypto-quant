//! Metrics for the order status poller.

use prometheus::{Counter, CounterVec, Gauge, Histogram, HistogramOpts, Opts, Registry};

/// Metrics for the order status poller.
#[derive(Clone)]
pub struct PollerMetrics {
    /// Orders currently being monitored
    pub monitored_orders: Gauge,

    /// Poll cycles completed
    pub poll_cycles_total: Counter,

    /// Poll cycle duration histogram
    pub poll_cycle_duration_seconds: Histogram,

    /// Orders polled per cycle
    pub orders_polled_per_cycle: Histogram,

    /// Fills detected
    pub fills_detected_total: Counter,

    /// API errors by type
    pub api_errors_total: CounterVec,

    /// Rate limiter wait time
    pub rate_limit_wait_seconds: Histogram,

    /// Position syncs completed
    pub position_syncs_total: Counter,

    /// Position drift events
    pub position_drift_detected_total: Counter,
}

impl PollerMetrics {
    /// Create new metrics and register with the given registry.
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let monitored_orders = Gauge::with_opts(Opts::new(
            "polymarket_poller_monitored_orders",
            "Number of orders currently being monitored",
        ))?;
        registry.register(Box::new(monitored_orders.clone()))?;

        let poll_cycles_total = Counter::with_opts(Opts::new(
            "polymarket_poller_poll_cycles_total",
            "Total number of poll cycles completed",
        ))?;
        registry.register(Box::new(poll_cycles_total.clone()))?;

        let poll_cycle_duration_seconds = Histogram::with_opts(HistogramOpts::new(
            "polymarket_poller_poll_cycle_duration_seconds",
            "Duration of poll cycles in seconds",
        ))?;
        registry.register(Box::new(poll_cycle_duration_seconds.clone()))?;

        let orders_polled_per_cycle = Histogram::with_opts(
            HistogramOpts::new(
                "polymarket_poller_orders_polled_per_cycle",
                "Number of orders polled per cycle",
            )
            .buckets(vec![0.0, 1.0, 5.0, 10.0, 20.0, 50.0, 100.0]),
        )?;
        registry.register(Box::new(orders_polled_per_cycle.clone()))?;

        let fills_detected_total = Counter::with_opts(Opts::new(
            "polymarket_poller_fills_detected_total",
            "Total number of fills detected",
        ))?;
        registry.register(Box::new(fills_detected_total.clone()))?;

        let api_errors_total = CounterVec::new(
            Opts::new(
                "polymarket_poller_api_errors_total",
                "Total API errors by type",
            ),
            &["error_type"],
        )?;
        registry.register(Box::new(api_errors_total.clone()))?;

        let rate_limit_wait_seconds = Histogram::with_opts(HistogramOpts::new(
            "polymarket_poller_rate_limit_wait_seconds",
            "Time spent waiting for rate limiter in seconds",
        ))?;
        registry.register(Box::new(rate_limit_wait_seconds.clone()))?;

        let position_syncs_total = Counter::with_opts(Opts::new(
            "polymarket_poller_position_syncs_total",
            "Total number of position syncs completed",
        ))?;
        registry.register(Box::new(position_syncs_total.clone()))?;

        let position_drift_detected_total = Counter::with_opts(Opts::new(
            "polymarket_poller_position_drift_detected_total",
            "Total number of position drift events detected",
        ))?;
        registry.register(Box::new(position_drift_detected_total.clone()))?;

        Ok(Self {
            monitored_orders,
            poll_cycles_total,
            poll_cycle_duration_seconds,
            orders_polled_per_cycle,
            fills_detected_total,
            api_errors_total,
            rate_limit_wait_seconds,
            position_syncs_total,
            position_drift_detected_total,
        })
    }

    /// Create metrics without registration (for testing).
    pub fn unregistered() -> Self {
        Self {
            monitored_orders: Gauge::new(
                "polymarket_poller_monitored_orders",
                "Number of orders currently being monitored",
            )
            .unwrap(),
            poll_cycles_total: Counter::new(
                "polymarket_poller_poll_cycles_total",
                "Total number of poll cycles completed",
            )
            .unwrap(),
            poll_cycle_duration_seconds: Histogram::with_opts(HistogramOpts::new(
                "polymarket_poller_poll_cycle_duration_seconds",
                "Duration of poll cycles in seconds",
            ))
            .unwrap(),
            orders_polled_per_cycle: Histogram::with_opts(HistogramOpts::new(
                "polymarket_poller_orders_polled_per_cycle",
                "Number of orders polled per cycle",
            ))
            .unwrap(),
            fills_detected_total: Counter::new(
                "polymarket_poller_fills_detected_total",
                "Total number of fills detected",
            )
            .unwrap(),
            api_errors_total: CounterVec::new(
                Opts::new(
                    "polymarket_poller_api_errors_total",
                    "Total API errors by type",
                ),
                &["error_type"],
            )
            .unwrap(),
            rate_limit_wait_seconds: Histogram::with_opts(HistogramOpts::new(
                "polymarket_poller_rate_limit_wait_seconds",
                "Time spent waiting for rate limiter in seconds",
            ))
            .unwrap(),
            position_syncs_total: Counter::new(
                "polymarket_poller_position_syncs_total",
                "Total number of position syncs completed",
            )
            .unwrap(),
            position_drift_detected_total: Counter::new(
                "polymarket_poller_position_drift_detected_total",
                "Total number of position drift events detected",
            )
            .unwrap(),
        }
    }

    /// Record a poll cycle completion.
    pub fn record_poll_cycle(&self, duration_secs: f64, orders_polled: usize, fills: usize) {
        self.poll_cycles_total.inc();
        self.poll_cycle_duration_seconds.observe(duration_secs);
        self.orders_polled_per_cycle.observe(orders_polled as f64);
        self.fills_detected_total.inc_by(fills as f64);
    }

    /// Record an API error.
    pub fn record_api_error(&self, error_type: &str) {
        self.api_errors_total.with_label_values(&[error_type]).inc();
    }

    /// Record rate limiter wait time.
    pub fn record_rate_limit_wait(&self, duration_secs: f64) {
        self.rate_limit_wait_seconds.observe(duration_secs);
    }

    /// Update the monitored orders count.
    pub fn set_monitored_orders(&self, count: usize) {
        self.monitored_orders.set(count as f64);
    }

    /// Record a position sync.
    pub fn record_position_sync(&self, drift_detected: bool) {
        self.position_syncs_total.inc();
        if drift_detected {
            self.position_drift_detected_total.inc();
        }
    }
}

impl Default for PollerMetrics {
    fn default() -> Self {
        Self::unregistered()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = PollerMetrics::unregistered();
        assert_eq!(metrics.monitored_orders.get(), 0.0);
    }

    #[test]
    fn test_record_poll_cycle() {
        let metrics = PollerMetrics::unregistered();
        metrics.record_poll_cycle(0.5, 10, 2);

        assert_eq!(metrics.poll_cycles_total.get(), 1.0);
        assert_eq!(metrics.fills_detected_total.get(), 2.0);
    }

    #[test]
    fn test_record_api_error() {
        let metrics = PollerMetrics::unregistered();
        metrics.record_api_error("timeout");
        metrics.record_api_error("timeout");
        metrics.record_api_error("rate_limit");

        assert_eq!(
            metrics
                .api_errors_total
                .with_label_values(&["timeout"])
                .get(),
            2.0
        );
        assert_eq!(
            metrics
                .api_errors_total
                .with_label_values(&["rate_limit"])
                .get(),
            1.0
        );
    }

    #[test]
    fn test_set_monitored_orders() {
        let metrics = PollerMetrics::unregistered();
        metrics.set_monitored_orders(5);
        assert_eq!(metrics.monitored_orders.get(), 5.0);

        metrics.set_monitored_orders(3);
        assert_eq!(metrics.monitored_orders.get(), 3.0);
    }
}
