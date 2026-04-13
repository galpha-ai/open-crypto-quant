# Polymarket Order Executor

## Summary

This design implements a `PolymarketOrderExecutor` that satisfies the `OrderExecutor` trait defined in `src/execution/executor.rs`. The executor enables the trade server to place and manage orders on Polymarket's Central Limit Order Book (CLOB) using the [polyfill-rs](https://github.com/floor-licker/polyfill-rs) library.

**Why polyfill-rs over polymarket-rs-client?**
- **Fixed-point optimization**: Uses `BTreeMap<u32, i64>` instead of `BTreeMap<Decimal, Decimal>` for orderbook operations, providing 2-10x speedup
- **API compatible**: Drop-in replacement for polymarket-rs-client with identical method signatures
- **Performance-critical paths**: Sub-microsecond orderbook updates via zero-allocation integer arithmetic
- **Built-in orderbook management**: `OrderBookManager` with depth limiting and stale data cleanup

## Goals

- Implement full `OrderExecutor` trait compliance for Polymarket
- Support limit order placement, cancellation, and fill tracking
- Support market order execution via FOK/FAK aggressive limit orders
- Integrate with Polymarket's WebSocket for real-time order updates and fills
- Track pending orders and emit appropriate `LimitOrderEvent` and `ExecutionEvent`

## Non-Goals

- **Multi-market execution**: Single market per executor instance
- **Portfolio margin**: Not modeling margin requirements
- **Historical data loading**: Handled by backtest infrastructure
- **Price improvement**: Orders placed at exact specified prices

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                     PolymarketOrderExecutor                         │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐ │
│  │   ClobClient    │  │  WebSocketStream│  │   PendingOrders     │ │
│  │  (polyfill-rs)  │  │  (polyfill-rs)  │  │   HashMap           │ │
│  │                 │  │                 │  │                     │ │
│  │ - post_order()  │  │ - user channel  │  │ - order_id -> Order │ │
│  │ - cancel()      │  │ - fill events   │  │ - fill tracking     │ │
│  │ - get_order()   │  │ - auto-reconnect│  │                     │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────────┘ │
│  ┌─────────────────┐  ┌─────────────────┐                          │
│  │ OrderBookManager│  │   FillEngine    │                          │
│  │ (fixed-point)   │  │ (market impact) │                          │
│  │                 │  │                 │                          │
│  │ - BTreeMap<u32> │  │ - slippage calc │                          │
│  │ - fast spread   │  │ - fill simulate │                          │
│  │ - depth limit   │  │                 │                          │
│  └─────────────────┘  └─────────────────┘                          │
├─────────────────────────────────────────────────────────────────────┤
│                        EventCoordinator                             │
│                     (for enqueueing fill events)                    │
└─────────────────────────────────────────────────────────────────────┘
```

## API

### PolymarketOrderExecutor

```rust
use polyfill_rs::{ClobClient, OrderBookManager, FillEngine, WebSocketStream, OrderArgs, Side, OrderType};

pub struct PolymarketOrderExecutor {
    /// Polymarket CLOB client for REST API calls (polyfill-rs)
    client: Arc<ClobClient>,

    /// Event coordinator for enqueueing fill events
    event_coordinator: Arc<dyn EventCoordinator>,

    /// Pending orders awaiting fill (order_id -> PendingOrder)
    pending_orders: Arc<RwLock<HashMap<String, PendingOrder>>>,

    /// High-performance orderbook manager with fixed-point arithmetic
    /// Uses BTreeMap<u32, i64> internally for sub-microsecond operations
    book_manager: Arc<OrderBookManager>,

    /// Fill simulation engine for market impact calculations
    fill_engine: Arc<FillEngine>,

    /// Configuration
    config: PolymarketConfig,

    /// WebSocket stream for real-time updates (auto-reconnect)
    ws_stream: Option<Arc<Mutex<WebSocketStream>>>,
}

pub struct PolymarketConfig {
    /// CLOB API host URL
    pub host: String,

    /// Private key for signing orders (hex string)
    pub private_key: String,

    /// Chain ID (137 for Polygon mainnet)
    pub chain_id: u64,

    /// API credentials (key, secret, passphrase)
    pub api_creds: Option<polyfill_rs::ApiCredentials>,

    /// Condition ID for the market to trade
    pub condition_id: String,

    /// Token ID (outcome token) to trade
    pub token_id: String,

    /// Default slippage for market orders (e.g., 0.01 = 1%)
    pub default_slippage: f64,

