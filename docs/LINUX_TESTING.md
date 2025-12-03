# AEGIS Linux Testing Guide

Comprehensive guide for running the full AEGIS test suite on Linux systems. This document covers all testing scenarios including unit tests, integration tests, smart contract tests, eBPF/XDP testing, P2P networking, and performance benchmarks.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [System Requirements](#system-requirements)
3. [Environment Setup](#environment-setup)
4. [Quick Start](#quick-start)
5. [Test Categories](#test-categories)
   - [Rust Node Tests](#1-rust-node-tests)
   - [Smart Contract Tests](#2-smart-contract-tests)
   - [eBPF/XDP Tests](#3-ebpfxdp-tests)
   - [P2P Networking Tests](#4-p2p-networking-tests)
   - [Wasm Runtime Tests](#5-wasm-runtime-tests)
   - [Integration Tests](#6-integration-tests)
   - [Performance Benchmarks](#7-performance-benchmarks)
6. [CI/CD Integration](#cicd-integration)
7. [Troubleshooting](#troubleshooting)
8. [Test Coverage Report](#test-coverage-report)

---

## Prerequisites

### Required Software

| Software | Version | Purpose |
|----------|---------|---------|
| **Rust** | 1.75+ | Node software, CLI, eBPF |
| **Node.js** | 18+ | Smart contract testing |
| **Anchor CLI** | 0.30+ | Solana smart contracts |
| **Solana CLI** | 1.18+ | Local validator, devnet |
| **NATS Server** | 2.10+ | Distributed state sync |
| **DragonflyDB/Redis** | Latest | Caching layer |
| **IPFS** | 0.22+ | Decentralized module storage |
| **hping3** | 3.0+ | eBPF/DDoS testing |
| **Docker** | 24+ | Optional containerized tests |

### Install Commands (Ubuntu/Debian)

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install build essentials
sudo apt install -y build-essential pkg-config libssl-dev libclang-dev \
    llvm clang linux-headers-$(uname -r) git curl wget

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Add Rust targets for eBPF
rustup target add bpfel-unknown-none
rustup component add rust-src
cargo install bpf-linker

# Install Node.js 18+
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt install -y nodejs

# Install Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

# Install Anchor CLI
cargo install --git https://github.com/coral-xyz/anchor avm --locked
avm install latest
avm use latest

# Install NATS Server
wget https://github.com/nats-io/nats-server/releases/download/v2.10.9/nats-server-v2.10.9-linux-amd64.tar.gz
tar -xzf nats-server-v2.10.9-linux-amd64.tar.gz
sudo mv nats-server-v2.10.9-linux-amd64/nats-server /usr/local/bin/

# Install DragonflyDB (or Redis)
curl -fsSL https://packages.dragonflydb.io/ubuntu/gpg.key | sudo gpg --dearmor -o /etc/apt/trusted.gpg.d/dragonflydb.gpg
echo "deb https://packages.dragonflydb.io/ubuntu $(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/dragonflydb.list
sudo apt update && sudo apt install -y dragonfly
# OR use Redis: sudo apt install -y redis-server

# Install IPFS
wget https://dist.ipfs.tech/kubo/v0.22.0/kubo_v0.22.0_linux-amd64.tar.gz
tar -xzf kubo_v0.22.0_linux-amd64.tar.gz
sudo mv kubo/ipfs /usr/local/bin/
ipfs init

# Install hping3 for DDoS testing
sudo apt install -y hping3

# Install k6 for load testing (optional)
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt update && sudo apt install -y k6

# Install flamegraph for profiling (optional)
cargo install flamegraph
```

### Install Commands (Fedora/RHEL)

```bash
# Install build essentials
sudo dnf install -y gcc gcc-c++ make openssl-devel clang llvm \
    kernel-devel kernel-headers git curl wget

# Install Rust (same as Ubuntu)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Rest follows similar pattern...
```

### Install Commands (Arch Linux)

```bash
# Install build essentials
sudo pacman -S base-devel openssl clang llvm linux-headers git

# Install Rust
sudo pacman -S rustup
rustup default stable

# Install NATS, Redis, etc.
yay -S nats-server redis ipfs-kubo hping
```

---

## System Requirements

### Minimum Requirements

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| **CPU** | 4 cores | 8+ cores |
| **RAM** | 8 GB | 16+ GB |
| **Disk** | 50 GB SSD | 100+ GB NVMe |
| **Kernel** | 5.4+ | 6.1+ (for eBPF) |

### Kernel Requirements for eBPF/XDP

```bash
# Check kernel version
uname -r

# Required kernel configs (check with)
cat /boot/config-$(uname -r) | grep -E "CONFIG_BPF|CONFIG_XDP"

# Expected output includes:
# CONFIG_BPF=y
# CONFIG_BPF_SYSCALL=y
# CONFIG_BPF_JIT=y
# CONFIG_XDP_SOCKETS=y
```

### Network Requirements

- Port 8080: HTTP server
- Port 6379: Redis/DragonflyDB
- Port 4222: NATS messaging
- Port 5001: IPFS API
- Port 8899: Solana validator

---

## Environment Setup

### Clone Repository

```bash
git clone https://github.com/FunwayHQ/project-aegis.git
cd project-aegis
```

### Configure Environment Variables

```bash
# Create .env file
cat > .env << 'EOF'
# Solana Configuration
SOLANA_CLUSTER=localnet
ANCHOR_WALLET=$HOME/.config/solana/id.json

# NATS Configuration
NATS_URL=nats://127.0.0.1:4222

# Redis/DragonflyDB
REDIS_URL=redis://127.0.0.1:6379

# IPFS
IPFS_API=http://127.0.0.1:5001

# Logging
RUST_LOG=info,aegis_node=debug

# Test Configuration
TEST_TIMEOUT=300
AEGIS_TEST_MODE=true
EOF

source .env
```

### Generate Solana Keypair

```bash
# Generate new keypair (if not exists)
solana-keygen new --outfile ~/.config/solana/id.json --no-bip39-passphrase

# Configure for localnet/devnet
solana config set --url localhost  # For localnet
# OR
solana config set --url devnet     # For devnet testing
```

---

## Quick Start

### Run All Tests

```bash
# Run complete test suite
./test-all.sh

# Expected output:
# ╔════════════════════════════════════════════╗
# ║     AEGIS Test Suite - All Components     ║
# ╚════════════════════════════════════════════╝
# ...
# ✓ All tests passed!
```

### Run Individual Test Categories

```bash
# Rust node tests only
cd node && cargo test --lib

# Smart contract tests only
cd contracts/dao && anchor test

# eBPF tests (requires root)
sudo ./node/test-syn-flood.sh
```

---

## Test Categories

### 1. Rust Node Tests

#### Unit Tests

```bash
cd node

# Run all unit tests
cargo test --lib

# Run specific test module
cargo test waf::tests
cargo test cache::tests
cargo test wasm_runtime::tests
cargo test threat_intel_p2p::tests
cargo test challenge::tests
cargo test behavioral_analysis::tests
cargo test api_security::tests
cargo test distributed_enforcement::tests

# Run with verbose output
cargo test --lib -- --nocapture

# Run tests matching pattern
cargo test --lib challenge -- --nocapture
```

#### Test Coverage by Module

| Module | Test Count | Coverage |
|--------|------------|----------|
| `waf.rs` | 17 | WAF rules, SQL injection, XSS |
| `waf_enhanced.rs` | 15 | OWASP CRS, ML anomaly scoring |
| `cache.rs` | 8 | DragonflyDB operations |
| `wasm_runtime.rs` | 24 | Sandbox, host API, signatures |
| `threat_intel_p2p.rs` | 30 | P2P gossip, threat sharing |
| `challenge.rs` | 14 | PoW, browser fingerprinting |
| `behavioral_analysis.rs` | 9 | Mouse/keyboard analysis |
| `api_security.rs` | 14 | JWT, OpenAPI, rate limits |
| `distributed_enforcement.rs` | 17 | IPv6, blocklist sync |
| `route_config.rs` | 12 | YAML config, dispatching |
| `ipfs_client.rs` | 11 | Module distribution |

#### Code Quality Checks

```bash
cd node

# Format check
cargo fmt -- --check

# Linting with Clippy
cargo clippy --lib -- -D warnings

# Security audit
cargo audit

# Dependency check
cargo outdated
```

### 2. Smart Contract Tests

#### Start Local Validator

```bash
# Terminal 1: Start Solana test validator
solana-test-validator --reset

# Wait for validator to be ready
solana logs  # Should show block production
```

#### Run Contract Tests

```bash
# Token Contract (40 tests)
cd contracts/token
npm install
anchor build
anchor test --skip-local-validator

# Registry Contract
cd ../registry
npm install
anchor build
anchor test --skip-local-validator

# Staking Contract
cd ../staking
npm install
anchor build
anchor test --skip-local-validator

# Rewards Contract
cd ../rewards
npm install
anchor build
anchor test --skip-local-validator

# DAO Contract (14 tests + vote escrow)
cd ../dao
npm install
anchor build
anchor test --skip-local-validator
```

#### Contract Test Details

| Contract | Tests | Key Scenarios |
|----------|-------|---------------|
| **Token** | 40 | Mint, burn, transfer, multi-sig |
| **Registry** | 25 | Node registration, metadata |
| **Staking** | 35 | Stake, unstake, slash, timelock |
| **Rewards** | 30 | Performance metrics, distribution |
| **DAO** | 14 | Proposals, voting, treasury |

#### Run All Contract Tests

```bash
# From project root
for contract in token registry staking rewards dao; do
    echo "Testing $contract..."
    cd contracts/$contract
    anchor test --skip-local-validator
    cd ../..
done
```

### 3. eBPF/XDP Tests

> **Note:** eBPF tests require Linux kernel 5.4+ and root privileges.

#### Build eBPF Programs

```bash
cd node/ebpf/syn-flood-filter

# Build for eBPF target
cargo build --release --target bpfel-unknown-none

# Verify build
ls -la target/bpfel-unknown-none/release/*.o
```

#### Run DDoS Protection Tests

```bash
# Must run as root
sudo ./test-syn-flood.sh

# Expected tests:
# 1. Legitimate traffic baseline
# 2. XDP program loading
# 3. Legitimate traffic with XDP active
# 4. SYN flood simulation (5000 packets)
# 5. Legitimate traffic during attack
# 6. Statistics validation
```

#### Manual eBPF Testing

```bash
# Load XDP program
sudo cargo run --bin aegis-ebpf-loader -- attach \
    --interface eth0 \
    --threshold 100

# Monitor stats
sudo cargo run --bin aegis-ebpf-loader -- stats

# Add IP to blocklist
sudo cargo run --bin aegis-ebpf-loader -- block --ip 192.168.1.100

# Remove IP from blocklist
sudo cargo run --bin aegis-ebpf-loader -- unblock --ip 192.168.1.100

# Detach XDP program
sudo cargo run --bin aegis-ebpf-loader -- detach --interface eth0
```

#### eBPF Test Validation

```bash
# Check if XDP is attached
ip link show eth0 | grep xdp

# Monitor eBPF map contents
sudo bpftool map dump name blocklist_v4
sudo bpftool map dump name syn_counts

# View eBPF programs
sudo bpftool prog list
```

### 4. P2P Networking Tests

#### Start NATS Server

```bash
# Terminal 1: Start NATS with JetStream
nats-server -js -m 8222

# Verify NATS is running
curl http://localhost:8222/healthz
```

#### Run P2P Tests

```bash
cd node

# Run P2P-specific tests
cargo test threat_intel_p2p::tests -- --test-threads=1

# Test distributed state sync
cargo test nats_sync::tests
cargo test distributed_counter::tests
cargo test distributed_rate_limiter::tests
```

#### Multi-Node P2P Testing

```bash
# Terminal 1: Start node 1
AEGIS_NODE_ID=node1 cargo run --bin aegis-threat-intel -- \
    --listen /ip4/127.0.0.1/tcp/4001

# Terminal 2: Start node 2
AEGIS_NODE_ID=node2 cargo run --bin aegis-threat-intel -- \
    --listen /ip4/127.0.0.1/tcp/4002 \
    --peer /ip4/127.0.0.1/tcp/4001/p2p/<NODE1_PEER_ID>

# Terminal 3: Start node 3
AEGIS_NODE_ID=node3 cargo run --bin aegis-threat-intel -- \
    --listen /ip4/127.0.0.1/tcp/4003 \
    --peer /ip4/127.0.0.1/tcp/4001/p2p/<NODE1_PEER_ID>

# Publish threat intel from node 1, verify propagation to nodes 2 & 3
```

#### Verify CRDT Convergence

```bash
# Test distributed rate limiter
cargo test distributed_rate_limiter::tests::test_convergence -- --nocapture

# Expected: All nodes converge to same counter value within 2 seconds
```

### 5. Wasm Runtime Tests

#### Build Test Modules

```bash
# Build WAF Wasm module
cd wasm-waf
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/wasm_waf.wasm ../node/test-modules/

# Build edge function example
cd ../wasm-edge-function-example
./build.sh
cp target/wasm32-unknown-unknown/release/edge_function.wasm ../node/test-modules/
```

#### Run Wasm Tests

```bash
cd node

# All Wasm runtime tests
cargo test wasm_runtime::tests -- --test-threads=1

# Specific test categories
cargo test wasm_runtime::tests::test_waf_module
cargo test wasm_runtime::tests::test_edge_function
cargo test wasm_runtime::tests::test_host_api_cache
cargo test wasm_runtime::tests::test_module_signature_verification
cargo test wasm_runtime::tests::test_resource_limits
```

#### Test Module Signing

```bash
cd node

# Generate Ed25519 keypair
cargo run --example generate_signing_key

# Sign a Wasm module
cargo run --example sign_module -- \
    --module test-modules/wasm_waf.wasm \
    --key private_key.hex \
    --output test-modules/wasm_waf.wasm.sig

# Verify signature
cargo run --example verify_module -- \
    --module test-modules/wasm_waf.wasm \
    --signature test-modules/wasm_waf.wasm.sig \
    --pubkey public_key.hex
```

### 6. Integration Tests

#### Prerequisites

```bash
# Start all required services
# Terminal 1: NATS
nats-server -js

# Terminal 2: Redis/DragonflyDB
dragonfly --port 6379
# OR: redis-server

# Terminal 3: IPFS
ipfs daemon

# Terminal 4: Solana validator (if testing contracts)
solana-test-validator
```

#### Run Integration Tests

```bash
cd node

# Build and start server
cargo build --release --bin aegis-node
./target/release/aegis-node &
SERVER_PID=$!
sleep 3

# Run integration tests
cargo test --test integration_test

# Stop server
kill $SERVER_PID
```

#### End-to-End Test Scenarios

```bash
# Test 1: Health check
curl -s http://localhost:8080/health

# Test 2: WAF blocking
curl -s "http://localhost:8080/?id=1' OR '1'='1"
# Expected: 403 Forbidden

# Test 3: Cache operations
curl -s -X POST http://localhost:8080/cache/set \
    -H "Content-Type: application/json" \
    -d '{"key": "test", "value": "hello"}'

curl -s http://localhost:8080/cache/get?key=test
# Expected: {"value": "hello"}

# Test 4: Challenge API
curl -s -X POST http://localhost:8080/aegis/challenge/issue \
    -H "Content-Type: application/json" \
    -d '{"type": "invisible"}'

# Test 5: Verifiable metrics
curl -s http://localhost:8080/verifiable-metrics
```

#### Load Testing

```bash
# Using k6
k6 run scripts/load-test.js

# Using wrk
wrk -t12 -c400 -d30s http://localhost:8080/health

# Using hey
hey -n 10000 -c 100 http://localhost:8080/health
```

### 7. Performance Benchmarks

#### Run Criterion Benchmarks

```bash
cd node

# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench performance -- waf
cargo bench --bench performance -- cache
cargo bench --bench performance -- wasm

# Generate HTML reports
cargo bench -- --html-reports
# Open target/criterion/report/index.html
```

#### Profiling

```bash
cd node

# CPU profiling with flamegraph
sudo cargo flamegraph --bin aegis-node -- --config config.toml

# Memory profiling with heaptrack
heaptrack cargo run --release --bin aegis-node

# Analyze results
heaptrack_gui heaptrack.aegis-node.*.zst
```

#### Stress Testing

```bash
# Run profiling script
./scripts/profile-run.sh

# Expected metrics:
# - Latency: <60ms TTFB (cached)
# - Throughput: >100k req/sec
# - Memory: <500MB baseline
# - CPU: <50% at 10k req/sec
```

---

## CI/CD Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/test.yml
name: AEGIS Test Suite

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  rust-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libssl-dev pkg-config

      - name: Run tests
        run: |
          cd node
          cargo test --lib
          cargo clippy -- -D warnings
          cargo fmt -- --check

  contract-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Solana
        run: |
          sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
          echo "$HOME/.local/share/solana/install/active_release/bin" >> $GITHUB_PATH

      - name: Install Anchor
        run: |
          cargo install --git https://github.com/coral-xyz/anchor anchor-cli

      - name: Run contract tests
        run: |
          solana-test-validator &
          sleep 5
          ./test-all.sh

  ebpf-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install eBPF dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y linux-headers-$(uname -r) llvm clang
          rustup target add bpfel-unknown-none
          cargo install bpf-linker

      - name: Build eBPF programs
        run: |
          cd node/ebpf/syn-flood-filter
          cargo build --release --target bpfel-unknown-none
```

### Docker Testing

```dockerfile
# Dockerfile.test
FROM rust:1.75-bookworm

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential pkg-config libssl-dev \
    nodejs npm \
    && rm -rf /var/lib/apt/lists/*

# Install Solana & Anchor
RUN sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
RUN cargo install --git https://github.com/coral-xyz/anchor anchor-cli

WORKDIR /app
COPY . .

CMD ["./test-all.sh"]
```

```bash
# Build and run tests in Docker
docker build -f Dockerfile.test -t aegis-test .
docker run --rm aegis-test
```

---

## Troubleshooting

### Common Issues

#### 1. Cargo build fails with OpenSSL errors

```bash
# Install OpenSSL development files
sudo apt install -y libssl-dev pkg-config

# Set environment variable
export OPENSSL_DIR=/usr/lib/ssl
```

#### 2. eBPF compilation fails

```bash
# Install required tools
sudo apt install -y llvm clang linux-headers-$(uname -r)

# Ensure bpf-linker is installed
cargo install bpf-linker

# Add eBPF target
rustup target add bpfel-unknown-none
```

#### 3. Anchor test fails with "validator not found"

```bash
# Start local validator first
solana-test-validator --reset &
sleep 5

# Then run tests
anchor test --skip-local-validator
```

#### 4. NATS connection refused

```bash
# Check if NATS is running
systemctl status nats-server

# Start NATS manually
nats-server -js -m 8222 &
```

#### 5. Redis/DragonflyDB connection failed

```bash
# Check Redis status
redis-cli ping

# Start Redis if needed
redis-server --daemonize yes
```

#### 6. IPFS daemon not responding

```bash
# Initialize IPFS (first time)
ipfs init

# Start daemon
ipfs daemon &

# Verify
ipfs id
```

#### 7. Permission denied for eBPF operations

```bash
# Run with sudo for eBPF tests
sudo -E cargo test ebpf

# Or add CAP_BPF capability
sudo setcap cap_bpf+ep target/release/aegis-ebpf-loader
```

#### 8. Out of memory during tests

```bash
# Increase swap
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Limit parallel test threads
cargo test -- --test-threads=2
```

### Debug Logging

```bash
# Enable verbose logging
RUST_LOG=debug cargo test -- --nocapture

# Enable specific module logging
RUST_LOG=aegis_node::waf=trace,aegis_node::challenge=debug cargo test

# Log to file
RUST_LOG=debug cargo test 2>&1 | tee test.log
```

---

## Test Coverage Report

### Generate Coverage Report

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Generate coverage report
cd node
cargo llvm-cov --html --output-dir coverage

# Open report
xdg-open coverage/html/index.html
```

### Current Test Statistics

| Component | Tests | Coverage |
|-----------|-------|----------|
| **Node (Rust)** | 284 | ~75% |
| **Token Contract** | 40 | ~90% |
| **Registry Contract** | 25 | ~85% |
| **Staking Contract** | 35 | ~88% |
| **Rewards Contract** | 30 | ~82% |
| **DAO Contract** | 14 | ~80% |
| **eBPF Programs** | 6 | ~60% |
| **Total** | 434+ | ~78% |

---

## Summary

### Test Commands Cheat Sheet

```bash
# Quick test (most common)
cd node && cargo test --lib

# Full test suite
./test-all.sh

# Smart contracts only
for c in token registry staking rewards dao; do
    cd contracts/$c && anchor test --skip-local-validator && cd ../..
done

# eBPF only (requires root)
sudo ./node/test-syn-flood.sh

# Performance benchmarks
cd node && cargo bench

# Coverage report
cd node && cargo llvm-cov --html

# Security audit
cargo audit
```

### Test Execution Order (Recommended)

1. **Pre-flight checks**: `cargo check`, `cargo fmt`, `cargo clippy`
2. **Unit tests**: `cargo test --lib`
3. **Integration tests**: `cargo test --test integration_test`
4. **Smart contract tests**: `anchor test`
5. **eBPF tests**: `sudo ./test-syn-flood.sh`
6. **Performance benchmarks**: `cargo bench`
7. **Security audit**: `cargo audit`

---

**Last Updated:** 2025-12-02
**Version:** Sprint 29
**Maintainer:** AEGIS Core Team
