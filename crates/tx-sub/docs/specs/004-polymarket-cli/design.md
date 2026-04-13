# Polymarket CLI

## Summary

The Polymarket CLI (`polymarket-cli`) is a command-line tool for testing Polymarket APIs and learning about markets, events, and tags. Built in TypeScript and running on the Bun runtime, it provides interactive access to Polymarket's public HTTP APIs with local persistence using SQLite. The initial implementation focuses on the tags API, enabling users to fetch all tags from Polymarket and store them locally for offline querying and analysis.

## Goals

- Provide a command-line interface for testing and exploring Polymarket HTTP APIs
- Implement paginated data fetching from Polymarket's public endpoints
- Persist API responses in a local SQLite database for offline access and analysis
- Enable users to learn about Polymarket's data model (tags, markets, events) through direct API interaction
- Serve as a reference implementation for integrating with Polymarket APIs
- Support both one-time data fetching and incremental updates

## Non-Goals

- **Real-time WebSocket Integration**: Live streaming of trade data (handled by `polymarket-sub` service)
- **Trading Functionality**: Executing trades or managing positions (authentication required)
- **Data Analytics**: Statistical analysis or visualization of market data (separate tooling)
- **Multi-User Support**: User accounts, permissions, or shared database access
- **API Rate Limiting**: Sophisticated retry logic or rate limit management (basic implementation only)
- **Full Schema Migration**: Database schema versioning and migration tooling (manual updates acceptable)
- **Cross-Platform GUI**: Graphical interface or web dashboard (CLI only)

## API

### CLI Interface

#### `polymarket-cli tags save`
- **Description**: Fetches all tags from Polymarket API and saves them to local SQLite database
- **Usage**: `polymarket-cli tags save [options]`
- **Options**:
  - `--db-path <path>`: Path to SQLite database file (default: `./polymarket.db`)
  - `--batch-size <number>`: Number of records to fetch per request (default: 100, max: 500)
  - `--base-url <url>`: Polymarket API base URL (default: `https://gamma-api.polymarket.com`)
  - `--verbose`: Enable verbose logging
  - `--force`: Truncate existing tags table before saving
- **Output**: Progress updates showing records fetched, current offset, and estimated remaining
- **Exit Codes**:
  - `0`: Success (all tags saved)
  - `1`: API error (network failure, invalid response)
  - `2`: Database error (connection failure, write error)
  - `3`: Configuration error (invalid options)

#### `polymarket-cli tags list`
- **Description**: Lists tags from local database with pagination support
- **Usage**: `polymarket-cli tags list [options]`
- **Options**:
  - `--db-path <path>`: Path to SQLite database file (default: `./polymarket.db`)
  - `--filter <text>`: Filter tags by label or slug (case-insensitive substring match)
  - `--format <format>`: Output format: `table` (default), `json`
  - `--limit <number>`: Maximum number of results to display (default: 100, max: 10000)
  - `--offset <number>`: Number of records to skip (default: 0)
  - `--sort-by <field>`: Sort field: `label` (default), `id`, `slug`, `publishedAt`, `createdAt`, `updatedAt`
  - `--order <direction>`: Sort order: `asc` (default), `desc`
  - `--all`: Fetch all results without limit (use with caution for large datasets)
  - `--show-count`: Display total count of matching records before results
- **Output**:
  - Formatted table/JSON of tags with columns: id, label, slug, publishedAt, createdAt, updatedAt
  - When `--show-count` enabled: Total count line (e.g., "Total: 10,234 tags" or "Showing 1-100 of 10,234")
  - Table format automatically enables count display; JSON format requires explicit `--show-count`
- **Exit Codes**:
  - `0`: Success
  - `2`: Database error
- **Examples**:
  ```bash
  # List first 100 tags (default)
  polymarket-cli tags list

  # Search for crypto-related tags
  polymarket-cli tags list --filter crypto --limit 50

  # Get page 2 of results (101-200)
  polymarket-cli tags list --limit 100 --offset 100

  # Sort by most recently created
  polymarket-cli tags list --sort-by createdAt --order desc

  # Export all tags to JSON (warning: large output)
  polymarket-cli tags list --all --format json > all_tags.json
  ```

#### `polymarket-cli tags get`
- **Description**: Get a specific tag by ID or slug
- **Usage**: `polymarket-cli tags get <id-or-slug> [options]`
- **Arguments**:
  - `<id-or-slug>`: Tag ID (numeric) or slug (string)
