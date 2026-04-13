//! Price change event parsing
//!
//! This module handles parsing of Polymarket price_change WebSocket events.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use popeyes_trading_types::TradeSide;

use crate::types::{ParsedPriceChange, ParsedPriceChangeEvent};

/// Parse a price_change event from JSON
///
/// # Arguments
/// * `data` - The JSON data from the WebSocket message
/// * `observed_at` - When the message was received by the subscriber service
pub fn parse_price_change(
    data: &serde_json::Value,
    observed_at: DateTime<Utc>,
) -> Result<ParsedPriceChangeEvent> {
    // Extract market
    let market = data
        .get("market")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid market"))?
        .to_string();

    // Parse timestamp
    let timestamp = if let Some(ts_str) = data.get("timestamp").and_then(|v| v.as_str()) {
        ts_str.parse::<i64>().context("Failed to parse timestamp")?
    } else if let Some(ts_num) = data.get("timestamp").and_then(|v| v.as_i64()) {
        ts_num
    } else {
        return Err(anyhow!("Missing or invalid timestamp"));
    };

    // Parse price_changes array
    let price_changes_array = data
        .get("price_changes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing or invalid price_changes array"))?;

    if price_changes_array.is_empty() {
        return Err(anyhow!("Empty price_changes array"));
    }

    let mut price_changes = Vec::new();
    for change in price_changes_array {
        price_changes.push(parse_single_price_change(change)?);
    }

    Ok(ParsedPriceChangeEvent {
        market,
        price_changes,
        timestamp,
        observed_at,
    })
}

/// Parse a single price change from array
fn parse_single_price_change(data: &serde_json::Value) -> Result<ParsedPriceChange> {
    // Extract asset_id
    let asset_id = data
        .get("asset_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid asset_id"))?
        .to_string();

    // Parse price (0-1 range)
    let price = data
        .get("price")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing price"))?
        .parse::<f64>()
        .context("Failed to parse price")?;

    if !(0.0..=1.0).contains(&price) {
        return Err(anyhow!("Price out of range (0-1): {}", price));
    }

    // Parse size
    let size = data
        .get("size")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing size"))?
        .parse::<f64>()
        .context("Failed to parse size")?;

    if size < 0.0 {
        return Err(anyhow!("Size cannot be negative: {}", size));
    }

    // Parse side
    let side_str = data
        .get("side")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid side"))?;

    let side = match side_str {
        "BUY" => TradeSide::Buy,
        "SELL" => TradeSide::Sell,
        _ => return Err(anyhow!("Invalid side value: {}", side_str)),
    };

    // Parse hash
    let hash = data
        .get("hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing or invalid hash"))?
        .to_string();

    if hash.is_empty() {
        return Err(anyhow!("Hash cannot be empty"));
    }

    // Parse best_bid
    let best_bid = data
        .get("best_bid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing best_bid"))?
        .parse::<f64>()
        .context("Failed to parse best_bid")?;

    if !(0.0..=1.0).contains(&best_bid) {
        return Err(anyhow!("best_bid out of range (0-1): {}", best_bid));
    }

    // Parse best_ask
    let best_ask = data
        .get("best_ask")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing best_ask"))?
        .parse::<f64>()
        .context("Failed to parse best_ask")?;

    if !(0.0..=1.0).contains(&best_ask) {
        return Err(anyhow!("best_ask out of range (0-1): {}", best_ask));
    }

    Ok(ParsedPriceChange {
        asset_id,
        price,
        size,
        side,
        hash,
        best_bid,
        best_ask,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_observed_at() -> DateTime<Utc> {
        Utc::now()
    }

    fn valid_price_change_event() -> serde_json::Value {
        json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "150.5",
                    "side": "BUY",
                    "hash": "0x1234abcd",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                }
            ]
        })
    }

    #[test]
    fn test_parse_valid_price_change() {
        let data = valid_price_change_event();
        let observed_at = test_observed_at();

        let result = parse_price_change(&data, observed_at);
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.market, "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999");
        assert_eq!(event.timestamp, 1699564800123);
        assert_eq!(event.observed_at, observed_at);
        assert_eq!(event.price_changes.len(), 1);

        let change = &event.price_changes[0];
        assert_eq!(change.asset_id, "109681959945973300464568698402968596289258214226684818748321941747028805721376");
        assert_eq!(change.price, 0.52);
        assert_eq!(change.size, 150.5);
        assert!(matches!(change.side, TradeSide::Buy));
        assert_eq!(change.hash, "0x1234abcd");
        assert_eq!(change.best_bid, 0.52);
        assert_eq!(change.best_ask, 0.53);
    }

    #[test]
    fn test_parse_multiple_price_changes() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "150.5",
                    "side": "BUY",
                    "hash": "0x1234abcd",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                },
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721377",
                    "price": "0.48",
                    "size": "200.3",
                    "side": "SELL",
                    "hash": "0x5678efgh",
                    "best_bid": "0.47",
                    "best_ask": "0.48"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.price_changes.len(), 2);

        let change1 = &event.price_changes[0];
        assert_eq!(change1.price, 0.52);
        assert!(matches!(change1.side, TradeSide::Buy));

        let change2 = &event.price_changes[1];
        assert_eq!(change2.price, 0.48);
        assert!(matches!(change2.side, TradeSide::Sell));
    }

    #[test]
    fn test_parse_missing_market() {
        let mut data = valid_price_change_event();
        data.as_object_mut().unwrap().remove("market");

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("market"));
    }

    #[test]
    fn test_parse_empty_price_changes_array() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": []
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty"));
    }

    #[test]
    fn test_parse_invalid_price_out_of_range() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "1.5",
                    "size": "150.5",
                    "side": "BUY",
                    "hash": "0x1234abcd",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[test]
    fn test_parse_negative_size() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "-10.0",
                    "side": "BUY",
                    "hash": "0x1234abcd",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("negative"));
    }

    #[test]
    fn test_parse_invalid_side() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "150.5",
                    "side": "INVALID",
                    "hash": "0x1234abcd",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid side"));
    }

    #[test]
    fn test_parse_empty_hash() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "150.5",
                    "side": "BUY",
                    "hash": "",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hash"));
    }

    #[test]
    fn test_parse_zero_size() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "0.0",
                    "side": "BUY",
                    "hash": "0x1234abcd",
                    "best_bid": "0.52",
                    "best_ask": "0.53"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.price_changes[0].size, 0.0);
    }

    #[test]
    fn test_parse_best_bid_ask_out_of_range() {
        let data = json!({
            "event_type": "price_change",
            "market": "dd22472e552920b8438158ea7238bfadfa4f736aa4cee91a6b86c39c6780a999",
            "timestamp": 1699564800123i64,
            "price_changes": [
                {
                    "asset_id": "109681959945973300464568698402968596289258214226684818748321941747028805721376",
                    "price": "0.52",
                    "size": "150.5",
                    "side": "BUY",
                    "hash": "0x1234abcd",
                    "best_bid": "1.5",
                    "best_ask": "0.53"
                }
            ]
        });

        let result = parse_price_change(&data, test_observed_at());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("best_bid"));
    }
}
