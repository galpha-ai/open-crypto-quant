//! Instruction for creating new tokens with bonding curves
//!
//! This module provides the functionality to create new tokens with associated bonding curves.
//! It includes the instruction data structure and helper function to build the Solana instruction.

use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use spl_associated_token_account::get_associated_token_address;

use crate::parser::pumpfun2::{PumpFun, constants};

/// Instruction data for creating a new token
///
/// # Fields
///
/// * `name` - Name of the token to be created
/// * `symbol` - Symbol/ticker of the token to be created
/// * `uri` - Metadata URI containing token information (image, description, etc.)
/// * `creator` - Public key of the token creator
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct Create {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub creator: Pubkey,
}

impl Create {
    /// Instruction discriminator used to identify this instruction
    pub const DISCRIMINATOR: [u8; 8] = [24, 30, 200, 40, 5, 28, 7, 119];

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

/// Creates an instruction to create a new token with bonding curve
///
/// Creates a new SPL token with an associated bonding curve that determines its price.
/// The token will have metadata and be tradable according to the bonding curve formula.
///
/// # Arguments
///
/// * `payer` - Keypair that will pay for account creation and transaction fees
/// * `mint` - Keypair for the new token mint account that will be created
/// * `args` - Create instruction data containing token name, symbol, metadata URI, and creator
///
/// # Returns
///
/// Returns a Solana instruction that when executed will create the token and its accounts
///
/// # Account Requirements
///
/// The instruction requires the following accounts in this order:
/// 1. Mint account (signer, writable)
/// 2. Mint authority PDA (readonly)
/// 3. Bonding curve PDA (writable)
/// 4. Bonding curve token account (writable)
/// 5. Global configuration PDA (readonly)
/// 6. MPL Token Metadata program (readonly)
/// 7. Metadata PDA (writable)
/// 8. Payer account (signer, writable)
/// 9. System program (readonly)
/// 10. Token program (readonly)
/// 11. Associated token program (readonly)
/// 12. Rent sysvar (readonly)
/// 13. Event authority (readonly)
/// 14. Pump.fun program ID (readonly)
pub fn create(payer: &Pubkey, mint: &Pubkey, args: Create) -> Instruction {
    let bonding_curve: Pubkey = PumpFun::get_bonding_curve_pda(&mint).unwrap();
    Instruction::new_with_bytes(
        constants::accounts::PUMPFUN,
        &args.data(),
        vec![
            AccountMeta::new(*mint, true),
            AccountMeta::new(PumpFun::get_mint_authority_pda(), false),
            AccountMeta::new(bonding_curve, false),
            AccountMeta::new(get_associated_token_address(&bonding_curve, &mint), false),
            AccountMeta::new_readonly(PumpFun::get_global_pda(), false),
            AccountMeta::new_readonly(constants::accounts::MPL_TOKEN_METADATA, false),
            AccountMeta::new(PumpFun::get_metadata_pda(&mint), false),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(constants::accounts::SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::ASSOCIATED_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(constants::accounts::RENT, false),
            AccountMeta::new_readonly(constants::accounts::EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(constants::accounts::PUMPFUN, false),
        ],
    )
}
