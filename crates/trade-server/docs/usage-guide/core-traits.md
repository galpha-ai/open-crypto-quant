# Core Traits

This document covers the main traits you need to implement when building a trading bot with `trade_server`.

## SignalGenerator

Process events and generate trading signals:

```rust
#[async_trait]
pub trait SignalGenerator {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>>;
}
```

The `generate_signal` method is called for every event. Return an empty vector if no signals should be generated.

### Example

```rust
use trade_server::signal::SignalGenerator;
use trade_server::domain::SystemEvent;

struct MyStrategy {
    threshold: f64,
}

#[async_trait]
impl SignalGenerator for MyStrategy {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        match event {
            SystemEvent::Token(token_event) => {
                // Your strategy logic here
                Ok(vec![])
            }
            _ => Ok(vec![]),
        }
    }
}
```

## TradableSignal

Define your signal structure. Key methods:

| Method | Purpose |
|--------|---------|
| `signal_id()` | Unique identifier for the signal |
| `signal_type()` | Type classification (e.g., "buy", "sell") |
| `get_mint()` | Token mint address |
| `get_price()` | Signal price |
| `get_timestamp()` | When the signal was generated |
| `passes_filter()` | Return `true` if the signal should result in a trade |
| `as_notifiable()` | Return `Some(...)` to send notifications |
| `signal_action()` | Indicate Entry, Exit, ModifyOrder, or CancelOrder (default: Entry) |

### Minimal Implementation

```rust
use trade_server::signal::TradableSignal;

#[derive(Debug)]
struct MySignal {
    id: String,
    mint: String,
    price: f64,
    timestamp: DateTime<Utc>,
}

impl TradableSignal for MySignal {
    fn signal_id(&self) -> &str { &self.id }
    fn signal_type(&self) -> &str { "my_signal" }
    fn get_mint(&self) -> Option<&str> { Some(&self.mint) }
    fn get_price(&self) -> Option<f64> { Some(self.price) }
    fn get_timestamp(&self) -> Option<DateTime<Utc>> { Some(self.timestamp) }
    fn get_slot(&self) -> Option<u64> { None }
    fn get_creator(&self) -> Option<Pubkey> { None }
    fn passes_filter(&self) -> bool { true }
    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> { None }
    fn as_any(&self) -> &dyn Any { self }
    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "id": self.id,
            "mint": self.mint,
            "price": self.price,
        }))
    }
}
```

## ExitStrategy

Control when positions are automatically closed:

```rust
pub trait ExitStrategy: Send + Sync {
    fn should_exit(&self, position: &Position, current_time: DateTime<Utc>) -> bool;
    fn get_exit_reason(&self, position: &Position, current_time: DateTime<Utc>) -> ExitReason;
}
```

### Built-in ConfigurableExitStrategy

The built-in `ConfigurableExitStrategy` supports:

| Parameter | Description |
|-----------|-------------|
| `take_profit_pct` | Exit at profit threshold (e.g., 0.5 = +50%) |
| `stop_loss_pct` | Exit at loss threshold (e.g., 0.2 = -20%) |
| `max_hold_time_secs` | Maximum holding period |
| `max_sell_failures` | Force exit after N failed sell attempts |

See [Configuration](configuration.md) for YAML configuration examples.

## Next Steps

- [Configuration](configuration.md) - Configure Redis, positions, and execution
- [Orderbook Trading](orderbook-trading.md) - Working with CLOB venues
- [Intent-Based Signals](intent-signals.md) - Advanced market making patterns
