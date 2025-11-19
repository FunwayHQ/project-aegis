# AEGIS Testing Guide

## Overview

AEGIS maintains comprehensive test coverage across all components to ensure reliability and security. This guide covers testing strategies, running tests, and contributing new tests.

## Test Categories

### 1. Unit Tests (Fast, No External Dependencies)

**HTTP Server Unit Tests** (`node/src/server.rs`, `node/src/config.rs`)
- Test individual functions in isolation
- Mock external dependencies
- Run in milliseconds
- Coverage: 100% of public APIs

**Run**:
```bash
cd node
cargo test --lib
```

**Example Tests**:
- Configuration validation
- Request routing logic
- Response formatting
- Error handling

### 2. Integration Tests (Medium Speed, Server Required)

**HTTP Server Integration** (`node/tests/integration_test.rs`)
- Test full request/response cycle
- Verify HTTP protocol compliance
- Check concurrent request handling
- Measure performance baselines

**Run**:
```bash
cd node
# Start server in background
cargo run &
SERVER_PID=$!
sleep 2

# Run tests
cargo test --test integration_test

# Stop server
kill $SERVER_PID
```

**Test Scenarios**:
- Health check endpoint returns valid JSON
- Metrics endpoint provides correct data
- 404 handling for unknown routes
- Concurrent request stress test (10+ simultaneous)
- Performance: <10ms average latency for local requests

### 3. Smart Contract Tests (Slow, Blockchain Required)

**Token Program Tests** (`contracts/token/tests/`)

**Basic Tests** (`aegis-token.ts`):
- Mint initialization
- Token minting with supply cap
- Transfers between accounts
- Burn mechanism
- Error cases (invalid decimals, zero amounts)

**Advanced Tests** (`advanced-scenarios.ts`):
- Security: Unauthorized minting attempts
- Supply cap enforcement (exact 1B limit)
- Multi-user scenarios
- Tokenomics simulation (distribution, fee burns)
- Event emission verification
- Gas cost measurements

**Run**:
```bash
cd contracts/token

# Install dependencies
npm install

# Run all tests (starts local validator automatically)
anchor test

# Run specific test file
anchor test -- --grep "Security Tests"

# Run without local validator (uses Devnet/Testnet)
anchor test --skip-local-validator
```

**Coverage**:
- 6 basic test cases
- 15+ advanced scenarios
- Total: 20+ test cases for token program

## Test Statistics

### HTTP Server (Rust)

| Metric | Value |
|--------|-------|
| Unit tests | 14 |
| Integration tests | 5 |
| Total coverage | ~95% |
| Test execution time | <3 seconds |
| Lines of test code | ~300 |

### Token Program (Solana)

| Metric | Value |
|--------|-------|
| Basic tests | 6 |
| Advanced tests | 15 |
| Total coverage | ~90% |
| Test execution time | ~30 seconds (local validator) |
| Lines of test code | ~500 |

## Running All Tests

### Quick Test (Unit only - 3 seconds)
```bash
# HTTP server
cd node && cargo test --lib

# Smart contracts (compile only)
cd ../contracts/token && anchor build
```

### Full Test Suite (~2 minutes)
```bash
# Run comprehensive test script
./scripts/test-all.sh
```

Or manually:
```bash
# 1. Test Rust node
cd node
cargo test

# 2. Test Solana contracts
cd ../contracts/token
anchor test

# 3. Verify builds
cargo build --release
anchor build
```

## CI/CD Pipeline

GitHub Actions runs automatically on push/PR:

**Workflow** (`.github/workflows/ci.yml`):
1. **Rust Node Tests**:
   - Format check (`cargo fmt`)
   - Linter (`cargo clippy`)
   - Unit tests
   - Integration tests
   - Release build

2. **Solana Contract Tests**:
   - Anchor build
   - All test suites
   - Deploy check

3. **Security Audit**:
   - `cargo audit` (dependency vulnerabilities)
   - `cargo deny` (license compliance)

4. **Release Artifacts**:
   - Build binaries for deployment
   - Upload to GitHub Artifacts

**Status Badge**: Add to README.md
```markdown
[![CI Status](https://github.com/aegis-network/aegis/workflows/AEGIS%20CI%2FCD/badge.svg)](https://github.com/aegis-network/aegis/actions)
```

## Writing New Tests

### HTTP Server Tests

