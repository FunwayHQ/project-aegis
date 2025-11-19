#!/bin/bash
# AEGIS Comprehensive Test Runner
# Run all tests across HTTP server and Solana contracts

set -e  # Exit on error

echo "╔════════════════════════════════════════════╗"
echo "║     AEGIS Test Suite - Sprint 1            ║"
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
run_test_section "Cargo Check" "cargo check --quiet"
run_test_section "Unit Tests" "cargo test --lib --quiet"
run_test_section "Format Check" "cargo fmt -- --check"
run_test_section "Clippy Lints" "cargo clippy --quiet -- -D warnings"

# 2. Integration Tests (with server)
echo "═══════════════════════════════════════════"
echo " 2. Integration Tests (with server)"
echo "═══════════════════════════════════════════"

# Start server in background
cargo build --release --quiet
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
    cd contracts/token

    run_test_section "NPM Dependencies" "npm install --quiet"
    run_test_section "Anchor Build" "anchor build"
    run_test_section "Token Program Tests" "anchor test --skip-local-validator"
    run_test_section "Advanced Scenarios" "anchor test --skip-local-validator -- --grep 'advanced'"

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
echo "Next steps:"
echo "  - Deploy to Devnet: cd contracts/token && anchor deploy"
echo "  - Run HTTP server: cd node && cargo run"
echo ""
