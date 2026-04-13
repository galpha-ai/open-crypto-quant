# Polymarket Subscriber Design

## Summary

The Polymarket subscriber (`polymarket-sub`) is a real-time data subscription service that connects to Polymarket's WebSocket API, processes trade events from prediction markets, and publishes structured data to Redis. This service enables downstream analysis of emotional panic signals and market inefficiencies by tracking trade prices, volumes, and directions across configured prediction market assets.

## Goals

- Establish and maintain stable WebSocket connections to Polymarket's CLOB API
- Subscribe to and process `last_trade_price` events for configured prediction market assets
- Transform Polymarket trade data into structured events and publish to Redis queues/streams
- Expose Prometheus metrics for monitoring connection health and trade processing
- Exit cleanly on connection failures, relying on Kubernetes for automatic restart
- Maintain architectural consistency with existing `solana-sub` service in the workspace

## Non-Goals

- **User Channel Support**: Private WebSocket channels requiring authentication (deferred to future phase)
- **Orderbook Reconstruction**: Processing `book` and `price_change` events for full orderbook state (Phase 2 feature)
- **Historical Data Backfill**: Fetching historical trades via REST API
- **Multi-Market Strategies**: Cross-market arbitrage or correlation analysis (handled by downstream consumers)
- **Event Replay**: Reading and replaying events from Redis for backtesting (separate tooling)
- **Dynamic Asset Management**: Hot-reloading asset configuration without service restart

## Behavior

### Connection Lifecycle

1. **Startup**:
   - Load and validate YAML configuration
   - Parse asset IDs and validate format (256-bit integers as strings)
   - Establish Redis connection using `ConnectionManager`
   - Initialize Prometheus metrics registry and start HTTP server
   - Connect to Polymarket WSS endpoint with TLS

2. **Subscription**:
   - On WebSocket open event, send market channel subscription message
   - Include all configured asset IDs in single subscription
   - Log subscription confirmation at INFO level
   - Set `polymarket_ws_connection_status` to 1

3. **Message Processing Loop**:
   - Receive JSON text frames from WebSocket
   - Parse JSON and extract `event_type` field
   - Route to event-type-specific parser
   - For `last_trade_price` events:
     - Validate required fields are present
     - Parse string decimals to f64 (price, size)
     - Parse timestamp to i64 milliseconds
     - Validate side is "BUY" or "SELL"
     - Create `PolymarketTradeEvent` struct
     - Serialize to JSON
     - Publish to configured Redis targets (LIST, STREAM, PUBSUB)
   - Increment `polymarket_ws_events_received_total{event_type}`
   - Calculate latency: `now() - timestamp` and record histogram

4. **Ping Loop** (concurrent task):
   - Every 10 seconds, send PING message to WebSocket
   - If send fails, log error and exit process with code 1

5. **Error Handling**:
   - On parsing error:
     - Log warning with event_type and error details
     - Increment `polymarket_parsing_errors_total{event_type}`
     - Continue processing other events
   - On Redis publish error:
     - Log warning with queue name and error
     - Increment `polymarket_redis_publish_failures_total{queue}`
     - Buffer event in memory (up to 1000 events)
     - Retry on next event publish
   - On WebSocket connection error:
     - Log error with full context
     - Set `polymarket_ws_connection_status` to 0
     - Flush buffered events to Redis (best effort)
     - Exit process with code 1 (Kubernetes will restart)

6. **Graceful Shutdown** (SIGINT/SIGTERM):
   - Log shutdown signal received
   - Close WebSocket connection gracefully
   - Flush buffered Redis events (with 5-second timeout)
   - Log total events processed and uptime
   - Exit with code 0

## Data Model

### PolymarketTradeEvent

**Purpose**: Represents a single trade execution on a Polymarket prediction market

**Location**: `popeyes_trading_types` crate (`/home/zfeng/popeyes/trading-types`)

