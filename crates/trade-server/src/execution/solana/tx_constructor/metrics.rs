use std::sync::Arc;

use anyhow::Result;
use prometheus::{
    CounterVec, Histogram, Opts, Registry, histogram_opts, register_counter_vec_with_registry,
    register_histogram_with_registry,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetricsError {
    #[error("Prometheus registration error: {0}")]
    Prometheus(#[from] prometheus::Error),
}

#[derive(Clone, Debug)]
pub struct TxnMakerMetrics {
    pub requests_total: Arc<CounterVec>,
    pub request_latency_milliseconds: Arc<Histogram>,
}

impl TxnMakerMetrics {
    pub fn new(registry: &Registry) -> Result<Self, MetricsError> {
        let requests_opts = Opts::new(
            "txn_maker_requests_total",
            "Number of requests sent to TxnMaker service",
        );
        let requests_total =
            register_counter_vec_with_registry!(requests_opts, &["status"], registry)?;

        let request_latency_milliseconds = register_histogram_with_registry!(
            histogram_opts!(
                "txn_maker_request_latency_milliseconds",
                "Latency of requests to TxnMaker service in milliseconds",
                vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0],
            ),
            registry,
        )?;

        Ok(Self {
            requests_total: Arc::new(requests_total),
            request_latency_milliseconds: Arc::new(request_latency_milliseconds),
        })
    }
}
