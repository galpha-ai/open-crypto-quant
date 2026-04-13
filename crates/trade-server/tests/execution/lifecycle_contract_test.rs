use trade_server::execution::{
    LifecycleCancelConfirmed, LifecycleCancelRequest, LifecycleCancelRequestOutcome, LifecycleEngine,
    LifecycleError, LifecycleFillEvidence, LifecycleOrderRef, LifecyclePlaceRequest,
    LifecyclePlaceSuccess, LifecycleState, OrderSide, TerminalReason, TimeInForce,
    UnknownOrderCancelPolicy,
};

#[derive(Clone, Copy)]
struct HarnessConfig {
    name: &'static str,
    use_venue_ref_after_place: bool,
}

const HARNESS_CONFIGS: [HarnessConfig; 3] = [
    HarnessConfig {
        name: "backtest",
        use_venue_ref_after_place: false,
    },
    HarnessConfig {
        name: "paper",
        use_venue_ref_after_place: false,
    },
    HarnessConfig {
        name: "live",
        use_venue_ref_after_place: true,
    },
];

fn place_request(config: HarnessConfig, client_order_id: &str, size: f64) -> LifecyclePlaceRequest {
    LifecyclePlaceRequest {
        lifecycle_id: Some(format!("{}-{}", config.name, client_order_id)),
        client_order_id: client_order_id.to_string(),
        mint: format!("asset-{}", config.name),
        market: Some("market-1".to_string()),
        side: OrderSide::Buy,
        price: 0.42,
        size,
        time_in_force: TimeInForce::GoodTilCancelled,
        signal_id: Some(format!("signal-{}", config.name)),
    }
}

fn active_order_ref(
    config: HarnessConfig,
    client_order_id: &str,
    venue_order_id: &str,
) -> LifecycleOrderRef {
    if config.use_venue_ref_after_place {
        LifecycleOrderRef::VenueOrderId(venue_order_id.to_string())
    } else {
        LifecycleOrderRef::ClientOrderId(client_order_id.to_string())
    }
}

fn cancel_outcome_state(outcome: LifecycleCancelRequestOutcome<'_>) -> Option<LifecycleState> {
    match outcome {
        LifecycleCancelRequestOutcome::Updated(order) => Some(order.state),
        LifecycleCancelRequestOutcome::IgnoredUnknownOrder => None,
    }
}

fn run_place_then_immediate_cancel_before_place_confirm(config: HarnessConfig) {
    let mut engine = LifecycleEngine::new();
    let client_order_id = format!("{}-client", config.name);
    let venue_order_id = format!("{}-venue", config.name);

    engine
        .record_place_request(place_request(config, &client_order_id, 10.0))
        .expect("place request should succeed");

    let cancel_state = cancel_outcome_state(
        engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: LifecycleOrderRef::ClientOrderId(client_order_id.clone()),
            })
            .expect("cancel request before place success should succeed"),
    )
    .expect("cancel request should return updated order");
    assert_eq!(
        cancel_state,
        LifecycleState::SubmitPending,
        "{}: cancel request during submit-pending should stay submit-pending",
        config.name
    );

    let placed = engine
        .record_place_success(LifecyclePlaceSuccess {
            order_ref: LifecycleOrderRef::ClientOrderId(client_order_id.clone()),
            venue_order_id: venue_order_id.clone(),
        })
        .expect("place success should succeed");
    assert_eq!(
        placed.state,
        LifecycleState::CancelPending,
        "{}: place success after queued cancel must transition to cancel-pending",
        config.name
    );

    let terminal = engine
        .record_cancel_confirmed(LifecycleCancelConfirmed {
            order_ref: active_order_ref(config, &client_order_id, &venue_order_id),
            venue_order_id: Some(venue_order_id),
        })
        .expect("cancel confirmation should succeed");
    assert_eq!(
        terminal.state,
        LifecycleState::Terminal(TerminalReason::Cancelled),
        "{}: cancel confirmation should terminate as cancelled",
        config.name
    );
}

