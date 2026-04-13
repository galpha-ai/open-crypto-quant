# Automated Crypto Binary Prediction Market Subscription

## Summary

This design enables the Polymarket subscriber to automatically discover and subscribe to crypto binary prediction markets (15-minute BTC/ETH/SOL/XRP "Up or Down" markets) without manual configuration. The system periodically fetches active markets from the Polymarket API, extracts asset IDs, dynamically manages WebSocket subscriptions, and implements safety mechanisms to detect silent rate limiting. This eliminates the need for manual asset ID configuration and ensures continuous coverage of new markets as they are created every 15 minutes.

## Goals

- Automatically discover active crypto binary prediction markets using Polymarket Gamma API
- Dynamically manage WebSocket subscriptions to track current markets without service restarts
- Prune closed/expired markets to keep subscription count bounded
- Implement health monitoring to detect silent rate limiting (exit for orchestrator restart)
- Support configurable subscription limits to avoid API throttling
- Maintain observability into market discovery, subscription changes, and health status
- Enable deployment in Kubernetes with automatic restart on rate limit detection

## Non-Goals

- Building a local database/cache of historical market data (use polycli for this)
- Supporting non-crypto prediction markets (can be added later if needed)
- Implementing custom retry logic for individual market API failures (rely on periodic refresh)
- Providing UI/dashboard for subscription management (use Prometheus metrics and logs)
- Trading or order placement functionality (out of scope for subscriber)

## API

This feature does not expose new external APIs. It enhances the existing `polymarket-sub` service with internal components for market discovery and subscription management.

### Polymarket Gamma API Integration

The service will call the following Polymarket API endpoints:

**Endpoint**: `GET https://gamma-api.polymarket.com/events`

**Query Parameters**:
- `tag`: `21` (Crypto tag ID)
- `active`: `true` (only active markets)
- `limit`: `100` (batch size)
- `offset`: `{offset}` (pagination)
- `closed`: `false` (exclude closed markets)

**Response**: Array of event objects with embedded markets containing `clobTokenIds`

**Example Response Structure**:
```json
[
  {
    "id": "83768",
    "ticker": "sol-updown-15m-1763489700",
    "slug": "solana-up-or-down-november-18-115pm-130pm-et",
    "title": "Solana Up or Down - November 18, 1:15PM-1:30PM ET",
    "end_date": "2025-11-18T18:30:00Z",
    "active": true,
    "markets": [
      {
        "id": "687550",
        "question": "Solana Up or Down - November 18, 1:15PM-1:30PM ET",
        "clob_token_ids": "[\"109681959945973826496234384791167033612800000000000000000000000376\", \"52181619848812915160551060468842099699261274828279546744438464278138132224\"]"
      }
    ]
  }
]
```

## Behavior

### Application Startup Flow

On service start:

1. **Initialize HTTP client** for Polymarket Gamma API with timeout (30s) and retry configuration (3 attempts, 1s backoff)
2. **Fetch initial market data**:
   - Call `/events` API with `tag=21`, `active=true`, `closed=false`
   - Paginate through all results (100 events per batch)
   - Filter events matching crypto binary patterns: `{btc,eth,sol,xrp}-updown-15m-*`
   - Extract asset IDs from `markets[].clob_token_ids` JSON array
   - Build initial subscription list (deduplicated asset IDs)
3. **Validate subscription count**:
   - If asset count > `max_subscriptions` config, log error and apply prioritization:
     - Sort by end_date ascending (prioritize markets resolving soonest)
     - Take first `max_subscriptions` assets
   - Log warning if markets were dropped
4. **Connect to WebSocket** with subscription list:
   - Send subscription message: `{"assets_ids": [...], "type": "market"}`
   - Start ping loop (every 10 seconds)
5. **Start background tasks**:
   - Market discovery task (periodic refresh)
   - Health monitoring task (event rate tracking)

### Market Discovery Task (Periodic Refresh)

Runs every `discovery_interval` (default: 5 minutes):

