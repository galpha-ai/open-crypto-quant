import { Database } from "bun:sqlite";
import type { Market } from "./types";

export class MarketsRepository {
  private db: Database;
  private upsertStmt: ReturnType<Database["prepare"]>;

  constructor(db: Database) {
    this.db = db;

    // Prepare upsert statement for performance
    this.upsertStmt = db.prepare(`
      INSERT INTO markets (
        id, event_id, question, condition_id, slug,
        start_date, end_date, liquidity, volume,
        outcomes, outcome_prices, clob_token_ids, active, closed,
        created_at, updated_at
      )
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        event_id = excluded.event_id,
        question = excluded.question,
        condition_id = excluded.condition_id,
        slug = excluded.slug,
        start_date = excluded.start_date,
        end_date = excluded.end_date,
        liquidity = excluded.liquidity,
        volume = excluded.volume,
        outcomes = excluded.outcomes,
        outcome_prices = excluded.outcome_prices,
        clob_token_ids = excluded.clob_token_ids,
        active = excluded.active,
        closed = excluded.closed,
        updated_at = excluded.updated_at
    `);
  }

  upsertBatch(markets: Market[]): void {
    const upsertTransaction = this.db.transaction((markets: Market[]) => {
      for (const market of markets) {
        this.upsertStmt.run(
          market.id,
          market.event_id,
          market.question,
          market.condition_id,
          market.slug,
          market.start_date,
          market.end_date,
          market.liquidity,
          market.volume,
          market.outcomes,
          market.outcome_prices,
          market.clob_token_ids,
          market.active ? 1 : 0,
          market.closed ? 1 : 0,
          market.created_at,
          market.updated_at
        );
      }
    });

    upsertTransaction(markets);
  }

  findByEventId(eventId: string): Market[] {
    const stmt = this.db.prepare("SELECT * FROM markets WHERE event_id = ?");
    const rows = stmt.all(eventId) as any[];

    return rows.map((row) => ({
      ...row,
      active: Boolean(row.active),
      closed: Boolean(row.closed),
    })) as Market[];
  }

  findById(marketId: string): Market | null {
    const stmt = this.db.prepare("SELECT * FROM markets WHERE id = ?");
    const row = stmt.get(marketId) as any;

    if (!row) return null;

    return {
      ...row,
      active: Boolean(row.active),
      closed: Boolean(row.closed),
    } as Market;
  }

  findBySlug(slug: string): Market | null {
    const stmt = this.db.prepare("SELECT * FROM markets WHERE slug = ?");
    const row = stmt.get(slug) as any;

    if (!row) return null;

    return {
      ...row,
      active: Boolean(row.active),
      closed: Boolean(row.closed),
    } as Market;
  }

  count(options?: { eventId?: string; closed?: boolean; active?: boolean }): number {
    const { eventId, closed, active } = options || {};

    let sql = "SELECT COUNT(*) as count FROM markets";
    const params: (string | number)[] = [];
    const conditions: string[] = [];

    if (eventId) {
      conditions.push("event_id = ?");
      params.push(eventId);
    }

    if (closed !== undefined) {
      conditions.push("closed = ?");
      params.push(closed ? 1 : 0);
    }

    if (active !== undefined) {
      conditions.push("active = ?");
      params.push(active ? 1 : 0);
    }

    if (conditions.length > 0) {
      sql += " WHERE " + conditions.join(" AND ");
    }

    const stmt = this.db.prepare(sql);
    const result = stmt.get(...params) as { count: number };
    return result.count;
  }

  deleteByEventId(eventId: string): void {
    const stmt = this.db.prepare("DELETE FROM markets WHERE event_id = ?");
    stmt.run(eventId);
  }
}
