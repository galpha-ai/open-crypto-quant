# Task Tracker: Automated Crypto Binary Prediction Market Subscription

## 1. Problem Statement

The Polymarket subscriber currently requires manually configuring asset IDs in the config file (`configs/polymarket-sub/config.yaml`). This is problematic for crypto binary prediction markets because:

- New 15-minute markets are created every 15 minutes (BTC/ETH/SOL/XRP "Up or Down" markets)
- Markets expire after resolving, requiring constant manual updates
- Manually tracking ~388 active markets across 4 cryptocurrencies is impractical
- The service needs restart to pick up new asset IDs

This task implements automatic market discovery by:
1. Periodically fetching active crypto binary markets from Polymarket Gamma API
2. Dynamically updating WebSocket subscriptions without service restart
3. Pruning closed markets to keep subscription count bounded
4. Detecting silent rate limiting via health monitoring (triggering orchestrator restart)

## 2. Plan

The implementation follows a layered approach:

**Foundation Layer**: Add configuration schema and data structures for market discovery, subscription state, and health monitoring.

**Market Discovery Layer**: Implement HTTP client to fetch markets from Polymarket API, filter by ticker patterns (`{btc,eth,sol,xrp}-updown-15m-*`), extract asset IDs from `clobTokenIds`, and broadcast updates via channel.

**Subscription Management Layer**: Calculate subscription diffs (additions/removals), enforce max subscription limits with prioritization by end_date, coordinate WebSocket reconnection when subscription list changes.

**Health Monitoring Layer**: Track event rate over rolling 60-second window, detect silent rate limiting when events drop below threshold (default: 1 event/min), exit process to trigger orchestrator restart.

**Integration Layer**: Wire all components together in the App orchestrator, add Prometheus metrics for observability, coordinate graceful shutdown.

**Testing & Deployment**: Unit tests for core logic, integration tests with mocks, phased rollout starting with manual mode for validation, gradual tuning of subscription limits and health thresholds.

## 3. Implementation Phases

### Phase 1: Foundation (Tasks 1-2)
- **Objective**: Establish configuration schema and core data structures
- **Tasks**: Tasks 1-2
- **Deliverable**: Configuration types and in-memory data structures for market metadata, subscription state, and event tracking

### Phase 2: Market Discovery (Tasks 3-5)
- **Objective**: Implement automatic market discovery from Polymarket API
- **Tasks**: Tasks 3-5
- **Deliverable**: MarketDiscoveryService that periodically fetches, filters, and broadcasts active market asset IDs

### Phase 3: Subscription Management (Tasks 6-8)
- **Objective**: Dynamically manage WebSocket subscriptions based on discovered markets
- **Tasks**: Tasks 6-8
- **Deliverable**: SubscriptionManager that calculates diffs, enforces limits, and coordinates WebSocket reconnection

### Phase 4: Health Monitoring (Tasks 9-10)
- **Objective**: Detect silent rate limiting via event rate monitoring
- **Tasks**: Tasks 9-10
- **Deliverable**: HealthMonitor that tracks event rate and exits process when unhealthy

### Phase 5: Integration & Metrics (Tasks 11-13)
- **Objective**: Orchestrate all components and add observability
- **Tasks**: Tasks 11-13
- **Deliverable**: Updated App with all components wired together, Prometheus metrics, and graceful shutdown

### Phase 6: Testing (Tasks 14-16)
- **Objective**: Validate implementation with comprehensive tests
- **Tasks**: Tasks 14-16
- **Deliverable**: Unit and integration test suite covering core logic and end-to-end flows

### Phase 7: Deployment & Tuning (Tasks 17-22)
- **Objective**: Deploy to production with phased rollout and parameter tuning
- **Tasks**: Tasks 17-22
- **Deliverable**: Production-ready service with validated configuration, documentation, and monitoring

## 4. TODO List

