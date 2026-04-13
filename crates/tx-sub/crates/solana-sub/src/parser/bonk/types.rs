use borsh::BorshDeserialize;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshDeserialize)]
pub enum TradeDirection {
    #[default]
    Buy,
    Sell,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshDeserialize)]
pub enum PoolStatus {
    #[default]
    Fund,
    Migrate,
    Trade,
}

// Bonk swap struct for the parsed trade data
#[derive(Debug, Clone, PartialEq)]
pub struct BonkSwap {
    pub direction: TradeDirection,
    pub pool_address: String,
    pub base_mint: String,
    pub quote_mint: String,
    pub base_amount: u64,
    pub quote_amount: u64,
    pub user: String,
}
