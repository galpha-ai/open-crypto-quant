use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, Registry, opts, register_counter,
    register_counter_vec, register_gauge, register_gauge_vec, register_histogram,
};

/// Metrics for tracking trade server performance and activity
pub struct TradeServerMetrics {
    /// Counter for signals processed, labeled by signal type
    pub signals_processed: CounterVec,
    /// Counter for orders executed
    pub orders_executed: CounterVec,
    /// Counter for position creation/closing events
    pub position_events: CounterVec,
    /// Gauge for current open positions
    pub open_positions: Gauge,
    /// Gauge for available quote currency balance
    pub available_quote: Gauge,
    /// Gauge for net cash flow (quote received - quote spent)
    pub net_cash_flow: Gauge,
    /// Gauge for cumulative quote currency received from sells
    pub total_quote_received: Gauge,
    /// Gauge for cumulative quote currency spent on buys
    pub total_quote_spent: Gauge,
    /// Gauge for total PnL including inventory mark-to-market
    pub total_pnl: Gauge,
    /// Gauge for total unrealized PnL from open positions
    pub total_unrealized_pnl: Gauge,
    /// Histogram for order execution latency
    pub execution_latency: Histogram,
    /// Counter for token events processed, labeled by event type
    pub token_events: CounterVec,
    /// Histogram for signal execution slot latency
    pub signal_execution_slot_latency: Histogram,

    // Orderbook-specific metrics
    /// Gauge for orderbook spread in price units, labeled by asset_id
    pub orderbook_spread: GaugeVec,
    /// Gauge for orderbook mid price, labeled by asset_id
    pub orderbook_mid_price: GaugeVec,
    /// Gauge for orderbook depth (total size on each side), labeled by asset_id and side
    pub orderbook_depth: GaugeVec,

    // Limit order metrics
    /// Counter for limit orders placed, labeled by venue and status
    pub limit_orders_placed: CounterVec,
    /// Counter for limit orders filled, labeled by venue
    pub limit_orders_filled: CounterVec,
    /// Counter for limit orders cancelled, labeled by venue
    pub limit_orders_cancelled: CounterVec,
    /// Histogram for limit order latency (placement to fill) in milliseconds
    pub limit_order_fill_latency: Histogram,
    /// Counter for stale events dropped due to high latency
    pub stale_events_dropped: Counter,
}

