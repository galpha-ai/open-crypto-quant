import chalk from "chalk";
import { Database } from "bun:sqlite";
import { EventsRepository } from "../../db/events";
import { MarketsRepository } from "../../db/markets";
import {
  formatEventAsTable,
  formatEventAsJson,
  formatError,
} from "../../utils/format";

export interface GetOptions {
  dbPath: string;
  format: "table" | "json";
}

export async function getMarketCommand(
  idOrSlug: string,
  options: GetOptions
): Promise<void> {
  try {
    if (options.format !== "table" && options.format !== "json") {
      console.error(chalk.red("Error: --format must be 'table' or 'json'"));
      process.exit(3);
    }

    const db = new Database(options.dbPath, { readonly: true });
    const eventsRepo = new EventsRepository(db);
    const marketsRepo = new MarketsRepository(db);

    const isNumeric = /^\d+$/.test(idOrSlug);

    const event = isNumeric
      ? eventsRepo.findById(idOrSlug)
      : eventsRepo.findBySlug(idOrSlug);

    if (!event) {
      console.error(chalk.red("Event not found"));
      db.close();
      process.exit(1);
    }

    const markets = marketsRepo.findByEventId(event.id);

    const eventWithMarkets = {
      ...event,
      markets,
    };

    const output =
      options.format === "table"
        ? formatEventAsTable(eventWithMarkets)
        : formatEventAsJson(eventWithMarkets);

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

    console.error(formatError("Database error", error));
    process.exit(2);
  }
}
