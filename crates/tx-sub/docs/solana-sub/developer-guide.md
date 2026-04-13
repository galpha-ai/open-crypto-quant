# Solana Subscriber Developer Guide

This guide provides detailed information for developing and debugging the Solana subscriber service.

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
- **gRPC endpoint access** (Yellowstone gRPC service)

### Quick Setup

```bash
# Clone the repository
cd /path/to/tx-sub

# Start Redis
docker compose --profile all up -d redis

# Build the Solana subscriber
cargo build -p solana-sub

# Run with test config
cargo run -p solana-sub -- --config-file configs/solana-sub/config.e2e_test.yaml
```

### Project Structure

```
crates/solana-sub/
├── src/
│   ├── main.rs                    # Entry point
│   ├── lib.rs                     # Library exports
│   ├── app.rs                     # Application orchestration
│   ├── config.rs                  # Configuration loading and validation
│   ├── grpc/                      # gRPC integration
│   │   ├── grpc.rs                # GrpcDataSubscriptionManager
│   │   └── types.rs               # Transaction types
│   ├── parser/                    # Transaction parsing
│   │   ├── parser.rs              # Core parser
│   │   ├── market.rs              # Market trade types
│   │   ├── pumpfun.rs             # PumpFun parser
│   │   └── raydium.rs             # Raydium AMM v4 parser
│   ├── redis_trade_consumer.rs   # Redis publishing
│   ├── redis_tx_consumer.rs      # Transaction persistence
│   ├── redis_tx_retriever.rs     # Transaction retrieval
│   └── metrics.rs                 # Prometheus metrics
├── Cargo.toml                     # Dependencies
└── tests/                         # Integration tests
```

## Architecture Overview

### Component Interaction

```
Solana Blockchain (via Yellowstone gRPC)
       |
       | (gRPC stream)
       v
GrpcDataSubscriptionManager
       |
       | (TransactionData broadcast)
       v
TransactionParser
       |
       | (MarketTrade broadcast)
       v
RedisTradeConsumer --> Redis (List/Stream/Pubsub)
       |
       v
  TokenEvent JSON
```

### Key Components

#### 1. App (app.rs)
- **Responsibility**: Orchestrates all components and manages lifecycle
- **Coordination**: Creates broadcast channels, initializes components, runs concurrent tasks

#### 2. gRPC Client (grpc/grpc.rs)
- **Responsibility**: Connect to Yellowstone gRPC service
- **Features**:
  - Subscribe to transactions for specified program IDs
  - Filter out vote and failed transactions
  - Automatic ping/pong keep-alive
  - Block time latency tracking

#### 3. Transaction Parser (parser/parser.rs)
- **Responsibility**: Parse Solana transactions to trade events
- **Protocols Supported**:
  - **PumpFun**: Token creation, buy/sell trades
  - **Raydium AMM v4**: Swap transactions

#### 4. Redis Consumer (redis_trade_consumer.rs)
- **Responsibility**: Publish trade events to Redis
- **Features**:
  - Multi-target publishing (List, Stream, Pubsub)
  - Pipelined operations for efficiency
  - Trade latency tracking

#### 5. Transaction Persistence (redis_tx_consumer.rs)
- **Responsibility**: Store raw transactions in Redis (optional)
- **Features**:
  - Protobuf encoding
  - Configurable TTL (default 10 minutes)
  - Key format: `/transactions/{environment}/{signature}`

## Development Workflow

### Building

```bash
# Build Solana subscriber only
cargo build -p solana-sub

# Build entire workspace
cargo build --workspace

# Build with optimizations (release mode)
cargo build -p solana-sub --release
```

### Running Locally

#### Option 1: Direct Cargo Run

```bash
# Start Redis first
docker compose --profile all up -d redis

# Run with e2e test config
cargo run -p solana-sub -- --config-file configs/solana-sub/config.e2e_test.yaml

# Run with custom config
cargo run -p solana-sub -- --config-file path/to/config.yaml
```

#### Option 2: Docker Compose