- **Options**:
  - `--db-path <path>`: Path to SQLite database file (default: `./polymarket.db`)
  - `--format <format>`: Output format: `json` (default), `table`
- **Output**: Single tag record in specified format
- **Exit Codes**:
  - `0`: Success
  - `1`: Tag not found
  - `2`: Database error

### HTTP API Integration

#### Polymarket Tags Endpoint
- **Endpoint**: `GET https://gamma-api.polymarket.com/tags`
- **Query Parameters**:
  - `limit`: Number of records to return (1-500, default: 100)
  - `offset`: Number of records to skip (default: 0)
- **Response**: JSON array of tag objects
- **Response Schema**:
  ```json
  [
    {
      "id": "207",
      "label": "dating",
      "slug": "dating",
      "publishedAt": "2023-11-02 21:35:57.945+00",
      "createdAt": "2023-11-02T21:35:57.968Z",
      "updatedAt": "2023-11-02T21:35:57.968Z"
    }
  ]
  ```
- **Pagination**: Exhausted when response array is empty
- **Rate Limits**: Not publicly documented (use exponential backoff on 429 errors)
- **Errors**:
  - `200`: Success
  - `400`: Invalid query parameters
  - `429`: Rate limit exceeded
  - `500`: Server error

### Errors

**API Errors**:
- `NetworkError`: Failed to connect to Polymarket API (network unreachable, DNS failure)
- `HttpError`: HTTP error response from API (status code 4xx/5xx)
- `ParseError`: Failed to parse JSON response (malformed data)
- `ValidationError`: Response data failed schema validation

**Database Errors**:
- `ConnectionError`: Failed to connect to SQLite database
- `SchemaError`: Database schema missing or incompatible
- `WriteError`: Failed to insert/update records
- `QueryError`: Failed to execute SELECT query

**Configuration Errors**:
- `InvalidPathError`: Database path invalid or not writable
- `InvalidOptionError`: CLI option value out of bounds or invalid format

**Error Handling Strategy**:
- Network errors: Retry up to 3 times with exponential backoff (1s, 2s, 4s)
- HTTP 429 (rate limit): Wait for Retry-After header value, fallback to 60s
- HTTP 5xx: Retry up to 3 times
- Database errors: Log error details and exit (no automatic recovery)
- Validation errors: Log schema mismatch and continue with next batch

## Behavior

### Tags Save Flow

Given user executes `polymarket-cli tags save`:

1. **Initialization**:
   - Parse CLI options and validate values
   - Resolve database path (relative to current working directory)
   - Initialize SQLite connection or create database file if missing
   - Run schema initialization: create `tags` table if not exists
   - If `--force` flag present, execute `DELETE FROM tags`

2. **Pagination Loop**:
   - Set `offset = 0`, `batch_size = options.batchSize`
   - Initialize progress tracker (total_fetched = 0)

3. **Fetch Batch**:
   - Construct URL: `${baseUrl}/tags?limit=${batchSize}&offset=${offset}`
   - Send HTTP GET request with timeout (30 seconds)
   - On network error: retry up to 3 times with exponential backoff
   - On HTTP 429: wait for Retry-After header duration, then retry
   - On HTTP 5xx: retry up to 3 times
   - On HTTP 4xx (except 429): log error and exit with code 1
   - Parse JSON response body
   - Validate response is array of objects with required fields

4. **Store Batch**:
   - Begin SQLite transaction
   - For each tag in response:
     - Execute UPSERT statement using `INSERT ... ON CONFLICT(id) DO UPDATE`
     - Map API field names to database columns
     - Parse timestamps to ISO 8601 strings
   - Commit transaction
   - On database error: rollback transaction, log error, exit with code 2

5. **Update Progress**:
   - Increment `total_fetched` by batch size
   - Log progress: `Fetched ${total_fetched} tags (offset: ${offset})`
   - If verbose mode: log individual tag IDs

6. **Check Termination**:
   - If response array length < batch_size: pagination exhausted, exit loop
   - If response array is empty: pagination exhausted, exit loop
   - Otherwise: increment offset by batch_size, go to step 3

7. **Finalization**:
   - Log summary: `Successfully saved ${total_fetched} tags to ${dbPath}`
   - Close database connection
   - Exit with code 0

