#!/bin/bash
# Test all contracts on local validator

echo "Starting local validator and testing all contracts..."
cd contracts/staking
anchor test --skip-deploy

cd ../rewards  
anchor test --skip-deploy

echo ""
echo "✅ All local tests complete!"
echo ""
echo "Deployed contracts on Devnet (when stable):"
echo "  Token:    9uVLmgqJz3nYcCxHVSAJA8bi6412LEZ5uGM5yguvKHRq"
echo "  Registry: 4JRL443DxceXsgqqxmBt4tD8TecBBo9Xr5kTLNRupiG6" 
echo "  Staking:  85Pd1GRJ1qA3kVTn3ERHsyuUpkr2bbb9L9opwS9UnHEQ (ready)"
echo "  Rewards:  (ready to generate)"
