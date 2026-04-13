# Multi-Chain Transaction Subscriber

A Rust workspace containing blockchain transaction subscription services that connect to various blockchains, process transactions in real-time, and publish parsed trade data to Redis.

## Overview

This workspace includes:
- **`crates/common`**: Shared infrastructure (Redis publishers, metrics, types)
- **`crates/solana-sub`**: Solana transaction subscriber (PumpFun, Raydium AMM v4)
- **`crates/polymarket-sub`**: Polymarket prediction market subscriber

Each subscriber connects to its respective blockchain/API, processes events in real-time, and publishes structured data to Redis queues, streams, and pubsub channels.

## Quick Start

### Prerequisites

- **Rust** (1.70+)
- **Docker** and Docker Compose
- **Just** (optional, but recommended)

### Run with Docker Compose

```bash
# Start all services (Redis + all subscribers)
just run-local

# View logs
just logs-local

# Stop services
just stop-local
```

### Run Individual Subscribers

```bash
# Solana subscriber only
just run-local-solana
just logs-solana-local

# Polymarket subscriber only
just run-local-polymarket
just logs-polymarket-local
```

## Building and Testing

```bash
# Build all crates
just build

# Build specific subscriber
just build-solana
just build-polymarket

# Run all tests
just test

# Run specific tests
just test-solana
just test-polymarket

# Format code
just fmt

# Check code
just check
```

## Configuration

Each subscriber uses a YAML configuration file:

**Solana**: `configs/solana-sub/config.yaml`
- gRPC endpoint and authentication
- Program IDs to monitor
- Redis connection and queues
- Optional transaction persistence

**Polymarket**: `configs/polymarket-sub/config.yaml`
- WebSocket endpoint
- Market discovery settings (optional)
- Health monitoring settings (optional)
- Redis connection and targets

See service-specific developer guides for detailed configuration options.

## Monitoring

### Redis Data

Monitor published data:

```bash
# Connect to Redis
docker compose exec redis redis-cli

# Check Solana queues
LRANGE sniper_tx_events 0 -1
LRANGE ch_tx_events_v2 0 -1

# Check Polymarket queues
LRANGE polymarket:trades 0 -1

# Monitor in real-time
MONITOR
```

## Development Workflow

1. **Make code changes**
2. **Run tests**: `just test`
3. **Format code**: `just fmt`
4. **Rebuild and restart**:
   - Docker: `just restart-solana-local` or `just restart-polymarket-local`
   - Native: Re-run `cargo run` command

## Project Structure

```
.
├── crates/
│   ├── common/              # Shared infrastructure
│   ├── solana-sub/          # Solana subscriber
│   └── polymarket-sub/      # Polymarket subscriber
├── configs/
│   ├── solana-sub/          # Solana configurations
│   └── polymarket-sub/      # Polymarket configurations
├── docs/
│   ├── architecture.md      # Architecture documentation
│   ├── solana-sub/          # Solana-specific docs
│   ├── polymarket-sub/      # Polymarket-specific docs
│   └── specs/               # Design specifications
├── docker-compose.yaml      # Docker Compose configuration
├── justfile                 # Command runner recipes
└── Cargo.toml              # Workspace configuration
```

## Documentation

- **Architecture**: [docs/architecture.md](docs/architecture.md)
- **Solana Developer Guide**: [docs/solana-sub/developer-guide.md](docs/solana-sub/developer-guide.md)
- **Polymarket Developer Guide**: [docs/polymarket-sub/developer-guide.md](docs/polymarket-sub/developer-guide.md)

