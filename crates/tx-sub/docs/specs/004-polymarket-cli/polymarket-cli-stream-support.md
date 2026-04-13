# Polymarket CLI WebSocket Streaming Support

## Overview

Add a `stream` command to the Polymarket CLI for debugging and monitoring real-time orderbook updates from the Polymarket WebSocket market channel. This is a lightweight debugging tool that outputs JSON events as they occur.

## Goals

- Subscribe to real-time market data via WebSocket
- Stream JSON-formatted events to stdout
- Filter by event type
- Support multiple asset subscriptions
- Graceful connection handling and shutdown

## Non-Goals

- Database persistence (simplified for debugging)
- Formatted table views (JSON output only)
- Historical data replay
- User channel authentication (market channel only)

## Command Structure

```bash
polymarket-cli stream <asset-ids> [options]
```

### Arguments

- `<asset-ids>`: Comma-separated list of asset IDs to subscribe to

### Options

- `--event-types <types>`: Comma-separated event types to filter (default: all)
  - Valid types: `book`, `price_change`, `tick_size_change`, `last_trade_price`
- `--log-file <path>`: Write stream output to file (in addition to stdout)
- `--verbose`: Enable verbose logging (connection status, ping/pong)
- `--no-ping`: Disable automatic ping messages (default: ping every 10s)

### Examples

```bash
# Stream single asset (all events)
polymarket-cli stream 109681959945973826496234384791167033612800000000000000000000000376

# Stream multiple assets
polymarket-cli stream 109681959945973...376,521816198488129...422

# Filter specific event types
polymarket-cli stream 109681959945973...376 --event-types book,last_trade_price

# Log to file and stdout
polymarket-cli stream 109681959945973...376 --log-file ./market-stream.jsonl

# Verbose mode for debugging connection issues
polymarket-cli stream 109681959945973...376 --verbose
```

## Architecture

### Component Structure

```
polycli/src/
├── commands/
│   └── stream/
│       └── index.ts              # Stream command implementation
├── stream/
│   ├── websocket-client.ts       # WebSocket connection manager
│   ├── types.ts                  # Message type definitions
│   └── logger.ts                 # Stream output logger
└── index.ts                      # Add stream command to CLI
```

### 1. WebSocket Client (`src/stream/websocket-client.ts`)

```typescript
interface WebSocketClientOptions {
  url: string;
  assetIds: string[];
  onMessage: (message: MarketMessage) => void;
  onError: (error: Error) => void;
  onConnect?: () => void;
  onDisconnect?: () => void;
  enablePing?: boolean;
  pingIntervalMs?: number;
}

class PolymarketWebSocketClient {
  private ws: WebSocket;
  private pingInterval?: NodeJS.Timeout;
  private reconnectAttempts: number = 0;
  private maxReconnectAttempts: number = 5;

  constructor(options: WebSocketClientOptions);

  /**
   * Connect to WebSocket and send subscription message
   */
  connect(): void;

  /**
   * Send subscription message for market channel
   * Format: {"assets_ids": ["..."], "type": "market"}
   */
  private subscribe(): void;

  /**
   * Start ping loop (send "PING" every 10 seconds)
   */
  private startPingLoop(): void;

  /**
   * Handle incoming messages
   */
  private handleMessage(data: string): void;

  /**
   * Reconnect with exponential backoff
   */
  private reconnect(): void;

  /**
   * Gracefully disconnect
   */
  disconnect(): void;
}
```

**Key Behaviors:**
- Connect to `wss://ws-subscriptions-clob.polymarket.com/ws/market`
- Send subscription on connection: `{"assets_ids": [...], "type": "market"}`
- Ping every 10 seconds (unless `--no-ping`)
- Auto-reconnect on disconnect (max 5 attempts with exponential backoff: 1s, 2s, 4s, 8s, 16s)
- Handle SIGINT/SIGTERM for graceful shutdown

### 2. Message Types (`src/stream/types.ts`)

