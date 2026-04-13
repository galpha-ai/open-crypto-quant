# Crypto Binary Prediction Markets on Polymarket

## Overview

Polymarket offers 15-minute binary prediction markets for major cryptocurrencies. These markets allow users to predict whether a cryptocurrency's price will go "Up" or "Down" during a specific 15-minute time window.

**Supported Cryptocurrencies:**
- Bitcoin (BTC) - ~97 markets
- Ethereum (ETH) - ~97 markets
- Solana (SOL) - ~97 markets
- XRP (XRP) - ~98 markets

All markets use Chainlink price feeds as their resolution source for objective, verifiable price data.

## Market Structure

### Event and Market Relationship

Each 15-minute prediction window consists of:
- **1 Event**: Container with metadata, timing, and resolution rules
- **1 Market**: Trading venue with condition ID, outcomes, and prices

```
Event (e.g., "Solana Up or Down - November 18, 1:15PM-1:30PM ET")
  └── Market (with condition_id, outcomes: ["Up", "Down"])
```

### Market Lifecycle

**Key Finding: Each 15-minute interval gets a NEW market**

Markets are NOT reused. For example:
- 1:15-1:30 PM ET market → unique IDs, resolves at 1:30 PM
- 1:30-1:45 PM ET market → different unique IDs, resolves at 1:45 PM
- Pattern continues every 15 minutes

**Market Creation Pattern:**
- New markets are created approximately every 15 minutes
- Each market has unique `event_id`, `market_id`, and `condition_id`
- Markets are created ~15 minutes before their start time
- After resolution, markets are marked as `closed = 1` but remain in the database

## Database Schema

The Polymarket CLI stores data in three main tables:

### `events` Table
Contains high-level event information:
```sql
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    ticker TEXT NOT NULL,              -- e.g., "sol-updown-15m-1763489700"
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,               -- e.g., "Solana Up or Down - November 18, 1:15PM-1:30PM ET"
    description TEXT,
    resolution_source TEXT,            -- Chainlink data stream URL
    start_date TEXT,
    creation_date TEXT,
    end_date TEXT,                     -- When market resolves
    active INTEGER NOT NULL DEFAULT 1,
    closed INTEGER NOT NULL DEFAULT 0,
    -- ... other fields
);
```

### `markets` Table
Contains trading-specific information:
```sql
CREATE TABLE markets (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    question TEXT NOT NULL,
    condition_id TEXT,                  -- Smart contract condition ID (crucial for trading)
    slug TEXT NOT NULL,
    start_date TEXT,
    end_date TEXT,
    outcomes TEXT,                      -- JSON: ["Up", "Down"]
    outcome_prices TEXT,                -- JSON: ["0.5", "0.5"]
    active INTEGER NOT NULL DEFAULT 1,
    closed INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(event_id) REFERENCES events(id)
);
```

### `event_tags` Table
Links events to categories:
```sql
CREATE TABLE event_tags (
    event_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    tag_label TEXT,                     -- "Crypto" for all crypto markets
    PRIMARY KEY (event_id, tag_id),
    FOREIGN KEY(event_id) REFERENCES events(id),
    FOREIGN KEY(tag_id) REFERENCES tags(id)
);
```

## Ticker Naming Convention

Binary prediction market tickers follow a consistent pattern:

```
{crypto_symbol}-updown-15m-{unix_timestamp}
```

**Examples:**
- `sol-updown-15m-1763489700` - Solana market ending at Unix timestamp 1763489700
- `btc-updown-15m-1763489700` - Bitcoin market ending at same time
- `eth-updown-15m-1763489700` - Ethereum market ending at same time
- `xrp-updown-15m-1763489700` - XRP market ending at same time

The Unix timestamp represents the **end time** of the 15-minute prediction window.

## Resolution Rules

All crypto binary prediction markets follow the same resolution logic:

**"Up" Resolution:**
- Price at end of time range ≥ Price at beginning of time range

**"Down" Resolution:**
- Price at end of time range < Price at beginning of time range

**Resolution Sources:**
All markets use Chainlink price feeds (not spot exchange prices):
- Bitcoin: https://data.chain.link/streams/btc-usd
- Ethereum: https://data.chain.link/streams/eth-usd
- Solana: https://data.chain.link/streams/sol-usd
- XRP: https://data.chain.link/streams/xrp-usd

