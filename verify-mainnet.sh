#!/bin/bash
#
# AEGIS Mainnet Deployment Verification Script
#
# Verifies that all AEGIS contracts are properly deployed and operational.
#
# Usage:
#   ./verify-mainnet.sh [deployment_manifest.json]
#
# If no manifest is provided, looks for the most recent one in deployments/mainnet/

set -euo pipefail

# Configuration
MAINNET_RPC="https://api.mainnet-beta.solana.com"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() {
    local level=$1
    shift
    case $level in
        INFO)  echo -e "${BLUE}[INFO]${NC}  $@" ;;
        OK)    echo -e "${GREEN}[OK]${NC}    $@" ;;
        WARN)  echo -e "${YELLOW}[WARN]${NC}  $@" ;;
        ERROR) echo -e "${RED}[ERROR]${NC} $@" ;;
    esac
}

echo ""
echo "================================================================================"
echo "                    AEGIS MAINNET VERIFICATION SCRIPT"
echo "================================================================================"
echo ""

# Find manifest
MANIFEST="${1:-}"
if [ -z "$MANIFEST" ]; then
    MANIFEST=$(ls -t deployments/mainnet/deployment_*.json 2>/dev/null | head -1 || true)
    if [ -z "$MANIFEST" ]; then
        log ERROR "No deployment manifest found"
        log ERROR "Run ./deploy-mainnet.sh first, or specify a manifest file"
        exit 1
    fi
    log INFO "Using most recent manifest: $MANIFEST"
fi

if [ ! -f "$MANIFEST" ]; then
    log ERROR "Manifest file not found: $MANIFEST"
    exit 1
fi

# Check jq
if ! command -v jq &> /dev/null; then
    log ERROR "jq is required for JSON parsing. Install with: brew install jq"
    exit 1
fi

# Parse manifest
log INFO "Parsing deployment manifest..."
NETWORK=$(jq -r '.network' "$MANIFEST")
TIMESTAMP=$(jq -r '.timestamp' "$MANIFEST")
DEPLOYER=$(jq -r '.deployer' "$MANIFEST")

echo ""
echo "Deployment Details:"
echo "  Network:   $NETWORK"
echo "  Timestamp: $TIMESTAMP"
echo "  Deployer:  $DEPLOYER"
echo ""

# Verify each program
log INFO "Verifying deployed programs..."
echo ""

CONTRACTS=(token registry staking rewards dao)
ALL_OK=true

for contract in "${CONTRACTS[@]}"; do
    PROGRAM_ID=$(jq -r ".programs.$contract" "$MANIFEST")

    if [ "$PROGRAM_ID" = "null" ] || [ -z "$PROGRAM_ID" ]; then
        log WARN "$contract: Not found in manifest"
        continue
    fi

    echo -n "  Checking $contract ($PROGRAM_ID)... "

    # Check if program exists
    if solana program show "$PROGRAM_ID" --url "$MAINNET_RPC" > /dev/null 2>&1; then
        # Get program info
        PROGRAM_INFO=$(solana program show "$PROGRAM_ID" --url "$MAINNET_RPC" 2>&1)
        SLOT=$(echo "$PROGRAM_INFO" | grep "Last Deployed Slot" | awk '{print $4}' || echo "N/A")
        SIZE=$(echo "$PROGRAM_INFO" | grep "Data Length" | awk '{print $3, $4}' || echo "N/A")

        echo -e "${GREEN}OK${NC}"
        echo "    Slot: $SLOT, Size: $SIZE"
    else
        echo -e "${RED}FAILED${NC}"
        log ERROR "Program not found or not accessible: $PROGRAM_ID"
        ALL_OK=false
    fi
done

echo ""

# Summary
if [ "$ALL_OK" = true ]; then
    echo "================================================================================"
    echo -e "                    ${GREEN}ALL PROGRAMS VERIFIED SUCCESSFULLY${NC}"
    echo "================================================================================"
else
    echo "================================================================================"
    echo -e "                    ${RED}VERIFICATION FAILED${NC}"
    echo "================================================================================"
    echo ""
    log ERROR "Some programs failed verification. Check the output above."
    exit 1
fi

echo ""

# Additional checks
log INFO "Running additional verification checks..."
echo ""

# Check if programs are executable
echo "Program executability:"
for contract in "${CONTRACTS[@]}"; do
    PROGRAM_ID=$(jq -r ".programs.$contract" "$MANIFEST")
    if [ "$PROGRAM_ID" != "null" ] && [ -n "$PROGRAM_ID" ]; then
        EXECUTABLE=$(solana program show "$PROGRAM_ID" --url "$MAINNET_RPC" 2>&1 | grep "Executable:" | awk '{print $2}' || echo "unknown")
        echo "  $contract: $EXECUTABLE"
    fi
done

echo ""

# Print explorer links
echo "Explorer Links:"
for contract in "${CONTRACTS[@]}"; do
    PROGRAM_ID=$(jq -r ".programs.$contract" "$MANIFEST")
    if [ "$PROGRAM_ID" != "null" ] && [ -n "$PROGRAM_ID" ]; then
        echo "  $contract:"
        echo "    Solana: https://explorer.solana.com/address/$PROGRAM_ID"
        echo "    Solscan: https://solscan.io/account/$PROGRAM_ID"
    fi
done

echo ""
log OK "Verification complete"
echo ""
