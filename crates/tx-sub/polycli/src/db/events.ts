import { Database } from "bun:sqlite";
import type { Event, EventQueryOptions, EventSortField, EventWithMarkets, Market } from "./types";
import { MarketsRepository } from "./markets";

export class EventsRepository {
  private db: Database;
  private upsertStmt: ReturnType<Database["prepare"]>;

  constructor(db: Database) {
    this.db = db;

    // Prepare upsert statement for performance
    this.upsertStmt = db.prepare(`
      INSERT INTO events (
        id, ticker, slug, title, description, resolution_source,
        start_date, creation_date, end_date, image, icon,
        active, closed, archived, new, featured, restricted,
        liquidity, volume, open_interest, competitive, volume_1mo, volume_1yr, comment_count,
        enable_order_book, cyom, show_all_outcomes, show_market_images,
        enable_neg_risk, automatically_active, neg_risk_augmented,
        pending_deployment, deploying, created_at, updated_at
      )
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        ticker = excluded.ticker,
        slug = excluded.slug,
        title = excluded.title,
        description = excluded.description,
        resolution_source = excluded.resolution_source,
        start_date = excluded.start_date,
        creation_date = excluded.creation_date,
        end_date = excluded.end_date,
        image = excluded.image,
        icon = excluded.icon,
        active = excluded.active,
        closed = excluded.closed,
        archived = excluded.archived,
        new = excluded.new,
        featured = excluded.featured,
        restricted = excluded.restricted,
        liquidity = excluded.liquidity,
        volume = excluded.volume,
        open_interest = excluded.open_interest,
        competitive = excluded.competitive,
        volume_1mo = excluded.volume_1mo,
        volume_1yr = excluded.volume_1yr,
        comment_count = excluded.comment_count,
        enable_order_book = excluded.enable_order_book,
        cyom = excluded.cyom,
        show_all_outcomes = excluded.show_all_outcomes,
        show_market_images = excluded.show_market_images,
        enable_neg_risk = excluded.enable_neg_risk,
        automatically_active = excluded.automatically_active,
        neg_risk_augmented = excluded.neg_risk_augmented,
        pending_deployment = excluded.pending_deployment,
        deploying = excluded.deploying,
        updated_at = excluded.updated_at
    `);
  }

  upsertBatch(events: Event[]): void {
    const upsertTransaction = this.db.transaction((events: Event[]) => {
      for (const event of events) {
        this.upsertStmt.run(
          event.id,
          event.ticker,
          event.slug,
          event.title,
          event.description,
          event.resolution_source,
          event.start_date,
          event.creation_date,
          event.end_date,
          event.image,
          event.icon,
          event.active ? 1 : 0,
          event.closed ? 1 : 0,
          event.archived ? 1 : 0,
          event.new ? 1 : 0,
          event.featured ? 1 : 0,
          event.restricted ? 1 : 0,
          event.liquidity,
          event.volume,
          event.open_interest,
          event.competitive,
          event.volume_1mo,
          event.volume_1yr,
          event.comment_count,
          event.enable_order_book !== null ? (event.enable_order_book ? 1 : 0) : null,
          event.cyom !== null ? (event.cyom ? 1 : 0) : null,
          event.show_all_outcomes !== null ? (event.show_all_outcomes ? 1 : 0) : null,
          event.show_market_images !== null ? (event.show_market_images ? 1 : 0) : null,
          event.enable_neg_risk !== null ? (event.enable_neg_risk ? 1 : 0) : null,
          event.automatically_active !== null ? (event.automatically_active ? 1 : 0) : null,
          event.neg_risk_augmented !== null ? (event.neg_risk_augmented ? 1 : 0) : null,
          event.pending_deployment !== null ? (event.pending_deployment ? 1 : 0) : null,
          event.deploying !== null ? (event.deploying ? 1 : 0) : null,
          event.created_at,
          event.updated_at
        );
      }
    });

    upsertTransaction(events);
  }

