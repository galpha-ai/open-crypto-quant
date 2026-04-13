# Orderbook Trading Support

## Summary

This design extends the trade_server crate to support orderbook-based trading (e.g., Polymarket prediction markets) alongside the existing AMM-based trading (PumpFun, Raydium, Bonk). The architecture changes are additive and backward-compatible, introducing new event handling paths, extended traits with default implementations, and optional limit order capabilities while preserving all existing AMM functionality.

## Goals

- Process new `TokenEvent::OrderbookUpdate` and `TokenEvent::OrderbookSnapshot` events from `popeyes_trading_types`
- Enable signal generators to produce trading signals from orderbook data (bid/ask spreads, depth changes)
- Support position valuation using bid/ask prices instead of single spot prices
- Provide optional limit order support for orderbook-based venues
- Maintain full backward compatibility with existing AMM-based bots
- Enable backtesting of orderbook strategies using the existing backtest infrastructure

## Non-Goals

- **Order management UI**: No REST/WebSocket API for manual order management (deferred to future work)
- **Advanced order types**: No support for stop-loss orders, OCO, or conditional orders at the execution layer (can be implemented in signal generators)
- **Cross-venue arbitrage**: No built-in support for arbitrage across multiple orderbook venues
- **Orderbook reconstruction from trade events**: We rely on explicit `OrderbookSnapshot`/`OrderbookUpdate` events, not trade-based inference
- **Real-time orderbook state persistence**: Orderbook state is ephemeral; reconstruction happens on restart from snapshots

## Behavior

### Event Processing Flow

Given a `SystemEvent::Token(TokenEvent::OrderbookSnapshot(snapshot))`:

1. TradeServer receives event via `EventCoordinator.next_event()`
2. Metrics are incremented for `orderbook_snapshot` event type
3. Price extraction: Calculate mid price from best bid/ask levels
   - If `bids` is non-empty, `best_bid = bids[0].price`
   - If `asks` is non-empty, `best_ask = asks[0].price`
   - `mid_price = (best_bid + best_ask) / 2.0`
4. Position update: Call `PositionHandler.update_price(asset_id, mid_price, timestamp)`
5. Orderbook state update (optional): If `OrderbookStateManager` is configured, call `handle_orderbook_snapshot(snapshot)`
6. Signal generation: Pass event to all registered `SignalGenerator` instances
7. Signal processing: For each generated signal, follow existing flow (filter, notify, handle)

Given a `SystemEvent::Token(TokenEvent::OrderbookUpdate(update))`:

1. TradeServer receives event via `EventCoordinator.next_event()`
2. Metrics are incremented for `orderbook_update` event type
3. Price extraction: Calculate mid price from `best_bid` and `best_ask` fields
   - `mid_price = (update.best_bid + update.best_ask) / 2.0`
4. Position update: Call `PositionHandler.update_price(asset_id, mid_price, timestamp)`
5. Orderbook state update (optional): If `OrderbookStateManager` is configured, call `handle_orderbook_update(update)`
6. Signal generation: Pass event to all registered `SignalGenerator` instances
7. Signal processing: For each generated signal, follow existing flow

### Limit Order Execution Flow

Given an `Order` with `OrderType::LimitBuy` or `OrderType::LimitSell`:

1. `PositionHandler.handle_signal()` creates limit order based on signal parameters
2. Order is passed to `OrderExecutor.execute_limit_order(order)`
3. Executor submits order to venue (e.g., Polymarket API)
4. Executor returns `LimitOrderEvent::OrderPlaced` with `order_id`
5. Position is marked as "pending buy" or "pending sell"
6. Subsequent `OrderbookUpdate` or trade events may trigger fill detection
7. On fill detection, executor emits `ExecutionEvent::OrderFilled`
8. `PositionHandler.handle_execution_event()` updates position state

### Position Valuation with Bid/Ask Spread

For positions in orderbook markets, valuation considers the exit side:

- **Long position value**: Use `best_bid` (price you could sell at)
- **Short position value**: Use `best_ask` (price you would need to buy at)
- **Mark-to-market for display**: Use mid price

This is handled in `PositionManager.update_price_with_spread()` for venues that support it.

## Data Model

