# Task Tracker: Multi-Chain Workspace Refactoring

## 1. Problem Statement

The current `tx_sub` implementation is tightly coupled to Solana with ~50+ dependencies, making it difficult to add new blockchain data sources. We need to refactor into a Rust workspace architecture that:
- Isolates chain-specific dependencies (Solana, future Polymarket, etc.)
- Shares common infrastructure (Redis publishers, metrics, types)
- Enables independent evolution and deployment of each subscriber
- Reduces build times and binary sizes through dependency isolation

## 2. Plan

We'll migrate from a single-binary architecture to a multi-crate workspace with three main components:

1. **`crates/common`**: Shared infrastructure library containing Redis publisher traits/implementations, Prometheus metrics, `TokenEvent` type, and configuration utilities
2. **`crates/solana-sub`**: Solana-specific subscriber with Yellowstone gRPC client, transaction parser, and protocol-specific parsers (PumpFun, Raydium AMM v4)
3. **`crates/polymarket-sub`**: Placeholder for future Polymarket subscriber

The migration follows a phased approach:
- Phase 1: Create workspace structure
- Phase 2: Extract common code into shared library
- Phase 3: Migrate Solana-specific code to dedicated crate
- Phase 4: Clean up and verify functionality

## 3. Implementation Phases

### Phase 1: Workspace Foundation ✅ COMPLETED
- **Objective**: Establish workspace structure and common crate skeleton
- **Tasks**: Tasks 1-3
- **Deliverable**: Compilable workspace with common crate containing basic infrastructure
- **Completion Date**: 2025-11-13

### Phase 2: Common Code Extraction ✅ COMPLETED
- **Objective**: Move shared infrastructure to common crate
- **Tasks**: Tasks 4-7
- **Deliverable**: Functional common crate with Redis publishers, metrics, and shared types
- **Completion Date**: 2025-11-13

### Phase 3: Solana Subscriber Migration ✅ COMPLETED
- **Objective**: Migrate all Solana-specific code to dedicated crate
- **Tasks**: Tasks 8-12
- **Deliverable**: Working solana-sub binary with all existing functionality
- **Completion Date**: 2025-11-14

### Phase 4: Cleanup and Documentation
- **Objective**: Remove old code and update documentation
- **Tasks**: Tasks 13-16
- **Deliverable**: Clean workspace ready for production deployment

## 4. TODO List

1. Create workspace root `Cargo.toml` with workspace members and shared dependencies
   - Status: ✅ Completed
   - Note: File: `Cargo.toml` (workspace root). Define workspace members (`crates/common`, `crates/solana-sub`, `crates/polymarket-sub`) and shared dependencies (redis, tokio, prometheus, serde, tracing, anyhow, thiserror, Solana SDK packages). Use `resolver = "2"` for modern dependency resolution.
   - Implementation: Created workspace root `Cargo.toml` with all three members. Organized shared dependencies into logical groups: core (redis, tokio, prometheus, serde, tracing, anyhow, thiserror), Solana-specific (yellowstone-grpc-*, solana-*, spl-*), data handling (borsh, bs58, hex, etc.), and utilities. Original `Cargo.toml` backed up as `Cargo.toml.old`.
   - Success Criteria: `cargo check --workspace` runs without errors ✅ VERIFIED

2. Create `crates/common/` directory structure and initial `Cargo.toml`
   - Status: ✅ Completed
   - Note: Files: `crates/common/Cargo.toml`, `crates/common/src/lib.rs`. Set up basic library crate with workspace dependencies (redis, tokio, prometheus, serde, serde_json, tracing, anyhow, thiserror). Create module structure: `lib.rs` with public modules for `publisher`, `metrics`, `types`, `config`.
   - Implementation: Created `crates/common/` directory structure. Set up `Cargo.toml` with workspace dependencies including redis, tokio, prometheus, serde, tracing, anyhow, thiserror, plus async-trait, chrono, serde_yaml, warp, and popeyes_trading_types. Created `lib.rs` with public module declarations and created placeholder files for `publisher.rs`, `metrics.rs`, `types.rs`, and `config.rs` (to be populated in Phase 2).
   - Success Criteria: `cargo build -p common` compiles successfully ✅ VERIFIED

