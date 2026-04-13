use prometheus::{
    Counter, CounterVec, Histogram, Registry, histogram_opts, opts,
    register_counter_vec_with_registry, register_counter_with_registry,
    register_histogram_with_registry,
};

// Solana-specific metrics
pub struct Metrics {
    /// Counter for processed transactions
    pub transactions_processed: Counter,
    /// Counter for transaction send failures with labels
    pub send_failures: CounterVec,
    /// Histogram for block time latency (time between block production and our processing)
    pub block_time_latency: Histogram,
    /// Counter for parsed trades, labeled by DEX
    pub parsed_trades_total: CounterVec,
    /// Counter for Redis publish operations, labeled by publisher type and status
    pub redis_publish_total: CounterVec,
    /// Counter for inactivity watchdog exits
    pub inactivity_exits_total: Counter,
}

impl Metrics {
    /// Creates a new Metrics instance and registers it with the provided registry
    pub fn new(registry: &Registry) -> Self {
        let transactions_processed = register_counter_with_registry!(
            opts!(
                "tx_sub_transactions_processed",
                "Total number of transactions processed"
            ),
            registry,
        )
        .expect("Failed to create transactions_processed counter");

        let send_failures = register_counter_vec_with_registry!(
            opts!(
                "tx_sub_send_failures",
                "Number of failures when sending transactions"
            ),
            &["reason"],
            registry,
        )
        .expect("Failed to create send_failures counter vec");

        // Block time latency histogram with buckets in milliseconds
        let block_time_latency = register_histogram_with_registry!(
            histogram_opts!(
                "tx_sub_block_time_latency_ms",
                "Latency between block production and receiving in milliseconds",
                vec![
                    10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0
                ]
            ),
            registry,
        )
        .expect("Failed to create block_time_latency histogram");

        let parsed_trades_total = register_counter_vec_with_registry!(
            opts!(
                "tx_sub_parsed_trades_total",
                "Total number of successfully parsed trades, labeled by DEX"
            ),
            &["dex"],
            registry,
        )
        .expect("Failed to create parsed_trades_total counter vec");

        let redis_publish_total = register_counter_vec_with_registry!(
            opts!(
                "tx_sub_redis_publish_total",
                "Total number of Redis publish operations"
            ),
            &["publisher_type", "status"],
            registry,
        )
        .expect("Failed to create redis_publish_total counter vec");

        let inactivity_exits_total = register_counter_with_registry!(
            opts!(
                "tx_sub_inactivity_exits_total",
                "Total number of process exits due to inactivity timeout"
            ),
            registry,
        )
        .expect("Failed to create inactivity_exits_total counter");

        Self {
            transactions_processed,
            send_failures,
            block_time_latency,
            parsed_trades_total,
            redis_publish_total,
            inactivity_exits_total,
        }
    }
}
