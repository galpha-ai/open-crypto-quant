//! Calldata encoding functions for Polymarket contract interactions

use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_sol_types::SolCall;

use super::abi::{IConditionalTokens, IERC20, IERC1155, IMultiSend, INegRiskAdapter};
use super::constants::USDC_ADDRESS;

/// Encode ERC20 approve calldata
pub fn encode_erc20_approve(spender: Address, amount: U256) -> Bytes {
    let call = IERC20::approveCall { spender, amount };
    Bytes::from(call.abi_encode())
}

/// Encode ERC20 transfer calldata
pub fn encode_erc20_transfer(to: Address, amount: U256) -> Bytes {
    let call = IERC20::transferCall { to, amount };
    Bytes::from(call.abi_encode())
}

/// Encode ERC1155 setApprovalForAll calldata
pub fn encode_erc1155_approve(operator: Address, approved: bool) -> Bytes {
    let call = IERC1155::setApprovalForAllCall { operator, approved };
    Bytes::from(call.abi_encode())
}

/// Encode ERC1155 safeTransferFrom calldata
pub fn encode_erc1155_transfer(from: Address, to: Address, id: U256, amount: U256) -> Bytes {
    let call = IERC1155::safeTransferFromCall {
        from,
        to,
        id,
        amount,
        data: Bytes::new(),
    };
    Bytes::from(call.abi_encode())
}

/// Encode CTF splitPosition calldata
/// Used for non-negRisk markets
pub fn encode_split(
    collateral_token: Address,
    condition_id: FixedBytes<32>,
    amount: U256,
) -> Bytes {
    let call = IConditionalTokens::splitPositionCall {
        collateralToken: collateral_token,
        parentCollectionId: FixedBytes::ZERO,
        conditionId: condition_id,
        partition: vec![U256::from(1), U256::from(2)],
        amount,
    };
    Bytes::from(call.abi_encode())
}

/// Encode CTF mergePositions calldata
/// Used for non-negRisk markets
pub fn encode_merge(
    collateral_token: Address,
    condition_id: FixedBytes<32>,
    amount: U256,
) -> Bytes {
    let call = IConditionalTokens::mergePositionsCall {
        collateralToken: collateral_token,
        parentCollectionId: FixedBytes::ZERO,
        conditionId: condition_id,
        partition: vec![U256::from(1), U256::from(2)],
        amount,
    };
    Bytes::from(call.abi_encode())
}

/// Encode CTF redeemPositions calldata
/// Used for non-negRisk markets
pub fn encode_redeem(collateral_token: Address, condition_id: FixedBytes<32>) -> Bytes {
    let call = IConditionalTokens::redeemPositionsCall {
        collateralToken: collateral_token,
        parentCollectionId: FixedBytes::ZERO,
        conditionId: condition_id,
        indexSets: vec![U256::from(1), U256::from(2)],
    };
    Bytes::from(call.abi_encode())
}

/// Encode NegRiskAdapter splitPosition calldata (5 params version)
/// Used for negRisk markets
pub fn encode_split_neg_risk(condition_id: FixedBytes<32>, amount: U256) -> Bytes {
    let call = IConditionalTokens::splitPositionCall {
        collateralToken: USDC_ADDRESS,
        parentCollectionId: FixedBytes::ZERO,
        conditionId: condition_id,
        partition: vec![U256::from(1), U256::from(2)],
        amount,
    };
    Bytes::from(call.abi_encode())
}

/// Encode NegRiskAdapter mergePositions calldata (5 params version)
/// Used for negRisk markets
pub fn encode_merge_neg_risk(condition_id: FixedBytes<32>, amount: U256) -> Bytes {
    let call = IConditionalTokens::mergePositionsCall {
        collateralToken: USDC_ADDRESS,
        parentCollectionId: FixedBytes::ZERO,
        conditionId: condition_id,
        partition: vec![U256::from(1), U256::from(2)],
        amount,
    };
    Bytes::from(call.abi_encode())
}

