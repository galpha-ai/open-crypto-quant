//! Prometheus metrics for Polymarket subscriber
//!
//! This module defines all metrics exposed by the polymarket-sub service.

use prometheus::{
    register_counter_vec_with_registry, register_gauge_vec_with_registry, register_gauge_with_registry,
    register_histogram_vec_with_registry, CounterVec, Gauge, GaugeVec, HistogramVec, Registry,
};

/// All Prometheus metrics for the Polymarket subscriber
pub struct Metrics {
    /// Total WebSocket events received by event type
    pub ws_events_received: CounterVec,
    /// Total trades published by market and side
    pub trades_published: CounterVec,
    /// Total parsing errors by event type
    pub parsing_errors: CounterVec,
    /// Total Redis publish failures by queue
    pub redis_publish_failures: CounterVec,
    /// WebSocket connection status (1 = connected, 0 = disconnected)
    pub ws_connection_status: Gauge,
    /// Trade latency from timestamp to publish
    pub trade_latency: HistogramVec,

    // Market discovery metrics
    /// Number of active crypto binary markets discovered
    pub markets_discovered: Gauge,
    /// Total API fetch failures by error type
    pub api_fetch_failures: CounterVec,
    /// Current number of asset IDs subscribed to WebSocket
    pub assets_subscribed: Gauge,
    /// Total subscription update operations
    pub subscription_updates: CounterVec,
    /// Total markets dropped due to subscription limit
    pub markets_dropped: CounterVec,

    // Health monitoring metrics
    /// Rolling event rate (events/minute over last 60 seconds)
    pub events_per_minute: Gauge,
    /// Total health check failures leading to process exit
    pub health_check_failures: CounterVec,
    /// Unix timestamp of last received event
    pub last_event_timestamp: Gauge,

    // Orderbook metrics
    /// Count of price_change events received
    pub price_changes_received: CounterVec,
    /// Count of individual price changes published
    pub price_changes_published: CounterVec,
    /// Count of orderbook removals (size=0)
    pub orderbook_removals: CounterVec,
    /// Best bid price gauge
    pub best_bid: GaugeVec,
    /// Best ask price gauge
    pub best_ask: GaugeVec,
    /// Orderbook update latency histogram
    pub orderbook_latency: HistogramVec,

    // Book snapshot metrics
    /// Count of book snapshot events received
    pub book_snapshots_received: CounterVec,
    /// Count of book snapshots published
    pub book_snapshots_published: CounterVec,
    /// Book snapshot latency histogram
    pub book_snapshot_latency: HistogramVec,
}

impl Metrics {
    /// Create a new Metrics instance and register with the given registry
    pub fn new(registry: &Registry) -> anyhow::Result<Self> {
        let ws_events_received = register_counter_vec_with_registry!(
            "polymarket_ws_events_received_total",
            "Total WebSocket events received by type",
            &["event_type"],
            registry
        )?;

        let trades_published = register_counter_vec_with_registry!(
            "polymarket_trades_published_total",
            "Total trades published by market and side",
            &["market", "side"],
            registry
        )?;

        let parsing_errors = register_counter_vec_with_registry!(
            "polymarket_parsing_errors_total",
            "Total parsing errors by event type",
            &["event_type"],
            registry
        )?;

        let redis_publish_failures = register_counter_vec_with_registry!(
            "polymarket_redis_publish_failures_total",
            "Total Redis publish failures by queue",
            &["queue"],
            registry
        )?;

        let ws_connection_status = register_gauge_with_registry!(
            "polymarket_ws_connection_status",
            "WebSocket connection status (1 = connected, 0 = disconnected)",
            registry
        )?;

        let trade_latency = register_histogram_vec_with_registry!(
            "polymarket_trade_latency_seconds",
            "Trade latency from timestamp to publish",
            &["market"],
            vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0],
            registry
        )?;

        // Market discovery metrics
        let markets_discovered = register_gauge_with_registry!(
            "polymarket_markets_discovered",
            "Number of active crypto binary markets discovered",
            registry
        )?;

        let api_fetch_failures = register_counter_vec_with_registry!(
            "polymarket_api_fetch_failures_total",
            "Total API fetch failures by error type",
            &["error_type"],
            registry
        )?;

        let assets_subscribed = register_gauge_with_registry!(
            "polymarket_assets_subscribed",
            "Current number of asset IDs subscribed to WebSocket",
            registry
        )?;

        let subscription_updates = register_counter_vec_with_registry!(
            "polymarket_subscription_updates_total",
            "Total subscription update operations",
            &["operation"],
            registry
        )?;

        let markets_dropped = register_counter_vec_with_registry!(
            "polymarket_markets_dropped_total",
            "Total markets dropped due to subscription limit",
            &["reason"],
            registry
        )?;

        // Health monitoring metrics
        let events_per_minute = register_gauge_with_registry!(
            "polymarket_events_per_minute",
            "Rolling event rate (events/minute over last 60 seconds)",
            registry
        )?;

        let health_check_failures = register_counter_vec_with_registry!(
            "polymarket_health_check_failures_total",
            "Total health check failures leading to process exit",
            &["reason"],
            registry
        )?;

        let last_event_timestamp = register_gauge_with_registry!(
            "polymarket_last_event_timestamp",
            "Unix timestamp of last received event",
            registry
        )?;

        // Orderbook metrics
        let price_changes_received = register_counter_vec_with_registry!(
            "polymarket_price_changes_received_total",
            "Total price_change events received from WebSocket",
            &["market"],
            registry
        )?;

        let price_changes_published = register_counter_vec_with_registry!(
            "polymarket_price_changes_published_total",
            "Total individual price changes published to Redis",
            &["market", "side"],
            registry
        )?;

        let orderbook_removals = register_counter_vec_with_registry!(
            "polymarket_orderbook_removals_total",
            "Total orderbook removals (size=0)",
            &["market", "side"],
            registry
        )?;

        let best_bid = register_gauge_vec_with_registry!(
            "polymarket_best_bid",
            "Current best bid price",
            &["market", "asset_id"],
            registry
        )?;

        let best_ask = register_gauge_vec_with_registry!(
            "polymarket_best_ask",
            "Current best ask price",
            &["market", "asset_id"],
            registry
        )?;

        let orderbook_latency = register_histogram_vec_with_registry!(
            "polymarket_orderbook_latency_seconds",
            "Orderbook update latency (seconds)",
            &["market"],
            vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0],
            registry
        )?;

        // Book snapshot metrics
        let book_snapshots_received = register_counter_vec_with_registry!(
            "polymarket_book_snapshots_received_total",
            "Total book snapshot events received from WebSocket",
            &["market"],
            registry
        )?;

        let book_snapshots_published = register_counter_vec_with_registry!(
            "polymarket_book_snapshots_published_total",
            "Total book snapshots published to Redis",
            &["market"],
            registry
        )?;

        let book_snapshot_latency = register_histogram_vec_with_registry!(
            "polymarket_book_snapshot_latency_seconds",
            "Book snapshot latency (seconds)",
            &["market"],
            vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0],
            registry
        )?;

        Ok(Self {
            ws_events_received,
            trades_published,
            parsing_errors,
            redis_publish_failures,
            ws_connection_status,
            trade_latency,
            markets_discovered,
            api_fetch_failures,
            assets_subscribed,
            subscription_updates,
            markets_dropped,
            events_per_minute,
            health_check_failures,
            last_event_timestamp,
            price_changes_received,
            price_changes_published,
            orderbook_removals,
            best_bid,
            best_ask,
            orderbook_latency,
            book_snapshots_received,
            book_snapshots_published,
            book_snapshot_latency,
        })
    }
}
