# Orderbook Trading

This document covers trading on CLOB (Central Limit Order Book) venues such as prediction markets.

## Exit Modes

Positions support two exit modes:

### Automatic Mode (default)

The `ExitStrategy` controls exit timing using stop-loss, take-profit, and timeout rules. This is the default mode for AMM-based trading.

### Strategy-Managed Mode

The `SignalGenerator` controls exit timing by emitting Exit signals. Use this for CLOB strategies that need fine-grained control over exit order placement.

```rust
// Position opened with Strategy-Managed exit mode
position.exit_mode = ExitMode::StrategyManaged;
```

Safety limits still apply in Strategy-Managed mode as a backstop.

## OrderbookTracker

For CLOB venues, the trade server provides an `OrderbookTracker` that maintains full orderbook state per market.

### How It Works

The `OrderbookTracker` automatically:

1. Stores full orderbook state when receiving `OrderbookSnapshot` events
2. Applies incremental `OrderbookUpdate` events to produce merged snapshots
3. Converts update events to snapshot events before passing to signal generators
4. Evicts inactive markets after 1 hour (configurable)

### Signal Generator Simplification

**Your signal generator only needs to handle `OrderbookSnapshot` events.** The trade server converts all `OrderbookUpdate` events into `OrderbookSnapshot` events internally, so you always receive a complete view of the orderbook.

```rust
async fn generate_signal(&mut self, event: &SystemEvent) -> Result<Vec<Box<dyn TradableSignal>>> {
    match event {
        SystemEvent::Token(TokenEvent::OrderbookSnapshot(snapshot)) => {
            // You always receive the full orderbook state here
            // Even if the original event was an OrderbookUpdate
            let best_bid = snapshot.bids.first().map(|b| b.price);
            let best_ask = snapshot.asks.first().map(|a| a.price);

            // Your strategy logic...
            Ok(vec![])
        }
        _ => Ok(vec![]),
    }
}
```

## Signal Actions

Signals use `signal_action()` to indicate their purpose:

| Action | Description |
|--------|-------------|
| `Entry` | Open a new position (default) |
| `Exit` | Close an existing position |
| `ModifyOrder` | Adjust an existing limit order |
| `CancelOrder` | Cancel an active limit order |

### Using Signal Actions

Implement `signal_action()` on your signal to control routing:

```rust
impl TradableSignal for ExitSignal {
    fn signal_action(&self) -> SignalAction {
        SignalAction::Exit
    }

    fn references_position(&self) -> Option<&str> {
        Some(&self.mint)  // Which position to exit
    }

    // ... other required methods
}
```

Use these actions to manage exits by emitting `ExitSignal`, `ModifyOrderSignal`, or `CancelOrderSignal` from your signal generator.

## Pair Redemption (Binary Markets)

For binary prediction markets, positions can be closed through pair redemption. When you hold both Up and Down tokens, they can be redeemed for $1 per pair.

Use `RedemptionPolicy` with intent signals to enable automatic redemption:

```rust
use trade_server::signal::{OrderIntent, RedemptionPolicy};

let intent = OrderIntent::new(mint, market, bids, asks, signal_id, timestamp, None)
    .with_redemption_policy(RedemptionPolicy::new(
        market_id,
        up_asset_id,
        down_asset_id,
        50.0,  // Max unredeemed pair value
    ));
```

When redemption occurs, positions close with `ExitReason::Redemption`.

See [Intent Signals - Pair Redemption](intent-signals.md#pair-redemption-for-binary-markets) for full documentation.

## Next Steps

- [Intent-Based Signals](intent-signals.md) - Advanced market making with reconciliation
- [Core Traits](core-traits.md) - TradableSignal implementation details
