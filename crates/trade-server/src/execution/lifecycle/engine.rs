use std::collections::HashMap;

use thiserror::Error;
use tracing::{debug, warn};
use uuid::Uuid;

use super::{
    LifecycleCancelConfirmed, LifecycleCancelRequest, LifecycleFillEvidence, LifecycleMetrics,
    LifecycleOrder, LifecycleOrderRef, LifecyclePlaceRejected, LifecyclePlaceRequest,
    LifecyclePlaceSuccess, LifecycleState, LifecycleTerminalEvidence, TerminalReason,
    UnknownOrderCancelPolicy,
};

#[derive(Debug, Error, PartialEq)]
pub enum LifecycleError {
    #[error("lifecycle_id already exists: {0}")]
    DuplicateLifecycleId(String),
    #[error("client_order_id already exists: {0}")]
    DuplicateClientOrderId(String),
    #[error("venue_order_id already exists: {0}")]
    DuplicateVenueOrderId(String),
    #[error("lifecycle record not found: {0:?}")]
    UnknownOrder(LifecycleOrderRef),
    #[error("invalid transition for {action} from {state:?}")]
    InvalidTransition {
        action: &'static str,
        state: LifecycleState,
    },
    #[error("conflicting venue_order_id for lifecycle: existing={existing}, incoming={incoming}")]
    ConflictingVenueOrderId { existing: String, incoming: String },
    #[error("fill size must be > 0, got {0}")]
    InvalidFillSize(f64),
    #[error("order size must be > 0, got {0}")]
    InvalidOrderSize(f64),
}

#[derive(Debug)]
pub enum LifecycleCancelRequestOutcome<'a> {
    Updated(&'a LifecycleOrder),
    IgnoredUnknownOrder,
}

/// Shared lifecycle state owner used by backtest/paper/live adapters.
#[derive(Debug)]
pub struct LifecycleEngine {
    orders: HashMap<String, LifecycleOrder>,
    client_to_lifecycle: HashMap<String, String>,
    venue_to_lifecycle: HashMap<String, String>,
    metrics: Option<LifecycleMetrics>,
    mode_label: String,
    unknown_order_cancel_policy: UnknownOrderCancelPolicy,
}

impl Default for LifecycleEngine {
    fn default() -> Self {
        Self {
            orders: HashMap::new(),
            client_to_lifecycle: HashMap::new(),
            venue_to_lifecycle: HashMap::new(),
            metrics: None,
            mode_label: "unknown".to_string(),
            unknown_order_cancel_policy: UnknownOrderCancelPolicy::Idempotent,
        }
    }
}

