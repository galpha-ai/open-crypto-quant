# Polymarket Executor Implementation Tasks

## Phase 1: Foundation

### 1.1 Project Setup
- [ ] Add `polymarket-rs-client` to Cargo.toml (or evaluate custom implementation)
- [ ] Create `src/execution/polymarket/` module structure:
  - [ ] `mod.rs` - Module exports
  - [ ] `executor.rs` - Main executor implementation
  - [ ] `builder.rs` - Builder pattern for executor construction
  - [ ] `config.rs` - Configuration types
  - [ ] `types.rs` - Internal types (PendingOrder, OrderbookState)
  - [ ] `websocket.rs` - WebSocket listener for fill events
  - [ ] `error.rs` - Error types

### 1.2 Configuration
- [ ] Define `PolymarketConfig` struct
- [ ] Add Polymarket section to main config schema
- [ ] Implement config loading from YAML
- [ ] Implement environment variable loading for secrets
- [ ] Add config validation

## Phase 2: Core Implementation

### 2.1 ClobClient Integration
- [ ] Create wrapper around `ClobClient` (or implement custom client)
- [ ] Implement L2 authentication setup
- [ ] Implement API credential derivation/creation
- [ ] Add retry logic with exponential backoff
- [ ] Add request logging and metrics

### 2.2 OrderExecutor Trait - Basic Methods
- [ ] Implement `supports_limit_orders()` → `true`
- [ ] Implement `handle_signal()` (cache signal data if needed)
- [ ] Implement `handle_orderbook_snapshot()`
- [ ] Implement `handle_orderbook_update()`
- [ ] Implement `handle_token_trade()`

### 2.3 Limit Order Execution
- [ ] Implement `execute_limit_order()`
  - [ ] Map Order to Polymarket order format
  - [ ] Create signed order
  - [ ] Submit via API
  - [ ] Track in pending_orders
  - [ ] Return `LimitOrderEvent::OrderPlaced`
- [ ] Handle order rejection cases
- [ ] Implement price/size validation

### 2.4 Order Cancellation
- [ ] Implement `cancel_order()`
  - [ ] Call Polymarket cancel API
  - [ ] Remove from pending_orders
  - [ ] Return `LimitOrderEvent::OrderCancelled`
- [ ] Handle cancellation failures (order already filled, etc.)

### 2.5 Market Order Execution
- [ ] Implement `execute_market_order()`
  - [ ] Calculate aggressive limit price from orderbook
  - [ ] Submit as IOC order
  - [ ] Wait for fill confirmation
  - [ ] Return `ExecutionEvent::OrderFilled`
- [ ] Handle partial fills
- [ ] Handle slippage protection

## Phase 3: Real-Time Updates

### 3.1 WebSocket Integration
- [ ] Implement WebSocket connection to user channel
- [ ] Parse fill messages
- [ ] Parse order update messages
- [ ] Implement automatic reconnection
- [ ] Add heartbeat/ping-pong handling

### 3.2 Fill Detection
- [ ] Emit `LimitOrderEvent::OrderPartiallyFilled` for partial fills
- [ ] Emit `LimitOrderEvent::OrderFilled` (implicit via ExecutionEvent) for complete fills
- [ ] Update pending_orders tracking
- [ ] Enqueue fill events to EventCoordinator

### 3.3 Fallback Mechanisms
- [ ] Implement polling-based fill detection as fallback
- [ ] Add configurable polling interval
- [ ] Reconcile WebSocket and polling data

## Phase 4: Builder & Factory

### 4.1 Builder Implementation
- [ ] Implement `PolymarketOrderExecutorBuilder`
- [ ] Add validation in `build()`
- [ ] Support both provided and auto-derived API credentials

### 4.2 Factory Integration
- [ ] Add Polymarket executor to executor factory/registry
- [ ] Update trade server initialization to support Polymarket

## Phase 5: Testing

### 5.1 Unit Tests
- [ ] Test order mapping functions
- [ ] Test price calculation for market orders
- [ ] Test pending order tracking
- [ ] Test fill event generation
- [ ] Test config validation

### 5.2 Integration Tests
- [ ] Test against Polymarket testnet (if available)
- [ ] Test order placement flow
- [ ] Test cancellation flow
- [ ] Test WebSocket connection
- [ ] Test error handling and retries

### 5.3 Mock Tests
- [ ] Create MockClobClient for testing
- [ ] Test various fill scenarios
- [ ] Test reconnection logic
- [ ] Test rate limit handling

## Phase 6: Observability

### 6.1 Metrics
- [ ] Add order placement counter
- [ ] Add order fill counter
- [ ] Add cancellation counter
- [ ] Add API latency histogram
- [ ] Add fill latency histogram
- [ ] Add WebSocket reconnect counter
- [ ] Add error counter by type

### 6.2 Logging
- [ ] Add structured logging for order lifecycle
- [ ] Add debug logging for API calls
- [ ] Add warning logs for retries
- [ ] Add error logs for failures
- [ ] Ensure no sensitive data (keys) in logs

## Phase 7: Documentation

- [ ] Add inline code documentation
- [ ] Update architecture.md with Polymarket executor details
- [ ] Create usage guide for Polymarket trading
- [ ] Document configuration options
- [ ] Add example configuration

## Acceptance Criteria

### Functional Requirements
- [ ] Can place limit buy orders on Polymarket
- [ ] Can place limit sell orders on Polymarket
- [ ] Can cancel pending orders
- [ ] Can execute market orders (as aggressive limit orders)
- [ ] Receives real-time fill notifications
- [ ] Correctly tracks pending order state
- [ ] Emits correct `LimitOrderEvent` and `ExecutionEvent` types

### Non-Functional Requirements
- [ ] API calls complete within 5s (P99)
- [ ] WebSocket reconnects within 10s of disconnection
- [ ] Handles rate limits gracefully
- [ ] No memory leaks in long-running operation
- [ ] Passes all unit and integration tests

### Integration Requirements
- [ ] Works with existing `PositionManager`
- [ ] Works with existing `EventCoordinator`
- [ ] Integrates with trade server configuration system
- [ ] Compatible with backtest mode (via `BacktestOrderExecutor`)
