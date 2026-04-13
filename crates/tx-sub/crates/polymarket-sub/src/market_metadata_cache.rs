//! In-memory cache for Polymarket market metadata
//!
//! This module provides a thread-safe cache for storing market metadata indexed by condition_id.
//! The cache is used to enrich trade events with human-readable market information (ticker, title, end date)
//! without requiring additional API calls during trade processing.

use popeyes_trading_types::PolymarketMarketMetadata;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe in-memory cache for market metadata
///
/// The cache uses asset_id (256-bit integer as decimal string) as the key to look up market metadata.
/// This enables fast enrichment of trade events with market context (including outcome)
/// during event processing.
///
/// ## Thread Safety
/// Uses `Arc<RwLock<>>` for concurrent access:
/// - Multiple readers can access simultaneously
/// - Writers have exclusive access
/// - No blocking on reads when cache is not being modified
///
/// ## Usage Example
/// ```ignore
/// use crate::market_metadata_cache::MarketMetadataCache;
/// use popeyes_trading_types::PolymarketMarketMetadata;
///
/// #[tokio::main]
/// async fn main() {
///     let cache = MarketMetadataCache::new();
///
///     let metadata = PolymarketMarketMetadata {
///         event_id: "84565".to_string(),
///         ticker: "btc-updown-15m-1763567100".to_string(),
///         title: "Bitcoin Up or Down - November 19, 10:45AM-11:00AM ET".to_string(),
///         end_date: "2025-11-19T16:00:00Z".to_string(),
///     };
///
///     cache.insert("condition_id_hex".to_string(), metadata).await;
///
///     if let Some(metadata) = cache.get("condition_id_hex").await {
///         println!("Found metadata for ticker: {}", metadata.ticker);
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MarketMetadataCache {
    /// Internal cache storage: asset_id -> metadata
    cache: Arc<RwLock<HashMap<String, PolymarketMarketMetadata>>>,
}

impl MarketMetadataCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert market metadata for an asset_id
    ///
    /// # Arguments
    /// * `asset_id` - Asset ID (256-bit integer as decimal string)
    /// * `metadata` - Market metadata to cache
    ///
    /// # Example
    /// ```ignore
    /// let cache = MarketMetadataCache::new();
    /// let metadata = PolymarketMarketMetadata {
    ///     event_id: "84565".to_string(),
    ///     ticker: "btc-updown-15m-1763567100".to_string(),
    ///     title: "Bitcoin Up or Down".to_string(),
    ///     end_date: "2025-11-19T16:00:00Z".to_string(),
    /// };
    /// cache.insert("abc123".to_string(), metadata).await;
    /// ```
    #[allow(dead_code)]
    pub async fn insert(&self, asset_id: String, metadata: PolymarketMarketMetadata) {
        let mut cache = self.cache.write().await;
        cache.insert(asset_id, metadata);
    }

    /// Get market metadata for an asset_id
    ///
    /// Returns `None` if the asset_id is not found in the cache.
    ///
    /// # Arguments
    /// * `asset_id` - Asset ID to lookup (256-bit integer as decimal string)
    ///
    /// # Returns
    /// `Some(metadata)` if found, `None` otherwise
    ///
    /// # Example
    /// ```ignore
    /// let cache = MarketMetadataCache::new();
    /// if let Some(metadata) = cache.get("abc123").await {
    ///     println!("Ticker: {}", metadata.ticker);
    /// }
    /// ```
    pub async fn get(&self, asset_id: &str) -> Option<PolymarketMarketMetadata> {
        let cache = self.cache.read().await;
        cache.get(asset_id).cloned()
    }

    /// Get the current number of cached entries
    ///
    /// # Returns
    /// Number of asset_ids currently cached
    pub async fn len(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Check if the cache is empty
    ///
    /// # Returns
    /// `true` if cache contains no entries, `false` otherwise
    #[allow(dead_code)]
    pub async fn is_empty(&self) -> bool {
        let cache = self.cache.read().await;
        cache.is_empty()
    }

    /// Clear all cached entries
    ///
    /// This is useful for testing or when forcing a full cache refresh.
    #[allow(dead_code)]
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Insert multiple metadata entries at once
    ///
    /// More efficient than calling `insert()` multiple times as it only acquires the write lock once.
    ///
    /// # Arguments
    /// * `entries` - Iterator of (asset_id, metadata) tuples
    ///
    /// # Example
    /// ```ignore
    /// let cache = MarketMetadataCache::new();
    /// let entries = vec![
    ///     ("cond1".to_string(), PolymarketMarketMetadata {
    ///         event_id: "1".to_string(),
    ///         ticker: "btc-updown-15m-1".to_string(),
    ///         title: "BTC Market 1".to_string(),
    ///         end_date: "2025-11-19T16:00:00Z".to_string(),
    ///     }),
    ///     ("cond2".to_string(), PolymarketMarketMetadata {
    ///         event_id: "2".to_string(),
    ///         ticker: "eth-updown-15m-2".to_string(),
    ///         title: "ETH Market 2".to_string(),
    ///         end_date: "2025-11-19T17:00:00Z".to_string(),
    ///     }),
    /// ];
    /// cache.insert_batch(entries).await;
    /// ```
    pub async fn insert_batch<I>(&self, entries: I)
    where
        I: IntoIterator<Item = (String, PolymarketMarketMetadata)>,
    {
        let mut cache = self.cache.write().await;
        for (asset_id, metadata) in entries {
            cache.insert(asset_id, metadata);
        }
    }
}

