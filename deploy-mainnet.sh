#!/bin/bash
#
# AEGIS Mainnet Deployment Script
#
# IMPORTANT: This script deploys REAL contracts to Solana Mainnet.
# - Requires significant SOL balance for deployment fees
# - Contracts are immutable once deployed (no upgrades without authority)
# - Should only be run after thorough testing on devnet
#
# Prerequisites:
# - Solana CLI installed and configured
# - Anchor CLI installed
# - Mainnet wallet with sufficient SOL (~10 SOL recommended)
# - All contracts tested and audited
#
# Usage:
#   ./deploy-mainnet.sh [--confirm]
#
# The --confirm flag is required to actually deploy. Without it, the script
# runs in dry-run mode showing what would be deployed.

set -euo pipefail

# Configuration
MAINNET_RPC="https://api.mainnet-beta.solana.com"
MIN_SOL_BALANCE=5  # Minimum SOL required for deployment
DEPLOY_DIR="deployments/mainnet"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="${DEPLOY_DIR}/deployment_${TIMESTAMP}.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
DRY_RUN=true
if [[ "${1:-}" == "--confirm" ]]; then
    DRY_RUN=false
fi

# Logging function
log() {
    local level=$1
    shift
    local msg="$@"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')

    case $level in
        INFO)  echo -e "${BLUE}[INFO]${NC}  $msg" ;;
        OK)    echo -e "${GREEN}[OK]${NC}    $msg" ;;
        WARN)  echo -e "${YELLOW}[WARN]${NC}  $msg" ;;
        ERROR) echo -e "${RED}[ERROR]${NC} $msg" ;;
    esac

    if [ -f "$LOG_FILE" ]; then
        echo "[$timestamp] [$level] $msg" >> "$LOG_FILE"
    fi
}

# Create deployment directory
mkdir -p "$DEPLOY_DIR"
touch "$LOG_FILE"

echo ""
echo "================================================================================"
echo "                     AEGIS MAINNET DEPLOYMENT SCRIPT"
echo "================================================================================"
echo ""

if [ "$DRY_RUN" = true ]; then
    log WARN "DRY RUN MODE - No actual deployment will occur"
    log WARN "Run with --confirm flag to actually deploy"
    echo ""
fi

# Check prerequisites
log INFO "Checking prerequisites..."

# Check Solana CLI
if ! command -v solana &> /dev/null; then
    log ERROR "Solana CLI not found. Install from https://docs.solana.com/cli/install-solana-cli-tools"
    exit 1
fi
log OK "Solana CLI: $(solana --version)"

# Check Anchor CLI
if ! command -v anchor &> /dev/null; then
    log ERROR "Anchor CLI not found. Install with: cargo install --git https://github.com/coral-xyz/anchor anchor-cli"
    exit 1
fi
log OK "Anchor CLI: $(anchor --version)"

# Check wallet configuration
WALLET_PATH=$(solana config get | grep "Keypair Path" | awk '{print $3}')
if [ ! -f "$WALLET_PATH" ]; then
    log ERROR "Wallet keypair not found at: $WALLET_PATH"
    log ERROR "Configure with: solana config set --keypair <path>"
    exit 1
fi

WALLET_PUBKEY=$(solana-keygen pubkey "$WALLET_PATH")
log OK "Wallet: $WALLET_PUBKEY"

# Switch to mainnet
log INFO "Switching to mainnet-beta..."
solana config set --url "$MAINNET_RPC" > /dev/null
log OK "RPC: $MAINNET_RPC"

# Check mainnet connectivity
log INFO "Checking mainnet connectivity..."
if ! solana balance --url "$MAINNET_RPC" > /dev/null 2>&1; then
    log ERROR "Mainnet RPC is not responsive"
    log ERROR "Check your internet connection or try a different RPC endpoint"
    exit 1
fi
log OK "Mainnet is responsive"

