# AEGIS Installation Guide

**Status**: Sprint 1 Environment Setup
**Last Updated**: November 19, 2025

## Installation Checklist

### ✅ Completed
- [x] Rust 1.93.0 installed and verified
- [x] Node.js v20.19.5 installed and verified
- [x] npm 10.8.2 available
- [x] Project directory structure created
- [x] HTTP server code ready (compiles successfully)
- [x] Token smart contract code ready

### ⏳ Remaining
- [ ] Solana CLI v1.18.26+ installed
- [ ] Anchor framework v0.30.1+ installed
- [ ] Solana Devnet wallet created and funded
- [ ] Token program deployed to Devnet

---

## Step-by-Step Installation

### 1. Install Solana CLI

**Option A: Download and Run Installer (Recommended)**

1. **Download the installer**:
   - Visit: https://release.solana.com/v1.18.26/solana-install-init-x86_64-pc-windows-msvc.exe
   - Or use PowerShell:
   ```powershell
   Invoke-WebRequest -Uri "https://release.solana.com/v1.18.26/solana-install-init-x86_64-pc-windows-msvc.exe" -OutFile "$env:TEMP\solana-install.exe"
   ```

2. **Run the installer**:
   ```powershell
   # In PowerShell
   & "$env:TEMP\solana-install.exe" v1.18.26
   ```

3. **Add to PATH** (if not automatic):
   ```powershell
   # Add to current session
   $env:PATH += ";$env:USERPROFILE\.local\share\solana\install\active_release\bin"

   # Add permanently (restart terminal after)
   [Environment]::SetEnvironmentVariable("PATH", $env:PATH + ";$env:USERPROFILE\.local\share\solana\install\active_release\bin", "User")
   ```

4. **Verify installation**:
   ```bash
   # Restart your terminal first!
   solana --version
   # Should show: solana-cli 1.18.26
   ```

**Option B: Using Pre-built Script**

We've created `install-solana.ps1` in this directory. Run:
```powershell
powershell -ExecutionPolicy Bypass -File install-solana.ps1
```

### 2. Configure Solana for Devnet

```bash
# Set cluster to Devnet
solana config set --url https://api.devnet.solana.com

# Verify configuration
solana config get
# Should show:
# Config File: ~/.config/solana/cli/config.yml
# RPC URL: https://api.devnet.solana.com
# WebSocket URL: wss://api.devnet.solana.com/
# Keypair Path: (not set yet)
```

### 3. Create Solana Wallet

```bash
# Generate new keypair for Devnet testing
solana-keygen new --outfile ~/.config/solana/devnet-wallet.json

# ⚠️ IMPORTANT: Write down the seed phrase shown!
# This is for Devnet testing only, but don't lose it during development

# Set as default keypair
solana config set --keypair ~/.config/solana/devnet-wallet.json

# Get your wallet address
solana address
# Example output: 7xYZ...abc123 (your unique address)
```

### 4. Fund Devnet Wallet

**Method 1: Command Line (Recommended)**
```bash
# Request 2 SOL airdrop
solana airdrop 2

# Check balance
solana balance
# Should show: 2 SOL
```

**Method 2: Web Faucet (if airdrop fails)**
1. Get your address: `solana address`
2. Visit: https://faucet.solana.com
3. Paste your address and request airdrop
4. Wait 30 seconds and check: `solana balance`

**If both fail**: Try smaller amount
```bash
solana airdrop 1
# or
solana airdrop 0.5
```

### 5. Install Anchor Framework

**Prerequisites Check**:
```bash
# Verify all dependencies
rustc --version    # Should be 1.70+
cargo --version    # Should be 1.70+
node --version     # Should be v18+
npm --version      # Should be 9+
solana --version   # Should be 1.18+
```

**Install Anchor CLI**:
```bash
# This takes 5-10 minutes (compiling from source)
cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli --locked

# Verify installation
anchor --version
# Should show: anchor-cli 0.30.1
```

**If installation fails** with memory errors:
```bash
# Install with fewer parallel jobs
cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli --locked -j 2
```

### 6. Install Additional Tools (Optional but Recommended)

**SPL Token CLI** (for token operations):
```bash
cargo install spl-token-cli

# Verify
spl-token --version
```

**Solana Program Library** (for reference):
```bash
git clone https://github.com/solana-labs/solana-program-library
```

---

## Verification: Test Everything

Once all installations complete, run this verification script:

```bash
# Save this as verify-install.sh
#!/bin/bash

echo "==================================="
echo "AEGIS Development Environment Check"
echo "==================================="
echo ""

echo "1. Rust:"
rustc --version && cargo --version && echo "  ✓ Rust OK" || echo "  ✗ Rust MISSING"
echo ""

echo "2. Node.js:"
node --version && npm --version && echo "  ✓ Node.js OK" || echo "  ✗ Node.js MISSING"
echo ""

echo "3. Solana:"
solana --version && echo "  ✓ Solana CLI OK" || echo "  ✗ Solana CLI MISSING"
echo ""

echo "4. Solana Config:"
solana config get | grep -q "devnet" && echo "  ✓ Devnet configured" || echo "  ✗ Not on Devnet"
echo ""

echo "5. Wallet:"
solana balance && echo "  ✓ Wallet funded" || echo "  ✗ Wallet not funded"
echo ""

echo "6. Anchor:"
anchor --version && echo "  ✓ Anchor OK" || echo "  ✗ Anchor MISSING"
echo ""

echo "==================================="
echo "If all show ✓, you're ready for Sprint 1!"
echo "==================================="
```

