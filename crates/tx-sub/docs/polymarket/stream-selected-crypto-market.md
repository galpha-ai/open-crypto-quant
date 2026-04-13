# Stream Crypto Binary Prediction Markets - Complete Workflow

## Overview

This document demonstrates the complete end-to-end workflow for discovering and streaming real-time data from Polymarket's **15-minute crypto binary prediction markets** using the Polymarket CLI. These markets predict whether Bitcoin, Ethereum, Solana, or XRP will go "Up" or "Down" during specific 15-minute time windows.

## What Are Crypto Binary Prediction Markets?

Polymarket offers 15-minute binary prediction markets for major cryptocurrencies:
- **Bitcoin (BTC)** - ~97 markets
- **Ethereum (ETH)** - ~97 markets
- **Solana (SOL)** - ~97 markets
- **XRP (XRP)** - ~98 markets

Each market:
- Predicts "Up" or "Down" price movement during a 15-minute window
- Has a unique ticker like `sol-updown-15m-1763489700` (timestamp = end time)
- Resolves using Chainlink price feeds (not spot exchange prices)
- Is created ~15 minutes before its start time
- Gets replaced by a new market every 15 minutes (markets are NOT reused)

## Quick Start

```bash
# 1. Setup (one-time)
cd polycli
bun run start -- tags save
bun run start -- markets save --tag crypto

# 2. Find current Solana markets
bun run start -- markets list --tag crypto --active true --filter sol-updown-15m --limit 5

# 3. Stream a specific market
bun run start -- markets stream sol-updown-15m-1763489700 --verbose
```

## Prerequisites

- Bun runtime installed
- Polymarket CLI installed and built
- Internet connection for API access

## Complete Workflow

### Step 1: Initialize Tags Database

First, populate the local database with available tags from Polymarket:

```bash
cd polycli
bun run start -- tags save --batch-size 100
```

**Output:**
```
Fetched 0 tags (offset: 0, batch: 0)
Fetched 100 tags (offset: 0, batch: 100)
Fetched 200 tags (offset: 100, batch: 100)
...
Successfully saved 4377 tags to ./polymarket.db
```

### Step 2: Fetch All Crypto Markets

Fetch all markets and events associated with the crypto tag (ID: 21). **This step stores asset IDs (clobTokenIds) locally:**

```bash
bun run start -- markets save --tag crypto --batch-size 50
```

**Output:**
```
Fetching markets for tag 'Crypto' (ID: 21)
Fetched 50 events and 138 markets (offset: 0)
Fetched 100 events and 237 markets (offset: 50)
Fetched 150 events and 498 markets (offset: 100)
...
Successfully saved 869 events and 1985 markets for tag 'Crypto' (ID: 21) to ./polymarket.db
```

This saves ALL crypto markets, including the 15-minute binary prediction markets.

### Step 3: Query Current 15-Minute Binary Prediction Markets

Now query the database to find **active** 15-minute crypto binary prediction markets. These markets have tickers matching the pattern `{crypto}-updown-15m-{timestamp}`.

#### Option A: Find All Active 15-Minute Markets (All Cryptos)

```bash
bun run start -- markets list --tag crypto --active true --filter updown-15m --limit 20
```

**Output:**
```
Showing 1-20 of 388 markets

┌────────┬─────────────────────────────┬──────────────────────────────────────────┬────────────────────┬────────┬────────┐
│ ID     │ Ticker                      │ Title                                     │ End Date           │ Volume │ Liquid │
├────────┼─────────────────────────────┼──────────────────────────────────────────┼────────────────────┼────────┼────────┤
│ 83768  │ sol-updown-15m-1763489700   │ Solana Up or Down - Nov 18, 1:15-1:30 PM │ 2025-11-18T18:30:00│ 12534  │ 5432   │
├────────┼─────────────────────────────┼──────────────────────────────────────────┼────────────────────┼────────┼────────┤
│ 83769  │ btc-updown-15m-1763489700   │ Bitcoin Up or Down - Nov 18, 1:15-1:30 PM│ 2025-11-18T18:30:00│ 45231  │ 23451  │
├────────┼─────────────────────────────┼──────────────────────────────────────────┼────────────────────┼────────┼────────┤
│ 83770  │ eth-updown-15m-1763489700   │ Ethereum Up or Down - Nov 18, 1:15-1:30PM│ 2025-11-18T18:30:00│ 34125  │ 17823  │
└────────┴─────────────────────────────┴──────────────────────────────────────────┴────────────────────┴────────┴────────┘
```

