# poly-strat-starter

## What this project does

Runs a minimal `trade_server` backtest with a starter Polymarket strategy that buys cheap outcomes.

## File map

- `src/main.rs`: CLI parsing, `BacktestConfig` wiring, runner invocation, metrics output
- `src/strategy.rs`: strategy implementation (`CheapBuyerGenerator` + `BuyIntentSignal`)
- `Cargo.toml`: dependencies (including pinned `trade_server` git rev)
- `justfile`: build/run helpers

Primary edit target for agents: `src/strategy.rs`.

## How to build

```bash
cargo build
```

or

```bash
just build
```

## How to run

```bash
cargo run -- \
  --snapshot-path /path/to/snapshots.parquet \
  --update-path /path/to/updates.parquet \
  --trade-path /path/to/trades.parquet
```

Common flags:

- `--output-path output/backtest_events.jsonl`
- `--outcome-filter Up`
- `--ticker-pattern btc-updown-15m-*` (repeatable)
- `--threshold 0.30`
- `--trade-amount 100`
- `--max-positions 10`
- `--take-profit 0.15`
- `--stop-loss 0.10`
- `--max-hold-secs 3600`
- `--initial-balance 10000`
- `--capture-snapshots` (optional; disabled by default)
- `--capture-updates` (optional; disabled by default)
- `--capture-trades` (optional; disabled by default)
- `--capture-spot-prices` (optional; disabled by default)

By default, market-data events are not captured in `backtest_events.jsonl` to keep output size manageable. Re-enable specific event streams with the `--capture-*` flags when debugging.

## How to modify the strategy

Edit `generate_signal()` in `src/strategy.rs`.

- Change entry conditions and signal frequency
- Express desired orderbook state with intent signals (`is_intent_signal()` + `get_order_intent()`)
- Build intent levels with `trade_server::signal::QuoteLevel` (for example, `QuoteLevel::gtc(price, size)`)
- Keep signal timestamps tied to logical event time (see `event.timestamp()` usage)

## Reference docs

- trade-server backtesting guide:
  `/home/devbox/trade-server/docs/usage-guide/backtesting.md`
- trade-server intent signals guide:
  `/home/devbox/trade-server/docs/usage-guide/intent-signals.md`
- polysharp market-maker backtest guide:
  `/home/devbox/polysharp/docs/systems/market-maker/backtest-guide.md`

Use trade-server docs for:
- `SignalGenerator` and `TradableSignal` interfaces
- `BacktestConfig` and `PositionConfig`
- fill simulation model
- latency simulation and signal timestamp requirements
- event types and JSONL output format
