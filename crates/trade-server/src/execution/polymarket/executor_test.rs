//! Tests for PolymarketOrderExecutor.
//!
//! ## Integration Tests
//!
//! Integration tests are marked with `#[ignore]` and require Polymarket credentials.
//! To run them:
//!
//! ```bash
//! # Required environment variables
//! export POLYMARKET_PRIVATE_KEY="your_private_key"
//! export POLYMARKET_TEST_TOKEN_ID="token_id_to_test"  # A valid token/asset ID
//!
//! # Optional - API credentials will be auto-derived if not provided
//! export POLYMARKET_API_KEY="your_api_key"
//! export POLYMARKET_API_SECRET="your_api_secret"
//! export POLYMARKET_API_PASSPHRASE="your_passphrase"
//!
//! # Run integration tests
//! cargo test polymarket_integration --ignored -- --nocapture
//! ```

#[cfg(test)]
mod tests {
    use crate::execution::{
        events::{OrderSide, TimeInForce},
        polymarket::{config::PolymarketConfig, types::PendingOrder},
    };

    #[test]
    fn test_pending_order_creation() {
        let order = PendingOrder::new(
            "order_123".to_string(),
            "token_abc".to_string(),
            Some("market_xyz".to_string()),
            OrderSide::Buy,
            0.65,
            100.0,
            TimeInForce::GoodTilCancelled,
            Some("signal_1".to_string()),
        );

        assert_eq!(order.order_id, "order_123");
        assert_eq!(order.mint, "token_abc");
        assert_eq!(order.price, 0.65);
        assert_eq!(order.original_size, 100.0);
        assert_eq!(order.remaining_size, 100.0);
        assert_eq!(order.last_known_filled, 0.0);
        assert!(!order.is_filled());
    }

    #[test]
    fn test_pending_order_update_filled() {
        let mut order = PendingOrder::new(
            "order_123".to_string(),
            "token_abc".to_string(),
            None,
            OrderSide::Buy,
            0.65,
            100.0,
            TimeInForce::GoodTilCancelled,
            None,
        );

        // First fill: 30 shares
        let new_fill = order.update_filled(30.0);
        assert_eq!(new_fill, 30.0);
        assert_eq!(order.last_known_filled, 30.0);
        assert_eq!(order.remaining_size, 70.0);
        assert!(!order.is_filled());

        // Second fill: 50 more shares (total 80)
        let new_fill = order.update_filled(80.0);
        assert_eq!(new_fill, 50.0);
        assert_eq!(order.last_known_filled, 80.0);
        assert_eq!(order.remaining_size, 20.0);
        assert!(!order.is_filled());

        // Final fill: complete the order
        let new_fill = order.update_filled(100.0);
        assert_eq!(new_fill, 20.0);
        assert_eq!(order.last_known_filled, 100.0);
        assert_eq!(order.remaining_size, 0.0);
        assert!(order.is_filled());
    }

    #[test]
    fn test_pending_order_no_double_count() {
        let mut order = PendingOrder::new(
            "order_123".to_string(),
            "token_abc".to_string(),
            None,
            OrderSide::Sell,
            0.70,
            50.0,
            TimeInForce::GoodTilCancelled,
            None,
        );

        // Fill 20
        let _ = order.update_filled(20.0);

        // Same fill amount (duplicate update) - should return 0
        let new_fill = order.update_filled(20.0);
        assert_eq!(new_fill, 0.0);
        assert_eq!(order.remaining_size, 30.0);
    }

    #[test]
    fn test_config_defaults() {
        let config = PolymarketConfig::default();

        assert_eq!(config.host, "https://clob.polymarket.com");
        assert_eq!(config.chain_id, 137);
    }

    #[test]
    fn test_order_side_conversion() {
        // This tests the internal conversion functions indirectly
        // through type usage
        let buy_side = OrderSide::Buy;
        let sell_side = OrderSide::Sell;

        assert_eq!(buy_side, OrderSide::Buy);
        assert_eq!(sell_side, OrderSide::Sell);
    }

    #[test]
    fn test_time_in_force_values() {
        let gtc = TimeInForce::GoodTilCancelled;
        let fok = TimeInForce::FillOrKill;
        let ioc = TimeInForce::ImmediateOrCancel;

        assert_eq!(gtc, TimeInForce::GoodTilCancelled);
        assert_eq!(fok, TimeInForce::FillOrKill);
        assert_eq!(ioc, TimeInForce::ImmediateOrCancel);
    }
}

