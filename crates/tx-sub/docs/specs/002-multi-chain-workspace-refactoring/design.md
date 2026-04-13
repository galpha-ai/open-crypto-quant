# Multi-Chain Workspace Architecture

## Overview

This document describes the refactoring of `tx_sub` from a single-binary Solana-specific subscriber to a multi-chain workspace architecture that can support multiple blockchain data sources (Solana, Polymarket, etc.) while sharing common infrastructure.

## Problem Statement

The current `tx_sub` implementation is tightly coupled to Solana:
- ~50+ dependencies, many Solana-specific (solana-sdk, yellowstone-grpc, spl-token, serum_dex)
- Single binary architecture makes it difficult to add new chains
- No clear separation between chain-specific and common logic
- Adding new chains would bloat dependencies and increase build times

### Requirements
1. Support multiple blockchain data sources (Solana, Polymarket, future chains)
2. Share common infrastructure (Redis, metrics, publisher traits)
3. Isolate chain-specific dependencies
4. Enable independent evolution of each subscriber
5. Maintain efficient build and deployment workflows

## Architectural Decision: Rust Workspace

**Decision**: Use Rust workspace with individual crates for each subscriber

### Why Workspace Over Multiple Binaries

| Aspect                | Workspace (Recommended)    | Multiple Binaries               |
|-----------------------|----------------------------|---------------------------------|
| **Dependencies**      | Isolated per crate         | Shared across all binaries      |
| **Build Time**        | Only compile needed crate  | Always compile all dependencies |
| **Binary Size**       | Smaller, chain-specific    | Larger, includes all deps       |
| **Versioning**        | Independent per subscriber | Single version for all          |
| **Team Scaling**      | Clear ownership boundaries | Shared codebase conflicts       |
| **Deploy Efficiency** | Deploy only what changed   | Deploy monolithic binary        |

### Key Benefits

1. **Dependency Isolation**
   - Solana subscriber: Solana SDK stack only
   - Polymarket subscriber: WebSocket and market-specific dependencies only
   - No dependency conflicts or bloat

2. **Independent Evolution**
   - Update Solana SDK without touching Polymarket
   - Version/release each subscriber independently
   - Different configuration patterns per chain

3. **Build Efficiency**
   - Build only the subscriber being deployed
   - Parallel CI/CD for different chains
   - Smaller Docker images

4. **Clear Architecture**
   - Common code in shared library
   - Chain-specific logic isolated
   - Easy to understand and maintain

## Proposed Workspace Structure

```
popeyes-tx-sub/
├── Cargo.toml                    # Workspace root
├── docs/
│   ├── architecture.md           # Current architecture (reference)
│   └── 002-multi-chain-workspace-refactoring/
│       └── design.md             # This document
├── crates/
│   ├── common/                   # Shared infrastructure
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── publisher/        # Redis publisher traits & impls
│   │       ├── metrics.rs        # Prometheus metrics
│   │       ├── types.rs          # TokenEvent, common types
│   │       └── config.rs         # Shared config utilities
│   │
│   ├── solana-sub/               # Solana subscriber
│   │   ├── Cargo.toml
│   │   ├── config.yaml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── app.rs            # Solana-specific app orchestration
│   │       ├── grpc/             # Yellowstone gRPC client
│   │       ├── parser/           # Solana transaction parsing
│   │       │   ├── pumpfun/
│   │       │   ├── ray_ammv4/
│   │       │   └── parser.rs
│   │       └── config.rs         # Solana-specific config
│   │
│   └── polymarket-sub/           # Polymarket subscriber (future)
│       ├── Cargo.toml
│       ├── config.yaml
│       └── src/
│           ├── main.rs
│           ├── app.rs            # Polymarket-specific app
│           ├── websocket/        # WebSocket client
│           ├── parser/           # CLOB event parsing
│           └── config.rs         # Polymarket-specific config
```

## Component Breakdown

### Common Crate (`crates/common`)

**Purpose**: Shared infrastructure used by all subscribers

**Responsibilities**:
- Redis publisher implementations (List, PubSub, Stream)
- Publisher trait definitions
- Prometheus metrics infrastructure
- Common data types (`TokenEvent`, `MarketTrade`)
- Shared configuration utilities
- Common error types

**Dependencies**:
- `redis`: Redis client
- `tokio`: Async runtime
- `prometheus`: Metrics
- `serde`, `serde_json`: Serialization
- `tracing`: Logging
- `anyhow`, `thiserror`: Error handling