# Check SOL balance
SOL_BALANCE=$(solana balance --url "$MAINNET_RPC" | awk '{print $1}')
log INFO "Wallet balance: $SOL_BALANCE SOL"

# Use bc for float comparison
if (( $(echo "$SOL_BALANCE < $MIN_SOL_BALANCE" | bc -l) )); then
    log ERROR "Insufficient SOL balance. Required: ${MIN_SOL_BALANCE} SOL, Available: ${SOL_BALANCE} SOL"
    log ERROR "Deposit more SOL to your wallet: $WALLET_PUBKEY"
    exit 1
fi
log OK "Sufficient balance for deployment"

echo ""
echo "--------------------------------------------------------------------------------"
echo "                           PRE-DEPLOYMENT CHECKLIST"
echo "--------------------------------------------------------------------------------"
echo ""

# Checklist
CHECKLIST=(
    "All contracts have been tested on devnet"
    "Security audit has been completed"
    "Contract code has been frozen (no pending changes)"
    "Deployment wallet is properly secured"
    "Team has been notified of deployment"
    "Emergency response plan is in place"
)

for item in "${CHECKLIST[@]}"; do
    if [ "$DRY_RUN" = true ]; then
        echo "  [ ] $item"
    else
        read -p "  [ ] $item (y/n): " response
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            log ERROR "Deployment aborted - checklist item not confirmed"
            exit 1
        fi
        echo "  [x] $item"
    fi
done

echo ""
echo "--------------------------------------------------------------------------------"
echo "                            CONTRACTS TO DEPLOY"
echo "--------------------------------------------------------------------------------"
echo ""

# Contract order (dependencies first)
CONTRACTS=(
    "token:AEGIS Token Contract"
    "registry:Node Registry Contract"
    "staking:Staking Contract"
    "rewards:Rewards Distribution Contract"
    "dao:DAO Governance Contract"
)

for contract in "${CONTRACTS[@]}"; do
    name="${contract%%:*}"
    desc="${contract#*:}"
    echo "  - $desc (contracts/$name)"
done

echo ""

if [ "$DRY_RUN" = true ]; then
    log INFO "Dry run complete. Run with --confirm to deploy."
    exit 0
fi

# Final confirmation
echo ""
echo "================================================================================"
echo "                        FINAL DEPLOYMENT CONFIRMATION"
echo "================================================================================"
echo ""
echo "You are about to deploy AEGIS contracts to SOLANA MAINNET."
echo ""
echo "This action is IRREVERSIBLE. Contracts cannot be deleted once deployed."
echo "Deployment will consume approximately 3-5 SOL in transaction fees."
echo ""
read -p "Type 'DEPLOY TO MAINNET' to proceed: " confirm
if [[ "$confirm" != "DEPLOY TO MAINNET" ]]; then
    log ERROR "Deployment cancelled by user"
    exit 1
fi

echo ""
echo "================================================================================"
echo "                          DEPLOYING CONTRACTS"
echo "================================================================================"
echo ""

# Store deployed program IDs
declare -A PROGRAM_IDS

# Function to deploy a contract
deploy_contract() {
    local name=$1
    local desc=$2
    local dir="contracts/$name"

    echo ""
    log INFO "Deploying $desc..."
    echo "--------------------------------------------------------------------------------"

    cd "$dir"

    # Generate keypair if not exists
    local keypair_path="target/deploy/${name}-keypair.json"
    if [ ! -f "$keypair_path" ]; then
        log INFO "Generating program keypair..."
        mkdir -p target/deploy
        solana-keygen new --no-bip39-passphrase -o "$keypair_path" --force
    fi

    # Get program ID
    local program_id=$(solana-keygen pubkey "$keypair_path")
    log INFO "Program ID: $program_id"

    # Sync program ID in Anchor.toml
    log INFO "Syncing program IDs..."
    anchor keys sync

    # Build
    log INFO "Building contract..."
    anchor build

    # Verify build
    local so_path="target/deploy/${name}.so"
    if [ ! -f "$so_path" ]; then
        log ERROR "Build failed - .so file not found"
        exit 1
    fi
    local so_size=$(du -h "$so_path" | awk '{print $1}')
    log OK "Built: $so_path ($so_size)"

    # Deploy
    log INFO "Deploying to mainnet..."
    anchor deploy --provider.cluster mainnet-beta

    # Verify deployment
    log INFO "Verifying deployment..."
    if ! solana program show "$program_id" --url "$MAINNET_RPC" > /dev/null 2>&1; then
        log ERROR "Deployment verification failed for $program_id"
        exit 1
    fi
    log OK "$desc deployed: $program_id"

    # Store program ID
    PROGRAM_IDS[$name]=$program_id

    # Save keypair backup
    local backup_path="../../${DEPLOY_DIR}/${name}-keypair-${TIMESTAMP}.json"
    cp "$keypair_path" "$backup_path"
    log INFO "Keypair backed up to: $backup_path"

    cd ../..
}

