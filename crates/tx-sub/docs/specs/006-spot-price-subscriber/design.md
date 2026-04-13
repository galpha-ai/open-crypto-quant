# Spot Price Subscriber

## Summary

This design introduces a new subscriber service (`spot-price-sub`) that connects to Polymarket's Real-Time Data Socket (RTDS) API to receive real-time spot price updates for major cryptocurrencies (BTC, ETH, SOL, XRP). The service publishes these price updates to Redis queues, enabling downstream consumers (particularly the Polymarket trading bot) to join spot price data with prediction market data to generate trading signals for ML/RL models.

## Goals

- Subscribe to real-time spot price updates for BTC, ETH, SOL, and XRP using Polymarket RTDS API
- Publish spot price updates to Redis queues in a standardized format
- Implement health monitoring to detect data interruptions and trigger automatic reconnection
- Follow the architectural patterns established in `polymarket-sub` for consistency
- Support graceful shutdown and error resilience
- Provide Prometheus metrics for observability
- Enable deployment in containerized environments (Docker, Kubernetes)

## Non-Goals

- Historical price data fetching or backfilling (use dedicated data providers)
- Price aggregation from multiple exchanges (only use Polymarket RTDS Binance source)
- Trading logic or signal generation (handled by downstream consumers)
- Price storage or database persistence (Redis is used only as a message queue)
- Support for additional cryptocurrencies beyond BTC, ETH, SOL, XRP (can be added later)
- KLine/candlestick data generation (may be added as future enhancement)

## Data Structure Design

### New Types in `popeyes_trading_types`

