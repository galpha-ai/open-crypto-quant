import { createDatabase } from "../../db/connection";
import { TagsRepository } from "../../db/tags";
import { ApiClient } from "../../api/client";
import { TagsApi } from "../../api/tags";
import { logger } from "../../utils/logger";
import { formatProgress, formatSuccess, formatError } from "../../utils/format";
import type { Tag } from "../../db/types";
import type { ApiTag } from "../../api/types";

export interface SaveOptions {
  dbPath: string;
  batchSize: number;
  baseUrl: string;
  verbose: boolean;
  force: boolean;
}

export async function saveTagsCommand(options: SaveOptions): Promise<void> {
  const startTime = Date.now();

  // Set verbose mode
  logger.setVerbose(options.verbose);

  logger.info("Starting tags save command", {
    batchSize: options.batchSize,
    dbPath: options.dbPath,
    baseUrl: options.baseUrl,
    force: options.force,
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

  // Truncate if force flag is set
  if (options.force) {
    logger.info("Truncating existing tags table");
    tagsRepo.truncate();
  }

  // Initialize API client
  const apiClient = new ApiClient({
    baseUrl: options.baseUrl,
    timeout: 30000,
    retryAttempts: 3,
    retryBackoffMs: 1000,
  });

  const tagsApi = new TagsApi(apiClient);

  let offset = 0;
  let totalFetched = 0;

  try {
    console.log(formatProgress(totalFetched, offset, 0));

    while (true) {
      // Fetch batch from API
      logger.debug("Fetching batch", { offset, limit: options.batchSize });

      const apiTags = await tagsApi.fetchTags(options.batchSize, offset);

      if (apiTags.length === 0) {
        logger.info("No more tags to fetch, pagination exhausted");
        break;
      }

      // Filter out incomplete tags (missing label or slug) and convert API tags to database tags
      const tags: Tag[] = apiTags
        .filter((apiTag: ApiTag) => apiTag.label && apiTag.slug)
        .map((apiTag: ApiTag) => ({
          id: apiTag.id,
          label: apiTag.label!,
          slug: apiTag.slug!,
          publishedAt: apiTag.publishedAt || null,
          createdAt: apiTag.createdAt || null,
          updatedAt: apiTag.updatedAt || null,
        }));

      // Log warning if any tags were skipped
      const skippedCount = apiTags.length - tags.length;
      if (skippedCount > 0) {
        logger.warn("Skipped incomplete tags", {
          count: skippedCount,
          offset,
        });
      }

      // Store batch in database
      logger.debug("Storing batch", { count: tags.length });
      tagsRepo.upsertBatch(tags);

      totalFetched += tags.length;

      logger.info("Fetched batch", {
        count: tags.length,
        offset,
        totalFetched,
      });

      // Update progress on stdout
      console.log(formatProgress(totalFetched, offset, tags.length));

      // Increment offset by the batch size to fetch next page
      offset += options.batchSize;
    }

    const duration = Date.now() - startTime;
    logger.info("Tags save completed", { totalFetched, duration });

    console.log(formatSuccess(totalFetched, options.dbPath));
  } catch (error) {
    console.error(formatError("Error during tags save", error));

    logger.error("Tags save failed", {
      error: error instanceof Error ? error.message : String(error),
      totalFetched,
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
