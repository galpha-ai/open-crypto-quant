export interface PolymarketConfig {
  database: {
    path: string;
  };
  api: {
    baseUrl: string;
    timeoutMs: number;
    retryAttempts: number;
    retryBackoffMs: number;
  };
  pagination: {
    defaultBatchSize: number;
    maxBatchSize: number;
  };
  output: {
    defaultFormat: "table" | "json";
    tableMaxColumnWidth: number;
  };
}

export const DEFAULT_CONFIG: PolymarketConfig = {
  database: {
    path: "./polymarket.db",
  },
  api: {
    baseUrl: "https://gamma-api.polymarket.com",
    timeoutMs: 30000,
    retryAttempts: 3,
    retryBackoffMs: 1000,
  },
  pagination: {
    defaultBatchSize: 100,
    maxBatchSize: 500,
  },
  output: {
    defaultFormat: "table",
    tableMaxColumnWidth: 50,
  },
};