### Tags List Flow

Given user executes `polymarket-cli tags list`:

1. **Parse Options**:
   - Validate `--limit` is between 1 and 10000 (exit code 3 if invalid)
   - Validate `--offset` is non-negative (exit code 3 if invalid)
   - Validate `--sort-by` is valid field name (exit code 3 if invalid)
   - Validate `--order` is 'asc' or 'desc' (exit code 3 if invalid)
   - If `--all` flag present, set limit to NULL (no limit)

2. **Open database connection** (exit code 2 if fails)

3. **Get total count** (if `--show-count` or table format):
   - Build count query: `SELECT COUNT(*) FROM tags`
   - If `--filter` provided: add `WHERE (label LIKE '%${filter}%' OR slug LIKE '%${filter}%') COLLATE NOCASE`
   - Execute count query and store result

4. **Build main SQL query**:
   - Base: `SELECT * FROM tags`
   - If `--filter` provided: add `WHERE (label LIKE '%${filter}%' OR slug LIKE '%${filter}%') COLLATE NOCASE`
   - Add `ORDER BY ${sortBy} ${order}` (default: `label ASC`)
   - If not `--all` flag: add `LIMIT ${limit}` (default: 100)
   - If `--offset` > 0: add `OFFSET ${offset}`

5. **Execute query and fetch results**

6. **Format output** based on `--format` option:
   - **`table` format**:
     - If count query executed: print header "Showing ${offset+1}-${offset+results.length} of ${totalCount} tags"
     - Use CLI table library (e.g., `cli-table3`) with column headers
     - Truncate long fields to fit terminal width
   - **`json` format**:
     - If `--show-count`: prepend total count as separate line or metadata object
     - Serialize array to JSON with pretty printing (indent 2 spaces)

7. **Write formatted output to stdout**

8. **Exit with code 0**

**Performance Notes**:
- For table format with 10k+ results without `--limit`, warn user before rendering
- COUNT queries use the `idx_tags_label` index for fast filtering
- Pagination queries use LIMIT/OFFSET which is efficient for moderate offsets
- For very large offsets (>10000), consider warning about cursor-based pagination in future

### Tags Get Flow

Given user executes `polymarket-cli tags get <id-or-slug>`:

1. Open database connection (exit code 2 if fails)
2. Determine if argument is numeric (ID) or string (slug)
3. Build SQL query:
   - If numeric: `SELECT * FROM tags WHERE id = ?`
   - If string: `SELECT * FROM tags WHERE slug = ?`
4. Execute query with parameterized value
5. If no results: print "Tag not found", exit with code 1
6. Format single result based on `--format`:
   - `json`: Serialize object to JSON with pretty printing
   - `table`: Display single record as table with key-value rows
7. Write formatted output to stdout
8. Exit with code 0

## Data Model

### Tags Table

- **Purpose**: Stores Polymarket tag metadata for offline querying
- **Fields**:
  - `id`: TEXT PRIMARY KEY - Tag identifier (stored as string to match API format)
  - `label`: TEXT NOT NULL - Human-readable tag name
  - `slug`: TEXT NOT NULL UNIQUE - URL-safe tag identifier
  - `publishedAt`: TEXT NOT NULL - Publication timestamp (ISO 8601)
  - `createdAt`: TEXT NOT NULL - Creation timestamp (ISO 8601)
  - `updatedAt`: TEXT NOT NULL - Last update timestamp (ISO 8601)
- **Indexes**:
  - Primary key index on `id` (automatic)
  - Unique index on `slug` (explicit)
  - Index on `label` for filtering and sorting performance (explicit)
  - Index on `createdAt` for sorting by creation time (explicit)
  - Index on `publishedAt` for sorting by publication time (explicit)
- **Constraints**:
  - `id` must be unique (PRIMARY KEY)
  - `slug` must be unique (UNIQUE)
  - All timestamp fields stored as TEXT in ISO 8601 format
- **Schema Definition**:
  ```sql
  CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    publishedAt TEXT NOT NULL,
    createdAt TEXT NOT NULL,
    updatedAt TEXT NOT NULL
  );

  CREATE INDEX IF NOT EXISTS idx_tags_label ON tags(label);
  CREATE INDEX IF NOT EXISTS idx_tags_created_at ON tags(createdAt);
  CREATE INDEX IF NOT EXISTS idx_tags_published_at ON tags(publishedAt);
  ```

