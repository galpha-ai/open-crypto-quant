# Bonk Parser E2E Test Guide

## Overview

The e2e_test binary is designed to test the Bonk parser with real blockchain data by connecting to a live gRPC endpoint and capturing actual Bonk transactions. It verifies that the parser correctly handles the raw event data format used by Bonk transactions.

## Prerequisites

Before running the test, ensure you have:

1. **Rust development environment** set up
2. **Access to a gRPC endpoint** (e.g., Yellowstone/Fountainhead)
3. **gRPC authentication token** (if required by your endpoint)

## Configuration

Example config: config/config.e2e_test.yaml

## Running the Test

### Basic Usage

```bash
# Build the test binary
cargo build --bin e2e_test

# Run with default config (config/config.e2e_test.yaml)
cargo run --bin e2e_test

# Run with custom config file
cargo run --bin e2e_test -- path/to/custom_config.yaml
```

### What the Test Does

1. Connects to the configured gRPC endpoint
2. Subscribes to transactions for the Bonk program ID
3. Waits for and parses incoming Bonk transactions
4. Prints detailed information about each parsed trade
5. Automatically exits after receiving the configured number of events


## Troubleshooting

### No Events Received

If the test runs but doesn't receive any events:

1. **Check Bonk trading activity**: Bonk might not have trades during your test window
2. **Verify endpoint connectivity**: Ensure your gRPC endpoint is accessible
3. **Check authentication**: Verify your auth token is valid and properly configured
4. **Monitor logs**: The test outputs JSON logs with detailed connection information

### Connection Errors

If you get connection errors:

```bash
# Enable debug logging
RUST_LOG=debug cargo run --bin e2e_test
```

Common issues:
- Invalid or expired auth token
- Firewall blocking gRPC connections
- Incorrect endpoint URL

### Parser Errors

If the parser fails to decode transactions:

1. Check the logs for the specific error message
2. The test will print raw transaction data when parser errors occur
3. Report issues with the transaction signature for debugging

## Interpreting Results

### Success Indicators

- Test receives and prints the configured number of Bonk events
- Each event contains valid trade data (amounts, addresses, timestamps)
- No parser errors in the logs
- Test exits cleanly after reaching event count

### What This Verifies

1. **gRPC Connection**: Successfully connects and subscribes to blockchain data
2. **Transaction Filtering**: Correctly identifies Bonk program transactions
3. **Parser Functionality**: Properly decodes Bonk's raw event data format
4. **Data Integrity**: All expected fields are present and valid

## Advanced Usage

### Continuous Monitoring

To run continuously without event limit:

1. Set `event_count` to a very high number (e.g., 999999)
2. Use Ctrl+C to stop when desired

### Debugging Parser Issues

For detailed parser debugging:

```bash
# Maximum verbosity
RUST_LOG=tx_sub=trace cargo run --bin e2e_test
```

### Performance Testing

To test high-volume scenarios:

1. Run during active Bonk trading periods
2. Monitor memory usage and processing latency
3. Check for "lagged" messages indicating missed events

## Notes

- The test bypasses Redis completely, focusing only on gRPC subscription and parsing
- No metrics server is started in test mode
- Transaction persistence is disabled
- The test is designed to be run manually for debugging and verification
- Results should match the format expected by downstream consumers