    /// Maximum retries for failed API calls (uses polyfill-rs RetryConfig)
    pub max_retries: u32,

    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,

    /// Orderbook depth to maintain (default: 50 levels)
    /// More levels = more memory but better market impact calculation
    pub orderbook_depth: usize,

    /// Fee rate in basis points for fill simulation
    pub fee_rate_bps: u32,
}

pub struct PendingOrder {
    pub order_id: String,
    pub mint: String,
    pub market: Option<String>,
    pub side: OrderSide,
    pub price: f64,
    pub original_size: f64,
    pub remaining_size: f64,
    pub last_known_filled: f64,  // Track filled amount for WebSocket updates
    pub placed_at: DateTime<Utc>,
    pub time_in_force: TimeInForce,
    pub signal_id: Option<String>,
}

// OrderbookState is managed by polyfill_rs::OrderBookManager
// which uses fixed-point BTreeMap<Price, Qty> internally:
//   type Price = u32;  // price in ticks (0.0001 precision)
//   type Qty = i64;    // size in fixed-point units
//
// Access via book_manager.get_book(token_id) -> OrderBook snapshot
```

### Builder Pattern

```rust
pub struct PolymarketOrderExecutorBuilder {
    host: Option<String>,
    private_key: Option<String>,
    chain_id: u64,
    api_creds: Option<polyfill_rs::ApiCredentials>,
    condition_id: Option<String>,
    token_id: Option<String>,
    default_slippage: f64,
    max_retries: u32,
    retry_delay_ms: u64,
    orderbook_depth: usize,
    fee_rate_bps: u32,
    event_coordinator: Option<Arc<dyn EventCoordinator>>,
}

impl PolymarketOrderExecutorBuilder {
    pub fn new() -> Self;
    pub fn host(self, host: impl Into<String>) -> Self;
    pub fn private_key(self, key: impl Into<String>) -> Self;
    pub fn chain_id(self, chain_id: u64) -> Self;
    pub fn api_creds(self, creds: polyfill_rs::ApiCredentials) -> Self;
    pub fn condition_id(self, id: impl Into<String>) -> Self;
    pub fn token_id(self, id: impl Into<String>) -> Self;
    pub fn default_slippage(self, slippage: f64) -> Self;
    pub fn orderbook_depth(self, depth: usize) -> Self;  // default: 50
    pub fn fee_rate_bps(self, bps: u32) -> Self;         // default: 0
    pub fn event_coordinator(self, coordinator: Arc<dyn EventCoordinator>) -> Self;
    pub async fn build(self) -> Result<PolymarketOrderExecutor>;
}
```

### OrderExecutor Implementation

```rust
#[async_trait]
impl OrderExecutor for PolymarketOrderExecutor {
    /// Execute market order using FOK/FAK limit order at calculated market price
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent>;

    /// Execute limit order on Polymarket CLOB
    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent>;

    /// Cancel an existing order
    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent>;

    /// Update internal orderbook state from snapshot
    async fn handle_orderbook_snapshot(&self, snapshot: &OrderbookSnapshotEvent) -> Result<()>;

    /// Update internal orderbook state from incremental update
    async fn handle_orderbook_update(&self, update: &OrderbookUpdateEvent) -> Result<()>;

    /// Handle trade events - check if any pending orders were filled
    async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()>;