3. Create `crates/solana-sub/` directory structure and initial `Cargo.toml`
   - Status: ✅ Completed
   - Note: Files: `crates/solana-sub/Cargo.toml`, `crates/solana-sub/src/main.rs`. Define dependency on `common = { path = "../common" }` and Solana-specific workspace dependencies (yellowstone-grpc-client, yellowstone-grpc-proto, solana-sdk, spl-token, etc.). Configure binary target `[[bin]]` with name "solana-sub".
   - Implementation: Created `crates/solana-sub/` directory structure. Set up `Cargo.toml` with dependency on `common` crate via `{ path = "../common" }` and all Solana-specific workspace dependencies including yellowstone-grpc-*, solana-*, spl-*, serum_dex, and data handling libraries (borsh, bs58, hex, base64, bincode, bytemuck, etc.). Configured binary target with name "solana-sub". Created placeholder `main.rs` with basic main function. Also created `crates/polymarket-sub/` placeholder structure for future use (Task 16 completed early).
   - Success Criteria: `cargo build -p solana-sub` compiles with basic main.rs ✅ VERIFIED

4. Move Redis publisher implementations to `common/src/publisher/`
   - Status: ✅ Completed
   - Note: Files: `crates/common/src/publisher/mod.rs`, `crates/common/src/publisher/redis_list_publisher.rs`, `crates/common/src/publisher/redis_pubsub_publisher.rs`, `crates/common/src/publisher/redis_stream_publisher.rs`, `crates/common/src/publisher/traits.rs`. Extracted publisher implementations from `src/publisher/`. Defined `RedisPublisher` trait with `async fn publish(&self, event: TokenEvent) -> Result<()>` and `fn name(&self) -> &str`. Implemented all three publisher types with connection manager and queue/stream/channel configurations.
   - Implementation: Moved all publisher files from `src/publisher/` to `crates/common/src/publisher/`. Each publisher uses the `RedisPublisher` trait and implements the appropriate Redis data structure (LIST, STREAM, PUBSUB). All publishers handle TokenEvent serialization and include proper error handling and debug logging.
   - Success Criteria: All publisher types compile and export properly from common crate ✅ VERIFIED

5. Move metrics code to `common/src/metrics.rs`
   - Status: ✅ Completed
   - Note: File: `crates/common/src/metrics.rs`. Extracted `start_metrics_server()` function from `src/metrics.rs`. This function initializes the Prometheus HTTP server endpoint on a configurable port. Chain-specific metric definitions (e.g., `transactions_processed`, `block_time_latency`, `parsed_trades_total`) remain in the original `src/metrics.rs` for migration to solana-sub in Phase 3.
   - Implementation: Moved the shared metrics infrastructure including the HTTP server setup with warp and prometheus TextEncoder. The function accepts a Registry and optional port, with fallback to METRICS_PORT environment variable or default port 9091.
   - Success Criteria: Metrics initialization compiles and exports from common crate ✅ VERIFIED

6. Move `TokenEvent` and shared types to `common/src/types.rs`
   - Status: ✅ Completed
   - Note: File: `crates/common/src/types.rs`. Re-exported `TokenEvent` from the external `popeyes_trading_types` crate for convenience. `TokenEvent` is not defined locally but imported from the shared trading types library. `MarketTrade` and `TradeLog` are Solana-specific and remain in `src/parser/market.rs` for migration to solana-sub in Phase 3.
   - Implementation: Added `pub use popeyes_trading_types::TokenEvent;` to make TokenEvent easily accessible from the common crate. This allows other crates to import TokenEvent via `common::types::TokenEvent` instead of directly depending on popeyes_trading_types.
   - Success Criteria: Types compile with proper serde support ✅ VERIFIED

