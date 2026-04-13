//! ABI definitions for Polymarket contracts using alloy sol! macro

use alloy_sol_types::sol;

// ERC20 interface
sol! {
    #[derive(Debug)]
    interface IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function transfer(address to, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256);
    }
}

// ERC1155 interface
sol! {
    #[derive(Debug)]
    interface IERC1155 {
        function setApprovalForAll(address operator, bool approved) external;
        function isApprovedForAll(address account, address operator) external view returns (bool);
        function balanceOf(address account, uint256 id) external view returns (uint256);
        function safeTransferFrom(address from, address to, uint256 id, uint256 amount, bytes calldata data) external;
    }
}

// Conditional Tokens Framework (CTF) interface
sol! {
    #[derive(Debug)]
    interface IConditionalTokens {
        function splitPosition(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata partition,
            uint256 amount
        ) external;

        function mergePositions(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata partition,
            uint256 amount
        ) external;

        function redeemPositions(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata indexSets
        ) external;

        function balanceOf(address owner, uint256 id) external view returns (uint256);
        function getCollectionId(bytes32 parentCollectionId, bytes32 conditionId, uint256 indexSet) external view returns (bytes32);
        function getPositionId(address collateralToken, bytes32 collectionId) external pure returns (uint256);
    }
}

// Neg Risk Adapter interface
sol! {
    #[derive(Debug)]
    interface INegRiskAdapter {
        // Split with 5 params (compatible with CTF interface)
        function splitPosition(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata partition,
            uint256 amount
        ) external;

        // Split with 2 params (simplified)
        function splitPosition(bytes32 conditionId, uint256 amount) external;

        // Merge with 5 params (compatible with CTF interface)
        function mergePositions(
            address collateralToken,
            bytes32 parentCollectionId,
            bytes32 conditionId,
            uint256[] calldata partition,
            uint256 amount
        ) external;

        // Merge with 2 params (simplified)
        function mergePositions(bytes32 conditionId, uint256 amount) external;

        // Redeem positions
        function redeemPositions(bytes32 conditionId, uint256[] calldata amounts) external;

        // Convert positions between YES/NO tokens
        function convertPositions(bytes32 marketId, uint256 indexSet, uint256 amount) external;

        function balanceOf(address owner, uint256 id) external view returns (uint256);
        function getPositionId(bytes32 questionId, bool outcome) external view returns (uint256);
    }
}

// Safe (Gnosis Safe) interface
sol! {
    #[derive(Debug)]
    interface ISafe {
        function nonce() external view returns (uint256);

        function getTransactionHash(
            address to,
            uint256 value,
            bytes calldata data,
            uint8 operation,
            uint256 safeTxGas,
            uint256 baseGas,
            uint256 gasPrice,
            address gasToken,
            address refundReceiver,
            uint256 _nonce
        ) external view returns (bytes32);

        function execTransaction(
            address to,
            uint256 value,
            bytes calldata data,
            uint8 operation,
            uint256 safeTxGas,
            uint256 baseGas,
            uint256 gasPrice,
            address gasToken,
            address refundReceiver,
            bytes calldata signatures
        ) external payable returns (bool);

        function getOwners() external view returns (address[] memory);
        function getThreshold() external view returns (uint256);
    }
}

// Multisend interface
sol! {
    #[derive(Debug)]
    interface IMultiSend {
        function multiSend(bytes calldata transactions) external payable;
    }
}
