# AEGIS Smart Contract Ownership - Devnet

**Network**: Solana Devnet
**Date**: November 20, 2025
**Status**: Development/Testing Phase

---

## Deployed Smart Contracts

### 1. Token Program (aegis_token)
**Program ID**: `JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB`
**Deployment Wallet**: `~/.config/solana/id.json`
**Network**: Devnet
**Explorer**: https://explorer.solana.com/address/JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB?cluster=devnet

**Upgrade Authority**: The wallet that deployed the contract
**Mint Authority**: Controlled by the Token program

**Configuration** (`contracts/token/Anchor.toml`):
```toml
[provider]
cluster = "Devnet"
wallet = "~/.config/solana/id.json"
```

---

### 2. Node Registry Program (node_registry)
**Program ID**: `D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6`
**Deployment Wallet**: `~/.config/solana/id.json`
**Network**: Devnet
**Explorer**: https://explorer.solana.com/address/D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6?cluster=devnet

**Upgrade Authority**: The wallet that deployed the contract

**Note**: There's a discrepancy in the code:
- Registry program ID in CLI: `D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6`
- Anchor.toml shows: `GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno`

**⚠️ Action Required**: Verify which is the correct deployed address

**Configuration** (`contracts/registry/Anchor.toml`):
```toml
[provider]
cluster = "Devnet"
wallet = "~/.config/solana/id.json"

[programs.devnet]
node_registry = "GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno"
```

---

### 3. Staking Program (staking)
**Program ID**: `5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H`
**Deployment Wallet**: `~/.config/solana/id.json`
**Network**: Devnet
**Explorer**: https://explorer.solana.com/address/5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H?cluster=devnet

**Upgrade Authority**: The wallet that deployed the contract

**Note**: Anchor.toml shows cluster as "Localnet" but program is deployed to Devnet

**Configuration** (`contracts/staking/Anchor.toml`):
```toml
[provider]
cluster = "Localnet"  # ⚠️ Should be "Devnet"
wallet = "~/.config/solana/id.json"

[programs.devnet]
staking = "5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H"
```

---

### 4. Rewards Program (rewards)
**Program ID**: `3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c`
**Deployment Wallet**: `~/.config/solana/id.json`
**Network**: Devnet
**Explorer**: https://explorer.solana.com/address/3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c?cluster=devnet

**Upgrade Authority**: The wallet that deployed the contract

**Configuration** (`contracts/rewards/Anchor.toml`):
```toml
[provider]
cluster = "Devnet"
wallet = "~/.config/solana/id.json"

[programs.localnet]
rewards = "3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c"
```

---

## Ownership Summary

### Current Ownership (Devnet)

All 4 contracts are owned by the **default Solana CLI wallet**:
- **Wallet Path**: `~/.config/solana/id.json`
- **Type**: Development wallet (single-signature)
- **Network**: Devnet (testing)

**⚠️ Important**: This is a **development deployment** on Devnet. For production (mainnet), ownership should be transferred to:
1. Multi-signature wallet
2. DAO governance contract
3. Secure key management system (HSM)

---

## How to Check Upgrade Authority

### Method 1: Using Solana CLI

```bash
# In Linux/WSL
solana program show JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB --url devnet
```

**Output Example**:
```
Program Id: JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB
Owner: BPFLoaderUpgradeab1e11111111111111111111111
ProgramData Address: <address>
Authority: <your-wallet-pubkey>
Last Deployed In Slot: XXXXX
Data Length: XXXXX bytes
```

The **Authority** field shows the upgrade authority (owner).

### Method 2: Using Solana Explorer

**Steps**:
1. Go to: https://explorer.solana.com?cluster=devnet
2. Search for program ID (e.g., `JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB`)
3. Look for "Upgradeable Program" section
4. Find "Upgrade Authority" field

### Method 3: Using Anchor CLI

```bash
cd contracts/token
anchor show program
```

**Output**: Shows program details including authority

---

## Wallet Location

### Default Solana Wallet

**Path**: `~/.config/solana/id.json`

**This typically expands to**:
- **Linux/WSL**: `/home/<username>/.config/solana/id.json`
- **macOS**: `/Users/<username>/.config/solana/id.json`

### Getting Wallet Public Key

```bash
# Show the public key of the deployment wallet
solana-keygen pubkey ~/.config/solana/id.json
```

**This is the upgrade authority for all 4 contracts.**

---

## Ownership Verification Checklist

To verify ownership of deployed contracts:

- [ ] **Token Program**:
  ```bash
  solana program show JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB --url devnet
  ```
  Note the "Authority" field

- [ ] **Registry Program**:
  ```bash
  # Check both addresses
  solana program show D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6 --url devnet
  solana program show GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno --url devnet
  ```
  **⚠️ Verify which address is correct**