### Extended Order Types

**Purpose**: Support limit orders alongside existing market orders

**Fields/Attributes**:
```rust
pub enum OrderType {
    // Existing variants (renamed for clarity)
    MarketBuy {
        quote_amount: f64,  // Amount in quote currency (SOL, USDC)
    },
    MarketSell {
        token_amount: f64,  // Amount in base token
        clear_position: bool,
    },

    // New limit order variants
    LimitBuy {
        quote_amount: f64,
        limit_price: f64,
        time_in_force: TimeInForce,
    },
    LimitSell {
        token_amount: f64,
        limit_price: f64,
        clear_position: bool,
        time_in_force: TimeInForce,
    },
}

pub enum TimeInForce {
    GoodTilCancelled,      // Remains active until filled or cancelled
    ImmediateOrCancel,     // Fill what's possible, cancel rest
    FillOrKill,            // Fill entirely or cancel
}
```

**Migration Note**: Existing `Buy` and `Sell` variants are aliased to `MarketBuy` and `MarketSell` for backward compatibility.

### Extended Order Struct

**Purpose**: Support orderbook market identifiers alongside token mints

**Fields/Attributes**:
```rust
pub struct Order {
    pub mint: String,                    // Asset identifier (token mint or asset_id)
    pub market: Option<String>,          // NEW: Market/condition ID for orderbooks
    pub order_type: OrderType,
    pub price: Option<f64>,
    pub status: OrderStatus,
    pub timestamp: DateTime<Utc>,
    pub signal_slot: Option<u64>,
    pub signal_id: Option<String>,
    pub dex_type: Option<DexType>,
    pub venue_order_id: Option<String>,  // NEW: External order ID from venue
}
```

### Limit Order Events

**Purpose**: Track lifecycle of limit orders

```rust
pub enum LimitOrderEvent {
    OrderPlaced {
        order_id: String,
        mint: String,
        market: Option<String>,
        price: f64,
        size: f64,
        side: OrderSide,
        timestamp: DateTime<Utc>,
    },
    OrderPartiallyFilled {
        order_id: String,
        filled_size: f64,
        remaining_size: f64,
        fill_price: f64,
        timestamp: DateTime<Utc>,
    },
    OrderCancelled {
        order_id: String,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    OrderExpired {
        order_id: String,
        timestamp: DateTime<Utc>,
    },
    OrderRejected {
        reason: String,
    },
}

pub enum OrderSide {
    Buy,
    Sell,
}
```

### Orderbook State (Optional Component)

**Purpose**: Maintain local orderbook state for strategy decisions

```rust
pub struct OrderbookState {
    pub asset_id: String,
    pub market: String,
    pub bids: BTreeMap<OrderedFloat<f64>, f64>,  // price -> size, descending
    pub asks: BTreeMap<OrderedFloat<f64>, f64>,  // price -> size, ascending
    pub last_update: DateTime<Utc>,
    pub sequence: u64,  // For ordering updates
}
```

**Note**: This is an optional component. Simple strategies can rely solely on the `best_bid`/`best_ask` from events.

## API

### Extended TradableSignal Trait

**Purpose**: Add optional orderbook-specific methods to the signal trait

```rust
pub trait TradableSignal: Send + Sync + Debug {
    // Existing required methods
    fn signal_id(&self) -> &str;
    fn signal_type(&self) -> &str;
    fn get_mint(&self) -> Option<&str>;
    fn get_price(&self) -> Option<f64>;
    fn get_timestamp(&self) -> Option<DateTime<Utc>>;
    fn get_slot(&self) -> Option<u64>;
    fn get_creator(&self) -> Option<Pubkey>;
    fn passes_filter(&self) -> bool;
    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>>;
    fn as_any(&self) -> &dyn Any;
    fn to_json(&self) -> Result<serde_json::Value>;

    // New optional methods with defaults
    fn get_market(&self) -> Option<&str> { None }
    fn get_bid_price(&self) -> Option<f64> { self.get_price() }
    fn get_ask_price(&self) -> Option<f64> { self.get_price() }
    fn get_limit_price(&self) -> Option<f64> { None }
    fn is_limit_order(&self) -> bool { false }
    fn get_time_in_force(&self) -> Option<TimeInForce> { None }
}
```