## Querying Market Data

### Example: Solana Market Details

The Solana market for "November 18, 1:15PM-1:30PM ET":

```sql
SELECT
    e.id,
    e.ticker,
    e.title,
    e.description,
    e.resolution_source,
    e.end_date,
    e.active,
    e.closed,
    m.id as market_id,
    m.condition_id,
    m.outcomes,
    m.outcome_prices
FROM events e
JOIN markets m ON e.id = m.event_id
WHERE e.ticker = 'sol-updown-15m-1763489700';
```

**Result:**
```
Event ID:       83768
Ticker:         sol-updown-15m-1763489700
Title:          Solana Up or Down - November 18, 1:15PM-1:30PM ET
Market ID:      687550
Condition ID:   0x22a74a25eaf9addf08e35530a2c37f85794e5a0ae6df7524b89a58d9ad497568
End Date:       2025-11-18T18:30:00Z
Outcomes:       ["Up", "Down"]
Prices:         ["0.5", "0.5"]
Resolution:     https://data.chain.link/streams/sol-usd
```

### Find All Active 15-Minute Markets for Any Crypto

```sql
SELECT
    e.ticker,
    e.title,
    e.end_date,
    m.condition_id,
    CASE
        WHEN e.ticker LIKE 'btc-%' THEN 'Bitcoin'
        WHEN e.ticker LIKE 'eth-%' THEN 'Ethereum'
        WHEN e.ticker LIKE 'sol-%' THEN 'Solana'
        WHEN e.ticker LIKE 'xrp-%' THEN 'XRP'
    END as crypto
FROM events e
JOIN markets m ON e.id = m.event_id
WHERE e.ticker LIKE '%-updown-15m-%'
  AND e.active = 1
  AND e.closed = 0
ORDER BY e.end_date ASC;
```

### Find Upcoming Markets (Next 2 Hours)

```sql
SELECT
    e.ticker,
    e.title,
    e.end_date,
    m.condition_id,
    m.outcomes
FROM events e
JOIN markets m ON e.id = m.event_id
WHERE e.ticker LIKE '%-updown-15m-%'
  AND e.active = 1
  AND e.closed = 0
  AND e.end_date > datetime('now')
  AND e.end_date < datetime('now', '+2 hours')
ORDER BY e.end_date ASC;
```

### Find Newly Created Markets (Last Hour)

Use this query to monitor for new markets being created:

```sql
SELECT
    e.id,
    e.ticker,
    e.title,
    e.created_at,
    e.end_date,
    m.id as market_id,
    m.condition_id
FROM events e
JOIN markets m ON e.id = m.event_id
WHERE e.ticker LIKE '%-updown-15m-%'
  AND e.created_at > datetime('now', '-1 hour')
  AND e.active = 1
ORDER BY e.created_at DESC;
```

### Find Markets by Specific Crypto

**Solana Only:**
```sql
SELECT
    e.ticker,
    e.title,
    e.end_date,
    m.condition_id
FROM events e
JOIN markets m ON e.id = m.event_id
WHERE e.ticker LIKE 'sol-updown-15m-%'
  AND e.active = 1
  AND e.closed = 0
ORDER BY e.end_date ASC
LIMIT 10;
```

**Bitcoin Only:**
```sql
WHERE e.ticker LIKE 'btc-updown-15m-%'
```

**Ethereum Only:**
```sql
WHERE e.ticker LIKE 'eth-updown-15m-%'
```

**XRP Only:**
```sql
WHERE e.ticker LIKE 'xrp-updown-15m-%'
```

### Find Markets Using Crypto Tag

All crypto markets are tagged with tag_id = '21' (label: "Crypto"):

```sql
SELECT
    e.ticker,
    e.title,
    t.label as tag
FROM events e
JOIN event_tags et ON e.id = et.event_id
JOIN tags t ON et.tag_id = t.id
WHERE et.tag_id = '21'
  AND e.ticker LIKE '%-updown-15m-%'
  AND e.active = 1
ORDER BY e.created_at DESC
LIMIT 20;
```

## Programmatic Monitoring Strategy

### Real-Time Market Discovery

To build a system that monitors for new crypto binary prediction markets:

1. **Poll Database Every 5-10 Minutes:**
   ```sql
   SELECT e.id, e.ticker, e.title, e.end_date, m.condition_id
   FROM events e
   JOIN markets m ON e.id = m.event_id
   WHERE e.ticker LIKE '%-updown-15m-%'
     AND e.created_at > ?  -- Your last check timestamp
   ORDER BY e.created_at DESC;
   ```

2. **Extract Key Trading Information:**
   - `condition_id` - Required for interacting with Polymarket smart contracts
   - `end_date` - When the market will resolve
   - `ticker` - Parse crypto symbol and timestamp
   - `outcomes` - Always `["Up", "Down"]` for these markets

3. **Track Market States:**
   - Active markets: `active = 1 AND closed = 0`
   - Resolved markets: `closed = 1`
   - Markets about to resolve: `end_date < datetime('now', '+30 minutes')`

### Example: Monitor Solana Markets

```python
# Pseudocode for monitoring Solana markets
import sqlite3
from datetime import datetime, timedelta

def get_upcoming_solana_markets(db_path, hours_ahead=2):
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    query = """
    SELECT
        e.id,
        e.ticker,
        e.title,
        e.end_date,
        m.condition_id,
        m.outcomes,
        m.outcome_prices
    FROM events e
    JOIN markets m ON e.id = m.event_id
    WHERE e.ticker LIKE 'sol-updown-15m-%'
      AND e.active = 1
      AND e.closed = 0
      AND e.end_date > datetime('now')
      AND e.end_date < datetime('now', '+{} hours')
    ORDER BY e.end_date ASC
    """.format(hours_ahead)

    cursor.execute(query)
    return cursor.fetchall()

# Use this to get markets and trade on them
markets = get_upcoming_solana_markets('polymarket.db')
for market in markets:
    event_id, ticker, title, end_date, condition_id, outcomes, prices = market
    print(f"Market: {title}")
    print(f"Condition ID: {condition_id}")
    print(f"Resolves at: {end_date}")
```

## Market Lifecycle Example

Let's trace a Solana market through its lifecycle:

### 1. Market Creation (Before 1:15 PM)
```
Created At:  2025-11-17T18:17:25Z
Start Date:  2025-11-17T18:22:27Z (1:15 PM ET)
End Date:    2025-11-18T18:30:00Z (1:30 PM ET)
Status:      active=1, closed=0
```

### 2. Trading Period (1:15 PM - 1:30 PM)
Users trade on "Up" vs "Down" outcomes based on their price predictions.

### 3. Market Resolution (At 1:30 PM)
- Chainlink price at 1:15 PM compared to price at 1:30 PM
- Market resolves to "Up" or "Down"
- Status changed to: `active=1, closed=1`

### 4. Next Market (1:30 PM - 1:45 PM)
A completely NEW market is created with:
- New event_id (e.g., 83799)
- New market_id (e.g., 687645)
- New condition_id (e.g., 0xf2928dcc241d0e68a841cccaca1b9890fb8c7aaf4a00600d16a3805ebfeec828)
- Different ticker: `sol-updown-15m-1763490600`

## Important Notes

1. **Market Uniqueness**: Every 15-minute window has a unique market. Never assume markets are reused.

2. **Condition ID**: The `condition_id` is the critical identifier for programmatic trading on Polymarket. It's used in smart contract interactions.

3. **Timestamp in Ticker**: The Unix timestamp in the ticker represents the END time of the prediction window, not the start time.

4. **Resolution Source**: All markets use Chainlink data streams, NOT spot exchange prices. This is important for understanding final resolutions.

5. **Market Creation Timing**: Markets are typically created ~15 minutes before their start time, giving traders time to discover and participate.

6. **Cross-Crypto Synchronization**: For the same time window (e.g., 1:15-1:30 PM), markets for different cryptos (BTC, ETH, SOL, XRP) share the same timestamp in their tickers but have different event/market IDs.

## Use Cases

### Trading Bot
Monitor upcoming markets, analyze price trends, and place automated trades using the `condition_id`.

### Market Analytics
Track historical market outcomes, price movements, and prediction accuracy across different cryptocurrencies.

### Arbitrage Detection
Compare Polymarket odds with actual price movement probabilities from other sources.

### Notification System
Alert users when new markets are created for their favorite cryptocurrencies.

## Related Resources

- [Polymarket API Documentation](https://docs.polymarket.com/)
- [Chainlink Data Streams](https://data.chain.link/)
- Polymarket CLI: `polycli/` directory in this repository
