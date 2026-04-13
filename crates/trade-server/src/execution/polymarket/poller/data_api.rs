//! Data API utilities for merge indexing queries.
//!
//! This module provides functions for querying the Polymarket Data API
//! to check if merge transactions have been indexed.

use std::time::Duration;

use tracing::{debug, info, warn};

/// Wait for a merge transaction to be indexed in the Data API.
///
/// Polls the Polymarket Data API /activity endpoint until the merge transaction
/// appears in the activity list. This ensures the CLOB has indexed the merge
/// before we try to refresh balances.
///
/// # Arguments
/// * `http_client` - HTTP client for making requests
/// * `wallet` - Wallet address (formatted as hex string with 0x prefix)
/// * `tx_hash` - Transaction hash to look for
/// * `max_attempts` - Maximum number of polling attempts
///
/// # Returns
/// `true` if the merge was found, `false` if timeout.
pub async fn wait_for_merge_indexed(
    http_client: &reqwest::Client,
    wallet: &str,
    tx_hash: &str,
    max_attempts: u32,
) -> bool {
    let url = format!(
        "https://data-api.polymarket.com/activity?user={}&type=MERGE&limit=20",
        wallet
    );

    info!(
        tx_hash = %tx_hash,
        wallet = %wallet,
        "Waiting for merge to be indexed in Data API"
    );

    for i in 0..max_attempts {
        match http_client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Vec<serde_json::Value>>().await {
                        Ok(activities) => {
                            // Search for matching transactionHash in activity list
                            for activity in &activities {
                                let api_hash = activity
                                    .get("transactionHash")
                                    .or_else(|| activity.get("hash"))
                                    .and_then(|v| v.as_str());

                                if let Some(hash) = api_hash {
                                    if hash.eq_ignore_ascii_case(tx_hash) {
                                        info!(
                                            tx_hash = %tx_hash,
                                            activity_type = ?activity.get("type"),
                                            "Merge transaction found in Data API"
                                        );
                                        return true;
                                    }
                                }
                            }
                            debug!(
                                tx_hash = %tx_hash,
                                attempt = i + 1,
                                max_attempts = max_attempts,
                                activities_count = activities.len(),
                                "Merge not yet indexed, waiting..."
                            );
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to parse Data API response");
                        }
                    }
                } else {
                    warn!(status = %response.status(), "Data API request failed");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to call Data API");
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    warn!(
        tx_hash = %tx_hash,
        max_attempts = max_attempts,
        "Timeout waiting for merge to be indexed"
    );
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wait_for_merge_indexed_timeout() {
        // Test with invalid URL to simulate timeout behavior
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();

        let result = wait_for_merge_indexed(
            &client,
            "0x1234567890abcdef",
            "0xdeadbeef",
            1, // Only 1 attempt for quick test
        )
        .await;

        // Should return false due to network error/timeout
        assert!(!result);
    }
}
