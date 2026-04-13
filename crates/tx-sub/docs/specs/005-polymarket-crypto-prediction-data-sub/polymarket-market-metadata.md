# Polymarket Market Metadata Enrichment

## Overview

Enrich Polymarket trade events published to Redis with market metadata to make it easier for downstream consumers to understand and process trade data without additional API lookups.

## Current State

### Current PolymarketTradeEvent Structure
```rust
pub struct PolymarketTradeEvent {
    pub asset_id: String,           // 256-bit token ID as decimal string
    pub market: String,              // Condition ID (hex without 0x prefix)
    pub price: f64,                  // 0-1 probability
    pub size: f64,                   // Trade volume
    pub side: TradeSide,             // BUY/SELL
    pub timestamp: i64,              // Unix timestamp in milliseconds
    pub fee_rate_bps: u32,          // Fee rate in basis points
}
```

### Current Redis Output
```json
{
  "buy": {
    "asset_id": "66565189719795067598116359786239144764346773839958163034253441997130385232203",
    "market": "0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e",
    "price": 0.85,
    "size": 10.33,
    "side": "BUY",
    "timestamp": 1763562856866,
    "fee_rate_bps": 0
  }
}
```

### Problem
- Downstream consumers need to make additional API calls to understand what market the trade is for
- No human-readable context (ticker, title, resolution time)
- No way to filter or route trades based on market characteristics

## Target State

### Enhanced PolymarketTradeEvent Structure
```rust
pub struct PolymarketTradeEvent {
    pub asset_id: String,
    pub market: String,              // Condition ID (kept for backward compatibility)
    pub price: f64,
    pub size: f64,
    pub side: TradeSide,
    pub timestamp: i64,
    pub fee_rate_bps: u32,
    pub market_metadata: Option<PolymarketMarketMetadata>,  // NEW
}

pub struct PolymarketMarketMetadata {
    pub event_id: String,            // Event ID from API
    pub ticker: String,              // e.g., "btc-updown-15m-1763567100"
    pub title: String,               // e.g., "Bitcoin Up or Down - November 19, 10:45AM-11:00AM ET"
    pub end_date: String,            // ISO 8601 timestamp (e.g., "2025-11-19T16:00:00Z")
}
```

### Enhanced Redis Output
```json
{
  "buy": {
    "asset_id": "66565189719795067598116359786239144764346773839958163034253441997130385232203",
    "market": "0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e",
    "price": 0.85,
    "size": 10.33,
    "side": "BUY",
    "timestamp": 1763562856866,
    "fee_rate_bps": 0,
    "market_metadata": {
      "event_id": "84565",
      "ticker": "btc-updown-15m-1763567100",
      "title": "Bitcoin Up or Down - November 19, 10:45AM-11:00AM ET",
      "end_date": "2025-11-19T16:00:00Z"
    }
  }
}
```

## Implementation Plan

### Phase 1: Update Trading Types Crate

**Location:** `/home/zfeng/popeyes/trading-types/`

#### 1.1 Add New Type Definition
- **File:** `src/trade_event.rs`
- **Changes:**
  1. Add `PolymarketMarketMetadata` struct with fields:
     - `event_id: String`
     - `ticker: String`
     - `title: String`
     - `end_date: String` (ISO 8601 format)
  2. Add `market_metadata: Option<PolymarketMarketMetadata>` field to `PolymarketTradeEvent`
  3. Derive `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize` for new struct
  4. Update existing tests to handle optional metadata field

#### 1.2 Update Tests
- **File:** `src/trade_event.rs` (test module)
- **Changes:**
  1. Add test for serialization/deserialization with metadata
  2. Add test for serialization/deserialization without metadata (backward compatibility)
  3. Update existing `PolymarketTradeEvent` tests to work with optional metadata

#### 1.3 Bump Version
- **File:** `Cargo.toml`
- **Change:** Bump version from current to next minor version (e.g., `0.2.4` → `0.2.5`)

### Phase 2: Update Polymarket Subscriber ✅ COMPLETED

**Location:** `/home/zfeng/popeyes/tx-sub/crates/polymarket-sub/`

**Status:** ✅ All tasks completed (2.1-2.9) and fully integrated with popeyes_trading_types v0.2.5
**Test Results:** ✅ 64 tests passing

