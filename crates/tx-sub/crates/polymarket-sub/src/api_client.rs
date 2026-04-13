//! HTTP client for Polymarket Gamma API
//!
//! This module provides an HTTP client for fetching market data from the
//! Polymarket Gamma API. It includes retry logic with exponential backoff
//! for handling transient failures.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::{debug, warn};

/// API response for a single event
#[derive(Debug, Clone, Deserialize)]
pub struct ApiEvent {
    /// Event ID
    pub id: String,
    /// Event ticker (e.g., "sol-updown-15m-1763489700")
    pub ticker: String,
    /// Event title/question (e.g., "Bitcoin Up or Down - November 19, 10:45AM-11:00AM ET")
    pub title: Option<String>,
    /// Event end date (ISO 8601 format)
    /// Note: Some events don't have an endDate and will be filtered out
    #[serde(rename = "endDate")]
    pub end_date: Option<String>,
    /// Whether the event is active
    #[allow(dead_code)]
    pub active: bool,
    /// Markets associated with this event
    pub markets: Vec<ApiMarket>,
}

/// API response for a single market within an event
#[derive(Debug, Clone, Deserialize)]
pub struct ApiMarket {
    /// Market ID
    pub id: String,
    /// Condition ID (hex string without 0x prefix)
    /// Used as the key in market metadata cache
    #[serde(rename = "conditionId")]
    pub condition_id: Option<String>,
    /// CLOB token IDs (JSON array string: ["id1", "id2"])
    /// These are the asset IDs used for WebSocket subscriptions
    #[serde(rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
}

/// Detailed market response from /markets/slug/{slug} endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct DetailedMarket {
    /// Market slug (same as ticker for crypto binary markets)
    #[allow(dead_code)]
    pub slug: String,
    /// Outcomes array as JSON string (e.g., "[\"Up\", \"Down\"]")
    pub outcomes: String,
    /// CLOB token IDs as JSON string (e.g., "[\"id1\", \"id2\"]")
    #[serde(rename = "clobTokenIds")]
    pub clob_token_ids: String,
}

/// Parsed outcome-to-asset mapping from DetailedMarket
#[derive(Debug, Clone)]
pub struct OutcomeAssetMapping {
    /// Maps asset_id -> outcome (e.g., "Up" or "Down")
    pub mappings: Vec<(String, String)>,
}

impl DetailedMarket {
    /// Parse outcomes and clobTokenIds to create asset_id -> outcome mappings
    ///
    /// Returns None if:
    /// - JSON parsing fails
    /// - Arrays have different lengths
    /// - Arrays are empty
    pub fn parse_outcome_mappings(&self) -> Option<OutcomeAssetMapping> {
        // Parse outcomes JSON array
        let outcomes: Vec<String> = serde_json::from_str(&self.outcomes).ok()?;

        // Parse clob_token_ids JSON array
        let asset_ids: Vec<String> = serde_json::from_str(&self.clob_token_ids).ok()?;

        // Validate arrays have same length and are not empty
        if outcomes.len() != asset_ids.len() || outcomes.is_empty() {
            return None;
        }

        // Create mappings: asset_id -> outcome
        let mappings = asset_ids
            .into_iter()
            .zip(outcomes.into_iter())
            .collect();

        Some(OutcomeAssetMapping { mappings })
    }
}

/// Error types for API client operations
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpError(reqwest::Error),

    /// HTTP error status code
    #[error("HTTP error {status}: {body}")]
    HttpStatus { status: u16, body: String },

    /// JSON parsing failed
    #[error("Failed to parse JSON response: {0}")]
    ParseError(String),

    /// Request timeout
    #[error("Request timeout after {0:?}")]
    Timeout(Duration),

    /// All retry attempts exhausted
    #[error("All {0} retry attempts exhausted")]
    RetriesExhausted(u32),
}

/// HTTP client for Polymarket Gamma API
pub struct PolymarketApiClient {
    /// HTTP client with timeout and TLS configuration
    client: Client,
    /// API base URL (e.g., "https://gamma-api.polymarket.com")
    base_url: String,
    /// Request timeout duration
    timeout: Duration,
    /// Number of retry attempts for failed requests
    retry_attempts: u32,
    /// Initial backoff duration between retries
    retry_backoff: Duration,
}