**Errors**: No new errors; existing trait error handling applies.

### Extended OrderExecutor Trait

**Purpose**: Add limit order and orderbook handling capabilities

```rust
#[async_trait]
pub trait OrderExecutor: Send + Sync + 'static {
    // Existing methods
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent>;
    async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()>;
    async fn handle_signal(&self, _signal: &dyn TradableSignal) -> Result<()> { Ok(()) }

    // New methods with defaults
    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent> {
        Err(anyhow!("Limit orders not supported by this executor"))
    }

    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent> {
        Err(anyhow!("Order cancellation not supported by this executor"))
    }

    async fn handle_orderbook_snapshot(&self, _snapshot: &OrderbookSnapshotEvent) -> Result<()> {
        Ok(())
    }

    async fn handle_orderbook_update(&self, _update: &OrderbookUpdateEvent) -> Result<()> {
        Ok(())
    }

    fn supports_limit_orders(&self) -> bool { false }
}
```

**Errors**:
- `anyhow::Error` with message "Limit orders not supported by this executor" when calling limit order methods on AMM executors
- `anyhow::Error` with message "Order cancellation not supported by this executor" for cancel on unsupported executors

### Extended PositionManager Trait

**Purpose**: Add spread-aware pricing and limit order tracking

```rust
#[async_trait]
pub trait PositionManager: Send + Sync {
    // Existing methods unchanged...

    // New methods with defaults
    async fn update_price_with_spread(
        &self,
        mint: &str,
        bid: f64,
        ask: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<PositionEvent, PositionError> {
        // Default: use mid price
        let mid = (bid + ask) / 2.0;
        self.update_price(mint, mid, timestamp).await
    }

    async fn add_pending_limit_order(&self, _order: &Order) -> Result<(), PositionError> {
        Ok(())  // Default no-op
    }

    async fn get_pending_limit_orders(&self, _mint: &str) -> Vec<Order> {
        vec![]  // Default empty
    }

    async fn remove_pending_limit_order(&self, _order_id: &str) -> Result<Option<Order>, PositionError> {
        Ok(None)  // Default no-op
    }
}
```

## Configuration

No new configuration is required for basic orderbook event processing. The system automatically handles new event types.

For limit order support, executors may require venue-specific configuration:

```yaml
# Example Polymarket executor configuration
execution:
  type: polymarket
  api_key: ${POLYMARKET_API_KEY}
  api_secret: ${POLYMARKET_API_SECRET}
  # Funder address for CLOB orders
  funder: "0x..."
  # Default time in force for limit orders
  default_tif: "GTC"
  # Maximum order size in USDC
  max_order_size: 1000.0
```

Configuration is executor-specific and not part of the core trade_server configuration.

## TradeServer Event Routing

### Changes to `trade_server.rs`

The main event processing loop needs to handle new `TokenEvent` variants:

**Metrics tracking** (around line 126-144):
```rust
match token_event {
    TokenEvent::Buy(_) => self.metrics.token_events.with_label_values(&["buy"]).inc(),
    TokenEvent::Sell(_) => self.metrics.token_events.with_label_values(&["sell"]).inc(),
    TokenEvent::Create(_) => self.metrics.token_events.with_label_values(&["create"]).inc(),
    TokenEvent::Swap(_) => self.metrics.token_events.with_label_values(&["swap"]).inc(),
    // NEW
    TokenEvent::OrderbookUpdate(_) => {
        self.metrics.token_events.with_label_values(&["orderbook_update"]).inc()
    }
    TokenEvent::OrderbookSnapshot(_) => {
        self.metrics.token_events.with_label_values(&["orderbook_snapshot"]).inc()
    }
}
```

