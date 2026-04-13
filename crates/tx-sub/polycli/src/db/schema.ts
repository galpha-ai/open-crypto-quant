import { Database } from "bun:sqlite";

export function initializeSchema(db: Database): void {
  // Create tags table
  db.exec(`
    CREATE TABLE IF NOT EXISTS tags (
      id TEXT PRIMARY KEY,
      label TEXT NOT NULL,
      slug TEXT NOT NULL UNIQUE,
      publishedAt TEXT,
      createdAt TEXT,
      updatedAt TEXT
    )
  `);

  // Create indexes for performance
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_tags_label ON tags(label)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_tags_created_at ON tags(createdAt)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_tags_published_at ON tags(publishedAt)
  `);

  // Create events table
  db.exec(`
    CREATE TABLE IF NOT EXISTS events (
      id TEXT PRIMARY KEY,
      ticker TEXT NOT NULL,
      slug TEXT NOT NULL UNIQUE,
      title TEXT NOT NULL,
      description TEXT,
      resolution_source TEXT,
      start_date TEXT,
      creation_date TEXT,
      end_date TEXT,
      image TEXT,
      icon TEXT,
      active INTEGER NOT NULL DEFAULT 1,
      closed INTEGER NOT NULL DEFAULT 0,
      archived INTEGER NOT NULL DEFAULT 0,
      new INTEGER NOT NULL DEFAULT 0,
      featured INTEGER NOT NULL DEFAULT 0,
      restricted INTEGER NOT NULL DEFAULT 0,
      liquidity REAL,
      volume REAL,
      open_interest REAL,
      competitive REAL,
      volume_1mo REAL,
      volume_1yr REAL,
      comment_count INTEGER,
      enable_order_book INTEGER,
      cyom INTEGER,
      show_all_outcomes INTEGER,
      show_market_images INTEGER,
      enable_neg_risk INTEGER,
      automatically_active INTEGER,
      neg_risk_augmented INTEGER,
      pending_deployment INTEGER,
      deploying INTEGER,
      created_at TEXT,
      updated_at TEXT
    )
  `);

  // Create markets table
  db.exec(`
    CREATE TABLE IF NOT EXISTS markets (
      id TEXT PRIMARY KEY,
      event_id TEXT NOT NULL,
      question TEXT NOT NULL,
      condition_id TEXT,
      slug TEXT NOT NULL,
      start_date TEXT,
      end_date TEXT,
      liquidity REAL,
      volume REAL,
      outcomes TEXT,
      outcome_prices TEXT,
      clob_token_ids TEXT,
      active INTEGER NOT NULL DEFAULT 1,
      closed INTEGER NOT NULL DEFAULT 0,
      created_at TEXT,
      updated_at TEXT,
      FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE
    )
  `);

  // Create event_tags join table
  db.exec(`
    CREATE TABLE IF NOT EXISTS event_tags (
      event_id TEXT NOT NULL,
      tag_id TEXT NOT NULL,
      tag_label TEXT,
      PRIMARY KEY (event_id, tag_id),
      FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE,
      FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
    )
  `);

  // Create indexes for events table
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_events_closed ON events(closed)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_events_active ON events(active)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_events_ticker ON events(ticker)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_events_volume ON events(volume)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_events_end_date ON events(end_date)
  `);

  // Create indexes for markets table
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_markets_event_id ON markets(event_id)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_markets_closed ON markets(closed)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_markets_active ON markets(active)
  `);

  // Create indexes for event_tags join table
  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_event_tags_tag_id ON event_tags(tag_id)
  `);

  db.exec(`
    CREATE INDEX IF NOT EXISTS idx_event_tags_event_id ON event_tags(event_id)
  `);
}
