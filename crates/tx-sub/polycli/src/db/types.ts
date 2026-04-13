export interface Tag {
  id: string;
  label: string;
  slug: string;
  publishedAt: string | null;
  createdAt: string | null;
  updatedAt: string | null;
}

export interface QueryOptions {
  filter?: string;
  limit?: number | null;
  offset?: number;
  sortBy?: TagSortField;
  order?: "asc" | "desc";
}

export type TagSortField =
  | "id"
  | "label"
  | "slug"
  | "publishedAt"
  | "createdAt"
  | "updatedAt";

export interface Event {
  id: string;
  ticker: string;
  slug: string;
  title: string;
  description: string | null;
  resolution_source: string | null;
  start_date: string | null;
  creation_date: string | null;
  end_date: string | null;
  image: string | null;
  icon: string | null;
  active: boolean;
  closed: boolean;
  archived: boolean;
  new: boolean;
  featured: boolean;
  restricted: boolean;
  liquidity: number | null;
  volume: number | null;
  open_interest: number | null;
  competitive: number | null;
  volume_1mo: number | null;
  volume_1yr: number | null;
  comment_count: number | null;
  enable_order_book: boolean | null;
  cyom: boolean | null;
  show_all_outcomes: boolean | null;
  show_market_images: boolean | null;
  enable_neg_risk: boolean | null;
  automatically_active: boolean | null;
  neg_risk_augmented: boolean | null;
  pending_deployment: boolean | null;
  deploying: boolean | null;
  created_at: string;
  updated_at: string;
}

export interface Market {
  id: string;
  event_id: string;
  question: string;
  condition_id: string | null;
  slug: string;
  start_date: string | null;
  end_date: string | null;
  liquidity: number | null;
  volume: number | null;
  outcomes: string;
  outcome_prices: string;
  clob_token_ids: string | null;
  active: boolean;
  closed: boolean;
  created_at: string;
  updated_at: string;
}

export interface EventWithMarkets extends Event {
  markets: Market[];
}

export interface EventTag {
  event_id: string;
  tag_id: string;
  tag_label: string | null;
}

export type EventSortField =
  | "id"
  | "ticker"
  | "title"
  | "end_date"
  | "volume"
  | "liquidity"
  | "created_at"
  | "updated_at";

export interface EventQueryOptions {
  filter?: string;
  tagId?: string;
  closed?: boolean;
  active?: boolean;
  limit?: number | null;
  offset?: number;
  sortBy?: EventSortField;
  order?: "asc" | "desc";
}