1. Add configuration schema for market discovery and health monitoring
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/config.rs`. Add two new nested config structs: `MarketDiscoveryConfig` with fields (`enabled: bool`, `api_base_url: String`, `tag_id: u32`, `ticker_patterns: Vec<String>`, `discovery_interval_secs: u64`, `max_subscriptions: usize`, `api_timeout_secs: u64`, `api_retry_attempts: u32`, `api_retry_backoff_ms: u64`) and `HealthMonitoringConfig` with fields (`enabled: bool`, `check_interval_secs: u64`, `min_events_per_minute: u64`, `event_tracking_window_secs: u64`). Add these as optional fields to `PolymarketConfig`. Implement validation logic: if `market_discovery.enabled = true`, at least one ticker pattern must be specified; `max_subscriptions` must be 1-10000; `discovery_interval_secs` >= 60; `check_interval_secs` >= 10. Make `assets` field optional (XOR with market_discovery.enabled).
   - Success Criteria: Config parses successfully from YAML, validation rejects invalid settings (e.g., empty ticker_patterns, discovery_interval < 60), service can toggle between manual and automatic mode

2. Create data structures for market metadata, subscription state, and event tracking
   - Status: Completed
   - Note: **Files**: Create `crates/polymarket-sub/src/types.rs` with three structs: `MarketMetadata` (fields: `event_id: String`, `ticker: String`, `end_date: DateTime<Utc>`, `asset_ids: Vec<String>`, `discovered_at: DateTime<Utc>`), `SubscriptionState` (fields: `current_assets: HashSet<String>`, `last_updated: DateTime<Utc>`, `total_markets: usize`, methods: `calculate_diff(&self, new_assets: HashSet<String>) -> (Vec<String>, Vec<String>)`, `update(&mut self, new_assets: HashSet<String>)`, `len(&self) -> usize`), `EventRateTracker` (fields: `events_per_second: HashMap<u64, u64>` for ring buffer, `total_events: u64`, methods: `record_event(&mut self)`, `get_rate_per_minute(&self) -> u64`, `prune_old(&mut self, cutoff: DateTime<Utc>)`). Use `Arc<Mutex<>>` wrapper for shared state.
   - Success Criteria: SubscriptionState.calculate_diff correctly identifies additions and removals, EventRateTracker.get_rate_per_minute sums last 60 seconds, prune_old removes entries older than tracking window

3. Implement HTTP client for Polymarket Gamma API
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/api_client.rs`. Create `PolymarketApiClient` struct wrapping `reqwest::Client` with timeout and retry configuration. Implement method `fetch_events(&self, tag_id: u32, limit: usize, offset: usize) -> Result<Vec<ApiEvent>>` that calls `GET https://gamma-api.polymarket.com/events?tag={tag_id}&active=true&closed=false&limit={limit}&offset={offset}`. Use retry middleware with exponential backoff (3 attempts, 1s initial backoff). Define `ApiEvent` and `ApiMarket` serde structs matching API response (see design doc Data Structures section). Handle HTTP errors (4xx/5xx), timeouts, and JSON parse errors with detailed error types.
   - Success Criteria: Client successfully fetches and deserializes events, retries on transient failures, returns error after max retries, validates TLS certificates

4. Implement market discovery service with filtering and asset extraction
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/market_discovery.rs`. Create `MarketDiscoveryService` struct with `PolymarketApiClient`, `MarketDiscoveryConfig`, and broadcast sender. Implement async method `run(&self, cancellation_token: CancellationToken)` that: (1) runs on interval timer (`discovery_interval_secs`), (2) paginates through all events (100 per batch), (3) filters events where ticker matches any pattern in `ticker_patterns` using `starts_with()`, (4) extracts asset IDs by parsing `clob_token_ids` JSON array string with `serde_json::from_str()`, (5) deduplicates asset IDs, (6) sorts markets by `end_date` ascending, (7) broadcasts `Vec<DiscoveredMarket>` via channel. Handle empty results (log warning, keep current subscriptions), parse errors (skip market, log error), and API failures (log error, increment metric, continue with stale data).
   - Success Criteria: Discovers 350-400 active markets on startup, filters only crypto binary markets, extracts all asset IDs, broadcasts updates every 5 minutes, handles API errors gracefully without crashing

5. Add API client error handling and metrics
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/metrics.rs`. Add Prometheus counter `api_fetch_failures_total` with label `error_type: {timeout, http_error, parse_error}`. Update `PolymarketApiClient` to increment counter on failures. Add gauge `markets_discovered` to track count of active markets. In `MarketDiscoveryService.run()`, log detailed errors: INFO on successful discovery with counts, WARN on fetch failures with attempt count, ERROR after all retries exhausted. Use structured logging with fields: `markets_count`, `asset_count`, `error_type`, `attempt`.
   - Success Criteria: Metrics accurately reflect API health, logs provide enough context for debugging, service continues operating after transient failures