- [ ] **Staking Program**:
  ```bash
  solana program show 5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H --url devnet
  ```

- [ ] **Rewards Program**:
  ```bash
  solana program show 3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c --url devnet
  ```

---

## Recommended Actions

### For Devnet (Current)

**Current Status**: ✅ ACCEPTABLE for development
- Single wallet ownership is fine for testing
- Allows quick iterations and updates
- No real value at risk on Devnet

**Recommendations**:
1. ✅ Keep current setup for development
2. Document the wallet public key
3. Backup the wallet keypair file
4. Don't share private keys

### For Mainnet (Future)

**Required Changes**: 🔴 CRITICAL

**1. Multi-Signature Wallet**:
```bash
# Create 3-of-5 multisig for production
# Use Squads Protocol or similar
```

**2. Transfer to DAO**:
```rust
// Upgrade authority → DAO governance contract
// Requires proposals + voting for upgrades
```

**3. Security Measures**:
- Hardware wallet (Ledger) for signing
- Cold storage for backup keys
- Key ceremony for multi-sig setup
- Time-locked upgrades
- Emergency pause mechanism

---

## Contract Update Procedures

### Current (Devnet)

**To upgrade a contract**:
```bash
cd contracts/<contract-name>
anchor build
anchor upgrade <program-id> --program-keypair target/deploy/<name>-keypair.json
```

**Authority Required**: `~/.config/solana/id.json` (current owner)

### Future (Mainnet)

**To upgrade a contract**:
1. Create governance proposal
2. Submit to DAO
3. Community votes
4. If passed, multi-sig signs upgrade
5. Upgrade executed with time lock

**Authority Required**: Multi-sig wallet or DAO

---

## Security Considerations

### Current Risks (Devnet)

**Single Point of Failure**:
- ⚠️ One wallet controls all 4 contracts
- ⚠️ If keypair lost, cannot upgrade
- ⚠️ If keypair compromised, malicious upgrade possible

**Mitigation**:
- ✅ Only Devnet (no real value)
- ✅ Keypair backed up
- ✅ Not publicly shared

### Production Requirements (Mainnet)

**Must Have**:
- 🔴 Multi-signature control (3-of-5 minimum)
- 🔴 DAO governance integration
- 🔴 Time-locked upgrades (24-48 hour delay)
- 🔴 Emergency pause mechanism
- 🔴 Audit trail for all upgrades
- 🔴 Hardware wallet signatures

---

## Discrepancy Resolution

### Registry Contract Address Mismatch

**CLI Uses**: `D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6`
**Anchor.toml Shows**: `GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno`

**To Resolve**:

**Option 1: Check Both on Explorer**
```
https://explorer.solana.com/address/D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6?cluster=devnet
https://explorer.solana.com/address/GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno?cluster=devnet
```

**Option 2: Test CLI**
```bash
cd cli
cargo run -- status
# If status shows node data, CLI address is correct
```

**Option 3: Update Anchor.toml**
```bash
cd contracts/registry
# Update Anchor.toml to match CLI (D6kkpe...)
```

**Action**: Use the address that has actual node accounts on-chain

---

## Contract Control Functions

### Token Program
**Mint Authority**: Can mint new tokens (up to 1B cap)
**Upgrade Authority**: Can upgrade program logic
**Transfer Control**: Anyone can transfer their tokens

### Registry Program
**Upgrade Authority**: Can upgrade program logic
**Registration Control**: Anyone can register (with min stake)
**Admin Functions**: None (decentralized)

### Staking Program
**Upgrade Authority**: Can upgrade program logic
**Slash Authority**: Can slash malicious operators (⚠️ check who can call)
**Stake Control**: Operators control their own stakes

### Rewards Program
**Upgrade Authority**: Can upgrade program logic
**Pool Funding**: Can fund reward pool (⚠️ check who can call)
**Claim Control**: Operators claim their own rewards

---

## Recommendations

### Immediate (Devnet)

1. **Document Wallet Public Key**:
   ```bash
   solana-keygen pubkey ~/.config/solana/id.json > DEVNET_OWNER.txt
   ```

2. **Backup Wallet**:
   ```bash
   cp ~/.config/solana/id.json ~/.config/solana/id-backup-$(date +%Y%m%d).json
   ```

3. **Resolve Registry Address**:
   - Determine which address is correct
   - Update either Anchor.toml or CLI contracts.rs
   - Ensure consistency

### Before Mainnet

1. **Multi-Sig Setup**:
   - Create 3-of-5 or 5-of-7 multi-sig wallet
   - Distribute keys to team members
   - Test upgrade process on Devnet

2. **Transfer Ownership**:
   ```bash
   solana program set-upgrade-authority <program-id> --new-upgrade-authority <multisig-address>
   ```