    /// Returns true - Polymarket supports limit orders
    fn supports_limit_orders(&self) -> bool { true }
}
```

### Errors

| Error | Description | Handling |
|-------|-------------|----------|
| `AuthenticationError` | Invalid API credentials or signature | Re-authenticate or check credentials |
| `InsufficientBalance` | Not enough USDC balance | Reject order with clear reason |
| `OrderRejected` | Order rejected by exchange | Return `LimitOrderEvent::OrderRejected` |
| `NetworkError` | Connection failed | Retry with exponential backoff |
| `RateLimitExceeded` | Too many requests | Backoff and retry |
| `InvalidOrder` | Malformed order parameters | Return rejection with details |
| `WebSocketDisconnected` | Lost real-time connection | Reconnect automatically |

## Behavior

### Initialization

1. Create `ClobClient` with appropriate configuration:
   ```rust
   // For co-located servers (aggressive settings)
   let client = ClobClient::new_colocated(&config.host);
   // Or for internet connections (conservative, reliable)
   let client = ClobClient::new_internet(&config.host);
   ```
2. Set up L1/L2 headers for authentication:
   ```rust
   client.set_l1_headers(&config.private_key, config.chain_id);
   let api_creds = client.create_or_derive_api_key(None).await?;
   client.set_api_creds(api_creds);
   ```
3. Initialize `OrderBookManager` with configured depth:
   ```rust
   let book_manager = OrderBookManager::new(config.orderbook_depth);
   ```
4. Initialize `FillEngine` for market impact calculations:
   ```rust
   let fill_engine = FillEngine::new(
       Decimal::from_str(&config.default_slippage.to_string())?,
       Decimal::ZERO,  // fee_rate
       config.fee_rate_bps,
   );
   ```
5. Start `WebSocketStream` for real-time updates (auto-reconnect enabled):
   ```rust
   let ws_stream = WebSocketStream::new("wss://ws-subscriptions-clob.polymarket.com/ws")
       .with_auth(auth)
       .with_reconnect_config(ReconnectConfig::default());
   ws_stream.subscribe_market_channel(vec![config.token_id.clone()]).await?;
   ```
6. Verify connectivity with health check

### Execute Market Order

Market orders are implemented as aggressive limit orders. Polymarket does not have native market orders - all orders are limit orders with different time-in-force settings.

**Order Types for Market Execution:**
- **FOK (Fill-Or-Kill)**: Must fill entirely or cancel completely
- **FAK (Fill-And-Kill)**: Fill as much as possible, cancel remainder (equivalent to IOC)

**Price Calculation and Market Impact (using polyfill-rs FillEngine):**

```rust
// Use FillEngine to simulate market order before execution
let book = self.book_manager.get_book(&config.token_id)?;
let order_request = MarketOrderRequest {
    token_id: config.token_id.clone(),
    side: order.side(),
    amount: order.quote_amount(),
    slippage_tolerance: Some(Decimal::from_str(&config.default_slippage.to_string())?),
    client_id: None,
};

// Simulate to check market impact before actual execution
let fill_result = self.fill_engine.execute_market_order(&order_request, &book)?;

// fill_result contains:
//   - average_price: VWAP across all fills
//   - total_size: shares to receive/sell
//   - fees: estimated fees
//   - impact_pct: market impact in percentage
```

**Alternative: Direct orderbook calculation (faster for simple cases):**

```rust
// Use OrderBook's built-in market impact calculation
let impact = book.calculate_market_impact(side, size)?;
// Returns: MarketImpact { average_price, impact_pct, total_cost, size_filled }
```

**Execution Flow:**

```
1. Get orderbook from book_manager (uses cached fixed-point data)
   let book = book_manager.get_book(token_id)?;

2. Calculate execution price using FillEngine or book.calculate_market_impact()
   - Handles slippage calculation
   - Returns VWAP and impact percentage

3. Validate price is in range: tick_size <= price <= (1 - tick_size)

4. Create signed order via ClobClient.create_and_post_order():
   let order_args = OrderArgs::new(token_id, price, size, side);
   let result = client.create_and_post_order(&order_args).await?;

5. For FOK/FAK orders, use post_order with specific OrderType:
   let signed = client.create_order(&order_args, None, None, None).await?;
   let result = client.post_order(signed, OrderType::FOK).await?;

6. Wait for fill confirmation (WebSocket auto-delivers via StreamMessage)

7. Return ExecutionEvent with actual fill details
```

### Execute Limit Order

```
1. Validate order parameters:
   - Price must be in range: tick_size <= price <= (1 - tick_size)
   - Size must be >= minimum_order_size
   - token_id must be valid

2. Fetch market metadata if not cached:
   - tick_size via ClobClient.get_tick_size(token_id)
   - neg_risk via ClobClient.get_neg_risk(token_id)

3. Create signed order using ClobClient.create_order():
   - Builds OrderArgs { token_id, price, size, side }
   - Calculates maker_amount and taker_amount based on side
   - Signs with EIP-712 using private key

4. Submit order via ClobClient.post_order(signed_order, OrderType::GTC)
   - Returns order_id on success

5. Add to pending_orders map for fill tracking

6. Return LimitOrderEvent::OrderPlaced
```

**Price/Amount Calculation:**

```rust
// For BUY orders: spending USDC to get shares
maker_amount = size * price  // USDC to spend
taker_amount = size          // shares to receive