/// Encode NegRiskAdapter redeemPositions calldata
pub fn encode_redeem_neg_risk(condition_id: FixedBytes<32>, amounts: Vec<U256>) -> Bytes {
    let call = INegRiskAdapter::redeemPositionsCall {
        conditionId: condition_id,
        amounts,
    };
    Bytes::from(call.abi_encode())
}

/// Encode NegRiskAdapter convertPositions calldata
pub fn encode_convert(market_id: FixedBytes<32>, index_set: U256, amount: U256) -> Bytes {
    let call = INegRiskAdapter::convertPositionsCall {
        marketId: market_id,
        indexSet: index_set,
        amount,
    };
    Bytes::from(call.abi_encode())
}

/// Operation type for Safe transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OperationType {
    Call = 0,
    DelegateCall = 1,
}

/// A Safe transaction
#[derive(Debug, Clone)]
pub struct SafeTransaction {
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub operation: OperationType,
}

/// Encode a single transaction for multisend
fn encode_packed_transaction(tx: &SafeTransaction) -> Vec<u8> {
    let mut packed = Vec::new();
    // operation (1 byte)
    packed.push(tx.operation as u8);
    // to address (20 bytes)
    packed.extend_from_slice(tx.to.as_slice());
    // value (32 bytes)
    packed.extend_from_slice(&tx.value.to_be_bytes::<32>());
    // data length (32 bytes)
    let data_len = U256::from(tx.data.len());
    packed.extend_from_slice(&data_len.to_be_bytes::<32>());
    // data (variable)
    packed.extend_from_slice(&tx.data);
    packed
}

/// Encode multisend calldata from multiple Safe transactions
pub fn encode_multisend(transactions: &[SafeTransaction]) -> Bytes {
    let mut packed_txs = Vec::new();
    for tx in transactions {
        packed_txs.extend(encode_packed_transaction(tx));
    }

    let call = IMultiSend::multiSendCall {
        transactions: Bytes::from(packed_txs),
    };
    Bytes::from(call.abi_encode())
}

/// Create an aggregated Safe transaction (single or multisend)
pub fn aggregate_transaction(
    transactions: Vec<SafeTransaction>,
    multisend_address: Address,
) -> SafeTransaction {
    if transactions.len() == 1 {
        transactions.into_iter().next().unwrap()
    } else {
        SafeTransaction {
            to: multisend_address,
            value: U256::ZERO,
            data: encode_multisend(&transactions),
            operation: OperationType::DelegateCall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn test_encode_erc20_approve() {
        let spender = address!("4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E");
        let amount = U256::MAX;
        let data = encode_erc20_approve(spender, amount);
        assert!(!data.is_empty());
        // approve(address,uint256) selector is 0x095ea7b3
        assert_eq!(&data[0..4], &[0x09, 0x5e, 0xa7, 0xb3]);
    }

    #[test]
    fn test_encode_split() {
        let collateral = USDC_ADDRESS;
        let condition_id = FixedBytes::ZERO;
        let amount = U256::from(1_000_000u64); // 1 USDC
        let data = encode_split(collateral, condition_id, amount);
        assert!(!data.is_empty());
        // splitPosition selector is 0x72ce4275
        assert_eq!(&data[0..4], &[0x72, 0xce, 0x42, 0x75]);
    }

    #[test]
    fn test_encode_merge() {
        let collateral = USDC_ADDRESS;
        let condition_id = FixedBytes::ZERO;
        let amount = U256::from(1_000_000u64);
        let data = encode_merge(collateral, condition_id, amount);
        assert!(!data.is_empty());
        // mergePositions selector is 0x9e7212ad
        assert_eq!(&data[0..4], &[0x9e, 0x72, 0x12, 0xad]);
    }

    #[test]
    fn test_encode_redeem() {
        let collateral = USDC_ADDRESS;
        let condition_id = FixedBytes::ZERO;
        let data = encode_redeem(collateral, condition_id);
        assert!(!data.is_empty());
        // redeemPositions(address,bytes32,bytes32,uint256[]) selector is 0x01b7037c
        assert_eq!(&data[0..4], &[0x01, 0xb7, 0x03, 0x7c]);
    }
}
