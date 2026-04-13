//! Prometheus metrics for spot price subscriber

use anyhow::Result;
use prometheus::{
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts, Registry,
};

/// Metrics for spot price subscriber
pub struct Metrics {
    // WebSocket metrics
    pub ws_messages_received: IntCounterVec,
    pub ws_connection_status: IntGauge,

    // Spot price metrics
    pub spot_prices_published: IntCounterVec,
    pub parsing_errors: IntCounterVec,
    pub spot_price_latency: HistogramVec,

    // Redis metrics
    pub redis_publish_failures: IntCounterVec,
    pub buffer_size: IntGauge,
    pub buffer_overflows: IntCounter,

    // Health metrics
    pub updates_per_minute: IntGauge,
    pub health_check_failures: IntCounter,
    pub last_update_timestamp: IntGauge,
}

impl Metrics {
    /// Create new metrics and register them with the provided registry
    pub fn new(registry: &Registry) -> Result<Self> {
        // WebSocket metrics
        let ws_messages_received = IntCounterVec::new(
            Opts::new(
                "ws_messages_received",
                "Total WebSocket messages received by message type",
            ),
            &["message_type"],
        )?;
        registry.register(Box::new(ws_messages_received.clone()))?;

        let ws_connection_status = IntGauge::new(
            "ws_connection_status",
            "WebSocket connection status (1=connected, 0=disconnected)",
        )?;
        registry.register(Box::new(ws_connection_status.clone()))?;

        // Spot price metrics
        let spot_prices_published = IntCounterVec::new(
            Opts::new(
                "spot_prices_published",
                "Total spot price updates published to Redis by symbol",
            ),
            &["symbol"],
        )?;
        registry.register(Box::new(spot_prices_published.clone()))?;

        let parsing_errors = IntCounterVec::new(
            Opts::new(
                "parsing_errors",
                "Total parsing errors by error type",
            ),
            &["error_type"],
        )?;
        registry.register(Box::new(parsing_errors.clone()))?;

        let spot_price_latency = HistogramVec::new(
            HistogramOpts::new(
                "spot_price_latency_seconds",
                "Latency from spot price timestamp to Redis publish (seconds)",
            )
            .buckets(vec![
                0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
            &["symbol"],
        )?;
        registry.register(Box::new(spot_price_latency.clone()))?;

        // Redis metrics
        let redis_publish_failures = IntCounterVec::new(
            Opts::new(
                "redis_publish_failures",
                "Total Redis publish failures by target",
            ),
            &["target_type", "target_name"],
        )?;
        registry.register(Box::new(redis_publish_failures.clone()))?;

        let buffer_size = IntGauge::new(
            "buffer_size",
            "Current number of buffered spot price updates",
        )?;
        registry.register(Box::new(buffer_size.clone()))?;

        let buffer_overflows = IntCounter::new(
            "buffer_overflows",
            "Total number of buffer overflow events (oldest events dropped)",
        )?;
        registry.register(Box::new(buffer_overflows.clone()))?;

        // Health metrics
        let updates_per_minute = IntGauge::new(
            "updates_per_minute",
            "Rolling average of spot price updates per minute",
        )?;
        registry.register(Box::new(updates_per_minute.clone()))?;

        let health_check_failures = IntCounter::new(
            "health_check_failures",
            "Total health check failures leading to process exit",
        )?;
        registry.register(Box::new(health_check_failures.clone()))?;

        let last_update_timestamp = IntGauge::new(
            "last_update_timestamp",
            "Unix timestamp (seconds) of last received spot price update",
        )?;
        registry.register(Box::new(last_update_timestamp.clone()))?;

        Ok(Self {
            ws_messages_received,
            ws_connection_status,
            spot_prices_published,
            parsing_errors,
            spot_price_latency,
            redis_publish_failures,
            buffer_size,
            buffer_overflows,
            updates_per_minute,
            health_check_failures,
            last_update_timestamp,
        })
    }
}
