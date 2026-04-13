# Galpha Proto

Protocol Buffers definitions for Galpha services, with automatic Rust and TypeScript code generation.

## Overview

This crate provides shared protobuf definitions for the Galpha ecosystem. It uses [prost](https://github.com/tokio-rs/prost) for generating Rust structs and [pbjson](https://github.com/influxdata/pbjson) for JSON serialization support. For TypeScript/JavaScript projects, it uses [protobuf.js](https://github.com/protobufjs/protobuf.js) for code generation.

## Features

- 🔧 **Type-safe API definitions** - Protocol buffers ensure type safety across services
- 📦 **JSON serialization** - All types support serialization in both Rust and TypeScript
- 🏗️ **Automatic code generation** - Code is generated at build time from `.proto` files
- 📚 **Hierarchical module structure** - Clean, organized access to types via `service::module::Type`
- 🌐 **Multi-language support** - Works seamlessly with Rust and TypeScript/JavaScript

## Module Structure

```
galpha_proto
└── ledger
    └── http
        ├── GetBalanceRequest
        ├── GetBalanceResponse
        ├── StakeRequest
        ├── StakeResponse
        ├── CreateUnstakingRequest
        ├── CreateUnstakingResponse
        ├── CreateWithdrawalRequest
        ├── CreateWithdrawalResponse
        ├── Transaction
        ├── GetUserTransactionsRequest
        ├── GetUserTransactionsResponse
        ├── PaginationInfo
        ├── CreatePendingDepositRequest
        ├── CreatePendingDepositResponse
        ├── PendingDeposit
        ├── GetPendingDepositsRequest
        ├── GetPendingDepositsResponse
        ├── GetQuoteRequest
        ├── GetQuoteResponse
        ├── GetTvlRequest
        ├── GetTvlResponse
        └── ErrorResponse
```

## Usage

### Rust Usage

#### Adding as a dependency

Add to your `Cargo.toml`:

```toml
[dependencies]
galpha-proto = { path = "../galpha-proto" }
```

#### Using in code

```rust
use galpha_proto::ledger::http::{
    GetBalanceRequest,
    GetBalanceResponse,
    StakeRequest,
    StakeResponse,
};
use serde_json;

// Create a request
let request = GetBalanceRequest {};

// Create a response
let response = GetBalanceResponse {
    user_id: "user123".to_string(),
    trading_balance: "1000.50".to_string(),
    staked_balance: "500.00".to_string(),
    total_balance: "1500.50".to_string(),
    yield_accumulated: "10.25".to_string(),
    version: 1,
};

// Serialize to JSON
let json = serde_json::to_string(&response).unwrap();
println!("{}", json);

// Deserialize from JSON
let parsed: GetBalanceResponse = serde_json::from_str(&json).unwrap();
```

### TypeScript/JavaScript Usage

#### Installation

Using npm:
```bash
npm install @galpha/proto
```

Using pnpm:
```bash
pnpm add @galpha/proto
```

Or from local path in package.json:
```json
{
  "dependencies": {
    "@galpha/proto": "file:../galpha-proto"
  }
}
```

#### Using in TypeScript code

```typescript
import {
  GetBalanceRequest,
  GetBalanceResponse,
  StakeRequest,
  StakeResponse,
  Root
} from '@galpha/proto';

// Create a message using the constructor
const request = Root.ledger.v1.GetBalanceRequest.create({});

// Create a response
const response = Root.ledger.v1.GetBalanceResponse.create({
  userId: "user123",
  tradingBalance: "1000.50",
  stakedBalance: "500.00",
  totalBalance: "1500.50",
  yieldAccumulated: "10.25",
  version: 1
});

// Serialize to JSON
const json = Root.ledger.v1.GetBalanceResponse.toObject(response);
console.log(json);

// Deserialize from JSON
const parsed = Root.ledger.v1.GetBalanceResponse.fromObject(json);

// Encode to binary (for network transmission)
const buffer = Root.ledger.v1.GetBalanceResponse.encode(response).finish();

// Decode from binary
const decoded = Root.ledger.v1.GetBalanceResponse.decode(buffer);
```

#### Using with HTTP APIs

```typescript
import { GetBalanceResponse, Root } from '@galpha/proto';
import axios from 'axios';

// Fetch from API
const response = await axios.get('/api/balance');

// Parse the JSON response
const balance = Root.ledger.v1.GetBalanceResponse.fromObject(response.data);

console.log(`Total balance: ${balance.totalBalance}`);
console.log(`Trading balance: ${balance.tradingBalance}`);
console.log(`Staked balance: ${balance.stakedBalance}`);
```

## API Reference

### Balance APIs

#### GetBalanceResponse
```rust
pub struct GetBalanceResponse {
    pub user_id: String,
    pub trading_balance: String,    // Available balance
    pub staked_balance: String,      // Staked amount
    pub total_balance: String,       // Total = trading + staked
    pub yield_accumulated: String,   // Accumulated yield
    pub version: i32,                // Balance version for optimistic locking
}
```

### Staking APIs

#### StakeResponse
```rust
pub struct StakeResponse {
    pub success: bool,
    pub status: String,
    pub message: String,
    pub transaction_id: String,  // UUID
}
```

#### CreateUnstakingResponse
```rust
pub struct CreateUnstakingResponse {
    pub request_id: String,      // UUID
    pub unlock_at: String,       // RFC3339 timestamp
    pub transaction_id: String,  // UUID
}
```

### Withdrawal APIs

#### CreateWithdrawalResponse
```rust
pub struct CreateWithdrawalResponse {
    pub success: bool,
    pub withdrawal_id: String,           // UUID
    pub transaction_id: String,          // UUID
    pub asset: String,                   // "usdc" or "usdt"
    pub requested_amount: String,
    pub fee_amount: String,
    pub net_amount: String,
    pub actual_withdrawal_amount: String,
    pub new_trading_balance: String,
    pub destination_address: String,
    pub status: String,
}
```

### Transaction APIs

#### Transaction
```rust
pub struct Transaction {
    pub id: String,                     // UUID
    pub user_id: Option<String>,
    pub transaction_type: String,       // "deposit", "withdrawal", "stake", etc.
    pub status: String,                 // "pending", "confirmed", "failed"
    pub blockchain_ref: Option<String>, // Transaction hash
    pub chain_id: Option<String>,       // CAIP-2 format (e.g., "eip155:1")
    pub amount: Option<String>,
    pub created_at: String,             // RFC3339 timestamp
    pub updated_at: String,             // RFC3339 timestamp
}
```

#### GetUserTransactionsResponse
```rust
pub struct GetUserTransactionsResponse {
    pub transactions: Vec<Transaction>,
    pub pagination: Option<PaginationInfo>,
}

pub struct PaginationInfo {
    pub page: i32,
    pub limit: i32,
    pub total_count: i64,
    pub total_pages: i32,
    pub has_next: bool,
    pub has_prev: bool,
}
```

### Pending Deposit APIs

#### PendingDeposit
```rust
pub struct PendingDeposit {
    pub id: String,              // UUID
    pub tx_hash: String,
    pub chain_id: String,        // CAIP-2 format
    pub asset: String,
    pub original_amount: String,
    pub aiusd_amount: Option<String>,
    pub auto_stake: bool,
    pub status: String,
    pub created_at: String,      // RFC3339 timestamp
    pub updated_at: String,      // RFC3339 timestamp
}
```

### Other APIs

#### GetTvlResponse
```rust
pub struct GetTvlResponse {
    pub tvl: String,  // Total value locked
}
```

#### GetQuoteResponse
```rust
pub struct GetQuoteResponse {
    pub symbol: String,  // e.g., "BTCUSDT"
    pub price: String,
}
```

## Development

### Project Structure

```
galpha-proto/
├── Cargo.toml              # Rust package configuration
├── package.json            # NPM package configuration
├── tsconfig.json           # TypeScript configuration
├── build.rs                # Rust build script
├── proto/                  # Protocol buffer definitions
│   ├── ledger/v1/
│   │   └── http.proto
│   ├── position_manager/v1/
│   │   └── rpc.proto
│   └── user_service/v1/
│       └── http.proto
├── src/
│   └── lib.rs              # Rust library entry point
├── dist/                   # Generated TypeScript/JavaScript (gitignored)
│   ├── proto.js
│   ├── proto.d.ts
│   └── index.ts
└── scripts/
    └── generate-index.js   # TypeScript code generation helper
```

### Adding New Definitions

1. Edit or create `.proto` files in `proto/` directory
2. **For Rust:**
   - Update `build.rs` to include the new proto files
   - Run `cargo build` to generate Rust code
3. **For TypeScript:**
   - Update `package.json` scripts to include the new proto files
   - Run `npm run generate` to regenerate TypeScript code
4. The generated types will be available under the corresponding module path

### Build Process

#### Rust Build Process

The Rust build process is handled by `build.rs`:

1. **prost-build** compiles `.proto` files to Rust structs
2. **pbjson-build** generates `serde::Serialize` and `serde::Deserialize` implementations
3. Generated code is placed in `$OUT_DIR` and included via `include!()` macro

#### TypeScript Build Process

The TypeScript build process uses npm scripts:

1. **pbjs** compiles `.proto` files to JavaScript code
2. **pbts** generates TypeScript type definitions from the JavaScript code
3. **generate-index.js** creates convenient re-exports and type aliases
4. Generated code is placed in `dist/` directory

### Regenerating Code

#### Rust
```bash
# Clean and rebuild
cargo clean
cargo build

# Check generated code (optional)
cargo expand
```

#### TypeScript
```bash
# Install dependencies
npm install

# Generate TypeScript code
npm run generate

# Or step by step:
npm run clean           # Remove old generated files
npm run generate:js     # Generate JavaScript
npm run generate:ts     # Generate TypeScript definitions
```

## Notes

- All numeric amounts are represented as `String` to preserve precision (uses Decimal in actual implementation)
- UUIDs are represented as `String` in protobuf
- Timestamps use RFC3339 string format for JSON compatibility
- Chain IDs follow CAIP-2 format (e.g., `eip155:1` for Ethereum mainnet)
- All types automatically support JSON serialization/deserialization via `serde`

## Version

Current version: 0.1.0

## License

MIT