// For SELL orders: selling shares to get USDC
maker_amount = size          // shares to sell
taker_amount = size * price  // USDC to receive
```

### Cancel Order

```
1. Call ClobClient.cancel(order_id)
2. Remove from pending_orders map
3. Return LimitOrderEvent::OrderCancelled
```

### Fill Detection

**IMPORTANT**: Unlike Solana where trades include `signature` and `trader_public_key`, Polymarket's public trade events are **completely anonymous** - they do not include `order_id` or any way to identify whose order was filled. Therefore, `handle_token_trade` **cannot be used for fill detection**.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Solana (PumpFun/Raydium)              │  Polymarket                        │
├────────────────────────────────────────┼────────────────────────────────────┤
│  Trade Event includes:                 │  Trade Event includes:             │
│  - signature ✅                        │  - asset_id                        │
│  - trader_public_key ✅                │  - price, size, side               │
│  - Can match YOUR trade                │  - ❌ NO order_id                  │
│                                        │  - ❌ NO trader info               │
│                                        │  - Cannot identify whose trade     │
└────────────────────────────────────────┴────────────────────────────────────┘
```

**Primary: WebSocket User Channel (Required for Live Trading)**

The User Channel is a **private, authenticated** channel that provides updates for YOUR orders only.

```rust
// Subscribe to User Channel (requires API key authentication)
ws_stream.subscribe_user_channel().await?;

// User Channel provides two types of messages:

// 1. Order Update - emitted on placement, partial fill, or cancellation
{
    "id": "order_123",           // YOUR order ID
    "type": "UPDATE",            // PLACEMENT | UPDATE | CANCELLATION
    "size_matched": 50.0,        // Total filled so far
    "original_size": 100.0,
    "associate_trades": [...]    // Related trade IDs
}

// 2. Trade Message - emitted when your order is matched
{
    "taker_order_id": "xxx",
    "maker_orders": [{
        "order_id": "order_123",  // YOUR order ID
        "matched_amount": 50.0,
    }],
    "status": "MATCHED",         // MATCHED → MINED → CONFIRMED
    "price": 0.65,
    "size": 50.0
}
```

**Fill Detection Flow:**

```rust
async fn handle_user_channel_message(&self, msg: UserChannelMessage) -> Result<()> {
    match msg {
        UserChannelMessage::OrderUpdate { id, type_, size_matched, .. } => {
            if type_ == "UPDATE" {
                // Partial or full fill
                let pending = self.pending_orders.read().await.get(&id).cloned();
                if let Some(order) = pending {
                    let new_fill = size_matched - order.last_known_filled;
                    if new_fill > 0.0 {
                        let remaining = order.original_size - size_matched;
                        let event = LimitOrderEvent::OrderPartiallyFilled {
                            order_id: id.clone(),
                            filled_size: new_fill,
                            remaining_size: remaining,
                            fill_price: order.price,
                            timestamp: Utc::now(),
                        };

                        // Update tracking
                        self.pending_orders.write().await
                            .get_mut(&id)
                            .map(|o| o.last_known_filled = size_matched);

                        // Remove if fully filled
                        if remaining <= 0.0 {
                            self.pending_orders.write().await.remove(&id);
                        }

                        // Enqueue for position manager
                        self.event_coordinator
                            .enqueue_event(SystemEvent::LimitOrder(event))
                            .await?;
                    }
                }
            }
        }
        UserChannelMessage::OrderUpdate { id, type_: "CANCELLATION", .. } => {
            self.pending_orders.write().await.remove(&id);
            // Emit cancellation event...
        }
        _ => {}
    }
    Ok(())
}
```

**Fallback: Polling (For Reliability)**

```rust
// Periodically poll order status as a fallback
async fn poll_pending_orders(&self) -> Result<()> {
    for (order_id, order) in self.pending_orders.read().await.iter() {
        let status = self.client.get_order(order_id).await?;

        if status.size_matched > order.last_known_filled {
            // Detected a fill that WebSocket missed
            // Emit LimitOrderEvent...
        }
    }
    Ok(())
}
```

**What handle_token_trade Does (NOT fill detection):**

```rust
/// For Polymarket, handle_token_trade only updates the orderbook cache.
/// It CANNOT be used for fill detection because public trades are anonymous.
async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()> {
    // Only update last trade price for market order pricing
    if let TokenTradeEvent::Polymarket(pm_trade) = trade {
        self.book_manager.update_last_price(
            &pm_trade.asset_id,
            pm_trade.price
        )?;
    }
    // Do NOT attempt to match against pending_orders - trades are anonymous!
    Ok(())
}
```

### Orderbook State Management

polyfill-rs `OrderBookManager` provides high-performance orderbook management with fixed-point arithmetic:

```rust
async fn handle_orderbook_snapshot(&self, snapshot: &OrderbookSnapshotEvent) -> Result<()> {
    // Get or create book for this token
    let _ = self.book_manager.get_or_create_book(&snapshot.token_id)?;

    // Apply each bid/ask level as a delta
    for (price, size) in &snapshot.bids {
        let delta = OrderDelta {
            token_id: snapshot.token_id.clone(),
            timestamp: snapshot.timestamp,
            side: Side::BUY,
            price: *price,
            size: *size,
            sequence: snapshot.sequence,
        };
        self.book_manager.apply_delta(delta)?;
    }
    // Similar for asks...
    Ok(())
}

async fn handle_orderbook_update(&self, update: &OrderbookUpdateEvent) -> Result<()> {
    // Apply incremental delta (uses fixed-point internally - very fast)
    let delta = OrderDelta {
        token_id: update.token_id.clone(),
        timestamp: update.timestamp,
        side: update.side,
        price: update.price,
        size: update.size,  // 0 = remove level
        sequence: update.sequence,
    };

    // This is the hot path - uses BTreeMap<u32, i64> internally
    // Performance: ~5ns per operation vs ~100ns with Decimal
    self.book_manager.apply_delta(delta)?;
    Ok(())
}

// Periodic cleanup of stale books (prevents memory leaks)
async fn cleanup_stale_books(&self) -> Result<()> {
    let removed = self.book_manager.cleanup_stale_books(Duration::from_secs(300))?;
    if removed > 0 {
        info!("Cleaned up {} stale order books", removed);
    }
    Ok(())
}
```

**Performance characteristics:**
- `apply_delta`: ~5ns per operation (fixed-point integer operations)
- `spread()` / `mid_price()`: ~3ns (uses `spread_fast()` / `mid_price_fast()` internally)
- Memory: ~32 bytes per price level × configured depth

## Data Model

### Order Mapping

| Trade Server | Polymarket |
|--------------|------------|
| `Order.mint` | `token_id` (asset_id) |
| `Order.market` | `condition_id` |
| `OrderType::LimitBuy` | `Side::BUY` |
| `OrderType::LimitSell` | `Side::SELL` |
| `TimeInForce::GoodTilCancelled` | `OrderType::GTC` |
| `TimeInForce::GoodTilDate` | `OrderType::GTD` |
| `TimeInForce::FillOrKill` | `OrderType::FOK` |
| `TimeInForce::FillAndKill` | N/A (use FAK in post_order) |

**Note:** Polymarket does not support IOC (Immediate-Or-Cancel). Use FAK (Fill-And-Kill) instead, which fills as much as possible and cancels the remainder.

### polyfill-rs Data Types

```rust
// ========== External API Types (Decimal-based) ==========

// Orderbook response from GET /book
pub struct OrderBookSummary {
    pub market: String,       // condition_id
    pub asset_id: String,     // token_id
    pub hash: String,         // orderbook snapshot hash
    pub timestamp: u64,
    pub bids: Vec<OrderSummary>,  // sorted by price descending
    pub asks: Vec<OrderSummary>,  // sorted by price ascending
}

pub struct OrderSummary {
    pub price: Decimal,  // 0.0 to 1.0
    pub size: Decimal,   // number of shares
}

// Order arguments for limit orders
pub struct OrderArgs {
    pub token_id: String,
    pub price: Decimal,
    pub size: Decimal,
    pub side: Side,
}

// Order arguments for market orders
pub struct MarketOrderRequest {
    pub token_id: String,
    pub side: Side,
    pub amount: Decimal,  // USDC amount
    pub slippage_tolerance: Option<Decimal>,
    pub client_id: Option<String>,
}

// Order types
pub enum OrderType {
    GTC,  // Good-Til-Cancelled
    GTD,  // Good-Til-Date
    FOK,  // Fill-Or-Kill
}

pub enum Side {
    BUY = 0,
    SELL = 1,
}

// ========== Internal Fixed-Point Types (High Performance) ==========

// Price in ticks: u32 with SCALE_FACTOR=10,000
// Example: $0.6543 = 6543 ticks
pub type Price = u32;

// Quantity in fixed-point: i64 with SCALE_FACTOR=10,000
// Example: 100.0 tokens = 1,000,000 units
pub type Qty = i64;

pub const SCALE_FACTOR: i64 = 10_000;

// Fast internal book level (no allocation)
pub struct FastBookLevel {
    pub price: Price,  // u32
    pub size: Qty,     // i64
}

// Fast order delta for hot path
pub struct FastOrderDelta {
    pub token_id_hash: u64,    // Hash for fast lookup
    pub timestamp: DateTime<Utc>,
    pub side: Side,
    pub price: Price,
    pub size: Qty,             // 0 = remove level
    pub sequence: u64,
}

// Conversion functions
pub fn decimal_to_price(d: Decimal) -> Result<Price>;  // API -> internal
pub fn price_to_decimal(p: Price) -> Decimal;          // internal -> API
pub fn decimal_to_qty(d: Decimal) -> Result<Qty>;
pub fn qty_to_decimal(q: Qty) -> Decimal;
```

