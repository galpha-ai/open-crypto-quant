# Polymarket CLI - Market Support Design

## Overview

This document outlines the design for adding Polymarket market data support to the CLI tool. The implementation enables users to fetch market data by tag and query it from a local SQLite database.

## Requirements

1. Support syncing markets matching a tag to the database
2. Support querying market information from the database
3. Maintain consistency with existing `tags` command patterns

## Database Schema

### Approach: Normalized Two-Table Design

The schema uses two tables to properly represent the one-to-many relationship between events and markets:
- **`events`** table: Stores event-level data (one row per event)
- **`markets`** table: Stores market-level data (one or more rows per event)

This normalized approach ensures all markets are preserved when an event has multiple markets.

#### Events Table

```sql
CREATE TABLE IF NOT EXISTS events (
  -- Primary key
  id TEXT PRIMARY KEY,

  -- Event identification
  ticker TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  description TEXT,

  -- Event metadata
  resolution_source TEXT,
  start_date TEXT,
  creation_date TEXT,
  end_date TEXT,
  image TEXT,
  icon TEXT,

  -- Event status flags
  active INTEGER NOT NULL DEFAULT 1,
  closed INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0,
  new INTEGER NOT NULL DEFAULT 0,
  featured INTEGER NOT NULL DEFAULT 0,
  restricted INTEGER NOT NULL DEFAULT 0,

  -- Event metrics
  liquidity REAL,
  volume REAL,
  open_interest REAL,
  competitive REAL,
  volume_1mo REAL,
  volume_1yr REAL,
  comment_count INTEGER,

  -- Configuration flags
  enable_order_book INTEGER,
  cyom INTEGER,
  show_all_outcomes INTEGER,
  show_market_images INTEGER,
  enable_neg_risk INTEGER,
  automatically_active INTEGER,
  neg_risk_augmented INTEGER,
  pending_deployment INTEGER,
  deploying INTEGER,

  -- Timestamps
  created_at TEXT,
  updated_at TEXT
);
```

#### Markets Table

```sql
CREATE TABLE IF NOT EXISTS markets (
  -- Primary key
  id TEXT PRIMARY KEY,

  -- Foreign key to events
  event_id TEXT NOT NULL,

  -- Market identification
  question TEXT NOT NULL,
  condition_id TEXT,
  slug TEXT NOT NULL,

  -- Market timing
  start_date TEXT,
  end_date TEXT,

  -- Market metrics
  liquidity REAL,
  volume REAL,

  -- Market outcomes
  outcomes TEXT, -- JSON array: ["Yes", "No"]
  outcome_prices TEXT, -- JSON array: ["0.088", "0.912"]

  -- Market status
  active INTEGER NOT NULL DEFAULT 1,
  closed INTEGER NOT NULL DEFAULT 0,

  -- Timestamps
  created_at TEXT,
  updated_at TEXT,

  FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE
);
```

#### Event-Tag Join Table

To support the many-to-many relationship between events and tags:

```sql
CREATE TABLE IF NOT EXISTS event_tags (
  event_id TEXT NOT NULL,
  tag_id TEXT NOT NULL,
  tag_label TEXT,

  PRIMARY KEY (event_id, tag_id),
  FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE,
  FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
```

### Indexes

```sql
-- Events table indexes
CREATE INDEX IF NOT EXISTS idx_events_closed ON events(closed);
CREATE INDEX IF NOT EXISTS idx_events_active ON events(active);
CREATE INDEX IF NOT EXISTS idx_events_ticker ON events(ticker);
CREATE INDEX IF NOT EXISTS idx_events_volume ON events(volume);
CREATE INDEX IF NOT EXISTS idx_events_end_date ON events(end_date);

-- Markets table indexes
CREATE INDEX IF NOT EXISTS idx_markets_event_id ON markets(event_id);
CREATE INDEX IF NOT EXISTS idx_markets_closed ON markets(closed);
CREATE INDEX IF NOT EXISTS idx_markets_active ON markets(active);

-- Event-tag join table indexes
CREATE INDEX IF NOT EXISTS idx_event_tags_tag_id ON event_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_event_tags_event_id ON event_tags(event_id);
```