impl LifecycleEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_metrics(
        mut self,
        metrics: LifecycleMetrics,
        mode_label: impl Into<String>,
    ) -> Self {
        self.metrics = Some(metrics);
        self.mode_label = mode_label.into();
        self.refresh_inflight_cancel_queue_depth();
        self
    }

    pub fn with_unknown_order_cancel_policy(mut self, policy: UnknownOrderCancelPolicy) -> Self {
        self.unknown_order_cancel_policy = policy;
        self
    }

    pub fn len(&self) -> usize {
        self.orders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    pub fn get(&self, order_ref: &LifecycleOrderRef) -> Option<&LifecycleOrder> {
        let lifecycle_id = self.resolve_lifecycle_id(order_ref)?;
        self.orders.get(&lifecycle_id)
    }

    pub fn record_place_request(
        &mut self,
        request: LifecyclePlaceRequest,
    ) -> Result<&LifecycleOrder, LifecycleError> {
        if request.size <= 0.0 {
            return Err(LifecycleError::InvalidOrderSize(request.size));
        }

        if self
            .client_to_lifecycle
            .contains_key(request.client_order_id.as_str())
        {
            return Err(LifecycleError::DuplicateClientOrderId(
                request.client_order_id,
            ));
        }

        let lifecycle_id = request
            .lifecycle_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if self.orders.contains_key(lifecycle_id.as_str()) {
            return Err(LifecycleError::DuplicateLifecycleId(lifecycle_id));
        }

        let client_order_id = request.client_order_id;
        let order = LifecycleOrder {
            lifecycle_id: lifecycle_id.clone(),
            client_order_id: client_order_id.clone(),
            venue_order_id: None,
            mint: request.mint,
            market: request.market,
            side: request.side,
            price: request.price,
            original_size: request.size,
            remaining_size: request.size,
            time_in_force: request.time_in_force,
            state: LifecycleState::SubmitPending,
            cancel_requested: false,
            signal_id: request.signal_id,
        };

        self.orders.insert(lifecycle_id.clone(), order);
        self.client_to_lifecycle
            .insert(client_order_id, lifecycle_id.clone());
        self.refresh_inflight_cancel_queue_depth();

        Ok(self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order inserted"))
    }

    pub fn record_place_success(
        &mut self,
        success: LifecyclePlaceSuccess,
    ) -> Result<&LifecycleOrder, LifecycleError> {
        let lifecycle_id = self
            .resolve_lifecycle_id(&success.order_ref)
            .ok_or_else(|| LifecycleError::UnknownOrder(success.order_ref.clone()))?;

        let (state, cancel_requested, existing_venue_order_id) = {
            let snapshot = self
                .orders
                .get(lifecycle_id.as_str())
                .expect("lifecycle order exists");
            if snapshot.is_terminal() {
                self.emit_terminal_dedup_observability(snapshot, "place_success");
                return Ok(self
                    .orders
                    .get(lifecycle_id.as_str())
                    .expect("lifecycle order exists"));
            }
            (
                snapshot.state,
                snapshot.cancel_requested,
                snapshot.venue_order_id.clone(),
            )
        };

        if let Some(existing) = existing_venue_order_id.as_ref() {
            if existing != &success.venue_order_id {
                return Err(LifecycleError::ConflictingVenueOrderId {
                    existing: existing.clone(),
                    incoming: success.venue_order_id,
                });
            }
        }

        match state {
            LifecycleState::SubmitPending
            | LifecycleState::Open
            | LifecycleState::CancelPending => {}
            other => {
                return Err(LifecycleError::InvalidTransition {
                    action: "record_place_success",
                    state: other,
                });
            }
        }

        self.ensure_venue_mapping(lifecycle_id.as_str(), success.venue_order_id.as_str())?;

        let order = self
            .orders
            .get_mut(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        let from_state = state;
        order.venue_order_id = Some(success.venue_order_id);
        if matches!(state, LifecycleState::SubmitPending) {
            order.state = if cancel_requested {
                LifecycleState::CancelPending
            } else {
                LifecycleState::Open
            };
        }
        let to_state = order.state;

        let order = self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        self.emit_transition_observability(order, from_state, to_state);
        self.refresh_inflight_cancel_queue_depth();

        Ok(order)
    }

    pub fn record_place_rejected(
        &mut self,
        rejected: LifecyclePlaceRejected,
    ) -> Result<&LifecycleOrder, LifecycleError> {
        let lifecycle_id = self
            .resolve_lifecycle_id(&rejected.order_ref)
            .ok_or_else(|| LifecycleError::UnknownOrder(rejected.order_ref.clone()))?;

        let state = self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order exists")
            .state;

        match state {
            LifecycleState::SubmitPending => {
                let order = self
                    .orders
                    .get_mut(lifecycle_id.as_str())
                    .expect("lifecycle order exists");
                order.state = LifecycleState::Terminal(TerminalReason::Rejected);
            }
            LifecycleState::Terminal(_) => {
                let order = self
                    .orders
                    .get(lifecycle_id.as_str())
                    .expect("lifecycle order exists");
                self.emit_terminal_dedup_observability(order, "place_rejected");
                return Ok(order);
            }
            other => {
                return Err(LifecycleError::InvalidTransition {
                    action: "record_place_rejected",
                    state: other,
                });
            }
        }
        let order = self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        self.emit_transition_observability(
            order,
            state,
            LifecycleState::Terminal(TerminalReason::Rejected),
        );
        self.refresh_inflight_cancel_queue_depth();
        Ok(order)
    }

    pub fn record_cancel_request(
        &mut self,
        cancel: LifecycleCancelRequest,
    ) -> Result<LifecycleCancelRequestOutcome<'_>, LifecycleError> {
        let lifecycle_id = match self.resolve_lifecycle_id(&cancel.order_ref) {
            Some(lifecycle_id) => lifecycle_id,
            None => {
                self.emit_unknown_cancel_observability(&cancel.order_ref);
                if matches!(
                    self.unknown_order_cancel_policy,
                    UnknownOrderCancelPolicy::Idempotent
                ) {
                    return Ok(LifecycleCancelRequestOutcome::IgnoredUnknownOrder);
                }

                return Err(LifecycleError::UnknownOrder(cancel.order_ref));
            }
        };

        let order = self
            .orders
            .get_mut(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        let from_state = order.state;
        match order.state {
            LifecycleState::SubmitPending => {
                order.cancel_requested = true;
            }
            LifecycleState::Open => {
                order.cancel_requested = true;
                order.state = LifecycleState::CancelPending;
            }
            LifecycleState::CancelPending => {
                order.cancel_requested = true;
            }
            LifecycleState::Terminal(_) => {}
        }
        let to_state = order.state;
        let order = self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        if !matches!(from_state, LifecycleState::Terminal(_)) {
            self.emit_cancel_requested_observability(order, from_state);
        }
        self.emit_transition_observability(order, from_state, to_state);
        self.refresh_inflight_cancel_queue_depth();
        Ok(LifecycleCancelRequestOutcome::Updated(order))
    }

    pub fn record_cancel_confirmed(
        &mut self,
        confirmation: LifecycleCancelConfirmed,
    ) -> Result<&LifecycleOrder, LifecycleError> {
        self.record_terminal_evidence(LifecycleTerminalEvidence {
            order_ref: confirmation.order_ref,
            terminal_reason: TerminalReason::Cancelled,
            venue_order_id: confirmation.venue_order_id,
        })
    }

    pub fn record_fill_evidence(
        &mut self,
        fill: LifecycleFillEvidence,
    ) -> Result<&LifecycleOrder, LifecycleError> {
        if fill.filled_size <= 0.0 {
            return Err(LifecycleError::InvalidFillSize(fill.filled_size));
        }

        let lifecycle_id = self
            .resolve_lifecycle_id(&fill.order_ref)
            .ok_or_else(|| LifecycleError::UnknownOrder(fill.order_ref.clone()))?;

        let order = self
            .orders
            .get_mut(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        let from_state = order.state;
        let was_terminal = order.is_terminal();
        if !order.is_terminal() {
            if matches!(order.state, LifecycleState::SubmitPending) {
                order.state = LifecycleState::Open;
            }

            order.remaining_size = (order.remaining_size - fill.filled_size).max(0.0);
            if order.remaining_size <= f64::EPSILON {
                order.state = LifecycleState::Terminal(TerminalReason::Filled);
            }
        }
        let to_state = order.state;
        let order = self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        if was_terminal {
            self.emit_terminal_dedup_observability(order, "fill_update");
            return Ok(order);
        }
        self.emit_transition_observability(order, from_state, to_state);
        self.refresh_inflight_cancel_queue_depth();
        Ok(order)
    }

    pub fn record_terminal_evidence(
        &mut self,
        evidence: LifecycleTerminalEvidence,
    ) -> Result<&LifecycleOrder, LifecycleError> {
        let lifecycle_id = self
            .resolve_lifecycle_id(&evidence.order_ref)
            .ok_or_else(|| LifecycleError::UnknownOrder(evidence.order_ref.clone()))?;

        let (state, cancel_requested, existing_venue_order_id) = {
            let snapshot = self
                .orders
                .get(lifecycle_id.as_str())
                .expect("lifecycle order exists");
            if snapshot.is_terminal() {
                self.emit_terminal_dedup_observability(snapshot, "terminal_evidence");
                return Ok(self
                    .orders
                    .get(lifecycle_id.as_str())
                    .expect("lifecycle order exists"));
            }
            (
                snapshot.state,
                snapshot.cancel_requested,
                snapshot.venue_order_id.clone(),
            )
        };

        if evidence.terminal_reason == TerminalReason::Cancelled
            && matches!(state, LifecycleState::SubmitPending)
            && !cancel_requested
        {
            return Err(LifecycleError::InvalidTransition {
                action: "record_terminal_evidence(cancelled)",
                state,
            });
        }

        if let Some(existing) = existing_venue_order_id.as_ref() {
            if let Some(incoming) = evidence.venue_order_id.as_ref() {
                if existing != incoming {
                    return Err(LifecycleError::ConflictingVenueOrderId {
                        existing: existing.clone(),
                        incoming: incoming.clone(),
                    });
                }
            }
        }

        if let Some(venue_order_id) = evidence.venue_order_id.as_ref() {
            self.ensure_venue_mapping(lifecycle_id.as_str(), venue_order_id)?;
        }

        let order = self
            .orders
            .get_mut(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        let from_state = state;
        if let Some(venue_order_id) = evidence.venue_order_id {
            order.venue_order_id = Some(venue_order_id);
        }
        if evidence.terminal_reason == TerminalReason::Cancelled {
            order.cancel_requested = true;
        }
        order.state = LifecycleState::Terminal(evidence.terminal_reason);
        let to_state = order.state;

        let order = self
            .orders
            .get(lifecycle_id.as_str())
            .expect("lifecycle order exists");
        self.emit_transition_observability(order, from_state, to_state);
        self.refresh_inflight_cancel_queue_depth();

        Ok(order)
    }

    fn resolve_lifecycle_id(&self, order_ref: &LifecycleOrderRef) -> Option<String> {
        match order_ref {
            LifecycleOrderRef::LifecycleId(lifecycle_id) => self
                .orders
                .contains_key(lifecycle_id)
                .then(|| lifecycle_id.clone()),
            LifecycleOrderRef::ClientOrderId(client_order_id) => {
                self.client_to_lifecycle.get(client_order_id).cloned()
            }
            LifecycleOrderRef::VenueOrderId(venue_order_id) => {
                self.venue_to_lifecycle.get(venue_order_id).cloned()
            }
        }
    }

    fn ensure_venue_mapping(
        &mut self,
        lifecycle_id: &str,
        venue_order_id: &str,
    ) -> Result<(), LifecycleError> {
        if let Some(bound_lifecycle_id) = self.venue_to_lifecycle.get(venue_order_id) {
            if bound_lifecycle_id != lifecycle_id {
                return Err(LifecycleError::DuplicateVenueOrderId(
                    venue_order_id.to_string(),
                ));
            }
        } else {
            self.venue_to_lifecycle
                .insert(venue_order_id.to_string(), lifecycle_id.to_string());
        }

        Ok(())
    }

    fn emit_transition_observability(
        &self,
        order: &LifecycleOrder,
        from_state: LifecycleState,
        to_state: LifecycleState,
    ) {
        if from_state == to_state {
            return;
        }

        debug!(
            from_state = %state_label(&from_state),
            to_state = %state_label(&to_state),
            lifecycle_id = %order.lifecycle_id,
            client_order_id = %order.client_order_id,
            venue_order_id = ?order.venue_order_id,
            "lifecycle transition",
        );

        if let Some(metrics) = &self.metrics {
            metrics.record_transition(self.mode_label.as_str(), &from_state, &to_state);
            if let LifecycleState::Terminal(reason) = to_state {
                metrics.record_cancel_terminal(self.mode_label.as_str(), reason);
            }
        }
    }

    fn emit_cancel_requested_observability(
        &self,
        order: &LifecycleOrder,
        from_state: LifecycleState,
    ) {
        let queued_while_submit_pending = matches!(from_state, LifecycleState::SubmitPending);
        let cancel_submitted_to_venue = matches!(from_state, LifecycleState::Open);
        debug!(
            lifecycle_id = %order.lifecycle_id,
            client_order_id = %order.client_order_id,
            venue_order_id = ?order.venue_order_id,
            from_state = %state_label(&from_state),
            queued_while_submit_pending = queued_while_submit_pending,
            cancel_submitted_to_venue = cancel_submitted_to_venue,
            "cancel requested",
        );

        if let Some(metrics) = &self.metrics {
            metrics.record_cancel_requested(self.mode_label.as_str(), &from_state);
        }
    }

    fn emit_terminal_dedup_observability(&self, order: &LifecycleOrder, event_type: &str) {
        debug!(
            lifecycle_id = %order.lifecycle_id,
            client_order_id = %order.client_order_id,
            venue_order_id = ?order.venue_order_id,
            state = %state_label(&order.state),
            event_type = event_type,
            "terminal evidence deduplicated",
        );

        if let Some(metrics) = &self.metrics {
            metrics.record_terminal_dedup(self.mode_label.as_str(), event_type);
        }
    }

    fn emit_unknown_cancel_observability(&self, order_ref: &LifecycleOrderRef) {
        let policy = policy_label(self.unknown_order_cancel_policy);
        warn!(
            order_ref = ?order_ref,
            policy = policy,
            "unknown-order cancel request",
        );

        if let Some(metrics) = &self.metrics {
            metrics.record_unknown_cancel(self.mode_label.as_str(), policy);
        }
    }

    fn refresh_inflight_cancel_queue_depth(&self) {
        if let Some(metrics) = &self.metrics {
            let depth = self
                .orders
                .values()
                .filter(|order| {
                    order.cancel_requested && matches!(order.state, LifecycleState::SubmitPending)
                })
                .count();
            metrics.set_inflight_cancel_queue_depth(self.mode_label.as_str(), depth);
        }
    }
}

fn state_label(state: &LifecycleState) -> &'static str {
    match state {
        LifecycleState::SubmitPending => "submit_pending",
        LifecycleState::Open => "open",
        LifecycleState::CancelPending => "cancel_pending",
        LifecycleState::Terminal(TerminalReason::Filled) => "terminal_filled",
        LifecycleState::Terminal(TerminalReason::Cancelled) => "terminal_cancelled",
        LifecycleState::Terminal(TerminalReason::Rejected) => "terminal_rejected",
        LifecycleState::Terminal(TerminalReason::Expired) => "terminal_expired",
    }
}

fn policy_label(policy: UnknownOrderCancelPolicy) -> &'static str {
    match policy {
        UnknownOrderCancelPolicy::Strict => "strict",
        UnknownOrderCancelPolicy::Idempotent => "idempotent",
    }
}

#[cfg(test)]
mod tests {
    use prometheus::Registry;

    use crate::execution::events::{OrderSide, TimeInForce};

    use super::*;

    fn sample_place_request(client_order_id: &str) -> LifecyclePlaceRequest {
        LifecyclePlaceRequest {
            lifecycle_id: None,
            client_order_id: client_order_id.to_string(),
            mint: "asset-1".to_string(),
            market: Some("market-1".to_string()),
            side: OrderSide::Buy,
            price: 0.47,
            size: 10.0,
            time_in_force: TimeInForce::GoodTilCancelled,
            signal_id: Some("signal-1".to_string()),
        }
    }

    fn place_and_open(engine: &mut LifecycleEngine, client_order_id: &str, venue_order_id: &str) {
        engine
            .record_place_request(sample_place_request(client_order_id))
            .unwrap();
        engine
            .record_place_success(LifecyclePlaceSuccess {
                order_ref: LifecycleOrderRef::ClientOrderId(client_order_id.to_string()),
                venue_order_id: venue_order_id.to_string(),
            })
            .unwrap();
    }

    #[test]
    fn record_place_request_starts_submit_pending() {
        let mut engine = LifecycleEngine::new();

        let order = engine
            .record_place_request(sample_place_request("client-1"))
            .unwrap();

        assert_eq!(order.client_order_id, "client-1");
        assert_eq!(order.state, LifecycleState::SubmitPending);
        assert!(!order.cancel_requested);
        assert_eq!(engine.len(), 1);
    }

    #[test]
    fn cancel_request_before_place_success_transitions_to_cancel_pending_on_ack() {
        let mut engine = LifecycleEngine::new();

        engine
            .record_place_request(sample_place_request("client-1"))
            .unwrap();
        let order = engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: LifecycleOrderRef::ClientOrderId("client-1".to_string()),
            })
            .unwrap();
        let LifecycleCancelRequestOutcome::Updated(order) = order else {
            panic!("expected updated lifecycle order");
        };
        assert_eq!(order.state, LifecycleState::SubmitPending);
        assert!(order.cancel_requested);

        let order = engine
            .record_place_success(LifecyclePlaceSuccess {
                order_ref: LifecycleOrderRef::ClientOrderId("client-1".to_string()),
                venue_order_id: "venue-1".to_string(),
            })
            .unwrap();
        assert_eq!(order.state, LifecycleState::CancelPending);
        assert_eq!(order.venue_order_id.as_deref(), Some("venue-1"));
    }

    #[test]
    fn fill_evidence_transitions_to_terminal_filled() {
        let mut engine = LifecycleEngine::new();

        place_and_open(&mut engine, "client-1", "venue-1");

        let order = engine
            .record_fill_evidence(LifecycleFillEvidence {
                order_ref: LifecycleOrderRef::VenueOrderId("venue-1".to_string()),
                filled_size: 10.0,
            })
            .unwrap();

        assert_eq!(order.remaining_size, 0.0);
        assert_eq!(
            order.state,
            LifecycleState::Terminal(TerminalReason::Filled)
        );
    }

    #[test]
    fn reject_after_open_is_invalid_transition() {
        let mut engine = LifecycleEngine::new();
        place_and_open(&mut engine, "client-1", "venue-1");

        let err = engine
            .record_place_rejected(LifecyclePlaceRejected {
                order_ref: LifecycleOrderRef::VenueOrderId("venue-1".to_string()),
            })
            .unwrap_err();

        assert_eq!(
            err,
            LifecycleError::InvalidTransition {
                action: "record_place_rejected",
                state: LifecycleState::Open,
            }
        );
    }

    #[test]
    fn duplicate_terminal_evidence_is_idempotent_noop() {
        let mut engine = LifecycleEngine::new();
        place_and_open(&mut engine, "client-1", "venue-1");

        let order = engine
            .record_terminal_evidence(LifecycleTerminalEvidence {
                order_ref: LifecycleOrderRef::VenueOrderId("venue-1".to_string()),
                terminal_reason: TerminalReason::Expired,
                venue_order_id: Some("venue-1".to_string()),
            })
            .unwrap();
        assert_eq!(
            order.state,
            LifecycleState::Terminal(TerminalReason::Expired)
        );

        let order = engine
            .record_terminal_evidence(LifecycleTerminalEvidence {
                order_ref: LifecycleOrderRef::VenueOrderId("venue-1".to_string()),
                terminal_reason: TerminalReason::Cancelled,
                venue_order_id: Some("venue-1".to_string()),
            })
            .unwrap();
        assert_eq!(
            order.state,
            LifecycleState::Terminal(TerminalReason::Expired)
        );
    }

    #[test]
    fn cancel_confirmation_requires_prior_cancel_intent_in_submit_pending() {
        let mut engine = LifecycleEngine::new();
        engine
            .record_place_request(sample_place_request("client-1"))
            .unwrap();

        let err = engine
            .record_cancel_confirmed(LifecycleCancelConfirmed {
                order_ref: LifecycleOrderRef::ClientOrderId("client-1".to_string()),
                venue_order_id: None,
            })
            .unwrap_err();

        assert_eq!(
            err,
            LifecycleError::InvalidTransition {
                action: "record_terminal_evidence(cancelled)",
                state: LifecycleState::SubmitPending,
            }
        );
    }

    #[test]
    fn metrics_record_transition_and_cancel_path() {
        let registry = Registry::new();
        let metrics = LifecycleMetrics::new(&registry).unwrap();
        let mut engine = LifecycleEngine::new().with_metrics(metrics.clone(), "backtest");

        engine
            .record_place_request(sample_place_request("client-1"))
            .unwrap();
        engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: LifecycleOrderRef::ClientOrderId("client-1".to_string()),
            })
            .unwrap();
        engine
            .record_place_success(LifecyclePlaceSuccess {
                order_ref: LifecycleOrderRef::ClientOrderId("client-1".to_string()),
                venue_order_id: "venue-1".to_string(),
            })
            .unwrap();

        assert_eq!(
            metrics
                .cancel_requested_total
                .with_label_values(&["backtest", "submit_pending"])
                .get(),
            1.0
        );
        assert_eq!(
            metrics
                .lifecycle_transitions_total
                .with_label_values(&["submit_pending", "cancel_pending", "backtest"])
                .get(),
            1.0
        );
        assert_eq!(
            metrics
                .inflight_cancel_queue_depth
                .with_label_values(&["backtest"])
                .get(),
            0.0
        );
    }

    #[test]
    fn unknown_cancel_idempotent_policy_returns_successful_ignore() {
        let mut engine = LifecycleEngine::new()
            .with_unknown_order_cancel_policy(UnknownOrderCancelPolicy::Idempotent);

        let outcome = engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: LifecycleOrderRef::ClientOrderId("missing-client".to_string()),
            })
            .unwrap();

        assert!(matches!(
            outcome,
            LifecycleCancelRequestOutcome::IgnoredUnknownOrder
        ));
    }

    #[test]
    fn unknown_cancel_strict_policy_returns_error() {
        let mut engine = LifecycleEngine::new()
            .with_unknown_order_cancel_policy(UnknownOrderCancelPolicy::Strict);

        let err = engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: LifecycleOrderRef::ClientOrderId("missing-client".to_string()),
            })
            .unwrap_err();

        assert_eq!(
            err,
            LifecycleError::UnknownOrder(LifecycleOrderRef::ClientOrderId(
                "missing-client".to_string()
            ))
        );
    }

    #[test]
    fn metrics_record_unknown_cancel_diagnostics() {
        let registry = Registry::new();
        let metrics = LifecycleMetrics::new(&registry).unwrap();
        let mut engine = LifecycleEngine::new()
            .with_metrics(metrics.clone(), "paper")
            .with_unknown_order_cancel_policy(UnknownOrderCancelPolicy::Idempotent);

        let _ = engine.record_cancel_request(LifecycleCancelRequest {
            order_ref: LifecycleOrderRef::ClientOrderId("missing-client".to_string()),
        });

        assert_eq!(
            metrics
                .unknown_cancel_requests_total
                .with_label_values(&["paper", "idempotent"])
                .get(),
            1.0
        );
    }
}