**Completed:**
- Tasks 2.1-2.5: Infrastructure (cache, loader, discovery integration)
- Tasks 2.6-2.9: Parser and app integration
- **Final Integration:** Removed temporary type definitions, updated to use popeyes_trading_types v0.2.5
- **Workspace Dependency:** Updated Cargo.toml to use popeyes_trading_types = "0.2.5"
- All market metadata enrichment code is now active and functional

#### 2.1 Update Cargo.toml to Use Local Path
- **File:** `Cargo.toml`
- **Change:**
  ```toml
  # Development: use local path
  popeyes_trading_types = { path = "/home/zfeng/popeyes/trading-types" }

  # Production: use crates.io version (comment out during development)
  # popeyes_trading_types = "0.2.5"
  ```

#### 2.2 Create Market Metadata Cache Module ✅ COMPLETED

- **File:** `src/market_metadata_cache.rs` (NEW)
- **Purpose:** In-memory cache for market metadata indexed by condition_id
- **Status:** ✅ Completed
- **Implementation:**
  - Created `MarketMetadataCache` struct with `Arc<RwLock<HashMap<String, PolymarketMarketMetadata>>>`
  - Implemented all required methods:
    - `new()` - Create new empty cache
    - `insert()` - Insert single metadata entry
    - `get()` - Retrieve metadata by condition_id
    - `len()` - Get cache size
    - `is_empty()` - Check if cache is empty
    - `clear()` - Clear all entries
    - `insert_batch()` - Insert multiple entries efficiently (bonus method)
  - Added comprehensive unit tests covering:
    - Basic insert/get operations
    - Cache overwrite behavior
    - Multiple entries handling
    - Clear functionality
    - Batch insert
    - Concurrent reads (10 parallel tasks)
    - Concurrent reads and writes (10 parallel tasks)
  - Registered module in `main.rs`
  - Temporarily defined `PolymarketMarketMetadata` in `types.rs` pending Phase 1 completion
  - All tests pass, module compiles cleanly with no errors or warnings

#### 2.3 Update API Client to Fetch Full Market Metadata ✅ COMPLETED
- **File:** `src/api_client.rs`
- **Status:** ✅ Completed
- **Implementation:**
  1. Extended `ApiEvent` struct to include:
     - `title: Option<String>` ✅
     - `end_date: Option<String>` (already existed) ✅
  2. Extended `ApiMarket` struct to include:
     - `condition_id: Option<String>` ✅
  3. Updated `DiscoveredMarket` to include:
     - `title: String` ✅
     - `condition_id: String` ✅
  4. Updated `DiscoveredMarket::from_api_event()` to extract and validate all required fields ✅
     - Added validation for title field (returns None if missing)
     - Added extraction of condition_id from first market
     - Updated documentation to reflect new requirements
  5. Updated all tests in api_client.rs, market_discovery.rs, and subscription_manager.rs ✅
  6. All tests passing (60 tests) ✅

#### 2.4 Update Market Discovery Service ✅ COMPLETED
- **File:** `src/market_discovery.rs`
- **Status:** ✅ Completed
- **Implementation:**
  1. Added `market_metadata_cache: Arc<MarketMetadataCache>` field to `MarketDiscoveryService` struct ✅
  2. Updated `new()` constructor to accept cache parameter ✅
  3. Created `populate_metadata_cache()` method to populate cache after discovery ✅
     - Maps `condition_id` -> `PolymarketMarketMetadata { event_id, ticker, title, end_date }`
     - Uses efficient batch insert via `insert_batch()`
     - Returns count of entries added
  4. Updated `discover_and_broadcast()` to call cache population after filtering markets ✅
  5. Added comprehensive logging with cache statistics (markets_count, cache_entries_added, total_cached) ✅
  6. Updated all test functions to pass cache parameter to `MarketDiscoveryService::new()` ✅
  7. Added new test cases:
     - `test_populate_metadata_cache()`: Verifies cache is correctly populated with discovered markets ✅
     - `test_populate_metadata_cache_empty()`: Verifies empty market list handling ✅
  8. Updated `App::run_with_discovery()` to create and pass cache to discovery service ✅
  9. All tests passing (62 tests total) ✅

