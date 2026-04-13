# Polymarket Subscriber Developer Guide

This guide provides detailed information for developing and debugging the Polymarket subscriber service.

For general workspace information, see the [README](../../README.md) in the project root.

## Table of Contents

1. [Getting Started](#getting-started)
2. [Architecture Overview](#architecture-overview)
3. [Development Workflow](#development-workflow)
4. [Testing](#testing)
5. [Configuration](#configuration)
6. [Debugging](#debugging)
7. [Contributing](#contributing)

## Getting Started

### Prerequisites

- **Rust toolchain** (1.70+) - see [workspace prerequisites](../../README.md#prerequisites)
- **Docker** and Docker Compose for Redis
- **Internet connection** for WebSocket and API access

### Quick Setup

```bash
# Navigate to workspace root
cd /path/to/tx-sub

# Start Redis (see workspace guide for details)
docker compose --profile all up -d redis

# Build the Polymarket subscriber
cargo build -p polymarket-sub

# Run with default config
cargo run -p polymarket-sub -- --config-file configs/polymarket-sub/config.yaml
```

For general workspace setup, see the [README](../../README.md#building-and-testing).

### Project Structure

```
crates/polymarket-sub/
├── src/
│   ├── main.rs                    # Entry point
│   ├── lib.rs                     # Library exports
│   ├── app.rs                     # Application orchestration
│   ├── config.rs                  # Configuration loading and validation
│   ├── ws_client.rs               # WebSocket client for CLOB API
│   ├── parser.rs                  # Event parser (JSON → PolymarketTradeEvent)
│   ├── redis_publisher.rs         # Redis publishing with buffering
│   ├── metrics.rs                 # Prometheus metrics
│   ├── api_client.rs              # HTTP client for Gamma API
│   ├── market_discovery.rs        # Market discovery service
│   ├── subscription_manager.rs    # Subscription management
│   ├── market_metadata_cache.rs   # In-memory metadata cache
│   ├── market_metadata_loader.rs  # Metadata loader for manual mode
│   ├── health_monitor.rs          # Health monitoring
│   ├── types.rs                   # Data structures
│   └── bin/
│       └── integration_test.rs    # Integration test binary
├── Cargo.toml                     # Dependencies
└── INTEGRATION_TEST.md            # Legacy test docs (see Testing section)
```

## Architecture Overview

### Component Interaction

```
Polymarket Gamma API (HTTPS)
       |
       | (Market data)
       v
MarketDiscoveryService
       |
       | (DiscoveredMarket broadcast)
       v
SubscriptionManager
       |
       | (SubscriptionUpdate broadcast)
       v
App (manages WebSocket lifecycle)
       |
       v
PolymarketWebSocketClient (wss://)
       |
       | (JSON messages broadcast)
       v
EventParser
       |
       | (PolymarketTradeEvent broadcast)
       v
RedisTradePublisher --> Redis (List/Stream/Pubsub)
       |
       v
  TokenEvent JSON
```

### Key Components

#### 1. App (app.rs)
- **Responsibility**: Orchestrates all components and manages lifecycle
- **Modes**:
  - **Manual Mode**: Uses static asset list from config
  - **Discovery Mode**: Dynamically discovers and subscribes to markets
- **Coordination**: Manages WebSocket reconnection on subscription updates

#### 2. WebSocket Client (ws_client.rs)
- **Responsibility**: Connect to Polymarket CLOB WebSocket API
- **Features**:
  - TLS support (wss://)
  - Automatic ping/pong keep-alive (10s interval)
  - Subscription message formatting
  - Message broadcasting to parser

#### 3. Event Parser (parser.rs)
- **Responsibility**: Parse JSON messages to `PolymarketTradeEvent`
- **Validation**:
  - Price range [0.0, 1.0]
  - Required fields (asset_id, market, price, size, side, timestamp)
  - Trade side enum (BUY/SELL)
- **Enrichment**: Looks up market metadata from cache if available

#### 4. Redis Publisher (redis_publisher.rs)
- **Responsibility**: Publish trade events to Redis
- **Features**:
  - Multi-target publishing (List, Stream, Pubsub)
  - Event buffering on Redis failures (max 1000 events)
  - Rate-limited buffer flushing (100 events/sec)
  - Trade latency tracking

#### 5. Market Discovery (market_discovery.rs)
- **Responsibility**: Discover active crypto binary prediction markets
- **Process**:
  1. Fetch events from Gamma API (paginated, batch size 100)
  2. Filter by ticker patterns (e.g., "btc-updown-15m-")
  3. Sort by end date ascending (prioritize resolving soonest)
  4. Broadcast to subscription manager
- **Scheduling**: Runs on startup and at configured intervals

#### 6. Subscription Manager (subscription_manager.rs)
- **Responsibility**: Manage WebSocket subscriptions
- **Logic**:
  - Calculate subscription diffs (additions/removals)
  - Apply subscription limit (max_subscriptions)
  - Trigger WebSocket reconnection only if subscriptions changed
  - Track state and metrics

#### 7. Market Metadata Cache (market_metadata_cache.rs)
- **Responsibility**: In-memory cache for market metadata
- **Thread-Safety**: Arc<DashMap> for concurrent access
- **Lookups**:
  - By asset_id
  - By condition_id (market field)

#### 8. Health Monitor (health_monitor.rs)
- **Responsibility**: Detect silent rate limiting
- **Mechanism**:
  - Track event rate over rolling window
  - Exit process if rate drops below threshold
  - Container orchestrator restarts process

## Development Workflow

For general workspace commands (build, test, format, check), see the [README](../../README.md#building-and-testing).

### Building

```bash
# Build Polymarket subscriber only
cargo build -p polymarket-sub

# Or use just command
just build-polymarket

# Build with optimizations (release mode)
cargo build -p polymarket-sub --release
```

### Running Locally

For general Docker Compose commands, see the [README](../../README.md#quick-start).

#### Option 1: Direct Cargo Run

```bash
# Start Redis first (see workspace guide)
docker compose --profile all up -d redis

# Run with default config
cargo run -p polymarket-sub

# Run with custom config
cargo run -p polymarket-sub -- --config-file path/to/config.yaml
```

#### Option 2: Docker Compose

```bash
# Build and start Polymarket subscriber
just run-local-polymarket

# View logs
just logs-polymarket-local

# Restart after code changes
just restart-polymarket-local

# Stop services (see workspace guide for more options)
just stop-local
```

## Testing

For general testing commands, see the [README](../../README.md#building-and-testing).

### Unit Tests

```bash
# Run all Polymarket subscriber tests
cargo test -p polymarket-sub

# Or use just command
just test-polymarket

# Run specific test
cargo test -p polymarket-sub -- test_parse_trade_event

# Run with output
cargo test -p polymarket-sub -- --nocapture
```

### Integration Test

The Polymarket subscriber includes an automated integration test that subscribes to active markets and runs for 10 seconds.

#### Quick Start

```bash
# Ensure Redis is running
docker compose --profile all up -d redis

# Run integration test
cargo run -p polymarket-sub --bin polymarket-integration-test

# Or use justfile
just test-polymarket-integration
```

#### What It Tests

The integration test validates:
1. ✅ Configuration loading and validation
2. ✅ Market discovery service initialization
3. ✅ API requests to Polymarket Gamma API
4. ✅ Fetching active crypto binary prediction markets
5. ✅ WebSocket client setup (if discovery completes in time)
6. ✅ Graceful shutdown after 10-second timeout
7. ✅ No panics or fatal errors

#### Configuration

**Location:** `configs/polymarket-sub/config.integration_test.yaml`

**Settings:**
- **Market discovery enabled**: Automatically finds active BTC/ETH/SOL updown markets
- **Max subscriptions**: 100 (reduced from production)
- **Health monitoring**: Disabled (not needed for 10s test)
- **Redis queue**: `polymarket:trades:integration_test`
- **Metrics port**: 9093

#### Expected Output

```json
{"timestamp":"2025-11-19T17:04:51.708876Z","level":"INFO","fields":{"message":"Starting Polymarket integration test"}}
{"timestamp":"2025-11-19T17:04:51.708935Z","level":"INFO","fields":{"message":"Test will run for 10 seconds and then exit"}}
...
{"timestamp":"2025-11-19T17:05:16.560975Z","level":"INFO","fields":{"message":"Integration test completed successfully (10 second timeout reached)"}}
```

#### Verifying Trade Events

Monitor Redis during the test:

```bash
# Check Redis queue for events
docker exec -it tx-sub-redis redis-cli LRANGE "polymarket:trades:integration_test" 0 -1

# Or subscribe to pubsub (if configured)
docker exec -it tx-sub-redis redis-cli SUBSCRIBE "polymarket:trades:integration_test"

# Check metrics
curl http://localhost:9093/metrics | grep polymarket
```

#### Manual Testing with Specific Markets

To test with known-active markets instead of discovery mode:

1. Create a manual config:

```yaml
polymarket:
  wss_endpoint: "wss://ws-subscriptions-clob.polymarket.com/ws/market"

  # Manually specify asset IDs (256-bit integers as decimal strings)
  assets:
    - "109681959945973826496234384791167033612800000000000000000000000376"
    - "52181619848812915160551060468842099699261274828279546744438464278138132224"

  market_discovery:
    enabled: false  # Disable discovery for manual mode

  health_monitoring:
    enabled: false
    check_interval_secs: 60
    min_events_per_minute: 1
    event_tracking_window_secs: 120

redis:
  url: "redis://127.0.0.1:6379"
  queues:
    - name: "polymarket:trades:integration_test"
      max_length: 1000

metrics:
  port: 9093
```

2. Run with custom config:

```bash
cargo run -p polymarket-sub --bin polymarket-integration-test -- path/to/manual-config.yaml
```

#### Troubleshooting

**Redis Connection Error**
```
Error: Connection refused (os error 111)
```
**Solution**: Start Redis container
```bash
docker compose --profile all up -d redis
```

**No Trade Events Received**

This is expected during low activity periods. The test succeeds if:
- Configuration loads successfully
- Market discovery completes
- WebSocket connection is established
- Test exits after 10 seconds

**Market Discovery Taking Too Long**

The test may timeout before market discovery completes if there are many active markets (1000+). This is expected for a 10-second test. In production, discovery completes fully.

### End-to-End Testing

For longer-running tests with real trade data:

```bash
# Run subscriber for 60 seconds
timeout 60 cargo run -p polymarket-sub

# Monitor Redis in another terminal
watch -n 1 'docker exec -it tx-sub-redis redis-cli LLEN "polymarket:trades"'
```

## Configuration

### Configuration File Structure

**Location:** `configs/polymarket-sub/config.yaml`

```yaml
polymarket:
  wss_endpoint: "wss://ws-subscriptions-clob.polymarket.com/ws/market"

  # Manual mode assets (used when market_discovery.enabled = false)
  assets:
    - "asset_id_1"
    - "asset_id_2"

  # Market discovery (optional)
  market_discovery:
    enabled: true
    api_base_url: "https://gamma-api.polymarket.com"
    tag_id: 21                           # Crypto tag
    ticker_patterns:
      - "btc-updown-15m-"
      - "eth-updown-15m-"
      - "sol-updown-15m-"
    discovery_interval_secs: 300         # Fetch every 5 minutes
    max_subscriptions: 1000              # Safety limit
    api_timeout_secs: 30
    api_retry_attempts: 3
    api_retry_backoff_ms: 1000

  # Health monitoring (optional)
  health_monitoring:
    enabled: true
    check_interval_secs: 60
    min_events_per_minute: 1
    event_tracking_window_secs: 120

redis:
  url: "redis://redis:6379"

  # Redis LIST queues
  queues:
    - name: "polymarket:trades"
      max_length: 10000

  # Redis STREAM (optional)
  streams:
    - name: "polymarket:trades:stream"
      max_length: 10000
      consumer_group: "polymarket-consumers"

  # Redis PUBSUB (optional)
  pubsub:
    - channel: "polymarket:trades:pubsub"

metrics:
  port: 9092
```

### Configuration Validation

The config loader validates:
- WSS endpoint starts with `wss://`
- API base URL starts with `https://` (when discovery enabled)
- At least one asset ID (manual mode) or ticker pattern (discovery mode)
- Subscription limits: 1-10000
- Discovery interval >= 60 seconds
- Health check interval >= 10 seconds
- Metrics port >= 1024

### Environment-Specific Configs

Create multiple configs for different environments:

```bash
configs/polymarket-sub/
├── config.yaml                  # Production
├── config.dev.yaml              # Development
├── config.staging.yaml          # Staging
└── config.integration_test.yaml # Integration tests
```

Run with specific config:

```bash
cargo run -p polymarket-sub -- --config-file configs/polymarket-sub/config.dev.yaml
```

## Debugging

For general debugging information and logging setup, see the [README](../../README.md#monitoring).

### Logging Levels

Set log level via `RUST_LOG` environment variable:

```bash
# Debug everything
RUST_LOG=debug cargo run -p polymarket-sub

# Debug Polymarket subscriber only
RUST_LOG=polymarket_sub=debug cargo run -p polymarket-sub

# Trace-level logging (very verbose)
RUST_LOG=trace cargo run -p polymarket-sub

# Multiple targets
RUST_LOG=polymarket_sub=debug,common=info cargo run -p polymarket-sub
```

### Structured Logging

Logs are JSON-formatted for easy parsing:

```bash
# Pretty-print logs with jq
cargo run -p polymarket-sub 2>&1 | jq -r '.fields.message'

# Filter by log level
cargo run -p polymarket-sub 2>&1 | jq 'select(.level == "ERROR")'

# Filter by target module
cargo run -p polymarket-sub 2>&1 | jq 'select(.target | startswith("polymarket_sub::parser"))'
```

### Metrics Debugging

Access Prometheus metrics:

```bash
# View all metrics
curl http://localhost:9092/metrics

# Filter specific metrics
curl http://localhost:9092/metrics | grep polymarket

# Check WebSocket connection status
curl http://localhost:9092/metrics | grep ws_connection_status

# Check trade counts
curl http://localhost:9092/metrics | grep trades_published
```

**Key Metrics:**
- `polymarket_ws_events_received`: Events by type
- `polymarket_trades_published`: Trades by market and side
- `polymarket_parsing_errors`: Parsing errors by type
- `polymarket_redis_publish_failures`: Redis failures by target
- `polymarket_ws_connection_status`: Connection health (1=connected, 0=disconnected)
- `polymarket_trade_latency_seconds`: Latency histogram
- `polymarket_markets_discovered`: Active markets found
- `polymarket_assets_subscribed`: Current subscriptions
- `polymarket_events_per_minute`: Rolling event rate

### Redis Debugging

```bash
# Check queue length
docker exec -it tx-sub-redis redis-cli LLEN "polymarket:trades"

# View recent events
docker exec -it tx-sub-redis redis-cli LRANGE "polymarket:trades" 0 5

# Pretty-print events
docker exec -it tx-sub-redis redis-cli --raw LRANGE "polymarket:trades" 0 0 | jq

# Monitor real-time events (pubsub)
docker exec -it tx-sub-redis redis-cli SUBSCRIBE "polymarket:trades:pubsub"

# Check stream length
docker exec -it tx-sub-redis redis-cli XLEN "polymarket:trades:stream"

# Read stream events
docker exec -it tx-sub-redis redis-cli XREAD COUNT 5 STREAMS "polymarket:trades:stream" 0
```

### Common Issues

#### WebSocket Connection Drops

**Symptoms:** `ws_connection_status` = 0, subscriber exits

**Causes:**
- Network issues
- Polymarket API downtime
- Rate limiting (silent or explicit)

**Solutions:**
- Check internet connection
- Verify Polymarket API status
- Enable health monitoring to auto-restart on rate limiting
- Reduce `max_subscriptions` if subscribing to too many markets

#### Market Discovery Hangs

**Symptoms:** Discovery process takes very long or times out

**Causes:**
- Large number of active events (1000+)
- API rate limiting
- Network latency

**Solutions:**
- Increase `discovery_interval_secs` to reduce API load
- Use manual mode with specific asset IDs
- Reduce `ticker_patterns` to fewer markets

#### Redis Connection Refused

**Symptoms:** `Connection refused (os error 111)`

**Causes:**
- Redis not running
- Wrong Redis URL in config

**Solutions:**
```bash
# Check Redis status
docker ps | grep redis

# Start Redis
docker compose --profile all up -d redis

# Verify connection
docker exec -it tx-sub-redis redis-cli ping
```

#### No Trade Events Received

**Symptoms:** Subscriber running but no events in Redis

**Causes:**
- Low market activity
- Subscribed to inactive markets
- WebSocket not connected
- Parser errors

**Debugging:**
```bash
# Check WebSocket connection
curl http://localhost:9092/metrics | grep ws_connection_status

# Check parsing errors
curl http://localhost:9092/metrics | grep parsing_errors

# Enable debug logs
RUST_LOG=polymarket_sub=debug cargo run -p polymarket-sub 2>&1 | grep -i "trade\|event\|parse"
```

## Contributing

### Adding New Features

1. **Create feature branch**
   ```bash
   git checkout -b feature/my-new-feature
   ```

2. **Implement changes**
   - Add code to appropriate module
   - Write unit tests
   - Update documentation

3. **Test thoroughly**
   ```bash
   # Run unit tests
   cargo test -p polymarket-sub

   # Run integration test
   just test-polymarket-integration

   # Check formatting
   cargo fmt --all -- --check

   # Run clippy
   cargo clippy -p polymarket-sub -- -D warnings
   ```

4. **Update documentation**
   - Update `docs/architecture.md` if architecture changes
   - Update this developer guide
   - Add inline code comments

5. **Submit PR**
   - Write clear PR description
   - Reference related issues
   - Ensure CI passes

### Code Organization Guidelines

- **Keep modules focused**: Each module should have a single responsibility
- **Use meaningful names**: Functions and variables should be self-documenting
- **Add comments for complex logic**: Explain *why*, not *what*
- **Write tests**: Aim for high test coverage
- **Handle errors gracefully**: Use `Result<T, E>` and provide context
- **Log appropriately**:
  - ERROR: Critical failures
  - WARN: Issues that don't stop execution
  - INFO: Important state changes
  - DEBUG: Detailed flow information
  - TRACE: Very verbose debugging

### Testing Guidelines

- **Write unit tests** for all public functions
- **Test error cases** as well as success cases
- **Use descriptive test names**: `test_parse_trade_event_with_invalid_price`
- **Keep tests fast**: Mock external dependencies
- **Test concurrency** if applicable

### Documentation Guidelines

- **Keep README up-to-date**: Reflect current state of the code
- **Document public APIs**: Use Rust doc comments (`///`)
- **Explain architecture decisions**: Add comments for non-obvious choices
- **Provide examples**: Show how to use new features
- **Update this guide**: Keep developer guide current

## Additional Resources

- **Workspace README**: [README.md](../../README.md)
- **Main architecture docs**: [docs/architecture.md](../architecture.md)
- **Workspace refactoring spec**: [docs/specs/002-multi-chain-workspace-refactoring/design.md](../specs/002-multi-chain-workspace-refactoring/design.md)
- **Market metadata spec**: [docs/specs/005-polymarket-crypto-prediction-data-sub/polymarket-market-metadata.md](../specs/005-polymarket-crypto-prediction-data-sub/polymarket-market-metadata.md)
- **Polymarket API docs**: https://docs.polymarket.com/
- **Common crate**: `crates/common/` (shared infrastructure)
- **Solana developer guide**: [docs/solana-sub/developer-guide.md](../solana-sub/developer-guide.md)