impl PolymarketApiClient {
    /// Create a new API client with the given configuration
    ///
    /// # Arguments
    /// * `base_url` - API base URL (must be HTTPS)
    /// * `timeout_secs` - Request timeout in seconds
    /// * `retry_attempts` - Number of retry attempts for failed requests
    /// * `retry_backoff_ms` - Initial backoff between retries in milliseconds
    pub fn new(
        base_url: String,
        timeout_secs: u64,
        retry_attempts: u32,
        retry_backoff_ms: u64,
    ) -> Result<Self> {
        let timeout = Duration::from_secs(timeout_secs);

        let client = Client::builder()
            .timeout(timeout)
            .use_rustls_tls() // Use rustls for TLS
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url,
            timeout,
            retry_attempts,
            retry_backoff: Duration::from_millis(retry_backoff_ms),
        })
    }

    /// Fetch detailed market data by slug
    ///
    /// # Arguments
    /// * `slug` - Market slug (e.g., "btc-updown-15m-1763567100")
    ///
    /// # Returns
    /// Detailed market data including outcomes and clobTokenIds, or an error if the request fails
    pub async fn fetch_market_by_slug(&self, slug: &str) -> Result<DetailedMarket> {
        let url = format!("{}/markets/slug/{}", self.base_url, slug);

        // Retry loop with exponential backoff
        let mut attempt = 0;
        let mut last_error = None;

        while attempt <= self.retry_attempts {
            if attempt > 0 {
                // Calculate exponential backoff: initial * 2^(attempt-1)
                let backoff = self.retry_backoff * 2_u32.pow(attempt - 1);
                debug!(
                    attempt = attempt,
                    backoff_ms = backoff.as_millis(),
                    slug = slug,
                    "Retrying market fetch after backoff"
                );
                tokio::time::sleep(backoff).await;
            }

            attempt += 1;

            // Execute HTTP request
            match self.execute_market_request(&url).await {
                Ok(market) => {
                    debug!(
                        slug = slug,
                        attempt = attempt,
                        "Successfully fetched market details from API"
                    );
                    return Ok(market);
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        slug = slug,
                        attempt = attempt,
                        max_attempts = self.retry_attempts + 1,
                        "Market fetch request failed"
                    );
                    last_error = Some(anyhow!(e));
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            anyhow!(ApiError::RetriesExhausted(self.retry_attempts))
        }))
    }

    /// Fetch events from the Polymarket API
    ///
    /// # Arguments
    /// * `tag_id` - Tag ID to filter by (e.g., 21 for Crypto)
    /// * `limit` - Number of events to fetch per request
    /// * `offset` - Pagination offset
    ///
    /// # Returns
    /// A vector of events matching the filters, or an error if the request fails
    /// after all retry attempts
    pub async fn fetch_events(
        &self,
        tag_id: u32,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ApiEvent>> {
        let url = format!("{}/events", self.base_url);

        // Build query parameters
        let params = [
            ("tag", tag_id.to_string()),
            ("active", "true".to_string()),
            ("closed", "false".to_string()),
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ];

        // Retry loop with exponential backoff
        let mut attempt = 0;
        let mut last_error = None;

        while attempt <= self.retry_attempts {
            if attempt > 0 {
                // Calculate exponential backoff: initial * 2^(attempt-1)
                let backoff = self.retry_backoff * 2_u32.pow(attempt - 1);
                debug!(
                    attempt = attempt,
                    backoff_ms = backoff.as_millis(),
                    "Retrying API request after backoff"
                );
                tokio::time::sleep(backoff).await;
            }

            attempt += 1;

            // Execute HTTP request
            match self.execute_request(&url, &params).await {
                Ok(events) => {
                    debug!(
                        events_count = events.len(),
                        attempt = attempt,
                        "Successfully fetched events from API"
                    );
                    return Ok(events);
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        attempt = attempt,
                        max_attempts = self.retry_attempts + 1,
                        "API request failed"
                    );
                    last_error = Some(anyhow!(e));
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            anyhow!(ApiError::RetriesExhausted(self.retry_attempts))
        }))
    }

    /// Execute a single HTTP request for market details with error handling
    async fn execute_market_request(&self, url: &str) -> Result<DetailedMarket, ApiError> {
        debug!(url = url, "Sending market details API request");

        // Send GET request
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ApiError::Timeout(self.timeout)
                } else {
                    ApiError::HttpError(e)
                }
            })?;

        // Check HTTP status code
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        // Get response body as text first for better error messages
        let body_text = response.text().await.map_err(|e| {
            ApiError::ParseError(format!("Failed to read response body: {}", e))
        })?;

        debug!(
            body_length = body_text.len(),
            "Received market details API response"
        );

        // Parse JSON response
        let market: DetailedMarket = serde_json::from_str(&body_text).map_err(|e| {
            warn!(
                error = %e,
                body_length = body_text.len(),
                body_preview = &body_text[..body_text.len().min(500)],
                "Failed to parse market details JSON response"
            );
            ApiError::ParseError(e.to_string())
        })?;

        Ok(market)
    }

    /// Execute a single HTTP request with error handling
    async fn execute_request(
        &self,
        url: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<ApiEvent>, ApiError> {
        debug!(
            url = url,
            params = ?params,
            "Sending API request"
        );

        // Send GET request with query parameters
        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ApiError::Timeout(self.timeout)
                } else {
                    ApiError::HttpError(e)
                }
            })?;

        // Check HTTP status code
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        // Get response body as text first for better error messages
        let body_text = response.text().await.map_err(|e| {
            ApiError::ParseError(format!("Failed to read response body: {}", e))
        })?;

        debug!(
            body_length = body_text.len(),
            "Received API response"
        );

        // Parse JSON response
        let events: Vec<ApiEvent> = serde_json::from_str(&body_text).map_err(|e| {
            warn!(
                error = %e,
                body_length = body_text.len(),
                body_preview = &body_text[..body_text.len().min(500)],
                "Failed to parse JSON response"
            );
            ApiError::ParseError(e.to_string())
        })?;

        Ok(events)
    }
}