#### Option B: Find Solana Markets Only

```bash
bun run start -- markets list --tag crypto --active true --filter sol-updown-15m --limit 10
```

#### Option C: Find Bitcoin Markets Only

```bash
bun run start -- markets list --tag crypto --active true --filter btc-updown-15m --limit 10
```

#### Option D: Find Upcoming Markets (Next 2 Hours)

To find markets that will resolve soon, use JSON output and filter by end date:

```bash
bun run start -- markets list --tag crypto --active true --filter updown-15m --format json --all | \
  jq '[.[] | select(.end_date > (now | strftime("%Y-%m-%dT%H:%M:%S")) and .end_date < ((now + 7200) | strftime("%Y-%m-%dT%H:%M:%S")))] | .[0:10]'
```

### Step 4: Get Specific Market Details

View detailed information about a specific 15-minute market event:

```bash
bun run start -- markets get sol-updown-15m-1763489700 --format json
```

**Output (truncated):**
```json
{
  "id": "83768",
  "ticker": "sol-updown-15m-1763489700",
  "slug": "solana-up-or-down-november-18-115pm-130pm-et",
  "title": "Solana Up or Down - November 18, 1:15PM-1:30PM ET",
  "description": "This market will resolve to \"Up\" if...",
  "end_date": "2025-11-18T18:30:00Z",
  "active": true,
  "markets": [
    {
      "id": "687550",
      "question": "Solana Up or Down - November 18, 1:15PM-1:30PM ET",
      "condition_id": "0x22a74a25eaf9addf08e35530a2c37f85794e5a0ae6df7524b89a58d9ad497568",
      "outcomes": "[\"Up\", \"Down\"]",
      "outcome_prices": "[\"0.52\", \"0.48\"]",
      "clob_token_ids": "[\"109681959945973826496234384791167033612800000000000000000000000376\", \"52181619848812915160551060468842099699261274828279546744438464278138132224\"]"
    }
  ]
}
```

**Key Fields:**
- `ticker`: Pattern is `{crypto}-updown-15m-{unix_timestamp}` where timestamp = end time
- `condition_id`: Required for smart contract interactions
- `clob_token_ids`: Asset IDs for WebSocket streaming (one per outcome: "Up" and "Down")
- `end_date`: When the market resolves

### Step 5: Stream Market Data in Real-Time

**This is the key feature** - stream real-time orderbook data for a 15-minute binary prediction market:

```bash
bun run start -- markets stream sol-updown-15m-1763489700 --verbose
```

**Output:**
```
Streaming event: Solana Up or Down - November 18, 1:15PM-1:30PM ET
Markets: 1
Asset IDs: 2

Asset IDs:
109681959945973826496234384791167033612800000000000000000000000376 (Up)
52181619848812915160551060468842099699261274828279546744438464278138132224 (Down)

[2025-11-18T18:22:09.186Z] Connecting to wss://ws-subscriptions-clob.polymarket.com/ws/market
[2025-11-18T18:22:09.425Z] Connected and subscribed: {"assetCount":2}
[{"market":"0x22a74a25eaf9addf08e35530a2c37f85794e5a0ae6df7524b89a58d9ad497568","asset_id":"109681959945973826496234384791167033612800000000000000000000000376","timestamp":"1763489754401","hash":"e95d8ee9e15569867cfbda6fe6fde4dbb957e7d5","bids":[{"price":"0.51","size":"1234.56"},{"price":"0.50","size":"2345.67"}],"asks":[{"price":"0.53","size":"987.65"},{"price":"0.54","size":"1876.54"}],"event_type":"book","last_trade_price":"0.52"}]
[{"market":"0x22a74a25eaf9addf08e35530a2c37f85794e5a0ae6df7524b89a58d9ad497568","asset_id":"52181619848812915160551060468842099699261274828279546744438464278138132224","timestamp":"1763489754423","hash":"a12b3c4d5e6f7g8h9i0j","bids":[{"price":"0.47","size":"2143.21"}],"asks":[{"price":"0.49","size":"1654.32"}],"event_type":"book","last_trade_price":"0.48"}]
...
```

