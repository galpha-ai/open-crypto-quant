import { appendFileSync, closeSync, openSync } from "fs";
import type { MarketMessage, MarketEventType } from "./types";

export interface StreamLoggerOptions {
  logFile?: string;
  eventTypes?: Set<MarketEventType>;
  verbose?: boolean;
}

/**
 * Logger for WebSocket stream output
 * Outputs JSON lines to stdout and optionally to a file
 */
export class StreamLogger {
  private fileHandle?: number;
  private readonly eventTypes?: Set<MarketEventType>;
  private readonly verbose: boolean;

  constructor(options: StreamLoggerOptions) {
    this.eventTypes = options.eventTypes;
    this.verbose = options.verbose ?? false;

    if (options.logFile) {
      try {
        this.fileHandle = openSync(options.logFile, "a");
      } catch (error) {
        const err = error instanceof Error ? error : new Error(String(error));
        console.error(`Failed to open log file: ${err.message}`);
      }
    }
  }

  /**
   * Log market message to stdout (and file if configured)
   * Filters by event type if specified
   */
  logMessage(message: MarketMessage): void {
    // Filter by event type if specified
    if (this.eventTypes && !this.eventTypes.has(message.event_type)) {
      return;
    }

    const jsonLine = JSON.stringify(message);

    // Write to stdout
    console.log(jsonLine);

    // Write to file if configured
    if (this.fileHandle !== undefined) {
      try {
        appendFileSync(this.fileHandle, jsonLine + "\n");
      } catch (error) {
        const err = error instanceof Error ? error : new Error(String(error));
        console.error(`Failed to write to log file: ${err.message}`);
      }
    }
  }

  /**
   * Log verbose connection events
   */
  logVerbose(message: string, data?: unknown): void {
    if (!this.verbose) {
      return;
    }

    const timestamp = new Date().toISOString();
    const logMessage = data
      ? `[${timestamp}] ${message}: ${JSON.stringify(data)}`
      : `[${timestamp}] ${message}`;

    console.error(logMessage);
  }

  /**
   * Close file handle
   */
  close(): void {
    if (this.fileHandle !== undefined) {
      try {
        closeSync(this.fileHandle);
      } catch (error) {
        const err = error instanceof Error ? error : new Error(String(error));
        console.error(`Failed to close log file: ${err.message}`);
      }
    }
  }
}
