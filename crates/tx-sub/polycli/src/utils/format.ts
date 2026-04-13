import chalk from "chalk";
import Table from "cli-table3";
import type { Tag, EventWithMarkets } from "../db/types";

export function formatProgress(
  totalFetched: number,
  offset: number,
  currentBatchSize: number
): string {
  return chalk.cyan(
    `Fetched ${totalFetched} tags (offset: ${offset}, batch: ${currentBatchSize})`
  );
}

export function formatSuccess(totalFetched: number, dbPath: string): string {
  return chalk.green(`Successfully saved ${totalFetched} tags to ${dbPath}`);
}

export function formatError(message: string, error?: unknown): string {
  const errorMsg = error instanceof Error ? error.message : String(error);
  return chalk.red(`${message}: ${errorMsg}`);
}

export function isInteractive(): boolean {
  return process.stdout.isTTY || false;
}

export interface FormatOptions {
  showCount?: boolean;
  totalCount?: number;
  offset?: number;
}

export function formatTagsAsTable(
  tags: Tag[],
  options?: FormatOptions
): string {
  const { showCount = true, totalCount, offset = 0 } = options || {};

  let output = "";

  // Add count header for table format (always shown by default)
  if (showCount && totalCount !== undefined) {
    const start = offset + 1;
    const end = offset + tags.length;
    output += chalk.bold(
      `Showing ${start}-${end} of ${totalCount.toLocaleString()} tags\n\n`
    );
  }

  // Create table
  const table = new Table({
    head: [
      chalk.cyan("ID"),
      chalk.cyan("Label"),
      chalk.cyan("Slug"),
      chalk.cyan("Published At"),
      chalk.cyan("Created At"),
      chalk.cyan("Updated At"),
    ],
    style: {
      head: [],
      border: [],
    },
    colWidths: [10, 25, 25, 28, 28, 28],
    wordWrap: true,
  });

  // Add rows
  for (const tag of tags) {
    table.push([
      tag.id,
      truncateString(tag.label, 23),
      truncateString(tag.slug, 23),
      formatTimestamp(tag.publishedAt),
      formatTimestamp(tag.createdAt),
      formatTimestamp(tag.updatedAt),
    ]);
  }

  output += table.toString();
  return output;
}

export function formatTagsAsJson(
  tags: Tag[],
  options?: FormatOptions
): string {
  const { showCount = false, totalCount, offset = 0 } = options || {};

  if (showCount && totalCount !== undefined) {
    // Include metadata with results
    return JSON.stringify(
      {
        metadata: {
          total: totalCount,
          offset,
          count: tags.length,
        },
        data: tags,
      },
      null,
      2
    );
  }

  // Just return the array
  return JSON.stringify(tags, null, 2);
}

export function formatTagAsTable(tag: Tag): string {
  const table = new Table({
    style: {
      head: [],
      border: [],
    },
  });

  table.push(
    { [chalk.cyan("ID")]: tag.id },
    { [chalk.cyan("Label")]: tag.label },
    { [chalk.cyan("Slug")]: tag.slug },
    { [chalk.cyan("Published At")]: formatTimestamp(tag.publishedAt) },
    { [chalk.cyan("Created At")]: formatTimestamp(tag.createdAt) },
    { [chalk.cyan("Updated At")]: formatTimestamp(tag.updatedAt) }
  );

  return table.toString();
}

export function formatTagAsJson(tag: Tag): string {
  return JSON.stringify(tag, null, 2);
}

function truncateString(str: string, maxLength: number): string {
  if (str.length <= maxLength) {
    return str;
  }
  return str.substring(0, maxLength - 3) + "...";
}

function formatTimestamp(timestamp: string | null): string {
  if (!timestamp) {
    return "N/A";
  }

  try {
    const date = new Date(timestamp);
    return date.toISOString();
  } catch {
    return timestamp;
  }
}

