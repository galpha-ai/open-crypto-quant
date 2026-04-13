/**
 * WebSocket message types for Polymarket market channel
 * Based on: https://docs.polymarket.com/developers/CLOB/websocket/market-channel
 */

export type MarketEventType =
  | "book"
  | "price_change"
  | "tick_size_change"
  | "last_trade_price";

export interface OrderSummary {
  price: string;
  size: string;
}

export interface BookMessage {
  event_type: "book";
  asset_id: string;
  market: string;
  timestamp: string;
  hash: string;
  bids: OrderSummary[];
  asks: OrderSummary[];
}

export interface PriceChange {
  asset_id: string;
  price: string;
  size: string;
  side: "BUY" | "SELL";
  hash: string;
  best_bid: string;
  best_ask: string;
}

export interface PriceChangeMessage {
  event_type: "price_change";
  market: string;
  price_changes: PriceChange[];
  timestamp: string;
}

export interface TickSizeChangeMessage {
  event_type: "tick_size_change";
  asset_id: string;
  market: string;
  old_tick_size: string;
  new_tick_size: string;
  timestamp: string;
}

export interface LastTradePriceMessage {
  event_type: "last_trade_price";
  asset_id: string;
  market: string;
  price: string;
  side: "BUY" | "SELL";
  size: string;
  fee_rate_bps: string;
  timestamp: string;
}

export type MarketMessage =
  | BookMessage
  | PriceChangeMessage
  | TickSizeChangeMessage
  | LastTradePriceMessage;

/**
 * Subscription message sent to WebSocket on connection
 */
export interface MarketSubscriptionMessage {
  assets_ids: string[];
  type: "market";
}
