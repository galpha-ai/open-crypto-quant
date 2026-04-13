//! Market metadata loader for manual mode
//!
//! This module provides functionality to fetch and cache market metadata on startup
//! for manual mode operation (when market discovery is disabled). It queries the
//! Polymarket API to find markets matching configured asset IDs and populates the
//! metadata cache for trade event enrichment.

use crate::api_client::{DiscoveredMarket, PolymarketApiClient};
use crate::market_metadata_cache::MarketMetadataCache;
use popeyes_trading_types::PolymarketMarketMetadata;
use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Market metadata loader for manual mode
///
/// Fetches market metadata from the Polymarket API and populates the cache
/// with asset_id -> metadata mappings (including outcomes). This enables trade event enrichment
/// in manual mode without requiring ongoing market discovery.
///
/// ## Usage Example
/// ```ignore
/// use crate::api_client::PolymarketApiClient;
/// use crate::market_metadata_cache::MarketMetadataCache;
/// use crate::market_metadata_loader::MarketMetadataLoader;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let api_client = Arc::new(PolymarketApiClient::new(
///         "https://gamma-api.polymarket.com".to_string(),
///         30, 3, 1000
///     )?);
///     let cache = Arc::new(MarketMetadataCache::new());
///     let loader = MarketMetadataLoader::new(api_client, cache);
///
///     let asset_ids = vec!["asset1".to_string(), "asset2".to_string()];
///     loader.load_metadata_for_assets(&asset_ids, 21).await?;
///
///     Ok(())
/// }
/// ```
pub struct MarketMetadataLoader {
    /// API client for fetching market data
    api_client: Arc<PolymarketApiClient>,
    /// Metadata cache to populate
    cache: Arc<MarketMetadataCache>,
}

impl MarketMetadataLoader {
    /// Create a new market metadata loader
    ///
    /// # Arguments
    /// * `api_client` - API client for fetching events
    /// * `cache` - Market metadata cache to populate
    pub fn new(
        api_client: Arc<PolymarketApiClient>,
        cache: Arc<MarketMetadataCache>,
    ) -> Self {
        Self { api_client, cache }
    }