6. Implement subscription state management and diff calculation
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/subscription_manager.rs`. Create `SubscriptionManager` struct with `Arc<Mutex<SubscriptionState>>`, `MarketDiscoveryConfig`, and broadcast channels (receive from discovery, send to app). Implement `calculate_diff()` logic: compare new asset set with current, identify additions (in new but not current), removals (in current but not new). Implement `apply_subscription_limit()`: if total assets > `max_subscriptions`, sort combined (new + unchanged) by end_date ascending, take first `max_subscriptions` assets, log WARNING with dropped count. Update `SubscriptionState` with new asset set and timestamp.
   - Success Criteria: Diff calculation correctly identifies all additions and removals, limit enforcement prioritizes markets resolving soonest, dropped markets are logged with sample IDs

7. Implement subscription manager orchestration
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/subscription_manager.rs`. Implement async method `run(&self, mut discovery_rx: Receiver<Vec<DiscoveredMarket>>, cancellation_token: CancellationToken)` that: (1) receives market updates from discovery service, (2) calls `calculate_diff()` and `apply_subscription_limit()`, (3) if changes exist, broadcasts new asset list to app for WebSocket reconnection, (4) updates metrics (`assets_subscribed`, `subscription_updates_total{operation=added}`, `subscription_updates_total{operation=removed}`), (5) logs INFO with subscription change summary. Handle empty market lists (log WARN, keep current subscriptions).
   - Success Criteria: Subscription updates trigger WebSocket reconnection only when changes occur, metrics accurately reflect subscription state, logs show detailed change information

8. Implement WebSocket reconnection coordination
   - Status: Completed
   - Note: **Files**: `crates/polymarket-sub/src/app.rs`, `crates/polymarket-sub/src/ws_client.rs`, `crates/polymarket-sub/src/parser.rs`, `crates/polymarket-sub/src/redis_publisher.rs`. Added `CancellationToken` support to all components for graceful shutdown. Updated `App.run()` to support two modes: (1) manual mode with static asset list (backward compatible), (2) automatic discovery mode with `run_with_discovery()`. In discovery mode, the app waits for initial subscription from discovery service, then enters main coordination loop monitoring for subscription updates and WebSocket exits. On subscription update: abort current WebSocket task, wait 500ms for cleanup, spawn new WebSocket with updated asset list. All components (WebSocket, parser, Redis publisher, discovery, subscription manager) accept cancellation tokens and shut down gracefully. Parser and Redis publisher updated to handle cancellation in their main loops.
   - Success Criteria: Reconnection completes within 10 seconds, no events lost during transition (buffer is flushed), connection is cleanly closed, new subscription takes effect immediately

9. Implement event rate tracker for health monitoring
   - Status: Completed
   - Note: **File**: `crates/polymarket-sub/src/health_monitor.rs`. Created `HealthMonitor` struct with `Arc<Mutex<EventRateTracker>>`, `HealthMonitoringConfig`, `Arc<Mutex<SubscriptionState>>` (to check subscription count). Implemented async method `run()` that spawns two tasks: (1) event recording task that subscribes to parsed trade events via broadcast receiver and records them via `EventRateTracker.record_event()`, (2) health check task that periodically (every `check_interval_secs`) calculates events/min via `get_rate_per_minute()` and prunes old entries via `prune_old()`. EventRateTracker uses ring buffer keyed by `timestamp % tracking_window_secs` for O(1) lookups.
   - Success Criteria: Event rate is accurately calculated over 60-second window, old entries are pruned to prevent unbounded growth, minimal overhead per event

10. Implement health check logic with process exit
    - Status: Completed
    - Note: **File**: `crates/polymarket-sub/src/health_monitor.rs`. Implemented in `HealthMonitor.health_check_task()` periodic check: (1) gets events/min from `EventRateTracker`, (2) gets subscription count from `SubscriptionState`, (3) if `subscription_count > 0 && events_per_minute < min_events_per_minute && time_since_startup > 60s`: logs CRITICAL error with context (`events_per_minute`, `subscription_count`, `threshold`), increments `health_check_failures_total` metric, calls `std::process::exit(1)`, (4) else updates `events_per_minute` and `last_event_timestamp` gauges, logs INFO on success. Implemented 60-second grace period after startup using `Instant::now()` to track startup time. Integrated into both manual and automatic discovery modes in `App`.
    - Success Criteria: Process exits with code 1 when event rate drops below threshold (with subscriptions active), grace period prevents false positives during startup, metrics are updated before exit

