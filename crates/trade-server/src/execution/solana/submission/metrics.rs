use std::sync::Arc;

use anyhow::Result;
use prometheus::{
    Counter, Gauge, Opts, Registry, register_counter_with_registry, register_gauge_with_registry,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetricsError {
    #[error("Prometheus registration error: {0}")]
    Prometheus(#[from] prometheus::Error),
}

#[derive(Clone, Debug)]
pub struct JitoMetrics {
    pub bundle_submission_attempts_total: Arc<Counter>,
    pub bundle_landed_total: Arc<Counter>,
    pub bundle_landing_rate: Arc<Gauge>,
}

impl JitoMetrics {
    pub fn new(registry: &Registry) -> Result<Self, MetricsError> {
        let bundle_submission_attempts_opts = Opts::new(
            "jito_bundle_submission_attempts_total",
            "Total number of Jito bundle submission attempts",
        );
        let bundle_submission_attempts_total =
            register_counter_with_registry!(bundle_submission_attempts_opts, registry)?;

        let bundle_landed_opts = Opts::new(
            "jito_bundle_landed_total",
            "Total number of Jito bundles that successfully landed",
        );
        let bundle_landed_total = register_counter_with_registry!(bundle_landed_opts, registry)?;

        let bundle_landing_rate_opts = Opts::new(
            "jito_bundle_landing_rate",
            "Rate of successfully landed Jito bundles",
        );
        let bundle_landing_rate =
            register_gauge_with_registry!(bundle_landing_rate_opts, registry)?;

        Ok(Self {
            bundle_submission_attempts_total: Arc::new(bundle_submission_attempts_total),
            bundle_landed_total: Arc::new(bundle_landed_total),
            bundle_landing_rate: Arc::new(bundle_landing_rate),
        })
    }

    pub fn update_landing_rate(&self) {
        let attempts = self.bundle_submission_attempts_total.get();
        let landed = self.bundle_landed_total.get();

        if attempts > 0.0 {
            let rate = landed / attempts;
            self.bundle_landing_rate.set(rate);
        }
    }
}
