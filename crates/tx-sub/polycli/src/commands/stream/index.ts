import { PolymarketWebSocketClient } from "../../stream/websocket-client";
import { StreamLogger } from "../../stream/logger";
import type { MarketEventType } from "../../stream/types";

export interface StreamOptions {
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
 * Stream command - Subscribe to real-time market data via WebSocket
 */
export async function streamCommand(
  assetIds: string,
  options: StreamOptions
): Promise<void> {
  // Parse comma-separated asset IDs
  const assets = assetIds
    .split(",")
    .map((id) => id.trim())
    .filter((id) => id.length > 0);

  if (assets.length === 0) {
    console.error("Error: At least one asset ID is required");
    process.exit(1);
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
        `Error: Invalid event types: ${invalidTypes.join(", ")}`
      );
      console.error(`Valid types: ${VALID_EVENT_TYPES.join(", ")}`);
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
    assetIds: assets,
    enablePing: !options.noPing,
    onConnect: () => {
      logger.logVerbose("Connected and subscribed", { assets });
    },
    onMessage: (message) => {
      logger.logMessage(message);
    },
    onError: (error) => {
      console.error(`WebSocket error: ${error.message}`);

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
}
