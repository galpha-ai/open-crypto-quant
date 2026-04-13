# Polymarket Subscriber Documentation

This directory contains documentation specific to the Polymarket subscriber implementation.

## Available Documentation

### [Developer Guide](developer-guide.md)
Comprehensive guide for developers working on the Polymarket subscriber.

**Contents:**
- Getting started and quick setup
- Architecture overview and component interaction
- Development workflow (building, running, debugging)
- Testing (unit tests, integration tests, e2e tests)
- Configuration management
- Debugging and troubleshooting
- Contributing guidelines

**Target audience:** Developers contributing to or maintaining the Polymarket subscriber

### Related Documentation

- **Main architecture docs**: [`../architecture.md`](../architecture.md) - Overall tx-sub workspace architecture
- **Market metadata spec**: [`../specs/005-polymarket-crypto-prediction-data-sub/polymarket-market-metadata.md`](../specs/005-polymarket-crypto-prediction-data-sub/polymarket-market-metadata.md) - Design and implementation of market metadata enrichment feature
- **Workspace refactoring**: [`../specs/002-multi-chain-workspace-refactoring/design.md`](../specs/002-multi-chain-workspace-refactoring/design.md) - Multi-chain workspace design decisions

## Quick Links

### For New Contributors
1. Start with [Developer Guide - Getting Started](developer-guide.md#getting-started)
2. Review [Architecture Overview](developer-guide.md#architecture-overview)
3. Read [Contributing Guidelines](developer-guide.md#contributing)

### For Testing
1. [Integration Test](developer-guide.md#integration-test) - 10-second automated test
2. [Unit Tests](developer-guide.md#unit-tests) - Component-level testing
3. [End-to-End Testing](developer-guide.md#end-to-end-testing) - Real-world testing

### For Debugging
1. [Logging Levels](developer-guide.md#logging-levels)
2. [Metrics Debugging](developer-guide.md#metrics-debugging)
3. [Redis Debugging](developer-guide.md#redis-debugging)
4. [Common Issues](developer-guide.md#common-issues)

## External Resources

- **Polymarket API Documentation**: https://docs.polymarket.com/
- **Polymarket CLOB WebSocket**: wss://ws-subscriptions-clob.polymarket.com/ws/market
- **Polymarket Gamma API**: https://gamma-api.polymarket.com
- **Trading Types Crate**: [`popeyes_trading_types`](https://crates.io/crates/popeyes_trading_types)