**Key Modules**:
```rust
// Publisher traits and implementations
pub mod publisher {
    pub trait Publisher {
        async fn publish(&self, event: TokenEvent) -> Result<()>;
    }

    pub struct RedisListPublisher { /* ... */ }
    pub struct RedisPubsubPublisher { /* ... */ }
    pub struct RedisStreamPublisher { /* ... */ }
}

// Common types
pub mod types {
    pub struct TokenEvent { /* ... */ }
    pub struct TradeMetadata { /* ... */ }
}

// Metrics
pub mod metrics {
    pub fn init_metrics(port: u16) -> Result<()>;
    // Shared metric definitions
}
```

### Solana Subscriber (`crates/solana-sub`)

**Purpose**: Subscribe to Solana blockchain via Yellowstone gRPC, parse transactions, publish to Redis

**Responsibilities**:
- gRPC connection to Yellowstone
- Solana transaction decoding
- Protocol-specific parsing (PumpFun, Raydium AMM v4)
- Address lookup table handling
- Solana-specific configuration

**Dependencies**:
- `common` (workspace dependency)
- `yellowstone-grpc-client`, `yellowstone-grpc-proto`
- `solana-sdk`, `solana-transaction-status-client-types`
- `spl-token`, `serum_dex`
- Other Solana-specific crates

**Key Components**:
- `GrpcDataSubscriptionManager`: Yellowstone gRPC client
- `TransactionParser`: Parse Solana transactions
- Protocol parsers: PumpFun, Raydium AMM v4
- `App`: Orchestrates all Solana components

### Polymarket Subscriber (`crates/polymarket-sub`)

**Purpose**: Subscribe to Polymarket CLOB events (future implementation)

**Responsibilities**:
- Subscribe to Polymarket CLOB events via WebSocket
- Parse and process prediction market data
- Publish to Redis queues (specific data format to be determined)

**Dependencies**:
- `common` (workspace dependency - for Redis publishers, metrics)
- WebSocket client library
- Other dependencies TBD

**Key Components** (to be designed):
- WebSocket client for CLOB events
- Event parser and processor
- App orchestration

**Note**: Data format and types will be determined during implementation. May not directly use `TokenEvent` as this is prediction market data, not token trading data.

## Workspace Configuration

### Root `Cargo.toml`

```toml
[workspace]
members = [
    "crates/common",
    "crates/solana-sub",
    "crates/polymarket-sub",
]
resolver = "2"

[workspace.dependencies]
# Shared dependencies - versions defined once
redis = { version = "0.27.3", features = ["aio", "tokio-comp", "connection-manager"] }
tokio = { version = "1.41.1", features = ["full"] }
prometheus = "0.13.4"
serde = { version = "1.0.215", features = ["derive"] }
serde_json = "1.0.133"
tracing = "0.1.41"
tracing-subscriber = { version = "0.3.19", features = ["env-filter", "json", "time"] }
anyhow = "1.0.93"
thiserror = "2.0.3"

# Solana-specific dependencies
yellowstone-grpc-client = "5.0.0"
yellowstone-grpc-proto = "5.0.0"
solana-sdk = "2.1.15"
solana-transaction-status-client-types = "2.1.9"
solana-program = "2.1.10"
spl-token = { version = "7.0.0", features = ["no-entrypoint"] }
```

### Common Crate `Cargo.toml`

```toml
[package]
name = "common"
version = "0.1.0"
edition = "2024"

[dependencies]
redis = { workspace = true }
tokio = { workspace = true }
prometheus = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
```

### Solana Subscriber `Cargo.toml`

```toml
[package]
name = "solana-sub"
version = "0.1.0"
edition = "2024"

[dependencies]
# Workspace dependency on common crate
common = { path = "../common" }

# Shared dependencies from workspace
tokio = { workspace = true }
serde = { workspace = true }
tracing = { workspace = true }

# Solana-specific dependencies
yellowstone-grpc-client = { workspace = true }
yellowstone-grpc-proto = { workspace = true }
solana-sdk = { workspace = true }
# ... other Solana deps

[[bin]]
name = "solana-sub"
path = "src/main.rs"
```

## Migration Strategy

### Phase 1: Create Workspace Structure
1. Create workspace root `Cargo.toml`
2. Create `crates/` directory
3. Set up `crates/common/` with basic structure

### Phase 2: Extract Common Code
1. Move publisher implementations to `common`
2. Move metrics code to `common`
3. Move `TokenEvent` and shared types to `common`
4. Update imports and verify compilation