1. **Fetch latest market data** using same API call as startup
2. **Compare with current subscriptions**:
   - New assets: In fetched list but not currently subscribed
   - Removed assets: Currently subscribed but not in fetched list (market closed/expired)
   - Unchanged assets: In both lists
3. **Apply subscription limit**:
   - If `(current_count - removed_count + new_count) > max_subscriptions`:
     - Sort new + unchanged by end_date ascending
     - Take first `max_subscriptions` assets
4. **Update WebSocket subscription**:
   - Currently, WebSocket subscription is set on connection and cannot be changed
   - **Phase 1 (MVP)**: If subscription list changes, close current connection and reconnect with new list
   - **Future enhancement**: Investigate incremental subscription updates if API supports it
5. **Log subscription changes**:
   - Log added assets (count and sample IDs)
   - Log removed assets (count and sample IDs)
   - Update `assets_subscribed` metric

### Health Monitoring Task

Runs every `health_check_interval` (default: 60 seconds):

1. **Calculate event rate** over rolling window (last 60 seconds):
   - Track total events received via counter
   - Compare current counter with value from 60 seconds ago
2. **Check health threshold**:
   - If event rate < `min_events_per_minute` threshold (default: 1) AND subscriptions > 0:
     - Log critical error: "Event rate dropped to {rate}/min, possible silent rate limiting detected"
     - Increment `health_check_failures` metric
     - Exit process with exit code 1 (triggers orchestrator restart)
3. **Update metrics**:
   - `events_per_minute` gauge
   - `last_event_timestamp` gauge

### WebSocket Event Processing

No changes to existing event processing pipeline:
- WebSocketClient receives messages
- EventParser parses trade events
- RedisTradePublisher publishes to Redis

The existing metrics (`ws_events_received`) are used by health monitoring.

### Error Handling

**API Fetch Failures**:
- Retry up to 3 times with 1-second backoff
- If all retries fail:
  - Log error with details
  - Increment `api_fetch_failures` metric
  - Continue with current subscription list (stale data)
  - Retry on next discovery interval

**WebSocket Disconnection**:
- Existing reconnection logic applies
- On reconnect, use latest subscription list from market discovery