The following type will be added to the `popeyes_trading_types` crate:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Spot price update event from Polymarket RTDS API
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotPriceUpdate {
    /// Cryptocurrency symbol (e.g., "BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT")
    pub symbol: String,

    /// Current price in USDT
    pub price: f64,

    /// Timestamp when the price was recorded (from data source)
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,

    /// Source of the price data (e.g., "binance", "chainlink")
    pub source: String,
}
```

**Design Rationale**:
- **String-based symbol**: Uses plain `String` instead of enum for maximum flexibility
  - Users can add new trading pairs via configuration without code changes
  - Directly matches the API response format (no conversion needed)
  - Supports any symbol the RTDS API provides (DOGEUSDT, ADAUSDT, etc.)
- **Configuration-driven**: Supported symbols are defined in YAML config, not hardcoded
- **Simple validation**: Optional format validation in config loader (alphanumeric check only)

### Extension to `TokenEvent` Enum

Add a new variant to the existing `TokenEvent` enum:

```rust
pub enum TokenEvent {
    Create(TokenCreationEvent),
    Buy(TokenTradeEvent),
    Sell(TokenTradeEvent),
    Swap(TokenTradeEvent),
    OrderbookUpdate(OrderbookUpdateEvent),
    OrderbookSnapshot(OrderbookSnapshotEvent),
    SpotPrice(SpotPriceUpdate),  // NEW
}
```

### Redis Message Format

Spot price updates will be published as JSON to Redis queues/streams/pubsub:

```json
{
  "SpotPrice": {
    "symbol": "BTCUSDT",
    "price": 98750.25,
    "timestamp": 1732617600000,
    "source": "binance"
  }
}
```

## Architecture

The `spot-price-sub` crate will follow the same architectural patterns as `polymarket-sub`:

```
Polymarket RTDS WebSocket API
       |
       | (wss://ws-live-data.polymarket.com)
       v
WebSocketClient
       |
       | (JSON messages broadcast)
       v
EventParser
       |
       | (SpotPriceUpdate broadcast)
       v
RedisPublisher --> Redis Queues/Streams/Pubsub
       |
       v
  TokenEvent::SpotPrice JSON

HealthMonitor (monitors event rate)
```

### Component Structure

```
crates/spot-price-sub/
└── src/
    ├── main.rs              # Entry point
    ├── app.rs               # App orchestration
    ├── config.rs            # Configuration loading
    ├── metrics.rs           # Prometheus metrics
    ├── ws_client.rs         # RTDS WebSocket client
    ├── parser.rs            # Spot price event parser
    ├── redis_publisher.rs   # Redis publishing
    └── health_monitor.rs    # Health monitoring
```

## Behavior

### Configuration

The service uses a YAML configuration file:

```yaml
spot_price:
  # Polymarket RTDS WebSocket endpoint
  wss_endpoint: "wss://ws-live-data.polymarket.com"

  # Symbols to subscribe to (comma-separated in subscription)
  symbols:
    - BTCUSDT
    - ETHUSDT
    - SOLUSDT
    - XRPUSDT

  # Health monitoring configuration
  health_monitoring:
    enabled: true
    check_interval_secs: 30
    min_updates_per_minute: 4  # At least 1 update per symbol per minute
    event_tracking_window_secs: 60

redis:
  url: "redis://localhost:6379"

  # Spot price event targets
  queues:
    - name: "spot_price:updates"
      max_length: 10000

  streams:
    - name: "spot_price:stream"
      max_length: 10000

  pubsub:
    channel: "spot_price:pubsub"

metrics:
  port: 9093
```

### Application Startup Flow

1. **Load configuration** from YAML file
2. **Initialize metrics registry** and start Prometheus server
3. **Create broadcast channels** for inter-component communication:
   - `ws_message_tx/rx`: Raw WebSocket messages
   - `spot_price_tx/rx`: Parsed spot price updates
4. **Initialize components**:
   - `WebSocketClient`: Connects to RTDS and subscribes to `crypto_prices` topic
   - `EventParser`: Parses RTDS messages into `SpotPriceUpdate` events
   - `RedisPublisher`: Publishes to Redis queues/streams/pubsub
   - `HealthMonitor`: Tracks event rate and exits on anomalies
5. **Run all components concurrently** using `tokio::select!`
6. **Set up graceful shutdown** on Ctrl+C signal

### WebSocket Client (`ws_client.rs`)

**Responsibilities**:
- Connect to `wss://ws-live-data.polymarket.com`
- Send subscription message on connection:
  ```json
  {
    "action": "subscribe",
    "subscriptions": [
      {
        "topic": "crypto_prices",
        "type": "update",
        "filters": "{\"symbol\":\"BTCUSDT,ETHUSDT,SOLUSDT,XRPUSDT\"}"
      }
    ]
  }
  ```
- Send periodic ping messages (every 5 seconds) to maintain connection
- Broadcast received JSON messages to parser via channel
- Handle WebSocket close/error events and exit process for orchestrator restart
- Track connection status metrics

**Key Implementation Details**:
- Use `tokio_tungstenite` with `rustls` for TLS support
- Parse JSON messages using `serde_json::Value` before broadcasting
- Gracefully handle pong and close frames
- Log all connection events with structured logging

### Event Parser (`parser.rs`)

**Responsibilities**:
- Receive raw JSON messages from WebSocket broadcast channel
- Parse RTDS message format:
  ```json
  {
    "topic": "crypto_prices",
    "type": "update",
    "timestamp": 1732617600000,
    "payload": {
      "symbol": "BTCUSDT",
      "timestamp": 1732617595000,
      "value": 98750.25
    }
  }
  ```
- Convert to `SpotPriceUpdate` struct:
  - `symbol`: Parse from payload
  - `price`: Extract from `payload.value`
  - `timestamp`: Use `payload.timestamp` (price recorded time)
  - `source`: Set to `"binance"` (RTDS uses Binance for crypto_prices)
- Validate price is positive and finite
- Broadcast parsed events to Redis publisher
- Track parsing errors by error type in metrics

**Error Handling**:
- Log parsing errors with full message context
- Continue processing on parse failures (don't crash)
- Increment `parsing_errors` metric by error type

### Redis Publisher (`redis_publisher.rs`)

**Responsibilities**:
- Receive parsed `SpotPriceUpdate` events from broadcast channel
- Convert to `TokenEvent::SpotPrice` JSON format
- Publish to multiple Redis targets concurrently:
  - **Queues**: Using `LPUSH` with `LTRIM` for max length
  - **Streams**: Using `XADD` with `MAXLEN ~`
  - **Pubsub**: Using `PUBLISH`
- Track publish latency (timestamp to publish time)
- Implement event buffering for Redis unavailability

**Buffering Strategy** (same as `polymarket-sub`):
- Buffer size: 1000 events
- Flush rate limit: 100 events/second
- Periodic flush interval: 1 second
- Drop oldest events on buffer overflow
- Retry buffered events when Redis becomes available

**Metrics Tracked**:
- `spot_prices_published`: Total count by symbol
- `redis_publish_failures`: Failures by queue/stream/channel
- `spot_price_latency`: Histogram of publish latency
- `buffer_size`: Current buffer size gauge

### Health Monitor (`health_monitor.rs`)

**Responsibilities**:
- Track event rate over rolling window (last 60 seconds)
- Exit process if rate drops below threshold (indicates data interruption)
- Designed to trigger container orchestrator restart

**Health Check Process**:
1. **Event recording task**: Subscribe to spot price updates and record in tracker
2. **Health check task**: Periodically check event rate:
   - Grace period: 60 seconds after startup (no enforcement)
   - Calculate rolling event rate over last 60 seconds
   - If rate < `min_updates_per_minute` threshold:
     - Log critical error with current rate
     - Increment `health_check_failures` metric
     - Exit process with code 1

**Metrics Tracked**:
- `updates_per_minute`: Rolling event rate (updates/minute)
- `health_check_failures`: Total health check failures
- `last_update_timestamp`: Unix timestamp of last received update

### Metrics (`metrics.rs`)

Prometheus-compatible metrics exposed on configurable port:

**WebSocket Metrics**:
- `ws_messages_received`: Total messages by message type
- `ws_connection_status`: Connection status gauge (1=connected, 0=disconnected)

**Spot Price Metrics**:
- `spot_prices_published`: Total updates published by symbol
- `parsing_errors`: Parsing errors by error type
- `spot_price_latency`: Histogram of latency from timestamp to publish

**Health Metrics**:
- `updates_per_minute`: Rolling event rate
- `health_check_failures`: Total failures leading to exit
- `last_update_timestamp`: Last update timestamp

**Redis Metrics**:
- `redis_publish_failures`: Failures by target
- `buffer_size`: Current buffer size
- `buffer_overflows`: Total buffer overflows

## Implementation Plan

### Phase 1: Data Types (popeyes_trading_types)

1. Add `SpotPriceUpdate` struct (with `String` symbol field)
2. Add `SpotPrice` variant to `TokenEvent` enum
3. Add unit tests for serialization/deserialization
4. Publish new version of `popeyes_trading_types` crate (or use local path dependency)

### Phase 2: Core Subscriber (spot-price-sub)

1. **Setup crate structure**:
   - Add `spot-price-sub` to workspace members
   - Copy `Cargo.toml` dependencies from `polymarket-sub`
   - Create basic module structure

2. **Implement configuration**:
   - `config.rs`: YAML loading and validation
   - Add config validation (WSS endpoint format, symbol list, etc.)

3. **Implement WebSocket client**:
   - `ws_client.rs`: Connect, subscribe, ping loop
   - Handle RTDS subscription message format
   - Test connection manually with example config

4. **Implement event parser**:
   - `parser.rs`: Parse RTDS messages to `SpotPriceUpdate`
   - Handle both snapshot and update messages
   - Add comprehensive error handling

5. **Implement Redis publisher**:
   - `redis_publisher.rs`: Publish to queues/streams/pubsub
   - Implement buffering strategy
   - Add metrics tracking

6. **Implement health monitor**:
   - `health_monitor.rs`: Track event rate, exit on threshold
   - Use same pattern as `polymarket-sub`

7. **Implement metrics**:
   - `metrics.rs`: Define all Prometheus metrics
   - Start metrics server on configured port

8. **Implement app orchestration**:
   - `app.rs`: Coordinate all components with `tokio::select!`
   - Handle graceful shutdown
   - Set up Ctrl+C handler

9. **Implement main entry point**:
   - `main.rs`: Initialize logging, load config, run app

### Phase 3: Testing & Documentation

1. **Local testing**:
   - Add Docker Compose configuration
   - Test with local Redis instance
   - Verify all 4 symbols receive updates
   - Test health monitor by simulating connection loss

2. **Integration testing**:
   - Create integration test binary (similar to `polymarket-integration-test`)
   - Verify Redis message format
   - Test graceful shutdown

3. **Documentation**:
   - Create `docs/spot-price-sub/architecture.md`
   - Update workspace `CLAUDE.md` with spot-price-sub reference
   - Add example configuration file
   - Update `justfile` with spot-price-sub commands

4. **CI/CD**:
   - Add to GitHub Actions workflow (if exists)
   - Add Docker build configuration

## Testing Strategy

### Unit Tests

- `SpotPriceUpdate` serialization/deserialization
- Configuration validation (symbol format, endpoint validation)
- Parser message handling (valid and invalid inputs)

### Integration Tests

- End-to-end flow: WebSocket → Parser → Redis
- Health monitor triggers on low event rate
- Buffer overflow handling
- Graceful shutdown with pending events

### Manual Testing Checklist

- [ ] Subscribe to all 4 symbols (BTC, ETH, SOL, XRP)
- [ ] Verify updates arrive in Redis queues
- [ ] Verify updates arrive in Redis streams
- [ ] Verify updates arrive in Redis pubsub
- [ ] Verify Prometheus metrics are exposed
- [ ] Test health monitor by disconnecting WebSocket
- [ ] Test graceful shutdown with buffered events
- [ ] Verify Docker Compose deployment
- [ ] Test with Kubernetes deployment (if applicable)

## Deployment

### Docker Compose (Local Development)

Add to `docker-compose.yaml`:

```yaml
services:
  spot-price-sub:
    build:
      context: .
      dockerfile: Dockerfile.spot-price-sub
    depends_on:
      - redis
    volumes:
      - ./configs/spot-price-sub:/config:ro
    environment:
      - RUST_LOG=info
    command: ["--config-file", "/config/config.yaml"]
    profiles:
      - all
      - spot-price
```

### Kubernetes (Production)

Follow existing Kustomize patterns in `galpha-infra`:
- Base deployment manifest
- ConfigMap for configuration
- Service for metrics endpoint
- Resource limits (CPU: 100m-500m, Memory: 128Mi-512Mi)

## Future Enhancements

### Chainlink Source Support

Add support for Chainlink price feed as alternative/supplementary source:

```rust
pub enum PriceSource {
    Binance,
    Chainlink,
}

pub struct SpotPriceUpdate {
    // ... existing fields
    pub source: PriceSource,
}
```

Subscribe to `crypto_prices_chainlink` topic with filters:
```json
{
  "topic": "crypto_prices_chainlink",
  "type": "*",
  "filters": "{\"symbol\":\"btc/usd\"}"
}
```

### KLine Data Support

Generate KLine (candlestick) data from spot price updates:

- Aggregate updates into time windows (1m, 5m, 15m, 1h, etc.)
- Calculate OHLCV (Open, High, Low, Close, Volume)
- Publish `TokenEvent::KLine` events
- Store in separate Redis streams for each interval

### Price Change Detection

Detect significant price movements and publish alerts:

```rust
pub struct PriceChangeAlert {
    pub symbol: String,
    pub old_price: f64,
    pub new_price: f64,
    pub change_percent: f64,
    pub timestamp: DateTime<Utc>,
}
```

## Security Considerations

- **WebSocket TLS**: Always use `wss://` endpoint with proper certificate validation
- **Configuration Secrets**: No sensitive data in this service (WebSocket is public)
- **Rate Limiting**: Respect RTDS API rate limits (health monitor helps detect throttling)
- **Resource Limits**: Set appropriate memory/CPU limits in Kubernetes to prevent resource exhaustion

## Observability

### Logs

Structured JSON logs with:
- Component name (ws_client, parser, redis_publisher, health_monitor)
- Event type (connection, subscription, update, error)
- Symbol for price updates
- Error details for failures

### Metrics

Prometheus metrics on `/metrics` endpoint:
- Connection status and uptime
- Update rate by symbol
- Redis publish success/failure rates
- Latency percentiles (p50, p95, p99)
- Health check status

### Alerts (Recommended)

- `SpotPriceConnectionDown`: WebSocket connection lost
- `SpotPriceUpdateRateLow`: Update rate below threshold for > 5 minutes
- `SpotPricePublishFailures`: Redis publish failures > 100/min
- `SpotPriceHealthCheckFailed`: Health monitor triggered exit

## References

- [Polymarket RTDS Documentation](https://docs.polymarket.com/developers/RTDS/RTDS-crypto-prices)
- [Polymarket RTDS TypeScript Client](https://github.com/Polymarket/real-time-data-client)
- [Issue #17: Add support for spot price](https://github.com/PumpAndDumpling/popeyes-tx-sub/issues/17)
- [docs/polymarket-sub/architecture.md](../polymarket-sub/architecture.md) - Reference architecture
