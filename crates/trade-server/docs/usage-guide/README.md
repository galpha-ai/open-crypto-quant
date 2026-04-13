# Trade Server Usage Guide

This guide explains how to build a trading bot using the `trade_server` crate as a library.

## Overview

The `trade_server` crate provides the core infrastructure for event-driven trading systems:

- **Event Coordination**: Redis-based event sourcing with multiple subscriber modes
- **Position Management**: Track positions, cash-flow-based PnL, and configurable exit strategies
- **Order Execution**: Transaction construction and submission (Solana, Jito, BloxRoute)
- **Notifications**: Multi-channel alerts (Telegram, Console)
- **API Server**: JSON-RPC API with pluggable handlers
- **Metrics**: Prometheus integration

For detailed internal architecture, see [architecture.md](../architecture.md).

## Quick Start

Your bot implements the strategy-specific logic by:

1. Implementing `SignalGenerator` to process events and generate buy/sell signals
2. Implementing `TradableSignal` to define your signal structure
3. Wiring components together using the provided builders

```
Your Application
    ├── config.yaml        # Configuration file
    ├── signal.rs          # TradableSignal implementation
    └── signal_gen.rs      # SignalGenerator implementation

        ↓ uses

trade_server crate
    ├── TradeServer        # Main event loop
    ├── EventCoordinator   # Event sourcing from Redis
    ├── OrderbookTracker   # Maintains orderbook state per market
    ├── PositionManager    # Position tracking & exit strategies
    ├── OrderExecutor      # Trade execution
    └── Notifier           # Alerts
```

## Event Types

The `SystemEvent` enum represents all events your signal generator receives:

```rust
pub enum SystemEvent {
    Token(TokenEvent),           // Token buy/sell/create events, orderbook updates
    Timer(TimerEvent),           // Periodic timer events
    Signal(Box<dyn TradableSignal>),
    Position(PositionEvent),     // Position lifecycle events
    Execution(ExecutionEvent),   // Market order results
    LimitOrder(LimitOrderEvent), // Limit order lifecycle (orderbook venues)
    Redemption(RedemptionEvent), // Pair redemption events (binary markets)
}
```

Token events include:
- `Buy`, `Sell`, `Create`, `Swap` for AMM trading
- `OrderbookSnapshot`, `OrderbookUpdate` for CLOB venues

## Guide Contents

| Document                                  | Description                                                  |
|-------------------------------------------|--------------------------------------------------------------|
| [Core Traits](core-traits.md)             | SignalGenerator, TradableSignal, and ExitStrategy interfaces |
| [Configuration](configuration.md)         | Redis modes, position management, and execution settings     |
| [Orderbook Trading](orderbook-trading.md) | OrderbookTracker, signal intents, and exit modes             |
| [Intent-Based Signals](intent-signals.md) | Market making with intent-based position management          |
| [Backtesting](backtesting.md)             | Run strategies against historical data for evaluation        |
| [Paper Trading](paper-trading.md)         | Test strategies against live data without risking capital    |

## Best Practices

1. **Configuration-driven design**: Put tunable parameters in config files, not code
2. **Structured logging**: Use `tracing` with structured fields
3. **Prometheus metrics**: Expose operational metrics for monitoring
4. **Error handling**: Use `anyhow::Result` with context for debugging
5. **Test with mocks**: Create mock implementations for testing without external dependencies

## Summary

Building a trading bot with `trade_server`:

1. **Define configuration** in YAML with Redis, position, and execution settings
2. **Implement `TradableSignal`** for your signal structure
3. **Implement `SignalGenerator`** with your strategy logic
4. **Wire components** and create `TradeServer`
5. **Run the event loop** with `trade_server.run().await`

The crate handles event sourcing, position management, order execution, and notifications. You focus on implementing the strategy logic in your `SignalGenerator`.