Based on Polymarket WebSocket documentation:

```typescript
type MarketEventType = 'book' | 'price_change' | 'tick_size_change' | 'last_trade_price';

interface OrderSummary {
  price: string;
  size: string;
}

interface BookMessage {
  event_type: 'book';
  asset_id: string;
  market: string;
  timestamp: string;
  hash: string;
  bids: OrderSummary[];
  asks: OrderSummary[];
}

interface PriceChange {
  asset_id: string;
  price: string;
  size: string;
  side: 'BUY' | 'SELL';
  hash: string;
  best_bid: string;
  best_ask: string;
}

interface PriceChangeMessage {
  event_type: 'price_change';
  market: string;
  price_changes: PriceChange[];
  timestamp: string;
}

interface TickSizeChangeMessage {
  event_type: 'tick_size_change';
  asset_id: string;
  market: string;
  old_tick_size: string;
  new_tick_size: string;
  timestamp: string;
}

interface LastTradePriceMessage {
  event_type: 'last_trade_price';
  asset_id: string;
  market: string;
  price: string;
  side: 'BUY' | 'SELL';
  size: string;
  fee_rate_bps: string;
  timestamp: string;
}

type MarketMessage =
  | BookMessage
  | PriceChangeMessage
  | TickSizeChangeMessage
  | LastTradePriceMessage;
```

### 3. Stream Logger (`src/stream/logger.ts`)

```typescript
interface StreamLoggerOptions {
  logFile?: string;
  eventTypes?: Set<MarketEventType>;
  verbose?: boolean;
}

class StreamLogger {
  private fileHandle?: number;

  constructor(options: StreamLoggerOptions);

  /**
   * Log market message to stdout (and file if configured)
   * Filters by event type if specified
   */
  logMessage(message: MarketMessage): void;

  /**
   * Log verbose connection events
   */
  logVerbose(message: string, data?: any): void;

  /**
   * Close file handle
   */
  close(): void;
}
```

**Output Format:**
```jsonl
{"event_type":"book","asset_id":"109681...376","market":"0x1234...","timestamp":"2025-11-18T12:34:56.789Z","hash":"abc123","bids":[{"price":"0.48","size":"30.5"}],"asks":[{"price":"0.52","size":"25.2"}]}
{"event_type":"price_change","market":"0x1234...","price_changes":[{"asset_id":"109681...376","price":"0.49","size":"5.0","side":"BUY","hash":"def456","best_bid":"0.49","best_ask":"0.52"}],"timestamp":"2025-11-18T12:34:57.123Z"}
{"event_type":"last_trade_price","asset_id":"109681...376","market":"0x1234...","price":"0.50","side":"BUY","size":"10.0","fee_rate_bps":"25","timestamp":"2025-11-18T12:34:58.456Z"}
```

If `--verbose`:
```
[2025-11-18T12:34:55.000Z] Connecting to wss://ws-subscriptions-clob.polymarket.com/ws/market
[2025-11-18T12:34:55.234Z] Connected successfully
[2025-11-18T12:34:55.235Z] Subscribed to assets: 109681...376, 521816...422
[2025-11-18T12:34:55.500Z] Ping sent
{"event_type":"book",...}
[2025-11-18T12:35:05.500Z] Ping sent
{"event_type":"price_change",...}
```

### 4. Stream Command (`src/commands/stream/index.ts`)