### Database File Structure

- **Location**: User-specified via `--db-path` option, default: `./polymarket.db`
- **Format**: SQLite 3.x database file
- **Initialization**: Created automatically on first run if missing
- **Migrations**: Manual schema updates (future: migration scripts in `polycli/migrations/`)

## Configuration

The CLI uses a combination of command-line options and optional configuration file.

### Configuration File (Optional)

**Location**: `~/.polymarket-cli/config.yaml` (XDG_CONFIG_HOME compliant)

**Structure**:
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

**Configuration Priority** (highest to lowest):
1. CLI flags (e.g., `--db-path`)
2. Environment variables (e.g., `POLYMARKET_DB_PATH`)
3. Configuration file values
4. Built-in defaults

**Environment Variables**:
- `POLYMARKET_DB_PATH`: Override default database path
- `POLYMARKET_API_BASE_URL`: Override API base URL
- `POLYMARKET_LOG_LEVEL`: Set log level (debug, info, warn, error)

### CLI Dependencies

**Runtime Dependencies** (`package.json`):
- `commander`: CLI argument parsing
- `bun:sqlite`: Built-in SQLite database driver (no native bindings required)
- `chalk`: Terminal color output
- `cli-table3`: Table formatting for output

**Development Dependencies**:
- `@types/bun`: Bun type definitions
- `eslint`: Linting
- `prettier`: Code formatting

**Bun Built-in Features Used**:
- `Bun.file()`: Fast file I/O operations
- `fetch()`: Native HTTP client (replaces axios)
- `Bun.write()`: High-performance file writing
- `bun:sqlite`: Native SQLite integration
- TypeScript execution built-in (no ts-node required)
- Built-in test runner (replaces vitest)

## Project Structure

```
polycli/
├── package.json           # Bun project configuration
├── tsconfig.json          # TypeScript compiler configuration
├── bunfig.toml            # Bun runtime configuration (optional)
├── src/
│   ├── index.ts           # CLI entry point
│   ├── commands/
│   │   ├── tags/
│   │   │   ├── save.ts    # Tags save command implementation
│   │   │   ├── list.ts    # Tags list command implementation
│   │   │   └── get.ts     # Tags get command implementation
│   │   └── index.ts       # Command registry
│   ├── api/
│   │   ├── client.ts      # HTTP client wrapper (using native fetch)
│   │   ├── tags.ts        # Tags API methods
│   │   └── types.ts       # API response types
│   ├── db/
│   │   ├── connection.ts  # Database connection manager (bun:sqlite)
│   │   ├── schema.ts      # Schema initialization
│   │   ├── tags.ts        # Tags table operations
│   │   └── types.ts       # Database types
│   ├── config/
│   │   ├── loader.ts      # Configuration file loader
│   │   └── defaults.ts    # Default configuration values
│   ├── utils/
│   │   ├── logger.ts      # Logging utility
│   │   ├── retry.ts       # Retry logic with backoff
│   │   └── format.ts      # Output formatting utilities
│   └── types/
│       └── index.ts       # Shared TypeScript types
├── tests/
│   ├── commands/
│   │   └── tags.test.ts   # Tags command tests
│   ├── api/
│   │   └── client.test.ts # API client tests
│   ├── db/
│   │   └── tags.test.ts   # Database operations tests
│   └── fixtures/
│       └── tags.json      # Test data fixtures
├── bin/
│   └── polymarket-cli     # Executable script (#!/usr/bin/env bun)
└── README.md              # User documentation
```

## TypeScript Implementation Details

### API Client Module (`src/api/client.ts`)

**Responsibilities**:
- HTTP request execution with timeout using native `fetch()`
- Retry logic with exponential backoff
- Rate limit handling (429 responses)
- Error normalization

**Interface**:
```typescript
export interface ApiClientConfig {
  baseUrl: string;
  timeout: number;
  retryAttempts: number;
  retryBackoffMs: number;
}

export class ApiClient {
  constructor(config: ApiClientConfig);

  async get<T>(
    path: string,
    params?: Record<string, string | number>
  ): Promise<T>;
}
```

