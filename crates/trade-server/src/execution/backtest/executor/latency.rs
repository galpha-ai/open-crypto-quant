use chrono::{DateTime, Duration, Utc};
use rand::Rng;

use super::BacktestOrderExecutor;

impl BacktestOrderExecutor {
    pub(super) async fn set_current_time(&self, now: DateTime<Utc>) {
        let mut current = self.current_time.lock().await;
        *current = Some(now);
    }

    pub(super) async fn current_time(&self) -> DateTime<Utc> {
        let current = self.current_time.lock().await;
        current.as_ref().copied().unwrap_or_else(Utc::now)
    }

    pub(super) fn sample_place_latency_ms(&self) -> u64 {
        match &self.latency_config {
            Some(config) => {
                if let Some(rng) = &self.latency_rng {
                    let mut rng = rng.lock().expect("latency RNG mutex poisoned");
                    rng.gen_range(config.min_place_latency_ms..=config.max_place_latency_ms)
                } else {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(config.min_place_latency_ms..=config.max_place_latency_ms)
                }
            }
            None => 0,
        }
    }

    pub(super) fn sample_cancel_latency_ms(&self) -> u64 {
        match &self.latency_config {
            Some(config) => {
                let min_cancel = config
                    .min_cancel_latency_ms
                    .unwrap_or(config.min_place_latency_ms);
                let max_cancel = config
                    .max_cancel_latency_ms
                    .unwrap_or(config.max_place_latency_ms);
                if let Some(rng) = &self.latency_rng {
                    let mut rng = rng.lock().expect("latency RNG mutex poisoned");
                    rng.gen_range(min_cancel..=max_cancel)
                } else {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(min_cancel..=max_cancel)
                }
            }
            None => 0,
        }
    }

    /// Compute the eligible_for_fills_at timestamp for a new order.
    ///
    /// If latency simulation is enabled, adds a random placement latency within the configured
    /// range. Otherwise, returns the order timestamp (immediate eligibility).
    pub(super) fn compute_eligibility_time(&self, order_timestamp: DateTime<Utc>) -> DateTime<Utc> {
        let latency_ms = self.sample_place_latency_ms();
        order_timestamp + Duration::milliseconds(latency_ms as i64)
    }
}