**Event processing** (around line 163-181):
```rust
match token_event {
    TokenEvent::Buy(trade) | TokenEvent::Sell(trade) => {
        // Existing AMM logic unchanged
        self.order_executor.handle_token_trade(trade).await?;
        if let Some(price) = trade.spot_price() {
            self.position_handler.update_price(&trade.base_token_mint(), price, trade.base().timestamp).await?;
        }
    }
    // NEW: Orderbook event handling
    TokenEvent::OrderbookSnapshot(snapshot) => {
        self.order_executor.handle_orderbook_snapshot(snapshot).await?;
        if let Some(mid_price) = calculate_mid_price(&snapshot.bids, &snapshot.asks) {
            let timestamp = DateTime::from_timestamp_millis(snapshot.timestamp)
                .unwrap_or_else(Utc::now);
            self.position_handler.update_price(&snapshot.asset_id, mid_price, timestamp).await?;
        }
    }
    TokenEvent::OrderbookUpdate(update) => {
        self.order_executor.handle_orderbook_update(update).await?;
        let mid_price = (update.best_bid + update.best_ask) / 2.0;
        let timestamp = DateTime::from_timestamp_millis(update.timestamp)
            .unwrap_or_else(Utc::now);
        self.position_handler.update_price(&update.asset_id, mid_price, timestamp).await?;
    }
    _ => {}
}
```

**Helper function**:
```rust
fn calculate_mid_price(bids: &[OrderSummary], asks: &[OrderSummary]) -> Option<f64> {
    let best_bid = bids.first().map(|b| b.price);
    let best_ask = asks.first().map(|a| a.price);
    match (best_bid, best_ask) {
        (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
        (Some(bid), None) => Some(bid),
        (None, Some(ask)) => Some(ask),
        (None, None) => None,
    }
}
```

## Idempotency & Concurrency

### Event Processing

- Orderbook events are processed sequentially in the main event loop (same as existing events)
- No special idempotency handling required; events are naturally idempotent price updates

### Limit Order Placement

- Orders include `signal_id` to prevent duplicate orders from the same signal
- `PositionManager.try_mark_for_buying()` prevents concurrent buys for the same asset
- Executors should implement idempotency at the venue API level using client-generated order IDs

### Orderbook State Updates

- `OrderbookSnapshotEvent` replaces entire book state (naturally idempotent)
- `OrderbookUpdateEvent` is applied incrementally; out-of-order updates handled via timestamp/sequence

## Observability

### Logs

- `INFO`: "Processing orderbook snapshot for asset {asset_id}, {bids_count} bids, {asks_count} asks"
- `INFO`: "Processing orderbook update for asset {asset_id}, best_bid={best_bid}, best_ask={best_ask}"
- `INFO`: "Limit order placed: order_id={order_id}, asset={mint}, price={price}, size={size}"
- `DEBUG`: "Mid price calculated: asset={asset_id}, mid={mid_price}, spread={spread}"
- `WARN`: "Empty orderbook for asset {asset_id}, skipping price update"
- `ERROR`: "Limit order execution failed: {error}"

### Metrics

New Prometheus metrics:

```rust
// Event counters (extend existing)
token_events_total{type="orderbook_update"}
token_events_total{type="orderbook_snapshot"}

// Orderbook-specific metrics
orderbook_spread_gauge{asset_id}              // Current spread in price units
orderbook_mid_price_gauge{asset_id}           // Current mid price
orderbook_depth_gauge{asset_id, side}         // Total depth on each side

// Limit order metrics
limit_orders_placed_total{venue, status}      // placed, rejected
limit_orders_filled_total{venue}
limit_orders_cancelled_total{venue}
limit_order_latency_ms{venue}                 // Time from placement to fill
```

### Alerts

- Alert if `orderbook_spread_gauge` exceeds threshold (illiquid market)
- Alert if limit order fill rate drops below threshold
- Alert on sustained limit order rejection rate

## Security

- **API key protection**: Polymarket and other venue API keys must be stored securely (env vars or secrets manager)
- **Order signing**: Polymarket orders require cryptographic signing; private keys must never be logged
- **Rate limiting**: Respect venue rate limits to avoid account suspension
- **Input validation**: Validate order sizes and prices against configurable bounds before submission
- **No arbitrary code execution**: Signal generators cannot execute arbitrary code; they only produce data structures

## Strategy-Driven Exit Management

### Problem Statement

The current exit architecture assumes:
1. `ExitStrategy` answers: "Should I exit?" (boolean decision)
2. `PositionManager` answers: "How?" (always market order)
3. Exit is a single atomic action

