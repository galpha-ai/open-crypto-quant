# Polymarket Subscriber Architecture

## Overview

The Polymarket subscriber (`polymarket-sub`) connects to Polymarket's WebSocket API, processes trade events in real-time, and publishes structured data to Redis queues. It supports two operation modes: manual mode with a static asset list, and discovery mode with automatic market discovery and dynamic subscription management.

This crate is part of the `popeyes-tx-sub` workspace. For workspace-level architecture decisions, see [docs/specs/002-multi-chain-workspace-refactoring/design.md](../specs/002-multi-chain-workspace-refactoring/design.md).

## Crate Structure

```
crates/polymarket-sub/
└── src/
    ├── main.rs                      # Entry point
    ├── app.rs                       # App orchestration
    ├── config.rs                    # Configuration loading
    ├── metrics.rs                   # Prometheus metrics
    ├── ws_client.rs                 # WebSocket client
    ├── parser/                      # Event parsing module
    │   ├── event_parser.rs          # Main event parser
    │   └── price_change.rs          # Price change event parsing
    ├── redis_publisher.rs           # Redis publishing (trades)
    ├── redis_orderbook_publisher.rs # Redis publishing (orderbook)
    ├── api_client.rs                # HTTP client for Gamma API
    ├── market_discovery.rs          # Market discovery service
    ├── market_metadata_cache.rs     # In-memory metadata cache
    ├── market_metadata_loader.rs    # Metadata loader for manual mode
    ├── subscription_manager.rs      # Subscription management
    ├── health_monitor.rs            # Health monitoring
    └── types.rs                     # Data structures
```

## Components

### 1. Main Entry Point (`main.rs`)
- Initializes logging with structured JSON output
- Parses YAML configuration file using clap
- Creates and runs the main App instance

