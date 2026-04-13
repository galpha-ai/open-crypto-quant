import chalk from "chalk";
import { Database } from "bun:sqlite";
import { EventsRepository } from "../../db/events";
import { MarketsRepository } from "../../db/markets";
import { formatError } from "../../utils/format";
import { PolymarketWebSocketClient } from "../../stream/websocket-client";
import { StreamLogger } from "../../stream/logger";
import type { MarketEventType } from "../../stream/types";

export interface StreamMarketOptions {
  dbPath: string;
  eventTypes?: string;
  logFile?: string;
  verbose?: boolean;
  noPing?: boolean;
}

const POLYMARKET_WS_URL = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

const VALID_EVENT_TYPES: MarketEventType[] = [
  "book",
  "price_change",
  "tick_size_change",
  "last_trade_price",
];

/**
 * Stream market data by looking up asset IDs from local database
 */
export async function streamMarketCommand(
  eventSlugOrId: string,
  options: StreamMarketOptions
): Promise<void> {
  try {
    // Open database
    const db = new Database(options.dbPath, { readonly: true });
    const eventsRepo = new EventsRepository(db);
    const marketsRepo = new MarketsRepository(db);

    // Find event by ID or slug
    const isNumeric = /^\d+$/.test(eventSlugOrId);
    const event = isNumeric
      ? eventsRepo.findById(eventSlugOrId)
      : eventsRepo.findBySlug(eventSlugOrId);

    if (!event) {
      console.error(chalk.red("Event not found"));
      db.close();
      process.exit(1);
    }

    // Get markets for this event
    const markets = marketsRepo.findByEventId(event.id);

    if (markets.length === 0) {
      console.error(chalk.red("No markets found for this event"));
      db.close();
      process.exit(1);
    }

    // Extract all asset IDs from markets
    const assetIds: string[] = [];
    for (const market of markets) {
      if (market.clob_token_ids) {
        try {
          const tokenIds = JSON.parse(market.clob_token_ids) as string[];
          assetIds.push(...tokenIds);
        } catch (error) {
          console.warn(
            chalk.yellow(
              `Warning: Failed to parse clob_token_ids for market ${market.id}`
            )
          );
        }
      }
    }

    if (assetIds.length === 0) {
      console.error(
        chalk.red(
          "No asset IDs found for this event. Try running 'markets save' again to fetch the latest data."
        )
      );
      db.close();
      process.exit(1);
    }

    // Close database connection
    db.close();

    // Show what we're streaming
    console.log(chalk.green(`\nStreaming event: ${event.title}`));
    console.log(chalk.gray(`Markets: ${markets.length}`));
    console.log(chalk.gray(`Asset IDs: ${assetIds.length}`));
    if (options.verbose) {
      console.log(chalk.gray(`\nAsset IDs:\n${assetIds.join("\n")}\n`));
    }

    // Parse event types filter
    let eventTypesSet: Set<MarketEventType> | undefined;
    if (options.eventTypes) {
      const types = options.eventTypes
        .split(",")
        .map((t) => t.trim() as MarketEventType);

      // Validate event types
      const invalidTypes = types.filter((t) => !VALID_EVENT_TYPES.includes(t));
      if (invalidTypes.length > 0) {
        console.error(
          chalk.red(`Error: Invalid event types: ${invalidTypes.join(", ")}`)
        );
        console.error(
          chalk.gray(`Valid types: ${VALID_EVENT_TYPES.join(", ")}`)
        );
        process.exit(1);
      }

      eventTypesSet = new Set(types);
    }

    // Create logger
    const logger = new StreamLogger({
      logFile: options.logFile,
      eventTypes: eventTypesSet,
      verbose: options.verbose,
    });

    // Create WebSocket client
    const client = new PolymarketWebSocketClient({
      url: POLYMARKET_WS_URL,
      assetIds,
      enablePing: !options.noPing,
      onConnect: () => {
        logger.logVerbose("Connected and subscribed", { assetCount: assetIds.length });
      },
      onMessage: (message) => {
        logger.logMessage(message);
      },
      onError: (error) => {
        console.error(chalk.red(`WebSocket error: ${error.message}`));

        // Exit on max reconnect attempts exceeded
        if (error.message.includes("Max reconnection attempts")) {
          logger.close();
          process.exit(1);
        }
      },
      onDisconnect: () => {
        logger.logVerbose("Disconnected");
      },
    });

    // Handle graceful shutdown
    const shutdown = () => {
      logger.logVerbose("Shutting down...");
      client.disconnect();
      logger.close();
      process.exit(0);
    };

    process.on("SIGINT", shutdown);
    process.on("SIGTERM", shutdown);

    // Connect and start streaming
    logger.logVerbose(`Connecting to ${POLYMARKET_WS_URL}`);
    client.connect();

    // Keep the process alive
    await new Promise(() => {});
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
