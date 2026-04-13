import chalk from "chalk";
import { Database } from "bun:sqlite";
import { TagsRepository } from "../../db/tags";
import type { TagSortField } from "../../db/types";
import {
  formatTagsAsTable,
  formatTagsAsJson,
  formatError,
} from "../../utils/format";

export interface ListOptions {
  dbPath: string;
  filter?: string;
  format: "table" | "json";
  limit?: number;
  offset: number;
  sortBy: TagSortField;
  order: "asc" | "desc";
  all: boolean;
  showCount: boolean;
}

export async function listTagsCommand(options: ListOptions): Promise<void> {
  try {
    // Validate options
    validateOptions(options);

    // Open database connection
    const db = new Database(options.dbPath, { readonly: true });
    const tagsRepo = new TagsRepository(db);

    // Determine if we need to get count
    const needCount = options.showCount || options.format === "table";

    // Get total count if needed
    let totalCount: number | undefined;
    if (needCount) {
      totalCount = tagsRepo.count({
        filter: options.filter,
      });
    }

    // Determine actual limit
    const limit = options.all ? null : options.limit ?? 100;

    // Query tags
    const tags = tagsRepo.findAll({
      filter: options.filter,
      limit,
      offset: options.offset,
      sortBy: options.sortBy,
      order: options.order,
    });

    // Warn about large result sets if using --all
    if (options.all && tags.length > 10000) {
      console.error(
        chalk.yellow(
          `Warning: Rendering ${tags.length.toLocaleString()} results may take a while...`
        )
      );
    }

    // Format and output
    let output: string;
    if (options.format === "table") {
      output = formatTagsAsTable(tags, {
        showCount: true,
        totalCount,
        offset: options.offset,
      });
    } else {
      output = formatTagsAsJson(tags, {
        showCount: options.showCount,
        totalCount,
        offset: options.offset,
      });
    }

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

function validateOptions(options: ListOptions): void {
  // Validate limit
  if (options.limit !== undefined && !options.all) {
    if (options.limit < 1 || options.limit > 10000) {
      console.error(
        chalk.red("Error: --limit must be between 1 and 10000")
      );
      process.exit(3);
    }
  }

  // Validate offset
  if (options.offset < 0) {
    console.error(chalk.red("Error: --offset must be non-negative"));
    process.exit(3);
  }

  // Validate sort field
  const validSortFields: TagSortField[] = [
    "id",
    "label",
    "slug",
    "publishedAt",
    "createdAt",
    "updatedAt",
  ];
  if (!validSortFields.includes(options.sortBy)) {
    console.error(
      chalk.red(
        `Error: --sort-by must be one of: ${validSortFields.join(", ")}`
      )
    );
    process.exit(3);
  }

  // Validate order
  if (options.order !== "asc" && options.order !== "desc") {
    console.error(chalk.red("Error: --order must be 'asc' or 'desc'"));
    process.exit(3);
  }

  // Validate format
  if (options.format !== "table" && options.format !== "json") {
    console.error(chalk.red("Error: --format must be 'table' or 'json'"));
    process.exit(3);
  }
}