**Empty Market List**:
- If API returns 0 markets matching filter:
  - Log warning (may indicate API issue or all markets closed)
  - Keep current subscriptions (don't unsubscribe everything)
  - Retry on next discovery interval

**Rate Limit Detection**:
- Health monitor detects silent rate limiting
- Exit process immediately for orchestrator restart
- New instance starts with fresh subscription list

## Data Model

### MarketMetadata (In-Memory)

Represents a crypto binary prediction market discovered from API.

**Purpose**: Track active markets and their asset IDs for subscription management

**Fields**:
- `event_id`: String - Polymarket event ID
- `ticker`: String - Market ticker (e.g., "sol-updown-15m-1763489700")
- `end_date`: DateTime - Market resolution time
- `asset_ids`: Vec<String> - CLOB token IDs for this market (typically 2: "Up" and "Down")
- `discovered_at`: DateTime - When this market was first discovered

**Storage**: In-memory HashMap keyed by `event_id`, no persistence required

**Lifecycle**: Removed when market end_date passes and no longer returned by API

### SubscriptionState (In-Memory)

Tracks current WebSocket subscription state.

**Purpose**: Manage the set of asset IDs currently subscribed to

**Fields**:
- `current_assets`: HashSet<String> - Asset IDs currently subscribed
- `last_updated`: DateTime - When subscription list was last refreshed
- `total_markets`: usize - Number of markets represented

**Storage**: In-memory, protected by mutex for concurrent access

**Operations**:
- `calculate_diff(new_assets)` -> (additions, removals)
- `update(new_assets)` - Replace current set
- `len()` - Current subscription count

### EventRateTracker (In-Memory)

Tracks rolling event rate for health monitoring.

**Purpose**: Detect silent rate limiting by monitoring event flow

**Fields**:
- `events_per_second`: Vec<(DateTime, u64)> - Ring buffer of event counts per second (last 120 seconds)
- `total_events`: u64 - Cumulative event counter

**Operations**:
- `record_event()` - Increment counter for current second
- `get_rate_per_minute()` - Sum events in last 60 seconds
- `prune_old()` - Remove entries older than 120 seconds

## Configuration

### New Configuration Fields

Add to `PolymarketConfig` struct in `crates/polymarket-sub/src/config.rs`:

```yaml
polymarket:
  wss_endpoint: "wss://ws-subscriptions-clob.polymarket.com/ws/market"

  # NEW: Market discovery settings
  market_discovery:
    enabled: true                        # Enable automatic market discovery (set false to use manual assets list)
    api_base_url: "https://gamma-api.polymarket.com"
    tag_id: 21                           # Crypto tag ID
    ticker_patterns:                     # Filter patterns for market tickers
      - "btc-updown-15m-"
      - "eth-updown-15m-"
      - "sol-updown-15m-"
      - "xrp-updown-15m-"
    discovery_interval_secs: 300         # Fetch markets every 5 minutes
    max_subscriptions: 500               # Maximum asset IDs to subscribe (safety limit)
    api_timeout_secs: 30                 # HTTP request timeout
    api_retry_attempts: 3                # Number of retries for failed requests
    api_retry_backoff_ms: 1000          # Backoff between retries

  # NEW: Health monitoring settings
  health_monitoring:
    enabled: true
    check_interval_secs: 60              # Check health every 60 seconds
    min_events_per_minute: 1             # Minimum event rate (exit if below)
    event_tracking_window_secs: 120      # Track event rate over last 2 minutes

  # EXISTING: Manual asset list (used if market_discovery.enabled = false)
  assets:
    - "109681959945973300464568698402968596289258214226684818748321941747028805721376"

redis:
  url: "redis://redis:6379"
  queues:
    - name: "polymarket:trades"
      max_length: 10000

metrics:
  port: 9092
```

### Configuration Validation

On load, validate:
- `market_discovery.enabled` XOR `assets` list must be non-empty
- `market_discovery.api_base_url` must be valid HTTPS URL
- `market_discovery.max_subscriptions` must be > 0 and <= 10000
- `market_discovery.discovery_interval_secs` must be >= 60 (avoid API abuse)
- `health_monitoring.min_events_per_minute` must be >= 0
- `health_monitoring.check_interval_secs` must be >= 10
- At least one ticker pattern must be specified

## Market Discovery Service

New component: `MarketDiscoveryService` in `crates/polymarket-sub/src/market_discovery.rs`

### Responsibilities

1. **Fetch markets from Polymarket API**:
   - Paginate through all active crypto markets
   - Parse event and market JSON responses
   - Extract and parse `clob_token_ids` JSON arrays
2. **Filter by ticker patterns**:
   - Match event ticker against configured patterns
   - Only include active, non-closed markets
3. **Build asset ID list**:
   - Flatten all asset IDs from matching markets
   - Deduplicate (same asset may appear in multiple markets)
   - Sort by market end_date for prioritization
4. **Broadcast updates**:
   - Send new asset list via tokio broadcast channel to subscription manager

### API Client Implementation

Use `reqwest` HTTP client with:
- Connection pooling (reuse connections)
- Timeout configuration (30s default)
- Retry middleware (exponential backoff)
- JSON deserialization with serde

### Data Structures

```rust
#[derive(Debug, Deserialize)]
struct ApiEvent {
    id: String,
    ticker: String,
    end_date: String,
    active: bool,
    markets: Vec<ApiMarket>,
}

#[derive(Debug, Deserialize)]
struct ApiMarket {
    id: String,
    #[serde(rename = "clobTokenIds")]
    clob_token_ids: Option<String>,  // JSON array string
}

#[derive(Debug, Clone)]
struct DiscoveredMarket {
    event_id: String,
    ticker: String,
    end_date: DateTime<Utc>,
    asset_ids: Vec<String>,
}
```

### Error Handling

- **HTTP errors** (4xx, 5xx): Log error, retry with backoff, fail after max attempts
- **JSON parse errors**: Log malformed response, skip to next page
- **Missing clob_token_ids**: Log warning (some markets may not have IDs yet), skip market
- **Invalid end_date format**: Log error, skip market
- **Network timeouts**: Retry with backoff

## Subscription Manager

New component: `SubscriptionManager` in `crates/polymarket-sub/src/subscription_manager.rs`

### Responsibilities

1. **Receive market updates** from MarketDiscoveryService
2. **Calculate subscription diff**:
   - Compare new asset list with current subscriptions
   - Identify additions and removals
3. **Apply subscription limit**:
   - If total > max_subscriptions, prioritize by end_date
   - Log warning about dropped markets
4. **Trigger WebSocket reconnection**:
   - Send shutdown signal to current WebSocket client
   - Wait for disconnection
   - Start new WebSocket client with updated subscription list
5. **Update metrics**:
   - `assets_subscribed` gauge
   - `subscription_updates_total` counter (with labels: added, removed)
   - `markets_discovered` gauge

### State Management

- Maintain `Arc<Mutex<SubscriptionState>>` for thread-safe access
- Use tokio broadcast channel for coordination with WebSocket client
- Track last update timestamp to detect stale data

### Reconnection Strategy

**Phase 1 (MVP)**:
1. Send shutdown signal to WebSocket client task
2. WebSocket client gracefully closes connection
3. Wait for confirmation via channel
4. Spawn new WebSocket client task with updated asset list
5. New client connects and subscribes

**Trade-offs**:
- Brief interruption in data stream (acceptable for 5-minute refresh interval)
- Simpler implementation (no WebSocket protocol extensions needed)
- Clean state reset (no risk of subscription state drift)

**Future Enhancement**:
- Investigate if Polymarket WebSocket supports incremental subscription updates
- Implement add/remove subscription messages if supported
- Eliminate connection interruption

## Health Monitor

New component: `HealthMonitor` in `crates/polymarket-sub/src/health_monitor.rs`

### Responsibilities

1. **Track event rate**:
   - Subscribe to EventParser output channel (existing)
   - Increment counter for each received event
   - Maintain ring buffer of events per second
2. **Periodic health checks**:
   - Calculate events per minute over rolling window
   - Compare against threshold
3. **Trigger shutdown** if unhealthy:
   - Log critical error with context
   - Exit process with code 1
   - Kubernetes/Docker will restart container

### Event Rate Calculation

Use a ring buffer approach:
- Store event count for each second (index = unix_timestamp % 120)
- To get events/minute: sum counts for last 60 seconds
- Prune entries older than 120 seconds

### Health Check Logic

```rust
if subscription_count > 0 && events_per_minute < config.min_events_per_minute {
    error!(
        events_per_minute = events_per_minute,
        subscriptions = subscription_count,
        threshold = config.min_events_per_minute,
        "Event rate dropped below threshold, possible silent rate limiting detected"
    );
    std::process::exit(1);  // Trigger orchestrator restart
}
```

### Grace Period

- Don't enforce health checks during first 60 seconds after startup (allow time for initial events)
- Don't enforce if `subscription_count == 0` (no markets to stream)

## Idempotency & Concurrency

### Market Discovery
- Runs on fixed interval timer (tokio::time::interval)
- No concurrency issues (single task)
- API calls are idempotent (GET requests)

### Subscription Updates
- Controlled by SubscriptionManager via mutex-protected state
- Sequential processing: one update completes before next starts
- WebSocket reconnection is sequential (shutdown, wait, reconnect)

### Event Processing
- Existing pipeline remains unchanged
- EventParser -> RedisTradePublisher (existing broadcast channels)
- HealthMonitor observes events via clone of parser output channel

### Shutdown Coordination
- Use tokio CancellationToken for graceful shutdown
- All tasks check token periodically
- WebSocket client flushes buffer before exit
- SubscriptionManager logs final state

## Observability

### Logs

**Market Discovery**:
- INFO: "Discovered {count} active crypto binary markets (added: {added}, removed: {removed})"
- WARN: "Market API fetch failed (attempt {n}/{max}): {error}"
- ERROR: "Failed to fetch markets after {max} retries, using stale subscription list"
- WARN: "Subscription limit reached ({current}/{max}), dropped {dropped} markets"

**Subscription Management**:
- INFO: "Updating WebSocket subscriptions (current: {current}, new: {new}, added: {added}, removed: {removed})"
- INFO: "WebSocket reconnection complete with {count} subscriptions"
- DEBUG: "Added subscription for market {ticker} (ends: {end_date})"
- DEBUG: "Removed subscription for expired market {ticker}"

**Health Monitoring**:
- WARN: "Low event rate detected ({rate}/min, threshold: {threshold})"
- CRITICAL: "Event rate dropped to {rate}/min, possible silent rate limiting detected - exiting for restart"
- INFO: "Health check passed (events: {count}/min, subscriptions: {subs})"

### Metrics

New Prometheus metrics in `crates/polymarket-sub/src/metrics.rs`:

```rust
// Market discovery metrics
markets_discovered: Gauge
  - Description: "Number of active crypto binary markets discovered"

api_fetch_failures_total: Counter
  - Description: "Total API fetch failures"
  - Labels: error_type (timeout, http_error, parse_error)

// Subscription management metrics
assets_subscribed: Gauge
  - Description: "Current number of asset IDs subscribed to WebSocket"

subscription_updates_total: Counter
  - Description: "Total subscription update operations"
  - Labels: operation (added, removed)

markets_dropped_total: Counter
  - Description: "Total markets dropped due to subscription limit"

// Health monitoring metrics
events_per_minute: Gauge
  - Description: "Rolling event rate (events/minute over last 60 seconds)"

health_check_failures_total: Counter
  - Description: "Total health check failures leading to process exit"

last_event_timestamp: Gauge
  - Description: "Unix timestamp of last received event"
```

Existing metrics remain:
- `ws_events_received` (used by health monitor)
- `trades_published`
- `parsing_errors`
- `redis_publish_failures`
- `ws_connection_status`

### Alerts

Recommended alerts (implementation outside this service):

- `PolymarketSubscriberRestartLoop`: Rate of restarts > 3 per hour (possible persistent rate limiting)
- `PolymarketNoMarketDiscovery`: `markets_discovered` == 0 for > 10 minutes (API issue)
- `PolymarketHighAPIFailureRate`: `api_fetch_failures_total` increase > 10/hour (API degradation)
- `PolymarketLowEventRate`: `events_per_minute` < 5 for > 5 minutes (possible issue)

## Acceptance Criteria

- Service successfully discovers 350-400 active crypto binary markets on startup
- Asset ID list automatically updates every 5 minutes without service restart
- Closed markets are removed from subscriptions within 1 discovery cycle (5 minutes)
- New markets are added to subscriptions within 1 discovery cycle (5 minutes)
- If subscription count exceeds `max_subscriptions`, service logs warning and prioritizes by end_date
- If event rate drops below threshold for 60 seconds, service exits with code 1
- API fetch failures are retried 3 times with backoff before falling back to stale data
- Service continues operating with stale subscription list if API is temporarily unavailable
- All subscription changes are logged with counts and sample IDs
- Prometheus metrics accurately reflect current subscription count and event rate
- Service runs for 24 hours without crashes or memory leaks in dev environment
- WebSocket reconnection completes within 10 seconds during subscription updates
- Health monitor grace period allows 60 seconds for initial events after startup
- Configuration validation rejects invalid settings on startup
- Service can be toggled between automatic discovery and manual asset list via config

## References

- Polymarket CLI implementation: `polycli/src/commands/markets/save.ts`
- Polymarket CLI streaming workflow: `docs/polymarket/stream-selected-crypto-market.md`
- Polymarket WebSocket documentation: `docs/specs/003-polymarkket-support/wss-overview.md`
- Existing subscriber architecture: `docs/architecture.md` (Polymarket Subscriber section)
- Multi-chain workspace design: `docs/specs/002-multi-chain-workspace-refactoring/design.md`
- Polymarket Gamma API: `https://gamma-api.polymarket.com` (public API, no auth required)
