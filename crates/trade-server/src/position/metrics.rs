use std::sync::Arc;

use anyhow::Result;
use prometheus::{CounterVec, Opts, Registry, register_counter_vec_with_registry};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetricsError {
    #[error("Prometheus registration error: {0}")]
    Prometheus(#[from] prometheus::Error),
}

#[derive(Clone, Debug)]
pub struct PositionManagerMetrics {
    pub update_price_calls: Arc<CounterVec>,
    pub timer_events_handled: Arc<CounterVec>,
    pub order_rejected_handled: Arc<CounterVec>,
    pub orphan_positions_added: Arc<CounterVec>,
}

impl PositionManagerMetrics {
    pub fn new(registry: &Registry) -> Result<Self, MetricsError> {
        let update_price_opts = Opts::new(
            "position_manager_update_price_calls",
            "Number of calls to update_price",
        );
        let update_price_calls =
            register_counter_vec_with_registry!(update_price_opts, &[], registry)?;

        let timer_events_opts = Opts::new(
            "position_manager_timer_events_handled",
            "Number of timer events handled",
        );
        let timer_events_handled =
            register_counter_vec_with_registry!(timer_events_opts, &[], registry)?;

        let order_rejected_opts = Opts::new(
            "position_manager_order_rejected_handled",
            "Number of OrderRejected events handled",
        );
        let order_rejected_handled =
            register_counter_vec_with_registry!(order_rejected_opts, &["reason"], registry)?;

        let orphan_positions_opts = Opts::new(
            "position_manager_orphan_positions_added",
            "Number of orphan positions added to the manager",
        );
        let orphan_positions_added =
            register_counter_vec_with_registry!(orphan_positions_opts, &[], registry)?;

        Ok(Self {
            update_price_calls: Arc::new(update_price_calls),
            timer_events_handled: Arc::new(timer_events_handled),
            order_rejected_handled: Arc::new(order_rejected_handled),
            orphan_positions_added: Arc::new(orphan_positions_added),
        })
    }
}