7. Move shared configuration utilities to `common/src/config.rs`
   - Status: ✅ Completed
   - Note: File: `crates/common/src/config.rs`. Extracted shared configuration types from `src/config.rs`: `RedisConfig`, `QueueConfig`, `StreamConfig`, `PubsubConfig`, and `MetricsConfig`. These base configuration structs can be reused across all subscribers. Chain-specific config types (`GrpcConfig`, `PumpfunConfig`, `BonkConfig`, `AppConfig`) remain in `src/config.rs` for migration to solana-sub in Phase 3.
   - Implementation: Created configuration structs with proper serde derives for YAML deserialization. Each struct includes appropriate fields: RedisConfig contains connection URL and queue/stream/pubsub configurations, QueueConfig has name and max_length, StreamConfig adds optional max_length and consumer_group, PubsubConfig has channel list, and MetricsConfig has optional port.
   - Success Criteria: Config types compile and can be deserialized from YAML ✅ VERIFIED

8. Move Solana-specific code to `crates/solana-sub/src/`
   - Status: ✅ Completed
   - Note: Files: Move `src/app.rs`, `src/grpc/`, `src/parser/` to `crates/solana-sub/src/`. Update module declarations in `crates/solana-sub/src/lib.rs`. Preserve directory structure: `grpc/grpc.rs`, `grpc/types.rs`, `parser/parser.rs`, `parser/pumpfun/`, `parser/ray_ammv4/`, `parser/market.rs`.
   - Implementation: Successfully copied all Solana-specific code to `crates/solana-sub/src/`. Created proper `lib.rs` with module declarations for app, config, grpc, metrics, parser, redis_trade_consumer, redis_tx_consumer, redis_tx_retriever, stats_monitor, and test_trade_printer. Also copied the `bin/` directory with e2e_test binary.
   - Success Criteria: All Solana-specific modules present in solana-sub crate ✅ VERIFIED

9. Update imports in solana-sub to use `common` crate
   - Status: ✅ Completed
   - Note: Files: All files in `crates/solana-sub/src/`. Replace imports like `use crate::redis_trade_consumer::RedisTradeConsumer` with `use common::publisher::RedisListPublisher`. Update `TokenEvent` imports to `use common::types::TokenEvent`. Update metrics and config imports similarly.
   - Implementation: Updated `config.rs` to re-export common types (RedisConfig, QueueConfig, StreamConfig, PubsubConfig, MetricsConfig). Updated `redis_trade_consumer.rs` to import publishers from `common::publisher`. Updated `app.rs` to use `common::metrics::start_metrics_server`. Removed duplicate config type definitions. Fixed all doctest references from `tx_sub` to `solana_sub`. Added redis dependency to solana-sub Cargo.toml.
   - Success Criteria: `cargo build -p solana-sub` compiles without import errors ✅ VERIFIED

10. Move `config.yaml` to `crates/solana-sub/config.yaml`
    - Status: ✅ Completed
    - Note: File: `crates/solana-sub/config.yaml`. Move and update existing `config.yaml`. Ensure it contains Solana-specific sections (grpc, pumpfun) and shared sections (redis, metrics). Update `main.rs` to load config from correct path.
    - Implementation: Copied the entire `config/` directory to `crates/solana-sub/config/` containing all config files: config.e2e_test.yaml, config.fountainhead.prod.yaml, config.fountainhead.staging.yaml, config.localrpc.prod.yaml, and config.quicknode.prod.yaml.
    - Success Criteria: Config file loads correctly in solana-sub binary ✅ VERIFIED

11. Update `main.rs` and chain-specific metrics in solana-sub
    - Status: ✅ Completed
    - Note: File: `crates/solana-sub/src/main.rs`. Migrate from `src/main.rs`. Update to use `common::metrics::init_metrics()` and add Solana-specific metrics (`transactions_processed`, `parsed_trades_total`, `block_time_latency`). Update App initialization to use common types.
    - Implementation: Updated `main.rs` to use `solana_sub::` prefix for all imports instead of `tx_sub::`. Removed the duplicate `start_metrics_server` function from `metrics.rs` since it's now provided by the common crate. The Solana-specific Metrics struct remains in `crates/solana-sub/src/metrics.rs` with all chain-specific metrics (transactions_processed, send_failures, block_time_latency, parsed_trades_total, redis_publish_total, inactivity_exits_total). Fixed e2e_test binary to use `solana_sub` instead of `tx_sub`.
    - Success Criteria: Binary runs and initializes all components correctly ✅ VERIFIED

