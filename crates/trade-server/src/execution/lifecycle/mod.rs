mod engine;
pub mod metrics;

pub use engine::{LifecycleCancelRequestOutcome, LifecycleEngine, LifecycleError};
pub use metrics::LifecycleMetrics;

use serde::{Deserialize, Serialize};

use crate::execution::events::{OrderSide, TimeInForce};

/// Canonical lifecycle state for one logical limit-order intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    SubmitPending,
    Open,
    CancelPending,
    Terminal(TerminalReason),
}

impl LifecycleState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal(_))
    }
}

/// Terminal lifecycle outcomes for a logical limit-order intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalReason {
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

/// Shared lifecycle record keyed by `lifecycle_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecycleOrder {
    pub lifecycle_id: String,
    pub client_order_id: String,
    pub venue_order_id: Option<String>,
    pub mint: String,
    pub market: Option<String>,
    pub side: OrderSide,
    pub price: f64,
    pub original_size: f64,
    pub remaining_size: f64,
    pub time_in_force: TimeInForce,
    pub state: LifecycleState,
    pub cancel_requested: bool,
    pub signal_id: Option<String>,
}

impl LifecycleOrder {
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }
}

/// Lookup key for lifecycle records by internal or external identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LifecycleOrderRef {
    LifecycleId(String),
    ClientOrderId(String),
    VenueOrderId(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecyclePlaceRequest {
    pub lifecycle_id: Option<String>,
    pub client_order_id: String,
    pub mint: String,
    pub market: Option<String>,
    pub side: OrderSide,
    pub price: f64,
    pub size: f64,
    pub time_in_force: TimeInForce,
    pub signal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePlaceSuccess {
    pub order_ref: LifecycleOrderRef,
    pub venue_order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePlaceRejected {
    pub order_ref: LifecycleOrderRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCancelRequest {
    pub order_ref: LifecycleOrderRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCancelConfirmed {
    pub order_ref: LifecycleOrderRef,
    pub venue_order_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnknownOrderCancelPolicy {
    Strict,
    #[default]
    Idempotent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecycleFillEvidence {
    pub order_ref: LifecycleOrderRef,
    pub filled_size: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleTerminalEvidence {
    pub order_ref: LifecycleOrderRef,
    pub terminal_reason: TerminalReason,
    pub venue_order_id: Option<String>,
}
