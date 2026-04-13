//! Intent validation for order reconciliation.
//!
//! Validates `OrderIntent` before processing to ensure all values are valid.

use crate::position::errors::PositionError;
use crate::signal::OrderIntent;

/// Validate an OrderIntent before processing.
///
/// Ensures all prices and sizes are positive.
///
/// # Errors
///
/// Returns `PositionError::InvalidIntent` if:
/// - Any bid price is <= 0
/// - Any bid size is <= 0
/// - Any ask price is <= 0
/// - Any ask size is <= 0
pub fn validate_intent(intent: &OrderIntent) -> Result<(), PositionError> {
    // Validate that prices and sizes are positive
    if let Some(bids) = &intent.bids {
        for (i, level) in bids.iter().enumerate() {
            if level.price <= 0.0 {
                return Err(PositionError::InvalidIntent(format!(
                    "Bid level {} has non-positive price: {}",
                    i, level.price
                )));
            }
            if level.size <= 0.0 {
                return Err(PositionError::InvalidIntent(format!(
                    "Bid level {} has non-positive size: {}",
                    i, level.size
                )));
            }
        }
    }

    if let Some(asks) = &intent.asks {
        for (i, level) in asks.iter().enumerate() {
            if level.price <= 0.0 {
                return Err(PositionError::InvalidIntent(format!(
                    "Ask level {} has non-positive price: {}",
                    i, level.price
                )));
            }
            if level.size <= 0.0 {
                return Err(PositionError::InvalidIntent(format!(
                    "Ask level {} has non-positive size: {}",
                    i, level.size
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::QuoteLevel;
    use chrono::Utc;

    #[test]
    fn test_valid_intent_passes() {
        let intent = OrderIntent::new(
            "token123".to_string(),
            None,
            Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
            Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        assert!(validate_intent(&intent).is_ok());
    }

    #[test]
    fn test_negative_bid_price_rejected() {
        let intent = OrderIntent::new(
            "token123".to_string(),
            None,
            Some(vec![QuoteLevel::gtc(-0.45, 100.0)]),
            None,
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let result = validate_intent(&intent);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PositionError::InvalidIntent(_)
        ));
    }

    #[test]
    fn test_zero_bid_size_rejected() {
        let intent = OrderIntent::new(
            "token123".to_string(),
            None,
            Some(vec![QuoteLevel::gtc(0.45, 0.0)]),
            None,
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let result = validate_intent(&intent);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PositionError::InvalidIntent(_)
        ));
    }

    #[test]
    fn test_negative_ask_price_rejected() {
        let intent = OrderIntent::new(
            "token123".to_string(),
            None,
            None,
            Some(vec![QuoteLevel::gtc(-0.55, 100.0)]),
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let result = validate_intent(&intent);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PositionError::InvalidIntent(_)
        ));
    }

    #[test]
    fn test_empty_bids_asks_passes() {
        let intent = OrderIntent::new(
            "token123".to_string(),
            None,
            Some(vec![]), // Empty but valid
            Some(vec![]), // Empty but valid
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        assert!(validate_intent(&intent).is_ok());
    }

    #[test]
    fn test_none_bids_asks_passes() {
        let intent = OrderIntent::new(
            "token123".to_string(),
            None,
            None, // None means preserve existing
            None,
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        assert!(validate_intent(&intent).is_ok());
    }
}