export function formatEventsAsTable(
  events: EventWithMarkets[],
  options?: FormatOptions
): string {
  const { showCount = true, totalCount, offset = 0 } = options || {};

  let output = "";

  if (showCount && totalCount !== undefined) {
    const start = offset + 1;
    const end = offset + events.length;
    output += chalk.bold(
      `Showing ${start}-${end} of ${totalCount.toLocaleString()} events\n\n`
    );
  }

  const table = new Table({
    head: [
      chalk.cyan("ID"),
      chalk.cyan("Ticker"),
      chalk.cyan("Title"),
      chalk.cyan("Markets"),
      chalk.cyan("Volume"),
      chalk.cyan("Liquidity"),
      chalk.cyan("Active"),
      chalk.cyan("Closed"),
      chalk.cyan("End Date"),
    ],
    style: {
      head: [],
      border: [],
    },
    colWidths: [10, 15, 35, 10, 12, 12, 8, 8, 28],
    wordWrap: true,
  });

  for (const event of events) {
    table.push([
      event.id,
      truncateString(event.ticker, 13),
      truncateString(event.title, 33),
      event.markets.length.toString(),
      formatNumber(event.volume),
      formatNumber(event.liquidity),
      event.active ? chalk.green("Yes") : chalk.red("No"),
      event.closed ? chalk.red("Yes") : chalk.green("No"),
      formatTimestamp(event.end_date),
    ]);
  }

  output += table.toString();
  return output;
}

export function formatEventsAsJson(
  events: EventWithMarkets[],
  options?: FormatOptions
): string {
  const { showCount = false, totalCount, offset = 0 } = options || {};

  if (showCount && totalCount !== undefined) {
    return JSON.stringify(
      {
        metadata: {
          total: totalCount,
          offset,
          count: events.length,
        },
        data: events,
      },
      null,
      2
    );
  }

  return JSON.stringify(events, null, 2);
}

export function formatEventAsTable(event: EventWithMarkets): string {
  const table = new Table({
    style: {
      head: [],
      border: [],
    },
  });

  table.push(
    { [chalk.cyan("ID")]: event.id },
    { [chalk.cyan("Ticker")]: event.ticker },
    { [chalk.cyan("Slug")]: event.slug },
    { [chalk.cyan("Title")]: event.title },
    { [chalk.cyan("Description")]: event.description || "N/A" },
    { [chalk.cyan("Active")]: event.active ? "Yes" : "No" },
    { [chalk.cyan("Closed")]: event.closed ? "Yes" : "No" },
    { [chalk.cyan("Featured")]: event.featured ? "Yes" : "No" },
    { [chalk.cyan("Volume")]: formatNumber(event.volume) },
    { [chalk.cyan("Liquidity")]: formatNumber(event.liquidity) },
    { [chalk.cyan("Volume (1mo)")]: formatNumber(event.volume_1mo) },
    { [chalk.cyan("Volume (1yr)")]: formatNumber(event.volume_1yr) },
    { [chalk.cyan("Comment Count")]: event.comment_count?.toString() || "N/A" },
    { [chalk.cyan("Start Date")]: formatTimestamp(event.start_date) },
    { [chalk.cyan("End Date")]: formatTimestamp(event.end_date) },
    { [chalk.cyan("Created At")]: formatTimestamp(event.created_at) },
    { [chalk.cyan("Updated At")]: formatTimestamp(event.updated_at) }
  );

  let output = table.toString();

  if (event.markets.length > 0) {
    output += "\n\n" + chalk.bold.cyan(`Markets (${event.markets.length}):\n`);

    const marketsTable = new Table({
      head: [
        chalk.cyan("ID"),
        chalk.cyan("Question"),
        chalk.cyan("Outcomes"),
        chalk.cyan("Prices"),
        chalk.cyan("Volume"),
        chalk.cyan("Active"),
        chalk.cyan("Closed"),
      ],
      style: {
        head: [],
        border: [],
      },
      colWidths: [10, 40, 15, 20, 12, 8, 8],
      wordWrap: true,
    });

    for (const market of event.markets) {
      let outcomes = "N/A";
      let prices = "N/A";

      try {
        const outcomesArray = JSON.parse(market.outcomes);
        outcomes = outcomesArray.join(", ");
      } catch {}

      try {
        const pricesArray = JSON.parse(market.outcome_prices);
        prices = pricesArray.join(", ");
      } catch {}

      marketsTable.push([
        market.id,
        truncateString(market.question, 38),
        truncateString(outcomes, 13),
        truncateString(prices, 18),
        formatNumber(market.volume),
        market.active ? chalk.green("Yes") : chalk.red("No"),
        market.closed ? chalk.red("Yes") : chalk.green("No"),
      ]);
    }

    output += marketsTable.toString();
  }

  return output;
}

export function formatEventAsJson(event: EventWithMarkets): string {
  return JSON.stringify(event, null, 2);
}

function formatNumber(value: number | null | undefined): string {
  if (value === null || value === undefined) {
    return "N/A";
  }

  if (value >= 1000000) {
    return `${(value / 1000000).toFixed(2)}M`;
  } else if (value >= 1000) {
    return `${(value / 1000).toFixed(2)}K`;
  }

  return value.toFixed(2);
}
