use std::convert::TryInto;

use chrono::Utc;
use popeyes_trading_types::{
    BonkSwapDirection, BonkTradeEvent, MetDLMMSwapDirection, MetDLMMTradeEvent, PumpFunTradeEvent,
    RayAMMv4TradeEvent, RayAMMv4TradeSwapDirection, TokenCreationEvent, TokenEvent, TradeEventType,
};
use solana_sdk::pubkey::Pubkey;

use super::{
    bonk::BonkSwap,
    ray_ammv4::{Ray4AMMv4SwapDirection, RayAMMv4Swap},
};
use crate::parser::{DataParserError, meteora_dlmm::MetDLMMSwap};

#[derive(Debug, Clone)]
pub struct MarketTrade {
    pub slot: u64,
    pub signature: String,
    pub log: TradeLog,
}

#[derive(Debug, Clone)]
pub enum TradeLog {
    /// Represents a trade log from the Pumpfun protocol
    Pumpfun(PumpfunLog),
    /// Raydium AMMv4 swaps
    RayAMMv4(RayAMMv4Swap),
    /// Pumpfun create log
    PumpfunTokenCreate(TokenCreationEvent),
    /// Meteora DLMM
    MeteoraDLMM(MetDLMMSwap),
    /// Bonk DEX swaps
    Bonk(BonkSwap),
}

#[derive(Debug, Clone)]
pub struct PumpfunLog {
    /// Token mint address
    pub mint: Pubkey,

    /// Amount of SOL involved in the trade (in lamports)
    pub sol_amount: u64,

    /// Amount of tokens involved in the trade
    pub token_amount: u64,

    /// Whether this is a buy (true) or sell (false) transaction
    pub is_buy: bool,

    /// The user's wallet address that initiated the trade
    pub user: Pubkey,

    /// Unix timestamp of when the trade occurred
    pub timestamp: i64,

    /// Virtual SOL reserves in the pool after the trade
    pub virtual_sol_reserves: u64,

    /// Virtual token reserves in the pool after the trade
    pub virtual_token_reserves: u64,

    /// Real SOL reserves in the pool after the trade
    pub real_sol_reserves: u64,

    /// Real token reserves in the pool after the trade
    pub real_token_reserves: u64,

    /// Address of the fee recipient
    pub fee_recipient: Pubkey,

    /// Fee in basis points (1/100 of a percent)
    pub fee_basis_points: u64,

    /// Fee amount in lamports
    pub fee: u64,

    /// Creator address for the token
    pub creator: Pubkey,

    /// Creator fee in basis points (1/100 of a percent)
    pub creator_fee_basis_points: u64,

    /// Creator fee amount in lamports
    pub creator_fee: u64,
}

#[derive(Debug, Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PumpfunCreateLog {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,

    pub user: Pubkey,
    pub creator: Pubkey,

    pub timestamp: u64,

    pub virtual_token_reserves: u64,

    pub virtual_sol_reserves: u64,

    pub real_token_reserves: u64,

    pub token_total_supply: u64,
}