## API Integration

### Polymarket Events API

**Endpoint**: `GET /events`

**Query Parameters**:
- `tag_id`: Filter events by tag ID
- `limit`: Number of results per page (default: 100)
- `offset`: Pagination offset
- `closed`: Include closed markets (true/false)

### API Client Implementation

**src/api/markets.ts**:
```typescript
export class MarketsApi {
  constructor(private client: ApiClient) {}

  async fetchEvents(
    tagId: string,  // Numeric tag ID (not slug)
    limit: number = 100,
    offset: number = 0,
    closed: boolean = false
  ): Promise<ApiEvent[]> {
    return this.client.get<ApiEvent[]>("/events", {
      tag_id: tagId,  // API requires numeric tag ID
      limit,
      offset,
      closed: closed ? "true" : "false",
    });
  }

  async fetchEventById(eventId: string): Promise<ApiEvent>
  async fetchEventBySlug(slug: string): Promise<ApiEvent>
}
```

### Tag Resolution Logic

Since the Polymarket API requires numeric tag IDs but users often work with human-readable slugs (e.g., "crypto", "mlb"), the CLI provides automatic resolution:

**src/utils/tag-resolver.ts**:
```typescript
export class TagResolver {
  constructor(private tagsRepo: TagsRepository) {}

  /**
   * Resolves a tag identifier (ID or slug) to a numeric tag ID.
   *
   * @param idOrSlug - Tag ID (numeric string like "21") or slug (string like "crypto")
   * @returns Numeric tag ID as string
   * @throws Error if tag not found
   */
  resolve(idOrSlug: string): string {
    // Check if input is numeric (tag ID)
    if (/^\d+$/.test(idOrSlug)) {
      // Validate that tag exists
      const tag = this.tagsRepo.findById(idOrSlug);
      if (!tag) {
        throw new Error(`Tag ID '${idOrSlug}' not found in database. Run 'tags save' first.`);
      }
      return idOrSlug;
    }

    // Input is a slug - lookup by slug
    const tag = this.tagsRepo.findBySlug(idOrSlug);
    if (!tag) {
      throw new Error(
        `Tag slug '${idOrSlug}' not found in database. Run 'tags save' first or use numeric tag ID.`
      );
    }

    return tag.id;
  }

  /**
   * Resolves tag and returns full tag object with metadata.
   */
  resolveWithMetadata(idOrSlug: string): Tag {
    const tagId = this.resolve(idOrSlug);
    return this.tagsRepo.findById(tagId)!;
  }
}
```

**Usage in commands**:
```typescript
// In markets save command
const tagResolver = new TagResolver(tagsRepo);
const tag = tagResolver.resolveWithMetadata(options.tag);

console.log(`Fetching markets for tag '${tag.label}' (ID: ${tag.id})`);
const markets = await marketsApi.fetchEvents(tag.id, ...);
```

## Database Repository

### EventsRepository

**src/db/events.ts**:
```typescript
export class EventsRepository {
  constructor(private db: Database) {}

  /**
   * Insert or update events in batch (uses transaction)
   */
  upsertBatch(events: Event[]): void

  /**
   * Find all events matching query options
   */
  findAll(options?: EventQueryOptions): Event[]

  /**
   * Count events matching filter criteria
   */
  count(options?: { filter?: string, tagId?: string, closed?: boolean, active?: boolean }): number

  /**
   * Find event by ID
   */
  findById(eventId: string): Event | null

  /**
   * Find event by slug
   */
  findBySlug(slug: string): Event | null

  /**
   * Delete all events for a specific tag
   */
  deleteByTag(tagId: string): void

  /**
   * Find events with their associated markets (joined query)
   */
  findWithMarkets(options?: EventQueryOptions): EventWithMarkets[]
}
```

### MarketsRepository