impl Default for MarketMetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metadata(event_id: &str, ticker: &str) -> PolymarketMarketMetadata {
        PolymarketMarketMetadata {
            event_id: event_id.to_string(),
            ticker: ticker.to_string(),
            title: format!("Test Market - {}", ticker),
            end_date: "2025-11-19T16:00:00Z".to_string(),
            outcome: Some("Up".to_string()),
        }
    }

    #[tokio::test]
    async fn test_cache_new() {
        let cache = MarketMetadataCache::new();
        assert_eq!(cache.len().await, 0);
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_cache_insert_and_get() {
        let cache = MarketMetadataCache::new();
        let metadata = create_test_metadata("84565", "btc-updown-15m-1763567100");

        cache
            .insert("test_condition_id".to_string(), metadata.clone())
            .await;

        assert_eq!(cache.len().await, 1);
        assert!(!cache.is_empty().await);

        let retrieved = cache.get("test_condition_id").await;
        assert!(retrieved.is_some());

        let retrieved_metadata = retrieved.unwrap();
        assert_eq!(retrieved_metadata.event_id, "84565");
        assert_eq!(retrieved_metadata.ticker, "btc-updown-15m-1763567100");
        assert_eq!(
            retrieved_metadata.title,
            "Test Market - btc-updown-15m-1763567100"
        );
        assert_eq!(retrieved_metadata.end_date, "2025-11-19T16:00:00Z");
    }

    #[tokio::test]
    async fn test_cache_get_nonexistent() {
        let cache = MarketMetadataCache::new();
        let result = cache.get("nonexistent_condition_id").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_insert_overwrite() {
        let cache = MarketMetadataCache::new();

        let metadata1 = create_test_metadata("84565", "btc-updown-15m-1");
        let metadata2 = create_test_metadata("84566", "btc-updown-15m-2");

        cache
            .insert("test_condition_id".to_string(), metadata1)
            .await;
        cache
            .insert("test_condition_id".to_string(), metadata2)
            .await;

        assert_eq!(cache.len().await, 1);

        let retrieved = cache.get("test_condition_id").await.unwrap();
        assert_eq!(retrieved.event_id, "84566");
        assert_eq!(retrieved.ticker, "btc-updown-15m-2");
    }

    #[tokio::test]
    async fn test_cache_multiple_entries() {
        let cache = MarketMetadataCache::new();

        let metadata1 = create_test_metadata("1", "btc-updown-15m-1");
        let metadata2 = create_test_metadata("2", "eth-updown-15m-2");
        let metadata3 = create_test_metadata("3", "sol-updown-15m-3");

        cache.insert("cond1".to_string(), metadata1).await;
        cache.insert("cond2".to_string(), metadata2).await;
        cache.insert("cond3".to_string(), metadata3).await;

        assert_eq!(cache.len().await, 3);

        assert!(cache.get("cond1").await.is_some());
        assert!(cache.get("cond2").await.is_some());
        assert!(cache.get("cond3").await.is_some());
        assert!(cache.get("cond4").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = MarketMetadataCache::new();

        let metadata = create_test_metadata("84565", "btc-updown-15m-1");
        cache.insert("cond1".to_string(), metadata).await;

        assert_eq!(cache.len().await, 1);

        cache.clear().await;

        assert_eq!(cache.len().await, 0);
        assert!(cache.is_empty().await);
        assert!(cache.get("cond1").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_insert_batch() {
        let cache = MarketMetadataCache::new();

        let entries = vec![
            (
                "cond1".to_string(),
                create_test_metadata("1", "btc-updown-15m-1"),
            ),
            (
                "cond2".to_string(),
                create_test_metadata("2", "eth-updown-15m-2"),
            ),
            (
                "cond3".to_string(),
                create_test_metadata("3", "sol-updown-15m-3"),
            ),
        ];

        cache.insert_batch(entries).await;

        assert_eq!(cache.len().await, 3);
        assert!(cache.get("cond1").await.is_some());
        assert!(cache.get("cond2").await.is_some());
        assert!(cache.get("cond3").await.is_some());
    }

    #[tokio::test]
    async fn test_cache_concurrent_reads() {
        let cache = MarketMetadataCache::new();
        let metadata = create_test_metadata("84565", "btc-updown-15m-1");
        cache.insert("cond1".to_string(), metadata).await;

        // Spawn multiple concurrent read tasks
        let mut handles = vec![];
        for _ in 0..10 {
            let cache_clone = cache.clone();
            let handle = tokio::spawn(async move {
                let result = cache_clone.get("cond1").await;
                assert!(result.is_some());
                result.unwrap()
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let metadata = handle.await.unwrap();
            assert_eq!(metadata.event_id, "84565");
        }
    }

    #[tokio::test]
    async fn test_cache_concurrent_reads_and_writes() {
        let cache = MarketMetadataCache::new();

        // Spawn concurrent read and write tasks
        let mut handles = vec![];

        // Writers
        for i in 0..5 {
            let cache_clone = cache.clone();
            let handle = tokio::spawn(async move {
                let metadata = create_test_metadata(&i.to_string(), &format!("ticker-{}", i));
                cache_clone
                    .insert(format!("cond{}", i), metadata)
                    .await;
            });
            handles.push(handle);
        }

        // Readers
        for i in 0..5 {
            let cache_clone = cache.clone();
            let handle = tokio::spawn(async move {
                // May or may not find the entry depending on timing
                let _ = cache_clone.get(&format!("cond{}", i)).await;
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all entries were written
        assert_eq!(cache.len().await, 5);
    }
}
