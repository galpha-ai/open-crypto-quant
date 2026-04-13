//! Book (orderbook snapshot) event parsing
//!
//! This module handles parsing of Polymarket book WebSocket events.
//! Book messages contain full orderbook snapshots with all bid and ask levels.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};

use crate::types::{ParsedBookEvent, ParsedOrderSummary};

/// Parse a book event from JSON
///
/// Book message structure from Polymarket:
/// ```json
/// {
///   "event_type": "book",
///   "asset_id": "65818619657568813474341868652308942079804919287380422192892211131408793125422",
///   "market": "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af",
///   "bids": [{ "price": ".48", "size": "30" }, ...],
///   "asks": [{ "price": ".52", "size": "25" }, ...],
///   "timestamp": "123456789000",
///   "hash": "0x0...."
/// }
/// ```
///
/// # Arguments
/// * `data` - The JSON data from the WebSocket message
/// * `observed_at` - When the message was received by the subscriber service
pub fn parse_book(data: &serde_json::Value, observed_at: DateTime<Utc>) -> Result<ParsedBookEvent> {
    // Extract asset_id
    let asset_id = data
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid asset_id"))?
        .to_string();

    // Extract market
    let market = data
        .get("market")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid market"))?
        .to_string();

    // Parse timestamp (can be string or number)
    let timestamp = if let Some(ts_str) = data.get("timestamp").and_then(|v| v.as_str()) {
        ts_str.parse::<i64>().context("Failed to parse timestamp")?
    } else if let Some(ts_num) = data.get("timestamp").and_then(|v| v.as_i64()) {
        ts_num
    } else {
        return Err(anyhow!("Missing or invalid timestamp"));
    };

    // Extract hash
    let hash = data
        .get("hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid hash"))?
        .to_string();

    // Parse bids array
    let bids_array = data
        .get("bids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing or invalid bids array"))?;

    let mut bids = Vec::new();
    for bid in bids_array {
        bids.push(parse_order_summary(bid)?);
    }

    // Parse asks array
    let asks_array = data
        .get("asks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing or invalid asks array"))?;

    let mut asks = Vec::new();
    for ask in asks_array {
        asks.push(parse_order_summary(ask)?);
    }

    // Sort bids descending by price (best bid = highest price first)
    bids.sort_by(|a, b| {
        b.price
            .partial_cmp(&a.price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Sort asks ascending by price (best ask = lowest price first)
    asks.sort_by(|a, b| {
        a.price
            .partial_cmp(&b.price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(ParsedBookEvent {
        asset_id,
        market,
        bids,
        asks,
        hash,
        timestamp,
        observed_at,
    })
}

/// Parse a single order summary (price level) from the bids/asks array
fn parse_order_summary(data: &serde_json::Value) -> Result<ParsedOrderSummary> {
    // Parse price (0-1 range, can be in various string formats like ".48" or "0.48")
    let price_str = data
        .get("price")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing price in order summary"))?;

    let price = price_str
        .parse::<f64>()
        .context("Failed to parse price")?;

    if !(0.0..=1.0).contains(&price) {
        return Err(anyhow!("Price out of range (0-1): {}", price));
    }

    // Parse size
    let size_str = data
        .get("size")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing size in order summary"))?;

    let size = size_str
        .parse::<f64>()
        .context("Failed to parse size")?;

    if size < 0.0 {
        return Err(anyhow!("Size cannot be negative: {}", size));
    }

    Ok(ParsedOrderSummary { price, size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_observed_at() -> DateTime<Utc> {
        Utc::now()
    }

    fn valid_book_event() -> serde_json::Value {
        json!({
            "event_type": "book",
            "asset_id": "65818619657568813474341868652308942079804919287380422192892211131408793125422",
            "market": "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af",
            "bids": [
                { "price": ".48", "size": "30" },
                { "price": ".49", "size": "20" },
                { "price": ".50", "size": "15" }
            ],
            "asks": [
                { "price": ".52", "size": "25" },
                { "price": ".53", "size": "60" },
                { "price": ".54", "size": "10" }
            ],
            "timestamp": "123456789000",
            "hash": "0x1234abcd"
        })
    }

    #[test]
    fn test_parse_valid_book_event() {
        let data = valid_book_event();
        let observed_at = test_observed_at();

        let result = parse_book(&data, observed_at);
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(
            event.asset_id,
            "65818619657568813474341868652308942079804919287380422192892211131408793125422"
        );
        assert_eq!(
            event.market,
            "0xbd31dc8a20211944f6b70f31557f1001557b59905b7738480ca09bd4532f84af"
        );
        assert_eq!(event.timestamp, 123456789000);
        assert_eq!(event.observed_at, observed_at);
        assert_eq!(event.hash, "0x1234abcd");
        assert_eq!(event.bids.len(), 3);
        assert_eq!(event.asks.len(), 3);
    }

    #[test]
    fn test_parse_book_bids_sorted_descending() {
        let data = valid_book_event();
        let event = parse_book(&data, test_observed_at()).unwrap();

        // Bids should be sorted descending by price (best bid first)
        assert_eq!(event.bids[0].price, 0.50);
        assert_eq!(event.bids[0].size, 15.0);
        assert_eq!(event.bids[1].price, 0.49);
        assert_eq!(event.bids[1].size, 20.0);
        assert_eq!(event.bids[2].price, 0.48);
        assert_eq!(event.bids[2].size, 30.0);
    }

    #[test]
    fn test_parse_book_asks_sorted_ascending() {
        let data = valid_book_event();
        let event = parse_book(&data, test_observed_at()).unwrap();

        // Asks should be sorted ascending by price (best ask first)
        assert_eq!(event.asks[0].price, 0.52);
        assert_eq!(event.asks[0].size, 25.0);
        assert_eq!(event.asks[1].price, 0.53);
        assert_eq!(event.asks[1].size, 60.0);
        assert_eq!(event.asks[2].price, 0.54);
        assert_eq!(event.asks[2].size, 10.0);
    }

    #[test]
    fn test_parse_book_sorts_unsorted_input() {
        // Test that parsing correctly sorts regardless of input order
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [
                { "price": "0.30", "size": "10" },  // lowest
                { "price": "0.50", "size": "30" },  // highest
                { "price": "0.40", "size": "20" }   // middle
            ],
            "asks": [
                { "price": "0.70", "size": "10" },  // highest
                { "price": "0.55", "size": "30" },  // lowest
                { "price": "0.60", "size": "20" }   // middle
            ],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let event = parse_book(&data, test_observed_at()).unwrap();

        // Bids should be sorted descending (best bid = highest price first)
        assert_eq!(event.bids[0].price, 0.50);
        assert_eq!(event.bids[1].price, 0.40);
        assert_eq!(event.bids[2].price, 0.30);

        // Asks should be sorted ascending (best ask = lowest price first)
        assert_eq!(event.asks[0].price, 0.55);
        assert_eq!(event.asks[1].price, 0.60);
        assert_eq!(event.asks[2].price, 0.70);
    }

    #[test]
    fn test_parse_empty_bids_and_asks() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [],
            "asks": [],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_ok());

        let event = result.unwrap();
        assert!(event.bids.is_empty());
        assert!(event.asks.is_empty());
    }

    #[test]
    fn test_parse_missing_asset_id() {
        let mut data = valid_book_event();
        data.as_object_mut().unwrap().remove("asset_id");

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("asset_id"));
    }

    #[test]
    fn test_parse_missing_market() {
        let mut data = valid_book_event();
        data.as_object_mut().unwrap().remove("market");

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("market"));
    }

    #[test]
    fn test_parse_missing_timestamp() {
        let mut data = valid_book_event();
        data.as_object_mut().unwrap().remove("timestamp");

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timestamp"));
    }

    #[test]
    fn test_parse_missing_hash() {
        let mut data = valid_book_event();
        data.as_object_mut().unwrap().remove("hash");

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("hash"));
    }

    #[test]
    fn test_parse_missing_bids() {
        let mut data = valid_book_event();
        data.as_object_mut().unwrap().remove("bids");

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bids"));
    }

    #[test]
    fn test_parse_missing_asks() {
        let mut data = valid_book_event();
        data.as_object_mut().unwrap().remove("asks");

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("asks"));
    }

    #[test]
    fn test_parse_invalid_price_out_of_range() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [{ "price": "1.5", "size": "30" }],
            "asks": [],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[test]
    fn test_parse_negative_size() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [{ "price": "0.5", "size": "-10" }],
            "asks": [],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("negative"));
    }

    #[test]
    fn test_parse_numeric_timestamp() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [],
            "asks": [],
            "timestamp": 123456789000i64,
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().timestamp, 123456789000);
    }

    #[test]
    fn test_parse_zero_price() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [{ "price": "0.0", "size": "30" }],
            "asks": [],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().bids[0].price, 0.0);
    }

    #[test]
    fn test_parse_price_at_one() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [{ "price": "1.0", "size": "30" }],
            "asks": [],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().bids[0].price, 1.0);
    }

    #[test]
    fn test_parse_zero_size() {
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [{ "price": "0.5", "size": "0" }],
            "asks": [],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().bids[0].size, 0.0);
    }

    #[test]
    fn test_parse_decimal_price_without_leading_zero() {
        // Polymarket sends prices like ".48" without leading zero
        let data = json!({
            "event_type": "book",
            "asset_id": "12345",
            "market": "0xabcdef",
            "bids": [{ "price": ".48", "size": "30" }],
            "asks": [{ "price": ".52", "size": "25" }],
            "timestamp": "123456789000",
            "hash": "0x1234"
        });

        let result = parse_book(&data, test_observed_at());
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.bids[0].price, 0.48);
        assert_eq!(event.asks[0].price, 0.52);
    }
}
