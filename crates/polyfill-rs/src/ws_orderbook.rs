//! WebSocket OrderBook utilities for Polymarket
//!
//! Shared functions for subscribing to WebSocket and maintaining local orderbooks.

use crate::book::OrderBook;
use crate::types::{OrderDelta, Side};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Replace top-of-book using best bid/ask prices from a price_change message
/// We only need the top levels for the strategies that consume this, so we
/// rebuild the book with tiny placeholder sizes to avoid stale/crossed books.
fn apply_best_levels(
    book: &mut OrderBook,
    seq: &mut u64,
    asset_id: &str,
    best_bid: Option<Decimal>,
    best_ask: Option<Decimal>,
) {
    // Reset the book to avoid mixing trade-side deltas (which do not represent book changes)
    book.clear();

    if let Some(price) = best_bid {
        *seq += 1;
        let _ = book.apply_delta(OrderDelta {
            token_id: asset_id.to_string(),
            timestamp: chrono::Utc::now(),
            side: Side::BUY,
            price,
            size: Decimal::ONE, // size is unknown in price_change, a small placeholder is fine
            sequence: *seq,
        });
    }

    if let Some(price) = best_ask {
        *seq += 1;
        let _ = book.apply_delta(OrderDelta {
            token_id: asset_id.to_string(),
            timestamp: chrono::Utc::now(),
            side: Side::SELL,
            price,
            size: Decimal::ONE,
            sequence: *seq,
        });
    }
}

/// Apply orderbook snapshot to book
/// IMPORTANT: Clears existing data first to avoid stale levels
pub fn apply_snapshot(
    book: &mut OrderBook,
    seq: &mut u64,
    asset_id: &str,
    asks: Option<&Vec<serde_json::Value>>,
    bids: Option<&Vec<serde_json::Value>>,
) {
    // Clear old data before applying snapshot to prevent stale price levels
    book.clear();

    if let Some(asks) = asks {
        for ask in asks {
            if let (Some(p), Some(s)) = (
                ask.get("price").and_then(|v| v.as_str()),
                ask.get("size").and_then(|v| v.as_str()),
            ) {
                if let (Ok(price), Ok(size)) = (Decimal::from_str(p), Decimal::from_str(s)) {
                    *seq += 1;
                    let _ = book.apply_delta(OrderDelta {
                        token_id: asset_id.to_string(),
                        timestamp: chrono::Utc::now(),
                        side: Side::SELL,
                        price,
                        size,
                        sequence: *seq,
                    });
                }
            }
        }
    }

    if let Some(bids) = bids {
        for bid in bids {
            if let (Some(p), Some(s)) = (
                bid.get("price").and_then(|v| v.as_str()),
                bid.get("size").and_then(|v| v.as_str()),
            ) {
                if let (Ok(price), Ok(size)) = (Decimal::from_str(p), Decimal::from_str(s)) {
                    *seq += 1;
                    let _ = book.apply_delta(OrderDelta {
                        token_id: asset_id.to_string(),
                        timestamp: chrono::Utc::now(),
                        side: Side::BUY,
                        price,
                        size,
                        sequence: *seq,
                    });
                }
            }
        }
    }
}

/// Apply price change to book
pub fn apply_price_change(
    book: &mut OrderBook,
    seq: &mut u64,
    asset_id: &str,
    side_str: &str,
    price: Decimal,
    size: Decimal,
) {
    let side = if side_str.eq_ignore_ascii_case("SELL") || side_str.eq_ignore_ascii_case("ASK") {
        Side::SELL
    } else {
        Side::BUY
    };
    *seq += 1;
    let _ = book.apply_delta(OrderDelta {
        token_id: asset_id.to_string(),
        timestamp: chrono::Utc::now(),
        side,
        price,
        size,
        sequence: *seq,
    });
}