**Error Handling**:
- Uses Bun's native `fetch()` API with AbortController for timeout
- Implements retry logic for network errors and 5xx responses
- Handles 429 rate limits with Retry-After header parsing
- Throws descriptive errors for 4xx client errors

**Bun-Specific Implementation**:
```typescript
// Example using Bun's native fetch
const controller = new AbortController();
const timeoutId = setTimeout(() => controller.abort(), this.config.timeout);

try {
  const response = await fetch(url, {
    signal: controller.signal,
  });
  // Process response
} finally {
  clearTimeout(timeoutId);
}
```

### Database Module (`src/db/tags.ts`)

**Responsibilities**:
- CRUD operations for tags table using `bun:sqlite`
- Transaction management
- Query building with parameterization

**Interface**:
```typescript
import { Database } from "bun:sqlite";

export interface Tag {
  id: string;
  label: string;
  slug: string;
  publishedAt: string;
  createdAt: string;
  updatedAt: string;
}

export class TagsRepository {
  constructor(db: Database);

  upsertBatch(tags: Tag[]): void;  // Synchronous with bun:sqlite
  findAll(options?: QueryOptions): Tag[];
  count(options?: { filter?: string }): number;  // For pagination
  findById(id: string): Tag | null;
  findBySlug(slug: string): Tag | null;
  truncate(): void;
}

export interface QueryOptions {
  filter?: string;           // Filter by label or slug
  limit?: number;            // Default: 100, null for unlimited
  offset?: number;           // Default: 0
  sortBy?: TagSortField;     // Default: 'label'
  order?: 'asc' | 'desc';    // Default: 'asc'
}

export type TagSortField = 'id' | 'label' | 'slug' | 'publishedAt' | 'createdAt' | 'updatedAt';
```

**Transaction Safety**:
- All batch operations wrapped in BEGIN/COMMIT transactions using `db.transaction()`
- Automatic rollback on errors
- UPSERT using `INSERT ... ON CONFLICT(id) DO UPDATE SET ...`

**Bun-Specific Implementation**:
```typescript
import { Database } from "bun:sqlite";

// Synchronous SQLite operations (Bun is optimized for this)
const db = new Database("polymarket.db");

// Prepared statements for performance
const upsertStmt = db.prepare(`
  INSERT INTO tags (id, label, slug, publishedAt, createdAt, updatedAt)
  VALUES (?, ?, ?, ?, ?, ?)
  ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    slug = excluded.slug,
    publishedAt = excluded.publishedAt,
    updatedAt = excluded.updatedAt
`);

// Transaction wrapper
const upsertBatch = db.transaction((tags: Tag[]) => {
  for (const tag of tags) {
    upsertStmt.run(tag.id, tag.label, tag.slug, tag.publishedAt, tag.createdAt, tag.updatedAt);
  }
});
```

### Command Implementation (`src/commands/tags/save.ts`)

**Responsibilities**:
- Parse CLI options
- Orchestrate API fetching and database persistence
- Display progress updates
- Handle errors and exit codes

**Implementation Pattern**:
```typescript
export async function saveTagsCommand(options: SaveOptions): Promise<void> {
  const db = initializeDatabase(options.dbPath);
  const apiClient = new ApiClient(loadApiConfig(options));
  const tagsRepo = new TagsRepository(db);

  if (options.force) {
    await tagsRepo.truncate();
  }

  let offset = 0;
  let totalFetched = 0;
  const spinner = ora('Fetching tags...').start();

  while (true) {
    const tags = await apiClient.get<Tag[]>('/tags', {
      limit: options.batchSize,
      offset
    });

    if (tags.length === 0) break;

    await tagsRepo.upsertBatch(tags);
    totalFetched += tags.length;
    spinner.text = `Fetched ${totalFetched} tags (offset: ${offset})`;

    if (tags.length < options.batchSize) break;
    offset += options.batchSize;
  }

  spinner.succeed(`Saved ${totalFetched} tags to ${options.dbPath}`);
}
```

## Idempotency & Concurrency

**Idempotency**:
- `tags save` command is idempotent: running multiple times produces same final state
- UPSERT operations ensure existing records are updated, not duplicated
- `--force` flag provides explicit truncate-and-reload behavior
- Safe to interrupt and resume (will update existing records on next run)

**Concurrency**:
- Single-user tool: no concurrent CLI invocations expected
- SQLite database uses WAL mode for better concurrency (multiple readers, single writer)
- No distributed locking required (local filesystem only)
- API requests are sequential (no parallel batching)

