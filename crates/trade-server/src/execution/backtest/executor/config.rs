use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex, atomic::AtomicU64},
};

use rand::{SeedableRng, rngs::StdRng};
use tokio::sync::Mutex;

use crate::config::LatencySimulationConfig;
use crate::execution::lifecycle::LifecycleEngine;

use super::BacktestOrderExecutor;

/// Builder for constructing a `BacktestOrderExecutor` with a fluent API.
///
/// # Example
/// ```ignore
/// let executor = BacktestOrderExecutor::builder()
///     .buy_slippage(0.01)
///     .sell_slippage(0.01)
///     .enforce_inventory_constraints(true)
///     .latency_config(Some(latency_config))
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct BacktestOrderExecutorBuilder {
    buy_slippage: f64,
    sell_slippage: f64,
    enforce_inventory_constraints: bool,
    latency_config: Option<LatencySimulationConfig>,
}

impl BacktestOrderExecutorBuilder {
    /// Create a new builder with default values.
    ///
    /// Defaults:
    /// - `buy_slippage`: 0.0
    /// - `sell_slippage`: 0.0
    /// - `enforce_inventory_constraints`: true
    /// - `latency_config`: None
    pub fn new() -> Self {
        Self {
            buy_slippage: 0.0,
            sell_slippage: 0.0,
            enforce_inventory_constraints: true,
            latency_config: None,
        }
    }

    /// Set the slippage percentage for buy orders (e.g., 0.01 for 1%).
    pub fn buy_slippage(mut self, slippage: f64) -> Self {
        self.buy_slippage = slippage;
        self
    }

    /// Set the slippage percentage for sell orders (e.g., 0.01 for 1%).
    pub fn sell_slippage(mut self, slippage: f64) -> Self {
        self.sell_slippage = slippage;
        self
    }

    /// Set both buy and sell slippage to the same value.
    pub fn slippage(mut self, slippage: f64) -> Self {
        self.buy_slippage = slippage;
        self.sell_slippage = slippage;
        self
    }

    /// Set whether to enforce inventory constraints on sell orders.
    ///
    /// When enabled (default), sell orders require positive inventory to fill,
    /// preventing unrealistic "naked shorting".
    pub fn enforce_inventory_constraints(mut self, enforce: bool) -> Self {
        self.enforce_inventory_constraints = enforce;
        self
    }

    /// Set the latency simulation configuration for realistic fill modeling.
    pub fn latency_config(mut self, config: Option<LatencySimulationConfig>) -> Self {
        self.latency_config = config;
        self
    }

    /// Build the `BacktestOrderExecutor` with the configured options.
    pub fn build(self) -> BacktestOrderExecutor {
        let latency_rng = self
            .latency_config
            .as_ref()
            .and_then(|config| config.seed)
            .map(|seed| Arc::new(StdMutex::new(StdRng::seed_from_u64(seed))));

        BacktestOrderExecutor {
            last_prices: Arc::new(Mutex::new(HashMap::new())),
            buy_slippage: self.buy_slippage,
            sell_slippage: self.sell_slippage,
            pending_orders: Arc::new(Mutex::new(HashMap::new())),
            order_id_counter: Arc::new(AtomicU64::new(1)),
            enforce_inventory_constraints: self.enforce_inventory_constraints,
            latency_config: self.latency_config,
            latency_rng,
            quote_lanes: Arc::new(Mutex::new(HashMap::new())),
            order_id_to_lane: Arc::new(Mutex::new(HashMap::new())),
            current_time: Arc::new(Mutex::new(None)),
            market_assets: Arc::new(Mutex::new(HashMap::new())),
            complement_by_asset: Arc::new(Mutex::new(HashMap::new())),
            lifecycle_engine: Arc::new(StdMutex::new(LifecycleEngine::new())),
        }
    }
}

impl BacktestOrderExecutor {
    /// Create a new builder for constructing a `BacktestOrderExecutor`.
    ///
    /// This is the preferred way to create a `BacktestOrderExecutor`.
    ///
    /// # Example
    /// ```ignore
    /// let executor = BacktestOrderExecutor::builder()
    ///     .buy_slippage(0.01)
    ///     .sell_slippage(0.01)
    ///     .enforce_inventory_constraints(true)
    ///     .latency_config(Some(latency_config))
    ///     .build();
    /// ```
    pub fn builder() -> BacktestOrderExecutorBuilder {
        BacktestOrderExecutorBuilder::new()
    }
}
