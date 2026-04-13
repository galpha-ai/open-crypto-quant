import chalk from "chalk";
import { Database } from "bun:sqlite";
import { TagsRepository } from "../../db/tags";
import {
  formatTagAsTable,
  formatTagAsJson,
  formatError,
} from "../../utils/format";

export interface GetOptions {
  dbPath: string;
  format: "table" | "json";
}

export async function getTagCommand(
  idOrSlug: string,
  options: GetOptions
): Promise<void> {
  try {
    // Validate format
    if (options.format !== "table" && options.format !== "json") {
      console.error(chalk.red("Error: --format must be 'table' or 'json'"));
      process.exit(3);
    }

    // Open database connection
    const db = new Database(options.dbPath, { readonly: true });
    const tagsRepo = new TagsRepository(db);

    // Determine if argument is numeric (ID) or string (slug)
    const isNumeric = /^\d+$/.test(idOrSlug);

    // Query by ID or slug
    const tag = isNumeric
      ? tagsRepo.findById(idOrSlug)
      : tagsRepo.findBySlug(idOrSlug);

    // Check if tag was found
    if (!tag) {
      console.error(chalk.red("Tag not found"));
      db.close();
      process.exit(1);
    }

    // Format and output
    const output =
      options.format === "table"
        ? formatTagAsTable(tag)
        : formatTagAsJson(tag);

    console.log(output);

    // Close database
    db.close();
  } catch (error) {
    if (error instanceof Error && error.message.includes("no such table")) {
      console.error(
        formatError(
          "Database not initialized. Run 'tags save' command first",
          error
        )
      );
      process.exit(2);
    }

    console.error(formatError("Database error", error));
    process.exit(2);
  }
}
