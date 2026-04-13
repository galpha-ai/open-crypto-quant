# Intent-Based Position Management

For orderbook market making strategies, the trade server provides an intent-based model where strategies express **desired order state** rather than discrete actions. The PositionManager automatically reconciles current pending orders with the desired state and generates the necessary actions.

## Why Intent-Based?

Traditional action-based signals require strategies to track order state and emit discrete place/cancel signals. This leads to:

- Complex state management in strategy code
- Race conditions between signals and order events
- Non-idempotent behavior (duplicate signals cause issues)

Intent-based signals solve these problems:

| Benefit         | Description                                    |
|-----------------|------------------------------------------------|
| **Simplicity**  | Express desired quotes, not discrete actions   |
| **Idempotency** | Same intent + same state = no orders generated |
| **Robustness**  | Reconciliation handles races and edge cases    |

## OrderIntent Structure

```rust
use trade_server::signal::{OrderIntent, QuoteLevel};

// Desired order book state
let intent = OrderIntent::new(
    "token123".to_string(),           // Asset identifier
    Some("market456".to_string()),    // Market ID for CLOB venues
    Some(vec![                        // Desired bids
        QuoteLevel::gtc(0.45, 100.0), // Bid at 0.45, size 100
        QuoteLevel::gtc(0.44, 50.0),  // Bid at 0.44, size 50
    ]),
    Some(vec![                        // Desired asks
        QuoteLevel::gtc(0.55, 100.0), // Ask at 0.55, size 100
    ]),
    "signal_id".to_string(),
    Utc::now(),
    None,                             // Optional debug context
);
```

## Option Semantics

The `bids` and `asks` fields use `Option` semantics:

| Value               | Meaning                                           |
|---------------------|---------------------------------------------------|
| `None`              | No change to this side (preserve existing orders) |
| `Some([])`          | Cancel all orders on this side                    |
| `Some([levels...])` | Desired state for this side (reconcile to match)  |

### Examples

```rust
// Cancel all bids, preserve existing asks
let intent = OrderIntent::new(
    mint, market,
    Some(vec![]),  // Cancel all bids
    None,          // Preserve asks
    signal_id, timestamp,
    None,          // Optional debug context
);

// Cancel all orders on both sides
let intent = OrderIntent::cancel_all(mint, market, signal_id, timestamp);
```

## Implementing an Intent Signal

To create an intent-based signal, implement `TradableSignal` with:

1. `is_intent_signal()` returning `true`
2. `get_order_intent()` returning the desired `OrderIntent`

```rust
use trade_server::signal::{TradableSignal, SignalIntent, OrderIntent, QuoteLevel};

#[derive(Debug)]
struct MarketMakingSignal {
    signal_id: String,
    mint: String,
    market: String,
    desired_bids: Vec<QuoteLevel>,
    desired_asks: Vec<QuoteLevel>,
    timestamp: DateTime<Utc>,
}

impl TradableSignal for MarketMakingSignal {
    fn signal_id(&self) -> &str { &self.signal_id }
    fn signal_type(&self) -> &str { "market_making" }
    fn get_mint(&self) -> Option<&str> { Some(&self.mint) }
    fn get_market(&self) -> Option<&str> { Some(&self.market) }
    fn get_price(&self) -> Option<f64> { None }
    fn get_timestamp(&self) -> Option<DateTime<Utc>> { Some(self.timestamp) }
    fn get_slot(&self) -> Option<u64> { None }
    fn get_creator(&self) -> Option<Pubkey> { None }
    fn passes_filter(&self) -> bool { true }
    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> { None }
    fn as_any(&self) -> &dyn Any { self }
    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "signal_id": self.signal_id,
            "mint": self.mint,
        }))
    }

    // Intent-based signal methods
    fn is_intent_signal(&self) -> bool {
        true  // This is an intent signal
    }

    fn get_order_intent(&self) -> Option<OrderIntent> {
        Some(OrderIntent::new(
            self.mint.clone(),
            Some(self.market.clone()),
            Some(self.desired_bids.clone()),
            Some(self.desired_asks.clone()),
            self.signal_id.clone(),
            self.timestamp,
            self.get_context(),  // Pass through debug context
        ))
    }
}
```

## How Reconciliation Works

### Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                  Signal Generator (Stateless)                   │
│                                                                 │
│  Input:                           Output:                       │
│  ├─ PositionEvent (position)      └─ OrderIntent                │
│  └─ OrderbookSnapshot (market)        ├─ desired_bids: [...]    │
│                                       └─ desired_asks: [...]    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│               PositionManager.reconcile_intent()                │
│                                                                 │
│  1. Gets current pending_limit_orders (internal state)          │
│  2. Computes diff: current vs desired                           │
│  3. Returns: Vec<Order> (cancels + placements)                  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       OrderExecutor                             │
│                                                                 │
│  Executes cancel orders first, then limit order placements      │
└─────────────────────────────────────────────────────────────────┘
```

The signal generator is a pure function of `(position, orderbook) → desired_quotes`. It does not need to know about pending orders - the PositionManager handles all stateful reconciliation internally.

### Processing Steps

When an intent signal is processed:

1. **Extract Intent**: `signal.get_order_intent()` returns the desired state
2. **Get Current State**: PositionManager retrieves current pending orders
3. **Compute Diff**: Reconciliation engine compares current vs. desired:
   - Orders to cancel: current orders not matching any desired level
   - Orders to place: desired levels not matching any current order
4. **Execute Actions**: Cancel orders are executed first, then placements

**Matching Logic**: Two levels match if both price and size are equal within 1e-9 tolerance.

## Position Events and Inventory

The signal generator receives `PositionEvent` updates containing complete position state. The `Position` struct includes inventory information needed for market making:

```rust
struct Position {
    pub mint: String,
    pub amount: f64,              // Current inventory (position size)
    pub entry_price: Option<f64>, // Average entry price
    pub current_price: Option<f64>,
    pub pnl_pct: Option<f64>,     // Computed P&L percentage
    // ...
}
```

### PositionEvent Variants

```rust
enum PositionEvent {
    PositionCreated(Position),
    PositionUpdated { position: Position, source: PositionUpdateSource },
    PositionClosed { position: Position, ... },
}

enum PositionUpdateSource {
    Fill { fill_price, fill_size, side, order_id },  // Order filled
    Reconciliation { previous_amount, drift },        // Exchange sync correction
    Manual,                                            // API/admin adjustment
    PriceUpdate,                                       // Mark-to-market only
}
```

### Handle All PositionUpdated Events

Signal generators should handle **all** `PositionUpdated` events uniformly, regardless of source:

| Source | Why Handle |
|--------|-----------|
| `Fill` | Inventory changed from trade execution |
| `Reconciliation` | Inventory corrected due to missed WS messages or API sync |
| `Manual` | External adjustment changed inventory |
| `PriceUpdate` | Inventory unchanged, but safe to handle uniformly |

Thanks to intent-based idempotency, handling all events is safe: if nothing meaningful changed, the same intent produces the same desired state, and reconciliation becomes a no-op.

## Signal Generator Example

A market maker that reacts to both orderbook updates and position changes:

```rust
use trade_server::signal::SignalGenerator;
use trade_server::domain::SystemEvent;
use trade_server::position::{PositionEvent, Position};

struct MarketMaker {
    spread_bps: f64,
    base_size: f64,
    // Cache latest state for computing quotes
    last_orderbook: Option<OrderbookSnapshotEvent>,
    last_position: Option<Position>,
}

#[async_trait]
impl SignalGenerator for MarketMaker {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        // Update cached state based on event type
        match event {
            SystemEvent::Token(TokenEvent::OrderbookSnapshot(snapshot)) => {
                self.last_orderbook = Some(snapshot.clone());
            }
            SystemEvent::Position(pos_event) => {
                // Handle all PositionUpdated events uniformly
                let position = match pos_event {
                    PositionEvent::PositionCreated(p) => Some(p.clone()),
                    PositionEvent::PositionUpdated { position, .. } => Some(position.clone()),
                    PositionEvent::PositionClosed { .. } => None,
                };
                self.last_position = position;
            }
            _ => return Ok(vec![]),
        }

        // Compute quotes from current state
        self.compute_quotes()
    }
}

impl MarketMaker {
    fn compute_quotes(&self) -> Result<Vec<Box<dyn TradableSignal>>> {
        let orderbook = match &self.last_orderbook {
            Some(ob) => ob,
            None => return Ok(vec![]),
        };

        let mid_price = (orderbook.best_bid()? + orderbook.best_ask()?) / 2.0;
        let spread = mid_price * self.spread_bps / 10000.0;

        // Skew quotes based on inventory
        let inventory = self.last_position.as_ref().map(|p| p.amount).unwrap_or(0.0);
        let skew = inventory * 0.001; // Example: 0.1% skew per unit

        let bid_price = mid_price - spread / 2.0 - skew;
        let ask_price = mid_price + spread / 2.0 - skew;

        // Adjust size based on inventory (reduce when position grows)
        let size = (self.base_size - inventory.abs() * 0.1).max(0.0);

        if size <= 0.0 {
            // Cancel all quotes when at max inventory
            return Ok(vec![Box::new(MarketMakingSignal::cancel_all(
                orderbook.asset_id.clone(),
                orderbook.market_id.clone(),
            ))]);
        }

        Ok(vec![Box::new(MarketMakingSignal {
            signal_id: uuid::Uuid::new_v4().to_string(),
            mint: orderbook.asset_id.clone(),
            market: orderbook.market_id.clone(),
            desired_bids: vec![QuoteLevel::gtc(bid_price, size)],
            desired_asks: vec![QuoteLevel::gtc(ask_price, size)],
            timestamp: Utc::now(),
        })])
    }
}
```

Key points:
- Cache both orderbook and position state
- Recompute quotes on **any** relevant event (orderbook or position update)
- Use `Position.amount` for inventory-based quote skewing
- Handle all `PositionUpdated` variants uniformly

## Pending Order Tracking

The PositionManager tracks pending limit orders to enable accurate reconciliation:

```rust
// LimitOrderEvent updates pending order state
pub enum LimitOrderEvent {
    OrderPlaced { order_id, mint, market, price, size, side, timestamp },
    OrderPartiallyFilled { order_id, filled_size, remaining_size, fill_price, timestamp },
    OrderCancelled { order_id, reason, timestamp },
    OrderExpired { order_id, timestamp },
    OrderRejected { reason },
}
```

| Event                  | Action                     |
|------------------------|----------------------------|
| `OrderPlaced`          | Add to pending orders      |
| `OrderPartiallyFilled` | Update remaining_size      |
| `OrderCancelled`       | Remove from pending orders |
| `OrderExpired`         | Remove from pending orders |
| `OrderRejected`        | No-op (was never pending)  |

## Combining with Exit Modes

Intent-based signals work with Strategy-Managed exit mode for full control:

```rust
// Position opened with Strategy-Managed exit mode
position.exit_mode = ExitMode::StrategyManaged;