impl MarketTrade {
    pub fn to_token_event(&self) -> TokenEvent {
        match &self.log {
            TradeLog::Pumpfun(log) => {
                // Convert from raw values to the expected format
                let trader_public_key = log.user.to_string();
                let timestamp = chrono::DateTime::<Utc>::from_timestamp(log.timestamp, 0)
                    .unwrap_or_else(|| Utc::now());
                let mint = log.mint.to_string();

                // Calculate market cap (virtual SOL in the pool)
                let v_sol_in_bonding_curve = Some(log.virtual_sol_reserves as f64);
                let v_tokens_in_bonding_curve = Some(log.virtual_token_reserves as f64);
                let market_cap_sol = v_sol_in_bonding_curve;

                // Convert token amounts
                let token_amount = log.token_amount as f64;
                let sol_amount = Some(log.sol_amount as f64);

                let real_sol_reserves = Some(log.real_sol_reserves as f64);
                let real_token_reserves = Some(log.real_token_reserves as f64);
                let fee_recipient = Some(log.fee_recipient.to_string());
                let fee_basis_points = Some(log.fee_basis_points as u16);
                let fee_amount = Some(log.fee as f64);
                let creator = Some(log.creator.to_string());
                let creator_fee_basis_points = Some(log.creator_fee_basis_points as u16);
                let creator_fee_amount = Some(log.creator_fee as f64);

                let slot = self.slot;

                // Create the appropriate TokenEvent variant based on buy/sell
                return if log.is_buy {
                    TokenEvent::Buy(
                        PumpFunTradeEvent::new(
                            self.signature.clone(),
                            mint,
                            trader_public_key,
                            TradeEventType::Buy,
                            token_amount,
                            sol_amount,
                            v_tokens_in_bonding_curve,
                            v_sol_in_bonding_curve,
                            None,
                            real_sol_reserves,
                            real_token_reserves,
                            fee_recipient,
                            fee_basis_points,
                            fee_amount,
                            creator,
                            creator_fee_basis_points,
                            creator_fee_amount,
                            timestamp,
                            slot,
                        )
                        .into(),
                    )
                } else {
                    TokenEvent::Sell(
                        PumpFunTradeEvent::new(
                            self.signature.clone(),
                            mint,
                            trader_public_key,
                            TradeEventType::Sell,
                            token_amount,
                            sol_amount,
                            v_tokens_in_bonding_curve,
                            v_sol_in_bonding_curve,
                            market_cap_sol,
                            real_sol_reserves,
                            real_token_reserves,
                            fee_recipient,
                            fee_basis_points,
                            fee_amount,
                            creator,
                            creator_fee_basis_points,
                            creator_fee_amount,
                            timestamp,
                            slot,
                        )
                        .into(),
                    )
                };
            }
            TradeLog::MeteoraDLMM(met_dlmmswap) => {
                let trader_public_key = met_dlmmswap.user.to_string();
                let timestamp = chrono::DateTime::<Utc>::from_timestamp(met_dlmmswap.timestamp, 0)
                    .unwrap_or_else(|| Utc::now());

                return if met_dlmmswap.direction == MetDLMMSwapDirection::BaseIn {
                    TokenEvent::Sell(
                        MetDLMMTradeEvent::new(
                            self.signature.clone(),
                            trader_public_key,
                            self.slot,
                            TradeEventType::Sell,
                            met_dlmmswap.direction,
                            met_dlmmswap.market.to_string(),
                            met_dlmmswap.input_token_mint.to_string(),
                            met_dlmmswap.output_token_mint.to_string(),
                            met_dlmmswap.qty_in,
                            met_dlmmswap.qty_out,
                            timestamp,
                        )
                        .into(),
                    )
                } else {
                    TokenEvent::Buy(
                        MetDLMMTradeEvent::new(
                            self.signature.clone(),
                            trader_public_key,
                            self.slot,
                            TradeEventType::Buy,
                            met_dlmmswap.direction,
                            met_dlmmswap.market.to_string(),
                            met_dlmmswap.input_token_mint.to_string(),
                            met_dlmmswap.output_token_mint.to_string(),
                            met_dlmmswap.qty_in,
                            met_dlmmswap.qty_out,
                            timestamp,
                        )
                        .into(),
                    )
                };
            }

            TradeLog::RayAMMv4(log) => {
                let trader_public_key = log.user.to_string();
                let timestamp = chrono::DateTime::<Utc>::from_timestamp(log.timestamp, 0)
                    .unwrap_or_else(|| Utc::now());

                return if log.direction == Ray4AMMv4SwapDirection::BaseIn {
                    TokenEvent::Sell(
                        RayAMMv4TradeEvent::new(
                            self.signature.clone(),
                            trader_public_key,
                            TradeEventType::Swap,
                            RayAMMv4TradeSwapDirection::BaseIn,
                            log.market.to_string(),
                            log.input_token_mint.to_string(),
                            log.output_token_mint.to_string(),
                            log.qty_in,
                            log.qty_out,
                            timestamp,
                            self.slot,
                        )
                        .into(),
                    )
                } else {
                    TokenEvent::Buy(
                        RayAMMv4TradeEvent::new(
                            self.signature.clone(),
                            trader_public_key,
                            TradeEventType::Swap,
                            RayAMMv4TradeSwapDirection::BaseOut,
                            log.market.to_string(),
                            log.input_token_mint.to_string(),
                            log.output_token_mint.to_string(),
                            log.qty_in,
                            log.qty_out,
                            timestamp,
                            self.slot,
                        )
                        .into(),
                    )
                };
            }
            TradeLog::Bonk(log) => {
                let trader_public_key = log.user.clone();
                let timestamp = Utc::now(); // Using current time as Bonk doesn't provide timestamp

                let bonk_direction = match log.direction {
                    super::bonk::TradeDirection::Buy => BonkSwapDirection::Buy,
                    super::bonk::TradeDirection::Sell => BonkSwapDirection::Sell,
                };

                let (input_mint, output_mint, qty_in, qty_out) = match log.direction {
                    super::bonk::TradeDirection::Buy => (
                        log.quote_mint.clone(),
                        log.base_mint.clone(),
                        log.quote_amount,
                        log.base_amount,
                    ),
                    super::bonk::TradeDirection::Sell => (
                        log.base_mint.clone(),
                        log.quote_mint.clone(),
                        log.base_amount,
                        log.quote_amount,
                    ),
                };

                return if matches!(log.direction, super::bonk::TradeDirection::Buy) {
                    TokenEvent::Buy(
                        BonkTradeEvent::new(
                            self.signature.clone(),
                            trader_public_key,
                            self.slot,
                            TradeEventType::Buy,
                            bonk_direction,
                            log.pool_address.clone(),
                            input_mint,
                            output_mint,
                            qty_in,
                            qty_out,
                            timestamp,
                        )
                        .into(),
                    )
                } else {
                    TokenEvent::Sell(
                        BonkTradeEvent::new(
                            self.signature.clone(),
                            trader_public_key,
                            self.slot,
                            TradeEventType::Sell,
                            bonk_direction,
                            log.pool_address.clone(),
                            input_mint,
                            output_mint,
                            qty_in,
                            qty_out,
                            timestamp,
                        )
                        .into(),
                    )
                };
            }
            TradeLog::PumpfunTokenCreate(create_log) => TokenEvent::Create(create_log.clone()),
        } // end of match
    }
}

