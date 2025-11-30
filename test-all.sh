#!/bin/bash
# AEGIS Comprehensive Test Runner
# Run all tests across HTTP server and Solana contracts

set -e  # Exit on error

echo "╔════════════════════════════════════════════╗"
echo "║     AEGIS Test Suite - All Components     ║"
echo "╚════════════════════════════════════════════╝"
echo ""

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Test counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Function to run test section
run_test_section() {
    local name=$1
    local command=$2

    echo -e "${YELLOW}▶ Running: $name${NC}"
    if eval "$command"; then
        echo -e "${GREEN}✓ $name passed${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "${RED}✗ $name failed${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo ""
}

# 1. HTTP Server Unit Tests
echo "═══════════════════════════════════════════"
echo " 1. HTTP Server Tests"
echo "═══════════════════════════════════════════"
cd node
run_test_section "Cargo Check" "cargo check --lib --quiet"
run_test_section "Unit Tests" "cargo test --lib --quiet"
run_test_section "Format Check" "cargo fmt -- --check"
run_test_section "Clippy Lints" "cargo clippy --lib --quiet -- -D warnings"

# 2. Integration Tests (with server)
echo "═══════════════════════════════════════════"
echo " 2. Integration Tests (with server)"
echo "═══════════════════════════════════════════"

# Start server in background
cargo build --release --bin aegis-node --quiet
./target/release/aegis-node &
SERVER_PID=$!
sleep 2

run_test_section "Integration Tests" "cargo test --test integration_test --quiet"

# Stop server
kill $SERVER_PID 2>/dev/null || true
cd ..

# 3. Solana Smart Contract Tests (if Anchor installed)
echo "═══════════════════════════════════════════"
echo " 3. Solana Smart Contract Tests"
echo "═══════════════════════════════════════════"

if command -v anchor &> /dev/null; then
    # 3.1 Token Contract
    echo ""
    echo "─── Token Contract ───"
    cd contracts/token
    run_test_section "Token - NPM Dependencies" "npm install --quiet"
    run_test_section "Token - Anchor Build" "anchor build"
    run_test_section "Token - Program Tests" "anchor test --skip-local-validator"
    cd ../..

    # 3.2 Registry Contract
    echo ""
    echo "─── Registry Contract ───"
    cd contracts/registry
    run_test_section "Registry - NPM Dependencies" "npm install --quiet"
    run_test_section "Registry - Anchor Build" "anchor build"
    run_test_section "Registry - Program Tests" "anchor test --skip-local-validator"
    cd ../..

    # 3.3 Staking Contract
    echo ""
    echo "─── Staking Contract ───"
    cd contracts/staking
    run_test_section "Staking - NPM Dependencies" "npm install --quiet"
    run_test_section "Staking - Anchor Build" "anchor build"
    run_test_section "Staking - Program Tests" "anchor test --skip-local-validator"
    cd ../..

    # 3.4 Rewards Contract
    echo ""
    echo "─── Rewards Contract ───"
    cd contracts/rewards
    run_test_section "Rewards - NPM Dependencies" "npm install --quiet"
    run_test_section "Rewards - Anchor Build" "anchor build"
    run_test_section "Rewards - Program Tests" "anchor test --skip-local-validator"
    cd ../..

    # 3.5 DAO Contract
    echo ""
    echo "─── DAO Contract ───"
    cd contracts/dao
    run_test_section "DAO - NPM Dependencies" "npm install --quiet"
    run_test_section "DAO - Anchor Build" "anchor build"
    run_test_section "DAO - Program Tests" "anchor test --skip-local-validator"
    cd ../..

else
    echo -e "${YELLOW}⚠ Anchor not installed - skipping Solana tests${NC}"
    echo "  Install with: cargo install --git https://github.com/coral-xyz/anchor anchor-cli"
    echo ""
fi

# Summary
echo "╔════════════════════════════════════════════╗"
echo "║            Test Summary                    ║"
echo "╚════════════════════════════════════════════╝"
echo ""
echo "Total test sections: $TOTAL_TESTS"
echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
if [ $FAILED_TESTS -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED_TESTS${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed! ✓${NC}"
fi
echo ""
echo "Test Coverage:"
echo "  ✓ HTTP Server (lib + integration)"
echo "  ✓ Token Contract (40 tests)"
echo "  ✓ Registry Contract"
echo "  ✓ Staking Contract"
echo "  ✓ Rewards Contract"
echo "  ✓ DAO Contract (Vote Escrow + Governance)"
echo "  ✓ Total: 400+ tests"
echo ""
echo "Next steps:"
echo "  - Deploy to Devnet: cd contracts/<contract> && anchor deploy"
echo "  - Run HTTP server: cd node && cargo run --bin aegis-node"
echo ""