**Important precision note:** polyfill-rs uses 4 decimal places (SCALE_FACTOR=10,000). Prices like 0.00001 will be rounded to 0.0001. This is sufficient for Polymarket where tick sizes are typically 0.01 or 0.001.

### Two-Step Order Flow

Orders in Polymarket follow a two-step process:

```rust
// Step 1: Create and sign order locally (no network call)
let signed_order = client.create_order(&order_args, expiration, extras, options).await?;

// Step 2: Submit signed order to Polymarket (HTTP POST /order)
let response = client.post_order(signed_order, OrderType::GTC).await?;
```

This separation allows:
- Private key never leaves local environment
- Batch creation of signed orders
- Offline signing with later submission

### WebSocket Channels

Polymarket provides two WebSocket channels with different purposes:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         WebSocket Channels                                   │
├─────────────────────────────────┬───────────────────────────────────────────┤
│  Market Channel (Public)        │  User Channel (Private)                   │
├─────────────────────────────────┼───────────────────────────────────────────┤
│  Authentication: None           │  Authentication: API Key required         │
│  Subscribe: by token_id         │  Subscribe: automatic for your orders    │
│                                 │                                           │
│  Events:                        │  Events:                                  │
│  - book (orderbook snapshot)    │  - order (placement/update/cancel)        │
│  - price_change (L2 updates)    │  - trade (YOUR fills with order_id)       │
│  - last_trade_price (anonymous) │                                           │
│  - tick_size_change             │                                           │
│                                 │                                           │
│  Use for:                       │  Use for:                                 │
│  ✅ Orderbook updates           │  ✅ Fill detection (has order_id!)        │
│  ✅ Market data cache           │  ✅ Order status tracking                 │
│  ❌ Fill detection (anonymous!) │  ✅ Position updates                      │
└─────────────────────────────────┴───────────────────────────────────────────┘
```

**Market Channel Messages (Public - for orderbook):**

```rust
// last_trade_price - anonymous, CANNOT identify whose trade
{
    "event_type": "last_trade_price",
    "asset_id": "123...",
    "price": "0.65",
    "size": "100",
    "side": "BUY",
    "timestamp": "1234567890000"
    // ❌ NO order_id - cannot match to your orders!
}

// price_change - orderbook updates
{
    "event_type": "price_change",
    "market": "0x...",
    "price_changes": [{
        "asset_id": "123...",
        "price": "0.65",
        "size": "500",    // New total at this level
        "side": "BUY",
        "best_bid": "0.64",
        "best_ask": "0.66"
    }]
}
```

**User Channel Messages (Private - for fill detection):**

```rust
// Order update - YOUR order status
{
    "id": "order_abc123",        // ✅ YOUR order ID
    "type": "UPDATE",            // PLACEMENT | UPDATE | CANCELLATION
    "status": "LIVE",            // LIVE | MATCHED | CANCELLED
    "size_matched": "50.0",      // ✅ How much filled
    "original_size": "100.0",
    "price": "0.65",
    "side": "BUY",
    "asset_id": "123...",
    "associate_trades": ["trade_xyz"]
}

