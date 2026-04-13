//! Instruction for buying tokens from bonding curves
//!
//! This module provides the functionality to buy tokens from bonding curves.
//! It includes the instruction data structure and helper function to build the Solana instruction.

use anyhow::{Context, Result};
use borsh::{BorshDeserialize, BorshSerialize};
//use pumpfun_cpi::BondingCurve;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use spl_associated_token_account::get_associated_token_address;

use crate::parser::pumpfun2::{PumpFun, constants};

/// Instruction data for buying tokens from a bonding curve
///
/// # Fields
///
/// * `amount` - Amount of tokens to buy (in token smallest units)
/// * `max_sol_cost` - Maximum acceptable SOL cost for the purchase (slippage protection)
#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Buy {
    pub amount: u64,
    pub max_sol_cost: u64,
}

impl Buy {
    /// Instruction discriminator used to identify this instruction
    pub const DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];

    /// Serializes the instruction data with the appropriate discriminator
    ///
    /// # Returns
    ///
    /// Byte vector containing the serialized instruction data
    pub fn data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(256);
        data.extend_from_slice(&Self::DISCRIMINATOR);
        self.serialize(&mut data).unwrap();
        data
    }
}

// /// Creates an instruction to buy tokens from a bonding curve
// ///
// /// Buys tokens by providing SOL. The amount of tokens received is calculated based on
// /// the bonding curve formula. A portion of the SOL is taken as a fee and sent to the
// /// fee recipient account. The price increases as more tokens are purchased according to
// /// the bonding curve function.
// ///
// /// # Arguments
// ///
// /// * `payer` - Keypair that will provide the SOL to buy tokens
// /// * `mint` - Public key of the token mint to buy
// /// * `fee_recipient` - Public key of the account that will receive the transaction fee
// /// * `args` - Buy instruction data containing the token amount and maximum acceptable SOL price
// /// * `creator` - Optional creator wallet address to avoid RPC call
// ///
// /// # Returns
// ///
// /// Returns a Solana instruction that when executed will buy tokens from the bonding curve
// ///
// /// # Account Requirements
// ///
// /// The instruction requires the following accounts in this order:
// /// 1. Global configuration PDA (readonly)
// /// 2. Fee recipient account (writable)
// /// 3. Token mint account (readonly)
// /// 4. Bonding curve PDA (writable)
// /// 5. Bonding curve token account (writable)
// /// 6. Buyer's token account (writable)
// /// 7. Payer account (signer, writable)
// /// 8. System program (readonly)
// /// 9. Token program (readonly)
// /// 10. Rent sysvar (readonly)
// /// 11. Event authority (readonly)
// /// 12. Pump.fun program ID (readonly)
// pub fn buy(
//     rpc_client: &solana_rpc_client::rpc_client::RpcClient,
//     payer: &Pubkey,
//     mint: &Pubkey,
//     fee_recipient: &Pubkey,
//     args: Buy,
//     creator: Option<&Pubkey>,
// ) -> Result<Instruction> {
//     eprintln!("buy instruction");
//     let bonding_curve: Pubkey =
//         PumpFun::get_bonding_curve_pda(mint).context("Failed to derive bonding curve PDA")?;
//
//     // Determine creator pubkey - either use provided creator or fetch from RPC
//     let creator_pubkey = if let Some(creator_key) = creator {
//         // Use provided creator key
//         eprintln!("using creator_pubkey from request {}", creator_key);
//         creator_key.clone()
//     } else {
//         // Fetch bonding curve data from RPC
//         let bonding_curve_account = rpc_client
//             .get_account(&bonding_curve)
//             .context("Failed to fetch bonding curve account")?;
//
//         let bonding_curve_data =
//             bincode::deserialize::<BondingCurve>(&bonding_curve_account.data[8..])
//                 .context("Failed to deserialize bonding curve data")?;
//
//         eprintln!(
//             "using creator_pubkey from rpc result {}",
//             &bonding_curve_data.creator
//         );
//
//         bonding_curve_data.creator
//     };
//
//     // Create creator vault using the determined creator pubkey
//     let creator_vault = {
//         let seeds: &[&[u8]; 2] = &["creator-vault".as_bytes(), &creator_pubkey.to_bytes()];
//         Pubkey::find_program_address(seeds, &constants::accounts::PUMPFUN).0
//     };
//
//     Ok(Instruction::new_with_bytes(
//         constants::accounts::PUMPFUN,
//         &args.data(),
//         vec![
//             AccountMeta::new_readonly(PumpFun::get_global_pda(), false),
//             AccountMeta::new(*fee_recipient, false),
//             AccountMeta::new_readonly(mint.clone(), false),
//             AccountMeta::new(bonding_curve.clone(), false),
//             AccountMeta::new(get_associated_token_address(&bonding_curve, mint), false),
//             AccountMeta::new(get_associated_token_address(&payer, mint), false),
//             AccountMeta::new(payer.clone(), true),
//             AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
//             AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
//             //AccountMeta::new_readonly(constants::accounts::RENT, false),
//             AccountMeta::new(creator_vault, false),
//             AccountMeta::new_readonly(constants::accounts::EVENT_AUTHORITY, false),
//             AccountMeta::new_readonly(constants::accounts::PUMPFUN, false),
//         ],
//     ))
// }