**src/db/markets.ts**:
```typescript
export class MarketsRepository {
  constructor(private db: Database) {}

  /**
   * Insert or update markets in batch (uses transaction)
   */
  upsertBatch(markets: Market[]): void

  /**
   * Find all markets for a specific event
   */
  findByEventId(eventId: string): Market[]

  /**
   * Find market by ID
   */
  findById(marketId: string): Market | null

  /**
   * Find market by slug
   */
  findBySlug(slug: string): Market | null

  /**
   * Count markets matching filter criteria
   */
  count(options?: { eventId?: string, closed?: boolean, active?: boolean }): number

  /**
   * Delete all markets for a specific event
   */
  deleteByEventId(eventId: string): void
}
```

### EventTagsRepository

**src/db/event-tags.ts**:
```typescript
export class EventTagsRepository {
  constructor(private db: Database) {}

  /**
   * Associate events with tags in batch
   */
  upsertBatch(associations: EventTag[]): void

  /**
   * Find all event IDs for a specific tag
   */
  findEventIdsByTag(tagId: string): string[]

  /**
   * Find all tags for a specific event
   */
  findTagsByEvent(eventId: string): Tag[]

  /**
   * Delete all associations for a specific tag
   */
  deleteByTag(tagId: string): void

  /**
   * Delete all associations for a specific event
   */
  deleteByEvent(eventId: string): void
}
```

### Query Options

```typescript
interface EventQueryOptions {
  filter?: string;           // Text search in title, ticker, description
  tagId?: string;            // Filter by tag (via join)
  closed?: boolean;          // Filter by closed status
  active?: boolean;          // Filter by active status
  limit?: number | null;     // Result limit
  offset?: number;           // Pagination offset
  sortBy?: EventSortField;   // Sort field
  order?: "asc" | "desc";    // Sort direction
}

interface EventWithMarkets extends Event {
  markets: Market[];         // Associated markets for this event
}

interface EventTag {
  event_id: string;
  tag_id: string;
  tag_label?: string;
}
```

## CLI Commands

### 1. markets save

Fetch markets for a specific tag and save to the database.

**Usage**:
```bash
polycli markets save --tag <id-or-slug> [options]
```

**Options**:
- `--tag <id-or-slug>` (required): Tag ID (numeric, e.g., `21`) or slug (string, e.g., `crypto`)
- `--db-path <path>`: SQLite database path (default: ./polymarket.db)
- `--batch-size <number>`: Records per API request (default: 100)
- `--base-url <url>`: Polymarket API base URL
- `--verbose`: Enable verbose logging
- `--force`: Truncate existing markets for this tag before saving
- `--include-closed`: Include closed markets in sync

**Behavior**:
- If `--tag` is a string, the CLI will first query the tags table to resolve the slug to a numeric tag ID
- The resolved numeric tag ID is then used to query the Polymarket API
- If the slug is not found in the database, an error is displayed

**Examples**:
```bash
# Save markets for crypto tag (by slug - CLI resolves to tag ID 21)
bun run start -- markets save --tag crypto

# Save markets for crypto tag (by numeric ID)
bun run start -- markets save --tag 21

# Save markets including closed ones (using numeric tag ID)
bun run start -- markets save --tag 100381 --include-closed

# Force refresh of markets for a tag (using slug)
bun run start -- markets save --tag mlb --force
```

### 2. markets list

List markets from the local database with filtering and pagination.

**Usage**:
```bash
polycli markets list [options]
```

**Options**:
- `--db-path <path>`: SQLite database path
- `--tag <id-or-slug>`: Filter by tag ID (numeric) or slug (string)
- `--filter <text>`: Search in title, ticker, or description
- `--closed <boolean>`: Filter by closed status (true/false)
- `--active <boolean>`: Filter by active status (true/false)
- `--format <format>`: Output format: table, json (default: table)
- `--limit <number>`: Maximum results (default: 100)
- `--offset <number>`: Pagination offset (default: 0)
- `--sort-by <field>`: Sort field (default: end_date)
- `--order <direction>`: Sort order: asc, desc (default: desc)
- `--all`: Fetch all results without limit
- `--show-count`: Display total count

**Behavior**:
- If `--tag` is a string, the CLI will first resolve the slug to a numeric tag ID via database lookup
- The resolved numeric tag ID is used internally to filter markets

