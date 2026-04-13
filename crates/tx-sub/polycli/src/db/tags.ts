import { Database } from "bun:sqlite";
import type { Tag, QueryOptions, TagSortField } from "./types";

export class TagsRepository {
  private db: Database;
  private upsertStmt: ReturnType<Database["prepare"]>;

  constructor(db: Database) {
    this.db = db;

    // Prepare upsert statement for performance
    this.upsertStmt = db.prepare(`
      INSERT INTO tags (id, label, slug, publishedAt, createdAt, updatedAt)
      VALUES (?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        label = excluded.label,
        slug = excluded.slug,
        publishedAt = excluded.publishedAt,
        updatedAt = excluded.updatedAt
    `);
  }

  upsertBatch(tags: Tag[]): void {
    // Use transaction for batch operations
    const upsertTransaction = this.db.transaction((tags: Tag[]) => {
      for (const tag of tags) {
        this.upsertStmt.run(
          tag.id,
          tag.label,
          tag.slug,
          tag.publishedAt,
          tag.createdAt,
          tag.updatedAt
        );
      }
    });

    upsertTransaction(tags);
  }

  findAll(options?: QueryOptions): Tag[] {
    const {
      filter,
      limit = 100,
      offset = 0,
      sortBy = "label",
      order = "asc",
    } = options || {};

    let sql = "SELECT * FROM tags";
    const params: (string | number)[] = [];

    // Add filter condition
    if (filter) {
      sql += " WHERE (label LIKE ? OR slug LIKE ?) COLLATE NOCASE";
      const filterPattern = `%${filter}%`;
      params.push(filterPattern, filterPattern);
    }

    // Add sorting
    const validSortFields: TagSortField[] = [
      "id",
      "label",
      "slug",
      "publishedAt",
      "createdAt",
      "updatedAt",
    ];
    if (!validSortFields.includes(sortBy)) {
      throw new Error(`Invalid sort field: ${sortBy}`);
    }
    const validOrder = order === "desc" ? "DESC" : "ASC";
    sql += ` ORDER BY ${sortBy} ${validOrder}`;

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
    return stmt.all(...params) as Tag[];
  }

  count(options?: { filter?: string }): number {
    const { filter } = options || {};

    let sql = "SELECT COUNT(*) as count FROM tags";
    const params: string[] = [];

    if (filter) {
      sql += " WHERE (label LIKE ? OR slug LIKE ?) COLLATE NOCASE";
      const filterPattern = `%${filter}%`;
      params.push(filterPattern, filterPattern);
    }

    const stmt = this.db.prepare(sql);
    const result = stmt.get(...params) as { count: number };
    return result.count;
  }

  findById(id: string): Tag | null {
    const stmt = this.db.prepare("SELECT * FROM tags WHERE id = ?");
    return (stmt.get(id) as Tag) || null;
  }

  findBySlug(slug: string): Tag | null {
    const stmt = this.db.prepare("SELECT * FROM tags WHERE slug = ?");
    return (stmt.get(slug) as Tag) || null;
  }

  truncate(): void {
    this.db.exec("DELETE FROM tags");
  }
}