**Unit Test Example** (`node/src/server.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_feature() {
        let req = create_request(Method::GET, "/new-endpoint");
        let response = handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

**Integration Test Example** (`node/tests/`):
```rust
#[tokio::test]
async fn test_new_integration() {
    let client = create_client();
    let req = Request::builder()
        .uri("http://127.0.0.1:8080/new-endpoint")
        .body(Body::empty())
        .unwrap();

    let response = client.request(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

### Smart Contract Tests

**Basic Test** (`contracts/token/tests/aegis-token.ts`):
```typescript
it("Tests new token feature", async () => {
  const result = await program.methods
    .newInstruction(params)
    .accounts({ /* ... */ })
    .rpc();

  expect(result).to.exist;
});
```

**Advanced Test** (`contracts/token/tests/advanced-scenarios.ts`):
```typescript
describe("New Feature Suite", () => {
  it("Tests complex scenario", async () => {
    // Setup
    const setup = await prepareTest();

    // Execute
    const result = await complexOperation();

    // Verify
    expect(result).to.satisfy(condition);
  });
});
```

## Test Data & Fixtures

### Mock Wallets (Devnet Only!)

```typescript
// contracts/token/tests/helpers.ts
export const TEST_WALLETS = {
  nodeOperator: Keypair.generate(),
  serviceConsumer: Keypair.generate(),
  daoTreasury: Keypair.generate(),
};
```

### Mock Configurations

```toml
# node/tests/fixtures/test-config.toml
[server]
host = "127.0.0.1"
port = 8888
max_connections = 100

[cache]
enabled = false
default_ttl = 60
max_size_mb = 128
```

## Performance Benchmarks

### Baseline Metrics (Sprint 1)

**HTTP Server**:
- Average latency: <10ms (local)
- Throughput: >1000 req/sec (single thread)
- Memory usage: <50MB (idle)

**Token Program**:
- Mint transaction: <0.001 SOL (~$0.00025)
- Transfer transaction: <0.0005 SOL (~$0.00012)
- Burn transaction: <0.0005 SOL

### Running Benchmarks

```bash
# HTTP server performance
cd node
cargo test test_server_performance_baseline -- --nocapture

# Token program gas costs
cd contracts/token
anchor test -- --grep "Gas Optimization"
```

## Code Coverage

### Generate Coverage Report (Rust)

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate HTML coverage report
cd node
cargo tarpaulin --out Html --output-dir coverage

# Open coverage/index.html in browser
```

### Target Coverage

| Component | Target | Current |
|-----------|--------|---------|
| HTTP Server | 90% | ~95% |
| Config Module | 90% | ~100% |
| Token Program | 85% | ~90% |

## Test-Driven Development (TDD)

For new features, follow TDD:

1. **Write failing test**:
   ```rust
   #[test]
   fn test_new_feature() {
       let result = new_feature();
       assert_eq!(result, expected);
   }
   ```

2. **Run test** (should fail):
   ```bash
   cargo test test_new_feature
   ```

3. **Implement feature** until test passes

4. **Refactor** while keeping tests green

## Continuous Testing During Development

### Watch Mode (Auto-run on file changes)

```bash
# Install cargo-watch
cargo install cargo-watch

# Watch for changes and run tests
cd node
cargo watch -x test

# Watch and also check/lint
cargo watch -x check -x test -x clippy
```

## Security Testing

### Fuzzing (Advanced)

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Create fuzz target
cd node
cargo fuzz init

# Run fuzzer
cargo fuzz run fuzz_target_1
```

### Smart Contract Audit Preparation

Before external audits:
1. ✅ 100% test coverage on critical functions
2. ✅ All clippy warnings resolved
3. ✅ Security-focused tests (unauthorized access, overflow)
4. ✅ Gas optimization tests
5. ✅ Multi-user interaction tests
6. ✅ Event emission verification

## Troubleshooting Tests

### "Test failed: server not running"
Integration tests need server running:
```bash
# Terminal 1
cargo run

# Terminal 2
cargo test --test integration_test
```

### "Insufficient funds" in Solana tests
```bash
# Fund test wallet
solana airdrop 5
```

### Tests timeout on Anchor
Increase timeout in `Anchor.toml`:
```toml
[scripts]
test = "yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/**/*.ts"
#                                             ^ 1 million ms = 16+ minutes
```

### Flaky tests
Use `--test-threads=1` to run serially:
```bash
cargo test -- --test-threads=1
```

## Best Practices

1. **Fast Feedback**: Unit tests should run in <1 second
2. **Isolation**: Each test independent (no shared state)
3. **Determinism**: Same input → same result (no random failures)
4. **Clear Names**: `test_mint_rejects_zero_amount` not `test_case_1`
5. **Arrange-Act-Assert**: Setup, execute, verify (clear structure)
6. **Edge Cases**: Test boundaries (zero, max, negative)
7. **Error Paths**: Test failure scenarios, not just happy path

## Contributing Tests

When submitting PRs:
- ✅ Add tests for all new features
- ✅ Maintain >85% coverage
- ✅ All tests must pass in CI
- ✅ No clippy warnings
- ✅ Formatted with `cargo fmt`

## Future Testing (Upcoming Sprints)

**Sprint 2+**:
- Load testing (Apache Bench, wrk)
- Chaos engineering (random node failures)
- P2P network simulation
- eBPF/XDP kernel tests
- Wasm module isolation tests
- Multi-node distributed tests

**Sprint 4+**:
- End-to-end testing (browser automation)
- Security penetration testing
- Third-party audit results
- Formal verification (Certora)

---

**Test early, test often! 🧪**
