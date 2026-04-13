# Execution Module Architecture

## Overview

The `execution` module provides a shared order execution interface (`OrderExecutor`) and venue/mode-specific implementations (`backtest`, `paper`, `polymarket`, `solana`).

For limit orders, lifecycle state is managed by a single canonical state machine in `execution/lifecycle` (`LifecycleEngine`). Executors feed evidence into that engine rather than owning independent lifecycle state transitions.

## Lifecycle Model

Canonical lifecycle states for a logical limit-order intent:

1. `SubmitPending`
2. `Open`
3. `CancelPending`
4. `Terminal(Filled | Cancelled | Rejected | Expired)`

The lifecycle record is keyed internally by `lifecycle_id` and can be resolved by:

- `LifecycleOrderRef::LifecycleId`
- `LifecycleOrderRef::ClientOrderId`
- `LifecycleOrderRef::VenueOrderId`

This allows command-path actions (client ID) and venue evidence (venue ID) to converge on the same canonical record.

## High-Level Sequence

```mermaid
sequenceDiagram
    autonumber
    participant PH as PositionHandler
    participant EX as OrderExecutor (mode-specific)
    participant LE as LifecycleEngine
    participant VE as Venue/Poller/Trade Feed
    participant EC as EventCoordinator

    PH->>EX: execute_limit_order(order)
    EX->>LE: record_place_request(client_order_id, order fields)
    EX-->>PH: OrderPlaced ack (immediate or deferred by executor policy)

    alt Immediate place confirmation path
        EX->>LE: record_place_success(order_ref, venue_order_id)
        LE->>LE: transition SubmitPending to Open or CancelPending
    else Deferred/evidence-driven place confirmation
        VE-->>EX: place accepted evidence
        EX->>LE: record_place_success(order_ref, venue_order_id)
    end

    opt Cancel requested
        PH->>EX: cancel_order(order_id)
        EX->>LE: record_cancel_request(order_ref)
        LE->>LE: transition Open to CancelPending or queue intent in SubmitPending
    end

    loop Venue evidence stream
        VE-->>EX: fill / cancel / reject / expire evidence
        alt Fill evidence
            EX->>LE: record_fill_evidence(order_ref, filled_size)
            LE-->>EC: emit LimitOrderEvent::OrderPartiallyFilled (via executor flow)
        else Terminal evidence
            EX->>LE: record_terminal_evidence(order_ref, terminal_reason)
            LE-->>EC: emit terminal limit-order event (via executor/poller flow)
        end
    end

    LE->>LE: deduplicate repeated terminal evidence and reject invalid transitions
```

## Mode-Specific Notes

- Backtest and paper executors may defer place/cancel event enqueue when latency simulation is enabled.
- Polymarket defers cancel terminal events until poller/websocket terminal evidence confirms cancellation.
- `LifecycleEngine` remains the single transition authority across these modes.