```bash
# Build and start Solana subscriber
just run-local-solana

# View logs
just logs-solana-local

# Restart after code changes
just restart-solana-local

# Stop services
just stop-local
```

### Code Style

```bash
# Format code
cargo fmt --all

# Run clippy lints
cargo clippy -p solana-sub

# Fix common issues
cargo fix -p solana-sub
```

## Testing

### Unit Tests

```bash
# Run all Solana subscriber tests
cargo test -p solana-sub

# Run specific test
cargo test -p solana-sub -- test_parse_pumpfun_trade

# Run with output
cargo test -p solana-sub -- --nocapture
```

### Integration Tests

Currently, the Solana subscriber does not have dedicated integration tests. To add integration tests:

1. Create `crates/solana-sub/tests/` directory
2. Add integration test files (e.g., `grpc_integration.rs`, `parser_integration.rs`)
3. Run with: `cargo test -p solana-sub --test <test-name>`

### End-to-End Testing

For longer-running tests with real transaction data:

```bash
# Run subscriber for 60 seconds
timeout 60 cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml

# Monitor Redis in another terminal
watch -n 1 'docker exec -it tx-sub-redis redis-cli LLEN "sniper_tx_events"'
```

## Configuration

### Configuration File Structure

**Location:** `configs/solana-sub/config.yaml`

```yaml
grpc:
  endpoint: "https://your-grpc-endpoint:10000/"
  x_token: "your-token-here"
  program_ids:
    - "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"  # PumpFun
    - "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8"  # Raydium AMM v4

pumpfun:
  program_id: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
  rpc_url: "https://your-rpc-url"  # Optional

redis:
  url: "redis://localhost:6379"

  # Redis LIST queues
  queues:
    - name: "sniper_tx_events"
      max_length: 10000
    - name: "ch_tx_events_v2"
      max_length: 10000

  # Redis STREAM (optional)
  streams:
    - name: "solana:trades:stream"
      max_length: 10000
      consumer_group: "solana-consumers"

  # Redis PUBSUB (optional)
  pubsub:
    - channel: "solana:trades:pubsub"

metrics:
  port: 9094

# Optional transaction persistence
redis_tx_store:
  enabled: false
  key_prefix: "/transactions"
  ttl_secs: 600
  environment: "dev"
```

### Configuration Validation

The config loader validates:
- gRPC endpoint URL format
- At least one program ID specified
- Redis URL format
- Metrics port >= 1024 (if specified)
- PumpFun program ID matches if specified

### Environment-Specific Configs

Available configurations in `configs/solana-sub/`:

```bash
configs/solana-sub/
├── config.e2e_test.yaml             # End-to-end testing
├── config.fountainhead.staging.yaml # Fountainhead staging
├── config.fountainhead.prod.yaml    # Fountainhead production
├── config.quicknode.prod.yaml       # QuickNode production
├── config.localrpc.prod.yaml        # Local RPC production
└── config.local.yaml                # Local development (gitignored)
```

Run with specific config:

```bash
cargo run -p solana-sub -- --config-file configs/solana-sub/config.e2e_test.yaml
```

### Creating Local Configuration

Create `configs/solana-sub/config.local.yaml` (gitignored):

```yaml
grpc:
  endpoint: "https://your-grpc-endpoint:10000/"
  x_token: "your-token-here"
  program_ids:
    - "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"

pumpfun:
  program_id: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"

redis:
  url: "redis://localhost:6379"
  queues:
    - name: "test-queue"
      max_length: 1000

metrics:
  port: 9094
```

## Debugging

### Logging Levels

Set log level via `RUST_LOG` environment variable:

```bash
# Debug everything
RUST_LOG=debug cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml

# Debug Solana subscriber only
RUST_LOG=solana_sub=debug cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml

# Trace-level logging (very verbose)
RUST_LOG=trace cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml

# Multiple targets
RUST_LOG=solana_sub=debug,common=info cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml
```

### Structured Logging

Logs are JSON-formatted for easy parsing:

```bash
# Pretty-print logs with jq
cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml 2>&1 | jq -r '.fields.message'

# Filter by log level
cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml 2>&1 | jq 'select(.level == "ERROR")'

# Filter by target module
cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml 2>&1 | jq 'select(.target | startswith("solana_sub::parser"))'
```

### Metrics Debugging

Access Prometheus metrics:

```bash
# View all metrics
curl http://localhost:9094/metrics

# Filter specific metrics
curl http://localhost:9094/metrics | grep solana

# Check transaction processing count
curl http://localhost:9094/metrics | grep transactions_processed

# Check parsed trades by DEX
curl http://localhost:9094/metrics | grep parsed_trades_total
```

**Key Metrics:**
- `solana_transactions_processed`: Total gRPC transactions received
- `solana_parsed_trades_total`: Trades parsed by DEX (pumpfun, raydium_amm_v4)
- `solana_send_failures`: Channel send failures by type
- `solana_block_time_latency_seconds`: Block processing latency histogram

### Redis Debugging

```bash
# Check queue length
docker exec -it tx-sub-redis redis-cli LLEN "sniper_tx_events"

# View recent events
docker exec -it tx-sub-redis redis-cli LRANGE "sniper_tx_events" 0 5

# Pretty-print events
docker exec -it tx-sub-redis redis-cli --raw LRANGE "sniper_tx_events" 0 0 | jq

# Monitor real-time events
docker exec -it tx-sub-redis redis-cli MONITOR

# Check stream length (if using streams)
docker exec -it tx-sub-redis redis-cli XLEN "solana:trades:stream"

# Read stream events
docker exec -it tx-sub-redis redis-cli XREAD COUNT 5 STREAMS "solana:trades:stream" 0
```

### Common Issues

#### gRPC Connection Drops

**Symptoms:** Subscriber exits, no transactions received

**Causes:**
- Invalid gRPC endpoint or token
- Network issues
- gRPC service downtime

**Solutions:**
- Verify gRPC credentials in config
- Check internet connection
- Verify gRPC service status
- Check logs for error messages

#### No Trade Events Parsed

**Symptoms:** Transactions received but no trades parsed

**Causes:**
- Transactions don't match monitored program IDs
- Parsing errors
- No trades in monitored markets

**Debugging:**
```bash
# Check transactions processed count
curl http://localhost:9094/metrics | grep transactions_processed

# Check parsing errors (if metric exists)
curl http://localhost:9094/metrics | grep parsing_errors

# Enable debug logs for parser
RUST_LOG=solana_sub::parser=debug cargo run -p solana-sub -- --config-file configs/solana-sub/config.local.yaml 2>&1 | grep -i "parse\|trade"
```

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

#### High Block Processing Latency

**Symptoms:** `block_time_latency_seconds` > 5 seconds

**Causes:**
- Network latency
- Slow parsing
- Redis backpressure

**Solutions:**
- Check network connection to gRPC service
- Monitor Redis queue lengths
- Consider horizontal scaling if processing capacity is insufficient

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
   cargo test -p solana-sub

   # Check formatting
   cargo fmt --all -- --check

   # Run clippy
   cargo clippy -p solana-sub -- -D warnings
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
- **Use descriptive test names**: `test_parse_pumpfun_trade_with_invalid_data`
- **Keep tests fast**: Mock external dependencies
- **Test concurrency** if applicable

### Documentation Guidelines

- **Keep README up-to-date**: Reflect current state of the code
- **Document public APIs**: Use Rust doc comments (`///`)
- **Explain architecture decisions**: Add comments for non-obvious choices
- **Provide examples**: Show how to use new features
- **Update this guide**: Keep developer guide current

## Additional Resources

- **Main architecture docs**: `docs/architecture.md`
- **Workspace refactoring spec**: `docs/specs/002-multi-chain-workspace-refactoring/design.md`
- **Common crate**: `crates/common/` (shared infrastructure)
- **Workspace README**: [README.md](../../README.md)
