//! Order status poller for monitoring pending orders via REST API.
//!
//! This module provides centralized rate limit management and reliable fill detection
//! without WebSocket dependencies. It is organized into focused submodules:
//!
//! - `poller` - Core order polling functionality
//! - `merge_executor` - Merge/redemption execution
//! - `balance_api` - Balance query utilities
//! - `data_api` - Data API utilities for merge indexing

mod balance_api;
mod config;
mod data_api;
mod merge_config;
mod merge_executor;
mod merge_metrics;
mod metrics;
mod poller;
mod rate_limiter;
mod types;

pub use config::PollerConfig;
pub use merge_config::MergeConfig;
pub use merge_executor::MergeExecutor;
pub use merge_metrics::MergeMetrics;
pub use metrics::PollerMetrics;
pub use poller::OrderStatusPoller;
pub use rate_limiter::RateLimiter;
pub use types::{MonitoredOrder, PollCycleResult, PositionSnapshot, PositionSyncResult};

// Re-export balance API utilities for external use
pub use balance_api::{get_token_balance, get_usdc_balance, refresh_clob_balance};

// Re-export data API utilities for external use
pub use data_api::wait_for_merge_indexed;

#[cfg(test)]
mod poller_test;
