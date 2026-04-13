//! Reconciliation modules for position and order management.
//!
//! This module provides:
//! - **Order Reconciliation**: Intent-based order reconciliation for orderbook trading
//! - **Position Reconciliation**: Periodic position sync with exchange
//!
//! ## Order Reconciliation
//!
//! The order reconciliation engine implements an intent-based trading model where:
//! - Strategies express desired order book state via `OrderIntent`
//! - The engine diffs current state against desired state
//! - Orders are generated to reach the desired state (cancels then placements)
//!
//! ## Position Reconciliation
//!
//! The position reconciler periodically queries the exchange for actual position
//! state and reconciles it with the local position manager. This catches any drift
//! between expected and actual positions (e.g., from missed fill events).
//!
//! ## Example: Order Reconciliation
//!
//! ```ignore
//! use trade_server::position::reconciliation::ReconciliationEngine;
//!
//! let engine = ReconciliationEngine::new();
//!
//! // Get current pending orders for the mint
//! let current_orders = manager.get_pending_orders_for_mint("token123");
//!
//! // Compute the diff
//! let orders = engine.compute_reconciliation(&intent, &current_orders)?;
//!
//! // Execute the generated orders
//! for order in orders {
//!     executor.execute_order(order).await?;
//! }
//! ```
//!
//! ## Example: Position Reconciliation
//!
//! ```ignore
//! use trade_server::position::reconciliation::{PositionReconciler, PositionReconcilerConfig};
//!
//! let config = PositionReconcilerConfig::default()
//!     .with_interval(Duration::from_secs(30))
//!     .with_drift_threshold(0.01);
//!
//! let reconciler = PositionReconcilerBuilder::new()
//!     .config(config)
//!     .position_querier(querier)
//!     .position_manager(manager)
//!     .event_coordinator(coordinator)
//!     .build();
//!
//! // Start background reconciliation
//! reconciler.start();
//! ```

mod engine;
mod position_reconciler;
mod validation;

pub use engine::ReconciliationEngine;
pub use position_reconciler::{
    PositionReconciler, PositionReconcilerBuilder, PositionReconcilerConfig,
};
pub use validation::validate_intent;