### Phase 3: Create Solana Subscriber
1. Create `crates/solana-sub/`
2. Move Solana-specific code from root
3. Update dependencies to use `common` crate
4. Move configuration files
5. Test end-to-end functionality

### Phase 4: Clean Up
1. Remove old root `src/` directory
2. Update documentation
3. Update build/deploy scripts
4. Update CI/CD pipelines

### Phase 5: Add New Chain (Polymarket)
1. Create `crates/polymarket-sub/`
2. Implement using `common` crate
3. Add chain-specific dependencies
4. Implement protocol-specific parsing

## Build & Deploy Workflow

### Development

```bash
# Build specific subscriber
cargo build -p solana-sub
cargo build -p polymarket-sub

# Test specific subscriber
cargo test -p solana-sub

# Build all
cargo build --workspace

# Format all
cargo fmt --all
```

### Docker

After refactoring, we'll organize Docker builds as follows:

```
docker/
├── solana-sub.Dockerfile      # Builds solana-sub binary
└── polymarket-sub.Dockerfile  # Builds polymarket-sub binary
```

Each Dockerfile will follow the existing multi-stage build pattern from `Dockerfile` at the project root, adapted for workspace builds:

```dockerfile
# Example: docker/solana-sub.Dockerfile
FROM rust:1.87-slim AS builder
# ... (similar setup to existing Dockerfile)
RUN cargo build --release -p solana-sub

FROM debian:bookworm-slim
# ... (runtime setup)
COPY --from=builder /app/target/release/solana-sub /usr/local/bin/
CMD ["solana-sub", "--config-file", "/app/config.yaml"]
```

This approach:
- Maintains consistency with the existing build process
- Creates subscriber-specific images without shared dependencies
- Enables independent deployment of each subscriber

### CI/CD Benefits
- Run tests in parallel for each subscriber
- Only build/deploy changed subscribers
- Independent deployment pipelines
- Faster feedback loops

## Shared Data Types

### TokenEvent (in `common`)

The existing `TokenEvent` type will be moved to `common` crate **without modifications**. This is the current standardized event format used by the Solana subscriber:

```rust
// Existing TokenEvent - no changes
pub struct TokenEvent {
    // Current fields preserved as-is
    // See: popeyes_trading_types crate for actual definition
}
```

**Important**: `TokenEvent` is specific to Solana token trading data. The Polymarket subscriber will use its own data types suitable for prediction market events.

### Publisher Trait (in `common`)

Generic publisher trait for Redis operations:

```rust
#[async_trait]
pub trait Publisher: Send + Sync {
    async fn publish(&self, data: &[u8]) -> Result<()>;
}
```

This allows each subscriber to publish its own data format while reusing the same Redis infrastructure.

## Configuration Management

### Shared Config Structure (in `common`)

```rust
pub struct RedisConfig {
    pub url: String,
    pub queues: Vec<QueueConfig>,
}

pub struct MetricsConfig {
    pub port: Option<u16>,
}
```

### Chain-Specific Config

Each subscriber maintains its own `config.yaml` with chain-specific settings:

**Solana** (`crates/solana-sub/config.yaml`):
```yaml
chain: solana
grpc:
  endpoint: "https://..."
  token: "..."
  program_ids: ["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"]

pumpfun:
  program_id: "..."

redis:
  url: "redis://localhost"
  queues: [...]
```

**Polymarket** (`crates/polymarket-sub/config.yaml`):
```yaml
# Configuration structure TBD - will be determined during implementation
chain: polymarket
websocket:
  endpoint: "wss://..."

redis:
  url: "redis://localhost"
  queues: [...]
```

## Testing Strategy

### Unit Tests
- Each crate has its own unit tests
- Test chain-specific logic independently
- Mock common components

### Integration Tests
- Test publisher implementations in `common`
- Test end-to-end flow for each subscriber
- Redis integration tests

### E2E Tests
- Each subscriber has its own E2E test binary
- Example: `crates/solana-sub/src/bin/e2e_test.rs`

## Metrics & Monitoring

### Shared Metrics (in `common`)
- `events_published_total{chain, event_type}`
- `publish_failures_total{chain, error_type}`
- `redis_operation_duration_seconds`

### Chain-Specific Metrics
- Solana: `transactions_processed`, `block_time_latency`
- Polymarket: `events_processed`, `block_number_lag`

## References

- Current architecture: `docs/architecture.md`
- Project repository: `popeyes-tx-sub`
