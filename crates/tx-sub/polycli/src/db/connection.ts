import { Database } from "bun:sqlite";
import { initializeSchema } from "./schema";

export function createDatabase(path: string): Database {
  try {
    const db = new Database(path, { create: true });

    // Enable WAL mode for better concurrency
    db.exec("PRAGMA journal_mode = WAL");

    // Initialize schema
    initializeSchema(db);

    return db;
  } catch (error) {
    throw new Error(
      `Failed to create/connect to database at ${path}: ${error instanceof Error ? error.message : String(error)}`
    );
  }
}