/// Discovered market metadata from API
#[derive(Debug, Clone)]
pub struct DiscoveredMarket {
    /// Event ID
    pub event_id: String,
    /// Event ticker (e.g., "sol-updown-15m-1763489700")
    pub ticker: String,
    /// Event title/question (e.g., "Bitcoin Up or Down - November 19, 10:45AM-11:00AM ET")
    pub title: String,
    /// Market end date (when it resolves)
    pub end_date: DateTime<Utc>,
    /// Condition ID (hex string without 0x prefix)
    /// Used as the key in market metadata cache
    #[allow(dead_code)]
    pub condition_id: String,
    /// Asset IDs for this market (typically 2: "Up" and "Down")
    pub asset_ids: Vec<String>,
}

impl DiscoveredMarket {
    /// Try to create a DiscoveredMarket from an ApiEvent
    ///
    /// Returns None if:
    /// - The event has no markets
    /// - The title is missing
    /// - The end_date is missing or cannot be parsed
    /// - The condition_id is missing from the first market
    /// - No clob_token_ids are present
    pub fn from_api_event(event: ApiEvent) -> Option<Self> {
        // Require title (skip events without a title)
        let title = event.title.as_ref()?.clone();

        // Parse end_date (skip events without an end date)
        let end_date_str = event.end_date.as_ref()?;
        let end_date = DateTime::parse_from_rfc3339(end_date_str)
            .ok()?
            .with_timezone(&Utc);

        // Get condition_id from first market (all markets in an event share the same condition_id)
        let first_market = event.markets.first()?;
        let condition_id = first_market.condition_id.as_ref()?.clone();

        // Collect all asset IDs from markets
        let mut asset_ids = Vec::new();
        for market in &event.markets {
            if let Some(ref clob_ids) = market.clob_token_ids {
                // Parse JSON array: ["id1", "id2"]
                match serde_json::from_str::<Vec<String>>(clob_ids) {
                    Ok(ids) => asset_ids.extend(ids),
                    Err(e) => {
                        warn!(
                            event_id = %event.id,
                            market_id = %market.id,
                            error = %e,
                            "Failed to parse clob_token_ids JSON"
                        );
                        continue;
                    }
                }
            }
        }

        // Require at least one asset ID
        if asset_ids.is_empty() {
            return None;
        }

        Some(DiscoveredMarket {
            event_id: event.id,
            ticker: event.ticker,
            title,
            end_date,
            condition_id,
            asset_ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_api_event() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: Some("2025-11-18T18:30:00Z".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: Some("0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e".to_string()),
                clob_token_ids: Some(
                    r#"["109681959945973826496234384791167033612800000000000000000000000376", "52181619848812915160551060468842099699261274828279546744438464278138132224"]"#.to_string(),
                ),
            }],
        };

        let discovered = DiscoveredMarket::from_api_event(event).unwrap();
        assert_eq!(discovered.event_id, "83768");
        assert_eq!(discovered.ticker, "sol-updown-15m-1763489700");
        assert_eq!(discovered.title, "Solana Up or Down - November 18, 6:30PM ET");
        assert_eq!(discovered.condition_id, "0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e");
        assert_eq!(discovered.asset_ids.len(), 2);
        assert_eq!(
            discovered.asset_ids[0],
            "109681959945973826496234384791167033612800000000000000000000000376"
        );
    }

