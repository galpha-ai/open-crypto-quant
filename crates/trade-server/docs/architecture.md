# Trade Server Crate Architecture

## Overview

The `trade_server` crate is the core trading engine that orchestrates signal generation, position management, and trade execution. It operates as an event-driven system that processes various events (token events, signals, timers, execution events) through a centralized event loop and coordinates between multiple subsystems.

**Detailed design docs:**
- [Signal Processing](signal-processing.md) - Signal flow sequence diagrams for realtime and backtest modes
- [Orderbook Trading](design/orderbook-trading.md) - CLOB/orderbook venue support
- [Backtest Infrastructure](design/backtest.md) - Strategy backtesting system

## Directory Structure

```
src/
├── api/                    # JSON-RPC API server with pluggable handlers
├── backtest/               # Backtesting infrastructure (see design/backtest.md)
├── client/                 # External service clients
│   ├── polymarket/         # Polymarket Safe client for CLOB trading
│   └── user_service_client.rs  # User service client
├── domain/                 # Core domain models and event types
├── event_coordinator/      # Event sourcing and coordination layer
├── event_source/           # Redis-based event sources
├── execution/              # Trade execution engine
│   ├── architecture.md     # Execution module architecture and lifecycle sequence
│   ├── order.rs            # Order, OrderType, OrderStatus
│   ├── events.rs           # ExecutionEvent, LimitOrderEvent
│   ├── executor.rs         # OrderExecutor trait
│   ├── lifecycle/          # Shared limit-order lifecycle state machine foundation
│   ├── backtest/           # Backtest executor with fill simulation
│   │   ├── executor.rs     # BacktestOrderExecutor facade/orchestration
│   │   └── executor/       # Internal backtest executor modules
│   │       ├── config.rs   # Builder/configuration plumbing
│   │       ├── fill_engine.rs # Trade-based fill matching engine
│   │       ├── latency.rs  # Simulation-time and latency helpers
│   │       ├── market_registry.rs # Market/complement asset mapping
│   │       ├── quote_lifecycle.rs # Quote lane lifecycle transitions
│   │       └── types.rs    # Pending order + quote-lane data types
│   ├── paper/              # Paper trading executor with simulated fills
│   ├── polymarket/         # Polymarket CLOB execution
│   └── solana/             # Solana execution
│       ├── signing/        # Transaction signing
│       ├── submission/     # Transaction submission
│       ├── confirmation/   # Transaction confirmation
│       ├── tx_constructor/ # DEX-specific transaction builders
│       └── utils/          # Solana-specific utilities
├── filter/                 # Trade filtering logic
├── leader_monitor/         # Solana leader schedule monitoring
├── notifier/               # Multi-channel notification system
├── orderbook_tracker/      # Orderbook state management
├── position/               # Position management and tracking
│   ├── handlers/           # Position manager logic handlers
│   ├── orderbook/          # Orderbook intent + limit order handling
│   └── reconciliation/     # Intent-based order reconciliation
├── position_closer/        # Standalone position closing/cleanup
├── signal/                 # Signal generation and processing
├── trade_server/           # Main server orchestration
│   ├── position_handler/   # Position lifecycle coordination
│   ├── event_monitor.rs    # Event monitoring utilities
│   └── processor.rs        # Event processing helpers
├── utils/                  # Utility functions and token metadata
├── config.rs               # Configuration structures
├── lib.rs                  # Library root module
└── persistence.rs          # Event persistence to Redis
```

## Key Components

### TradeServer (Main Orchestrator)
`src/trade_server/trade_server.rs`

Central event loop that coordinates all subsystems:
- Fetches events from EventCoordinator
- Routes events to appropriate handlers
- Processes signals from multiple generators
- Coordinates position management and trade execution

### Event System

- **SystemEvent** (`src/domain/events.rs`) - Union type for all events (Token, MarketData, Timer, Signal, Position, Execution, LimitOrder)
- **EventCoordinator** (`src/event_coordinator/`) - Manages event sourcing and queuing
- **CapturingEventCoordinator** (`src/event_coordinator/capturing.rs`) - Decorator that captures events when they are emitted via `next_event()` (egress-only capture)
- **EventCollector** (`src/event_coordinator/collector.rs`) - Trait for event capture:
  - `InMemoryEventCollector` - Stores events in memory, exports to JSONL (for backtests)
  - `RedisEventCollector` - Streams events to Redis in real-time (for paper trading)