```typescript
interface StreamOptions {
  eventTypes?: string;
  logFile?: string;
  verbose?: boolean;
  noPing?: boolean;
}

export async function streamCommand(
  assetIds: string,
  options: StreamOptions
): Promise<void> {
  // Parse comma-separated asset IDs
  const assets = assetIds.split(',').map(id => id.trim());

  // Parse event types filter
  const eventTypesSet = options.eventTypes
    ? new Set(options.eventTypes.split(',').map(t => t.trim()))
    : undefined;

  // Create logger
  const logger = new StreamLogger({
    logFile: options.logFile,
    eventTypes: eventTypesSet,
    verbose: options.verbose,
  });

  // Create WebSocket client
  const client = new PolymarketWebSocketClient({
    url: 'wss://ws-subscriptions-clob.polymarket.com/ws/market',
    assetIds: assets,
    enablePing: !options.noPing,
    onConnect: () => {
      if (options.verbose) {
        logger.logVerbose('Connected and subscribed', { assets });
      }
    },
    onMessage: (message) => {
      logger.logMessage(message);
    },
    onError: (error) => {
      console.error(`WebSocket error: ${error.message}`);
    },
    onDisconnect: () => {
      if (options.verbose) {
        logger.logVerbose('Disconnected');
      }
    },
  });

  // Handle graceful shutdown
  const shutdown = () => {
    logger.logVerbose('Shutting down...');
    client.disconnect();
    logger.close();
    process.exit(0);
  };

  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);

  // Connect and start streaming
  client.connect();
}
```

### 5. CLI Integration (`src/index.ts`)

```typescript
// Add stream command
program
  .command('stream')
  .description('Stream real-time market data via WebSocket')
  .argument('<asset-ids>', 'Comma-separated asset IDs to subscribe to')
  .option(
    '--event-types <types>',
    'Comma-separated event types to filter (book, price_change, tick_size_change, last_trade_price)'
  )
  .option('--log-file <path>', 'Write stream output to file')
  .option('--verbose', 'Enable verbose logging', false)
  .option('--no-ping', 'Disable automatic ping messages')
  .action(async (assetIds, options) => {
    await streamCommand(assetIds, options);
  });
```

## Dependencies

Add WebSocket support to `package.json`:

```json
{
  "dependencies": {
    "ws": "^8.18.0"
  },
  "devDependencies": {
    "@types/ws": "^8.5.13"
  }
}
```

**Note:** Bun has native WebSocket support, so we can use the built-in `WebSocket` API instead of `ws` if preferred.

## Error Handling

### Connection Errors
- Initial connection failure: Exit with error code 1
- Disconnect during streaming: Auto-reconnect (max 5 attempts with exponential backoff)
- Max reconnect attempts exceeded: Exit with error code 1

### Message Parse Errors
- Log parse error to stderr
- Continue processing subsequent messages
- Don't crash on malformed messages

### Invalid Input
- Invalid asset ID format: Exit with error code 1 and usage message
- Invalid event type: Exit with error code 1 and list valid types

### Signal Handling
- SIGINT (Ctrl+C): Graceful shutdown
- SIGTERM: Graceful shutdown
- Both: Close WebSocket, close file handle, exit code 0

## Testing

### Manual Testing
```bash
# Test with real Polymarket asset
bun run dev stream 109681959945973826496234384791167033612800000000000000000000000376 --verbose

# Test event filtering
bun run dev stream 109681959945973...376 --event-types book

# Test file logging
bun run dev stream 109681959945973...376 --log-file ./test.jsonl
# Verify file contents: cat test.jsonl | jq
```

### Integration Testing
- Mock WebSocket server for subscription flow
- Test reconnection logic with simulated disconnects
- Verify message filtering
- Verify file logging

## Implementation Phases

### Phase 1: Core Streaming (MVP)
- WebSocket client with connection management
- Parse and output all message types as JSON
- Handle SIGINT for graceful shutdown
- Basic error handling

### Phase 2: Enhanced Features
- Event type filtering
- File logging
- Verbose mode
- Auto-reconnection with backoff

### Phase 3: Polish
- Comprehensive error messages
- Input validation
- Documentation and examples

## Success Criteria

1. Successfully connect to Polymarket WebSocket
2. Receive and output real-time orderbook updates as JSON
3. Filter events by type when specified
4. Log to file while streaming to stdout
5. Graceful shutdown on Ctrl+C
6. Auto-reconnect on connection loss
7. Clear error messages for common issues