The CLI automatically:
1. ✅ Looks up the event by ticker (or slug/ID)
2. ✅ Retrieves the associated market from the database
3. ✅ Extracts the 2 asset IDs (one for "Up", one for "Down")
4. ✅ Connects to Polymarket's WebSocket endpoint
5. ✅ Subscribes to both asset IDs
6. ✅ Streams real-time orderbook updates, price changes, and trades

### Step 6: Stream with Filters

Filter specific event types for more focused monitoring of orderbook changes:

```bash
bun run start -- markets stream sol-updown-15m-1763489700 \
  --event-types price_change,last_trade_price \
  --verbose
```

**Available event types:**
- `book` - Full orderbook snapshots (bids/asks)
- `price_change` - Price level changes (new orders, cancellations)
- `tick_size_change` - Tick size updates
- `last_trade_price` - Trade execution events

### Step 7: Save Stream to File

Log all stream data to a file for later analysis:

```bash
bun run start -- markets stream sol-updown-15m-1763489700 \
  --log-file sol_market_1763489700.jsonl
```

This creates a JSONL (JSON Lines) file with each WebSocket message on a separate line, ideal for processing.

### Step 8: Stream Multiple Markets Simultaneously

To monitor multiple cryptos at the same time, open separate terminal windows:

**Terminal 1 - Solana:**
```bash
bun run start -- markets stream sol-updown-15m-1763489700 --log-file sol.jsonl
```

**Terminal 2 - Bitcoin:**
```bash
bun run start -- markets stream btc-updown-15m-1763489700 --log-file btc.jsonl
```

**Terminal 3 - Ethereum:**
```bash
bun run start -- markets stream eth-updown-15m-1763489700 --log-file eth.jsonl
```

## Alternative Methods

### Alternative 1: Stream by Event Slug

You can use the human-readable slug instead of the ticker:

```bash
bun run start -- markets stream solana-up-or-down-november-18-115pm-130pm-et --verbose
```

### Alternative 2: Stream by Event ID

You can use the numeric event ID:

```bash
bun run start -- markets stream 83768 --verbose
```

### Alternative 3: Direct Asset ID Streaming

If you already have specific asset IDs, use the lower-level `stream` command:

```bash
# Stream both "Up" and "Down" outcomes for a Solana market
bun run start -- stream 109681959945973826496234384791167033612800000000000000000000000376,52181619848812915160551060468842099699261274828279546744438464278138132224 --verbose
```

This is useful when:
- You have asset IDs from another source
- You want to monitor specific outcomes only (e.g., only "Up")
- You're integrating with external systems

## Command Reference

### Markets Stream Command

```
Usage: polymarket-cli markets stream [options] <id-or-slug>

Stream real-time data for a market by event ID or slug

Arguments:
  id-or-slug             Event ID (numeric) or slug (string)

Options:
  --db-path <path>       Path to SQLite database file (default: "./polymarket.db")
  --event-types <types>  Comma-separated event types to filter
  --log-file <path>      Write stream output to file
  --verbose              Enable verbose logging (default: false)
  --no-ping              Disable automatic ping messages
  -h, --help             display help for command
```

## Implementation Details

### What Changed to Enable This Workflow

The following enhancements were made to support seamless market streaming:

1. **API Types** (`src/api/types.ts`): Added `clobTokenIds?: string` field to `ApiMarket` interface to capture asset IDs from API responses

2. **Database Schema** (`src/db/schema.ts`): Added `clob_token_ids TEXT` column to the `markets` table for persistent storage

3. **Database Types** (`src/db/types.ts`): Added `clob_token_ids: string | null` to the `Market` interface

4. **Markets Repository** (`src/db/markets.ts`): Updated SQL statements and runtime code to handle storing and retrieving asset IDs

5. **Markets Save Command** (`src/commands/markets/save.ts`): Modified to extract `clobTokenIds` from API responses and store them in the database

6. **New Command** (`src/commands/markets/stream.ts`): Created new convenience command that:
   - Accepts event slug or ID as input
   - Queries the database for associated markets
   - Extracts and parses asset IDs
   - Automatically connects to WebSocket stream
   - Handles errors gracefully (missing data, parse failures, etc.)

7. **CLI Index** (`src/index.ts`): Registered `markets stream` as a subcommand under the `markets` command group

### Database Schema

The `markets` table now includes:

```sql
CREATE TABLE markets (
  id TEXT PRIMARY KEY,
  event_id TEXT NOT NULL,
  question TEXT NOT NULL,
  condition_id TEXT,
  slug TEXT NOT NULL,
  ...
  clob_token_ids TEXT,  -- NEW: Stores JSON array of asset IDs
  active INTEGER NOT NULL DEFAULT 1,
  closed INTEGER NOT NULL DEFAULT 0,
  ...
)
```

### Data Flow

```
Polymarket API
      ↓
markets save command
      ↓
Extract clobTokenIds from API response
      ↓
Store in SQLite database
      ↓
markets stream command
      ↓
Query database by event slug/ID
      ↓
Parse JSON array of asset IDs
      ↓
Connect to WebSocket
      ↓
Subscribe with asset IDs
      ↓
Stream real-time data
```

## Key Benefits

- **Seamless Experience**: No manual API lookups or asset ID copy-pasting required
- **Event-Level Streaming**: Stream all outcome markets for an event with a single command
- **Persistent Storage**: Asset IDs stored locally for offline queries and fast lookups
- **Flexible Input**: Accept either human-readable slugs or numeric IDs
- **Automatic Discovery**: CLI handles all the complexity of mapping events to asset IDs
- **Error Handling**: Graceful handling of missing data with helpful error messages

## Troubleshooting

### Error: "Event not found"

The event slug or ID doesn't exist in the local database. Run:
```bash
bun run start -- markets save --tag crypto
```

### Error: "No asset IDs found"

The markets were saved before asset ID support was added, or the API didn't return asset IDs. Re-run:
```bash
bun run start -- markets save --tag crypto --force
```

The `--force` flag will delete and re-fetch the markets with the new schema.

### Error: "Database not initialized"

The database doesn't exist or tables are missing. Run:
```bash
bun run start -- tags save
bun run start -- markets save --tag crypto
```

## Example: Complete Session for Crypto Binary Prediction Markets

Here's a complete session showing the full workflow:

```bash
# Initialize
cd polycli
bun install

# Step 1: Fetch tags
bun run start -- tags save --batch-size 100

# Step 2: Fetch all crypto markets (including 15-minute binary prediction markets)
bun run start -- markets save --tag crypto --batch-size 50

# Step 3: Find all active 15-minute binary prediction markets
bun run start -- markets list --tag crypto --active true --filter updown-15m --limit 20

# Step 4: Find upcoming Solana markets
bun run start -- markets list --tag crypto --active true --filter sol-updown-15m --limit 10

# Step 5: Get detailed info about a specific Solana market
bun run start -- markets get sol-updown-15m-1763489700 --format json

# Step 6: Stream real-time orderbook data for that market
bun run start -- markets stream sol-updown-15m-1763489700 --verbose

# Step 7: Stream with filters and logging
bun run start -- markets stream sol-updown-15m-1763489700 \
  --event-types price_change,last_trade_price \
  --log-file sol_market_1763489700.jsonl
```

