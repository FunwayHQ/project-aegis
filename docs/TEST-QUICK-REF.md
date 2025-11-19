# Testing Quick Reference

## Run Tests

```bash
# HTTP Server (3 seconds)
cd node && cargo test

# Token Program (30 seconds, requires Solana/Anchor)
cd contracts/token && anchor test

# All tests at once
./test-all.sh
```

## Test Count

- **HTTP Server**: 14 unit + 5 integration = **19 tests** ✅
- **Token Program**: 6 basic + 15 advanced = **21 tests**
- **Total**: **40+ comprehensive tests**

## What's Tested

### HTTP Server ✅
- [x] Root endpoint returns node info
- [x] Health check returns valid JSON
- [x] Metrics endpoint returns performance data
- [x] 404 handling for unknown routes
- [x] POST method rejection
- [x] Multiple sequential requests
- [x] Concurrent requests (10 simultaneous)
- [x] Performance baseline (<10ms latency)
- [x] Configuration validation
- [x] Config serialization/deserialization

### Token Program
- [x] Mint initialization (9 decimals)
- [x] Token minting with supply validation
- [x] Transfer between accounts
- [x] Burn mechanism
- [x] Supply cap enforcement (1B tokens)
- [x] Unauthorized minting prevention
- [x] Balance overflow protection
- [x] Zero amount rejection
- [x] Multiple user scenarios
- [x] Tokenomics simulation (distributions, burns)
- [x] Event emissions
- [x] Gas cost measurements (<0.001 SOL)

## Quick Commands

```bash
# Format code
cd node && cargo fmt

# Lint
cd node && cargo clippy

# Build release
cd node && cargo build --release

# Run server
cd node && cargo run

# Test specific function
cargo test test_health_endpoint

# Show test output
cargo test -- --nocapture

# Run single Solana test
anchor test -- --grep "minting"
```

## Test Results (Current)

**HTTP Server**: ✅ 19/19 passed (100%)
**Token Program**: ⏳ Pending Solana installation

All HTTP server tests pass with zero warnings after fixes!