#### 2.5 Create Market Metadata Loader for Manual Mode ✅ COMPLETED
- **File:** `src/market_metadata_loader.rs` (NEW)
- **Purpose:** Fetch and cache market metadata on startup for manual mode
- **Status:** ✅ Completed
- **Implementation:**
  ```rust
  pub struct MarketMetadataLoader {
      api_client: Arc<PolymarketApiClient>,
      cache: Arc<MarketMetadataCache>,
  }

  impl MarketMetadataLoader {
      pub fn new(api_client: Arc<PolymarketApiClient>, cache: Arc<MarketMetadataCache>) -> Self;

      /// Fetch metadata for given asset IDs by querying API
      /// Maps asset_ids -> condition_ids -> metadata
      /// Returns number of markets cached
      pub async fn load_metadata_for_assets(&self, asset_ids: &[String], tag_id: u32) -> Result<usize>;

      /// Fetch all active markets for a tag and cache their metadata
      /// Returns number of markets cached
      pub async fn load_metadata_for_tag(&self, tag_id: u32) -> Result<usize>;
  }
  ```
- **Implementation Details:**
  1. Created `MarketMetadataLoader` struct with API client and cache dependencies ✅
  2. Implemented `load_metadata_for_assets()` method that:
     - Fetches events from API with pagination (batch size: 100) ✅
     - Filters events by matching asset IDs ✅
     - Converts `DiscoveredMarket` to `PolymarketMarketMetadata` ✅
     - Populates cache with condition_id -> metadata mapping using batch insert ✅
     - Returns error if no metadata found for configured assets ✅
     - Returns count of cached markets ✅
  3. Implemented `load_metadata_for_tag()` method for loading all markets for a tag ✅
  4. Added comprehensive logging (info, debug, warn levels) ✅
  5. Added unit tests for metadata conversion and asset ID filtering ✅
  6. Registered module in `main.rs` ✅
  7. Module compiles cleanly with no errors ✅

#### 2.6 Update Event Parser ✅ COMPLETED
- **File:** `src/parser.rs`
- **Status:** ✅ Completed
- **Implementation:**
  1. Added `market_metadata_cache: Arc<MarketMetadataCache>` to `EventParser` ✅
  2. Updated `parse_last_trade_price()` to async function ✅
     - Looks up metadata from cache using `market` (condition_id) ✅
     - Logs debug message if metadata not found (graceful degradation) ✅
     - Note: Field assignment commented out pending Phase 1 completion (trading types update)
  3. Updated constructor to accept cache parameter ✅
  4. Updated all test functions to async/await and pass cache parameter ✅
  5. All 64 tests passing ✅

#### 2.7 Update App Orchestration ✅ COMPLETED
- **File:** `src/app.rs`
- **Status:** ✅ Completed
- **Implementation:**

**Manual Mode (discovery disabled):** ✅
  1. Created `MarketMetadataCache` ✅
  2. Created `PolymarketApiClient` from discovery config (if exists) ✅
  3. Created `MarketMetadataLoader` with API client and cache ✅
  4. Call `load_metadata_for_assets(&config.assets, tag_id)` before starting components ✅
  5. Graceful degradation: Log warning if loader fails, continue without metadata ✅
  6. Pass cache to `EventParser` constructor ✅
  7. Added proper error handling and logging ✅

**Discovery Mode (discovery enabled):** ✅
  1. Created `MarketMetadataCache` (already existed) ✅
  2. Cache passed to `MarketDiscoveryService` constructor (already done in 2.4) ✅
  3. Discovery service populates cache automatically during discovery ✅
  4. Pass cache to `EventParser` constructor ✅

#### 2.8 Update Configuration ✅ COMPLETED
- **File:** `src/config.rs`
- **Status:** ✅ Completed
- **Implementation:**
  1. Validation already ensures tag_id is present when discovery is enabled ✅
  2. No structural changes needed (existing fields sufficient) ✅
  3. Existing validation logic is adequate for metadata enrichment requirements ✅

#### 2.9 Add Module Registration ✅ COMPLETED
- **File:** `src/main.rs`
- **Status:** ✅ Completed
- **Changes:**
  ```rust
  mod market_metadata_cache;
  mod market_metadata_loader;
  ```
- Both modules registered and compiling successfully ✅

### Phase 3: Testing

#### 3.1 Unit Tests

**File: `crates/polymarket-sub/src/market_metadata_cache.rs`**
- Test cache insert and get operations
- Test concurrent access (multiple readers/writers)
- Test cache size tracking

**File: `crates/polymarket-sub/src/market_metadata_loader.rs`**
- Test loading metadata for known asset IDs
- Test handling API failures gracefully
- Test empty result handling

