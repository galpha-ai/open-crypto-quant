use prometheus::{
    CounterVec, GaugeVec, Opts, Registry, register_counter_vec_with_registry,
    register_gauge_vec_with_registry,
};

use super::{LifecycleState, TerminalReason};

#[derive(Clone, Debug)]
pub struct LifecycleMetrics {
    pub lifecycle_transitions_total: CounterVec,
    pub cancel_requested_total: CounterVec,
    pub cancel_terminal_total: CounterVec,
    pub terminal_dedup_total: CounterVec,
    pub unknown_cancel_requests_total: CounterVec,
    pub inflight_cancel_queue_depth: GaugeVec,
}

impl LifecycleMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let lifecycle_transitions_total = register_counter_vec_with_registry!(
            Opts::new(
                "order_lifecycle_transitions_total",
                "Lifecycle state transitions by mode"
            ),
            &["from", "to", "mode"],
            registry
        )?;

        let cancel_requested_total = register_counter_vec_with_registry!(
            Opts::new(
                "order_cancel_requested_total",
                "Cancel intent requests by lifecycle state and mode"
            ),
            &["mode", "state"],
            registry
        )?;

        let cancel_terminal_total = register_counter_vec_with_registry!(
            Opts::new(
                "order_cancel_terminal_total",
                "Terminal lifecycle outcomes observed by mode"
            ),
            &["mode", "reason"],
            registry
        )?;

        let terminal_dedup_total = register_counter_vec_with_registry!(
            Opts::new(
                "order_lifecycle_terminal_dedup_total",
                "Terminal events suppressed due to already-terminal lifecycle state"
            ),
            &["mode", "event_type"],
            registry
        )?;

        let unknown_cancel_requests_total = register_counter_vec_with_registry!(
            Opts::new(
                "order_cancel_unknown_total",
                "Unknown-order cancel requests by mode and policy outcome"
            ),
            &["mode", "policy"],
            registry
        )?;

        let inflight_cancel_queue_depth = register_gauge_vec_with_registry!(
            Opts::new(
                "order_inflight_cancel_queue_depth",
                "Count of cancel intents queued while orders are submit-pending"
            ),
            &["mode"],
            registry
        )?;

        Ok(Self {
            lifecycle_transitions_total,
            cancel_requested_total,
            cancel_terminal_total,
            terminal_dedup_total,
            unknown_cancel_requests_total,
            inflight_cancel_queue_depth,
        })
    }

    pub fn record_transition(&self, mode: &str, from: &LifecycleState, to: &LifecycleState) {
        self.lifecycle_transitions_total
            .with_label_values(&[state_label(from), state_label(to), mode])
            .inc();
    }

    pub fn record_cancel_requested(&self, mode: &str, state: &LifecycleState) {
        self.cancel_requested_total
            .with_label_values(&[mode, state_label(state)])
            .inc();
    }

    pub fn record_cancel_terminal(&self, mode: &str, reason: TerminalReason) {
        self.cancel_terminal_total
            .with_label_values(&[mode, reason_label(reason)])
            .inc();
    }

    pub fn record_terminal_dedup(&self, mode: &str, event_type: &str) {
        self.terminal_dedup_total
            .with_label_values(&[mode, event_type])
            .inc();
    }

    pub fn record_unknown_cancel(&self, mode: &str, policy: &str) {
        self.unknown_cancel_requests_total
            .with_label_values(&[mode, policy])
            .inc();
    }

    pub fn set_inflight_cancel_queue_depth(&self, mode: &str, depth: usize) {
        self.inflight_cancel_queue_depth
            .with_label_values(&[mode])
            .set(depth as f64);
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

fn reason_label(reason: TerminalReason) -> &'static str {
    match reason {
        TerminalReason::Filled => "filled",
        TerminalReason::Cancelled => "cancelled",
        TerminalReason::Rejected => "rejected",
        TerminalReason::Expired => "expired",
    }
}

#[cfg(test)]
mod tests {
    use prometheus::Registry;

    use super::*;

    #[test]
    fn lifecycle_metrics_can_be_recorded() {
        let registry = Registry::new();
        let metrics = LifecycleMetrics::new(&registry).unwrap();

        metrics.record_transition(
            "paper",
            &LifecycleState::SubmitPending,
            &LifecycleState::Open,
        );
        metrics.record_cancel_requested("paper", &LifecycleState::Open);
        metrics.record_cancel_terminal("paper", TerminalReason::Cancelled);
        metrics.record_terminal_dedup("paper", "cancel_confirmed");
        metrics.record_unknown_cancel("paper", "idempotent");
        metrics.set_inflight_cancel_queue_depth("paper", 2);

        assert!(!registry.gather().is_empty());
    }
}