  findAll(options?: EventQueryOptions): Event[] {
    const {
      filter,
      tagId,
      closed,
      active,
      limit = 100,
      offset = 0,
      sortBy = "end_date",
      order = "desc",
    } = options || {};

    let sql = "SELECT DISTINCT events.* FROM events";
    const params: (string | number)[] = [];

    // Join with event_tags if filtering by tag
    if (tagId) {
      sql += " INNER JOIN event_tags ON events.id = event_tags.event_id";
    }

    const conditions: string[] = [];

    // Add tag filter
    if (tagId) {
      conditions.push("event_tags.tag_id = ?");
      params.push(tagId);
    }

    // Add text filter
    if (filter) {
      conditions.push("(events.title LIKE ? OR events.ticker LIKE ? OR events.description LIKE ?) COLLATE NOCASE");
      const filterPattern = `%${filter}%`;
      params.push(filterPattern, filterPattern, filterPattern);
    }

    // Add closed filter
    if (closed !== undefined) {
      conditions.push("events.closed = ?");
      params.push(closed ? 1 : 0);
    }

    // Add active filter
    if (active !== undefined) {
      conditions.push("events.active = ?");
      params.push(active ? 1 : 0);
    }

    if (conditions.length > 0) {
      sql += " WHERE " + conditions.join(" AND ");
    }

    // Add sorting
    const validSortFields: EventSortField[] = [
      "id",
      "ticker",
      "title",
      "end_date",
      "volume",
      "liquidity",
      "created_at",
      "updated_at",
    ];
    if (!validSortFields.includes(sortBy)) {
      throw new Error(`Invalid sort field: ${sortBy}`);
    }
    const validOrder = order === "desc" ? "DESC" : "ASC";
    sql += ` ORDER BY events.${sortBy} ${validOrder}`;

    // Add limit and offset
    if (limit !== null && limit !== undefined) {
      sql += " LIMIT ?";
      params.push(limit);
    }

    if (offset > 0) {
      sql += " OFFSET ?";
      params.push(offset);
    }

    const stmt = this.db.prepare(sql);
    const rows = stmt.all(...params) as any[];

    // Convert INTEGER back to boolean for boolean fields
    return rows.map((row) => ({
      ...row,
      active: Boolean(row.active),
      closed: Boolean(row.closed),
      archived: Boolean(row.archived),
      new: Boolean(row.new),
      featured: Boolean(row.featured),
      restricted: Boolean(row.restricted),
      enable_order_book: row.enable_order_book !== null ? Boolean(row.enable_order_book) : null,
      cyom: row.cyom !== null ? Boolean(row.cyom) : null,
      show_all_outcomes: row.show_all_outcomes !== null ? Boolean(row.show_all_outcomes) : null,
      show_market_images: row.show_market_images !== null ? Boolean(row.show_market_images) : null,
      enable_neg_risk: row.enable_neg_risk !== null ? Boolean(row.enable_neg_risk) : null,
      automatically_active: row.automatically_active !== null ? Boolean(row.automatically_active) : null,
      neg_risk_augmented: row.neg_risk_augmented !== null ? Boolean(row.neg_risk_augmented) : null,
      pending_deployment: row.pending_deployment !== null ? Boolean(row.pending_deployment) : null,
      deploying: row.deploying !== null ? Boolean(row.deploying) : null,
    })) as Event[];
  }