// Strategy emits intent signals to manage quotes
// Safety limits still apply as a backstop
```

See [Orderbook Trading](orderbook-trading.md) for more on exit modes.

## Pair Redemption for Binary Markets

For binary prediction markets (Up/Down), holding paired tokens can be redeemed for quote currency. The intent-based system supports automatic pair redemption through redemption policies.

### How Pair Redemption Works

In binary markets, 1 Up + 1 Down token can always be redeemed for exactly $1. This is useful for:

1. **Guaranteeing profit** when pair cost < $1
2. **Reducing market risk** from holding unredeemed pairs
3. **Freeing up capital** locked in paired inventory

### RedemptionPolicy

A redemption policy is a declarative constraint on paired inventory:

```rust
use trade_server::signal::RedemptionPolicy;

// Create a policy that keeps unredeemed pairs below $50
let policy = RedemptionPolicy::new(
    "market-123".to_string(),  // Market identifier
    "UP-asset-id".to_string(), // Up token asset ID
    "DOWN-asset-id".to_string(), // Down token asset ID
    50.0,                      // max_unredeemed_pair_value
);
```

When `min(up_qty, down_qty) > max_unredeemed_pair_value`, the system automatically redeems the excess.

### Adding Redemption Policy to OrderIntent

Attach a redemption policy to your intent signals:

```rust
let intent = OrderIntent::new(
    mint.clone(),
    Some(market.clone()),
    Some(desired_bids),
    Some(desired_asks),
    signal_id,
    timestamp,
    None,
).with_redemption_policy(RedemptionPolicy::new(
    market,
    up_asset_id,
    down_asset_id,
    50.0,  // Redeem when paired value exceeds $50
));
```

### Dynamic Policy Adjustment

Strategies can adjust the policy threshold based on market conditions:

```rust
// During active trading: tolerate capital lock-up
let policy = RedemptionPolicy::new(market, up, down, 50.0);

// Approaching market close: start tightening
let policy = RedemptionPolicy::new(market, up, down, 5.0);

// After market close: redeem everything
let policy = RedemptionPolicy::new(market, up, down, 0.1);
```

### Redemption Data Flow

```
Signal (with policy) -> IntentHandler -> update policy -> reconcile
Fill event -> PositionHandler -> update position -> reconcile
                                                        |
                              if constraint violated: generate RedemptionAction
                                                        |
                                              execute redemption
                                                        |
                                              RedemptionEvent
                                                        |
                                              update positions
```

### RedemptionEvent

When a redemption executes, you'll receive a `RedemptionEvent`:

```rust
pub enum RedemptionEvent {
    RedemptionCompleted {
        market: String,
        up_asset_id: String,
        down_asset_id: String,
        quantity: f64,        // Pairs redeemed
        quote_received: f64,  // Quote currency received ($1 per pair)
        timestamp: DateTime<Utc>,
    },
    RedemptionFailed {
        market: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
}
```

### Position Updates from Redemption

When a redemption completes:
- Both Up and Down positions are reduced by the redeemed quantity
- The quote received is added to the balance tracker
- If a position is fully redeemed, it closes with `ExitReason::Redemption`

### Executor Support

Redemption requires executor support. Check if your executor supports it:

```rust
if order_executor.supports_redemption() {
    // Redemption policies will be enforced
}
```

The backtest executor supports redemption. Live executors may need implementation.

## Next Steps

- [Orderbook Trading](orderbook-trading.md) - OrderbookTracker and signal intents
- [Core Traits](core-traits.md) - Full TradableSignal interface