fn run_place_then_cancel_with_partial_fill_race(config: HarnessConfig) {
    let mut engine = LifecycleEngine::new();
    let client_order_id = format!("{}-client", config.name);
    let venue_order_id = format!("{}-venue", config.name);

    engine
        .record_place_request(place_request(config, &client_order_id, 10.0))
        .expect("place request should succeed");
    engine
        .record_place_success(LifecyclePlaceSuccess {
            order_ref: LifecycleOrderRef::ClientOrderId(client_order_id.clone()),
            venue_order_id: venue_order_id.clone(),
        })
        .expect("place success should succeed");

    let active_ref = active_order_ref(config, &client_order_id, &venue_order_id);
    let cancel_state = cancel_outcome_state(
        engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: active_ref.clone(),
            })
            .expect("cancel request should succeed"),
    )
    .expect("cancel request should return updated order");
    assert_eq!(
        cancel_state,
        LifecycleState::CancelPending,
        "{}: cancel after open should move to cancel-pending",
        config.name
    );

    let partially_filled = engine
        .record_fill_evidence(LifecycleFillEvidence {
            order_ref: active_ref.clone(),
            filled_size: 4.0,
        })
        .expect("partial fill evidence should succeed");
    assert_eq!(
        partially_filled.state,
        LifecycleState::CancelPending,
        "{}: partial fill during cancel-pending should remain cancel-pending",
        config.name
    );
    assert!(
        (partially_filled.remaining_size - 6.0).abs() <= f64::EPSILON,
        "{}: remaining size should track partial fill",
        config.name
    );

    let cancelled = engine
        .record_cancel_confirmed(LifecycleCancelConfirmed {
            order_ref: active_ref,
            venue_order_id: Some(venue_order_id),
        })
        .expect("cancel confirmation should succeed");
    assert_eq!(
        cancelled.state,
        LifecycleState::Terminal(TerminalReason::Cancelled),
        "{}: terminal cancel should win after partial fill race",
        config.name
    );
}

fn run_cancel_after_terminal_fill(config: HarnessConfig) {
    let mut engine = LifecycleEngine::new();
    let client_order_id = format!("{}-client", config.name);
    let venue_order_id = format!("{}-venue", config.name);

    engine
        .record_place_request(place_request(config, &client_order_id, 10.0))
        .expect("place request should succeed");
    engine
        .record_place_success(LifecyclePlaceSuccess {
            order_ref: LifecycleOrderRef::ClientOrderId(client_order_id.clone()),
            venue_order_id: venue_order_id.clone(),
        })
        .expect("place success should succeed");

    let active_ref = active_order_ref(config, &client_order_id, &venue_order_id);
    let filled = engine
        .record_fill_evidence(LifecycleFillEvidence {
            order_ref: active_ref.clone(),
            filled_size: 10.0,
        })
        .expect("full fill should succeed");
    assert_eq!(
        filled.state,
        LifecycleState::Terminal(TerminalReason::Filled),
        "{}: full fill should transition to terminal filled",
        config.name
    );

    let cancel_after_fill = cancel_outcome_state(
        engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: active_ref.clone(),
            })
            .expect("cancel after terminal fill should be idempotent"),
    )
    .expect("cancel should return updated order");
    assert_eq!(
        cancel_after_fill,
        LifecycleState::Terminal(TerminalReason::Filled),
        "{}: cancel after terminal fill must not overwrite terminal filled",
        config.name
    );

    let terminal_after_cancel_confirm = engine
        .record_cancel_confirmed(LifecycleCancelConfirmed {
            order_ref: active_ref,
            venue_order_id: Some(venue_order_id),
        })
        .expect("duplicate terminal evidence should be deduplicated");
    assert_eq!(
        terminal_after_cancel_confirm.state,
        LifecycleState::Terminal(TerminalReason::Filled),
        "{}: terminal fill should remain authoritative after cancel confirmation",
        config.name
    );
}