**Definition**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketTradeEvent {
    /// Token identifier (256-bit integer as decimal string)
    pub asset_id: String,

    /// Market/condition identifier (hex string without 0x prefix)
    pub market: String,

    /// Trade execution price (0-1 probability range)
    pub price: f64,

    /// Trade volume in outcome tokens
    pub size: f64,

    /// Trade direction
    pub side: TradeSide,

    /// Unix timestamp in milliseconds
    pub timestamp: i64,

    /// Fee rate in basis points (e.g., 10 = 0.1%)
    pub fee_rate_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeSide {
    Buy,
    Sell,
}
```

**Design Rationale**:
- Defined in shared `popeyes_trading_types` crate to ensure serialization/deserialization consistency between producer and consumers
- **Not compatible** with existing `TokenEvent`/`TokenTradeEvent` (which are Solana blockchain-specific)
- String format for `asset_id` preserves 256-bit precision without overflow
- `market` field uses hex string for compatibility with Polymarket's condition IDs
- `price` and `size` as f64 for computational efficiency (precision validated as sufficient)

**JSON Serialization Example**:
```json
{
  "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
  "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
  "price": 0.456,
  "size": 219.217767,
  "side": "BUY",
  "timestamp": 1699564800123,
  "fee_rate_bps": 10
}
```

## Configuration

**File Location**: `configs/polymarket-sub/config.yaml`

**Structure**:
```yaml
polymarket:
  # WebSocket endpoint for Polymarket CLOB
  wss_endpoint: "wss://ws-subscriptions-clob.polymarket.com"

  # List of asset IDs to subscribe to (256-bit integers as decimal strings)
  assets:
    - "109681959945973300464568698402968596289258214226684818748321941747028805721376"
    - "71321045679065213050239142329854245220944540058416089105450777857394034319923"

redis:
  # Redis connection URL (supports environment variable substitution)
  url: "${REDIS_URL:-redis://localhost:6379}"

  # Redis LIST queues (RPUSH with LTRIM)
  queues:
    - name: "polymarket:trades"
      max_length: 10000

  # Redis STREAM configurations (XADD with XTRIM)
  streams:
    - name: "polymarket:trades:stream"
      max_length: 10000
      consumer_group: "analysis"

  # Redis PUBSUB channels (PUBLISH)
  pubsub:
    - channel: "polymarket:trades:live"

metrics:
  # Prometheus HTTP server port (different from solana-sub: 9091)
  port: 9092
```

**Configuration Validation**:
- `wss_endpoint` must be valid `wss://` URL
- At least one asset ID must be configured
- Asset IDs must be non-empty strings (format validated at runtime)
- Redis URL must be valid Redis connection string
- Metrics port must be in range 1024-65535
- If any validation fails, exit with code 2 and descriptive error message

**Environment Variable Substitution**:
- Format: `${VAR_NAME}` or `${VAR_NAME:-default_value}`
- Applied at config load time before deserialization
- Missing required variables without defaults cause startup failure

## WebSocket Client Component

### Technology Stack
- **Library**: `tokio-tungstenite` with `native-tls` feature for WSS support
- **Message Format**: JSON text frames
- **Concurrency**: Separate async tasks for receiving, pinging, and processing

### Connection Manager

**Responsibilities**:
- Establish WSS connection with TLS validation
- Send subscription message on connection open
- Receive and parse JSON messages from WebSocket
- Broadcast parsed messages to parser via `tokio::sync::broadcast` channel
- Maintain connection health via periodic PING messages
- Detect connection drops and trigger process exit

**Implementation Pattern** (similar to `GrpcDataSubscriptionManager`):
```rust
pub struct PolymarketWebSocketClient {
    endpoint: String,
    asset_ids: Vec<String>,
    message_tx: broadcast::Sender<PolymarketMessage>,
}

impl PolymarketWebSocketClient {
    pub async fn run(&self) -> Result<()> {
        // 1. Connect to WebSocket
        // 2. Spawn ping task
        // 3. Send subscription message
        // 4. Loop: receive messages, parse, broadcast
        // 5. On error: log and exit process
    }
}
```

**Ping Strategy**:
- Separate async task sends PING every 10 seconds
- If PING send fails, assume connection is dead and exit
- No PONG validation required (connection failure will be detected by send error)

## Event Parser Component

### Parser Architecture

**Input**: Broadcast channel of raw WebSocket JSON messages
**Output**: Broadcast channel of `PolymarketTradeEvent`

**Parsing Flow**:
1. Receive JSON value from WebSocket client broadcast
2. Extract `event_type` field
3. Match event type:
   - `"last_trade_price"` → Parse trade event
   - Other types → Log info message, increment metric, skip
4. Validate required fields (fail fast with descriptive error)
5. Parse string decimals to f64 with overflow/underflow checks
6. Parse side string to `TradeSide` enum
7. Construct `PolymarketTradeEvent`
8. Broadcast to Redis publisher

**Error Recovery**:
- Parsing errors for individual events are logged but do not stop processing
- Channel lag (`RecvError::Lagged`) is logged as warning, processing continues
- Metrics track error counts by event type

**Extensibility**:
- Parser designed with match statement on event_type
- Future event types (`book`, `price_change`) can be added as new match arms
- Each event type can have its own struct and parsing logic

## Redis Publisher Component

### Publishing Strategy

Reuses `common::publisher` infrastructure with three concurrent publishers:

1. **RedisListPublisher**: RPUSH + LTRIM for fixed-size queue
2. **RedisStreamPublisher**: XADD + XTRIM for stream processing
3. **RedisPubsubPublisher**: PUBLISH for real-time subscribers

**Concurrency Model**:
- Single task receives `PolymarketTradeEvent` from parser broadcast
- Serializes to JSON once
- Publishes to all configured Redis targets using pipelined commands
- Tracks success/failure per target in metrics

**Buffering Strategy**:
- In-memory buffer (VecDeque) holds up to 1000 events during Redis unavailability
- On successful reconnect, flush buffer with rate limiting (100 events/second)
- If buffer exceeds 1000 events, drop oldest events and increment metric

## Idempotency & Concurrency

**Idempotency**:
- Polymarket WebSocket provides at-least-once delivery (no idempotency keys)
- Service may publish duplicate events if Kubernetes restarts during publish
- Downstream consumers must handle duplicates using `(asset_id, market, timestamp, side)` as composite key

**Concurrency**:
- Single WebSocket connection per service instance
- Single subscription covers all configured assets
- No concurrent writes to same Redis key (sequential publishing)
- Safe to run multiple instances with different asset configurations (events are independent)

**Race Conditions**:
- No shared mutable state between events (stateless processing)
- Redis STREAM consumer groups handle concurrent consumers downstream
- Metrics are thread-safe (Prometheus counter atomics)

## Observability

### Structured Logging

**Format**: JSON lines (via `tracing-subscriber` with JSON formatter)

**Log Levels**:
- **INFO**: Connection established, subscription sent, shutdown initiated, statistics
- **WARN**: Parsing errors, Redis publish failures, channel lag, buffer overflow
- **ERROR**: WebSocket connection failures, configuration errors, fatal errors

**Context Fields**:
- All logs: `timestamp`, `level`, `target`, `message`
- Trade events: `asset_id`, `market`, `side`, `price`, `size`, `timestamp`
- Errors: `error_type`, `error_message`, `event_type`

**Example Logs**:
```json
{"timestamp":"2025-11-14T10:30:15Z","level":"INFO","message":"WebSocket connected","endpoint":"wss://ws-subscriptions-clob.polymarket.com"}
{"timestamp":"2025-11-14T10:30:16Z","level":"INFO","message":"Subscribed to market channel","asset_count":5}
{"timestamp":"2025-11-14T10:30:20Z","level":"WARN","message":"Parsing error","event_type":"last_trade_price","error":"missing field: timestamp"}
```

### Prometheus Metrics

**Counters**:
- `polymarket_ws_events_received_total{event_type}`: Total events received by type
- `polymarket_trades_published_total{market, side}`: Trades published (labeled by market and direction)
- `polymarket_parsing_errors_total{event_type}`: Parsing failures by event type
- `polymarket_redis_publish_failures_total{queue}`: Redis publish errors by queue name

**Gauges**:
- `polymarket_ws_connection_status`: 1 if connected, 0 if disconnected

**Histograms**:
- `polymarket_trade_latency_seconds`: Time from trade timestamp to publish (buckets: 0.1, 0.5, 1, 2, 5, 10)

**Dashboards**:
- Connection status timeline
- Trade volume by market and side
- Error rates (parsing, Redis publishing)
- Latency percentiles (p50, p95, p99)

## Implementation

For detailed implementation tasks, rollout phases, and tracking, see [task.md](./task.md).

The implementation follows a phased approach:
1. **Phase 1**: Shared type definitions in `popeyes_trading_types` crate
2. **Phase 2**: Core service implementation with WebSocket, parser, and Redis integration
3. **Phase 3**: Testing infrastructure with unit and integration tests
4. **Phase 4**: Deployment and validation in local, staging, and production environments

## Acceptance Criteria

- [ ] Service successfully connects to `wss://ws-subscriptions-clob.polymarket.com` with TLS validation
- [ ] Subscription message sent on connection open with all configured asset IDs
- [ ] `last_trade_price` events parsed correctly with all fields validated
- [ ] Events published to Redis LIST, STREAM, and PUBSUB (all three modes working)
- [ ] Prometheus metrics exposed on port 9092 with all defined metrics present
- [ ] Service exits with code 1 on WebSocket connection failure
- [ ] Service exits with code 0 on graceful shutdown (SIGTERM/SIGINT)
- [ ] Buffered events flushed to Redis before shutdown
- [ ] Parsing errors logged and tracked in metrics without crashing service
- [ ] Redis publish failures logged, buffered, and retried
- [ ] Service runs for 24+ hours in staging without crash (Kubernetes restarts for connection failures expected)
- [ ] Unit test coverage >80% for parser and config modules
- [ ] Integration test with mock WebSocket server passes
- [ ] Downstream consumer successfully deserializes `PolymarketTradeEvent` from Redis

## Implementation Dependencies

### External Repository: popeyes_trading_types

**Repository**: `/home/zfeng/popeyes/trading-types`
**Current Version**: 0.2.2
**Required Version**: 0.2.3+

**Required Changes**:
1. Add `PolymarketTradeEvent` struct to `src/trade_event.rs` or new `src/polymarket_event.rs`
2. Add `TradeSide` enum with serde derives
3. Add unit tests for serialization/deserialization
4. Bump version in `Cargo.toml`
5. Publish to registry

**Implementation Order**:
1. **First**: Implement and publish `popeyes_trading_types` 0.2.3
2. **Then**: Update `tx-sub` workspace to use new version
3. **Finally**: Implement `polymarket-sub` using the published type

## References

- [Polymarket WSS Overview](./wss-overview.md)
- [Polymarket Market Channel Spec](./market-channel.md)
- [Polymarket WSS Quickstart](./wss-quickstart.md)
- [Multi-Chain Workspace Refactoring Design](../002-multi-chain-workspace-refactoring/design.md)
- [Workspace Architecture](../../architecture.md)
- [Trading Types Repository](/home/zfeng/popeyes/trading-types)
- [solana-sub Implementation Reference](../../crates/solana-sub)