For CLOB markets, exit management is fundamentally different:
- **Stateful process**: Place limit order → monitor → adjust → eventually fill or fallback
- **Strategy-driven method**: The "how" is part of the strategy, not infrastructure
- **Active order management**: Cancel, replace, ladder, TWAP, etc.
- **Market context required**: Orderbook state affects exit decisions

### Design Principles

1. **Exit is part of the strategy** - For CLOB, the "how" of exiting is strategy logic
2. **Backward compatible** - AMM strategies continue to work unchanged
3. **Unified signal model** - Exit actions flow through the same signal pipeline as entries
4. **Explicit state management** - Track active exit orders per position
5. **Safety nets preserved** - Hard limits override strategy management when necessary

### Signal Action Extension

Extend `TradableSignal` to support exit and order management signals:

```rust
/// Indicates what action a signal represents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalAction {
    /// Open a new position (current behavior, default)
    Entry,
    /// Close an existing position
    Exit,
    /// Modify an existing order (cancel + replace)
    ModifyOrder,
    /// Cancel an existing order
    CancelOrder,
}

pub trait TradableSignal: Send + Sync + Debug {
    // ... existing methods unchanged ...

    // NEW: Signal action (default to Entry for backward compatibility)
    fn signal_action(&self) -> SignalAction { SignalAction::Entry }

    // NEW: For Exit/Modify/Cancel signals - which position/order does this reference?
    fn references_position(&self) -> Option<&str> { self.get_mint() }
    fn references_order_id(&self) -> Option<&str> { None }

    // Already defined above, critical for CLOB exits:
    // fn get_limit_price(&self) -> Option<f64> { None }
    // fn is_limit_order(&self) -> bool { false }
    // fn get_time_in_force(&self) -> Option<TimeInForce> { None }
}
```

### Position State Extensions