// Trade message - YOUR fills
{
    "taker_order_id": "order_taker",
    "maker_orders": [{
        "order_id": "order_abc123",   // ✅ YOUR order ID
        "matched_amount": "50.0"
    }],
    "status": "MATCHED",              // MATCHED → MINED → CONFIRMED
    "price": "0.65",
    "size": "50.0"
}
```

**WebSocket Processing Loop:**

```rust
async fn run_websocket_loop(&self) {
    // Subscribe to both channels
    self.ws_stream.subscribe_market_channel(vec![self.config.token_id.clone()]).await?;
    self.ws_stream.subscribe_user_channel().await?;  // Requires auth

    while let Some(message) = self.ws_stream.next().await {
        match message? {
            // Market Channel - orderbook updates only
            StreamMessage::PriceChange { data } => {
                for change in data.price_changes {
                    self.book_manager.apply_delta(OrderDelta {
                        token_id: change.asset_id,
                        price: change.price.parse()?,
                        size: change.size.parse()?,
                        side: change.side.parse()?,
                        ..
                    })?;
                }
            }

            // Market Channel - anonymous trades (NOT for fill detection)
            StreamMessage::LastTradePrice { data } => {
                // Only update price cache, NOT fill detection
                self.book_manager.update_last_price(&data.asset_id, data.price)?;
            }

            // User Channel - YOUR order updates (for fill detection!)
            StreamMessage::UserOrderUpdate { data } => {
                self.handle_user_order_update(&data).await?;
            }

            // User Channel - YOUR trade fills
            StreamMessage::UserTrade { data } => {
                self.handle_user_trade(&data).await?;
            }

            StreamMessage::Heartbeat { .. } => {
                // Connection alive
            }
        }
    }
}
```

## Configuration

### YAML Configuration Example

```yaml
polymarket:
  host: "https://clob.polymarket.com"
  chain_id: 137
  # Private key loaded from environment: POLYMARKET_PRIVATE_KEY
  condition_id: "0x..."  # Market condition ID
  token_id: "..."        # Outcome token ID (e.g., "Yes" outcome)
  default_slippage: 0.005  # 0.5% slippage for market orders
  max_retries: 3
  retry_delay_ms: 1000

  # polyfill-rs specific settings
  orderbook_depth: 50      # Number of price levels to track (memory vs accuracy tradeoff)
  fee_rate_bps: 0          # Fee rate for fill simulation (basis points)
  client_mode: "internet"  # "colocated" for low-latency servers, "internet" for general use

  websocket:
    url: "wss://ws-subscriptions-clob.polymarket.com/ws"
    reconnect:
      max_retries: 5
      base_delay_ms: 1000
      max_delay_ms: 60000
      backoff_multiplier: 2.0
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `POLYMARKET_PRIVATE_KEY` | Ethereum private key for signing |
| `POLYMARKET_API_KEY` | API key (if pre-generated) |
| `POLYMARKET_API_SECRET` | API secret |
| `POLYMARKET_PASSPHRASE` | API passphrase |

## Implementation Notes

### Using polyfill-rs

