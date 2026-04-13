// API response types matching Polymarket API format
export interface ApiTag {
  id: string;
  label?: string;
  slug?: string;
  publishedAt?: string;
  createdAt?: string;
  updatedAt?: string;
}

export interface ApiMarket {
  id: string;
  question: string;
  conditionId?: string;
  slug: string;
  endDate?: string;
  liquidity?: string;
  startDate?: string;
  outcomes?: string;
  outcomePrices?: string;
  volume?: string;
  active?: boolean;
  closed?: boolean;
  volumeNum?: number;
  liquidityNum?: number;
  volume1mo?: number;
  volume1yr?: number;
  clobTokenIds?: string;
}

export interface ApiEvent {
  id: string;
  ticker: string;
  slug: string;
  title: string;
  description?: string;
  resolutionSource?: string;
  startDate?: string;
  creationDate?: string;
  endDate?: string;
  image?: string;
  icon?: string;
  active?: boolean;
  closed?: boolean;
  archived?: boolean;
  new?: boolean;
  featured?: boolean;
  restricted?: boolean;
  liquidity?: number;
  volume?: number;
  openInterest?: number;
  competitive?: number;
  volume1mo?: number;
  volume1yr?: number;
  commentCount?: number;
  enableOrderBook?: boolean;
  cyom?: boolean;
  showAllOutcomes?: boolean;
  showMarketImages?: boolean;
  enableNegRisk?: boolean;
  automaticallyActive?: boolean;
  negRiskAugmented?: boolean;
  pendingDeployment?: boolean;
  deploying?: boolean;
  createdAt?: string;
  updatedAt?: string;
  markets?: ApiMarket[];
  tags?: ApiTag[];
}

export interface ApiClientConfig {
  baseUrl: string;
  timeout: number;
  retryAttempts: number;
  retryBackoffMs: number;
}

export class ApiError extends Error {
  constructor(
    message: string,
    public statusCode?: number,
    public response?: unknown
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export class NetworkError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NetworkError";
  }
}

export class ParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ParseError";
  }
}