### 2. Configuration (`config.rs`)
The service uses a YAML configuration file with the following structure:
- **Polymarket settings**:
  - WebSocket endpoint URL (wss://)
  - Asset IDs to subscribe to (used when market discovery is disabled)
  - Market discovery configuration (optional)
  - Health monitoring configuration (optional)
- **Redis settings**: Connection URL and queue/stream/pubsub configurations (shared from `common` crate)
  - Separate targets for trade events and orderbook events
- **Metrics settings**: Optional Prometheus metrics port (shared from `common` crate)

**Redis Configuration** (`PolymarketRedisConfig`):
- `url`: Redis connection URL
- `trade_queues`, `trade_streams`, `trade_pubsub`: Trade event targets
- `orderbook_queues`, `orderbook_streams`, `orderbook_pubsub`: Orderbook event targets (optional)

**Market Discovery Configuration** (`MarketDiscoveryConfig`):
- `enabled`: Enable automatic market discovery
- `api_base_url`: Polymarket Gamma API base URL (must be HTTPS)
- `tag_id`: Tag ID to filter markets (21 = Crypto)
- `ticker_patterns`: Ticker patterns to match (e.g., `["btc-updown-15m-", "eth-updown-15m-"]`)
- `discovery_interval_secs`: Discovery interval in seconds (minimum 60)
- `max_subscriptions`: Maximum number of asset IDs to subscribe (1-10000)
- `api_timeout_secs`: HTTP request timeout
- `api_retry_attempts`: Number of retry attempts for failed API requests
- `api_retry_backoff_ms`: Backoff duration between retries

**Health Monitoring Configuration** (`HealthMonitoringConfig`):
- `enabled`: Enable health monitoring
- `check_interval_secs`: Health check interval (minimum 10)
- `min_events_per_minute`: Minimum events per minute threshold
- `event_tracking_window_secs`: Event tracking window for rate calculation

Configuration validation ensures:
- WebSocket endpoint starts with `wss://`
- If discovery disabled: at least one asset ID is configured
- If discovery enabled: at least one ticker pattern is specified
- API base URL starts with `https://` (when discovery enabled)
- Subscription limits and intervals meet minimum requirements
- Metrics port is >= 1024 if specified

### 3. Core Application (`app.rs`)
The `App` struct orchestrates all components and supports two operation modes:

**Manual Mode** (when `market_discovery.enabled = false`):
- Uses static asset list from configuration
- Creates broadcast channels for inter-component communication
- Initializes WebSocket client, event parser, and Redis publisher
- Runs all components concurrently using `tokio::select!`
- Sets up metrics registry and starts metrics server
- Optionally starts health monitor

**Discovery Mode** (when `market_discovery.enabled = true`):
- Creates market discovery service and subscription manager
- Waits for initial market discovery to complete
- Creates WebSocket client with discovered asset IDs
- Dynamically reconnects WebSocket when subscription updates are received
- Coordinates all components in a main loop:
  - Monitors WebSocket client for exits
  - Watches for subscription updates from manager
  - Spawns new WebSocket client on subscription changes
- Manages graceful shutdown of all components

## Data Flow

**Manual Mode:**
```
Polymarket Gamma API (HTTPS)        Polymarket WebSocket API
       |                                   |
       | (Event data by asset)             | (JSON messages)
       v                                   v
MarketMetadataLoader             PolymarketWebSocketClient
       |                                   |
       | (populates cache)                 | (serde_json::Value broadcast)
       v                                   v
MarketMetadataCache <-------------- EventParser (enriches with metadata)
                                           |
                         +-----------------+------------------+------------------+
                         |                 |                                     |
                         | (Polymarket-    | (ParsedPriceChangeEvent)            | (ParsedBookEvent)
                         |  TradeEvent)    |                                     |
                         v                 +----------------+--------------------+
                 RedisTradePublisher                        |
                         |                                  v
                         v                        RedisOrderbookPublisher
                   Redis Trade Targets                      |
                         |                    +-------------+-------------+
                         v                    |                           |
  Event::MarketData::                         v                           v
   PolymarketTrade JSON             OrderbookUpdate JSON       OrderbookSnapshot JSON
```

**Discovery Mode:**
```
Polymarket Gamma API (HTTPS)
       |
       | (Event data with markets)
       v
MarketDiscoveryService
       |
       | (DiscoveredMarket broadcast)
       v
SubscriptionManager --> MarketMetadataCache (populated with outcomes)
       |
       | (SubscriptionUpdate broadcast)
       v
App (reconnects WebSocket) --> PolymarketWebSocketClient
       |
       | (JSON messages)
       v
EventParser (enriches with metadata)
       |
       +---------------------------+---------------------------+
       |                           |                           |
       | (PolymarketTradeEvent)    | (ParsedPriceChangeEvent)  | (ParsedBookEvent)
       v                           |                           |
HealthMonitor (tracks rate)        +-------------+-------------+
       |                                         |
       v                                         v
RedisTradePublisher                    RedisOrderbookPublisher
       |                                         |
       v                           +-------------+-------------+
Event::MarketData::                |                           |
PolymarketTrade JSON               v                           v
                         OrderbookUpdate JSON       OrderbookSnapshot JSON
```

### 4. API Client (`api_client.rs`)

**PolymarketApiClient**
- HTTP client for fetching market data from Polymarket Gamma API
- Built on `reqwest` with rustls for TLS
- Configurable timeout and retry logic with exponential backoff
- Fetches events filtered by tag ID (e.g., 21 for Crypto)
- Fetches detailed market data by slug for outcome mappings
- Supports pagination for large result sets
- Comprehensive error handling for HTTP, timeout, and parsing errors

**DiscoveredMarket**
- Parsed market metadata from API responses
- Contains event ID, ticker, title, end date, condition ID, and asset IDs
- Filters out events without valid end dates or asset IDs
- Parses CLOB token IDs from JSON array format

**DetailedMarket**
- Detailed market response from `/markets/slug/{slug}` endpoint
- Contains outcomes array and CLOB token IDs
- `parse_outcome_mappings()`: Creates asset_id -> outcome (e.g., "Up", "Down") mappings

**API Response Types**
- `ApiEvent`: Event data with ticker, title, end date, and markets
- `ApiMarket`: Market data with condition ID and CLOB token IDs (256-bit integers as decimal strings)

### 5. Market Discovery (`market_discovery.rs`)

**MarketDiscoveryService**
- Periodically fetches active crypto binary markets from Polymarket API
- Filters markets by configured ticker patterns (e.g., "btc-updown-15m-")
- Sorts markets by end date ascending (prioritizes markets resolving soonest)
- Broadcasts discovered markets to subscription manager
- Runs on startup and at configured intervals (minimum 60 seconds)
- Handles API failures gracefully and tracks metrics

**Discovery Process**
1. Fetch all events from API with pagination (batch size: 100)
2. Filter by ticker patterns
3. Convert to `DiscoveredMarket` (validates end date and asset IDs)
4. Sort by end date ascending
5. Broadcast to subscription manager
6. Update metrics

### 6. Market Metadata Cache (`market_metadata_cache.rs`)

**MarketMetadataCache**
- Thread-safe in-memory cache for market metadata
- Uses `Arc<RwLock<HashMap>>` for concurrent access (multiple readers, exclusive writers)
- Keyed by asset_id (256-bit integer as decimal string)
- Stores `PolymarketMarketMetadata`: event_id, ticker, title, end_date, outcome
- Enables fast enrichment of trade events with market context (including Up/Down outcome)
- Supports batch inserts for efficient population

### 7. Market Metadata Loader (`market_metadata_loader.rs`)

**MarketMetadataLoader**
- Fetches and caches market metadata on startup for manual mode
- Queries Polymarket API to find markets matching configured asset IDs
- Fetches detailed market data by slug to get outcome mappings
- Creates asset_id -> metadata mappings with outcomes (e.g., "Up", "Down")
- Populates the cache for trade event enrichment
- Handles pagination for large API responses

**Loading Process**
1. Fetch all events from API with pagination
2. Filter events by asset IDs (only markets user subscribed to)
3. For each market, fetch detailed data by slug
4. Parse outcome mappings (asset_id -> outcome)
5. Batch insert into cache

### 8. Subscription Manager (`subscription_manager.rs`)

**SubscriptionManager**
- Receives market updates from discovery service
- Calculates subscription diffs (additions/removals)
- Applies subscription limits (max_subscriptions config)
- Broadcasts subscription updates to App for WebSocket reconnection
- Tracks subscription state and metrics

**Subscription Management**
- Builds asset set from discovered markets (flattens and deduplicates asset IDs)
- Compares with current subscriptions to detect changes
- Applies max_subscriptions limit by prioritizing markets resolving soonest
- Only triggers reconnection if subscriptions actually changed
- Protects against empty market lists (keeps current subscriptions)

**SubscriptionUpdate Message**
- Contains new asset IDs list and market count
- Triggers App to reconnect WebSocket with updated subscription

### 9. WebSocket Client (`ws_client.rs`)

**PolymarketWebSocketClient**
- Connects to Polymarket CLOB WebSocket API with TLS support
- Subscribes to market channel for specified asset IDs
- Sends periodic ping messages (every 10 seconds) to maintain connection
- Handles text messages, close frames, pong responses, and PONG text messages
- Parses JSON messages and broadcasts to parser
- Tracks WebSocket connection status and events received metrics
- Exits process on connection failures for container restart

**Subscription Format**
- Uses JSON subscription message: `{"assets_ids": [...], "type": "market"}`
- Supports 256-bit asset IDs as decimal strings

### 10. Event Parser (`parser/`)

The parser is organized as a module with specialized sub-parsers:

**EventParser** (`event_parser.rs`)
- Receives raw JSON messages from WebSocket broadcast channel
- Routes messages by `event_type` field to appropriate parser
- Supports `last_trade_price`, `price_change`, and `book` events
- Enriches trade events with market metadata from cache (including outcome)
- Broadcasts parsed events to separate channels (trades, price changes, and book snapshots)
- Tracks parsing errors by event type in metrics

**Trade Event Parsing** (`last_trade_price`)
- Extracts required fields: `asset_id`, `market`, `price`, `size`, `side`, `timestamp`, `fee_rate_bps`
- Validates price is in range [0.0, 1.0] (probability)
- Converts side string ("BUY"/"SELL") to `TradeSide` enum
- Looks up market metadata from cache using asset_id
- Attaches metadata (ticker, title, outcome) to trade event
- Handles both string and numeric formats for timestamp and fee_rate_bps

**Price Change Parsing** (`price_change.rs`)
- Parses orderbook update events from WebSocket
- Extracts: `market`, `timestamp`, and `price_changes` array
- Each price change contains: `asset_id`, `price`, `size`, `side`, `hash`, `best_bid`, `best_ask`
- Validates price and best_bid/best_ask are in [0.0, 1.0] range
- Validates size is non-negative
- Returns `ParsedPriceChangeEvent` with multiple `ParsedPriceChange` entries

**Book Event Parsing** (`book`)
- Parses full orderbook snapshot events from WebSocket
- Extracts: `asset_id`, `market`, `timestamp`, `hash`, `bids`, and `asks` arrays
- Each bid/ask entry contains: `price` and `size`
- Returns `ParsedBookEvent` with complete orderbook state

### 11. Redis Integration

**RedisTradePublisher** (`redis_publisher.rs`)
- Receives parsed `PolymarketTradeEvent` from broadcast channel
- Wraps in `Event::MarketData(MarketDataEvent::PolymarketTrade)` for publishing
- Uses Redis publishers from `common` crate (List, Pubsub, Stream)
- Publishes to multiple Redis targets concurrently
- Tracks trade latency from event timestamp to publish time
- Implements event buffering with overflow protection

**RedisOrderbookPublisher** (`redis_orderbook_publisher.rs`)
- Receives parsed events from two broadcast channels:
  - `ParsedPriceChangeEvent` for incremental orderbook updates
  - `ParsedBookEvent` for full orderbook snapshots
- Wraps events for publishing:
  - Price changes → `Event::MarketData(MarketDataEvent::OrderbookUpdate)`
  - Book snapshots → `Event::MarketData(MarketDataEvent::OrderbookSnapshot)`
- Publishes individual price changes to separate Redis targets
- Tracks orderbook latency, removals (size=0), and best bid/ask prices
- Uses separate configuration: `orderbook_queues`, `orderbook_streams`, `orderbook_pubsub`
- Implements same buffering strategy as trade publisher

**Publishing Logic**
- Wraps `PolymarketTradeEvent` in `Event::MarketData(MarketDataEvent::PolymarketTrade)`
- Attempts to publish to all configured Redis targets
- Succeeds if at least one publisher succeeds
- Buffers events only when all publishers fail

**Buffer and Rate Limiting**
- Buffer size: 1000 events (BUFFER_MAX_SIZE)
- Flush rate limit: 100 events/second (FLUSH_RATE_LIMIT)
- Periodic flush interval: 1 second (FLUSH_INTERVAL_MS)
- Drops oldest events on buffer overflow
- Retries buffered events when Redis becomes available
- Attempts to flush buffer before publishing new events

**Error Handling**
- Continues if at least one publisher succeeds
- Buffers events when all publishers fail
- Flushes buffer on graceful shutdown
- Tracks Redis publish failures by queue/stream/channel
- Re-buffers events if flush fails

### 12. Health Monitor (`health_monitor.rs`)

**HealthMonitor**
- Tracks event rate over a rolling window to detect silent rate limiting
- Monitors trade events and exits process if rate drops below threshold
- Designed to trigger container orchestrator restart on unhealthy state
- Shares subscription state with SubscriptionManager (in discovery mode)

**Health Monitoring Process**
- Spawns two tasks:
  1. **Event recording task**: Subscribes to trade events and records in tracker
  2. **Health check task**: Periodically checks event rate and exits if unhealthy
- Grace period: 60 seconds after startup (no health checks enforced)
- Only enforces checks when subscriptions > 0

**EventRateTracker**
- Tracks events per second in a ring buffer (HashMap keyed by timestamp)
- Calculates rolling event rate over last 60 seconds
- Periodically prunes old entries to prevent unbounded growth
- Updates metrics with current event rate and last event timestamp

**Exit Behavior**
- Exits with code 1 if event rate drops below `min_events_per_minute` threshold
- Logs detailed exit message with current rate, subscriptions, and threshold
- Increments health check failure metric before exit
- Container orchestrator (e.g., Docker, Kubernetes) should restart the process

### 13. Data Types (`types.rs`)

**MarketMetadata**
- Event ID, ticker, end date, asset IDs
- Discovery timestamp

**SubscriptionState**
- Current asset IDs subscribed to WebSocket
- Last updated timestamp
- Total markets count
- Methods for calculating diffs and updating state

**EventRateTracker**
- Events per second ring buffer
- Total events counter
- Rate calculation and pruning methods

**ParsedPriceChangeEvent**
- Parsed price change event for orderbook updates
- Contains: market, timestamp, and array of `ParsedPriceChange`

**ParsedPriceChange**
- Individual price level change in orderbook
- Contains: asset_id, price, size, side, hash, best_bid, best_ask
- Uses numeric types for performance

**ParsedBookEvent**
- Parsed book event for full orderbook snapshots
- Contains: asset_id, market, timestamp, hash, bids, and asks arrays
- Each bid/ask is a `ParsedBookLevel` with price and size

### 14. Metrics (`metrics.rs`)
- Prometheus-compatible metrics exposed on configurable port
- Uses shared metrics server from `common` crate
- Polymarket-specific metrics:

**Trade Metrics**
  - `ws_events_received`: Total WebSocket events by event type
  - `trades_published`: Trades published by market and side
  - `parsing_errors`: Parsing errors by event type
  - `redis_publish_failures`: Redis publish failures by queue/stream/channel
  - `ws_connection_status`: Connection status gauge (1=connected, 0=disconnected)
  - `trade_latency`: Histogram of trade latency from timestamp to publish

**Market Discovery Metrics**
  - `markets_discovered`: Number of active crypto binary markets discovered
  - `api_fetch_failures`: Total API fetch failures by error type
  - `assets_subscribed`: Current number of asset IDs subscribed to WebSocket
  - `subscription_updates`: Total subscription update operations (added/removed)
  - `markets_dropped`: Total markets dropped due to subscription limit

**Health Monitoring Metrics**
  - `events_per_minute`: Rolling event rate (events/minute over last 60 seconds)
  - `health_check_failures`: Total health check failures leading to process exit
  - `last_event_timestamp`: Unix timestamp of last received event

**Orderbook Metrics**
  - `price_changes_received`: Total price_change events received from WebSocket
  - `price_changes_published`: Total individual price changes published to Redis
  - `orderbook_removals`: Total orderbook removals (size=0)
  - `best_bid`: Current best bid price (gauge by market and asset_id)
  - `best_ask`: Current best ask price (gauge by market and asset_id)
  - `orderbook_latency`: Histogram of orderbook update latency
  - `book_snapshots_received`: Total book snapshot events received from WebSocket
  - `book_snapshots_published`: Total book snapshots published to Redis
  - `book_snapshot_latency`: Histogram of book snapshot latency

## Key Design Patterns

1. **Dual Operation Modes**: Manual mode for static configs, Discovery mode for dynamic subscriptions
2. **Broadcast Channels**: Used for efficient multi-consumer data distribution
3. **Concurrent Processing**: All components run as independent async tasks
4. **Error Resilience**: Components handle lagged receivers and continue processing
5. **Event Buffering**: Handles Redis unavailability gracefully
6. **Structured Logging**: JSON-formatted logs with contextual information

## Performance Considerations

- **Market Metadata Caching**: Caches market metadata with outcomes for fast trade enrichment
- **Event Buffering**: Buffers events during Redis unavailability
- **Rate Limiting**: Controlled buffer flush rate to prevent overwhelming Redis
- **Connection Pooling**: Redis `ConnectionManager` for efficient connections
- **Graceful Degradation**: Handles broadcast channel lag gracefully