/// Process a WebSocket message and update orderbooks
/// Returns true if the message was processed
pub fn process_ws_message(
    json: &serde_json::Value,
    up_token: &str,
    down_token: &str,
    up_book: &mut OrderBook,
    down_book: &mut OrderBook,
    up_seq: &mut u64,
    down_seq: &mut u64,
) -> bool {
    // Handle snapshot (array)
    if let Some(arr) = json.as_array() {
        for item in arr {
            let asset_id = item.get("asset_id").and_then(|v| v.as_str()).unwrap_or("");
            let asks = item.get("asks").and_then(|v| v.as_array());
            let bids = item.get("bids").and_then(|v| v.as_array());

            if asset_id == up_token {
                apply_snapshot(up_book, up_seq, asset_id, asks, bids);
            } else if asset_id == down_token {
                apply_snapshot(down_book, down_seq, asset_id, asks, bids);
            }
        }
        return true;
    }

    // Handle price_changes
    if let Some(changes) = json.get("price_changes").and_then(|v| v.as_array()) {
        for change in changes {
            let asset_id = change.get("asset_id").and_then(|v| v.as_str()).unwrap_or("");
            let side_str = change.get("side").and_then(|v| v.as_str()).unwrap_or("");
            let price_str = change.get("price").and_then(|v| v.as_str()).unwrap_or("0");
            let size_str = change.get("size").and_then(|v| v.as_str()).unwrap_or("0");
            let best_bid = change
                .get("best_bid")
                .and_then(|v| v.as_str())
                .and_then(|s| Decimal::from_str(s).ok());
            let best_ask = change
                .get("best_ask")
                .and_then(|v| v.as_str())
                .and_then(|s| Decimal::from_str(s).ok());

            // price_change side represents the aggressor on the last trade, not a book delta.
            // Use best_bid/best_ask as authoritative to keep the book consistent.
            if asset_id == up_token {
                let applied_best = if best_bid.is_some() || best_ask.is_some() {
                    apply_best_levels(up_book, up_seq, asset_id, best_bid, best_ask);
                    true
                } else {
                    false
                };

                // Fallback to legacy delta handling only if we could not apply best prices
                if !applied_best {
                    if let (Ok(price), Ok(size)) =
                        (Decimal::from_str(price_str), Decimal::from_str(size_str))
                    {
                        apply_price_change(up_book, up_seq, asset_id, side_str, price, size);
                    }
                }
            } else if asset_id == down_token {
                let applied_best = if best_bid.is_some() || best_ask.is_some() {
                    apply_best_levels(down_book, down_seq, asset_id, best_bid, best_ask);
                    true
                } else {
                    false
                };

                if !applied_best {
                    if let (Ok(price), Ok(size)) =
                        (Decimal::from_str(price_str), Decimal::from_str(size_str))
                    {
                        apply_price_change(down_book, down_seq, asset_id, side_str, price, size);
                    }
                }
            }
        }
        return true;
    }

    false
}

/// Market info structure for binary markets
#[derive(Debug, Clone)]
pub struct BinaryMarketInfo {
    pub title: String,
    pub condition_id: String,
    pub neg_risk: bool,
    pub up_token: String,
    pub down_token: String,
}

/// Fetch market info from Gamma API by slug
pub async fn fetch_market_info(slug: &str) -> Result<BinaryMarketInfo, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://gamma-api.polymarket.com/events?slug={}", slug);
    let resp: serde_json::Value = reqwest::get(&url).await?.json().await?;

    let event = resp.get(0).ok_or("Market not found")?;
    let market = event.get("markets").and_then(|m| m.get(0)).ok_or("No markets")?;

    let title = event.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let condition_id = market.get("conditionId").and_then(|c| c.as_str()).unwrap_or("").to_string();
    let neg_risk = market.get("negRisk").and_then(|n| n.as_bool()).unwrap_or(false);

    let token_ids: Vec<String> = serde_json::from_str(
        market.get("clobTokenIds").and_then(|t| t.as_str()).unwrap_or("[]"),
    )?;

    if token_ids.len() < 2 {
        return Err("Not enough tokens".into());
    }

    Ok(BinaryMarketInfo {
        title,
        condition_id,
        neg_risk,
        up_token: token_ids[0].clone(),
        down_token: token_ids[1].clone(),
    })
}

/// Get current 15-minute interval timestamp
pub fn get_current_interval() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    (now / 900) * 900
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_interval() {
        let interval = get_current_interval();
        assert_eq!(interval % 900, 0);
    }
}
