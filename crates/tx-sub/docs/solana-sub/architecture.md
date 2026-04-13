# Solana Subscriber Architecture

## Overview

The Solana subscriber (`solana-sub`) connects to Solana blockchain via gRPC, processes transactions in real-time, and publishes parsed trade data to Redis queues. It focuses on monitoring and parsing transactions from specific protocols like PumpFun and Raydium AMM v4.

This crate is part of the `popeyes-tx-sub` workspace. For workspace-level architecture decisions, see [docs/specs/002-multi-chain-workspace-refactoring/design.md](../specs/002-multi-chain-workspace-refactoring/design.md).

## Crate Structure

```
crates/solana-sub/
└── src/
    ├── main.rs           # Entry point
    ├── app.rs            # App orchestration
    ├── config.rs         # Configuration loading
    ├── metrics.rs        # Prometheus metrics
    ├── grpc/             # Yellowstone gRPC client
    │   ├── grpc.rs       # GrpcDataSubscriptionManager
    │   └── types.rs      # Transaction types
    ├── parser/           # Transaction parsing
    │   ├── parser.rs     # Core parser
    │   ├── market.rs     # Market trade types
    │   ├── pumpfun.rs    # PumpFun protocol parser
    │   └── raydium.rs    # Raydium AMM v4 parser
    ├── redis_trade_consumer.rs   # Trade publishing
    ├── redis_tx_consumer.rs      # Raw tx persistence
    └── redis_tx_retriever.rs     # Tx retrieval utility
```

## Components

### 1. Main Entry Point (`main.rs`)
- Initializes logging with structured JSON output
- Parses YAML configuration file
- Creates and runs the main App instance

### 2. Configuration (`config.rs`)
The service uses a YAML configuration file with the following structure:
- **gRPC settings**: Endpoint URL, authentication token, and program IDs to monitor
- **PumpFun settings**: Program ID and optional RPC URL
- **Redis settings**: Connection URL and queue configurations (shared from `common` crate)
- **Metrics settings**: Optional Prometheus metrics port (shared from `common` crate)
- **Transaction persistence**: Optional Redis-based transaction storage

### 3. Core Application (`app.rs`)
The `App` struct orchestrates all components:
- Creates broadcast channels for inter-component communication
- Initializes gRPC subscriber, parser, and Redis consumers
- Runs all components concurrently using `tokio::select!`

## Data Flow

```
Solana Blockchain
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
RedisTradeConsumer --> Redis Queues
       |
       v
  TokenEvent JSON
```

### 4. gRPC Integration (`grpc/`)

**GrpcDataSubscriptionManager** (`grpc.rs`)
- Connects to Yellowstone gRPC service for real-time blockchain data
- Subscribes to transactions for specified program IDs
- Filters out vote and failed transactions
- Sends periodic ping messages to maintain connection
- Broadcasts received transactions via channel
- Tracks block time latency metrics

**Transaction Types** (`types.rs`)
- Supports two transaction formats:
  - `UiTransaction`: Standard Solana UI transaction format
  - `GrpcTransaction`: Yellowstone gRPC transaction format

### 5. Transaction Parser (`parser/`)

**Core Parser** (`parser.rs`)
- Receives transactions from gRPC broadcast channel
- Reconstructs `VersionedTransaction` from gRPC data
- Handles address lookup tables for transaction decoding
- Processes both main instructions and inner instructions
- Identifies and routes transactions to protocol-specific parsers

#### Supported Protocols

**PumpFun**
- Parses token creation events
- Parses buy/sell trade logs with detailed market data
- Extracts virtual and real reserves, fees, and creator info

**Raydium AMM v4**
- Parses swap transactions
- Determines swap direction (BaseIn/BaseOut)
- Extracts input/output amounts and token mints

**Market Trade Types** (`market.rs`)
- `MarketTrade`: Wrapper containing slot, signature, and specific trade log
- `TradeLog`: Enum of supported trade types (PumpFun, RayAMMv4, TokenCreation)
- Conversion to standardized `TokenEvent` format for downstream consumers

### 6. Redis Integration

**RedisTradeConsumer** (`redis_trade_consumer.rs`)
- Receives parsed `MarketTrade` events from parser
- Converts to `TokenEvent` JSON format
- Uses Redis publishers from `common` crate (List, Pubsub, Stream)
- Publishes to multiple Redis queues with configurable max lengths
- Uses pipelined operations for efficiency
- Tracks parsed trades by DEX in metrics

**RedisTxConsumer** (`redis_tx_consumer.rs`)
- Optional component for persisting raw transactions
- Stores protobuf-encoded transactions in Redis
- Uses configurable TTL (default 10 minutes)
- Key format: `/transactions/{environment}/{signature}`

**RedisTxRetriever** (`redis_tx_retriever.rs`)
- Utility for retrieving persisted transactions by signature
- Decodes protobuf data back to `SubscribeUpdateTransaction`

### 7. Metrics (`metrics.rs`)
- Prometheus-compatible metrics exposed on configurable port
- Uses shared metrics server from `common` crate
- Solana-specific metrics:
  - `transactions_processed`: Total gRPC transactions received
  - `parsed_trades_total`: Trades parsed by DEX
  - `send_failures`: Channel send failures by type
  - `block_time_latency`: Histogram of block processing latency

## Key Design Patterns

1. **Broadcast Channels**: Used for efficient multi-consumer data distribution
2. **Concurrent Processing**: All components run as independent async tasks
3. **Error Resilience**: Components handle lagged receivers and continue processing
4. **Modular Parsers**: Each protocol has its own parsing logic and types
5. **Structured Logging**: JSON-formatted logs with contextual information

## Performance Considerations

- **Caching**: Address lookup table data cached to reduce RPC calls
- **Connection Pooling**: Redis `ConnectionManager` for efficient connections
- **Batched Operations**: Redis pipeline commands for efficiency
- **Graceful Degradation**: Handles broadcast channel lag gracefully
- **Monitoring**: High block processing latency (>5 seconds) tracked and reported
