# crypto-quant

Crypto quant trading monorepo — trading server, data ingestion, market subscriptions, and strategy framework.

## Crates

| Crate | Description |
|-------|-------------|
| `trade-server` | Core trading server and backtest engine |
| `poly-strat-starter` | Trading strategy starter template |
| `ingester` | ClickHouse data ingestion service |
| `tx-sub/common` | Shared infrastructure (config, publishers, subscribers) |
| `tx-sub/solana-sub` | Solana transaction subscription service |
| `tx-sub/polymarket-sub` | Polymarket WebSocket subscription service |
| `tx-sub/spot-price-sub` | Spot price subscription service |
| `proto` | Protobuf service definitions |
| `polyfill-rs` | Polymarket HFT client library |

## Tools

| Tool | Language | Description |
|------|----------|-------------|
| `poly-data` | Python | Backtest data management CLI |

## Build

```bash
cargo check          # check all crates
cargo build          # build all crates
cargo test           # run all tests
```

## Individual binaries

```bash
cargo build -p trade_server
cargo build -p ingester
cargo build -p solana-sub
cargo build -p polymarket-sub
cargo build -p spot-price-sub
cargo build -p poly-strat-starter
```
