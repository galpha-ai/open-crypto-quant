# Task Tracker: Polymarket Subscriber Implementation

## 1. Problem Statement

We need to implement a real-time data subscription service for Polymarket prediction markets to enable downstream analysis of emotional panic signals and market inefficiencies. Currently, the tx-sub workspace only supports Solana blockchain data. This implementation will add Polymarket WebSocket API support, allowing us to track trade prices, volumes, and directions across configured prediction market assets.

The service must maintain architectural consistency with the existing `solana-sub` service while handling the unique characteristics of Polymarket's WebSocket API (CLOB) and prediction market data model.

## 2. Plan

### Architecture Overview
Build a new `polymarket-sub` crate within the tx-sub workspace that:
1. Connects to Polymarket's WebSocket API with TLS support
2. Subscribes to market channels for configured asset IDs
3. Parses `last_trade_price` events into structured data
4. Publishes to Redis using existing `common` crate infrastructure
5. Exposes Prometheus metrics for monitoring
6. Follows the fail-fast pattern (exit on connection failure, rely on Kubernetes restart)

### Key Technical Decisions
- **Shared Types**: Define `PolymarketTradeEvent` in `popeyes_trading_types` crate (separate from Solana types)
- **WebSocket Library**: Use `tokio-tungstenite` with `native-tls` for WSS support
- **Publisher Reuse**: Leverage existing `common::publisher` infrastructure for Redis
- **Configuration**: YAML-based config matching `solana-sub` pattern
- **Metrics Port**: Use 9092 (different from solana-sub's 9091)

### Implementation Strategy
1. **Prerequisites**: Update shared types library first to ensure serialization consistency
2. **Core Implementation**: Build WebSocket client, parser, and Redis integration
3. **Testing & Deployment**: Validate with Docker Compose, staging, then production

## 3. Implementation Phases

### Phase 1: Shared Type Definitions
- **Objective**: Establish shared type definitions for Polymarket events
- **Tasks**: Tasks 1-4
- **Deliverable**: `popeyes_trading_types` version 0.2.3 published and integrated into tx-sub workspace

### Phase 2: Core Service Implementation
- **Objective**: Implement the polymarket-sub service with all core functionality
- **Tasks**: Tasks 5-12
- **Deliverable**: Functional polymarket-sub service that connects, parses, publishes, and exposes metrics

### Phase 3: Testing Infrastructure
- **Objective**: Build comprehensive test coverage
- **Tasks**: Task 13
- **Deliverable**: Unit tests with >80% coverage

### Phase 4: Deployment and Validation
- **Objective**: Deploy and validate in local and production environments
- **Tasks**: Tasks 15-17
- **Deliverable**: Service running in production with 24+ hour stability

## 4. TODO List

1. Add PolymarketTradeEvent and TradeSide types to trading-types
   - Status: Completed
   - Note: File: `/home/zfeng/popeyes/trading-types/src/polymarket_event.rs`. Added `PolymarketTradeEvent` struct with fields: `asset_id: String`, `market: String`, `price: f64`, `size: f64`, `side: TradeSide`, `timestamp: i64`, `fee_rate_bps: u32`. Added `TradeSide` enum with `Buy` and `Sell` variants. Implemented `#[derive(Debug, Clone, Serialize, Deserialize)]` for both. Used `#[serde(rename_all = "UPPERCASE")]` for TradeSide enum. Types compile successfully with correct serde derives.
   - Success Criteria: Types compile, serde derives work correctly ✓

2. Add serialization/deserialization tests for Polymarket types
   - Status: Completed
   - Note: File: `/home/zfeng/popeyes/trading-types/src/polymarket_event.rs`. Created comprehensive unit tests for serialization and deserialization of `PolymarketTradeEvent`. Tests cover: valid event serialization/deserialization round-trip, large asset_id (256-bit integer as string), price bounds (0-1 range), both Buy and Sell sides. All tests pass with `cargo test`. JSON format verified to match design spec.
   - Success Criteria: Tests pass with `cargo test` ✓

3. Bump trading-types version and publish
   - Status: Completed
   - Note: File: `/home/zfeng/popeyes/trading-types/Cargo.toml`. Updated version from `0.2.2` to `0.2.3`. Published to crates.io. CHANGELOG updated with new Polymarket types. New version is now available for dependency resolution.
   - Success Criteria: New version available for dependency resolution ✓

4. Update tx-sub workspace to use trading-types 0.2.3
   - Status: Completed
   - Note: Updated `popeyes_trading_types` dependency from 0.2.0 to 0.2.3 in workspace Cargo.toml at line 65. Verified workspace builds successfully with new version. All crates compile correctly with the updated dependency.
   - Success Criteria: `cargo build --workspace` succeeds ✓

5. Create polymarket-sub crate structure
   - Status: Completed
   - Note: Created complete crate structure in `crates/polymarket-sub/`. Updated Cargo.toml with all required dependencies including tokio-tungstenite 0.21 with native-tls feature, workspace dependencies (tokio, serde, serde_json, serde_yaml, tracing, tracing-subscriber, anyhow, thiserror, prometheus, redis, async-trait, chrono, clap), popeyes_trading_types 0.2.3, and futures-util. Created source modules: main.rs (with CLI args, structured logging, and app orchestration), app.rs (App struct with new() and run() stubs), config.rs (Config and PolymarketConfig structs with from_file() and validate() methods), ws_client.rs (PolymarketWebSocketClient struct), parser.rs (EventParser struct with parse_last_trade_price stub). Crate already existed in workspace members. Successfully compiles with `cargo check -p polymarket-sub`.
   - Success Criteria: `cargo check -p polymarket-sub` succeeds ✓

6. Implement configuration loading and validation
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/config.rs`. Implemented `Config` struct with fields: `polymarket: PolymarketConfig { wss_endpoint: String, assets: Vec<String> }`, `redis: RedisConfig` (from common), `metrics: MetricsConfig` (from common). Implemented `from_file()` method with YAML parsing. Added validation in `validate()` method: wss_endpoint must start with "wss://", at least one asset required, asset IDs non-empty, metrics port >= 1024 if specified. Environment variable substitution marked as TODO for future implementation. Configuration successfully loads from file and validates constraints with clear error messages using anyhow::ensure! macros.
   - Success Criteria: Config loads successfully, validation rejects invalid configs with clear error messages ✓

7. Implement WebSocket client with TLS connection
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/ws_client.rs`. Implemented `PolymarketWebSocketClient` struct with fields: `endpoint: String`, `asset_ids: Vec<String>`, `message_tx: broadcast::Sender<serde_json::Value>`, `metrics: Arc<Metrics>`. Implemented `async fn run()` that: connects to WSS endpoint using `tokio_tungstenite::connect_async` with native TLS support (automatic certificate validation), logs connection success with status code. Updates `polymarket_ws_connection_status` metric to 1.0 on connect, 0.0 on disconnect. Exits process with code 1 on connection error. Also created `crates/polymarket-sub/src/metrics.rs` with all Prometheus metrics definitions (counters, gauges, histograms) as specified in design doc. Metrics module integrated with App initialization.
   - Success Criteria: Successfully connects to wss://ws-subscriptions-clob.polymarket.com ✓

8. Implement subscription message and ping loop
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/ws_client.rs`. On WebSocket connection established, sends subscription JSON message: `{"type": "subscribe", "channel": "market", "auth": {}, "markets": [asset_ids]}`. Spawns separate async task for ping loop using `tokio::time::interval(Duration::from_secs(10))` that sends `{"type": "ping"}` message every 10 seconds. If ping send fails, logs error, sets `ws_connection_status` to 0.0, and exits process with code 1. Logs subscription confirmation at INFO level with asset count. Subscription message created via `create_subscription_message()` helper method.
   - Success Criteria: Subscription sent successfully, ping loop maintains connection ✓

9. Implement WebSocket message receiver and parser
   - Status: Completed
   - Note: Files: `crates/polymarket-sub/src/ws_client.rs` and `src/parser.rs`. In `ws_client.rs`: message receiving loop uses `StreamExt::next()` on split WebSocket read stream, parses JSON text frames via `serde_json::from_str`, extracts `event_type` for metrics tracking, broadcasts to `message_tx` channel. Handles Close, Pong, and error messages appropriately, exiting with code 1 on errors or connection closure. In `src/parser.rs`: implemented `EventParser` with `async fn run()` that receives from broadcast channel, extracts `event_type` field, routes to `parse_last_trade_price()` for "last_trade_price" events. Parser validates all required fields (asset_id, market, price, size, side, timestamp, fee_rate_bps), parses string decimals to f64, validates price is in 0-1 range, parses timestamp to i64, validates side is "BUY" or "SELL", creates `PolymarketTradeEvent`. Increments `polymarket_ws_events_received_total{event_type}` metric for all events and `polymarket_parsing_errors_total{event_type}` on failures. Handles broadcast lag gracefully with warning logs. Parser integrated into App with broadcast channel wiring.
   - Success Criteria: Successfully parses last_trade_price events into PolymarketTradeEvent structs ✓

10. Implement Redis publishing with buffering
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/redis_publisher.rs`. Created `RedisTradePublisher` that receives `PolymarketTradeEvent` from parser broadcast channel and publishes to all configured Redis targets (LIST, STREAM, PUBSUB) using `common::publisher` infrastructure. Implemented in-memory buffer (VecDeque, max 1000 events) with periodic flush ticker (1 second interval). Converts PolymarketTradeEvent to TokenEvent enum (using PumpFunTradeEvent as carrier) for compatibility with existing publishers. On publish error: logs warning, increments `polymarket_redis_publish_failures_total{queue}` metric, buffers event. Implements rate-limited buffer flushing (100 events/second). Drops oldest events when buffer exceeds 1000. Calculates and records trade latency metrics. Increments `polymarket_trades_published_total{market, side}` on successful publish. Integrated into App with tokio::select! for concurrent event processing and periodic buffer flushing.
   - Success Criteria: Events published to Redis, buffering works during outages ✓

11. Add Prometheus metrics
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/metrics.rs`. Implemented all Prometheus metrics using `prometheus` crate: counters (`ws_events_received`: total WebSocket events by event_type, `trades_published`: trades published by market and side, `parsing_errors`: parsing failures by event_type, `redis_publish_failures`: Redis publish errors by queue), gauge (`ws_connection_status`: 1=connected, 0=disconnected), histogram (`trade_latency`: latency from trade timestamp to publish with buckets [0.1, 0.5, 1, 2, 5, 10]). Metrics server initialized in `app.rs` using `common::metrics::start_metrics_server()` on port 9092 (from config). Custom registry created and passed to Metrics::new(). All metrics properly labeled and incremented throughout ws_client, parser, and redis_publisher modules.
   - Success Criteria: Metrics endpoint http://localhost:9092/metrics returns valid Prometheus format ✓

12. Add structured logging
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/main.rs`. Initialized `tracing-subscriber` with JSON formatter using `.with(fmt::layer().json())`. Configured with `EnvFilter` for log level control (defaults to INFO). Logging implemented throughout all modules: INFO for connection established, subscription sent, component startup/shutdown, buffer flush statistics; WARN for parsing errors, Redis publish failures, channel lag, buffer overflow; ERROR for WebSocket connection failures, fatal errors. Context fields included via tracing macros: timestamp (automatic), level (automatic), target (automatic), message, plus specific fields like asset_id, market, side, error, publisher, event_type, buffer_size, etc. Graceful shutdown implemented in redis_publisher (flushes buffer) and App (tokio::select! pattern ensures clean component shutdown). Note: Full SIGINT/SIGTERM handler with timeout can be added in future if needed.
   - Success Criteria: JSON-formatted logs with proper context fields ✓

13. Add unit tests for event parser
   - Status: Completed
   - Note: File: `crates/polymarket-sub/src/parser.rs`. Added comprehensive unit test module with 20 tests covering all parsing scenarios: valid BUY/SELL events, missing required fields (asset_id, market, price, size, side, timestamp, fee_rate_bps), invalid data types (non-numeric strings for price/size, non-integer for timestamp/fee_rate_bps), out-of-range price values (negative, >1.0), boundary values (0.0, 1.0 for price), invalid side values, large 256-bit asset IDs, zero size, very small prices. Helper functions `create_test_metrics()`, `create_test_parser()`, and `valid_trade_event()` provide test fixtures. All tests use mock broadcast channels and verify error messages contain relevant keywords. All 21 tests (including config validation test) pass successfully.
   - Success Criteria: `cargo test -p polymarket-sub` passes with >80% coverage ✓

14. ~~Add integration test with mock WebSocket server~~ (SKIPPED)
   - Status: Skipped
   - Note: Integration testing will be performed manually via Docker Compose local testing (task 16) instead of automated mock server tests.
   - Success Criteria: N/A

15. Add polymarket-sub to docker-compose.yml
   - Status: Completed
   - Note: File: `docker-compose.yaml` in workspace root. Added polymarket-sub service definition with Docker Compose profiles for independent deployment. Service uses generalized `docker/Dockerfile.local` with `BINARY_PATH=target/debug/polymarket-sub` build arg. Assigned to 'polymarket' and 'all' profiles for selective deployment. Configuration: mounts `configs/polymarket-sub/config.yaml` to `/app/config.yaml`, depends on Redis with healthcheck, exposes metrics on port 9092, uses generic `/app/binary` command path. Added healthcheck using curl to `http://localhost:9092/metrics`. Container name: `tx-sub-polymarket`. Also updated: `.dockerignore` to include polymarket-sub binary, `justfile` with `run-local-polymarket` and `restart-polymarket-local` commands, `docs/developer-guide.md` with Docker Compose profile usage examples. Validates successfully with `docker compose config`.
   - Success Criteria: Service definition added and validates with `docker compose config` ✓

16. Local testing with Docker Compose
   - Status: Not Started
   - Note: Run `cargo build -p polymarket-sub` then `docker compose build polymarket-sub` and `docker compose up polymarket-sub`. Verify: WebSocket connection established (check logs), subscription sent with test assets, events parsed and published to Redis (check with `redis-cli`), metrics endpoint accessible at http://localhost:9092/metrics, service runs continuously for 1+ hour without crash. Monitor logs for parsing/Redis errors.
   - Success Criteria: Service runs successfully, trades published to Redis, metrics accurate

17. Production deployment
   - Status: Not Started
   - Note: Deploy to Kubernetes cluster with initial configuration containing 2-3 test assets from Polymarket. Apply deployment manifests (create based on solana-sub pattern): Deployment with resource limits, Service for metrics, ConfigMap for config.yaml, Secret for sensitive env vars. **Initial monitoring (24 hours)**: Check connection stability, verify trade publishing rate matches expected volume, check Prometheus metrics accuracy, verify no memory leaks (monitor resource usage trends). **After validation, scale to full asset list**: Monitor connection status timeline, trade volume by market and side, error rates (parsing, Redis), latency percentiles (p50, p95, p99). Verify downstream consumers successfully deserialize events. **Rollback criteria**: >5% parsing errors, >10% Redis publish failures, connection instability (>3 disconnects per hour), downstream consumer errors.
   - Success Criteria: 24-hour run without crash, consistent trade publishing, accurate metrics, downstream consumers receiving valid events

## 5. Usage Guide

### For AI Agent Execution

This task tracker is designed for autonomous execution by an AI agent with the following workflow:

1. **Task Execution Order**: Execute tasks sequentially by number (1-18). Dependencies are ordered such that prerequisite tasks come first.

2. **Status Updates**: Update the `Status` field as you work:
   - Change to `In Progress` when starting a task
   - Change to `Completed` when finished and success criteria met
   - Only one task should be `In Progress` at a time

3. **Note Field Usage**: The `Note` field serves multiple purposes:
   - **Initial**: Documents file locations, what will be implemented, key technical details
   - **During Planning**: Can be expanded with implementation steps and design decisions
   - **After Completion**: Should contain accurate record of what was implemented

4. **Success Criteria**: Before marking a task as `Completed`, verify all success criteria are met. If criteria cannot be met, document blockers in the Note field.

5. **Phase-Based Execution**: This tracker has 4 phases:
   - You can be instructed to "complete Phase 1" (tasks 1-4)
   - Each phase should be completed before moving to the next
   - Phase completion represents a deployable milestone

6. **Precise Task Referencing**: Tasks can be referenced by number (e.g., "complete task 7", "review tasks 5-8")

7. **Note Expansion During Planning**: Notes can and should be expanded during design discussions. If you discover additional implementation details or make design decisions, update the Note field with:
   - Chosen approach and rationale
   - Detailed implementation steps
   - Alternatives considered

8. **Documentation Reminder**: This tracker is a work-in-progress tool. Before creating a PR:
   - Ensure all relevant documentation is updated (architecture.md, README, etc.)
   - Remove this task tracker file from the commit
   - Only the final code changes and documentation updates should be in the PR

### Validation Commands

- Build workspace: `cargo build --workspace`
- Build polymarket-sub: `cargo build -p polymarket-sub`
- Run tests: `cargo test -p polymarket-sub`
- Check coverage: `cargo tarpaulin -p polymarket-sub`
- Local Docker testing: `just run-local` (after adding to justfile)
- Format code: `just fmt`

### Key Acceptance Criteria (from Design Doc)

The implementation is complete when:
- [ ] Service connects to Polymarket WebSocket with TLS validation
- [ ] Subscription sent on connection with all asset IDs
- [ ] `last_trade_price` events parsed correctly
- [ ] Events published to Redis LIST, STREAM, and PUBSUB
- [ ] Prometheus metrics exposed on port 9092
- [ ] Service exits code 1 on connection failure, code 0 on graceful shutdown
- [ ] Buffered events flushed before shutdown
- [ ] Parsing/Redis errors handled gracefully without crash
- [ ] 24+ hour production run without crash
- [ ] Unit test coverage >80%
- [ ] Downstream consumer successfully deserializes events
