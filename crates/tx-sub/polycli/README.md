# Polymarket CLI

A command-line tool for testing Polymarket APIs and learning about markets, events, and tags. Built in TypeScript and running on the Bun runtime.

## Requirements

- [Bun](https://bun.sh/) 1.0+

## Installation

```bash
# Install dependencies
bun install
```

## Usage

### Development Mode

```bash
# Run CLI directly
bun run dev -- --help

# Or use the bin script
bun run start -- --help
```

### Commands

#### Tags Commands

##### `tags save` - Fetch and save tags

Fetches all tags from Polymarket API and saves them to local SQLite database.

```bash
bun run start -- tags save [options]

Options:
  --db-path <path>       Path to SQLite database file (default: "./polymarket.db")
  --batch-size <number>  Number of records to fetch per request (default: 100, max: 500)
  --base-url <url>       Polymarket API base URL (default: "https://gamma-api.polymarket.com")
  --verbose              Enable verbose logging
  --force                Truncate existing tags table before saving
```

Examples:
```bash
# Fetch all tags with default settings
bun run start -- tags save

# Fetch with smaller batch size
bun run start -- tags save --batch-size 50

# Force refresh all tags
bun run start -- tags save --force
```

##### `tags list` - List tags with filtering and pagination

Lists tags from local database with support for filtering, sorting, and pagination.

```bash
bun run start -- tags list [options]

Options:
  --db-path <path>       Path to SQLite database file (default: "./polymarket.db")
  --filter <text>        Filter tags by label or slug (case-insensitive substring match)
  --format <format>      Output format: table, json (default: "table")
  --limit <number>       Maximum number of results to display (default: 100, max: 10000)
  --offset <number>      Number of records to skip (default: 0)
  --sort-by <field>      Sort field: label, id, slug, publishedAt, createdAt, updatedAt (default: "label")
  --order <direction>    Sort order: asc, desc (default: "asc")
  --all                  Fetch all results without limit
  --show-count           Display total count of matching records (table format always shows count)
```

Examples:
```bash
# List first 100 tags (default)
bun run start -- tags list

# Search for crypto-related tags
bun run start -- tags list --filter crypto --limit 50

# Get page 2 of results (101-200)
bun run start -- tags list --limit 100 --offset 100

# Sort by most recently created
bun run start -- tags list --sort-by createdAt --order desc

# Export all tags to JSON (warning: large output)
bun run start -- tags list --all --format json > all_tags.json

# Export with metadata
bun run start -- tags list --format json --show-count --limit 50 > tags_page1.json
```

##### `tags get` - Get a specific tag

Retrieves a single tag by ID (numeric) or slug (string).

```bash
bun run start -- tags get <id-or-slug> [options]

Options:
  --db-path <path>   Path to SQLite database file (default: "./polymarket.db")
  --format <format>  Output format: json, table (default: "json")
```

Examples:
```bash
# Get tag by ID
bun run start -- tags get 21

# Get tag by slug
bun run start -- tags get crypto

# Get tag as table
bun run start -- tags get crypto --format table
```

#### Markets Commands

##### `markets save` - Fetch and save markets for a tag

Fetches all markets (events and associated market data) for a specific tag from Polymarket API and saves them to local SQLite database.

```bash
bun run start -- markets save --tag <id-or-slug> [options]

Options:
  --tag <id-or-slug>     Tag ID (numeric) or slug (string) - REQUIRED
  --db-path <path>       Path to SQLite database file (default: "./polymarket.db")
  --batch-size <number>  Number of records to fetch per request (default: 100, max: 500)
  --base-url <url>       Polymarket API base URL (default: "https://gamma-api.polymarket.com")
  --verbose              Enable verbose logging
  --force                Delete existing markets for this tag before saving
  --include-closed       Include closed markets in sync
```

Examples:
```bash
# Save markets for crypto tag (by slug - CLI resolves to tag ID automatically)
bun run start -- markets save --tag crypto

# Save markets for crypto tag (by numeric ID)
bun run start -- markets save --tag 21

# Save markets including closed ones
bun run start -- markets save --tag mlb --include-closed

# Force refresh of markets for a tag
bun run start -- markets save --tag mlb --force
```

##### `markets list` - List markets with filtering and pagination

Lists markets/events from local database with support for filtering, sorting, and pagination.

```bash
bun run start -- markets list [options]

Options:
  --db-path <path>       Path to SQLite database file (default: "./polymarket.db")
  --tag <id-or-slug>     Filter by tag ID (numeric) or slug (string)
  --filter <text>        Search in title, ticker, or description (case-insensitive substring match)
  --closed <boolean>     Filter by closed status (true/false)
  --active <boolean>     Filter by active status (true/false)
  --format <format>      Output format: table, json (default: "table")
  --limit <number>       Maximum number of results to display (default: 100, max: 10000)
  --offset <number>      Number of records to skip (default: 0)
  --sort-by <field>      Sort field: id, ticker, title, end_date, volume, liquidity, created_at, updated_at (default: "end_date")
  --order <direction>    Sort order: asc, desc (default: "desc")
  --all                  Fetch all results without limit
  --show-count           Display total count of matching records (table format always shows count)
```

Examples:
```bash
# List all markets for MLB tag (using slug)
bun run start -- markets list --tag mlb

# List all markets for crypto tag (using numeric ID 21)
bun run start -- markets list --tag 21

# List active markets sorted by volume
bun run start -- markets list --active true --sort-by volume --order desc

# Search for specific markets
bun run start -- markets list --filter "Cleveland"

# Export markets to JSON
bun run start -- markets list --tag mlb --format json --all > mlb_markets.json

# Get page 2 of results (101-200)
bun run start -- markets list --limit 100 --offset 100
```

##### `markets get` - Get a specific market

Retrieves a single market event with all associated markets by event ID or slug.

```bash
bun run start -- markets get <id-or-slug> [options]

Options:
  --db-path <path>   Path to SQLite database file (default: "./polymarket.db")
  --format <format>  Output format: json, table (default: "json")
```

Examples:
```bash
# Get market by slug
bun run start -- markets get will-cleveland-change-name-to-indians-in-2025

# Get market by ID
bun run start -- markets get 33327

# Get market as table
bun run start -- markets get 33327 --format table
```

#### Stream Commands

##### `stream` - Stream real-time market data via WebSocket

Connects to Polymarket WebSocket and streams real-time orderbook updates for specified assets. Outputs JSON events to stdout.

```bash
bun run start -- stream <asset-ids> [options]

Arguments:
  asset-ids              Comma-separated asset IDs to subscribe to

Options:
  --event-types <types>  Comma-separated event types to filter (book, price_change, tick_size_change, last_trade_price)
  --log-file <path>      Write stream output to file (in addition to stdout)
  --verbose              Enable verbose logging (connection status, ping/pong)
  --no-ping              Disable automatic ping messages (default: ping every 10s)
```

Examples:
```bash
# Stream single asset (all events)
bun run start -- stream 109681959945973826496234384791167033612800000000000000000000000376

# Stream multiple assets
bun run start -- stream 109681959945973826496234384791167033612800000000000000000000000376,52181619848812915160551060468842099699261274828279546744438464278138132224

# Filter specific event types
bun run start -- stream 109681959945973826496234384791167033612800000000000000000000000376 --event-types book,last_trade_price

# Log to file and stdout
bun run start -- stream 109681959945973826496234384791167033612800000000000000000000000376 --log-file ./market-stream.jsonl

# Verbose mode for debugging connection issues
bun run start -- stream 109681959945973826496234384791167033612800000000000000000000000376 --verbose
```

Event Types:
- `book`: Full orderbook snapshot (bids/asks)
- `price_change`: Price level changes (new orders, cancellations)
- `tick_size_change`: Minimum tick size changes
- `last_trade_price`: Trade execution events

### Example Workflow

```bash
# 1. First-time setup: fetch all tags
bun run start -- tags save

# 2. Browse available tags
bun run start -- tags list --filter sports

# 3. Find tag ID for MLB (if needed)
bun run start -- tags get mlb

# 4. Sync markets for MLB tag (using slug - CLI resolves to ID automatically)
bun run start -- markets save --tag mlb

# 5. Query active MLB markets
bun run start -- markets list --tag mlb --active true

# 6. Get details for a specific market
bun run start -- markets get will-cleveland-change-name-to-indians-in-2025

# 7. Export to JSON for analysis
bun run start -- markets list --tag mlb --format json --all > mlb_markets.json
```

### Configuration

The CLI supports optional configuration via a YAML file and environment variables.

#### Configuration File

Create a configuration file at `~/.polymarket-cli/config.yaml`:

```yaml
# Default database path (can be overridden with --db-path)
database:
  path: "~/.polymarket-cli/polymarket.db"

# API configuration
api:
  base_url: "https://gamma-api.polymarket.com"
  timeout_ms: 30000
  retry_attempts: 3
  retry_backoff_ms: 1000

# Default pagination settings
pagination:
  default_batch_size: 100
  max_batch_size: 500

# Output formatting defaults
output:
  default_format: "table"
  table_max_column_width: 50
```

#### Environment Variables

- `POLYMARKET_DB_PATH`: Override default database path
- `POLYMARKET_API_BASE_URL`: Override API base URL
- `POLYMARKET_LOG_LEVEL`: Set log level (debug, info, warn, error)

#### Configuration Priority

Settings are applied in the following order (highest to lowest):

1. CLI flags (e.g., `--db-path`)
2. Environment variables (e.g., `POLYMARKET_DB_PATH`)
3. Configuration file values
4. Built-in defaults

### Build

```bash
# Compile to standalone executable
bun run build

# This creates a `polymarket-cli` executable
./polymarket-cli --help
```

## Development

### Project Structure

```
polycli/
├── bin/
│   └── polymarket-cli      # Executable entry point
├── src/
│   ├── index.ts            # CLI entry point
│   ├── commands/           # Command implementations
│   ├── api/                # API client
│   ├── db/                 # Database operations
│   ├── config/             # Configuration
│   ├── utils/              # Utilities
│   └── types/              # Type definitions
└── tests/                  # Test files
```

### Available Scripts

```bash
# Run tests
bun test

# Run tests in watch mode
bun test:watch

# Lint code
bun run lint
bun run lint:fix

# Format code
bun run format
bun run format:check

# Type check
bun run typecheck
```

## License

MIT
