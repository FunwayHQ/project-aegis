#!/bin/bash
# Deploy all AEGIS contracts to Solana Devnet
# Run this when devnet RPC is responsive

set -e

echo "╔════════════════════════════════════════════╗"
echo "║   AEGIS Devnet Deployment Script          ║"
echo "╚════════════════════════════════════════════╝"
echo ""

# Check devnet connectivity
echo "Checking devnet connectivity..."
if ! solana balance --url devnet >/dev/null 2>&1; then
    echo "❌ Devnet is not responsive. Please try again later."
    exit 1
fi

echo "✅ Devnet is responsive"
echo ""

# Token (already deployed)
echo "─── Token Contract ───"
echo "Already deployed: 9uVLmgqJz3nYcCxHVSAJA8bi6412LEZ5uGM5yguvKHRq"
echo ""

# Registry
echo "─── Registry Contract ───"
cd contracts/registry
if [ ! -f "target/deploy/registry-keypair.json" ]; then
    echo "Generating new keypair..."
    solana-keygen new --no-bip39-passphrase -o target/deploy/registry-keypair.json --force
fi
echo "Syncing program IDs..."
anchor keys sync
echo "Building..."
anchor build
echo "Deploying..."
anchor deploy --provider.cluster devnet
REGISTRY_ID=$(solana-keygen pubkey target/deploy/registry-keypair.json)
echo "✅ Registry deployed: $REGISTRY_ID"
echo ""

# Staking
echo "─── Staking Contract ───"
cd ../staking
if [ ! -f "target/deploy/staking-keypair.json" ]; then
    echo "Generating new keypair..."
    solana-keygen new --no-bip39-passphrase -o target/deploy/staking-keypair.json --force
fi
echo "Syncing program IDs..."
anchor keys sync
echo "Building..."
anchor build
echo "Deploying..."
anchor deploy --provider.cluster devnet
STAKING_ID=$(solana-keygen pubkey target/deploy/staking-keypair.json)
echo "✅ Staking deployed: $STAKING_ID"
echo ""

# Rewards
echo "─── Rewards Contract ───"
cd ../rewards
if [ ! -f "target/deploy/rewards-keypair.json" ]; then
    echo "Generating new keypair..."
    solana-keygen new --no-bip39-passphrase -o target/deploy/rewards-keypair.json --force
fi
echo "Syncing program IDs..."
anchor keys sync
echo "Building..."
anchor build
echo "Deploying..."
anchor deploy --provider.cluster devnet
REWARDS_ID=$(solana-keygen pubkey target/deploy/rewards-keypair.json)
echo "✅ Rewards deployed: $REWARDS_ID"
echo ""

# Summary
echo "╔════════════════════════════════════════════╗"
echo "║         Deployment Summary                 ║"
echo "╚════════════════════════════════════════════╝"
echo ""
echo "✅ Token:    9uVLmgqJz3nYcCxHVSAJA8bi6412LEZ5uGM5yguvKHRq"
echo "✅ Registry: $REGISTRY_ID"
echo "✅ Staking:  $STAKING_ID"
echo "✅ Rewards:  $REWARDS_ID"
echo ""
echo "All contracts deployed successfully to Solana Devnet!"
echo ""
echo "Next steps:"
echo "  - Update README.md with these program IDs"
echo "  - Run tests: cd ../.. && ./test-all.sh"
echo ""