11. Add Prometheus metrics for discovery, subscriptions, and health
    - Status: Completed
    - Note: **File**: `crates/polymarket-sub/src/metrics.rs`. Add to existing metrics registry: `markets_discovered: Gauge` ("Number of active crypto binary markets discovered"), `assets_subscribed: Gauge` ("Current number of asset IDs subscribed to WebSocket"), `subscription_updates_total: Counter` with label `operation: {added, removed}` ("Total subscription update operations"), `markets_dropped_total: Counter` ("Total markets dropped due to subscription limit"), `events_per_minute: Gauge` ("Rolling event rate over last 60 seconds"), `health_check_failures_total: Counter` ("Total health check failures leading to process exit"), `last_event_timestamp: Gauge` ("Unix timestamp of last received event"). Register all metrics in `init_metrics()` function.
    - Success Criteria: All metrics are exposed on Prometheus endpoint, gauges reflect current state, counters increment correctly, metric names follow Prometheus naming conventions

12. Update App to initialize and orchestrate new components
    - Status: Completed
    - Note: **File**: `crates/polymarket-sub/src/app.rs`. Modify `App::run()` to: (1) check `market_discovery.enabled` flag, (2) if enabled: create `PolymarketApiClient`, spawn `MarketDiscoveryService.run()` task, spawn `SubscriptionManager.run()` task, spawn `HealthMonitor.run()` task, (3) if disabled: use existing manual asset list from config, (4) add all tasks to `tokio::select!` for concurrent execution, (5) create CancellationToken and pass to all tasks. On initial startup (before WebSocket connection), fetch initial market list and build subscription set. Handle component failures: log error, trigger graceful shutdown if critical component fails.
    - Success Criteria: All components start successfully in both manual and automatic mode, components coordinate via broadcast channels, graceful shutdown propagates to all tasks

13. Wire up broadcast channels and shutdown coordination
    - Status: Completed
    - Note: **File**: `crates/polymarket-sub/src/app.rs`. Create broadcast channels: (1) `discovery_to_manager: broadcast::channel<Vec<DiscoveredMarket>>(1)` for market updates, (2) `manager_to_app: broadcast::channel<HashSet<String>>(1)` for subscription changes, (3) `parser_to_health: broadcast::channel<PolymarketTradeEvent>(1000)` (clone existing parser output). Implement graceful shutdown: on SIGTERM/SIGINT, set CancellationToken, await all task handles with timeout (30s), flush WebSocket buffer, log final state (subscription count, total events processed). Ensure shutdown order: stop health monitor first, then subscription manager, then discovery service, finally WebSocket client (after flush).
    - Success Criteria: Shutdown completes within 30 seconds, buffer is flushed before exit, no events are lost during shutdown, final state is logged

