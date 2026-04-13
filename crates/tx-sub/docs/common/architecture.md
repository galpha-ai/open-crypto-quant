# Common Crate Architecture

## Overview

The `common` crate is a shared infrastructure library that provides reusable components for all subscribers in the `popeyes-tx-sub` workspace. It abstracts Redis publishing/subscribing patterns, configuration types, metrics serving, and shared data types.

This crate is part of the `popeyes-tx-sub` workspace. For workspace-level architecture decisions, see [docs/specs/002-multi-chain-workspace-refactoring/design.md](../specs/002-multi-chain-workspace-refactoring/design.md).

## Crate Structure

```
crates/common/
└── src/
    ├── lib.rs              # Module exports
    ├── config.rs           # Redis and metrics configuration types
    ├── metrics.rs          # Prometheus metrics HTTP server
    ├── types.rs            # Shared type re-exports
    ├── publisher/          # Redis publishing implementations
    │   ├── mod.rs          # Module exports
    │   ├── traits.rs       # RedisPublisher trait
    │   ├── redis_list_publisher.rs
    │   ├── redis_stream_publisher.rs
    │   └── redis_pubsub_publisher.rs
    └── subscriber/         # Redis subscribing implementations
        ├── mod.rs          # Module exports
        ├── config.rs       # Subscriber-specific configs
        ├── traits.rs       # RedisSubscriber trait, ReceivedEvent
        ├── redis_list_subscriber.rs
        ├── redis_stream_subscriber.rs
        └── redis_pubsub_subscriber.rs
```

## Components

### 1. Configuration (`config.rs`)

Provides configuration structures for Redis connections and metrics:

**RedisConfig**
- Main Redis configuration container
- Supports multiple target types simultaneously:
  - `url`: Redis connection URL
  - `queues`: List of `QueueConfig` for Redis LIST targets
  - `streams`: Optional list of `StreamConfig` for Redis STREAM targets
  - `pubsub`: Optional `PubsubConfig` for Redis PUBSUB channels

**QueueConfig**
- `name`: Queue name
- `max_length`: Maximum queue size (enforced via LTRIM)

**StreamConfig**
- `name`: Stream name
- `max_length`: Optional MAXLEN for automatic trimming
- `consumer_group`: Consumer group name for subscribers

**PubsubConfig**
- `channels`: List of channel names

**MetricsConfig**
- `port`: Optional Prometheus metrics server port
- Falls back to `METRICS_PORT` env var or default 9091

### 2. Metrics (`metrics.rs`)

**start_metrics_server()**
- Spawns HTTP server on `/metrics` endpoint using Warp
- Serves Prometheus text format metrics
- Port resolution: explicit port > `METRICS_PORT` env var > 9091
- Binds to `0.0.0.0` (all interfaces)
- Returns `JoinHandle<()>` for async management

### 3. Types (`types.rs`)

Re-exports shared types for convenience:
- `TokenEvent` from `popeyes_trading_types`

This avoids requiring downstream crates to directly depend on `popeyes_trading_types`.

### 4. Publisher Module (`publisher/`)

Provides three Redis publishing implementations, all implementing a common trait.

#### RedisPublisher Trait (`traits.rs`)

```rust
#[async_trait]
pub trait RedisPublisher: Send + Sync {
    async fn publish(&self, event: Event) -> Result<()>;
    async fn publish_raw(&self, json: String) -> Result<()>;
    fn name(&self) -> &str;
}
```

