import { createDatabase } from "../../db/connection";
import { TagsRepository } from "../../db/tags";
import { EventsRepository } from "../../db/events";
import { MarketsRepository } from "../../db/markets";
import { EventTagsRepository } from "../../db/event-tags";
import { ApiClient } from "../../api/client";
import { MarketsApi } from "../../api/markets";
import { TagResolver } from "../../utils/tag-resolver";
import { logger } from "../../utils/logger";
import { formatProgress, formatSuccess, formatError } from "../../utils/format";
import type { Event, Market, EventTag } from "../../db/types";
import type { ApiEvent, ApiMarket } from "../../api/types";

export interface SaveOptions {
  tag: string;
  dbPath: string;
  batchSize: number;
  baseUrl: string;
  verbose: boolean;
  force: boolean;
  includeClosed: boolean;
}

export async function saveMarketsCommand(options: SaveOptions): Promise<void> {
  const startTime = Date.now();

  // Set verbose mode
  logger.setVerbose(options.verbose);

  logger.info("Starting markets save command", {
    tag: options.tag,
    batchSize: options.batchSize,
    dbPath: options.dbPath,
    baseUrl: options.baseUrl,
    force: options.force,
    includeClosed: options.includeClosed,
  });

  // Initialize database
  let db;
  try {
    db = createDatabase(options.dbPath);
  } catch (error) {
    console.error(formatError("Database error", error));
    logger.error("Failed to create database", {
      error: error instanceof Error ? error.message : String(error),
    });
    process.exit(2);
  }

  const tagsRepo = new TagsRepository(db);
  const eventsRepo = new EventsRepository(db);
  const marketsRepo = new MarketsRepository(db);
  const eventTagsRepo = new EventTagsRepository(db);

  // Resolve tag slug to ID
  const tagResolver = new TagResolver(tagsRepo);
  let tag;
  try {
    tag = tagResolver.resolveWithMetadata(options.tag);
  } catch (error) {
    console.error(formatError("Tag resolution error", error));
    logger.error("Failed to resolve tag", {
      tag: options.tag,
      error: error instanceof Error ? error.message : String(error),
    });
    process.exit(1);
  }

  console.log(`Fetching markets for tag '${tag.label}' (ID: ${tag.id})`);

  // Truncate if force flag is set
  if (options.force) {
    logger.info("Deleting existing events and markets for tag", { tagId: tag.id });
    eventsRepo.deleteByTag(tag.id);
    eventTagsRepo.deleteByTag(tag.id);
  }

  // Initialize API client
  const apiClient = new ApiClient({
    baseUrl: options.baseUrl,
    timeout: 30000,
    retryAttempts: 3,
    retryBackoffMs: 1000,
  });

  const marketsApi = new MarketsApi(apiClient);

  let offset = 0;
  let totalEventsFetched = 0;
  let totalMarketsFetched = 0;

  try {
    console.log(formatProgress(totalEventsFetched, offset, 0));

    while (true) {
      // Fetch batch from API
      logger.debug("Fetching batch", { offset, limit: options.batchSize });

      const apiEvents = await marketsApi.fetchEvents(
        tag.id,
        options.batchSize,
        offset,
        options.includeClosed
      );

      if (apiEvents.length === 0) {
        logger.info("No more events to fetch, pagination exhausted");
        break;
      }

      // Transform API events to database events
      const events: Event[] = [];
      const markets: Market[] = [];
      const eventTags: EventTag[] = [];

      for (const apiEvent of apiEvents) {
        // Create event record
        const event: Event = {
          id: apiEvent.id,
          ticker: apiEvent.ticker,
          slug: apiEvent.slug,
          title: apiEvent.title,
          description: apiEvent.description || null,
          resolution_source: apiEvent.resolutionSource || null,
          start_date: apiEvent.startDate || null,
          creation_date: apiEvent.creationDate || null,
          end_date: apiEvent.endDate || null,
          image: apiEvent.image || null,
          icon: apiEvent.icon || null,
          active: apiEvent.active ?? true,
          closed: apiEvent.closed ?? false,
          archived: apiEvent.archived ?? false,
          new: apiEvent.new ?? false,
          featured: apiEvent.featured ?? false,
          restricted: apiEvent.restricted ?? false,
          liquidity: apiEvent.liquidity ?? null,
          volume: apiEvent.volume ?? null,
          open_interest: apiEvent.openInterest ?? null,
          competitive: apiEvent.competitive ?? null,
          volume_1mo: apiEvent.volume1mo ?? null,
          volume_1yr: apiEvent.volume1yr ?? null,
          comment_count: apiEvent.commentCount ?? null,
          enable_order_book: apiEvent.enableOrderBook ?? null,
          cyom: apiEvent.cyom ?? null,
          show_all_outcomes: apiEvent.showAllOutcomes ?? null,
          show_market_images: apiEvent.showMarketImages ?? null,
          enable_neg_risk: apiEvent.enableNegRisk ?? null,
          automatically_active: apiEvent.automaticallyActive ?? null,
          neg_risk_augmented: apiEvent.negRiskAugmented ?? null,
          pending_deployment: apiEvent.pendingDeployment ?? null,
          deploying: apiEvent.deploying ?? null,
          created_at: apiEvent.createdAt || new Date().toISOString(),
          updated_at: apiEvent.updatedAt || new Date().toISOString(),
        };

        events.push(event);

        // Create event-tag association
        eventTags.push({
          event_id: apiEvent.id,
          tag_id: tag.id,
          tag_label: tag.label,
        });

        // Create market records
        if (apiEvent.markets && apiEvent.markets.length > 0) {
          for (const apiMarket of apiEvent.markets) {
            const market: Market = {
              id: apiMarket.id,
              event_id: apiEvent.id,
              question: apiMarket.question,
              condition_id: apiMarket.conditionId || null,
              slug: apiMarket.slug,
              start_date: apiMarket.startDate || null,
              end_date: apiMarket.endDate || null,
              liquidity: apiMarket.liquidityNum ?? (apiMarket.liquidity ? parseFloat(apiMarket.liquidity) : null),
              volume: apiMarket.volumeNum ?? (apiMarket.volume ? parseFloat(apiMarket.volume) : null),
              outcomes: apiMarket.outcomes || "[]",
              outcome_prices: apiMarket.outcomePrices || "[]",
              clob_token_ids: apiMarket.clobTokenIds || null,
              active: apiMarket.active ?? true,
              closed: apiMarket.closed ?? false,
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
            };

            markets.push(market);
          }
        }
      }

      // Store batch in database
      logger.debug("Storing batch", {
        events: events.length,
        markets: markets.length,
        eventTags: eventTags.length,
      });

      eventsRepo.upsertBatch(events);
      marketsRepo.upsertBatch(markets);
      eventTagsRepo.upsertBatch(eventTags);

      totalEventsFetched += events.length;
      totalMarketsFetched += markets.length;

      logger.info("Fetched batch", {
        events: events.length,
        markets: markets.length,
        offset,
        totalEventsFetched,
        totalMarketsFetched,
      });

      // Update progress on stdout
      console.log(
        `Fetched ${totalEventsFetched} events and ${totalMarketsFetched} markets (offset: ${offset})`
      );

      // Increment offset by the batch size to fetch next page
      offset += options.batchSize;
    }

    const duration = Date.now() - startTime;
    logger.info("Markets save completed", {
      totalEventsFetched,
      totalMarketsFetched,
      duration,
    });

    console.log(
      `\nSuccessfully saved ${totalEventsFetched} events and ${totalMarketsFetched} markets for tag '${tag.label}' (ID: ${tag.id}) to ${options.dbPath}`
    );
  } catch (error) {
    console.error(formatError("Error during markets save", error));

    logger.error("Markets save failed", {
      error: error instanceof Error ? error.message : String(error),
      totalEventsFetched,
      totalMarketsFetched,
      offset,
    });

    // Exit with appropriate code
    if (error instanceof Error) {
      if (error.name === "ApiError" || error.name === "NetworkError") {
        process.exit(1);
      }
    }
    process.exit(2);
  } finally {
    // Close database connection
    if (db) {
      db.close();
    }
  }
}