**Race Conditions**:
- No shared mutable state between commands
- Each command execution is independent
- Database writes are atomic (transaction-based)

## Observability

### Logging

**Format**: Structured JSON logs to stderr (using `console` with custom formatter)

**Log Levels**:
- **debug**: API request/response details, SQL queries
- **info**: Progress updates, successful operations, summary statistics
- **warn**: Retry attempts, validation warnings, skipped records
- **error**: Fatal errors, unhandled exceptions, exit conditions

**Configuration**:
- Default level: `info`
- Override via `POLYMARKET_LOG_LEVEL` environment variable
- Verbose flag (`--verbose`) sets level to `debug`

**Bun-Specific Logging**:
- Use `console.log()` for structured output (Bun has optimized console)
- Consider `Bun.inspect()` for detailed object inspection in debug mode
- Use `Bun.color()` for colored terminal output

**Example Logs**:
```json
{"level":"info","time":1731924000000,"msg":"Starting tags save command","batchSize":100,"dbPath":"./polymarket.db"}
{"level":"debug","time":1731924001000,"msg":"API request","method":"GET","url":"https://gamma-api.polymarket.com/tags?limit=100&offset=0"}
{"level":"info","time":1731924002000,"msg":"Fetched batch","count":100,"offset":0,"totalFetched":100}
{"level":"warn","time":1731924003000,"msg":"Retry attempt","attempt":1,"error":"ECONNRESET"}
{"level":"info","time":1731924010000,"msg":"Tags save completed","totalFetched":523,"duration":10000}
```

### Progress Indication

**Interactive Mode** (stdout is TTY):
- Use simple progress indicators with chalk colors
- Display current progress: records fetched, current offset
- Update progress line using ANSI escape sequences
- Success/failure symbols on completion (use `chalk` for colors)

**Non-Interactive Mode** (stdout is pipe/file):
- Log progress messages to stderr at regular intervals (every batch)
- No ANSI escape codes or animations
- Suitable for scripting and automation

### Error Reporting

**User-Facing Errors**:
- Clear, actionable error messages to stderr
- Include suggested remediation steps
- Exit with appropriate code (1=API, 2=DB, 3=config)

**Developer Errors**:
- Full stack traces in debug mode
- Unhandled promise rejections logged and exit(1)
- Network errors include request details (URL, headers)

## Security

### Authentication
- **No authentication required**: Uses public Polymarket APIs only
- Future commands requiring auth (trading) will use API key from environment variable or config file
- API keys stored in config file must have restrictive permissions (0600)

### Input Validation
- CLI options validated with type checking and bounds checking
- Database path canonicalized to prevent directory traversal
- API responses validated against JSON schema before insertion
- SQL queries use parameterized statements (no string interpolation)

### Data Protection
- SQLite database stored locally with user's file permissions
- No sensitive data stored (public market metadata only)
- HTTPS enforced for API requests (no plaintext HTTP)
- TLS certificate validation enabled by default