Run with: `bash verify-install.sh`

---

## Next Steps: Deploy Token Program

Once all tools are installed:

### 1. Build Token Program

```bash
cd D:/Projects/AEGIS/contracts/token

# Install Node dependencies
npm install

# Build the Anchor program
anchor build

# Should complete without errors
# Outputs:
# - target/deploy/aegis_token.so (compiled program)
# - target/idl/aegis_token.json (interface definition)
```

### 2. Run Tests

```bash
# Run Anchor tests (requires Devnet wallet funded)
anchor test

# Expected output:
# ✓ Initializes the AEGIS token mint
# ✓ Mints tokens to a user account
# ✓ Transfers tokens between accounts
# ✓ Burns tokens
# ✓ Fails to mint beyond total supply cap
# ✓ Fails to mint with invalid decimals
#
# 6 passing
```

### 3. Deploy to Devnet

```bash
# Deploy the program
anchor deploy

# Output will show:
# Program Id: <YOUR_PROGRAM_ID>
# (e.g., AEGIS1234567890abcdefghijklmnopqrstuvwxyz)

# Save this Program ID! Update it in Anchor.toml
```

### 4. Initialize Token Mint

```bash
# The deploy command will give you a program ID
# Update contracts/token/Anchor.toml with this ID

# Then re-deploy
anchor deploy

# Now initialize the mint (first time only)
# This will be done via the test suite or a custom script
```

### 5. Test HTTP Server

```bash
cd D:/Projects/AEGIS/node

# Build the server
cargo build

# Run the server
cargo run

# In another terminal, test it:
curl http://localhost:8080/health
# Should return: {"status":"healthy","version":"0.1.0",...}
```

---

## Troubleshooting

### Solana CLI: "command not found"

**Solution**: PATH not updated. Run:
```bash
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
# Or on Windows PowerShell:
$env:PATH += ";$env:USERPROFILE\.local\share\solana\install\active_release\bin"
```

Then restart your terminal.

### Anchor build fails: "error: linker 'link.exe' not found"

**Windows Solution**: Install Visual Studio Build Tools
1. Download: https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022
2. Run installer
3. Select "Desktop development with C++"
4. Install
5. Restart terminal and retry

**Alternative**: Install via rustup
```bash
rustup default stable-x86_64-pc-windows-msvc
```

### Anchor test fails: "Error: Account does not exist"

**Solution**: Fund your wallet
```bash
solana airdrop 2
solana balance  # Verify you have >2 SOL
```

### npm install fails in contracts/token

**Solution**: Delete and reinstall
```bash
rm -rf node_modules package-lock.json
npm install
```

### cargo install anchor-cli takes forever

**This is normal!** Anchor is a large project. It can take 10-20 minutes to compile on the first install. Be patient.

Progress indicators:
- Compiling dependencies: ~5 min
- Compiling anchor-lang: ~5 min
- Compiling anchor-cli: ~5 min
- Linking: ~2 min

**Tip**: Run with verbose output to see progress:
```bash
cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli --locked --verbose
```

---

## Current Status Summary

**Installed**:
- ✅ Rust 1.93.0-nightly
- ✅ Cargo 1.93.0-nightly
- ✅ Node.js v20.19.5
- ✅ npm 10.8.2

**Ready to Deploy**:
- ✅ AEGIS token smart contract (`contracts/token/programs/aegis-token/src/lib.rs`)
- ✅ HTTP server PoC (`node/src/main.rs`)
- ✅ Test suite (`contracts/token/tests/aegis-token.ts`)

**Needs Manual Installation**:
- ⏳ Solana CLI v1.18.26+
- ⏳ Anchor framework v0.30.1+

**After Installation Complete**:
- Deploy token program to Devnet
- Initialize $AEGIS mint
- Test end-to-end flow
- Complete Sprint 1!

---

## Quick Reference Commands

### Daily Workflow

```bash
# Check Solana cluster
solana config get

# Check wallet balance
solana balance

# Build token program
cd contracts/token && anchor build

# Run tests
anchor test

# Deploy to Devnet
anchor deploy

# Run HTTP server
cd ../../node && cargo run

# Test server
curl http://localhost:8080/health
```

### Reset Everything (if needed)

```bash
# Clean Anchor build
cd contracts/token
anchor clean
rm -rf target node_modules

# Clean Rust build
cd ../../node
cargo clean

# Rebuild everything
cd ../contracts/token
npm install
anchor build
anchor test
```

---

## Getting Help

### Official Documentation
- Solana: https://docs.solana.com
- Anchor: https://book.anchor-lang.com
- Rust: https://doc.rust-lang.org/book

### Community
- Solana Discord: https://discord.gg/solana
- Anchor Discord: https://discord.gg/PDeRXyVURd

### AEGIS-Specific
- See `docs/sprints/SPRINT-1-SETUP.md` for detailed technical information
- See `CLAUDE.md` for architecture overview
- See `README.md` for project overview

---

**Good luck with the installation! 🚀**

Once Solana and Anchor are installed, you'll be ready to deploy the $AEGIS token to Devnet and complete Sprint 1.
