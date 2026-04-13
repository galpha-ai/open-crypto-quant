# Orderbook Trading Support

The trade server supports orderbook-based trading venues (e.g., Polymarket, prediction markets) alongside AMM-based trading.

## Event Types

- **OrderbookSnapshot** (`popeyes_trading_types`): Full orderbook state with bid/ask levels
- **OrderbookUpdate** (`popeyes_trading_types`): Incremental updates with best bid/ask prices

## Order Types

Defined in `src/execution/order.rs`:

- **Market Orders**: Immediate execution at current price (AMM or taker on orderbook)
- **Limit Orders**: Placed on orderbook at specified price, filled when matched
- **Cancel Orders**: Cancel an existing order by ID

## Signal Action System

Signals can indicate different actions via `signal_action()` method on `TradableSignal`:

- `Entry`: Open a new position (default, backward compatible)
- `Exit`: Close an existing position (market or limit sell)
- `ModifyOrder`: Cancel and replace an existing limit order
- `CancelOrder`: Cancel an active limit order

Signal routing is handled by `PositionHandler` (`src/trade_server/position_handler/`):
- Intent-based signals → `IntentHandler`
- Action-based signals → `SignalHandler` by `SignalAction` variant

## Intent-Based Signals

For orderbook market making, strategies can use intent-based signals via `OrderIntent` (`src/signal/intent.rs`):

- Express desired orderbook state (bid and ask levels) rather than discrete orders
- System reconciles desired state against current pending orders
- Generates appropriate cancel and placement orders to reach desired state
- Flow: `OrderIntent` → `ReconciliationEngine.compute_reconciliation()` → Orders

This enables clean separation between strategy logic and order lifecycle management.

### OrderIntent Structure

```
OrderIntent {
    signal_id: String,
    mint: String,
    market: Option<String>,
    bids: Option<Vec<QuoteLevel>>,   // None = no change, Some([]) = cancel all
    asks: Option<Vec<QuoteLevel>>,   // None = no change, Some([]) = cancel all
}
```

### Reconciliation Engine

Located in `src/position/reconciliation/engine.rs`:

- `ReconciliationEngine`: Computes order diffs between desired and current state
- `PositionReconciler` (`src/position/reconciliation/position_reconciler.rs`): Periodic position sync with exchange to catch drift
- `validate_intent()` (`src/position/reconciliation/validation.rs`): Intent validation rules

Quote-update debouncing in live intent flow:
- Debounce state is tracked per quote lane `(market, mint, side)` in `PositionManagerState`.
- When `LatencySimulationConfig.min_quote_lifetime_ms` is set, transient quote updates are
  suppressed until the debounce window expires.
- If intent converges back to the currently live quote within that window, no cancel/replace
  orders are emitted.
- Cancel-all intents (`Some([])`) bypass debounce and execute immediately.

## Exit Management Modes

Positions can operate in two exit modes (see `ExitMode` in `src/position/`):

- **Automatic**: Traditional `ExitStrategy` controls exits (stop-loss, take-profit, timeout)
- **StrategyManaged**: SignalGenerator manages exits via Exit signals (for CLOB strategies)

Safety limits still apply in StrategyManaged mode as a backstop.

## Orderbook Tracker

Located in `src/orderbook_tracker/mod.rs`. Maintains live orderbook state per market.

Key features:
- `apply_update()`: Applies incremental updates (add/modify/remove price levels)
- `apply_snapshot()`: Replaces state with a new full snapshot
- `get_orderbook()`: Retrieves current snapshot for a market
- `evict_inactive()`: Removes markets not updated within threshold (default 1 hour)

## Metrics

Orderbook-specific metrics (Prometheus):

- `trade_server_orderbook_spread{asset_id}`: Current spread in price units
- `trade_server_orderbook_mid_price{asset_id}`: Current mid price
- `trade_server_orderbook_depth{asset_id, side}`: Total depth on bid/ask side
- `trade_server_limit_orders_placed{venue, status}`: Limit order placement count
- `trade_server_limit_orders_filled{venue}`: Limit order fill count
- `trade_server_limit_orders_cancelled{venue}`: Limit order cancellation count
- `trade_server_limit_order_fill_latency_ms`: Time from placement to fill

## Backward Compatibility

All changes are additive with sensible defaults:

- Existing signals default to `Entry` action
- Existing positions default to `Automatic` exit mode
- New trait methods have default implementations
- Existing AMM-based bots continue to work unchanged