- **EventFilter** (`src/event_coordinator/filter.rs`) - Configurable filter to exclude high-volume events
- **EventSource** (`src/event_source/`) - Trait for pluggable event sources (Redis streams, receivers, etc.)

### Signal Processing

- **SignalGenerator** (`src/signal/`) - Plugin interface for generating trading signals
- **TradableSignal** (`src/signal/`) - Core trait defining signal behavior and metadata
- **SignalAction** (`src/signal/`) - Signals indicate entry, exit, modify, or cancel actions

Signal flow: Event → Multiple Generators → Signals → Position Handler → Orders

### Position Management

- **PositionManager** (`src/position/`) - Tracks open positions, handles signal-to-order conversion, and manages cash-flow-based PnL tracking
- **Position** (`src/position/`) - Individual position state with entry/exit tracking
- **ExitStrategy** (`src/position/`) - Configurable exit logic (stop-loss, take-profit, time-based)
- **ReconciliationEngine** (`src/position/reconciliation/`) - Intent-based order reconciliation for orderbook trading
- **Intent Debounce State** (`src/position/state.rs`, `src/position/orderbook/intent.rs`) - Per-lane `(market,mint,side)` quote-update debouncing used to suppress transient cancel/replace churn before dispatch
- **Balance Tracker** (`src/position/balance_tracker.rs`) - Tracks quote currency balance and cumulative cash flows for PnL calculation

### Trade Execution

- **OrderExecutor** (`src/execution/executor.rs`) - Abstraction for trade execution backends
  - `check_fills_from_trade()` - Optional fill simulation from trade events (for backtest/paper trading)
  - `simulates_fills()` - Whether executor simulates fills from trade events
  - `defers_limit_order_events()` / `defers_cancel_order_events()` - Whether command-path place/cancel responses are immediate terminal events or non-terminal acknowledgements that must wait for lifecycle evidence
  - `fill_event_to_execution_event()` - Shared helper to convert limit order fills to execution events
- **LifecycleEngine** (`src/execution/lifecycle/engine.rs`) - Shared canonical order lifecycle state owner (`SubmitPending`/`Open`/`CancelPending`/terminal) with guarded transition entry points for place, cancel, fill, and terminal evidence updates. Shared lifecycle record/state types live in `src/execution/lifecycle/mod.rs`.
- **LifecycleAdapter Contract** (`src/execution/lifecycle/adapter.rs`) - Mode-specific adapter API for `submit_place`, `submit_cancel`, `poll_or_push_updates`, and clock abstraction (`LifecycleClock`) so backtest/paper/live provide evidence/timing while lifecycle transition authority remains in the shared engine.
- **LifecycleConfig** (`src/config.rs`) - Shared lifecycle controls for cancellation confirmation timeout, optional cancel-requested event emission, and unknown-order cancel policy.
- **LifecycleMetrics** (`src/execution/lifecycle/metrics.rs`) - Counters/gauges for lifecycle transitions, cancel intent paths, terminal dedup suppression, and queued cancel depth (labeled by execution mode).
- **Order** (`src/execution/order.rs`) - Order representation with market/limit types
- **SolanaOrderExecutor** (`src/execution/solana/`) - Solana-specific execution with signing, submission, confirmation
- **BacktestOrderExecutor** (`src/execution/backtest/`) - Simulated execution for backtesting with fill simulation. Public orchestration remains in `src/execution/backtest/executor.rs` with internal modules split across `src/execution/backtest/executor/config.rs`, `src/execution/backtest/executor/fill_engine.rs`, `src/execution/backtest/executor/latency.rs`, `src/execution/backtest/executor/market_registry.rs`, `src/execution/backtest/executor/quote_lifecycle.rs`, and `src/execution/backtest/executor/types.rs`.
  - Placement/cancel transitions in backtest quote lanes are projected through the shared `LifecycleEngine` so `SubmitPending` cancel intent is preserved until placement confirmation, with lane state kept as scheduling/replacement metadata.
  - Canonical quote lifecycle policy: once canceling is armed, cancellation is irreversible; quote convergence only affects which replacement quote is placed after cancel completion.