    #[test]
    fn test_parse_event_missing_clob_token_ids() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: Some("2025-11-18T18:30:00Z".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: Some("0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e".to_string()),
                clob_token_ids: None,
            }],
        };

        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_missing_end_date() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: None,
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: Some("0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e".to_string()),
                clob_token_ids: Some(r#"["asset1"]"#.to_string()),
            }],
        };

        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_invalid_end_date() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: Some("invalid-date".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: Some("0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e".to_string()),
                clob_token_ids: Some(r#"["asset1"]"#.to_string()),
            }],
        };

        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_invalid_clob_json() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: Some("2025-11-18T18:30:00Z".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: Some("0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e".to_string()),
                clob_token_ids: Some("not-valid-json".to_string()),
            }],
        };

        // Should return None because no valid asset IDs were extracted
        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_empty_markets() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: Some("2025-11-18T18:30:00Z".to_string()),
            active: true,
            markets: vec![],
        };

        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_missing_title() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: None,
            end_date: Some("2025-11-18T18:30:00Z".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: Some("0xc3b9208959db609c911f6e24289fb309179b5b556b9590268c2dc0274186676e".to_string()),
                clob_token_ids: Some(r#"["asset1"]"#.to_string()),
            }],
        };

        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_event_missing_condition_id() {
        let event = ApiEvent {
            id: "83768".to_string(),
            ticker: "sol-updown-15m-1763489700".to_string(),
            title: Some("Solana Up or Down - November 18, 6:30PM ET".to_string()),
            end_date: Some("2025-11-18T18:30:00Z".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "687550".to_string(),
                condition_id: None,
                clob_token_ids: Some(r#"["asset1"]"#.to_string()),
            }],
        };

        let result = DiscoveredMarket::from_api_event(event);
        assert!(result.is_none());
    }

    #[test]
    #[ignore] // Run manually with: cargo test test_parse_real_api_response -- --ignored --nocapture
    fn test_parse_real_api_response() {
        // Load real API response from /tmp/full_response.json
        let data = std::fs::read_to_string("/tmp/full_response.json")
            .expect("Failed to read /tmp/full_response.json");

        println!("Response size: {} bytes", data.len());

        // Try to parse it
        match serde_json::from_str::<Vec<ApiEvent>>(&data) {
            Ok(events) => {
                println!("Successfully parsed {} events", events.len());
                for (i, event) in events.iter().take(3).enumerate() {
                    println!("Event {}: id={}, ticker={}, markets={}",
                        i, event.id, event.ticker, event.markets.len());
                }
            }
            Err(e) => {
                println!("Parse error: {}", e);
                println!("Error position: line {}, column {}", e.line(), e.column());

                // Show context around the error
                let pos = e.column() - 1;
                let start = pos.saturating_sub(100);
                let end = (pos + 100).min(data.len());
                println!("\nContext around error:");
                println!("{}", &data[start..end]);
            }
        }
    }

    #[test]
    fn test_parse_detailed_market_valid() {
        let market = DetailedMarket {
            slug: "btc-updown-15m-1763567100".to_string(),
            outcomes: r#"["Up", "Down"]"#.to_string(),
            clob_token_ids: r#"["123456", "789012"]"#.to_string(),
        };

        let mapping = market.parse_outcome_mappings().unwrap();
        assert_eq!(mapping.mappings.len(), 2);
        assert_eq!(mapping.mappings[0].0, "123456");
        assert_eq!(mapping.mappings[0].1, "Up");
        assert_eq!(mapping.mappings[1].0, "789012");
        assert_eq!(mapping.mappings[1].1, "Down");
    }

    #[test]
    fn test_parse_detailed_market_length_mismatch() {
        let market = DetailedMarket {
            slug: "btc-updown-15m-1763567100".to_string(),
            outcomes: r#"["Up", "Down"]"#.to_string(),
            clob_token_ids: r#"["123456"]"#.to_string(),
        };

        let result = market.parse_outcome_mappings();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_detailed_market_empty_arrays() {
        let market = DetailedMarket {
            slug: "btc-updown-15m-1763567100".to_string(),
            outcomes: r#"[]"#.to_string(),
            clob_token_ids: r#"[]"#.to_string(),
        };

        let result = market.parse_outcome_mappings();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_detailed_market_invalid_json() {
        let market = DetailedMarket {
            slug: "btc-updown-15m-1763567100".to_string(),
            outcomes: r#"invalid json"#.to_string(),
            clob_token_ids: r#"["123456", "789012"]"#.to_string(),
        };

        let result = market.parse_outcome_mappings();
        assert!(result.is_none());
    }
}