**Examples**:
```bash
# List all markets for MLB tag (using slug)
bun run start -- markets list --tag mlb

# List all markets for crypto tag (using numeric ID 21)
bun run start -- markets list --tag 21

# List active markets sorted by volume
bun run start -- markets list --active true --sort-by volume --order desc

# Search for specific markets
bun run start -- markets list --filter "Cleveland"

# Export markets to JSON (using slug)
bun run start -- markets list --tag mlb --format json --all > mlb_markets.json
```

### 3. markets get

Get a specific market by event ID or slug.

**Usage**:
```bash
polycli markets get <id-or-slug> [options]
```

**Options**:
- `--db-path <path>`: SQLite database path
- `--format <format>`: Output format: json, table (default: json)

**Examples**:
```bash
# Get market by slug
bun run start -- markets get will-cleveland-change-name-to-indians-in-2025

# Get market by ID
bun run start -- markets get 33327 --format table
```

## Data Flow

### Save Command Flow

```
1. User runs: markets save --tag mlb
   |
   v
2. Resolve tag: Check if "mlb" is numeric
   - If numeric: Use as tag ID directly
   - If string: Query database for tag with slug="mlb"
   - Result: tag_id = 100381
   |
   v
3. Validate tag exists in database
   |
   v
4. API: GET /events?tag_id=100381&limit=100&offset=0&closed=false
   |
   v
5. Transform ApiEvent[] -> Event[], Market[], EventTag[]
   - Extract event-level data -> Event[]
   - Extract all markets from each event -> Market[]
   - Create event-tag associations -> EventTag[]
   |
   v
6. Upsert to SQLite in transaction:
   a. EventsRepository.upsertBatch(events)
   b. MarketsRepository.upsertBatch(markets)
   c. EventTagsRepository.upsertBatch(eventTags)
   |
   v
7. Repeat with pagination until no more results
   |
   v
8. Display summary:
   "Successfully saved 250 events and 378 markets for tag 'mlb' (ID: 100381)"
```

### List Command Flow

```
1. User runs: markets list --tag mlb --active true
   |
   v
2. Resolve tag (if provided): "mlb" -> tag_id = 100381
   |
   v
3. EventsRepository.findWithMarkets({
     tagId: "100381",
     active: true,
     limit: 100,
     offset: 0
   })
   |
   v
4. Execute JOIN query:
   SELECT events.*, markets.*
   FROM events
   INNER JOIN event_tags ON events.id = event_tags.event_id
   LEFT JOIN markets ON events.id = markets.event_id
   WHERE event_tags.tag_id = '100381'
     AND events.active = 1
   ORDER BY events.end_date DESC
   LIMIT 100
   |
   v
5. Group markets by event -> EventWithMarkets[]
   |
   v
6. Format and display results (table or JSON)
```

### Get Command Flow

```
1. User runs: markets get will-cleveland-change-name-to-indians-in-2025
   |
   v
2. Determine if input is ID or slug (slug in this case)
   |
   v
3. EventsRepository.findBySlug("will-cleveland-change-name-to-indians-in-2025")
   |
   v
4. If found, MarketsRepository.findByEventId(event.id)
   |
   v
5. Combine event + markets -> EventWithMarkets
   |
   v
6. Format and display (JSON or table)
```

## Implementation Files

### New Files
- `src/api/markets.ts` - MarketsApi class for API operations
- `src/db/events.ts` - EventsRepository for event database operations
- `src/db/markets.ts` - MarketsRepository for market database operations
- `src/db/event-tags.ts` - EventTagsRepository for event-tag associations
- `src/utils/tag-resolver.ts` - TagResolver class for resolving slugs to tag IDs
- `src/commands/markets/save.ts` - Save command implementation (saves events, markets, and associations)
- `src/commands/markets/list.ts` - List command implementation (queries events with markets)
- `src/commands/markets/get.ts` - Get command implementation (retrieves event with all markets)

### Modified Files
- `src/api/types.ts` - Add ApiEvent and ApiMarket interfaces
- `src/db/types.ts` - Add Event, Market, EventWithMarkets, EventTag, EventQueryOptions types
- `src/db/schema.ts` - Add events, markets, and event_tags table creation and indexes
- `src/db/tags.ts` - Add `findBySlug()` method to TagsRepository (if not already present)
- `src/index.ts` - Add markets command group

