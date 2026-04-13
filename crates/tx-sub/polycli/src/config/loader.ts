import { parse } from "yaml";
import { DEFAULT_CONFIG, type PolymarketConfig } from "./defaults";
import { existsSync } from "fs";
import { homedir } from "os";
import { join } from "path";

/**
 * Load configuration from file and merge with defaults
 * Priority (highest to lowest):
 * 1. CLI flags (handled by commander)
 * 2. Environment variables
 * 3. Configuration file
 * 4. Built-in defaults
 */
export async function loadConfig(): Promise<PolymarketConfig> {
  const config = { ...DEFAULT_CONFIG };

  // Try to load from config file
  const configFile = getConfigFilePath();
  if (configFile && existsSync(configFile)) {
    try {
      const fileContent = Bun.file(configFile);
      const text = await fileContent.text();
      const parsed = parse(text) as Partial<PolymarketConfig>;

      // Deep merge configuration
      if (parsed.database) {
        config.database = { ...config.database, ...parsed.database };
      }
      if (parsed.api) {
        config.api = { ...config.api, ...parsed.api };
      }
      if (parsed.pagination) {
        config.pagination = { ...config.pagination, ...parsed.pagination };
      }
      if (parsed.output) {
        config.output = { ...config.output, ...parsed.output };
      }
    } catch (error) {
      // Silently ignore config file errors and use defaults
      // This is acceptable as config file is optional
    }
  }

  // Override with environment variables
  if (process.env.POLYMARKET_DB_PATH) {
    config.database.path = process.env.POLYMARKET_DB_PATH;
  }
  if (process.env.POLYMARKET_API_BASE_URL) {
    config.api.baseUrl = process.env.POLYMARKET_API_BASE_URL;
  }
  if (process.env.POLYMARKET_LOG_LEVEL) {
    // Log level handling would go here if we implement logger levels
  }

  return config;
}

function getConfigFilePath(): string | null {
  // Check XDG_CONFIG_HOME first
  const xdgConfigHome = process.env.XDG_CONFIG_HOME;
  if (xdgConfigHome) {
    return join(xdgConfigHome, "polymarket-cli", "config.yaml");
  }

  // Fall back to ~/.polymarket-cli/config.yaml
  const home = homedir();
  return join(home, ".polymarket-cli", "config.yaml");
}

/**
 * Expand tilde in path to home directory
 */
export function expandPath(path: string): string {
  if (path.startsWith("~/")) {
    return join(homedir(), path.slice(2));
  }
  return path;
}