Track active exit orders and exit mode per position:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Position {
    // ... existing fields unchanged ...

    /// Active exit order for this position (for CLOB markets)
    pub active_exit_order: Option<ActiveExitOrder>,

    /// Exit mode: automatic (ExitStrategy) or strategy-managed
    pub exit_mode: ExitMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveExitOrder {
    pub order_id: String,
    pub price: f64,
    pub size: f64,
    pub placed_at: DateTime<Utc>,
    pub side: OrderSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExitMode {
    /// ExitStrategy controls exit timing, market orders (current behavior)
    #[default]
    Automatic,
    /// SignalGenerator manages exit via Exit signals
    StrategyManaged,
}
```

### Exit Signal Types

New signal types for strategy-driven exit management:

```rust
/// A signal that requests closing a position
#[derive(Debug, Clone, Serialize)]
pub struct ExitSignal {
    pub signal_meta: SignalMetadata,
    pub mint: String,
    pub market: Option<String>,
    pub exit_type: ExitType,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum ExitType {
    /// Market sell (immediate, for AMM or urgent exits)
    Market,
    /// Limit sell at specified price
    Limit {
        price: f64,
        time_in_force: TimeInForce,
    },
}

impl TradableSignal for ExitSignal {
    fn signal_action(&self) -> SignalAction { SignalAction::Exit }
    fn signal_id(&self) -> &str { self.signal_meta.id() }
    fn signal_type(&self) -> &str { "exit" }
    fn get_mint(&self) -> Option<&str> { Some(&self.mint) }
    fn get_market(&self) -> Option<&str> { self.market.as_deref() }
    fn get_limit_price(&self) -> Option<f64> {
        match &self.exit_type {
            ExitType::Limit { price, .. } => Some(*price),
            ExitType::Market => None,
        }
    }
    fn is_limit_order(&self) -> bool {
        matches!(self.exit_type, ExitType::Limit { .. })
    }
    fn get_time_in_force(&self) -> Option<TimeInForce> {
        match &self.exit_type {
            ExitType::Limit { time_in_force, .. } => Some(*time_in_force),
            ExitType::Market => None,
        }
    }
    // ... other trait methods
}

/// A signal to modify an existing exit order (cancel + replace)
#[derive(Debug, Clone, Serialize)]
pub struct ModifyOrderSignal {
    pub signal_meta: SignalMetadata,
    pub mint: String,
    pub order_id: String,
    pub new_price: f64,
    pub new_size: Option<f64>,
    pub reason: String,
}

impl TradableSignal for ModifyOrderSignal {
    fn signal_action(&self) -> SignalAction { SignalAction::ModifyOrder }
    fn references_order_id(&self) -> Option<&str> { Some(&self.order_id) }
    // ...
}

/// A signal to cancel an exit order
#[derive(Debug, Clone, Serialize)]
pub struct CancelOrderSignal {
    pub signal_meta: SignalMetadata,
    pub mint: String,
    pub order_id: String,
    pub reason: String,
}

impl TradableSignal for CancelOrderSignal {
    fn signal_action(&self) -> SignalAction { SignalAction::CancelOrder }
    fn references_order_id(&self) -> Option<&str> { Some(&self.order_id) }
    // ...
}
```

### Extended PositionManager Trait

Add methods to track active exit orders:

```rust
#[async_trait]
pub trait PositionManager: Send + Sync {
    // ... existing methods unchanged ...

    // NEW: Active exit order management
    async fn set_active_exit_order(
        &self,
        mint: &str,
        order: ActiveExitOrder,
    ) -> Result<(), PositionError> {
        Ok(())  // Default no-op for backward compatibility
    }

    async fn clear_active_exit_order(&self, mint: &str) -> Result<(), PositionError> {
        Ok(())  // Default no-op
    }

    async fn get_active_exit_order(&self, mint: &str) -> Option<ActiveExitOrder> {
        None  // Default: no active exit order
    }

    async fn set_exit_mode(&self, mint: &str, mode: ExitMode) -> Result<(), PositionError> {
        Ok(())  // Default no-op
    }
}
```

### PositionHandler Signal Routing

Route signals based on intent:

```rust
impl PositionHandler {
    pub async fn handle_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        match signal.signal_action() {
            SignalAction::Entry => self.handle_entry_signal(signal).await,
            SignalAction::Exit => self.handle_exit_signal(signal).await,
            SignalAction::ModifyOrder => self.handle_modify_order_signal(signal).await,
            SignalAction::CancelOrder => self.handle_cancel_order_signal(signal).await,
        }
    }

    async fn handle_exit_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let mint = signal.get_mint()
            .ok_or_else(|| anyhow!("Exit signal missing mint"))?;

        let position = self.position_manager.get_position(mint).await
            .ok_or_else(|| anyhow!("No position found for mint {}", mint))?;

        // Create the exit order based on signal parameters
        let order = if signal.is_limit_order() {
            Order {
                mint: mint.to_string(),
                market: signal.get_market().map(String::from),
                order_type: OrderType::LimitSell {
                    token_amount: position.amount,
                    limit_price: signal.get_limit_price().unwrap(),
                    clear_position: true,
                    time_in_force: signal.get_time_in_force()
                        .unwrap_or(TimeInForce::GoodTilCancelled),
                },
                price: signal.get_limit_price(),
                status: OrderStatus::Pending,
                timestamp: signal.get_timestamp().unwrap_or_else(Utc::now),
                signal_slot: signal.get_slot(),
                signal_id: Some(signal.signal_id().to_string()),
                dex_type: None,
                venue_order_id: None,
            }
        } else {
            Order {
                mint: mint.to_string(),
                market: signal.get_market().map(String::from),
                order_type: OrderType::MarketSell {
                    token_amount: position.amount,
                    clear_position: true,
                },
                // ... rest of fields
            }
        };

        // Execute the order
        if signal.is_limit_order() {
            match self.order_executor.execute_limit_order(order).await {
                Ok(LimitOrderEvent::OrderPlaced { order_id, price, size, .. }) => {
                    // Track the active exit order on the position
                    self.position_manager.set_active_exit_order(mint, ActiveExitOrder {
                        order_id,
                        price,
                        size,
                        placed_at: Utc::now(),
                        side: OrderSide::Sell,
                    }).await?;
                }
                Ok(event) => {
                    self.event_coordinator
                        .enqueue_event(SystemEvent::LimitOrder(event)).await?;
                }
                Err(e) => {
                    error!("Failed to execute limit exit order: {:?}", e);
                    return Err(e);
                }
            }
        } else {
            let event = self.order_executor.execute_market_order(order).await?;
            self.event_coordinator
                .enqueue_event(SystemEvent::Execution(event)).await?;
        }

        Ok(())
    }

    async fn handle_modify_order_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let order_id = signal.references_order_id()
            .ok_or_else(|| anyhow!("Modify signal missing order_id"))?;
        let mint = signal.get_mint()
            .ok_or_else(|| anyhow!("Modify signal missing mint"))?;

        // Cancel existing order
        self.order_executor.cancel_order(order_id).await?;

        // Place new order at updated price
        let position = self.position_manager.get_position(mint).await
            .ok_or_else(|| anyhow!("No position found for mint {}", mint))?;

        let new_order = Order {
            mint: mint.to_string(),
            order_type: OrderType::LimitSell {
                token_amount: position.amount,
                limit_price: signal.get_limit_price()
                    .ok_or_else(|| anyhow!("Modify signal missing new price"))?,
                clear_position: true,
                time_in_force: signal.get_time_in_force()
                    .unwrap_or(TimeInForce::GoodTilCancelled),
            },
            // ...
        };

        match self.order_executor.execute_limit_order(new_order).await {
            Ok(LimitOrderEvent::OrderPlaced { order_id, price, size, .. }) => {
                self.position_manager.set_active_exit_order(mint, ActiveExitOrder {
                    order_id,
                    price,
                    size,
                    placed_at: Utc::now(),
                    side: OrderSide::Sell,
                }).await?;
            }
            // ...
        }

        Ok(())
    }

    async fn handle_cancel_order_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        let order_id = signal.references_order_id()
            .ok_or_else(|| anyhow!("Cancel signal missing order_id"))?;
        let mint = signal.get_mint()
            .ok_or_else(|| anyhow!("Cancel signal missing mint"))?;

        self.order_executor.cancel_order(order_id).await?;
        self.position_manager.clear_active_exit_order(mint).await?;

        Ok(())
    }
}
```

### ExitStrategy as Safety Net

For strategy-managed positions, `ExitStrategy` becomes a safety net with hard limits:

```rust
impl InMemoryPositionManager {
    async fn handle_timer(&self, event: &TimerEvent) -> Result<Vec<Order>, PositionError> {
        let mut orders = Vec::new();

        let positions_snapshot = {
            let state = self.state.lock().unwrap();
            state.positions.values().cloned().collect::<Vec<_>>()
        };

        for position in positions_snapshot {
            if self.is_pending_sell(&position.mint).await {
                continue;
            }

            match position.exit_mode {
                ExitMode::Automatic => {
                    // Current behavior: ExitStrategy decides, market order generated
                    if self.exit_strategy.should_exit(&position, event.timestamp) {
                        orders.push(self.create_market_sell_order(&position, event.timestamp));
                    }
                }
                ExitMode::StrategyManaged => {
                    // Safety net: force market sell if position exceeds absolute limits
                    if self.exceeds_safety_limits(&position, event.timestamp) {
                        tracing::warn!(
                            mint = %position.mint,
                            pnl_pct = ?position.pnl_pct,
                            "Safety limit exceeded, forcing market exit"
                        );
                        // Cancel any active exit order first
                        if let Some(active) = &position.active_exit_order {
                            let _ = self.cancel_active_exit_order(&position.mint, &active.order_id).await;
                        }
                        orders.push(self.create_market_sell_order(&position, event.timestamp));
                    }
                }
            }
        }

        Ok(orders)
    }