impl TradeServerMetrics {
    /// Creates and registers all metrics with the provided Prometheus registry
    ///
    /// # Arguments
    /// * `registry` - The Prometheus registry to register metrics with
    ///
    /// # Returns
    /// A new TradeServerMetrics instance
    pub fn new(registry: &Registry) -> Self {
        let signals_processed = register_counter_vec!(
            opts!(
                "trade_server_signals_processed",
                "Total number of signals processed"
            ),
            &["signal_type"]
        )
        .unwrap();

        let orders_executed = register_counter_vec!(
            opts!("trade_server_orders_executed", "Number of orders executed"),
            &["order_type", "result"]
        )
        .unwrap();

        let position_events = register_counter_vec!(
            opts!("trade_server_position_events", "Number of position events"),
            &["event_type"]
        )
        .unwrap();

        let open_positions = register_gauge!(opts!(
            "trade_server_open_positions",
            "Current number of open positions"
        ))
        .unwrap();

        let available_quote = register_gauge!(opts!(
            "trade_server_available_quote",
            "Current available quote currency balance"
        ))
        .unwrap();

        let net_cash_flow = register_gauge!(opts!(
            "trade_server_net_cash_flow",
            "Net cash flow (quote received minus quote spent)"
        ))
        .unwrap();

        let total_quote_received = register_gauge!(opts!(
            "trade_server_total_quote_received",
            "Cumulative quote currency received from all sells"
        ))
        .unwrap();

        let total_quote_spent = register_gauge!(opts!(
            "trade_server_total_quote_spent",
            "Cumulative quote currency spent on all buys"
        ))
        .unwrap();

        let total_pnl = register_gauge!(opts!(
            "trade_server_total_pnl",
            "Total PnL including inventory mark-to-market"
        ))
        .unwrap();

        let total_unrealized_pnl = register_gauge!(opts!(
            "trade_server_total_unrealized_pnl",
            "Total unrealized PnL from open positions"
        ))
        .unwrap();

        let execution_latency = register_histogram!(
            "trade_server_execution_latency_ms",
            "Latency for order execution in milliseconds",
            vec![
                10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0
            ]
        )
        .unwrap();

        let token_events = register_counter_vec!(
            opts!(
                "trade_server_token_events",
                "Number of token events processed"
            ),
            &["event_type"]
        )
        .unwrap();

        let signal_execution_slot_latency = register_histogram!(
            "trade_server_signal_execution_slot_latency",
            "Latency between signal slot and execution slot in slots",
            vec![0.0, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]
        )
        .unwrap();

        // Orderbook-specific metrics
        let orderbook_spread = register_gauge_vec!(
            opts!(
                "trade_server_orderbook_spread",
                "Current orderbook spread in price units"
            ),
            &["asset_id"]
        )
        .unwrap();

        let orderbook_mid_price = register_gauge_vec!(
            opts!(
                "trade_server_orderbook_mid_price",
                "Current orderbook mid price"
            ),
            &["asset_id"]
        )
        .unwrap();

        let orderbook_depth = register_gauge_vec!(
            opts!(
                "trade_server_orderbook_depth",
                "Total orderbook depth (size) on each side"
            ),
            &["asset_id", "side"]
        )
        .unwrap();

        // Limit order metrics
        let limit_orders_placed = register_counter_vec!(
            opts!(
                "trade_server_limit_orders_placed",
                "Number of limit orders placed"
            ),
            &["venue", "status"]
        )
        .unwrap();

        let limit_orders_filled = register_counter_vec!(
            opts!(
                "trade_server_limit_orders_filled",
                "Number of limit orders filled"
            ),
            &["venue"]
        )
        .unwrap();

        let limit_orders_cancelled = register_counter_vec!(
            opts!(
                "trade_server_limit_orders_cancelled",
                "Number of limit orders cancelled"
            ),
            &["venue"]
        )
        .unwrap();

        let limit_order_fill_latency = register_histogram!(
            "trade_server_limit_order_fill_latency_ms",
            "Latency from limit order placement to fill in milliseconds",
            vec![
                100.0, 500.0, 1000.0, 5000.0, 10000.0, 30000.0, 60000.0, 300000.0
            ]
        )
        .unwrap();

        let stale_events_dropped = register_counter!(opts!(
            "trade_server_stale_events_dropped",
            "Number of stale events dropped due to high latency (>100ms)"
        ))
        .unwrap();

        // Register all metrics with the provided registry
        registry
            .register(Box::new(signals_processed.clone()))
            .unwrap();
        registry
            .register(Box::new(orders_executed.clone()))
            .unwrap();
        registry
            .register(Box::new(position_events.clone()))
            .unwrap();
        registry.register(Box::new(open_positions.clone())).unwrap();
        registry
            .register(Box::new(available_quote.clone()))
            .unwrap();
        registry.register(Box::new(net_cash_flow.clone())).unwrap();
        registry
            .register(Box::new(total_quote_received.clone()))
            .unwrap();
        registry
            .register(Box::new(total_quote_spent.clone()))
            .unwrap();
        registry.register(Box::new(total_pnl.clone())).unwrap();
        registry
            .register(Box::new(total_unrealized_pnl.clone()))
            .unwrap();
        registry
            .register(Box::new(execution_latency.clone()))
            .unwrap();
        registry.register(Box::new(token_events.clone())).unwrap();
        registry
            .register(Box::new(signal_execution_slot_latency.clone()))
            .unwrap();
        registry
            .register(Box::new(orderbook_spread.clone()))
            .unwrap();
        registry
            .register(Box::new(orderbook_mid_price.clone()))
            .unwrap();
        registry
            .register(Box::new(orderbook_depth.clone()))
            .unwrap();
        registry
            .register(Box::new(limit_orders_placed.clone()))
            .unwrap();
        registry
            .register(Box::new(limit_orders_filled.clone()))
            .unwrap();
        registry
            .register(Box::new(limit_orders_cancelled.clone()))
            .unwrap();
        registry
            .register(Box::new(limit_order_fill_latency.clone()))
            .unwrap();
        registry
            .register(Box::new(stale_events_dropped.clone()))
            .unwrap();

        Self {
            signals_processed,
            orders_executed,
            position_events,
            open_positions,
            available_quote,
            net_cash_flow,
            total_quote_received,
            total_quote_spent,
            total_pnl,
            total_unrealized_pnl,
            execution_latency,
            token_events,
            signal_execution_slot_latency,
            orderbook_spread,
            orderbook_mid_price,
            orderbook_depth,
            limit_orders_placed,
            limit_orders_filled,
            limit_orders_cancelled,
            limit_order_fill_latency,
            stale_events_dropped,
        }
    }
}