fn run_duplicate_cancel_requests(config: HarnessConfig) {
    let mut engine = LifecycleEngine::new();
    let client_order_id = format!("{}-client", config.name);
    let venue_order_id = format!("{}-venue", config.name);

    engine
        .record_place_request(place_request(config, &client_order_id, 10.0))
        .expect("place request should succeed");
    engine
        .record_place_success(LifecyclePlaceSuccess {
            order_ref: LifecycleOrderRef::ClientOrderId(client_order_id.clone()),
            venue_order_id: venue_order_id.clone(),
        })
        .expect("place success should succeed");

    let active_ref = active_order_ref(config, &client_order_id, &venue_order_id);

    let first_cancel = cancel_outcome_state(
        engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: active_ref.clone(),
            })
            .expect("first cancel request should succeed"),
    )
    .expect("first cancel should return updated order");
    assert_eq!(
        first_cancel,
        LifecycleState::CancelPending,
        "{}: first cancel request should move order to cancel-pending",
        config.name
    );

    let second_cancel = cancel_outcome_state(
        engine
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: active_ref.clone(),
            })
            .expect("duplicate cancel request should be idempotent"),
    )
    .expect("second cancel should return updated order");
    assert_eq!(
        second_cancel,
        LifecycleState::CancelPending,
        "{}: duplicate cancel request should stay cancel-pending",
        config.name
    );

    let first_cancel_confirm = engine
        .record_cancel_confirmed(LifecycleCancelConfirmed {
            order_ref: active_ref.clone(),
            venue_order_id: Some(venue_order_id.clone()),
        })
        .expect("cancel confirmation should succeed");
    assert_eq!(
        first_cancel_confirm.state,
        LifecycleState::Terminal(TerminalReason::Cancelled),
        "{}: cancel confirmation should terminate order as cancelled",
        config.name
    );

    let duplicate_cancel_confirm = engine
        .record_cancel_confirmed(LifecycleCancelConfirmed {
            order_ref: active_ref,
            venue_order_id: Some(venue_order_id),
        })
        .expect("duplicate cancel confirmation should be deduplicated");
    assert_eq!(
        duplicate_cancel_confirm.state,
        LifecycleState::Terminal(TerminalReason::Cancelled),
        "{}: duplicate cancel confirmation should not change terminal state",
        config.name
    );
}

fn run_unknown_order_cancel_idempotent(config: HarnessConfig) {
    let mut engine = LifecycleEngine::new()
        .with_unknown_order_cancel_policy(UnknownOrderCancelPolicy::Idempotent);

    let outcome = engine
        .record_cancel_request(LifecycleCancelRequest {
            order_ref: LifecycleOrderRef::ClientOrderId(format!("missing-{}", config.name)),
        })
        .expect("idempotent unknown cancel should not error");

    assert!(
        matches!(outcome, LifecycleCancelRequestOutcome::IgnoredUnknownOrder),
        "{}: idempotent unknown cancel should be ignored",
        config.name
    );
}

fn run_unknown_order_cancel_strict(config: HarnessConfig) {
    let mut engine = LifecycleEngine::new()
        .with_unknown_order_cancel_policy(UnknownOrderCancelPolicy::Strict);

    let err = engine
        .record_cancel_request(LifecycleCancelRequest {
            order_ref: LifecycleOrderRef::ClientOrderId(format!("missing-{}", config.name)),
        })
        .expect_err("strict unknown cancel should error");

    assert!(
        matches!(
            err,
            LifecycleError::UnknownOrder(LifecycleOrderRef::ClientOrderId(_))
        ),
        "{}: strict unknown cancel should report UnknownOrder",
        config.name
    );
}

#[test]
fn contract_place_then_immediate_cancel_before_place_confirm() {
    for config in HARNESS_CONFIGS {
        run_place_then_immediate_cancel_before_place_confirm(config);
    }
}

#[test]
fn contract_place_then_cancel_with_partial_fill_race() {
    for config in HARNESS_CONFIGS {
        run_place_then_cancel_with_partial_fill_race(config);
    }
}

#[test]
fn contract_cancel_after_terminal_fill() {
    for config in HARNESS_CONFIGS {
        run_cancel_after_terminal_fill(config);
    }
}

#[test]
fn contract_duplicate_cancel_requests_are_idempotent() {
    for config in HARNESS_CONFIGS {
        run_duplicate_cancel_requests(config);
    }
}

#[test]
fn contract_unknown_order_cancel_idempotent_policy() {
    for config in HARNESS_CONFIGS {
        run_unknown_order_cancel_idempotent(config);
    }
}

#[test]
fn contract_unknown_order_cancel_strict_policy() {
    for config in HARNESS_CONFIGS {
        run_unknown_order_cancel_strict(config);
    }
}
