import chalk from "chalk";
import { Database } from "bun:sqlite";
import { EventsRepository } from "../../db/events";
import { TagsRepository } from "../../db/tags";
import { TagResolver } from "../../utils/tag-resolver";
import type { EventSortField } from "../../db/types";
import {
  formatEventsAsTable,
  formatEventsAsJson,
  formatError,
} from "../../utils/format";

export interface ListOptions {
  dbPath: string;
  tag?: string;
  filter?: string;
  closed?: boolean;
  active?: boolean;
  format: "table" | "json";
  limit?: number;
  offset: number;
  sortBy: EventSortField;
  order: "asc" | "desc";
  all: boolean;
  showCount: boolean;
}

export async function listMarketsCommand(options: ListOptions): Promise<void> {
  try {
    validateOptions(options);

    const db = new Database(options.dbPath, { readonly: true });
    const eventsRepo = new EventsRepository(db);

    let tagId: string | undefined;
    if (options.tag) {
      const tagsRepo = new TagsRepository(db);
      const tagResolver = new TagResolver(tagsRepo);
      const tag = tagResolver.resolveWithMetadata(options.tag);
      tagId = tag.id;

      if (options.format === "table") {
        console.log(chalk.cyan(`Filtering by tag: ${tag.label} (ID: ${tag.id})\n`));
      }
    }

    const needCount = options.showCount || options.format === "table";

    let totalCount: number | undefined;
    if (needCount) {
      totalCount = eventsRepo.count({
        filter: options.filter,
        tagId,
        closed: options.closed,
        active: options.active,
      });
    }

    const limit = options.all ? null : options.limit ?? 100;

    const events = eventsRepo.findWithMarkets({
      filter: options.filter,
      tagId,
      closed: options.closed,
      active: options.active,
      limit,
      offset: options.offset,
      sortBy: options.sortBy,
      order: options.order,
    });

    if (options.all && events.length > 1000) {
      console.error(
        chalk.yellow(
          `Warning: Rendering ${events.length.toLocaleString()} results may take a while...`
        )
      );
    }

    let output: string;
    if (options.format === "table") {
      output = formatEventsAsTable(events, {
        showCount: true,
        totalCount,
        offset: options.offset,
      });
    } else {
      output = formatEventsAsJson(events, {
        showCount: options.showCount,
        totalCount,
        offset: options.offset,
      });
    }

    console.log(output);

    db.close();
  } catch (error) {
    if (error instanceof Error && error.message.includes("no such table")) {
      console.error(
        formatError(
          "Database not initialized. Run 'markets save' command first",
          error
        )
      );
      process.exit(2);
    }

    if (error instanceof Error && error.message.includes("not found in database")) {
      console.error(formatError("Tag not found", error));
      process.exit(1);
    }

    console.error(formatError("Database error", error));
    process.exit(2);
  }
}

function validateOptions(options: ListOptions): void {
  if (options.limit !== undefined && !options.all) {
    if (options.limit < 1 || options.limit > 10000) {
      console.error(
        chalk.red("Error: --limit must be between 1 and 10000")
      );
      process.exit(3);
    }
  }

  if (options.offset < 0) {
    console.error(chalk.red("Error: --offset must be non-negative"));
    process.exit(3);
  }

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
  if (!validSortFields.includes(options.sortBy)) {
    console.error(
      chalk.red(
        `Error: --sort-by must be one of: ${validSortFields.join(", ")}`
      )
    );
    process.exit(3);
  }

  if (options.order !== "asc" && options.order !== "desc") {
    console.error(chalk.red("Error: --order must be 'asc' or 'desc'"));
    process.exit(3);
  }

  if (options.format !== "table" && options.format !== "json") {
    console.error(chalk.red("Error: --format must be 'table' or 'json'"));
    process.exit(3);
  }

  if (options.closed !== undefined && typeof options.closed !== "boolean") {
    console.error(chalk.red("Error: --closed must be 'true' or 'false'"));
    process.exit(3);
  }

  if (options.active !== undefined && typeof options.active !== "boolean") {
    console.error(chalk.red("Error: --active must be 'true' or 'false'"));
    process.exit(3);
  }
}