    fn exceeds_safety_limits(&self, position: &Position, current_time: DateTime<Utc>) -> bool {
        // Hard stop-loss that overrides strategy management
        if let Some(pnl_pct) = position.pnl_pct {
            if pnl_pct <= -50.0 {  // -50% absolute limit (configurable)
                return true;
            }
        }

        // Maximum holding period (e.g., 2x the configured max)
        let holding_duration = current_time - position.entry_time;
        if holding_duration > self.max_holding_period * 2 {
            return true;
        }

        false
    }
}
```

### Usage Example: CLOB Exit Strategy

A complete example of a strategy that manages its own exits:

```rust
pub struct ClobTradingStrategy {
    position_manager: Arc<dyn PositionManager>,
    exit_config: ExitConfig,
}

#[derive(Clone)]
struct ExitConfig {
    initial_take_profit_pct: f64,  // 0.10 = 10%
    order_timeout_secs: u64,       // 120 = 2 minutes before adjusting
    fallback_spread_pct: f64,      // 0.02 = 2% from mid for stale orders
}

#[async_trait]
impl SignalGenerator for ClobTradingStrategy {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        let mut signals: Vec<Box<dyn TradableSignal>> = Vec::new();

        match event {
            SystemEvent::Token(TokenEvent::OrderbookUpdate(update)) => {
                // Generate entry signals from orderbook...
                if let Some(entry) = self.evaluate_entry(update).await {
                    signals.push(Box::new(entry));
                }

                // Manage exits for existing positions
                signals.extend(self.manage_exits(update).await?);
            }
            _ => {}
        }