## Automated Monitoring Strategy

### Finding the Next Market to Stream

Since new 15-minute markets are created every 15 minutes, you can automate discovery:

```bash
# Find the next active Solana market
NEXT_MARKET=$(bun run start -- markets list --tag crypto --active true --filter sol-updown-15m --format json --limit 1 | jq -r '.[0].ticker')

# Stream that market
bun run start -- markets stream "$NEXT_MARKET" --log-file "data/${NEXT_MARKET}.jsonl" --verbose
```

### Continuous Monitoring Script

Create a script that automatically switches to the next market when the current one resolves:

```bash
#!/bin/bash
# monitor_solana_markets.sh

while true; do
  # Get the next active Solana market
  MARKET=$(bun run start -- markets list --tag crypto --active true \
    --filter sol-updown-15m --format json --limit 1 | jq -r '.[0].ticker')

  if [ -z "$MARKET" ] || [ "$MARKET" = "null" ]; then
    echo "No active markets found. Waiting 60 seconds..."
    sleep 60
    continue
  fi

  echo "Streaming market: $MARKET"

  # Stream until the market closes (typically 15 minutes)
  bun run start -- markets stream "$MARKET" \
    --log-file "data/${MARKET}.jsonl" \
    --event-types book,last_trade_price

  # Market closed, wait briefly before finding next
  echo "Market closed. Finding next market in 10 seconds..."
  sleep 10
done
```

### Monitor All Four Cryptos Simultaneously

```bash
# Create data directory
mkdir -p crypto_streams

# Start four background streams (in separate terminals or tmux/screen)
bun run start -- markets stream $(bun run start -- markets list --filter btc-updown-15m --active true --format json --limit 1 | jq -r '.[0].ticker') --log-file crypto_streams/btc.jsonl &
bun run start -- markets stream $(bun run start -- markets list --filter eth-updown-15m --active true --format json --limit 1 | jq -r '.[0].ticker') --log-file crypto_streams/eth.jsonl &
bun run start -- markets stream $(bun run start -- markets list --filter sol-updown-15m --active true --format json --limit 1 | jq -r '.[0].ticker') --log-file crypto_streams/sol.jsonl &
bun run start -- markets stream $(bun run start -- markets list --filter xrp-updown-15m --active true --format json --limit 1 | jq -r '.[0].ticker') --log-file crypto_streams/xrp.jsonl &
```

## Use Cases for Binary Prediction Markets

### Trading Bot Development
Monitor orderbook changes and execute trades based on:
- Price movement patterns
- Orderbook imbalances (more bids than asks = bullish sentiment)
- Volume spikes

### Sentiment Analysis
Track prediction market odds to gauge crowd sentiment about short-term price movements:
- If "Up" trades at 0.65, the crowd is 65% confident price will rise
- Compare across different cryptos to identify relative strength

### Arbitrage Opportunities
- Compare Polymarket odds with actual crypto price charts
- Identify discrepancies between market sentiment and technical indicators
- Monitor correlation between different crypto markets (BTC vs ETH)

### Market Making
- Stream real-time orderbook depth
- Provide liquidity by placing orders on both "Up" and "Down" sides
- Adjust spreads based on volatility

### Data Collection for Research
- Collect historical orderbook snapshots
- Analyze prediction accuracy over time
- Study market microstructure and liquidity dynamics

## Next Steps

With this workflow, you can:
- Stream real-time orderbook data for crypto binary prediction markets
- Build automated monitoring systems that track new markets every 15 minutes
- Analyze sentiment and trading patterns across BTC, ETH, SOL, and XRP
- Develop trading strategies based on crowd predictions
- Export data for quantitative analysis and backtesting

The Polymarket CLI provides a complete toolkit for crypto binary prediction market analysis, from discovery to real-time monitoring.