### Dependency Security
- Use `bun audit` in CI to detect vulnerable dependencies (when available)
- Pin dependency versions in `bun.lockb` (Bun's binary lockfile)
- Prefer well-maintained libraries with active security updates
- No native modules required (Bun has built-in SQLite and fetch)

## Rollout Plan

### Phase 1: Project Setup
1. Create `polycli/` directory in workspace root
2. Initialize Bun project: `bun init`
3. Install dependencies (`commander`, `yaml`, `chalk`)
4. Set up build tooling (tsconfig, eslint, prettier)
5. Create basic CLI structure with `commander`
6. Add executable script in `bin/polymarket-cli` with shebang `#!/usr/bin/env bun`
7. Configure `package.json` with Bun-specific scripts

### Phase 2: Core Implementation
1. Implement database schema initialization using `bun:sqlite` (`src/db/schema.ts`)
2. Implement tags repository with synchronous CRUD operations (`src/db/tags.ts`)
3. Implement API client with native `fetch()` and retry logic (`src/api/client.ts`)
4. Implement tags API methods (`src/api/tags.ts`)
5. Implement `tags save` command (`src/commands/tags/save.ts`)
6. Add progress indication and logging using `chalk`

### Phase 3: Query Commands
1. Implement `tags list` command with filtering and formatting
2. Implement `tags get` command with ID/slug lookup
3. Add output formatters (table, JSON)
4. Implement configuration file loader

### Phase 4: Testing
1. Write unit tests using Bun's built-in test runner (`bun test`)
2. Write unit tests for database operations (using in-memory SQLite with `bun:sqlite`)
3. Write unit tests for API client (mocked fetch using `bun:test` mocking)
4. Write integration tests for commands (test fixtures)
5. Add CI workflow for tests and linting (`bun test`, `bun run lint`)
6. Manual testing with real Polymarket API

### Phase 5: Documentation and Polish
1. Write README with installation and usage instructions
2. Add inline code documentation (JSDoc comments)
3. Create example scripts and use cases
4. Add `--help` text for all commands
5. Final code review and refactoring

### Rollback Criteria
- If database corruption occurs during testing, revert schema changes
- If API rate limiting prevents reasonable usage, implement smarter backoff
- If performance is unacceptable (>5 min for full tags fetch), optimize batching

## Acceptance Criteria

- [ ] `polymarket-cli tags save` successfully fetches all tags from API (500+ records)
- [ ] Tags stored in SQLite database with correct schema and indexes
- [ ] Progress updates displayed during fetch with record count and offset
- [ ] `--force` flag truncates existing data and re-fetches from scratch
- [ ] Network errors retry up to 3 times with exponential backoff
- [ ] HTTP 429 rate limits handled with Retry-After header parsing
- [ ] Database transactions rollback on error without partial writes
- [ ] `polymarket-cli tags list` displays first 100 tags by default in formatted table
- [ ] `--filter` option performs case-insensitive substring match on label and slug
- [ ] `--limit` option limits results (default: 100, max: 10000)
- [ ] `--offset` option skips records for pagination
- [ ] `--sort-by` option sorts by specified field (label, id, slug, timestamps)
- [ ] `--order` option controls sort direction (asc/desc)
- [ ] `--all` flag fetches all records without limit
- [ ] `--show-count` displays total count of matching records
- [ ] Table format automatically shows "Showing X-Y of Z tags" header
- [ ] `--format json` outputs valid JSON array with optional count metadata
- [ ] `--format table` outputs formatted table with headers and proper column widths
- [ ] Large result sets (10k+) display performance warning when using `--all`
- [ ] `polymarket-cli tags get <id>` retrieves tag by ID
- [ ] `polymarket-cli tags get <slug>` retrieves tag by slug
- [ ] Exit code 0 on success, 1 on API error, 2 on DB error
- [ ] Verbose mode (`--verbose`) logs debug information including API requests
- [ ] Configuration file loaded from `~/.polymarket-cli/config.yaml` if present
- [ ] Unit test coverage >80% for core modules (db, api, commands)
- [ ] Integration tests pass with mocked API responses using Bun's test runner
- [ ] CLI runs successfully on Bun 1.0+ on Linux and macOS
- [ ] README includes Bun installation and usage instructions
- [ ] `--help` flag displays comprehensive usage information for all commands
- [ ] Executable works with `#!/usr/bin/env bun` shebang

## Future Enhancements

### Additional Subcommands
- `polymarket-cli markets`: Fetch and query prediction markets
- `polymarket-cli events`: Fetch and query market events
- `polymarket-cli prices`: Fetch historical price data
- `polymarket-cli sync`: Incremental sync command for all entities

### Advanced Features
- `--watch` mode for continuous synchronization
- Export commands to dump data as JSON/CSV files
- Import commands to load external data
- Data validation and integrity checks
- Full-text search across tags, markets, and events
- Graph visualization of market relationships

### Performance Optimizations
- Parallel batch fetching (configurable concurrency limit)
- Compressed database storage (SQLite VACUUM)
- Incremental updates using timestamp filtering
- Connection pooling for database operations

## References

- [Polymarket API Documentation](https://docs.polymarket.com) (if available)
- [Polymarket Gamma API Tags Endpoint](https://gamma-api.polymarket.com/tags)
- [Bun Documentation](https://bun.sh/docs)
- [Bun SQLite Documentation](https://bun.sh/docs/api/sqlite)
- [Bun Test Runner](https://bun.sh/docs/cli/test)
- [Commander.js](https://github.com/tj/commander.js) - CLI framework
- [Project Architecture](../../architecture.md)