# Deploy each contract
deploy_contract "token" "AEGIS Token"
deploy_contract "registry" "Node Registry"
deploy_contract "staking" "Staking"
deploy_contract "rewards" "Rewards Distribution"
deploy_contract "dao" "DAO Governance"

echo ""
echo "================================================================================"
echo "                        DEPLOYMENT COMPLETE"
echo "================================================================================"
echo ""

# Generate deployment manifest
MANIFEST_FILE="${DEPLOY_DIR}/deployment_${TIMESTAMP}.json"
cat > "$MANIFEST_FILE" << EOF
{
  "network": "mainnet-beta",
  "timestamp": "$(date -Iseconds)",
  "deployer": "$WALLET_PUBKEY",
  "programs": {
    "token": "${PROGRAM_IDS[token]}",
    "registry": "${PROGRAM_IDS[registry]}",
    "staking": "${PROGRAM_IDS[staking]}",
    "rewards": "${PROGRAM_IDS[rewards]}",
    "dao": "${PROGRAM_IDS[dao]}"
  },
  "rpc": "$MAINNET_RPC",
  "solana_cli_version": "$(solana --version | awk '{print $2}')",
  "anchor_cli_version": "$(anchor --version | awk '{print $2}')"
}
EOF

log OK "Deployment manifest saved: $MANIFEST_FILE"
echo ""

# Print summary
echo "Program IDs:"
echo "  Token:    ${PROGRAM_IDS[token]}"
echo "  Registry: ${PROGRAM_IDS[registry]}"
echo "  Staking:  ${PROGRAM_IDS[staking]}"
echo "  Rewards:  ${PROGRAM_IDS[rewards]}"
echo "  DAO:      ${PROGRAM_IDS[dao]}"
echo ""
echo "Deployment artifacts saved to: $DEPLOY_DIR/"
echo "  - Keypair backups"
echo "  - Deployment manifest (JSON)"
echo "  - Deployment log"
echo ""

# Remaining balance
REMAINING_BALANCE=$(solana balance --url "$MAINNET_RPC" | awk '{print $1}')
log INFO "Remaining wallet balance: $REMAINING_BALANCE SOL"
echo ""

# Post-deployment instructions
echo "================================================================================"
echo "                        POST-DEPLOYMENT STEPS"
echo "================================================================================"
echo ""
echo "1. Update program IDs in the codebase:"
echo "   - contracts/*/Anchor.toml"
echo "   - dashboard/packages/*/src/config.ts"
echo "   - cli/src/config.rs"
echo ""
echo "2. Initialize program state:"
echo "   - Initialize DAO config"
echo "   - Set up treasury multisig"
echo "   - Configure staking parameters"
echo ""
echo "3. Verify contracts on explorers:"
echo "   - https://explorer.solana.com"
echo "   - https://solscan.io"
echo ""
echo "4. Announce deployment:"
echo "   - Update documentation"
echo "   - Notify community"
echo ""
echo "IMPORTANT: Keep the keypair backups secure! They are needed for upgrades."
echo ""
