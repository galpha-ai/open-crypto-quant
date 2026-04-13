#!/usr/bin/env bun

import { Command } from "commander";
import chalk from "chalk";
import { saveTagsCommand } from "./commands/tags/save";
import { listTagsCommand } from "./commands/tags/list";
import { getTagCommand } from "./commands/tags/get";
import { saveMarketsCommand } from "./commands/markets/save";
import { listMarketsCommand } from "./commands/markets/list";
import { getMarketCommand } from "./commands/markets/get";
import { streamMarketCommand } from "./commands/markets/stream";
import { streamCommand } from "./commands/stream/index";

const program = new Command();

program
  .name("polymarket-cli")
  .description(
    "CLI tool for testing Polymarket APIs and learning about markets, events, and tags"
  )
  .version("0.1.0");

// Tags command group
const tagsCommand = program
  .command("tags")
  .description("Manage Polymarket tags");

tagsCommand
  .command("save")
  .description("Fetch all tags from Polymarket API and save to local database")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option(
    "--batch-size <number>",
    "Number of records to fetch per request",
    "100"
  )
  .option(
    "--base-url <url>",
    "Polymarket API base URL",
    "https://gamma-api.polymarket.com"
  )
  .option("--verbose", "Enable verbose logging", false)
  .option("--force", "Truncate existing tags table before saving", false)
  .action(async (options) => {
    await saveTagsCommand({
      dbPath: options.dbPath,
      batchSize: parseInt(options.batchSize, 10),
      baseUrl: options.baseUrl,
      verbose: options.verbose,
      force: options.force,
    });
  });

tagsCommand
  .command("list")
  .description("List tags from local database with pagination support")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option(
    "--filter <text>",
    "Filter tags by label or slug (case-insensitive substring match)"
  )
  .option("--format <format>", "Output format: table, json", "table")
  .option(
    "--limit <number>",
    "Maximum number of results to display (default: 100, max: 10000)"
  )
  .option("--offset <number>", "Number of records to skip", "0")
  .option(
    "--sort-by <field>",
    "Sort field: label, id, slug, publishedAt, createdAt, updatedAt",
    "label"
  )
  .option("--order <direction>", "Sort order: asc, desc", "asc")
  .option("--all", "Fetch all results without limit", false)
  .option("--show-count", "Display total count of matching records", false)
  .action(async (options) => {
    await listTagsCommand({
      dbPath: options.dbPath,
      filter: options.filter,
      format: options.format,
      limit: options.limit ? parseInt(options.limit, 10) : undefined,
      offset: parseInt(options.offset, 10),
      sortBy: options.sortBy,
      order: options.order,
      all: options.all,
      showCount: options.showCount,
    });
  });

tagsCommand
  .command("get")
  .description("Get a specific tag by ID or slug")
  .argument("<id-or-slug>", "Tag ID (numeric) or slug (string)")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option("--format <format>", "Output format: json, table", "json")
  .action(async (idOrSlug, options) => {
    await getTagCommand(idOrSlug, {
      dbPath: options.dbPath,
      format: options.format,
    });
  });

// Markets command group
const marketsCommand = program
  .command("markets")
  .description("Manage Polymarket markets and events");

marketsCommand
  .command("save")
  .description("Fetch markets for a specific tag and save to local database")
  .requiredOption("--tag <id-or-slug>", "Tag ID (numeric) or slug (string)")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option(
    "--batch-size <number>",
    "Number of records to fetch per request",
    "100"
  )
  .option(
    "--base-url <url>",
    "Polymarket API base URL",
    "https://gamma-api.polymarket.com"
  )
  .option("--verbose", "Enable verbose logging", false)
  .option("--force", "Delete existing markets for this tag before saving", false)
  .option("--include-closed", "Include closed markets in sync", false)
  .action(async (options) => {
    await saveMarketsCommand({
      tag: options.tag,
      dbPath: options.dbPath,
      batchSize: parseInt(options.batchSize, 10),
      baseUrl: options.baseUrl,
      verbose: options.verbose,
      force: options.force,
      includeClosed: options.includeClosed,
    });
  });

marketsCommand
  .command("list")
  .description("List markets from local database with filtering and pagination")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option("--tag <id-or-slug>", "Filter by tag ID (numeric) or slug (string)")
  .option(
    "--filter <text>",
    "Search in title, ticker, or description"
  )
  .option(
    "--closed <boolean>",
    "Filter by closed status (true/false)",
    (value) => {
      if (value === "true") return true;
      if (value === "false") return false;
      throw new Error("--closed must be 'true' or 'false'");
    }
  )
  .option(
    "--active <boolean>",
    "Filter by active status (true/false)",
    (value) => {
      if (value === "true") return true;
      if (value === "false") return false;
      throw new Error("--active must be 'true' or 'false'");
    }
  )
  .option("--format <format>", "Output format: table, json", "table")
  .option("--limit <number>", "Maximum results (default: 100)")
  .option("--offset <number>", "Pagination offset", "0")
  .option(
    "--sort-by <field>",
    "Sort field: id, ticker, title, end_date, volume, liquidity, created_at, updated_at",
    "end_date"
  )
  .option("--order <direction>", "Sort order: asc, desc", "desc")
  .option("--all", "Fetch all results without limit", false)
  .option("--show-count", "Display total count", false)
  .action(async (options) => {
    await listMarketsCommand({
      dbPath: options.dbPath,
      tag: options.tag,
      filter: options.filter,
      closed: options.closed,
      active: options.active,
      format: options.format,
      limit: options.limit ? parseInt(options.limit, 10) : undefined,
      offset: parseInt(options.offset, 10),
      sortBy: options.sortBy,
      order: options.order,
      all: options.all,
      showCount: options.showCount,
    });
  });

marketsCommand
  .command("get")
  .description("Get a specific market by event ID or slug")
  .argument("<id-or-slug>", "Event ID (numeric) or slug (string)")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option("--format <format>", "Output format: json, table", "json")
  .action(async (idOrSlug, options) => {
    await getMarketCommand(idOrSlug, {
      dbPath: options.dbPath,
      format: options.format,
    });
  });

marketsCommand
  .command("stream")
  .description("Stream real-time data for a market by event ID or slug")
  .argument("<id-or-slug>", "Event ID (numeric) or slug (string)")
  .option("--db-path <path>", "Path to SQLite database file", "./polymarket.db")
  .option(
    "--event-types <types>",
    "Comma-separated event types to filter (book, price_change, tick_size_change, last_trade_price)"
  )
  .option("--log-file <path>", "Write stream output to file")
  .option("--verbose", "Enable verbose logging", false)
  .option("--no-ping", "Disable automatic ping messages")
  .action(async (idOrSlug, options) => {
    await streamMarketCommand(idOrSlug, {
      dbPath: options.dbPath,
      eventTypes: options.eventTypes,
      logFile: options.logFile,
      verbose: options.verbose,
      noPing: !options.ping,
    });
  });

// Stream command
program
  .command("stream")
  .description("Stream real-time market data via WebSocket")
  .argument("<asset-ids>", "Comma-separated asset IDs to subscribe to")
  .option(
    "--event-types <types>",
    "Comma-separated event types to filter (book, price_change, tick_size_change, last_trade_price)"
  )
  .option("--log-file <path>", "Write stream output to file")
  .option("--verbose", "Enable verbose logging", false)
  .option("--no-ping", "Disable automatic ping messages")
  .action(async (assetIds, options) => {
    await streamCommand(assetIds, options);
  });

// Parse command line arguments
program.parse();