We use [polyfill-rs](https://github.com/floor-licker/polyfill-rs) as the foundation - a high-performance drop-in replacement for polymarket-rs-client.

**Key Advantages over polymarket-rs-client:**

| Feature | polymarket-rs-client | polyfill-rs |
|---------|---------------------|-------------|
| Orderbook storage | `BTreeMap<Decimal, Decimal>` | `BTreeMap<u32, i64>` |
| Price operations | ~50-100ns + allocation | ~5ns, no allocation |
| Memory per level | ~64+ bytes | ~12 bytes |
| Market impact calc | Not included | Built-in `FillEngine` |
| WebSocket | Manual handling | `WebSocketStream` with auto-reconnect |
| Orderbook manager | Not included | `OrderBookManager` with depth limiting |

**Strengths:**
- API-compatible with polymarket-rs-client (drop-in replacement)
- Fixed-point arithmetic eliminates Decimal overhead in hot paths
- Complete EIP-712 signing implementation
- Built-in orderbook management with stale data cleanup
- FillEngine for market impact simulation
- Auto-reconnecting WebSocket with exponential backoff
- MIT/Apache-2.0 license (compatible)

**Limitations to be aware of:**
1. 4 decimal precision (SCALE_FACTOR=10,000)
   - Prices like 0.00001 rounded to 0.0001
   - Sufficient for Polymarket's typical tick sizes (0.01)
2. `OrderBookManager` uses `RwLock`, not lock-free
   - Adequate for our use case but not true HFT
3. Some README claims are exaggerated (not 434M ops/sec)
   - Realistic: 2-10x faster than Decimal-based operations

**Key polyfill-rs Components:**

| Component | Purpose |
|-----------|---------|
| `ClobClient` | REST API client (compatible API) |
| `OrderBookManager` | Multi-token orderbook management |
| `OrderBookImpl` | Single orderbook with fixed-point ops |
| `FillEngine` | Market order simulation and impact |
| `WebSocketStream` | Real-time data with auto-reconnect |
| `OrderArgs` | Order creation parameters |
| `OrderDelta` | Orderbook update message |
| `FastOrderDelta` | Internal fixed-point delta |

**Key ClobClient Methods:**

| Method | Purpose |
|--------|---------|
| `new()` / `new_colocated()` / `new_internet()` | Create client with env-specific settings |
| `with_l1_headers()` / `with_l2_headers()` | Set authentication |
| `create_or_derive_api_key()` | Get or create API key |
| `get_order_book()` | Fetch orderbook snapshot |
| `get_tick_size()` | Get minimum tick size |
| `create_order()` | Create and sign order locally |
| `create_and_post_order()` | Create, sign, and submit in one call |
| `post_order()` | Submit signed order |
| `cancel()` | Cancel order by ID |
| `get_orders()` | Get open orders |

**Dependencies to add in Cargo.toml:**

```toml
[dependencies]
polyfill-rs = "0.1"
rust_decimal = "1.32"
rust_decimal_macros = "1.32"
```

## Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `polymarket_orders_placed` | Counter | `side`, `status` | Orders placed |
| `polymarket_orders_filled` | Counter | `side` | Orders fully filled |
| `polymarket_orders_cancelled` | Counter | `reason` | Orders cancelled |
| `polymarket_order_latency_ms` | Histogram | `operation` | API call latency |
| `polymarket_fill_latency_ms` | Histogram | | Time from placement to fill |
| `polymarket_websocket_reconnects` | Counter | | WebSocket reconnection count |
| `polymarket_api_errors` | Counter | `error_type` | API error count |

## Testing Strategy

### Unit Tests
- Order creation and signing
- Order mapping (trade server types <-> Polymarket types)
- Fill event parsing
- Orderbook state management

### Integration Tests
- API connectivity (testnet if available)
- Order lifecycle (place -> fill -> cancel)
- WebSocket connection and message handling
- Error handling and retries

### Mock Tests
- Mock ClobClient for deterministic testing
- Simulate various fill scenarios
- Test reconnection logic

## Security

- **Private Key Management**: Never log or expose private keys; load from environment
- **API Credentials**: Store securely; rotate periodically
- **Input Validation**: Validate all order parameters before submission
- **Rate Limiting**: Respect Polymarket rate limits; implement client-side throttling
- **TLS**: All API calls over HTTPS; verify certificates

## Implementation Tasks

See [tasks.md](./tasks.md) for detailed implementation checklist.

## Polymarket Architecture Overview

Polymarket uses a **hybrid-decentralized** model:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           User (Local)                                   │
├─────────────────────────────────────────────────────────────────────────┤
│  1. Create order data structure                                          │
│  2. Sign with EIP-712 (private key stays local)                          │
│  3. Send signed order to Operator                                        │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │ post_order()
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      Polymarket Operator (Off-chain)                     │
├─────────────────────────────────────────────────────────────────────────┤
│  - Receives signed orders                                                │
│  - Maintains orderbook                                                   │
│  - Matches orders (1 maker + N takers)                                   │
│  - Cannot modify orders or access funds                                  │
└───────────────────────────────────┬─────────────────────────────────────┘
                                    │ Submit matched trades
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    Polygon Exchange Contract (On-chain)                  │
├─────────────────────────────────────────────────────────────────────────┤
│  - Validates EIP-712 signatures                                          │
│  - Checks balances and allowances                                        │
│  - Executes atomic swap: USDC ↔ Outcome Token (ERC1155)                  │
│  - Assets transfer directly between user wallets                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Properties:**
- **Non-custodial**: Funds remain in user's wallet, only authorized via signature
- **Operator-limited**: Can only match and submit, cannot modify prices or steal funds
- **On-chain settlement**: Final settlement via audited smart contract
- **Gas abstraction**: Operator pays gas, included in trading fees

## References

- [OrderExecutor Trait](../../../src/execution/executor.rs)
- [polyfill-rs](https://github.com/floor-licker/polyfill-rs) - Primary client library
- [polyfill-rs docs.rs](https://docs.rs/polyfill-rs) - API documentation
- [polymarket-rs-client](https://github.com/TechieBoy/polymarket-rs-client) - Original reference implementation
- [Polymarket CLOB API Documentation](https://docs.polymarket.com)
- [Polymarket WebSocket User Channel](https://docs.polymarket.com/developers/CLOB/websocket/user-channel) - Private channel for fill detection
- [Polymarket WebSocket Market Channel](https://docs.polymarket.com/developers/CLOB/websocket/market-channel) - Public channel for orderbook
- [Polymarket WebSocket Overview](https://docs.polymarket.com/developers/CLOB/websocket/wss-overview)
- [Polymarket Exchange Audit (ChainSecurity)](https://old.chainsecurity.com/wp-content/uploads/2023/01/Polymarket-Exchange-Smart-Contract-audit-by-ChainSecurity-1.pdf)
- [Architecture Documentation](../../architecture.md)
- [Orderbook Trading Support](../002-orderbook-support/design.md)
- [Backtest Support](../004-backtest-support/design.md)