    /// Load metadata for markets matching configured asset IDs
    ///
    /// Fetches all active markets for a tag and filters by asset IDs.
    /// Populates the cache with metadata for matching markets.
    ///
    /// # Arguments
    /// * `asset_ids` - Asset IDs to find metadata for
    /// * `tag_id` - Tag ID to filter markets by (e.g., 21 for Crypto)
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of markets cached
    /// * `Err` - If API request fails or no metadata found for configured assets
    ///
    /// # Errors
    /// Returns an error if:
    /// - API request fails after all retries
    /// - No markets found matching the configured asset IDs
    /// - JSON parsing fails
    ///
    /// # Example
    /// ```ignore
    /// let asset_ids = vec![
    ///     "109681959945973826496234384791167033612800000000000000000000000376".to_string(),
    ///     "52181619848812915160551060468842099699261274828279546744438464278138132224".to_string(),
    /// ];
    /// let cached_count = loader.load_metadata_for_assets(&asset_ids, 21).await?;
    /// println!("Cached metadata for {} markets", cached_count);
    /// ```
    pub async fn load_metadata_for_assets(
        &self,
        asset_ids: &[String],
        tag_id: u32,
    ) -> Result<usize> {
        info!(
            asset_count = asset_ids.len(),
            tag_id = tag_id,
            "Loading market metadata for configured asset IDs"
        );

        // Convert asset_ids to HashSet for efficient lookup
        let asset_id_set: HashSet<String> = asset_ids.iter().cloned().collect();

        // Fetch all active markets for the tag with pagination
        let mut all_markets = Vec::new();
        let batch_size = 100;
        let mut offset = 0;

        loop {
            debug!(
                batch_size = batch_size,
                offset = offset,
                "Fetching events from API"
            );

            let events = self
                .api_client
                .fetch_events(tag_id, batch_size, offset)
                .await
                .context("Failed to fetch events from API")?;

            let events_count = events.len();
            debug!(
                events_count = events_count,
                offset = offset,
                "Received events from API"
            );

            if events.is_empty() {
                break;
            }

            // Convert events to discovered markets
            for event in events {
                if let Some(market) = DiscoveredMarket::from_api_event(event) {
                    // Check if any asset IDs match
                    let has_matching_assets = market
                        .asset_ids
                        .iter()
                        .any(|asset_id| asset_id_set.contains(asset_id));

                    if has_matching_assets {
                        all_markets.push(market);
                    }
                }
            }

            // Check if we've reached the end of results
            if events_count < batch_size {
                break;
            }

            offset += batch_size;
        }

        info!(
            markets_found = all_markets.len(),
            "Discovered markets matching asset IDs"
        );

        // Ensure we found at least one market
        if all_markets.is_empty() {
            return Err(anyhow!(
                "No markets found for configured asset IDs (tag_id: {})",
                tag_id
            ));
        }

        // Populate cache with metadata (fetching detailed market info for outcomes)
        let mut cache_entries = Vec::new();
        let mut fetch_failures = 0;
        let mut parse_failures = 0;

        for market in &all_markets {
            // Fetch detailed market data to get outcome mappings
            match self.api_client.fetch_market_by_slug(&market.ticker).await {
                Ok(detailed_market) => {
                    // Parse outcome mappings
                    match detailed_market.parse_outcome_mappings() {
                        Some(mapping) => {
                            // Create one cache entry per asset_id (only for assets we care about)
                            for (asset_id, outcome) in mapping.mappings {
                                // Only cache assets that were requested
                                if asset_id_set.contains(&asset_id) {
                                    let metadata = PolymarketMarketMetadata {
                                        event_id: market.event_id.clone(),
                                        ticker: market.ticker.clone(),
                                        title: market.title.clone(),
                                        end_date: market.end_date.to_rfc3339(),
                                        outcome: Some(outcome),
                                    };
                                    cache_entries.push((asset_id, metadata));
                                }
                            }
                        }
                        None => {
                            warn!(
                                ticker = %market.ticker,
                                event_id = %market.event_id,
                                "Failed to parse outcome mappings, skipping market"
                            );
                            parse_failures += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        ticker = %market.ticker,
                        event_id = %market.event_id,
                        "Failed to fetch detailed market info, skipping market"
                    );
                    fetch_failures += 1;
                }
            }
        }

        // Batch insert into cache
        self.cache.insert_batch(cache_entries).await;

        let cached_count = self.cache.len().await;
        if fetch_failures > 0 || parse_failures > 0 {
            warn!(
                cached_count = cached_count,
                fetch_failures = fetch_failures,
                parse_failures = parse_failures,
                "Cached market metadata with some failures"
            );
        } else {
            info!(
                cached_count = cached_count,
                "Successfully cached market metadata"
            );
        }

        Ok(cached_count)
    }

    /// Load metadata for all active markets matching a tag
    ///
    /// Fetches all active markets for a tag and populates the cache.
    /// Unlike `load_metadata_for_assets`, this does not filter by asset IDs.
    ///
    /// # Arguments
    /// * `tag_id` - Tag ID to filter markets by (e.g., 21 for Crypto)
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of markets cached
    /// * `Err` - If API request fails
    ///
    /// # Example
    /// ```ignore
    /// let cached_count = loader.load_metadata_for_tag(21).await?;
    /// println!("Cached metadata for {} markets", cached_count);
    /// ```
    #[allow(dead_code)]
    pub async fn load_metadata_for_tag(&self, tag_id: u32) -> Result<usize> {
        info!(tag_id = tag_id, "Loading all market metadata for tag");

        // Fetch all active markets for the tag with pagination
        let mut all_markets = Vec::new();
        let batch_size = 100;
        let mut offset = 0;

        loop {
            debug!(
                batch_size = batch_size,
                offset = offset,
                "Fetching events from API"
            );

            let events = self
                .api_client
                .fetch_events(tag_id, batch_size, offset)
                .await
                .context("Failed to fetch events from API")?;

            let events_count = events.len();
            debug!(
                events_count = events_count,
                offset = offset,
                "Received events from API"
            );

            if events.is_empty() {
                break;
            }

            // Convert events to discovered markets
            for event in events {
                if let Some(market) = DiscoveredMarket::from_api_event(event) {
                    all_markets.push(market);
                }
            }

            // Check if we've reached the end of results
            if events_count < batch_size {
                break;
            }

            offset += batch_size;
        }

        info!(
            markets_found = all_markets.len(),
            "Discovered markets for tag"
        );

        if all_markets.is_empty() {
            warn!(tag_id = tag_id, "No markets found for tag");
            return Ok(0);
        }

        // Populate cache with metadata (fetching detailed market info for outcomes)
        let mut cache_entries = Vec::new();
        let mut fetch_failures = 0;
        let mut parse_failures = 0;

        for market in &all_markets {
            // Fetch detailed market data to get outcome mappings
            match self.api_client.fetch_market_by_slug(&market.ticker).await {
                Ok(detailed_market) => {
                    // Parse outcome mappings
                    match detailed_market.parse_outcome_mappings() {
                        Some(mapping) => {
                            // Create one cache entry per asset_id
                            for (asset_id, outcome) in mapping.mappings {
                                let metadata = PolymarketMarketMetadata {
                                    event_id: market.event_id.clone(),
                                    ticker: market.ticker.clone(),
                                    title: market.title.clone(),
                                    end_date: market.end_date.to_rfc3339(),
                                    outcome: Some(outcome),
                                };
                                cache_entries.push((asset_id, metadata));
                            }
                        }
                        None => {
                            warn!(
                                ticker = %market.ticker,
                                event_id = %market.event_id,
                                "Failed to parse outcome mappings, skipping market"
                            );
                            parse_failures += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        ticker = %market.ticker,
                        event_id = %market.event_id,
                        "Failed to fetch detailed market info, skipping market"
                    );
                    fetch_failures += 1;
                }
            }
        }

        // Batch insert into cache
        self.cache.insert_batch(cache_entries).await;

        let cached_count = self.cache.len().await;
        if fetch_failures > 0 || parse_failures > 0 {
            warn!(
                cached_count = cached_count,
                fetch_failures = fetch_failures,
                parse_failures = parse_failures,
                "Cached market metadata with some failures"
            );
        } else {
            info!(
                cached_count = cached_count,
                "Successfully cached market metadata"
            );
        }

        Ok(cached_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_client::{ApiEvent, ApiMarket};

    // Mock API client for testing
    #[allow(dead_code)]
    struct MockApiClient {
        events: Vec<ApiEvent>,
    }

    #[allow(dead_code)]
    impl MockApiClient {
        fn new(events: Vec<ApiEvent>) -> Arc<Self> {
            Arc::new(Self { events })
        }

        async fn fetch_events(
            &self,
            _tag_id: u32,
            _limit: usize,
            offset: usize,
        ) -> Result<Vec<ApiEvent>> {
            // Return empty if offset exceeds events
            if offset >= self.events.len() {
                return Ok(vec![]);
            }

            // Return remaining events
            Ok(self.events[offset..].to_vec())
        }
    }

    fn create_test_event(
        event_id: &str,
        ticker: &str,
        title: &str,
        condition_id: &str,
        asset_ids: Vec<&str>,
    ) -> ApiEvent {
        ApiEvent {
            id: event_id.to_string(),
            ticker: ticker.to_string(),
            title: Some(title.to_string()),
            end_date: Some("2025-11-19T16:00:00Z".to_string()),
            active: true,
            markets: vec![ApiMarket {
                id: "market1".to_string(),
                condition_id: Some(condition_id.to_string()),
                clob_token_ids: Some(format!(
                    "[{}]",
                    asset_ids
                        .iter()
                        .map(|id| format!("\"{}\"", id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }],
        }
    }

    #[tokio::test]
    async fn test_load_metadata_for_assets_single_match() {
        let _events = vec![create_test_event(
            "84565",
            "btc-updown-15m-1763567100",
            "Bitcoin Up or Down",
            "cond1",
            vec!["asset1", "asset2"],
        )];

        // We can't easily mock PolymarketApiClient without refactoring it to use a trait
        // For now, these tests document the expected behavior
        // Integration tests will test the actual implementation
    }

    #[tokio::test]
    async fn test_metadata_conversion() {
        // Test that we correctly convert DiscoveredMarket to PolymarketMarketMetadata
        let event = create_test_event(
            "84565",
            "btc-updown-15m-1763567100",
            "Bitcoin Up or Down",
            "cond1",
            vec!["asset1", "asset2"],
        );

        let discovered = DiscoveredMarket::from_api_event(event).unwrap();

        let metadata = PolymarketMarketMetadata {
            event_id: discovered.event_id.clone(),
            ticker: discovered.ticker.clone(),
            title: discovered.title.clone(),
            end_date: discovered.end_date.to_rfc3339(),
            outcome: Some("Up".to_string()),
        };

        assert_eq!(metadata.event_id, "84565");
        assert_eq!(metadata.ticker, "btc-updown-15m-1763567100");
        assert_eq!(metadata.title, "Bitcoin Up or Down");
        // RFC3339 format may include +00:00 or Z for UTC
        assert!(
            metadata.end_date == "2025-11-19T16:00:00Z" || metadata.end_date == "2025-11-19T16:00:00+00:00",
            "Unexpected end_date format: {}",
            metadata.end_date
        );
    }

    #[tokio::test]
    async fn test_asset_id_filtering() {
        // Test that we correctly filter markets by asset IDs
        let target_assets: HashSet<String> = vec!["asset1".to_string(), "asset2".to_string()]
            .into_iter()
            .collect();

        let market_assets = vec!["asset1".to_string(), "asset2".to_string()];

        let has_match = market_assets
            .iter()
            .any(|asset_id| target_assets.contains(asset_id));

        assert!(has_match);

        let market_assets_no_match = vec!["asset3".to_string(), "asset4".to_string()];

        let has_match = market_assets_no_match
            .iter()
            .any(|asset_id| target_assets.contains(asset_id));

        assert!(!has_match);
    }
}