- **publish()**: Asynchronous method to publish an `Event`
- **publish_raw()**: Publish a pre-serialized JSON string directly (useful for events that don't fit the Event enum)
- **name()**: Returns publisher name for logging/metrics
- Trait is object-safe and thread-safe (`Send + Sync`)

#### RedisListPublisher (`redis_list_publisher.rs`)

Publishes to Redis LIST queues using LPUSH + LTRIM pattern.

**Behavior:**
- Serializes `TokenEvent` to JSON
- Uses Redis pipeline for efficiency
- For each configured queue:
  - `LPUSH queue_name <json>` - adds to left (newest)
  - `LTRIM queue_name 0 <max_length-1>` - enforces size limit
- Pipeline executes atomically
- Oldest items removed from right when limit exceeded

#### RedisStreamPublisher (`redis_stream_publisher.rs`)

Publishes to Redis STREAM using XADD.

**Behavior:**
- Serializes `TokenEvent` to JSON
- For each configured stream:
  - `XADD stream [MAXLEN ~ max_length] * data <json> event_type <type> timestamp <rfc3339>`
  - Auto-generated entry ID (`*`)
  - Approximate trimming (`~`) for performance
  - Includes metadata fields: `event_type`, `timestamp`

**Event Type Mapping:**

The top-level `Event` enum has two variants:
- `Event::Token(TokenEvent)` - Token lifecycle and trade events
- `Event::MarketData(MarketDataEvent)` - Market data events

TokenEvent variants:
- `TokenEvent::Create` → "create"
- `TokenEvent::Buy` → "buy"
- `TokenEvent::Sell` → "sell"
- `TokenEvent::Swap` → "swap"

MarketDataEvent variants:
- `MarketDataEvent::OrderbookUpdate` → "orderbook_update"
- `MarketDataEvent::OrderbookSnapshot` → "orderbook_snapshot"
- `MarketDataEvent::SpotPrice` → "spot_price"
- `MarketDataEvent::PolymarketTrade` → "polymarket_trade"

#### RedisPubsubPublisher (`redis_pubsub_publisher.rs`)

Publishes to Redis PUBSUB channels using PUBLISH.

**Behavior:**
- Serializes `TokenEvent` to JSON
- For each configured channel:
  - `PUBLISH channel <json>`
- Fire-and-forget semantics (no persistence)
- Returns number of subscribers who received message

### 5. Subscriber Module (`subscriber/`)

Provides three Redis consuming implementations for reading `TokenEvent` data.

#### Subscriber Configuration (`subscriber/config.rs`)

**ListSubscriberConfig**
- `name`: Queue name
- `timeout_secs`: BRPOP timeout (0 = block forever)

**StreamSubscriberConfig**
- `name`: Stream name
- `consumer_group`: Consumer group name
- `consumer_name`: Optional (defaults to `hostname-pid`)
- `block_ms`: XREADGROUP block timeout (default: 5000)
- `count`: Max entries per read (default: 10)

**PubsubSubscriberConfig**
- `channels`: List of channel names

#### ReceivedEvent (`traits.rs`)

```rust
pub struct ReceivedEvent {
    pub event: TokenEvent,
    pub stream_id: Option<String>,
    pub source: String,
}
```

- `event`: Parsed TokenEvent
- `stream_id`: Entry ID for stream acknowledgment (None for list/pubsub)
- `source`: Queue/stream/channel name

#### RedisSubscriber Trait (`traits.rs`)

```rust
#[async_trait]
pub trait RedisSubscriber: Send + Sync {
    async fn subscribe(&self) -> Result<Pin<Box<dyn Stream<Item = Result<ReceivedEvent>> + Send + '_>>>;
    async fn acknowledge(&self, event: &ReceivedEvent) -> Result<()>;
    fn name(&self) -> &str;
}
```

- **subscribe()**: Returns async stream of `ReceivedEvent` results
- **acknowledge()**: Acknowledges event processing (required for streams)
- **name()**: Returns subscriber name for logging

#### RedisListSubscriber (`redis_list_subscriber.rs`)

Consumes from Redis LIST using BRPOP (blocking right pop).

**Behavior:**
- Continuously calls `BRPOP queue_name timeout`
- Deserializes JSON to `TokenEvent`
- At-most-once delivery (message removed on pop)
- No acknowledgment needed (removal is atomic)
- On timeout, loop continues

#### RedisStreamSubscriber (`redis_stream_subscriber.rs`)

Consumes from Redis STREAM using XREADGROUP with consumer groups.

**Behavior:**
- Creates consumer group on startup: `XGROUP CREATE stream group $ MKSTREAM`
- Continuously calls: `XREADGROUP GROUP group consumer COUNT count BLOCK block_ms STREAMS stream >`
- Parses entries and yields `ReceivedEvent` with `stream_id`
- At-least-once delivery (requires acknowledgment)
- Auto-acknowledges unparseable messages to prevent redelivery

**acknowledge():**
- Executes `XACK stream group entry_id`
- Logs warning if already acknowledged

#### RedisPubsubSubscriber (`redis_pubsub_subscriber.rs`)

Consumes from Redis PUBSUB channels using SUBSCRIBE.

**Behavior:**
- Creates new connection per subscription (pubsub requirement)
- Subscribes to all configured channels
- Continuously receives messages via async pubsub interface
- Fire-and-forget (no persistence, no acknowledgment)
- Real-time only (missed if no subscribers connected)

## Data Flow

### Publishing Flow

```
TokenEvent
    |
    v
RedisPublisher.publish()
    |
    +----------------+----------------+
    |                |                |
    v                v                v
RedisListPublisher  RedisStreamPublisher  RedisPubsubPublisher
    |                |                |
    v                v                v
LPUSH + LTRIM       XADD              PUBLISH
    |                |                |
    v                v                v
Redis LIST          Redis STREAM      Redis PUBSUB
```

### Subscribing Flow

```
Redis LIST/STREAM/PUBSUB
    |
    v
RedisSubscriber.subscribe()
    |
    v
async Stream<ReceivedEvent>
    |
    v
Application Processing
    |
    v
RedisSubscriber.acknowledge()
    |
    v
XACK (streams only)
```

## Delivery Guarantees

| Type | Delivery | Persistence | Acknowledgment | Use Case |
|------|----------|-------------|----------------|----------|
| List | At-most-once | Yes | Automatic (BRPOP) | Simple queues |
| Stream | At-least-once | Yes | Manual (XACK) | Reliable processing |
| Pubsub | Fire-and-forget | No | None | Real-time broadcast |

## Key Design Patterns

1. **Trait-Based Polymorphism**: All publishers/subscribers implement common traits for uniform usage
2. **Multi-Target Publishing**: Single publisher can target multiple queues/streams/channels
3. **Async Streams**: Subscribers return async streams for natural async iteration
4. **Connection Pooling**: Uses Redis `ConnectionManager` for efficient connections
5. **Graceful Error Handling**: Comprehensive error context with `anyhow`
6. **Pipeline Operations**: List publisher uses Redis pipeline for atomic multi-command execution

## Usage Example

```rust
use common::publisher::{RedisListPublisher, RedisStreamPublisher, RedisPublisher};
use common::subscriber::{RedisStreamSubscriber, RedisSubscriber, StreamSubscriberConfig};
use common::config::{QueueConfig, StreamConfig};
use common::metrics::start_metrics_server;
use futures::StreamExt;

// Publishing
let list_publisher = RedisListPublisher::new(
    "redis://localhost:6379",
    vec![QueueConfig { name: "trades".into(), max_length: 10000 }]
).await?;

let stream_publisher = RedisStreamPublisher::new(
    "redis://localhost:6379",
    vec![StreamConfig { name: "trades".into(), max_length: Some(10000), consumer_group: None }]
).await?;

list_publisher.publish(token_event.clone()).await?;
stream_publisher.publish(token_event).await?;

// Subscribing
let subscriber = RedisStreamSubscriber::new(
    "redis://localhost:6379",
    StreamSubscriberConfig {
        name: "trades".into(),
        consumer_group: "my-group".into(),
        consumer_name: None,
        block_ms: Some(5000),
        count: Some(10),
    }
).await?;

let mut stream = subscriber.subscribe().await?;
while let Some(result) = stream.next().await {
    let received = result?;
    // Process received.event
    subscriber.acknowledge(&received).await?;
}

// Metrics
let registry = prometheus::Registry::new();
start_metrics_server(registry, Some(9090));
```

## Performance Considerations

- **Connection Pooling**: Redis `ConnectionManager` reuses connections efficiently
- **Pipeline Operations**: List publisher batches LPUSH + LTRIM atomically
- **Approximate Trimming**: Stream publisher uses `MAXLEN ~` for better performance
- **Blocking Reads**: Subscribers use blocking reads to avoid polling
- **Auto-Generated Consumer Names**: Stream subscriber generates unique names to prevent conflicts