## Type Definitions

### API Types (src/api/types.ts)

```typescript
export interface ApiMarket {
  id: string;
  question: string;
  conditionId: string;
  slug: string;
  endDate: string;
  liquidity: string;
  startDate: string;
  outcomes: string; // JSON: ["Yes", "No"]
  outcomePrices: string; // JSON: ["0.088", "0.912"]
  volume: string;
  active: boolean;
  closed: boolean;
  volumeNum: number;
  liquidityNum: number;
  volume1mo?: number;
  volume1yr?: number;
  // ... additional fields
}

export interface ApiEvent {
  id: string;
  ticker: string;
  slug: string;
  title: string;
  description?: string;
  startDate: string;
  endDate: string;
  active: boolean;
  closed: boolean;
  liquidity: number;
  volume: number;
  markets: ApiMarket[];
  tags: ApiTag[];
  // ... additional fields
}
```

### Database Types (src/db/types.ts)

```typescript
export interface Event {
  // Primary key
  id: string;

  // Event identification
  ticker: string;
  slug: string;
  title: string;
  description?: string;

  // Event metadata
  resolution_source?: string;
  start_date?: string;
  creation_date?: string;
  end_date?: string;
  image?: string;
  icon?: string;

  // Event status flags
  active: boolean;
  closed: boolean;
  archived: boolean;
  new: boolean;
  featured: boolean;
  restricted: boolean;

  // Event metrics
  liquidity?: number;
  volume?: number;
  open_interest?: number;
  competitive?: number;
  volume_1mo?: number;
  volume_1yr?: number;
  comment_count?: number;

  // Configuration flags
  enable_order_book?: boolean;
  cyom?: boolean;
  show_all_outcomes?: boolean;
  show_market_images?: boolean;
  enable_neg_risk?: boolean;
  automatically_active?: boolean;
  neg_risk_augmented?: boolean;
  pending_deployment?: boolean;
  deploying?: boolean;

  // Timestamps
  created_at: string;
  updated_at: string;
}

export interface Market {
  // Primary key
  id: string;

  // Foreign key
  event_id: string;

  // Market identification
  question: string;
  condition_id?: string;
  slug: string;

  // Market timing
  start_date?: string;
  end_date?: string;

  // Market metrics
  liquidity?: number;
  volume?: number;

  // Market outcomes (stored as JSON strings)
  outcomes: string;        // ["Yes", "No"]
  outcome_prices: string;  // ["0.088", "0.912"]

  // Market status
  active: boolean;
  closed: boolean;

  // Timestamps
  created_at: string;
  updated_at: string;
}

export interface EventWithMarkets extends Event {
  markets: Market[];
}

export interface EventTag {
  event_id: string;
  tag_id: string;
  tag_label?: string;
}

export type EventSortField =
  | "id"
  | "ticker"
  | "title"
  | "end_date"
  | "volume"
  | "liquidity"
  | "created_at"
  | "updated_at";

export type MarketSortField =
  | "id"
  | "question"
  | "end_date"
  | "volume"
  | "liquidity"
  | "created_at"
  | "updated_at";
```

## Design Decisions

### 1. Normalized Schema (Events + Markets)
- **Rationale**: Preserves all market data when events have multiple markets
- **Implementation**: Separate `events` and `markets` tables with foreign key relationship
- **Benefits**:
  - No data loss when events have multiple markets
  - Supports queries at both event and market levels
  - Clean separation of concerns
  - Easier to add market-specific features later
- **Trade-off**: Slightly more complex queries (requires joins) vs. complete data preservation

### 2. Event-Tag Many-to-Many Relationship
- **Rationale**: Events can have multiple tags in Polymarket API
- **Implementation**: `event_tags` join table to store associations
- **Benefits**:
  - Preserves all tag relationships
  - Supports filtering by any tag
  - Enables multi-tag queries in the future
- **Note**: Initial implementation focuses on single-tag queries (via `--tag` filter)