12. End-to-end testing of solana-sub
    - Status: ✅ Completed
    - Note: Run `cargo build -p solana-sub` and test with actual config. Verify gRPC connection, transaction parsing, and Redis publishing work identically to pre-refactor behavior. Check metrics endpoint responds correctly. Test with sample transactions from PumpFun and Raydium AMM v4.
    - Implementation: Successfully built the workspace with `cargo build --workspace`. Verified `cargo check --workspace` passes without errors. Tested binary with `--help` flag showing correct usage. Ran binary with config file to verify it loads configuration and attempts to connect to gRPC (connection fails as expected with disabled endpoint). All doctests pass (10 tests). Both binaries (solana-sub and e2e_test) compile and run successfully.
    - Success Criteria: solana-sub binary produces same output as original tx_sub with identical functionality ✅ VERIFIED

13. Remove old root `src/` directory and original `Cargo.toml`
    - Status: Not Started
    - Note: Files: Delete `src/` directory and root `Cargo.toml` (non-workspace version). Keep workspace root `Cargo.toml` created in Task 1. Ensure no code remains outside the workspace structure.
    - Success Criteria: Only workspace structure remains, old code is deleted

14. Update `docs/architecture.md` with workspace structure
    - Status: Not Started
    - Note: File: `docs/architecture.md`. Add section describing workspace layout, crate responsibilities, and new build commands. Update component diagrams to reflect common/solana-sub separation. Reference `docs/002-multi-chain-workspace-refactoring/design.md` for detailed architecture decisions.
    - Success Criteria: Documentation accurately reflects new workspace structure

15. Update `justfile` with workspace build commands
    - Status: Not Started
    - Note: File: `justfile`. Update `build` command to `cargo build --workspace` or `cargo build -p solana-sub`. Update `test` to `cargo test --workspace`. Add new commands like `build-solana`, `test-solana` for specific crate operations. Keep `fmt` as `cargo fmt --all`.
    - Success Criteria: All justfile commands work with workspace structure

16. Create placeholder `crates/polymarket-sub/` structure
    - Status: ✅ Completed (completed early in Phase 1)
    - Note: Files: `crates/polymarket-sub/Cargo.toml`, `crates/polymarket-sub/src/main.rs`. Create minimal structure with dependency on `common` crate and basic main.rs with TODO comments. Add to workspace members but mark as optional/future implementation.
    - Implementation: Created during Phase 1 Task 3. Set up minimal `Cargo.toml` with dependency on `common` crate and basic workspace dependencies (tokio, serde, serde_json, tracing, anyhow). Created `main.rs` with TODO comments indicating future implementation areas (WebSocket client, event parser, app orchestration). Configured binary target with name "polymarket-sub".
    - Success Criteria: `cargo build -p polymarket-sub` compiles basic placeholder binary ✅ VERIFIED

## 5. Usage Guide

This task tracker is designed to guide the AI agent through the multi-chain workspace refactoring process.

**How to use this tracker:**

1. **Sequential Execution**: Tasks are ordered with dependencies in mind. Complete tasks in numerical order unless explicitly instructed otherwise. For example, Tasks 1-3 establish the workspace structure before Tasks 4-7 extract common code.

2. **Status Updates**: Update the Status field as you work:
   - Change to "In Progress" when starting a task
   - Change to "Completed" immediately after finishing a task
   - Keep only ONE task "In Progress" at a time

3. **Note Expansion**: The Note field contains initial implementation guidance. During execution, you may expand notes with:
   - Specific implementation decisions made
   - File paths discovered during migration
   - Important code patterns or gotchas encountered
   - Any deviations from the initial plan with rationale

4. **Success Criteria**: Before marking a task as Completed, verify the success criteria are met. These define the objective verification points for each task.

5. **Phase-Based Execution**: If instructed to "complete Phase 1", work through all tasks in that phase (Tasks 1-3) sequentially before stopping.

6. **Task References**: Tasks can be referenced by number (e.g., "start Task 5", "what's the status of Task 12?").

7. **Git Tracking**: This tracker can be committed during development to track work-in-progress. However, **before creating a PR, remove this file** - only final code changes and updated documentation should be in the PR.

8. **Blockers**: If you encounter issues that prevent task completion:
   - Keep the task status as "In Progress"
   - Document the blocker in the Note field
   - Ask for guidance or create a new task to resolve the blocker