  count(options?: { filter?: string; tagId?: string; closed?: boolean; active?: boolean }): number {
    const { filter, tagId, closed, active } = options || {};

    let sql = "SELECT COUNT(DISTINCT events.id) as count FROM events";
    const params: (string | number)[] = [];

    if (tagId) {
      sql += " INNER JOIN event_tags ON events.id = event_tags.event_id";
    }

    const conditions: string[] = [];

    if (tagId) {
      conditions.push("event_tags.tag_id = ?");
      params.push(tagId);
    }

    if (filter) {
      conditions.push("(events.title LIKE ? OR events.ticker LIKE ? OR events.description LIKE ?) COLLATE NOCASE");
      const filterPattern = `%${filter}%`;
      params.push(filterPattern, filterPattern, filterPattern);
    }

    if (closed !== undefined) {
      conditions.push("events.closed = ?");
      params.push(closed ? 1 : 0);
    }

    if (active !== undefined) {
      conditions.push("events.active = ?");
      params.push(active ? 1 : 0);
    }

    if (conditions.length > 0) {
      sql += " WHERE " + conditions.join(" AND ");
    }

    const stmt = this.db.prepare(sql);
    const result = stmt.get(...params) as { count: number };
    return result.count;
  }

  findById(eventId: string): Event | null {
    const stmt = this.db.prepare("SELECT * FROM events WHERE id = ?");
    const row = stmt.get(eventId) as any;

    if (!row) return null;

    return {
      ...row,
      active: Boolean(row.active),
      closed: Boolean(row.closed),
      archived: Boolean(row.archived),
      new: Boolean(row.new),
      featured: Boolean(row.featured),
      restricted: Boolean(row.restricted),
      enable_order_book: row.enable_order_book !== null ? Boolean(row.enable_order_book) : null,
      cyom: row.cyom !== null ? Boolean(row.cyom) : null,
      show_all_outcomes: row.show_all_outcomes !== null ? Boolean(row.show_all_outcomes) : null,
      show_market_images: row.show_market_images !== null ? Boolean(row.show_market_images) : null,
      enable_neg_risk: row.enable_neg_risk !== null ? Boolean(row.enable_neg_risk) : null,
      automatically_active: row.automatically_active !== null ? Boolean(row.automatically_active) : null,
      neg_risk_augmented: row.neg_risk_augmented !== null ? Boolean(row.neg_risk_augmented) : null,
      pending_deployment: row.pending_deployment !== null ? Boolean(row.pending_deployment) : null,
      deploying: row.deploying !== null ? Boolean(row.deploying) : null,
    } as Event;
  }

  findBySlug(slug: string): Event | null {
    const stmt = this.db.prepare("SELECT * FROM events WHERE slug = ?");
    const row = stmt.get(slug) as any;

    if (!row) return null;

    return {
      ...row,
      active: Boolean(row.active),
      closed: Boolean(row.closed),
      archived: Boolean(row.archived),
      new: Boolean(row.new),
      featured: Boolean(row.featured),
      restricted: Boolean(row.restricted),
      enable_order_book: row.enable_order_book !== null ? Boolean(row.enable_order_book) : null,
      cyom: row.cyom !== null ? Boolean(row.cyom) : null,
      show_all_outcomes: row.show_all_outcomes !== null ? Boolean(row.show_all_outcomes) : null,
      show_market_images: row.show_market_images !== null ? Boolean(row.show_market_images) : null,
      enable_neg_risk: row.enable_neg_risk !== null ? Boolean(row.enable_neg_risk) : null,
      automatically_active: row.automatically_active !== null ? Boolean(row.automatically_active) : null,
      neg_risk_augmented: row.neg_risk_augmented !== null ? Boolean(row.neg_risk_augmented) : null,
      pending_deployment: row.pending_deployment !== null ? Boolean(row.pending_deployment) : null,
      deploying: row.deploying !== null ? Boolean(row.deploying) : null,
    } as Event;
  }

  deleteByTag(tagId: string): void {
    const stmt = this.db.prepare(`
      DELETE FROM events
      WHERE id IN (
        SELECT event_id FROM event_tags WHERE tag_id = ?
      )
    `);
    stmt.run(tagId);
  }

  findWithMarkets(options?: EventQueryOptions): EventWithMarkets[] {
    const events = this.findAll(options);
    const marketsRepo = new MarketsRepository(this.db);

    return events.map((event) => ({
      ...event,
      markets: marketsRepo.findByEventId(event.id),
    }));
  }
}
