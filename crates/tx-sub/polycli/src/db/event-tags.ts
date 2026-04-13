import { Database } from "bun:sqlite";
import type { EventTag, Tag } from "./types";

export class EventTagsRepository {
  private db: Database;
  private upsertStmt: ReturnType<Database["prepare"]>;

  constructor(db: Database) {
    this.db = db;

    // Prepare upsert statement for performance
    this.upsertStmt = db.prepare(`
      INSERT INTO event_tags (event_id, tag_id, tag_label)
      VALUES (?, ?, ?)
      ON CONFLICT(event_id, tag_id) DO NOTHING
    `);
  }

  upsertBatch(associations: EventTag[]): void {
    const upsertTransaction = this.db.transaction((associations: EventTag[]) => {
      for (const assoc of associations) {
        this.upsertStmt.run(
          assoc.event_id,
          assoc.tag_id,
          assoc.tag_label
        );
      }
    });

    upsertTransaction(associations);
  }

  findEventIdsByTag(tagId: string): string[] {
    const stmt = this.db.prepare("SELECT event_id FROM event_tags WHERE tag_id = ?");
    const rows = stmt.all(tagId) as { event_id: string }[];
    return rows.map((row) => row.event_id);
  }

  findTagsByEvent(eventId: string): Tag[] {
    const stmt = this.db.prepare(`
      SELECT tags.*
      FROM tags
      INNER JOIN event_tags ON tags.id = event_tags.tag_id
      WHERE event_tags.event_id = ?
    `);
    return stmt.all(eventId) as Tag[];
  }

  deleteByTag(tagId: string): void {
    const stmt = this.db.prepare("DELETE FROM event_tags WHERE tag_id = ?");
    stmt.run(tagId);
  }

  deleteByEvent(eventId: string): void {
    const stmt = this.db.prepare("DELETE FROM event_tags WHERE event_id = ?");
    stmt.run(eventId);
  }
}