        Ok(signals)
    }
}

impl ClobTradingStrategy {
    async fn manage_exits(
        &self,
        update: &OrderbookUpdateEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        let mut signals: Vec<Box<dyn TradableSignal>> = Vec::new();

        let position = match self.position_manager.get_position(&update.asset_id).await {
            Some(p) if p.exit_mode == ExitMode::StrategyManaged => p,
            _ => return Ok(signals),
        };

        let pnl_pct = position.pnl_pct.unwrap_or(0.0) / 100.0;

        match &position.active_exit_order {
            None => {
                // No exit order yet - should we place one?
                if pnl_pct >= self.exit_config.initial_take_profit_pct {
                    signals.push(Box::new(ExitSignal {
                        signal_meta: SignalMetadata::new(),
                        mint: position.mint.clone(),
                        market: None,
                        exit_type: ExitType::Limit {
                            price: update.best_ask,  // Aggressive: at the ask
                            time_in_force: TimeInForce::GoodTilCancelled,
                        },
                        reason: "take_profit_target_reached".to_string(),
                    }));
                }
            }
            Some(active_order) => {
                // We have an exit order - should we adjust it?
                let order_age = Utc::now() - active_order.placed_at;

                if order_age.num_seconds() > self.exit_config.order_timeout_secs as i64 {
                    // Order is stale, move price closer to market
                    let new_price = update.best_bid * (1.0 - self.exit_config.fallback_spread_pct);

                    signals.push(Box::new(ModifyOrderSignal {
                        signal_meta: SignalMetadata::new(),
                        mint: position.mint.clone(),
                        order_id: active_order.order_id.clone(),
                        new_price,
                        new_size: None,
                        reason: "order_timeout_adjustment".to_string(),
                    }));
                }
            }
        }

        Ok(signals)
    }
}
```

### State Tracking Summary

| What | Tracked By | Notes |
|------|-----------|-------|
| Settled positions (token holdings) | `PositionManager.positions` | Existing behavior |
| Open exit orders for positions | `Position.active_exit_order` | New field |
| Exit management mode | `Position.exit_mode` | New field |
| Open entry orders | `PositionManager.pending_limit_orders` | Existing in spec |
| Order lifecycle events | `LimitOrderEvent` → `SystemEvent` | Existing in spec |

### Backward Compatibility

All changes are additive with defaults that preserve existing behavior:

| Component | Change | Default Behavior |
|-----------|--------|------------------|
| `TradableSignal.signal_action()` | New method | Returns `SignalAction::Entry` |
| `TradableSignal.references_order_id()` | New method | Returns `None` |
| `Position.exit_mode` | New field | `ExitMode::Automatic` |
| `Position.active_exit_order` | New field | `None` |
| `PositionManager.set_active_exit_order()` | New method | No-op |
| `handle_timer()` behavior | Conditional on `exit_mode` | Same as current for `Automatic` |

## References

- `popeyes_trading_types` crate: `/home/zfeng/popeyes/trading-types/src/trade_event.rs`
- Trade server architecture: `docs/architecture.md`
- Trade server usage guide: `docs/usage-guide.md`
- Existing signal trait: `src/signal/sig.rs`
- Existing order types: `src/execution/order.rs`
- Existing executor trait: `src/execution/executor.rs`
- TradeServer main loop: `src/trade_server/trade_server.rs`
