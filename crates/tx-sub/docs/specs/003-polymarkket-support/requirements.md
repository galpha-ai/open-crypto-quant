# Polymarket Subscriber Requirements

## Overview

This document specifies requirements for implementing a real-time Polymarket data subscription service (`polymarket-sub`) within the tx-sub workspace. The service will connect to Polymarket's WebSocket API, process trade events, and publish structured data to Redis for downstream analysis.

## Research Context

The downstream analysis focuses on detecting emotional panic signals that lead to overbought/oversold extremes and price deviations in prediction markets. This behavioral analysis requires real-time trade data to identify:
- Rapid price movements indicating panic buying/selling
- Volume spikes correlating with emotional market reactions
- Price deviations from fundamental probability estimates

**Initial Scope**: Focus on `last_trade_price` events for real-time trade tracking
**Future Scope**: Orderbook data (`book`, `price_change` events) for advanced market depth analysis

## Functional Requirements

### FR1: WebSocket Connection Management

**FR1.1**: Connect to Polymarket WebSocket API
- Establish secure WebSocket (WSS) connection to Polymarket CLOB API
- Manage connection lifecycle (open, error, close)

**FR1.2**: Connection Health Monitoring
- Maintain connection health via periodic ping messages
- Detect connection drops and exit process for orchestrator restart
- Log connection errors before exit

### FR2: Market Channel Subscription

**FR2.1**: Subscribe to Market Data
- Subscribe to public market channel for configured prediction market assets
- Support multiple asset subscriptions in single connection
- No authentication required for public market data

**FR2.2**: Configuration-driven Asset Selection
- Load asset IDs from YAML configuration
- Validate asset ID format on startup

### FR3: Event Processing

**FR3.1**: Parse Trade Events (Phase 1)
- Process `last_trade_price` events from WebSocket stream
- Extract trade data: asset ID, market ID, price, size, side, timestamp, fees
- Handle malformed messages with error logging
- Track parsing errors in metrics

**FR3.2**: Future Event Types (Phase 2)
- Support for orderbook snapshots (`book` events)
- Support for incremental orderbook updates (`price_change` events)
- Extensible parsing architecture for new event types

**FR3.3**: Event Validation
- Validate required fields presence
- Validate data types and ranges
- Log validation failures without service crash

### FR4: Data Publishing

**FR4.1**: Event Format
- Define Polymarket-specific event structure (`PolymarketTradeEvent`)
- Separate from existing Solana event types (incompatible data models)
- Include market metadata, trade data, and timestamp

**FR4.2**: Redis Publishing
- Publish to multiple Redis targets: LIST, STREAM, PUBSUB
- Serialize events to JSON
- Leverage shared Redis publisher infrastructure

**FR4.3**: Publishing Reliability
- Handle Redis connection failures gracefully
- Buffer events during temporary outages
- Track publishing errors in metrics
- Log failures with context

### FR5: Configuration Management

**FR5.1**: YAML Configuration
- Support WebSocket endpoint configuration
- Asset ID list configuration
- Redis connection and queue settings
- Metrics server port configuration

**FR5.2**: Environment Variable Support
- Allow environment variable substitution in config
- Provide clear error messages for missing variables

**FR5.3**: Configuration Validation
- Validate all configuration on startup
- Exit with clear error messages for invalid config

### FR6: Observability

**FR6.1**: Metrics
- Expose Prometheus metrics on configurable port
- Track events received, trades published, errors, connection status
- Monitor trade processing latency

**FR6.2**: Logging
- JSON-formatted structured logs
- Appropriate log levels (INFO, WARN, ERROR)
- Include context fields for debugging

### FR7: Error Handling and Resilience

**FR7.1**: Connection Failures
- Exit immediately on WebSocket connection loss
- Log errors with full context
- Use exit codes to indicate failure type (connection vs configuration)

**FR7.2**: Graceful Degradation
- Continue processing despite individual parsing errors
- Serve metrics even during Redis unavailability
- Buffer events during temporary outages

**FR7.3**: Shutdown Handling
- Handle SIGINT/SIGTERM signals
- Close connections gracefully
- Flush buffered events before exit

## Non-Functional Requirements

### NFR1: Workspace Integration
- Follow workspace architecture patterns established by `solana-sub`
- Reuse shared infrastructure from `common` crate
- Maintain consistent project structure

### NFR2: Performance
- Process events with minimal latency (< 1 second from trade timestamp to publish)
- Handle continuous event stream without memory leaks
- Efficient Redis publishing using pipelining

### NFR3: Reliability
- Service uptime target: 99%+ (excluding orchestrated restarts)
- Zero data loss during normal operation (Redis available)
- Clean recovery after connection failures via orchestrator restart

### NFR4: Testing
- Unit test coverage >80% for core parsing and validation logic
- Integration tests with mock WebSocket server
- Test fixtures covering normal and edge cases

### NFR5: Maintainability
- Clear separation of concerns (connection, parsing, publishing)
- Comprehensive error messages for debugging
- Consistent code style with existing workspace crates

## Out of Scope (Future Enhancements)

1. **User Channel**: Private WebSocket channel for account-specific events
2. **Orderbook Reconstruction**: Full orderbook state management from book/price_change events
3. **Historical Data Backfill**: Fetching historical trades via REST API
4. **Multi-Market Strategies**: Cross-market arbitrage or correlation analysis
5. **Event Replay**: Reading and replaying events from Redis for backtesting
6. **Dynamic Asset Management**: Adding/removing assets without restart

## Success Criteria

1. Successfully connects to Polymarket WebSocket API and maintains stable connection
2. Parses and publishes 100% of `last_trade_price` events for configured assets
3. All required metrics are exposed and accurate
4. Service runs continuously for 24+ hours in staging without crash
5. Exits cleanly with appropriate error codes on failures
6. Redis queues populated with valid JSON events consumable by downstream services
7. Zero data loss during normal operation (Redis available)
8. Test coverage >80% for core logic

## Implementation Dependencies

### Prerequisite: Trading Types Update

The `popeyes_trading_types` shared library must be updated before implementing `polymarket-sub`:

**Repository**: `/home/zfeng/popeyes/trading-types` (version 0.2.2)

**Required Changes**:
- Add `PolymarketTradeEvent` type definition
- Bump version to 0.2.3+
- Publish updated crate

**Rationale**: Shared type definition ensures serialization/deserialization consistency between producer (polymarket-sub) and consumers (analysis services)

### Implementation Phases

1. **Phase 1**: Update `popeyes_trading_types` and publish new version
2. **Phase 2**: Implement `polymarket-sub` with `last_trade_price` support
3. **Phase 3**: Implement downstream consumers using shared types

## References

- [Polymarket Subscriber Design Document](./design.md) - Technical design and implementation details
- [Polymarket WSS Overview](./wss-overview.md) - WebSocket API overview
- [Polymarket Market Channel Spec](./market-channel.md) - Market channel specification
- [Polymarket WSS Quickstart](./wss-quickstart.md) - Quick start guide
- [Workspace Architecture](../../architecture.md) - Overall workspace structure
- [Multi-Chain Workspace Refactoring Design](../002-multi-chain-workspace-refactoring/design.md) - Workspace refactoring background