14. Unit tests for API response parsing and filtering
    - Status: Not Started
    - Note: **File**: `crates/polymarket-sub/src/market_discovery.rs` (test module). Test cases: (1) parse valid API response with 2 markets, verify `ApiEvent` deserialization, (2) filter markets by ticker patterns (match `sol-updown-15m-`, reject `sol-updown-1h-`), (3) extract asset IDs from `clobTokenIds` JSON array, verify deduplication, (4) handle missing `clobTokenIds` (skip market, continue processing), (5) handle invalid `end_date` format (skip market, log error), (6) handle empty API response (return empty vec, don't crash), (7) sort markets by `end_date` ascending. Use fixture data from actual Polymarket API responses.
    - Success Criteria: All test cases pass, edge cases handled correctly, no panics on malformed input

15. Unit tests for subscription diff calculation and limit enforcement
    - Status: Not Started
    - Note: **File**: `crates/polymarket-sub/src/subscription_manager.rs` (test module). Test cases: (1) calculate diff with additions only (empty current, 5 new), (2) calculate diff with removals only (5 current, empty new), (3) calculate diff with both (5 current, 3 removed, 4 added, 2 unchanged), (4) enforce limit when under limit (100 assets, limit 200 -> no drops), (5) enforce limit when over limit (500 assets, limit 300 -> drop 200), (6) verify prioritization by end_date (keep markets resolving soonest), (7) handle edge case: all markets have same end_date (stable sort by ticker). Assert additions/removals are correct, dropped markets logged.
    - Success Criteria: All test cases pass, diff calculation is accurate, limit enforcement prioritizes correctly, no off-by-one errors

16. Integration tests with mock API and WebSocket
    - Status: Not Started
    - Note: **File**: `crates/polymarket-sub/tests/integration_test.rs`. Set up mock HTTP server (using `wiremock` crate) returning paginated market data. Set up mock WebSocket server accepting subscriptions. Test scenarios: (1) end-to-end flow: discovery -> subscription update -> WebSocket reconnection, (2) API retry on transient failure (return 500 twice, then 200), (3) health check triggering exit (stop sending WebSocket events, verify process exits after threshold), (4) subscription limit enforcement (return 600 markets, verify only 500 subscribed), (5) graceful shutdown (send SIGTERM, verify buffer flush and clean exit). Verify metrics are updated correctly in each scenario.
    - Success Criteria: All integration tests pass, end-to-end flow works correctly, error handling is robust, metrics reflect actual behavior

17. Update configuration files with new settings
    - Status: Not Started
    - Note: **File**: `configs/polymarket-sub/config.yaml`. Add `market_discovery` and `health_monitoring` sections with initial conservative values: `enabled: false` (manual mode for initial deployment), `api_base_url: "https://gamma-api.polymarket.com"`, `tag_id: 21`, `ticker_patterns: ["btc-updown-15m-", "eth-updown-15m-", "sol-updown-15m-", "xrp-updown-15m-"]`, `discovery_interval_secs: 300`, `max_subscriptions: 1000` (will tune down in Phase 2), `api_timeout_secs: 30`, `api_retry_attempts: 3`, `api_retry_backoff_ms: 1000`, `health_monitoring.enabled: true`, `check_interval_secs: 60`, `min_events_per_minute: 1`, `event_tracking_window_secs: 120`. Keep existing `assets` list for fallback. Add comments explaining each setting.
    - Success Criteria: Config validates successfully, service starts in manual mode (backward compatible), settings are well-documented

18. Update documentation and deployment guides
    - Status: Not Started
    - Note: **Files**: Update `docs/architecture.md` Polymarket Subscriber section to describe new components (MarketDiscoveryService, SubscriptionManager, HealthMonitor). Update `README.md` with configuration options and monitoring guidelines. Create `docs/polymarket/auto-discovery.md` documenting the automatic market discovery workflow, configuration tuning process, and troubleshooting guide (common errors: API failures, rate limiting detection, subscription limit reached). Include example Prometheus queries for monitoring (`rate(api_fetch_failures_total[5m])`, `assets_subscribed`, `events_per_minute`). Document recommended alerts from design doc.
    - Success Criteria: Documentation clearly explains new features, configuration is well-documented, troubleshooting guide covers common issues

19. Deploy to dev with manual mode, validate existing functionality
    - Status: Not Started
    - Note: **Deployment**: Deploy to dev environment with `market_discovery.enabled: false` to verify backward compatibility. Monitor for 24 hours: check event rate remains stable, Redis publishes continue, no crashes or memory leaks, existing metrics (ws_events_received, trades_published) are unchanged. Run smoke tests: verify WebSocket connection, parser processing, Redis publishing. Check logs for errors or warnings. Rollback criteria: event rate drops to 0, crash loop (>5 restarts/hour), API errors >50%.
    - Success Criteria: Service runs for 24 hours without issues, existing functionality unaffected, no performance degradation

20. Enable automatic discovery in dev, monitor and tune subscription limits
    - Status: Not Started
    - Note: **Deployment**: Update config to `market_discovery.enabled: true` with `max_subscriptions: 1000`. Monitor logs for discovery events (expected: ~388 active markets). Verify WebSocket reconnection every 5 minutes when subscription list changes. Track new metrics: `markets_discovered` (should be 350-400), `assets_subscribed` (should be ~776 for 388 markets with 2 outcomes each), `subscription_updates_total`. Gradually reduce `max_subscriptions`: 1000 -> 750 -> 500 -> 300 over 2 weeks. At each step, monitor for health check failures (indicating silent rate limiting). Document working limit in config comments.
    - Success Criteria: Markets are discovered automatically, subscriptions update without restart, optimal max_subscriptions determined (no rate limiting), health monitor detects issues correctly

21. Deploy to staging with validated configuration
    - Status: Not Started
    - Note: **Deployment**: Deploy to staging with production-like config (using tuned `max_subscriptions` from dev). Run for 3 days monitoring: event rate stability (should be consistent), API fetch success rate (>95%), WebSocket reconnection success (>99%), no health check failures. Collect event rate data to calculate percentile distribution for threshold calibration. Adjust `min_events_per_minute` to p5 (5th percentile) to allow natural variance while catching true issues. Validate graceful shutdown: send SIGTERM, verify buffer flush and clean exit within 30s.
    - Success Criteria: Staging runs for 3 days without issues, event rate data collected, health threshold calibrated, graceful shutdown works correctly

22. Production rollout with gradual rollout strategy
    - Status: Not Started
    - Note: **Deployment**: Gradual rollout to production: Day 1 (25% of instances with automatic discovery, 75% manual), Day 3 (50/50 split), Day 5 (100% automatic). Monitor closely: zero unplanned restarts, event rate within 5% of manual baseline, `markets_discovered` ~388, `api_fetch_failures_total` low (<1% of requests), no alert triggers. Set up recommended alerts: `PolymarketSubscriberRestartLoop` (restarts >3/hour), `PolymarketNoMarketDiscovery` (markets_discovered == 0 for >10 min), `PolymarketHighAPIFailureRate` (failures >10/hour), `PolymarketLowEventRate` (events/min <5 for >5 min). Rollback plan: set `market_discovery.enabled: false`, redeploy with manual asset list, investigate before re-enabling.
    - Success Criteria: Production rollout completes successfully, no incidents, event rate stable, alerts configured, runbook documented

## 5. Usage Guide

This task tracker is designed for both human reviewers and AI agents executing the implementation.

### For AI Agents

**Updating Status:**
- Mark a task as "In Progress" BEFORE beginning work on it
- Mark as "Completed" IMMEDIATELY after finishing (don't batch completions)
- Only mark as "Completed" when you have FULLY accomplished the task
- If you encounter errors, blockers, or cannot finish, keep the task as "In Progress"

**Using the Note Field:**
The Note field documents:
1. **File locations** where changes will be made
2. **What will be implemented** (core functionality/changes)
3. **Key technical details** (important methods, patterns, constraints)

The Note field can and should be expanded during planning/design discussions:
- Initial notes establish scope and location
- Refined notes added after design review guide implementation
- Implementation steps can be added as sub-bullets under the Note

**Success Criteria:**
- Check all success criteria before marking a task as Completed
- If any criterion fails, the task is not complete

**Working with Phases:**
- This task list is divided into 7 phases (see Section 3)
- You can be instructed to complete specific phases (e.g., "complete Phase 1")
- Phases must be completed sequentially (Phase 2 depends on Phase 1)
- Tasks within a phase can sometimes be done in parallel (check dependencies in notes)

**Referencing Tasks:**
- Tasks are numbered 1-22 for precise referencing
- You can be instructed to work on specific tasks (e.g., "complete task 5")
- Check task dependencies: some tasks reference outputs from earlier tasks

**Version Control:**
- This tracker document can be committed to git during development to track work-in-progress
- **BEFORE creating a PR**: Remove this file
- Rationale: This file documents the work process, but only the final results (code changes, documentation updates) should be preserved in the PR
- Use this tracker as a reminder to review and update all relevant documentation before removing it

### For Reviewers

This task breakdown follows the design document at `docs/specs/005-polymarket-crypto-prediction-data-sub/design.md`. Each phase represents a logical grouping of related functionality:

- **Phase 1-2**: Foundation and market discovery (independent implementation)
- **Phase 3-4**: Subscription and health management (depends on Phase 2)
- **Phase 5**: Integration (depends on Phases 3-4)
- **Phase 6**: Testing (depends on Phase 5)
- **Phase 7**: Deployment (depends on Phase 6)

The rollout plan from the design document has been incorporated into Phase 7 tasks (19-22), providing specific deployment steps, monitoring criteria, and rollback procedures.

Key risks and mitigations:
- **Risk**: Silent rate limiting from Polymarket API → **Mitigation**: Health monitor with automatic restart
- **Risk**: Subscription limit unknown → **Mitigation**: Gradual tuning starting conservatively (Task 20)
- **Risk**: WebSocket reconnection disrupts data flow → **Mitigation**: Buffer flush, 5-minute interval limits disruption
- **Risk**: API changes break parsing → **Mitigation**: Comprehensive error handling, fallback to stale data

Progress can be tracked via task status and the new Prometheus metrics introduced in Task 11.