- **PaperTradingOrderExecutor** (`src/execution/paper/`) - Live paper trading with simulated fills
- **PolymarketOrderExecutor** (`src/execution/polymarket/`) - Polymarket CLOB trading via Safe wallet. Cancel command responses are non-terminal acknowledgements (deferred by `defers_cancel_order_events()`); terminal cancel/fill outcomes are emitted from venue evidence (poller/websocket) with lifecycle dedup guards.

Order flow: Signal → Order → Execution → ExecutionEvent/LimitOrderEvent → Position Update

Fill simulation (backtest/paper trading): Trade Event → `check_fills_from_trade()` → enqueue `SystemEvent::LimitOrder` → normal limit-order handler path (`fill_event_to_execution_event()`) → ExecutionEvent → Position Update

### Position Handler
`src/trade_server/position_handler/`

Coordinates position lifecycle with modular subcomponents:
- `PositionHandler` - Main entry point routing to specialized handlers
- `PositionEventHandler` - Handles position/execution/timer events
- `SignalHandler` - Handles entry/exit/modify/cancel signals
- `IntentHandler` - Handles intent-based orderbook signals

### Supporting Components

- **API Server** (`src/api/`) - JSON-RPC API with pluggable handlers
- **Leader Monitor** (`src/leader_monitor/`) - Solana leader schedule for optimal transaction timing
- **Orderbook Tracker** (`src/orderbook_tracker/`) - Live orderbook state per market
- **Notifier** (`src/notifier/`) - Multi-channel notifications (Telegram, Console)
- **PositionCloser** (`src/position_closer/`) - Standalone position closing and cleanup utility
- **SafeClient** (`src/client/polymarket/`) - Polymarket Safe wallet client for CLOB operations

## Architectural Patterns

### Event-Driven Architecture
- Central event loop in `TradeServer.run()` processes events sequentially
- Events sourced from Redis streams via EventCoordinator
- Different event types routed to specialized handlers

### Strategy Pattern
- Multiple pluggable `SignalGenerator` implementations
- Multiple `OrderExecutor` backends (Solana, Backtest, Paper, Polymarket)
- Configurable exit strategies
- DEX-specific transaction builders

### Plugin Architecture
- Pluggable JSON-RPC method handlers with prefixing
- Multiple event coordinator backends (Redis, Backtest, Noop)
- Multiple notification channels

## Data Flow

### Primary Event Flow
1. **Event Ingestion**: EventCoordinator fetches next SystemEvent
2. **Event Processing**: TradeServer routes event to appropriate handler
3. **Signal Generation**: All SignalGenerators process the event
4. **Signal Processing**: Persistence, notifications, position handling
5. **Order Execution**: Orders executed via OrderExecutor
6. **Position Updates**: ExecutionEvents update position state

### Position Lifecycle
1. Buy signal generated from market event
2. PositionManager converts signal to buy order
3. OrderExecutor submits transaction
4. ExecutionEvent creates new position
5. Trade events update position mark-to-market
6. ExitStrategy determines when to exit
7. Sell order generated and executed
8. Final ExecutionEvent closes position

## Integration Points

### External Systems
- **Redis**: Event streaming, persistence queues
- **Solana Blockchain**: Transaction submission and confirmation
- **Jito/BloxRoute**: MEV-protected and high-speed transaction submission
- **Polymarket CLOB**: Prediction market trading via Safe wallet
- **Telegram**: Trade notifications
- **Prometheus**: Metrics collection

### Internal Dependencies
- **popeyes_trading_types**: Shared type definitions for token and market events

## Design Decisions

- **Async/Trait-Based**: Heavy use of `async_trait` for extensibility and testability
- **Error Handling**: `anyhow::Result` for propagation, `thiserror` for domain errors
- **Observability**: Prometheus metrics, structured logging with `tracing`
- **Modularity**: Clear separation of concerns, mockable traits for testing
- **Unified Fill Simulation**: Both backtest and paper trading use the same `OrderExecutor` trait method (`check_fills_from_trade()`) for fill detection. TradeServer routes trade events to the executor's fill simulation when `simulates_fills()` returns true, eliminating duplicate logic between live and backtest paths.
