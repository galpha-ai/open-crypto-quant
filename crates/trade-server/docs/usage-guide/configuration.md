# Configuration

This document covers the YAML configuration options for the trade server.

## Redis Event Sources

The trade server supports three Redis subscriber modes for reading events.

### List Mode (default)

Simple queue semantics using BRPOP:

```yaml
redis:
  url: "redis://localhost:6379"
  token_events_key: "token_events"
  subscriber:
    type: list
    timeout_secs: 5
```

### Stream Mode

Consumer groups with at-least-once delivery:

```yaml
redis:
  url: "redis://localhost:6379"
  token_events_key: "token_events"
  subscriber:
    type: stream
    consumer_group: "trade-server"
    consumer_name: "instance-1"  # optional, defaults to hostname-pid
    block_ms: 5000               # optional
    count: 10                    # optional
```

### Pubsub Mode

Real-time broadcast (fire-and-forget):

```yaml
redis:
  url: "redis://localhost:6379"
  subscriber:
    type: pubsub
    channels:
      - "token_events"
      - "orderbook_updates"
```

## Position Management

Configure position tracking and exit strategies:

```yaml
position:
  trade_amount_sol: 0.1
  initial_sol: 10.0
  max_holding_period_secs: 300
  max_open_positions: 5
  exit_strategy:
    strategy_type: configurable
    take_profit_pct: 0.5    # Exit at +50% profit
    stop_loss_pct: 0.2      # Exit at -20% loss
    max_hold_time_secs: 300 # Exit after 5 minutes
    max_sell_failures: 3
```

### Parameters

| Parameter | Description |
|-----------|-------------|
| `trade_amount_sol` | SOL amount per trade |
| `initial_sol` | Starting balance for P&L tracking |
| `max_holding_period_secs` | Maximum time to hold a position |
| `max_open_positions` | Limit on concurrent positions |

### Exit Strategy Parameters

| Parameter | Description |
|-----------|-------------|
| `take_profit_pct` | Profit percentage to trigger exit (0.5 = 50%) |
| `stop_loss_pct` | Loss percentage to trigger exit (0.2 = 20%) |
| `max_hold_time_secs` | Time-based exit trigger |
| `max_sell_failures` | Force exit after N failed sells |

## Execution

Configure order execution and transaction submission:

```yaml
execution:
  simulation_mode: disabled  # pure, rpc_based, or disabled
  buy_slippage: 0.05
  sell_slippage: 0.05
  txn_maker:
    url: "http://localhost:8081"
    connection_type: tcp     # tcp, unix, or shadow
  submitter:
    submitter_type: solana   # solana, jito, bloxroute, or zeroslot
    confirmation_timeout_secs: 60
```

### Simulation Modes

| Mode | Description |
|------|-------------|
| `disabled` | Real trading (default) |
| `pure` | Simulated execution, no blockchain interaction |
| `rpc_based` | Simulated with RPC validation |

### Transaction Maker

| Parameter | Description |
|-----------|-------------|
| `url` | Transaction constructor service endpoint |
| `connection_type` | `tcp`, `unix` socket, or `shadow` |

### Submitter Types

| Type | Description |
|------|-------------|
| `solana` | Direct Solana RPC submission |
| `jito` | Jito MEV-protected submission |
| `bloxroute` | BloxRoute high-speed relay |
| `zeroslot` | ZeroSlot submission service |

## Full Example

```yaml
redis:
  url: "redis://localhost:6379"
  token_events_key: "token_events"
  subscriber:
    type: stream
    consumer_group: "my-bot"

position:
  trade_amount_sol: 0.1
  initial_sol: 10.0
  max_holding_period_secs: 300
  max_open_positions: 5
  exit_strategy:
    strategy_type: configurable
    take_profit_pct: 0.5
    stop_loss_pct: 0.2
    max_hold_time_secs: 300
    max_sell_failures: 3

execution:
  simulation_mode: disabled
  buy_slippage: 0.05
  sell_slippage: 0.05
  txn_maker:
    url: "http://localhost:8081"
    connection_type: tcp
  submitter:
    submitter_type: jito
    confirmation_timeout_secs: 60
```

## Next Steps

- [Core Traits](core-traits.md) - Implement SignalGenerator and TradableSignal
- [Orderbook Trading](orderbook-trading.md) - Configure for CLOB venues
