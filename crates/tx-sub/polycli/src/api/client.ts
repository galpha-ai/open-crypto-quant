import { retryWithBackoff, sleep } from "../utils/retry";
import { logger } from "../utils/logger";
import {
  ApiError,
  NetworkError,
  ParseError,
  type ApiClientConfig,
} from "./types";

export class ApiClient {
  private config: ApiClientConfig;

  constructor(config: ApiClientConfig) {
    this.config = config;
  }

  async get<T>(
    path: string,
    params?: Record<string, string | number>
  ): Promise<T> {
    // Build URL with query parameters
    const url = new URL(path, this.config.baseUrl);
    if (params) {
      for (const [key, value] of Object.entries(params)) {
        url.searchParams.append(key, String(value));
      }
    }

    logger.debug("API request", {
      method: "GET",
      url: url.toString(),
    });

    // Retry logic with exponential backoff
    return retryWithBackoff(
      () => this.executeRequest<T>(url),
      {
        maxAttempts: this.config.retryAttempts,
        initialDelayMs: this.config.retryBackoffMs,
        shouldRetry: (error) => {
          // Retry on network errors and 5xx server errors
          if (error instanceof NetworkError) {
            return true;
          }
          if (
            error instanceof ApiError &&
            error.statusCode &&
            error.statusCode >= 500
          ) {
            return true;
          }
          return false;
        },
        onRetry: (attempt, error) => {
          logger.warn("Retry attempt", {
            attempt,
            error:
              error instanceof Error ? error.message : String(error),
          });
        },
      }
    );
  }

  private async executeRequest<T>(url: URL): Promise<T> {
    const controller = new AbortController();
    const timeoutId = setTimeout(
      () => controller.abort(),
      this.config.timeout
    );

    try {
      const response = await fetch(url.toString(), {
        signal: controller.signal,
      });

      // Handle rate limiting (429)
      if (response.status === 429) {
        const retryAfter = response.headers.get("Retry-After");
        const waitMs = retryAfter ? parseInt(retryAfter) * 1000 : 60000;

        logger.warn("Rate limit exceeded", {
          retryAfterMs: waitMs,
        });

        await sleep(waitMs);

        // Retry the request after waiting
        return this.executeRequest<T>(url);
      }

      // Handle HTTP errors
      if (!response.ok) {
        const errorBody = await response.text().catch(() => "");
        throw new ApiError(
          `HTTP ${response.status}: ${response.statusText}`,
          response.status,
          errorBody
        );
      }

      // Parse JSON response
      try {
        const data = await response.json();
        return data as T;
      } catch (error) {
        throw new ParseError(
          `Failed to parse JSON response: ${error instanceof Error ? error.message : String(error)}`
        );
      }
    } catch (error) {
      // Handle abort/timeout
      if (error instanceof Error && error.name === "AbortError") {
        throw new NetworkError("Request timeout");
      }

      // Handle network errors
      if (error instanceof TypeError) {
        throw new NetworkError(
          `Network request failed: ${error.message}`
        );
      }

      // Re-throw other errors
      throw error;
    } finally {
      clearTimeout(timeoutId);
    }
  }
}