3. **DAO Integration**:
   - Deploy DAO governance contract
   - Transfer authority to DAO
   - Establish voting parameters

4. **Security Audit**:
   - Professional audit of all 4 contracts
   - Penetration testing
   - Formal verification (if possible)

---

## Owner Responsibilities

### Current Owner (Devnet Deployer)

**Can Do**:
- ✅ Upgrade contract code
- ✅ Deploy new versions
- ✅ Call any admin functions
- ✅ Fund reward pool
- ✅ Slash malicious operators (if implemented)

**Cannot Do**:
- ❌ Mint tokens beyond 1B cap (enforced by contract)
- ❌ Steal staked tokens (protected by contract logic)
- ❌ Change other users' stake amounts (without their signature)

**Should Do**:
- Backup keypair securely
- Test all upgrades on localnet first
- Monitor contract health
- Respond to security issues
- Plan mainnet transition

---

## Mainnet Ownership Plan

### Phase 1: Initial Mainnet (Months 1-3)

**Owner**: 3-of-5 Multi-sig
**Members**:
- Core team member 1
- Core team member 2
- Core team member 3
- Advisor 1
- Advisor 2

**Threshold**: 3 signatures required

### Phase 2: DAO Transition (Months 3-6)

**Owner**: DAO Governance Contract
**Voting Power**: Based on staked $AEGIS
**Proposal Threshold**: 1% of staked supply
**Voting Period**: 7 days
**Execution Delay**: 48 hours (time lock)

### Phase 3: Full Decentralization (Month 6+)

**Owner**: Fully on-chain DAO
**Emergency Powers**: Multi-sig can pause (not upgrade)
**Upgrades**: Require supermajority (67% vote)
**Transparency**: All proposals on-chain

---

## Emergency Procedures

### If Wallet Compromised

**Immediate Actions**:
1. Transfer upgrade authority to new wallet
2. Pause contracts (if pause functionality exists)
3. Notify community
4. Audit all recent transactions

**Commands**:
```bash
# Transfer upgrade authority
solana program set-upgrade-authority <program-id> --new-upgrade-authority <new-wallet>
```

### If Wallet Lost

**Recovery Options**:
- Use backup keypair
- If no backup and Devnet: Redeploy contracts
- If no backup and Mainnet: **Cannot recover** (contracts frozen)

**This is why multi-sig is critical for mainnet** 🔴

---

## Verification Commands

### Check Ownership (Linux/WSL)

```bash
# Set cluster
solana config set --url https://api.devnet.solana.com

# Check each program
solana program show JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB
solana program show D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6
solana program show 5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H
solana program show 3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c

# Look for "Authority: <pubkey>" in output
```

### Get Owner Public Key

```bash
# Show the public key of current owner
solana-keygen pubkey ~/.config/solana/id.json
```

**Expected Output**: A 44-character base58 string (your wallet public key)

This public key is the upgrade authority for all contracts.

---

## Summary

### Current State (Devnet)

| Contract | Program ID | Owner Wallet | Status |
|----------|-----------|--------------|--------|
| Token | `JLQ4c9...qyB` | `~/.config/solana/id.json` | ✅ |
| Registry | `D6kkpe...ykr6` or `GLpPpG...scno` | `~/.config/solana/id.json` | ⚠️ Verify |
| Staking | `5oGLkN...VD1H` | `~/.config/solana/id.json` | ✅ |
| Rewards | `3j4guu...Ns8c` | `~/.config/solana/id.json` | ✅ |

**Owner Type**: Single wallet (development)
**Network**: Devnet (testing)
**Security**: Adequate for testing, **NOT for mainnet**

---

## Action Items

### High Priority
1. **Verify Registry Address** - Resolve discrepancy between CLI and Anchor.toml
2. **Document Owner Pubkey** - Run `solana-keygen pubkey` and save
3. **Backup Wallet** - Ensure keypair is safely backed up

### Medium Priority
4. **Update Anchor.toml** - Set all clusters to "Devnet" consistently
5. **Test Upgrades** - Verify you can upgrade contracts with current wallet
6. **Document Procedures** - How to upgrade each contract

### Before Mainnet
7. **Create Multi-Sig** - 3-of-5 or better
8. **Transfer Authority** - From single wallet to multi-sig
9. **Implement DAO** - Governance contract for decentralized control
10. **Security Audit** - Professional review of ownership model

---

## Contact & Support

**For Ownership Questions**:
- Check Solana Explorer (links above)
- Run `solana program show <program-id>` in WSL
- Review Anchor deployment logs

**For Mainnet Planning**:
- Consult with Solana security experts
- Review multi-sig options (Squads Protocol)
- Plan DAO governance tokenomics

---

**Document Version**: 1.0
**Last Updated**: November 20, 2025
**Next Review**: Before mainnet deployment
