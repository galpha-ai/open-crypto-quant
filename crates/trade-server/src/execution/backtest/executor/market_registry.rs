use popeyes_trading_types::TradeSide;

use super::BacktestOrderExecutor;

impl BacktestOrderExecutor {
    pub(super) fn flip_trade_side(side: &TradeSide) -> TradeSide {
        match side {
            TradeSide::Buy => TradeSide::Sell,
            TradeSide::Sell => TradeSide::Buy,
        }
    }

    /// Record that we've observed `asset_id` in `market` and update complement mapping if possible.
    ///
    /// Polymarket binary markets have two complementary assets. Once both are observed, we can
    /// simulate the CTF mirroring relation by mapping trades on one asset to a mirrored trade on
    /// the other with:
    /// - `price' = 1 - price`
    /// - `side' = flip(side)`
    pub(super) async fn record_market_asset(&self, market: &str, asset_id: &str) {
        let mut market_assets = self.market_assets.lock().await;

        let entry = market_assets
            .entry(market.to_string())
            .or_insert_with(|| (asset_id.to_string(), None));

        if entry.0 == asset_id {
            return;
        }

        if entry.1.as_deref() == Some(asset_id) {
            return;
        }

        if entry.1.is_none() {
            entry.1 = Some(asset_id.to_string());
        }

        if let Some(second) = &entry.1 {
            let first = entry.0.clone();
            let second = second.clone();
            drop(market_assets);

            let mut complement = self.complement_by_asset.lock().await;
            complement.insert(first.clone(), second.clone());
            complement.insert(second, first);
        }
    }

    pub(super) async fn complement_asset_id(&self, asset_id: &str) -> Option<String> {
        let complement = self.complement_by_asset.lock().await;
        complement.get(asset_id).cloned()
    }
}