**File: `crates/polymarket-sub/src/parser.rs`**
- Test parsing trade event with metadata in cache
- Test parsing trade event without metadata in cache (graceful degradation)
- Test serialization of enriched event to JSON

#### 3.2 Integration Tests ✅ COMPLETED

**Automated Integration Test:** ✅
- **Location:** `crates/polymarket-sub/src/bin/integration_test.rs`
- **Config:** `configs/polymarket-sub/config.integration_test.yaml`
- **Documentation:** `docs/polymarket-sub/developer-guide.md` (Testing section)
- **Run command:** `cargo run -p polymarket-sub --bin polymarket-integration-test`
- **Or via justfile:** `just test-polymarket-integration`
- **Duration:** 10 seconds (automatic timeout and exit)
- **Features tested:**
  - Configuration loading and validation
  - Market discovery service initialization
  - API requests to fetch active crypto markets
  - WebSocket client setup (if discovery completes in time)
  - Graceful shutdown after timeout
- **Success criteria:** Test runs for 10 seconds and exits without errors

**Manual Mode Test:**
1. Configure polymarket-sub in manual mode with known asset IDs
2. Start subscriber (triggers metadata loader)
3. Verify metadata is loaded and cached
4. Trigger test trade events via WebSocket
5. Verify Redis output contains market_metadata field
6. Verify metadata matches expected values

**Discovery Mode Test:**
1. Configure polymarket-sub in discovery mode with crypto tag
2. Start subscriber (triggers discovery)
3. Verify metadata is discovered and cached
4. Wait for real trade events or inject test events
5. Verify Redis output contains market_metadata field
6. Verify metadata is updated when markets change

#### 3.3 Manual Testing

**Redis Output Verification:**
```bash
# Check Redis for enriched events
redis-cli LRANGE "polymarket:trades" 0 5

# Verify JSON structure includes market_metadata
redis-cli LRANGE "polymarket:trades" 0 0 | jq '.buy.market_metadata'
```

**Metrics Verification:**
- Check Prometheus metrics for:
  - Cache size: `polymarket_market_metadata_cache_size`
  - Cache hits: `polymarket_market_metadata_cache_hits`
  - Cache misses: `polymarket_market_metadata_cache_misses`
  - Loader success/failures: `polymarket_metadata_loader_fetch_total`

### Phase 4: Metrics and Observability

#### 4.1 Add Cache Metrics
- **File:** `src/metrics.rs`
- **New Metrics:**
  ```rust
  // Cache size gauge
  polymarket_market_metadata_cache_size: IntGauge

  // Cache hit/miss counters
  polymarket_market_metadata_cache_hits: IntCounter
  polymarket_market_metadata_cache_misses: IntCounter

  // Metadata loader metrics
  polymarket_metadata_loader_fetch_total: IntCounterVec (labels: status=success|failure)
  polymarket_metadata_loader_markets_cached: IntCounter
  ```

#### 4.2 Add Logging
- Log cache initialization
- Log metadata loader start/completion with counts
- Log cache hits/misses at debug level
- Log warnings when metadata not found for active trades

### Phase 5: Documentation

#### 5.1 Update Architecture Documentation
- **File:** `docs/architecture.md`
- **Changes:**
  1. Add section on market metadata enrichment in Polymarket subscriber
  2. Document cache architecture and lifecycle
  3. Update data flow diagrams to show metadata enrichment step

#### 5.2 Update Configuration Examples
- **Files:** `configs/polymarket-sub/*.yaml`
- **Changes:**
  1. Add comments explaining metadata enrichment
  2. Document that metadata is automatically loaded in both modes

### Phase 6: Production Deployment

#### 6.1 Publish Trading Types Crate
1. Revert `Cargo.toml` to use crates.io version
2. Test build with published crate
3. Publish new version to crates.io
4. Update all dependent crates

#### 6.2 Deploy and Monitor
1. Deploy to staging environment
2. Monitor metrics for cache hit rate
3. Verify Redis output contains metadata
4. Monitor error rates and logs
5. Deploy to production
6. Monitor for 24 hours

## Configuration Examples

