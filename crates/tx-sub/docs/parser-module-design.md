# Parser Module Design

This document explains how the parser module works and its design patterns. We'll use Bonk as an example to illustrate the concepts.

## Architecture Overview

The parser uses a **modular, protocol-specific approach** rather than traditional interfaces. Each DEX has its own parser module that handles protocol-specific transaction parsing logic.

### Core Components

1. **Main Parser** (`src/parser/parser.rs`) - Routes transactions to protocol-specific parsers
2. **Protocol Parsers** (`src/parser/{protocol}/`) - Handle protocol-specific parsing logic  
3. **Trade Log Unification** (`src/parser/market.rs`) - Unified output format for all trades

## How It Works

### 1. Transaction Routing

The main parser receives Solana transactions and routes them based on program ID:

```rust
if program_id == BONK_ID {
    if let Some(bonk_swap) = bonk::process_bonk(ctx, ix) {
        let trade = MarketTrade {
            slot: ctx.bctx.cur_slot,
            signature: sig.to_string(),
            log: crate::parser::TradeLog::Bonk(bonk_swap),
        };
        self.trade_tx.send(trade);
    }
}
```

### 2. Protocol-Specific Parsing

Each DEX has its own parser function with a standard signature:

```rust
pub fn process_bonk<'a>(
    ctx: &mut TxnDecodeCtx<'a>,
    ix: (usize, &'a CompiledInstruction),
) -> Option<BonkSwap>
```

### 3. Trade Log Unification

All parsed trades are wrapped in a common structure:

```rust
pub struct MarketTrade {
    pub slot: u64,
    pub signature: String,
    pub log: TradeLog,
}

pub enum TradeLog {
    Pumpfun(PumpfunLog),
    RayAMMv4(RayAMMv4Swap),
    Bonk(BonkSwap),
    // ... other protocols
}
```

## Design Principles

1. **Function-based Protocol Handlers**: Each protocol has a `process_*` function with consistent signature
2. **Enum-based Type System**: `TradeLog` enum unifies different trade types
3. **Protocol Independence**: Each parser handles its own data formats and logic
4. **Type Safety**: Rust's enum system ensures all trade types are handled
5. **Performance**: Direct function calls without virtual dispatch overhead

## Bonk Example

The Bonk parser demonstrates the typical pattern:

### Structure
- `src/parser/bonk/parser.rs` - Main parsing logic
- `src/parser/bonk/types.rs` - Data structures
- `src/parser/bonk/events.rs` - Event definitions and discriminators

### Parser Logic
1. **Discriminator Check**: Validates instruction discriminators to identify trade types
2. **Account Extraction**: Maps instruction accounts to meaningful addresses
3. **Event Parsing**: Searches inner instructions for trade events
4. **Data Deserialization**: Uses Borsh to parse protocol-specific event data
5. **Return Structured Data**: Returns a `BonkSwap` object with normalized trade information

## Adding New DEX Support

To add a new DEX:

1. Create a new protocol module under `src/parser/your_dex/`
2. Implement the standard `process_your_dex()` function
3. Add routing logic to the main parser
4. Add your trade type to the `TradeLog` enum

Each protocol module should follow the same patterns as existing implementations like Bonk, Raydium, or Meteora.