### 3. Tag-Centric Workflow
- **Rationale**: Markets are organized by tags in the Polymarket API
- **Workflow**: Users first sync tags, then sync markets for specific tags
- **Benefit**: Controlled data volume and clear organization

### 4. Tag Resolution (ID vs. Slug)
- **Rationale**: Polymarket API requires numeric tag IDs, but slugs are more user-friendly
- **Implementation**: CLI accepts both numeric IDs (e.g., `21`) and slugs (e.g., `crypto`)
- **Resolution**: Slugs are resolved to tag IDs via database lookup before API calls
- **Benefit**: Better UX without requiring users to memorize numeric IDs
- **Error Handling**: Clear error messages if tag not found, with suggestion to run `tags save`

### 5. Upsert Strategy
- **Rationale**: Support incremental updates without duplicates
- **Implementation**:
  - Events: ON CONFLICT(id) DO UPDATE SET...
  - Markets: ON CONFLICT(id) DO UPDATE SET...
  - Event-Tags: ON CONFLICT(event_id, tag_id) DO NOTHING
- **Benefit**: Safe to re-run sync commands without data loss

### 6. Cascade Deletion
- **Rationale**: Maintain referential integrity when events are deleted
- **Implementation**: Foreign key with ON DELETE CASCADE on markets and event_tags tables
- **Benefit**: Automatically clean up related markets and tag associations

### 7. JSON Storage for Arrays
- **Rationale**: `outcomes` and `outcome_prices` are arrays
- **Implementation**: Store as JSON strings, parse on read
- **Trade-off**: Simple storage vs. queryability

## Example Workflow

```bash
# 1. First-time setup: fetch all tags
bun run start -- tags save

# 2. Browse available tags
bun run start -- tags list --filter sports

# 3. Find tag ID for MLB (if needed)
bun run start -- tags get mlb
# Output shows: { "id": "100381", "label": "MLB", "slug": "mlb", ... }

# 4. Sync markets for MLB tag (using slug - CLI resolves to ID 100381)
bun run start -- markets save --tag mlb

# 5. Query active MLB markets (using slug)
bun run start -- markets list --tag mlb --active true

# 6. Get details for a specific market
bun run start -- markets get will-cleveland-change-name-to-indians-in-2025

# 7. Export to JSON for analysis (using numeric ID)
bun run start -- markets list --tag 100381 --format json --all > data.json
```

## Future Enhancements

1. **Multi-tag queries**: Support filtering by multiple tags simultaneously (e.g., `--tag crypto,defi`)
2. **Market-level queries**: Add `markets list` command that operates at market level (not event level)
3. **Historical data**: Track price changes over time in separate `market_snapshots` table
4. **Advanced filtering**: By date range, liquidity threshold, volume range, etc.
5. **Refresh command**: Update existing markets without full re-sync (incremental update)
6. **Market subscriptions**: Monitor specific markets for updates via polling or webhooks
7. **Market search**: Full-text search across market questions and event descriptions
8. **Analytics commands**: Aggregate statistics by tag, time period, etc.

## Performance Considerations

- **Batch operations**: Use SQLite transactions for batch inserts across all three tables
- **Indexes**: Optimize common query patterns:
  - Events: closed, active, ticker, volume, end_date
  - Markets: event_id, closed, active
  - Event-Tags: tag_id, event_id (both directions for efficient joins)
- **Pagination**: API and database both support efficient pagination
- **Rate limiting**: Handled by existing ApiClient retry logic
- **Database size estimates**:
  - Events: ~1.5KB per event
  - Markets: ~500 bytes per market
  - Event-Tags: ~50 bytes per association
  - Example: 10,000 events with avg 1.5 markets each = ~30MB total
- **JOIN optimization**: Event-market queries use indexed foreign keys for fast lookups

## Testing Strategy

1. **Unit tests**: Repository methods, API client methods
2. **Integration tests**: End-to-end command execution
3. **API mocking**: Test error handling without hitting real API
4. **Sample data**: Use MLB tag (manageable size) for testing
5. **Edge cases**: Empty results, malformed data, network errors