impl PumpfunLog {
    /// Convert from raw event bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, DataParserError> {
        const DISCRIMINATOR_SIZE: usize = 16;

        if data.len() < DISCRIMINATOR_SIZE {
            return Err(DataParserError::InsufficientData);
        }

        // Skip the discriminator
        let event_data = &data[DISCRIMINATOR_SIZE..];

        // Manual parsing logic based on the expected byte layout
        let mut offset = 0;

        // Parse mint (Pubkey = 32 bytes)
        if offset + 32 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let pubkey_slice = &event_data[offset..offset + 32];
        let mint = Pubkey::new_from_array(
            pubkey_slice
                .try_into()
                .map_err(|_| DataParserError::InvalidFormat)?,
        );
        offset += 32;

        // Parse sol_amount (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let sol_amount = u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse token_amount (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let token_amount = u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse is_buy (bool = 1 byte)
        if offset + 1 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let is_buy = event_data[offset] != 0;
        offset += 1;

        // Parse user (Pubkey = 32 bytes)
        if offset + 32 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let pubkey_slice = &event_data[offset..offset + 32];
        let user = Pubkey::new_from_array(
            pubkey_slice
                .try_into()
                .map_err(|_| DataParserError::InvalidFormat)?,
        );
        offset += 32;

        // Parse timestamp (i64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let timestamp = i64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse virtual_sol_reserves (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let virtual_sol_reserves =
            u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse virtual_token_reserves (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let virtual_token_reserves =
            u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Check if we have more data - if not, return with just the original fields
        // This ensures backward compatibility with older logs
        if offset >= event_data.len() {
            return Ok(Self {
                mint,
                sol_amount,
                token_amount,
                is_buy,
                user,
                timestamp,
                virtual_sol_reserves,
                virtual_token_reserves,
                real_sol_reserves: 0,
                real_token_reserves: 0,
                fee_recipient: Pubkey::default(),
                fee_basis_points: 0,
                fee: 0,
                creator: Pubkey::default(),
                creator_fee_basis_points: 0,
                creator_fee: 0,
            });
        }

        // Parse new fields if available

        // Parse real_sol_reserves (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let real_sol_reserves =
            u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse real_token_reserves (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let real_token_reserves =
            u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse fee_recipient (Pubkey = 32 bytes)
        if offset + 32 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let pubkey_slice = &event_data[offset..offset + 32];
        let fee_recipient = Pubkey::new_from_array(
            pubkey_slice
                .try_into()
                .map_err(|_| DataParserError::InvalidFormat)?,
        );
        offset += 32;

        // Parse fee_basis_points (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let fee_basis_points =
            u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse fee (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let fee = u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse creator (Pubkey = 32 bytes)
        if offset + 32 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let pubkey_slice = &event_data[offset..offset + 32];
        let creator = Pubkey::new_from_array(
            pubkey_slice
                .try_into()
                .map_err(|_| DataParserError::InvalidFormat)?,
        );
        offset += 32;

        // Parse creator_fee_basis_points (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let creator_fee_basis_points =
            u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Parse creator_fee (u64 = 8 bytes)
        if offset + 8 > event_data.len() {
            return Err(DataParserError::InsufficientData);
        }
        let creator_fee = u64::from_le_bytes(event_data[offset..offset + 8].try_into().unwrap());

        Ok(Self {
            mint,
            sol_amount,
            token_amount,
            is_buy,
            user,
            timestamp,
            virtual_sol_reserves,
            virtual_token_reserves,
            real_sol_reserves,
            real_token_reserves,
            fee_recipient,
            fee_basis_points,
            fee,
            creator,
            creator_fee_basis_points,
            creator_fee,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_pumpfun_log() {
        // Example tx signature: 4MfDuPtzkofWweftUkocqv7wKvk5MEpr76p1hTrZnnHkVkRmRZFHzUNXUeg5X3rzrnRjJhmNGYcVopsCy3RsQ1pm
        // Raw bytes from the provided hex string
        let raw_bytes = hex::decode("e445a52e51cb9a1dbddb7fd34ee661ee8e25fb95698ffb764d2e6f1647f6c6127ab3de6badee7a11c111611211cce53f4bddfa01000000008f976d4ba700000001d19eb6f9758c4e89b6c4d2d879019a7b71125b5a01cba038a6c0be573b725993bc022668000000005fac57fc08000000e3c942c697f602005f00340002000000e331307a06f801004ac2f8d0dd5cbc97e3289c197cb5062a54f3d956b9ce6e5115f96567aa5cb3e65f00000000000000b2d0040000000000c767ed7a90df09129f7cfd8c437b529585f04b43589f018549d5c5a1f92e1f080500000000000000e140000000000000").unwrap();

        // Parse the raw bytes
        let parsed_log = PumpfunLog::from_bytes(&raw_bytes).expect("Failed to parse Pumpfun log");

        // Verify the values match expected output
        assert_eq!(
            parsed_log.mint.to_string(),
            "AZtV9Gm1Hx6PsRcjzP3MkEsszjJ9YfWuaCcQaVbUpump"
        );
        assert_eq!(parsed_log.sol_amount, 33217867);
        assert_eq!(parsed_log.token_amount, 718525011855);
        assert_eq!(parsed_log.is_buy, true);
        assert_eq!(
            parsed_log.user.to_string(),
            "F7GaTt5t6tgmfxkqfdzNSNgmbSzuTEfUs4cFEwyTVgvN"
        );
        assert_eq!(parsed_log.timestamp, 1747321532);
        assert_eq!(parsed_log.virtual_sol_reserves, 38593342559);
        assert_eq!(parsed_log.virtual_token_reserves, 834081680181731);

        // Add assertions for the new fields
        assert_eq!(parsed_log.real_sol_reserves, 8593342559);
        assert_eq!(parsed_log.real_token_reserves, 554181680181731);
        assert_eq!(
            parsed_log.fee_recipient.to_string(),
            "62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV"
        );
        assert_eq!(parsed_log.fee_basis_points, 95);
        assert_eq!(parsed_log.fee, 315570);
        assert_eq!(
            parsed_log.creator.to_string(),
            "ERQ3fjRnauSZCMxyyPfYWABEuCyPkFV7Jk9Zxqj3YxjH"
        );
        assert_eq!(parsed_log.creator_fee_basis_points, 5);
        assert_eq!(parsed_log.creator_fee, 16609);
    }
}