### Manual Mode Configuration
```yaml
polymarket:
  ws_endpoint: "wss://ws-subscriptions-clob.polymarket.com"
  asset_ids:
    - "109681959945973826496234384791167033612800000000000000000000000376"
    - "52181619848812915160551060468842099699261274828279546744438464278138132224"

  market_discovery:
    enabled: false  # Manual mode
    # Metadata will be fetched via API on startup for configured asset_ids
```

### Discovery Mode Configuration
```yaml
polymarket:
  ws_endpoint: "wss://ws-subscriptions-clob.polymarket.com"

  market_discovery:
    enabled: true
    api_base_url: "https://gamma-api.polymarket.com"
    tag_id: 21  # Crypto tag
    ticker_patterns:
      - "btc-updown-15m-"
      - "eth-updown-15m-"
    # Metadata automatically populated during discovery
```

## Data Flow

### Manual Mode
```
Startup
  ↓
MarketMetadataLoader.load_metadata_for_assets(asset_ids)
  ↓
API Request (fetch events with matching asset_ids)
  ↓
Extract metadata (event_id, ticker, title, end_date, condition_id)
  ↓
Populate MarketMetadataCache (condition_id -> metadata)
  ↓
Start WebSocket Client
  ↓
EventParser receives trade event
  ↓
Lookup metadata in cache by condition_id
  ↓
Enrich PolymarketTradeEvent with metadata
  ↓
Publish to Redis
```

### Discovery Mode
```
Startup
  ↓
MarketDiscoveryService.run()
  ↓
API Request (fetch active markets for tag)
  ↓
Extract metadata (event_id, ticker, title, end_date, condition_id)
  ↓
Populate MarketMetadataCache (condition_id -> metadata)
  ↓
Broadcast SubscriptionUpdate with asset_ids
  ↓
App reconnects WebSocket with new subscriptions
  ↓
EventParser receives trade event
  ↓
Lookup metadata in cache by condition_id
  ↓
Enrich PolymarketTradeEvent with metadata
  ↓
Publish to Redis
  ↓
(Repeat discovery every N seconds, updating cache)
```

## Performance Considerations

### Memory Usage
- Cache size: ~100-500 markets × ~200 bytes/market = ~20-100 KB
- Negligible memory overhead
- Cache grows bounded by active markets count

### Latency Impact
- Cache lookup: O(1) hash map lookup, ~microseconds
- No additional API calls during trade processing
- Metadata loaded asynchronously on startup/discovery

### API Rate Limiting
- Metadata fetched once on startup (manual mode)
- Metadata refreshed on discovery interval (discovery mode, default 60s)
- No per-trade API calls

## Error Handling

### Metadata Loader Failures (Manual Mode)
- **Scenario:** API unavailable, network error, parse error
- **Handling:**
  1. Log error with details
  2. Continue starting subscriber (graceful degradation)
  3. Trades will be published without metadata
  4. Increment failure metric

### Cache Miss During Trade Processing
- **Scenario:** Trade event for market not in cache
- **Handling:**
  1. Log warning with condition_id
  2. Publish trade without metadata
  3. Increment cache miss counter
  4. Continue processing

### API Response Missing Fields
- **Scenario:** Event missing title, end_date, or condition_id
- **Handling:**
  1. Skip that event during metadata extraction
  2. Log warning with event_id
  3. Continue processing other events

## Open Questions

1. **Cache Invalidation:** Should we implement TTL for cache entries? Markets resolve and become inactive.
   - **Recommendation:** Not needed initially. Discovery mode naturally refreshes cache. Manual mode can restart periodically.

2. **Cache Persistence:** Should cache be persisted to disk for faster restarts?
   - **Recommendation:** Not needed initially. Cache is fast to rebuild (<5 seconds).

3. **Condition ID Format:** Should we normalize condition_id format (with/without 0x prefix)?
   - **Recommendation:** Use format as-is from WebSocket (without 0x). Document clearly.

4. **Metrics Cardinality:** Should we add market-level metrics (per ticker)?
   - **Recommendation:** Not initially. Use aggregated metrics to avoid cardinality explosion.

## Success Criteria

1. ✅ All trades published to Redis include `market_metadata` field when available
2. ✅ Cache hit rate > 95% in discovery mode
3. ✅ Cache hit rate > 90% in manual mode (lower due to static config)
4. ✅ No performance degradation (latency < 1ms per trade)
5. ✅ Zero breaking changes to existing deployments
6. ✅ All tests passing (unit + integration)
7. ✅ Documentation updated and complete