/// Integration tests that require real Polymarket API credentials.
/// Run with: `cargo test polymarket_integration --ignored -- --nocapture`
#[cfg(test)]
mod integration_tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use polyfill_rs::ClobClient;

    use crate::{
        domain::SystemEvent,
        execution::{
            Order, OrderExecutor,
            events::{LimitOrderEvent, TimeInForce},
            polymarket::builder::PolymarketOrderExecutorBuilder,
        },
    };

    /// Helper struct for test credentials
    struct TestCredentials {
        host: String,
        private_key: String,
        api_key: Option<String>,
        api_secret: Option<String>,
        api_passphrase: Option<String>,
        signature_type: Option<u8>,
        funder: Option<String>,
        token_id: String,
    }

    /// Helper to get test credentials from environment variables.
    /// Only POLYMARKET_PRIVATE_KEY and POLYMARKET_TEST_TOKEN_ID are required.
    /// API credentials are optional - they will be auto-derived if not provided.
    /// For proxy wallets, set POLYMARKET_SIGNATURE_TYPE (1 or 2) and POLYMARKET_FUNDER.
    fn get_test_credentials() -> Option<TestCredentials> {
        Some(TestCredentials {
            host: std::env::var("POLYMARKET_HOST")
                .unwrap_or_else(|_| "https://clob.polymarket.com".to_string()),
            private_key: std::env::var("POLYMARKET_PRIVATE_KEY").ok()?,
            api_key: std::env::var("POLYMARKET_API_KEY").ok(),
            api_secret: std::env::var("POLYMARKET_API_SECRET").ok(),
            api_passphrase: std::env::var("POLYMARKET_API_PASSPHRASE").ok(),
            signature_type: std::env::var("POLYMARKET_SIGNATURE_TYPE")
                .ok()
                .and_then(|s| s.parse().ok()),
            funder: std::env::var("POLYMARKET_FUNDER").ok(),
            token_id: std::env::var("POLYMARKET_TEST_TOKEN_ID").ok()?,
        })
    }

    /// Simple event coordinator for testing that just stores event count
    struct TestEventCoordinator {
        event_count: Arc<Mutex<usize>>,
        events: Arc<Mutex<VecDeque<String>>>, // Store event type names
    }

    impl TestEventCoordinator {
        fn new() -> Self {
            Self {
                event_count: Arc::new(Mutex::new(0)),
                events: Arc::new(Mutex::new(VecDeque::new())),
            }
        }

        fn get_event_count(&self) -> usize {
            *self.event_count.lock().unwrap()
        }

        fn get_event_types(&self) -> Vec<String> {
            self.events.lock().unwrap().iter().cloned().collect()
        }
    }

    #[async_trait::async_trait]
    impl crate::event_coordinator::EventCoordinator for TestEventCoordinator {
        async fn enqueue_event(&self, event: SystemEvent) -> anyhow::Result<()> {
            *self.event_count.lock().unwrap() += 1;
            self.events
                .lock()
                .unwrap()
                .push_back(event.event_type().to_string());
            Ok(())
        }

        async fn next_event(&self) -> anyhow::Result<SystemEvent> {
            Err(anyhow::anyhow!("No events"))
        }
    }

    /// Helper to build executor from credentials
    async fn build_executor(
        creds: &TestCredentials,
        event_coordinator: Arc<TestEventCoordinator>,
    ) -> crate::execution::polymarket::executor::PolymarketOrderExecutor {
        println!("  Host: {}", creds.host);
        println!("  Private key length: {} chars", creds.private_key.len());
        println!(
            "  Private key prefix: {}...",
            &creds.private_key[..std::cmp::min(10, creds.private_key.len())]
        );
        println!("  API credentials provided: {}", creds.api_key.is_some());
        println!(
            "  Proxy wallet: signature_type={:?}, funder={:?}",
            creds.signature_type, creds.funder
        );

        let mut builder = PolymarketOrderExecutorBuilder::new()
            .host(&creds.host)
            .private_key(&creds.private_key)
            .event_coordinator(event_coordinator);

        // Only set API credentials if all three are provided
        if let (Some(key), Some(secret), Some(pass)) =
            (&creds.api_key, &creds.api_secret, &creds.api_passphrase)
        {
            println!("  Using provided API credentials");
            builder = builder.api_credentials(key, secret, pass);
        } else {
            println!("  Will auto-derive API credentials from private key");
        }

        // Set proxy wallet configuration if provided
        if let (Some(sig_type), Some(funder)) = (creds.signature_type, &creds.funder) {
            println!(
                "  Using proxy wallet mode: signature_type={}, funder={}",
                sig_type, funder
            );
            builder = builder.proxy_wallet(sig_type, funder);
        }

        match builder.build().await {
            Ok(executor) => executor,
            Err(e) => {
                eprintln!("Build failed: {:?}", e);
                panic!("Failed to build executor: {}", e);
            }
        }
    }

    /// Test to inspect what get_balance returns from Polymarket API.
    ///
    /// Run with:
    /// POLYMARKET_PRIVATE_KEY=0x... POLYMARKET_FUNDER=0x... POLYMARKET_SIGNATURE_TYPE=2 \
    ///   cargo test test_get_balance_response -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Requires Polymarket API credentials"]
    async fn test_get_balance_response() {
        use polyfill_rs::{AssetType, BalanceAllowanceParams, ClobClient};

        println!("\n=== Test Get Balance Response ===\n");

        let private_key =
            std::env::var("POLYMARKET_PRIVATE_KEY").expect("Set POLYMARKET_PRIVATE_KEY");
        let host = std::env::var("POLYMARKET_HOST")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());
        let funder = std::env::var("POLYMARKET_FUNDER").ok();
        let sig_type: Option<u8> = std::env::var("POLYMARKET_SIGNATURE_TYPE")
            .ok()
            .and_then(|s| s.parse().ok());
        let token_id = std::env::var("POLYMARKET_TEST_TOKEN_ID").ok();

        println!("Host: {}", host);
        println!("Funder: {:?}", funder);
        println!("Signature Type: {:?}", sig_type);
        println!("Token ID: {:?}", token_id);

        // Create client and derive API credentials
        let temp_client = ClobClient::with_l1_headers(&host, &private_key, 137);
        let api_creds = temp_client
            .derive_api_key(None)
            .await
            .expect("Failed to derive API key");
        println!(
            "API Key: {}...",
            &api_creds.api_key[..std::cmp::min(8, api_creds.api_key.len())]
        );

        // Create client with proxy wallet if configured
        let client = if let (Some(sig), Some(fund)) = (sig_type, &funder) {
            println!(
                "\nUsing proxy wallet mode (sig_type={}, funder={})",
                sig, fund
            );
            ClobClient::with_proxy_wallet(&host, &private_key, 137, api_creds, sig, fund)
        } else {
            println!("\nUsing EOA mode");
            ClobClient::with_l2_headers(&host, &private_key, 137, api_creds)
        };

        // Test 1: Get USDC balance (no params)
        println!("\n--- Test 1: Get USDC Balance (no params) ---");
        match client.get_balance_allowance(None).await {
            Ok(response) => {
                println!(
                    "Raw response:\n{}",
                    serde_json::to_string_pretty(&response).unwrap()
                );
            }
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }

        // Test 2: Get USDC balance with explicit COLLATERAL type
        println!("\n--- Test 2: Get USDC Balance (COLLATERAL type) ---");
        let params = BalanceAllowanceParams {
            asset_type: Some(AssetType::COLLATERAL),
            token_id: None,
            signature_type: sig_type,
        };
        match client.get_balance_allowance(Some(params)).await {
            Ok(response) => {
                println!(
                    "Raw response:\n{}",
                    serde_json::to_string_pretty(&response).unwrap()
                );
            }
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }

        // Test 3: Get conditional token balance (if token_id provided)
        if let Some(tid) = &token_id {
            println!("\n--- Test 3: Get Conditional Token Balance ---");
            println!("Token ID: {}", tid);
            let params = BalanceAllowanceParams {
                asset_type: Some(AssetType::CONDITIONAL),
                token_id: Some(tid.clone()),
                signature_type: sig_type,
            };
            match client.get_balance_allowance(Some(params)).await {
                Ok(response) => {
                    println!(
                        "Raw response:\n{}",
                        serde_json::to_string_pretty(&response).unwrap()
                    );
                }
                Err(e) => {
                    println!("Error: {:?}", e);
                }
            }
        }

        println!("\n=== Test Complete ===");
    }

    /// Test to get all user positions from Polymarket Data API.
    ///
    /// Run with:
    /// POLYMARKET_FUNDER=0x... cargo test test_get_all_positions -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Requires Polymarket credentials"]
    async fn test_get_all_positions() {
        println!("\n=== Test Get All Positions ===\n");

        let funder = std::env::var("POLYMARKET_FUNDER")
            .expect("Set POLYMARKET_FUNDER (your proxy wallet address)");

        println!("Funder/Proxy Wallet: {}", funder);

        // Data API endpoint for user positions
        let url = format!("https://data-api.polymarket.com/positions?user={}", funder);
        println!("URL: {}", url);

        let client = reqwest::Client::new();
        match client.get(&url).send().await {
            Ok(response) => {
                let status = response.status();
                println!("Status: {}", status);

                match response.text().await {
                    Ok(body) => {
                        // Try to pretty print as JSON
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                            println!(
                                "Response:\n{}",
                                serde_json::to_string_pretty(&json).unwrap()
                            );
                        } else {
                            println!("Response (raw):\n{}", body);
                        }
                    }
                    Err(e) => println!("Failed to read body: {:?}", e),
                }
            }
            Err(e) => println!("Request failed: {:?}", e),
        }

        // Also try the /value endpoint
        println!("\n--- Holdings Value ---");
        let value_url = format!("https://data-api.polymarket.com/value?user={}", funder);
        println!("URL: {}", value_url);

        match client.get(&value_url).send().await {
            Ok(response) => {
                let status = response.status();
                println!("Status: {}", status);

                match response.text().await {
                    Ok(body) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                            println!(
                                "Response:\n{}",
                                serde_json::to_string_pretty(&json).unwrap()
                            );
                        } else {
                            println!("Response (raw):\n{}", body);
                        }
                    }
                    Err(e) => println!("Failed to read body: {:?}", e),
                }
            }
            Err(e) => println!("Request failed: {:?}", e),
        }

        println!("\n=== Test Complete ===");
    }

    #[tokio::test]
    #[ignore = "Requires Polymarket API credentials"]
    async fn test_query_historical_trades() {
        let private_key =
            std::env::var("POLYMARKET_PRIVATE_KEY").expect("Set POLYMARKET_PRIVATE_KEY");
        let host = std::env::var("POLYMARKET_HOST")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());

        println!("=== Query Historical Trades ===");
        println!("Host: {}", host);

        // Create client and derive API credentials
        let mut client = ClobClient::with_l1_headers(&host, &private_key, 137);

        println!("Deriving API credentials...");
        let creds = client
            .derive_api_key(None)
            .await
            .expect("Failed to derive API key");
        println!(
            "API Key: {}...",
            &creds.api_key[..std::cmp::min(8, creds.api_key.len())]
        );
        client.set_api_creds(creds);

        // Query historical trades (no filter = all trades for this account)
        println!("\nQuerying historical trades...");
        match client.get_trades(None, None).await {
            Ok(trades) => {
                println!("Found {} trades", trades.len());
                for (i, trade) in trades.iter().take(5).enumerate() {
                    println!("\nTrade {}:", i + 1);
                    println!(
                        "  {}",
                        serde_json::to_string_pretty(trade).unwrap_or_default()
                    );
                }
                if trades.len() > 5 {
                    println!("\n... and {} more trades", trades.len() - 5);
                }
            }
            Err(e) => {
                println!("Failed to query trades: {:?}", e);
            }
        }

        // Also query open orders
        println!("\nQuerying open orders...");
        match client.get_orders(None, None).await {
            Ok(orders) => {
                println!("Found {} open orders", orders.len());
                for (i, order) in orders.iter().take(5).enumerate() {
                    println!("\nOrder {}:", i + 1);
                    println!(
                        "  {}",
                        serde_json::to_string_pretty(order).unwrap_or_default()
                    );
                }
            }
            Err(e) => {
                println!("Failed to query orders: {:?}", e);
            }
        }

        println!("\n=== Query Test Complete ===");
    }

    #[tokio::test]
    #[ignore = "Requires Polymarket API credentials"]
    async fn test_execute_limit_order_and_cancel() {
        let creds = get_test_credentials()
            .expect("Set POLYMARKET_PRIVATE_KEY and POLYMARKET_TEST_TOKEN_ID");

        let event_coordinator = Arc::new(TestEventCoordinator::new());
        let executor = build_executor(&creds, event_coordinator.clone()).await;

        // Place a limit order at a price unlikely to fill (very low bid)
        let order = Order::new_limit_buy(
            creds.token_id.clone(),
            None,
            0.1,  // 0.1 USDC
            0.01, // Very low price: $0.01 (unlikely to fill) -> 10 shares
            TimeInForce::GoodTilCancelled,
            chrono::Utc::now(),
            Some("test_signal".to_string()),
            None,
        );

        println!("Placing limit order: {:?}", order);

        // Execute limit order
        let result = executor.execute_limit_order(order).await;
        println!("Execute result: {:?}", result);

        let place_event = result.expect("Failed to place limit order");

        let order_id = match &place_event {
            LimitOrderEvent::OrderPlaced {
                order_id,
                price,
                size,
                ..
            } => {
                println!(
                    "Order placed: id={}, price={}, size={}",
                    order_id, price, size
                );
                order_id.clone()
            }
            other => panic!("Expected OrderPlaced, got {:?}", other),
        };

        // Verify order is in pending orders
        let pending_count = executor.pending_order_count().await;
        println!("Pending orders count: {}", pending_count);
        assert!(pending_count > 0, "Order should be in pending orders");

        // Cancel the order
        println!("Cancelling order: {}", order_id);
        let cancel_result = executor.cancel_order(&order_id).await;
        println!("Cancel result: {:?}", cancel_result);

        let cancel_event = cancel_result.expect("Failed to cancel order");

        match cancel_event {
            LimitOrderEvent::OrderCancelled {
                order_id: cancelled_id,
                ..
            } => {
                assert_eq!(cancelled_id, order_id);
                println!("Order cancelled successfully");
            }
            other => panic!("Expected OrderCancelled, got {:?}", other),
        }

        // Verify order is removed from pending orders
        let pending_after = executor.pending_order_count().await;
        println!("Pending orders after cancel: {}", pending_after);
        assert_eq!(
            pending_after,
            pending_count - 1,
            "Order should be removed from pending"
        );

        // Verify events were enqueued to coordinator
        let event_count = event_coordinator.get_event_count();
        let event_types = event_coordinator.get_event_types();
        println!("Events captured: {} {:?}", event_count, event_types);
        assert_eq!(
            event_count, 2,
            "Should have OrderPlaced and OrderCancelled events"
        );
        assert!(
            event_types.contains(&"LimitOrder".to_string()),
            "Should have LimitOrder events"
        );
    }

    #[tokio::test]
    #[ignore = "Requires Polymarket API credentials"]
    async fn test_websocket_connection() {
        // Initialize tracing for debug logs
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();

        let creds = get_test_credentials()
            .expect("Set POLYMARKET_PRIVATE_KEY and POLYMARKET_TEST_TOKEN_ID");

        let event_coordinator = Arc::new(TestEventCoordinator::new());
        let executor = build_executor(&creds, event_coordinator.clone()).await;

        // This test uses polling fallback via poll_pending_orders()

        // Place a limit order
        let order = Order::new_limit_buy(
            creds.token_id.clone(),
            None,
            0.1,  // 0.1 USDC
            0.01, // Very low price: $0.01 -> 10 shares
            TimeInForce::GoodTilCancelled,
            chrono::Utc::now(),
            Some("ws_test_signal".to_string()),
            None,
        );

        let place_result = executor.execute_limit_order(order).await;
        let order_id = match place_result.expect("Failed to place order") {
            LimitOrderEvent::OrderPlaced { order_id, .. } => order_id,
            other => panic!("Expected OrderPlaced, got {:?}", other),
        };
        println!("Order placed: {}", order_id);

        // Wait a bit for WebSocket to receive any updates
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Cancel the order - this should trigger a WebSocket CANCELLATION message
        // (though our executor removes from pending first, so WebSocket will ignore)
        executor
            .cancel_order(&order_id)
            .await
            .expect("Failed to cancel");
        println!("Order cancelled: {}", order_id);

        // Wait for potential WebSocket events
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Check captured events
        let event_count = event_coordinator.get_event_count();
        let event_types = event_coordinator.get_event_types();
        println!("Captured {} events: {:?}", event_count, event_types);
    }

    #[tokio::test]
    #[ignore = "Requires Polymarket API credentials"]
    async fn test_poll_pending_orders() {
        let creds = get_test_credentials()
            .expect("Set POLYMARKET_PRIVATE_KEY and POLYMARKET_TEST_TOKEN_ID");

        let event_coordinator = Arc::new(TestEventCoordinator::new());
        let executor = build_executor(&creds, event_coordinator).await;

        // Place a limit order
        let order = Order::new_limit_buy(
            creds.token_id.clone(),
            None,
            1.0,
            0.01,
            TimeInForce::GoodTilCancelled,
            chrono::Utc::now(),
            Some("poll_test_signal".to_string()),
            None,
        );

        let place_result = executor.execute_limit_order(order).await;
        let order_id = match place_result.expect("Failed to place order") {
            LimitOrderEvent::OrderPlaced { order_id, .. } => order_id,
            other => panic!("Expected OrderPlaced, got {:?}", other),
        };
        println!("Order placed: {}", order_id);

        // Verify pending orders has the order
        let pending_count = executor.pending_order_count().await;
        println!("Pending orders count: {}", pending_count);
        assert!(pending_count > 0, "Should have at least 1 pending order");

        // Get the pending order details
        let pending_order = executor.get_pending_order(&order_id).await;
        assert!(
            pending_order.is_some(),
            "Should find the pending order by ID"
        );
        let pending = pending_order.unwrap();
        println!("Pending order details:");
        println!("  order_id: {}", pending.order_id);
        println!("  mint: {}", pending.mint);
        println!("  price: {}", pending.price);
        println!("  original_size: {}", pending.original_size);
        println!("  remaining_size: {}", pending.remaining_size);
        println!("  last_known_filled: {}", pending.last_known_filled);

        // Wait a bit to verify order tracking
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Verify pending order still exists (no fills at low price)
        let final_pending = executor.get_pending_order(&order_id).await;
        assert!(
            final_pending.is_some(),
            "Order should still be pending (no fills at $0.01)"
        );

        // Clean up
        executor
            .cancel_order(&order_id)
            .await
            .expect("Failed to cancel");
        println!("\nOrder cancelled");

        // Verify order removed from pending
        let after_cancel = executor.pending_order_count().await;
        println!("Pending after cancel: {}", after_cancel);
        assert_eq!(
            after_cancel, 0,
            "Should have no pending orders after cancel"
        );
    }

    /// Test WebSocket fill detection by placing an order at or near the best price.
    ///
    /// This test:
    /// 1. Fetches the current best bid and best ask for the test token
    /// 2. Places a limit buy order at the best ask price (likely to fill immediately)
    /// 3. Uses WebSocket to detect the fill event
    ///
    /// WARNING: This test may result in actual trades if the order fills!
    /// Only run with tokens you're willing to trade and small amounts.
    ///
    /// Run with: cargo test test_websocket_fill_detection -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "May result in actual trades - requires careful review"]
    async fn test_websocket_fill_detection() {
        use polyfill_rs::Side;
        use rust_decimal::prelude::ToPrimitive;

        // Initialize tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();

        let creds = get_test_credentials()
            .expect("Set POLYMARKET_PRIVATE_KEY and POLYMARKET_TEST_TOKEN_ID");

        let event_coordinator = Arc::new(TestEventCoordinator::new());
        let executor = build_executor(&creds, event_coordinator.clone()).await;

        // Fetch best bid and best ask
        println!("\n=== Fetching Market Prices ===");
        let best_bid = executor
            .client()
            .get_price(&creds.token_id, Side::BUY)
            .await
            .expect("Failed to get best bid");
        let best_ask = executor
            .client()
            .get_price(&creds.token_id, Side::SELL)
            .await
            .expect("Failed to get best ask");

        println!("Best Bid: ${}", best_bid.price);
        println!("Best Ask: ${}", best_ask.price);
        println!("Spread: ${}", best_ask.price - best_bid.price);

        // Validate market has liquidity
        if best_ask.price <= rust_decimal::Decimal::ZERO {
            println!("No ask liquidity, skipping test");
            return;
        }

        // Calculate order price: at best ask (will likely fill immediately as taker)
        // Or slightly below best ask to be a maker order that might fill
        let order_price = best_ask.price;
        // Use $1.00 order size (minimum for marketable orders)
        let order_size_usdc = rust_decimal::Decimal::new(100, 2); // $1.00
        let order_size_shares = (order_size_usdc / order_price).round_dp(0); // Round to whole shares

        println!("\n=== Order Parameters ===");
        println!("Order Type: Limit BUY");
        println!("Price: ${} (at best ask - taker)", order_price);
        println!(
            "Size: {} shares (~${} cost)",
            order_size_shares,
            order_size_shares * order_price
        );

        // Note: WebSocket fill detection is managed externally (see examples/poly-mm/)
        // This test monitors the event_coordinator for fill events
        // In production, an external WebSocket would push events to the coordinator

        // Place the order
        println!("\n=== Placing Order ===");
        // Note: Order size is in USDC value, not shares
        let order_usdc_value = (order_size_shares * order_price).to_f64().unwrap_or(5.0);
        let order = Order::new_limit_buy(
            creds.token_id.clone(),
            None,
            order_usdc_value,
            order_price.to_f64().unwrap_or(0.5),
            TimeInForce::GoodTilCancelled, // GTC so we can observe fills
            chrono::Utc::now(),
            Some("fill_test".to_string()),
            None,
        );
        println!("Order USDC value: ${}", order_usdc_value);

        let place_result = executor.execute_limit_order(order).await;
        let order_id = match place_result {
            Ok(LimitOrderEvent::OrderPlaced {
                order_id,
                price,
                size,
                ..
            }) => {
                println!("Order placed: id={}", order_id);
                println!("  price: {}, size: {}", price, size);
                order_id
            }
            Ok(other) => {
                println!("Unexpected event: {:?}", other);
                return;
            }
            Err(e) => {
                println!("Failed to place order: {:?}", e);
                return;
            }
        };

        // Wait for potential WebSocket fill events
        println!("\n=== Waiting for Fill Events ===");
        println!("Monitoring for 10 seconds...");

        let start = std::time::Instant::now();
        let mut last_event_count = event_coordinator.get_event_count();

        while start.elapsed() < Duration::from_secs(10) {
            let current_count = event_coordinator.get_event_count();
            let pending = executor.pending_order_count().await;

            if current_count > last_event_count {
                println!(
                    "\n[{:.1}s] New event detected!",
                    start.elapsed().as_secs_f32()
                );
                let event_types = event_coordinator.get_event_types();
                println!("  Events so far: {:?}", event_types);
                println!("  Pending orders: {}", pending);
                last_event_count = current_count;
            }

            // Check if order is completely filled (removed from pending)
            if pending == 0 && current_count > 1 {
                println!("\nOrder appears to be completely filled!");
                break;
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Final status
        println!("\n=== Final Status ===");
        let final_event_count = event_coordinator.get_event_count();
        let final_event_types = event_coordinator.get_event_types();
        let final_pending = executor.pending_order_count().await;

        println!("Total events captured: {}", final_event_count);
        println!("Event types: {:?}", final_event_types);
        println!("Remaining pending orders: {}", final_pending);

        // Check pending order state
        if let Some(pending_order) = executor.get_pending_order(&order_id).await {
            println!("\nPending order state:");
            println!("  original_size: {}", pending_order.original_size);
            println!("  remaining_size: {}", pending_order.remaining_size);
            println!("  last_known_filled: {}", pending_order.last_known_filled);
        }

        // Clean up: cancel if still pending
        if final_pending > 0 {
            println!("\nCleaning up - cancelling remaining order...");
            match executor.cancel_order(&order_id).await {
                Ok(_) => println!("Order cancelled"),
                Err(e) => println!("Failed to cancel (may already be filled): {:?}", e),
            }
        }

        // Report result
        if final_event_count > 1 {
            println!("\n✅ SUCCESS: WebSocket detected fill event(s)!");
        } else if final_pending == 0 {
            println!("\n⚠️  Order may have filled but WebSocket event was processed differently");
        } else {
            println!("\n⚠️  No fill detected - order may not have matched");
        }
    }

    /// Interactive test: places an order and waits for external cancellation.
    /// Use this to test WebSocket receiving CANCELLATION events from:
    /// - Manual cancellation via Polymarket UI
    /// - Order TTL expiration
    /// - Other external cancellation sources
    ///
    /// Run with: cargo test test_websocket_external_cancel -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Interactive test - requires manual cancellation"]
    async fn test_websocket_external_cancel() {
        // Initialize tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();

        let creds = get_test_credentials()
            .expect("Set POLYMARKET_PRIVATE_KEY and POLYMARKET_TEST_TOKEN_ID");

        let event_coordinator = Arc::new(TestEventCoordinator::new());
        let executor = build_executor(&creds, event_coordinator.clone()).await;

        // Note: WebSocket fill detection is managed externally (see examples/poly-mm/)
        // This test monitors the event_coordinator for cancellation events
        // In production, an external WebSocket would push events to the coordinator

        // Place a limit order at a price unlikely to fill
        let order = Order::new_limit_buy(
            creds.token_id.clone(),
            None,
            0.1,  // 0.1 USDC
            0.01, // Very low price
            TimeInForce::GoodTilCancelled,
            chrono::Utc::now(),
            Some("external_cancel_test".to_string()),
            None,
        );

        println!("\n=== Placing order ===");
        let place_result = executor.execute_limit_order(order).await;
        let order_id = match place_result.expect("Failed to place order") {
            LimitOrderEvent::OrderPlaced { order_id, .. } => order_id,
            other => panic!("Expected OrderPlaced, got {:?}", other),
        };
        println!("Order placed: {}", order_id);
        println!("Pending orders: {}", executor.pending_order_count().await);

        // Wait for external cancellation
        println!("\n========================================");
        println!("ORDER IS LIVE - WAITING FOR EXTERNAL CANCEL");
        println!("========================================");
        println!("Order ID: {}", order_id);
        println!("\nPlease cancel this order manually via:");
        println!("  - Polymarket UI");
        println!("  - Another API client");
        println!("  - Or wait for TTL expiration");
        println!("\nWaiting up to 120 seconds for WebSocket event...\n");

        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(120);

        // Poll for events
        while start.elapsed() < timeout {
            let event_count = event_coordinator.get_event_count();
            let pending = executor.pending_order_count().await;

            // Check if we received a cancellation event (more than the initial OrderPlaced)
            if event_count > 1 {
                println!("\n=== EVENT RECEIVED ===");
                let event_types = event_coordinator.get_event_types();
                println!("Events: {:?}", event_types);
                println!("Pending orders: {}", pending);
                break;
            }

            // Print status every 5 seconds
            if start.elapsed().as_secs() % 5 == 0 {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Final status
        println!("\n\n=== FINAL STATUS ===");
        let event_count = event_coordinator.get_event_count();
        let event_types = event_coordinator.get_event_types();
        let pending = executor.pending_order_count().await;

        println!("Total events captured: {}", event_count);
        println!("Event types: {:?}", event_types);
        println!("Remaining pending orders: {}", pending);

        // WebSocket is stopped when _ws_shutdown is dropped (end of function)
        println!("WebSocket will stop on function exit");

        // If order is still pending, cancel it for cleanup
        if pending > 0 {
            println!("\nCleaning up - cancelling remaining order...");
            let _ = executor.cancel_order(&order_id).await;
        }

        // Assert we received the cancellation event
        if event_count > 1 {
            println!("\n✅ SUCCESS: WebSocket received external cancellation event!");
        } else {
            println!("\n⚠️  No external cancellation received within timeout");
        }
    }

    /// Test querying market info and neg_risk for a binary market.
    ///
    /// This test demonstrates how to:
    /// 1. Fetch market info from Gamma API by slug (like spread_arbitrage)
    /// 2. Query neg_risk from CLOB API using token_id
    ///
    /// Run with: cargo test test_query_market_info_and_neg_risk -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Requires network access to Polymarket APIs"]
    async fn test_query_market_info_and_neg_risk() {
        use polyfill_rs::ClobClient;
        use polyfill_rs::ws_orderbook::{fetch_market_info, get_current_interval};

        println!("\n=== Query Market Info and Neg Risk ===\n");

        // Get market slug from env or use default
        let slug_prefix =
            std::env::var("MARKET_SLUG_PREFIX").unwrap_or_else(|_| "btc-updown-15m".to_string());
        let interval = get_current_interval();
        let slug = format!("{}-{}", slug_prefix, interval);

        println!("Market slug: {}", slug);
        println!("Current interval: {}", interval);

        // Step 1: Fetch market info from Gamma API
        println!("\n--- Step 1: Fetch Market Info from Gamma API ---");
        let market_info = match fetch_market_info(&slug).await {
            Ok(info) => {
                println!("✓ Market info fetched successfully");
                println!("  Title: {}", info.title);
                println!("  Condition ID: {}", info.condition_id);
                println!("  Neg Risk (from Gamma): {}", info.neg_risk);
                println!("  Up Token: {}", info.up_token);
                println!("  Down Token: {}", info.down_token);
                info
            }
            Err(e) => {
                println!("✗ Failed to fetch market info: {:?}", e);
                println!("  This may happen if the market doesn't exist for the current interval.");
                println!("  Try setting MARKET_SLUG_PREFIX to a valid market prefix.");
                return;
            }
        };

        // Step 2: Query neg_risk from CLOB API
        println!("\n--- Step 2: Query Neg Risk from CLOB API ---");

        let private_key = match std::env::var("POLYMARKET_PRIVATE_KEY") {
            Ok(key) => key,
            Err(_) => {
                println!("⚠️ POLYMARKET_PRIVATE_KEY not set, skipping CLOB API query");
                println!("  Set it to query neg_risk via CLOB API");
                return;
            }
        };

        let host = std::env::var("POLYMARKET_HOST")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());

        // Create client (L1 headers are sufficient for reading market data)
        let client = ClobClient::with_l1_headers(&host, &private_key, 137);

        // Query neg_risk for up token
        println!("\nQuerying neg_risk for Up Token...");
        match client.get_neg_risk(&market_info.up_token).await {
            Ok(neg_risk) => {
                println!("✓ Up Token neg_risk (from CLOB): {}", neg_risk);
                println!("  Matches Gamma API: {}", neg_risk == market_info.neg_risk);
            }
            Err(e) => {
                println!("✗ Failed to query neg_risk for up token: {:?}", e);
            }
        }

        // Query neg_risk for down token
        println!("\nQuerying neg_risk for Down Token...");
        match client.get_neg_risk(&market_info.down_token).await {
            Ok(neg_risk) => {
                println!("✓ Down Token neg_risk (from CLOB): {}", neg_risk);
                println!("  Matches Gamma API: {}", neg_risk == market_info.neg_risk);
            }
            Err(e) => {
                println!("✗ Failed to query neg_risk for down token: {:?}", e);
            }
        }

        println!("\n=== Query Test Complete ===");
    }

    /// Interactive test: monitor arbitrary order IDs and observe poller behavior.
    ///
    /// This test allows you to input order IDs from the command line and watch
    /// the poller query order status and emit events.
    ///
    /// Usage:
    /// 1. Run the test with: RUST_LOG=info cargo test test_interactive_order_monitor -- --ignored --nocapture
    /// 2. Enter order IDs one per line (from Polymarket UI or API)
    /// 3. Watch logs to see:
    ///    - "Registering order for monitoring" when order is added
    ///    - "Starting poll cycle" when poller runs
    ///    - "Fill detected" if order has any fills
    ///    - "Order not found" if order doesn't exist
    /// 4. Press Ctrl+C or type "quit" to exit
    ///
    /// Run with:
    /// RUST_LOG=info POLYMARKET_PRIVATE_KEY=0x... POLYMARKET_FUNDER=0x... POLYMARKET_SIGNATURE_TYPE=2 \
    ///   cargo test test_interactive_order_monitor -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Interactive test requiring user input"]
    async fn test_interactive_order_monitor() {
        use polyfill_rs::Side;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::sync::watch;

        use crate::execution::events::OrderSide;
        use crate::execution::polymarket::poller::{
            MonitoredOrder, OrderStatusPoller, PollerConfig,
        };

        // Initialize tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env().add_directive(
                    "trade_server::execution::polymarket::poller=debug"
                        .parse()
                        .unwrap(),
                ),
            )
            .with_test_writer()
            .try_init();

        println!("\n=== Interactive Order Monitor Test ===\n");

        // Get credentials
        let private_key =
            std::env::var("POLYMARKET_PRIVATE_KEY").expect("Set POLYMARKET_PRIVATE_KEY");
        let host = std::env::var("POLYMARKET_HOST")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());
        let funder = std::env::var("POLYMARKET_FUNDER").ok();
        let sig_type: Option<u8> = std::env::var("POLYMARKET_SIGNATURE_TYPE")
            .ok()
            .and_then(|s| s.parse().ok());

        println!("Host: {}", host);
        println!("Funder: {:?}", funder);
        println!("Signature Type: {:?}", sig_type);

        // Create CLOB client with API credentials
        let temp_client = ClobClient::with_l1_headers(&host, &private_key, 137);
        let api_creds = temp_client
            .derive_api_key(None)
            .await
            .expect("Failed to derive API key");
        println!(
            "API Key derived: {}...",
            &api_creds.api_key[..std::cmp::min(8, api_creds.api_key.len())]
        );

        let client = if let (Some(sig), Some(fund)) = (sig_type, &funder) {
            println!("Using proxy wallet mode");
            ClobClient::with_proxy_wallet(&host, &private_key, 137, api_creds, sig, fund)
        } else {
            println!("Using EOA mode");
            ClobClient::with_l2_headers(&host, &private_key, 137, api_creds)
        };
        let client = Arc::new(client);

        // Create event coordinator
        let event_coordinator = Arc::new(TestEventCoordinator::new());

        // Create position manager for testing (use a simple mock)
        struct MockPositionManager;
        #[async_trait::async_trait]
        impl crate::position::PositionManager for MockPositionManager {
            async fn handle_execution(
                &self,
                _event: &crate::execution::ExecutionEvent,
            ) -> Result<crate::position::PositionEvent, crate::position::PositionError>
            {
                Err(crate::position::PositionError::PositionNotFound(
                    "mock".to_string(),
                ))
            }
            async fn get_position(&self, _mint: &str) -> Option<crate::position::Position> {
                None
            }
            async fn update_price(
                &self,
                _mint: &str,
                _price: f64,
                _timestamp: chrono::DateTime<chrono::Utc>,
            ) -> Result<crate::position::PositionEvent, crate::position::PositionError>
            {
                Err(crate::position::PositionError::PositionNotFound(
                    "mock".to_string(),
                ))
            }
            async fn handle_signal(
                &self,
                _signal: &dyn crate::signal::TradableSignal,
            ) -> Result<Option<crate::execution::Order>, crate::position::PositionError>
            {
                Ok(None)
            }
            async fn handle_timer(
                &self,
                _event: &crate::domain::TimerEvent,
            ) -> Result<Vec<crate::execution::Order>, crate::position::PositionError> {
                Ok(vec![])
            }
            async fn get_available_quote(&self) -> f64 {
                0.0
            }
            async fn get_total_quote_received(&self) -> f64 {
                0.0
            }
            async fn get_total_quote_spent(&self) -> f64 {
                0.0
            }
            async fn get_total_pnl(&self) -> f64 {
                0.0
            }
            async fn get_total_unrealized_pnl(&self) -> f64 {
                0.0
            }
            async fn get_total_closed_positions(&self) -> u32 {
                0
            }
            async fn get_winning_trades(&self) -> u32 {
                0
            }
            async fn get_open_position_count(&self) -> u32 {
                0
            }
            async fn try_mark_pending_sell(&self, _mint: &str) -> bool {
                false
            }
            async fn try_mark_for_buying(&self, _mint: &str) -> bool {
                false
            }
            async fn remove_position(
                &self,
                _mint: &str,
            ) -> Result<Option<crate::position::Position>, crate::position::PositionError>
            {
                Ok(None)
            }
            async fn get_all_open_positions(&self) -> Vec<crate::position::Position> {
                vec![]
            }
            async fn clear_pending_sell(
                &self,
                _mint: &str,
            ) -> Result<(), crate::position::PositionError> {
                Ok(())
            }
            async fn increment_sell_failure_count(
                &self,
                _mint: &str,
            ) -> Result<(), crate::position::PositionError> {
                Ok(())
            }
            async fn add_orphan_position(
                &self,
                _mint: String,
                _amount: u64,
            ) -> Result<(), crate::position::PositionError> {
                Ok(())
            }
            async fn get_all_bought_mints(&self) -> Vec<String> {
                vec![]
            }
        }
        let position_manager: Arc<dyn crate::position::PositionManager> =
            Arc::new(MockPositionManager);

        // Create poller with fast polling for testing
        let config = PollerConfig::new()
            .with_poll_interval(Duration::from_secs(2)) // Poll every 2 seconds
            .with_position_sync(false); // Disable position sync for this test

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let poller = Arc::new(OrderStatusPoller::new(
            client.clone(),
            event_coordinator.clone(),
            position_manager,
            config,
            shutdown_rx,
        ));

        println!("\n--- Poller created with 2s polling interval ---\n");

        // Spawn poller background task
        let poller_clone = poller.clone();
        let poller_handle = tokio::spawn(async move {
            if let Err(e) = poller_clone.run().await {
                eprintln!("Poller error: {:?}", e);
            }
        });

        // Spawn event watcher
        let event_coordinator_clone = event_coordinator.clone();
        let event_watcher = tokio::spawn(async move {
            let mut last_count = 0;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let count = event_coordinator_clone.get_event_count();
                if count > last_count {
                    let events = event_coordinator_clone.get_event_types();
                    println!("\n📬 New event(s) detected! Total: {}", count);
                    println!("   Event types: {:?}", events);
                    last_count = count;
                }
            }
        });

        // Print instructions
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                  Interactive Order Monitor                    ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Commands:                                                     ║");
        println!("║   <order_id>              - Monitor an order                  ║");
        println!("║   status                  - Show monitored orders count       ║");
        println!("║   list                    - List all monitored orders         ║");
        println!("║   remove <order_id>       - Stop monitoring an order          ║");
        println!("║   poll                    - Trigger immediate poll cycle      ║");
        println!("║   quit                    - Exit the test                     ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();

        // Use async stdin reader
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        loop {
            // Use select to handle both stdin and allow other tasks to run
            let input = tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(l)) => l.trim().to_string(),
                        Ok(None) => break, // EOF
                        Err(_) => break,
                    }
                }
            };

            if input.is_empty() {
                continue;
            }

            if input == "quit" || input == "exit" || input == "q" {
                println!("\nShutting down...");
                break;
            }

            if input == "status" {
                let count = poller.monitored_count().await;
                let event_count = event_coordinator.get_event_count();
                println!(
                    "📊 Monitored orders: {}, Events captured: {}",
                    count, event_count
                );
                continue;
            }

            if input == "list" {
                let orders = poller.get_monitored_orders().await;
                if orders.is_empty() {
                    println!("📋 No orders being monitored");
                } else {
                    println!("📋 Monitored orders:");
                    for order in orders {
                        println!(
                            "   - {} | {} | {:?} | filled: {}/{}",
                            order.order_id,
                            order.mint,
                            order.side,
                            order.last_known_filled,
                            order.original_size
                        );
                    }
                }
                continue;
            }

            if input == "poll" {
                println!("🔄 Triggering manual poll cycle...");
                match poller.poll_cycle().await {
                    Ok(result) => {
                        println!(
                            "   Polled: {}, Fills: {}, Completed: {}, Errors: {}",
                            result.orders_polled,
                            result.fills_detected,
                            result.orders_completed,
                            result.errors
                        );
                    }
                    Err(e) => {
                        println!("   Poll error: {:?}", e);
                    }
                }
                continue;
            }

            if input.starts_with("remove ") {
                let order_id = input.strip_prefix("remove ").unwrap().trim();
                if poller.unmonitor_order(order_id).await.is_some() {
                    println!("✅ Stopped monitoring order: {}", order_id);
                } else {
                    println!("⚠️  Order not found: {}", order_id);
                }
                continue;
            }

            // Assume it's an order ID to monitor
            let order_id = input.clone();

            // First, try to query the order to get its details
            println!("🔍 Querying order {}...", order_id);
            match client.get_order(&order_id).await {
                Ok(api_order) => {
                    println!("✅ Order found:");
                    println!("   Asset: {}", api_order.asset_id);
                    println!("   Side: {:?}", api_order.side);
                    println!("   Price: {}", api_order.price);
                    println!("   Size: {}", api_order.original_size);
                    println!("   Filled: {}", api_order.size_matched);
                    println!("   Status: {}", api_order.status);

                    // Create monitored order
                    let side = match api_order.side {
                        Side::BUY => OrderSide::Buy,
                        Side::SELL => OrderSide::Sell,
                    };
                    let monitored = MonitoredOrder::new(
                        order_id.clone(),
                        api_order.asset_id.clone(),
                        None,
                        side,
                        api_order.price.to_string().parse().unwrap_or(0.5),
                        api_order.original_size.to_string().parse().unwrap_or(0.0),
                        None,
                    );

                    poller.monitor_order(monitored).await;
                    println!("👁️  Now monitoring order: {}", order_id);
                }
                Err(e) => {
                    println!("❌ Failed to query order: {:?}", e);
                }
            }
        }

        // Shutdown
        let _ = shutdown_tx.send(true);
        event_watcher.abort();
        let _ = tokio::time::timeout(Duration::from_secs(2), poller_handle).await;

        // Final stats
        println!("\n=== Final Statistics ===");
        println!("Events captured: {}", event_coordinator.get_event_count());
        println!("Event types: {:?}", event_coordinator.get_event_types());
        println!("\n=== Test Complete ===");
    }

    /// Test execute_redemption by first splitting USDC then redeeming back.
    ///
    /// This test:
    /// 1. Splits $1 USDC into Up + Down tokens via SafeClient
    /// 2. Waits for split to complete
    /// 3. Calls execute_redemption to merge tokens back to USDC
    ///
    /// WARNING: This test performs real blockchain transactions!
    ///
    /// Required env vars:
    /// - POLYMARKET_PRIVATE_KEY: Signer private key (with 0x prefix)
    /// - POLYMARKET_SAFE_ADDRESS: Safe wallet address
    /// - POLYMARKET_FUNDER: Same as SAFE_ADDRESS for proxy wallet
    ///
    /// Run with:
    /// POLYMARKET_PRIVATE_KEY=0x... POLYMARKET_SAFE_ADDRESS=0x... POLYMARKET_FUNDER=0x... \
    ///   cargo test test_execute_redemption_flow -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Performs real blockchain transactions"]
    async fn test_execute_redemption_flow() {
        use crate::client::polymarket::{SafeClient, SafeClientConfig};
        use crate::execution::OrderExecutor;
        use crate::signal::redemption::RedemptionAction;
        use alloy_primitives::Address;
        use polyfill_rs::ws_orderbook::{fetch_market_info, get_current_interval};
        use polyfill_rs::{AssetType, BalanceAllowanceParams};

        // Initialize tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();

        println!("\n=== Test Execute Redemption Flow ===\n");

        // Get credentials
        let private_key =
            std::env::var("POLYMARKET_PRIVATE_KEY").expect("Set POLYMARKET_PRIVATE_KEY");
        let safe_address =
            std::env::var("POLYMARKET_SAFE_ADDRESS").expect("Set POLYMARKET_SAFE_ADDRESS");
        let funder = std::env::var("POLYMARKET_FUNDER").unwrap_or_else(|_| safe_address.clone());
        let host = std::env::var("POLYMARKET_HOST")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());

        println!("Safe Address: {}", safe_address);
        println!("Funder: {}", funder);

        // Step 1: Fetch market info
        println!("\n--- Step 1: Fetch Market Info ---");
        let slug_prefix =
            std::env::var("MARKET_SLUG_PREFIX").unwrap_or_else(|_| "btc-updown-15m".to_string());
        let interval = get_current_interval();
        let slug = format!("{}-{}", slug_prefix, interval);

        println!("Market slug: {}", slug);

        let market_info = fetch_market_info(&slug)
            .await
            .expect("Failed to fetch market info");

        println!("Title: {}", market_info.title);
        println!("Condition ID: {}", market_info.condition_id);
        println!("Neg Risk: {}", market_info.neg_risk);
        println!("Up Token: {}", market_info.up_token);
        println!("Down Token: {}", market_info.down_token);

        // Step 2: Create SafeClient and split $1 USDC
        println!("\n--- Step 2: Split $1 USDC ---");
        let safe_client = SafeClient::new(
            SafeClientConfig {
                rpc_url: "https://polygon-rpc.com".to_string(),
                chain_id: 137,
                safe_address: safe_address
                    .parse::<Address>()
                    .expect("Invalid safe address"),
                max_wait_secs: Some(120),
            },
            &private_key,
        )
        .expect("Failed to create SafeClient");

        let split_amount = "1.0";
        println!("Splitting {} USDC...", split_amount);

        match safe_client
            .split_usdc(
                &market_info.condition_id,
                split_amount,
                market_info.neg_risk,
            )
            .await
        {
            Ok(tx_hash) => {
                println!("✓ Split tx submitted: {}", tx_hash);
            }
            Err(e) => {
                println!("✗ Split failed: {:?}", e);
                println!("  Make sure you have USDC in the Safe wallet");
                return;
            }
        }

        // Step 3: Wait for split to complete and verify balances
        println!("\n--- Step 3: Verify Token Balances ---");

        // Create balance client
        let l1_client = polyfill_rs::ClobClient::with_l1_headers(&host, &private_key, 137);
        let api_creds = l1_client
            .derive_api_key(None)
            .await
            .expect("Failed to get API credentials");
        let balance_client = polyfill_rs::ClobClient::with_proxy_wallet(
            &host,
            &private_key,
            137,
            api_creds,
            2, // PolyGnosisSafe
            &safe_address,
        );

        // Poll for token balances
        let mut up_balance = 0u64;
        let mut down_balance = 0u64;

        for i in 1..=15 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let up_params = BalanceAllowanceParams {
                asset_type: Some(AssetType::CONDITIONAL),
                token_id: Some(market_info.up_token.clone()),
                signature_type: None,
            };
            let down_params = BalanceAllowanceParams {
                asset_type: Some(AssetType::CONDITIONAL),
                token_id: Some(market_info.down_token.clone()),
                signature_type: None,
            };

            if let Ok(resp) = balance_client.get_balance_allowance(Some(up_params)).await {
                up_balance = resp
                    .get("balance")
                    .and_then(|b| b.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
            if let Ok(resp) = balance_client
                .get_balance_allowance(Some(down_params))
                .await
            {
                down_balance = resp
                    .get("balance")
                    .and_then(|b| b.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }

            println!("  [{}] UP: {} | DOWN: {}", i, up_balance, down_balance);

            // Check if we have enough tokens (at least 0.9 USDC worth = 900000)
            if up_balance >= 900000 && down_balance >= 900000 {
                println!("✓ Tokens received");
                break;
            }
        }

        if up_balance < 900000 || down_balance < 900000 {
            println!("✗ Did not receive expected tokens, aborting redemption test");
            return;
        }

        // Step 4: Build executor with Safe wallet and call execute_redemption
        println!("\n--- Step 4: Execute Redemption via Executor ---");

        let event_coordinator = Arc::new(TestEventCoordinator::new());

        let executor = PolymarketOrderExecutorBuilder::new()
            .host(&host)
            .private_key(&private_key)
            .proxy_wallet(2, &funder)
            .safe_wallet("https://polygon-rpc.com", &safe_address)
            .event_coordinator(event_coordinator.clone())
            .build()
            .await
            .expect("Failed to build executor");

        // Verify redemption is supported
        assert!(
            executor.supports_redemption(),
            "Executor should support redemption"
        );
        println!("✓ Executor supports redemption");

        // Create redemption action
        let redeem_quantity = 1.0; // Redeem $1 worth
        let action = RedemptionAction {
            market: market_info.condition_id.clone(),
            up_asset_id: market_info.up_token.clone(),
            down_asset_id: market_info.down_token.clone(),
            quantity: redeem_quantity,
            timestamp: chrono::Utc::now(),
        };

        println!(
            "Calling execute_redemption for {} pairs...",
            redeem_quantity
        );

        match executor.execute_redemption(&action).await {
            Ok(event) => {
                println!("✓ Redemption result: {:?}", event);

                match event {
                    crate::execution::RedemptionEvent::RedemptionCompleted {
                        market,
                        quantity,
                        quote_received,
                        ..
                    } => {
                        println!("\n✅ SUCCESS!");
                        println!("  Market: {}", market);
                        println!("  Quantity redeemed: {}", quantity);
                        println!("  USDC received: {}", quote_received);
                    }
                    crate::execution::RedemptionEvent::RedemptionFailed { reason, .. } => {
                        println!("\n⚠️ Redemption failed: {}", reason);
                    }
                }
            }
            Err(e) => {
                println!("✗ execute_redemption error: {:?}", e);
            }
        }

        println!("\n=== Test Complete ===");
    }

    /// Test split $1 USDC then run poller merge cycle.
    ///
    /// This test:
    /// 1. Splits $1 USDC into Up + Down tokens via SafeClient
    /// 2. Waits for split to complete and tokens to appear
    /// 3. Queues a merge request to the poller
    /// 4. Runs a merge cycle to merge tokens back to USDC
    /// 5. Verifies the merge was successful via Data API
    ///
    /// WARNING: This test performs real blockchain transactions!
    ///
    /// Required env vars:
    /// - POLYMARKET_PRIVATE_KEY: Signer private key (with 0x prefix)
    /// - POLYMARKET_SAFE_ADDRESS or POLYMARKET_FUNDER: Safe wallet address
    ///
    /// Run with:
    /// POLYMARKET_PRIVATE_KEY=0x... POLYMARKET_FUNDER=0x... \
    ///   cargo test test_split_and_poller_merge_cycle -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Performs real blockchain transactions"]
    async fn test_split_and_poller_merge_cycle() {
        use crate::client::polymarket::{SafeClient, SafeClientConfig};
        use crate::execution::polymarket::poller::{MergeConfig, MergeExecutor};
        use crate::signal::redemption::RedemptionAction;
        use alloy_primitives::Address;
        use polyfill_rs::ws_orderbook::{fetch_market_info, get_current_interval};
        use polyfill_rs::{AssetType, BalanceAllowanceParams, ClobClient};
        use tokio::sync::watch;

        // Initialize tracing
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(
                        "trade_server::execution::polymarket::poller=info"
                            .parse()
                            .unwrap(),
                    )
                    .add_directive("trade_server::client::polymarket=info".parse().unwrap()),
            )
            .with_test_writer()
            .try_init();

        println!("\n=== Test Split and Poller Merge Cycle ===\n");

        // Get credentials
        let private_key =
            std::env::var("POLYMARKET_PRIVATE_KEY").expect("Set POLYMARKET_PRIVATE_KEY");
        let safe_address = std::env::var("POLYMARKET_SAFE_ADDRESS")
            .or_else(|_| std::env::var("POLYMARKET_FUNDER"))
            .expect("Set POLYMARKET_SAFE_ADDRESS or POLYMARKET_FUNDER");
        let host = std::env::var("POLYMARKET_HOST")
            .unwrap_or_else(|_| "https://clob.polymarket.com".to_string());

        println!("Safe Address: {}", safe_address);
        println!("Host: {}", host);

        // Step 1: Fetch market info
        println!("\n--- Step 1: Fetch Market Info ---");
        let slug_prefix =
            std::env::var("MARKET_SLUG_PREFIX").unwrap_or_else(|_| "btc-updown-15m".to_string());
        let interval = get_current_interval();
        let slug = format!("{}-{}", slug_prefix, interval);

        println!("Market slug: {}", slug);

        let market_info = fetch_market_info(&slug)
            .await
            .expect("Failed to fetch market info");

        println!("Title: {}", market_info.title);
        println!("Condition ID: {}", market_info.condition_id);
        println!("Neg Risk: {}", market_info.neg_risk);
        println!("Up Token: {}", market_info.up_token);
        println!("Down Token: {}", market_info.down_token);

        // Step 2: Create SafeClient and split $1 USDC
        println!("\n--- Step 2: Split $1 USDC ---");
        let safe_config = SafeClientConfig {
            rpc_url: "https://polygon-rpc.com".to_string(),
            chain_id: 137,
            safe_address: safe_address
                .parse::<Address>()
                .expect("Invalid safe address"),
            max_wait_secs: Some(120),
        };
        let safe_client = SafeClient::new(safe_config.clone(), &private_key)
            .expect("Failed to create SafeClient");

        let split_amount = "1.0";
        println!("Splitting {} USDC...", split_amount);

        match safe_client
            .split_usdc(
                &market_info.condition_id,
                split_amount,
                market_info.neg_risk,
            )
            .await
        {
            Ok(tx_hash) => {
                println!("✓ Split tx submitted: {}", tx_hash);

                // Wait for on-chain confirmation
                println!("Waiting for on-chain confirmation...");
                let confirmation = safe_client.wait_for_transaction(&tx_hash, Some(60)).await;
                match &confirmation {
                    crate::client::polymarket::TransactionConfirmation::Success {
                        block_number,
                        ..
                    } => {
                        println!("✓ Split confirmed at block {}", block_number);
                    }
                    crate::client::polymarket::TransactionConfirmation::Failed { .. } => {
                        println!("✗ Split transaction reverted");
                        return;
                    }
                    crate::client::polymarket::TransactionConfirmation::Timeout { .. } => {
                        println!("⚠️ Split confirmation timeout, continuing anyway...");
                    }
                }
            }
            Err(e) => {
                println!("✗ Split failed: {:?}", e);
                println!("  Make sure you have USDC in the Safe wallet");
                return;
            }
        }

        // Step 3: Wait for tokens and verify balances via CLOB API
        println!("\n--- Step 3: Verify Token Balances ---");

        // Create CLOB client
        let l1_client = ClobClient::with_l1_headers(&host, &private_key, 137);
        let api_creds = l1_client
            .derive_api_key(None)
            .await
            .expect("Failed to get API credentials");
        let clob_client = ClobClient::with_proxy_wallet(
            &host,
            &private_key,
            137,
            api_creds.clone(),
            2, // PolyGnosisSafe
            &safe_address,
        );

        // Poll for token balances
        let mut up_balance = 0u64;
        let mut down_balance = 0u64;

        for i in 1..=20 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let up_params = BalanceAllowanceParams {
                asset_type: Some(AssetType::CONDITIONAL),
                token_id: Some(market_info.up_token.clone()),
                signature_type: None,
            };
            let down_params = BalanceAllowanceParams {
                asset_type: Some(AssetType::CONDITIONAL),
                token_id: Some(market_info.down_token.clone()),
                signature_type: None,
            };

            if let Ok(resp) = clob_client.get_balance_allowance(Some(up_params)).await {
                up_balance = resp
                    .get("balance")
                    .and_then(|b| b.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
            if let Ok(resp) = clob_client.get_balance_allowance(Some(down_params)).await {
                down_balance = resp
                    .get("balance")
                    .and_then(|b| b.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }

            println!("  [{}] UP: {} | DOWN: {}", i, up_balance, down_balance);

            // Check if we have enough tokens (at least 0.9 USDC worth = 900000)
            if up_balance >= 900000 && down_balance >= 900000 {
                println!("✓ Tokens received");
                break;
            }
        }

        if up_balance < 900000 || down_balance < 900000 {
            println!("✗ Did not receive expected tokens, aborting merge test");
            return;
        }

        // Step 4: Create poller and queue merge request
        println!("\n--- Step 4: Create Poller and Queue Merge ---");

        let event_coordinator = Arc::new(TestEventCoordinator::new());

        // Create new SafeClient for merge executor
        let merge_safe_client = SafeClient::new(safe_config, &private_key)
            .expect("Failed to create SafeClient for merge executor");

        // Create new ClobClient for merge executor
        let merge_clob_client = Arc::new(ClobClient::with_proxy_wallet(
            &host,
            &private_key,
            137,
            api_creds,
            2,
            &safe_address,
        ));

        let merge_config = MergeConfig::new().with_merge_poll_interval(Duration::from_secs(3));

        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        // Create merge executor
        let merge_executor = Arc::new(MergeExecutor::new(
            merge_safe_client,
            event_coordinator.clone(),
            merge_clob_client,
            merge_config,
        ));

        // Queue merge request
        let merge_action = RedemptionAction {
            market: market_info.condition_id.clone(),
            up_asset_id: market_info.up_token.clone(),
            down_asset_id: market_info.down_token.clone(),
            quantity: 1.0, // Merge $1 worth
            timestamp: chrono::Utc::now(),
        };

        println!("Queueing merge request...");
        merge_executor.request_merge(merge_action).await;
        println!("✓ Merge request queued");

        // Step 5: Run merge cycle
        println!("\n--- Step 5: Run Merge Cycle ---");
        println!("Running merge cycle (this will wait for on-chain + CLOB confirmation)...");

        let start = std::time::Instant::now();
        merge_executor.process_pending_merges().await;
        let elapsed = start.elapsed();

        println!("✓ Merge cycle completed in {:.1}s", elapsed.as_secs_f32());

        // Step 6: Check results
        println!("\n--- Step 6: Verify Results ---");

        let event_count = event_coordinator.get_event_count();
        let event_types = event_coordinator.get_event_types();
        println!("Events captured: {} {:?}", event_count, event_types);

        // Check if RedemptionCompleted event was emitted
        if event_types.contains(&"Redemption".to_string()) {
            println!("\n✅ SUCCESS! Merge cycle completed and RedemptionCompleted event emitted.");
        } else {
            println!("\n⚠️ Merge cycle completed but no Redemption event found.");
            println!("   This may indicate the merge is still pending or failed.");
        }

        // Cleanup
        let _ = shutdown_tx.send(true);

        println!("\n=== Test Complete ===");
    }
}
