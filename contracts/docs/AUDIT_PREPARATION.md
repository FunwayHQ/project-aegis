# AEGIS Smart Contract Audit Preparation

Sprint 27: Smart Contract Security Audit

## Executive Summary

This document provides comprehensive audit preparation materials for the 5 AEGIS Solana smart contracts. The contracts have undergone significant security hardening in Sprint 18 and 18.5, implementing vote escrow patterns, access controls, and CPI integration.

**Total Lines of Code (Anchor Rust):**
- DAO: ~2,054 LOC
- Staking: ~1,079 LOC
- Registry: ~724 LOC
- Rewards: ~913 LOC
- Token: ~1,020 LOC
- **Total: ~5,790 LOC**

**Deployed Networks:** Solana Devnet (all 5 contracts)

---

## Contract Inventory

| Contract | Program ID | LOC | Risk Level | Key Functions |
|----------|------------|-----|------------|---------------|
| **DAO** | `9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz` | 2,054 | Critical | Governance, treasury, voting |
| **Staking** | `85Pd1GRJ1qA3kVTn3ERHsyuUpkr2bbb9L9opwS9UnHEQ` | 1,079 | High | Token locking, slashing |
| **Registry** | `4JRL443DxceXsgqqxmBt4tD8TecBBo9Xr5kTLNRupiG6` | 724 | Medium | Node registration, heartbeat |
| **Rewards** | `8nr66XQcjr11HhMP9NU6d8j5iwX3yo59VDawQSmPWgnK` | 913 | High | Performance tracking, payouts |
| **Token** | `9uVLmgqJz3nYcCxHVSAJA8bi6412LEZ5uGM5yguvKHRq` | 1,020 | Critical | Minting, multi-sig treasury |

---

## 1. Threat Model

### 1.1 Assets at Risk

| Asset | Location | Value | Protection |
|-------|----------|-------|------------|
| **$AEGIS tokens** | Token mint | 1B supply cap | Multi-sig minting |
| **DAO Treasury** | PDA token account | Community funds | Vote + timelock |
| **Vote Vault** | PDA token account | Escrowed votes | Time-locked |
| **Staking Vault** | PDA token account | Operator stakes | Cooldown + slashing |
| **Reward Pool** | PDA token account | Node incentives | Authority-only |
| **Node Stakes** | Registry account | Reputation data | CPI-only updates |

### 1.2 Threat Actors

| Actor | Capability | Motivation | Target |
|-------|------------|------------|--------|
| **Flash Loan Attacker** | Borrow large tokens temporarily | Manipulate voting power | DAO governance |
| **Malicious Operator** | Control registered nodes | Claim unearned rewards | Rewards program |
| **Front-Runner** | Monitor mempool, submit first | Extract value | Treasury withdrawals |
| **Compromised Admin** | Access to authority keys | Drain funds | All programs |
| **Sybil Attacker** | Create many identities | Game reputation system | Registry/Rewards |

### 1.3 Attack Vectors

#### DAO Contract
| Vector | Risk | Mitigation | Status |
|--------|------|------------|--------|
| Flash loan voting | Critical | Vote Escrow Pattern - tokens locked | **FIXED** |
| Double voting | High | VoteEscrow PDA per voter per proposal | **FIXED** |
| Treasury drain | Critical | Recipient validation in execution | **FIXED** |
| Config manipulation | High | 48-hour timelock | **FIXED** |
| Proposal spam | Medium | 100 AEGIS bond, forfeited if defeated | Implemented |

#### Staking Contract
| Vector | Risk | Mitigation | Status |
|--------|------|------------|--------|
| Unauthorized slashing | Critical | Admin authority check | **FIXED** |
| Registry desync | High | CPI to registry on stake/unstake | **FIXED** |
| Cooldown bypass | Medium | Configurable cooldown period | Implemented |
| Stake manipulation | High | Only operator can stake/unstake | Implemented |

#### Registry Contract
| Vector | Risk | Mitigation | Status |
|--------|------|------------|--------|
| Stake spoofing | High | CPI-only stake updates | **FIXED** |
| Reputation manipulation | Medium | Admin/rewards-only updates | Implemented |
| Heartbeat replay | Low | Timestamp tracking | Implemented |

#### Rewards Contract
| Vector | Risk | Mitigation | Status |
|--------|------|------------|--------|
| Performance data spoofing | High | Ed25519 signature verification | Implemented |
| Oracle manipulation | Medium | Registered oracle registry | Implemented |
| Double claiming | Medium | unclaimed_rewards tracking | Implemented |

#### Token Contract
| Vector | Risk | Mitigation | Status |
|--------|------|------------|--------|
| Unauthorized minting | Critical | Multi-sig (threshold required) | Implemented |
| Supply inflation | Critical | TOTAL_SUPPLY cap (1B) | Implemented |
| Fee manipulation | Medium | Admin-only fee updates | Implemented |

---

## 2. Cross-Program Invocation (CPI) Map

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        AEGIS CPI Architecture                           │
└─────────────────────────────────────────────────────────────────────────┘

┌──────────────┐                     ┌──────────────┐
│    STAKING   │────── CPI ─────────►│   REGISTRY   │
│              │                     │              │
│ stake()      │──update_stake()────►│ NodeAccount  │
│ execute_     │                     │ .stake_amount│
│   unstake()  │──update_stake()────►│              │
│ slash_stake()│──update_stake()────►│              │
│ automated_   │                     │              │
│   slash()    │──update_stake()────►│              │
└──────────────┘                     └──────────────┘
       │                                    ▲
       │                                    │
       │              ┌──────────────┐      │
       │              │   REWARDS    │      │
       │              │              │──────┘
       │              │update_       │  update_reputation()
       │              │ reputation() │  (future CPI)
       │              └──────────────┘
       │
       │              ┌──────────────┐
       │              │     DAO      │
       │              │              │
       │              │ execute_     │───► Treasury transfer
       │              │   proposal() │     (token::transfer CPI)
       │              └──────────────┘
       │
       │              ┌──────────────┐
       └─────────────►│    TOKEN     │
                      │              │
                      │ Multi-sig    │───► SPL Token program
                      │ operations   │     (mint/transfer/burn)
                      └──────────────┘
```

### CPI Security Analysis

| Caller | Callee | Function | Authority | Risk |
|--------|--------|----------|-----------|------|
| Staking | Registry | `update_stake` | `staking_authority` PDA | Low - PDA signed |
| Staking | SPL Token | `transfer` | `stake_account` PDA | Low - PDA signed |
| DAO | SPL Token | `transfer` | `dao_config` PDA | Low - PDA signed |
| Token | SPL Token | `mint_to/transfer/burn` | `token_config` PDA | Low - PDA signed |
| Rewards | SPL Token | `transfer` | `reward_pool` PDA | Low - PDA signed |

**Key Security Properties:**
1. All CPIs use PDA-signed authorities (not user wallets)
2. Staking→Registry CPI validates `registry_program_id` in GlobalConfig
3. No re-entrancy possible (Solana's execution model)

---

## 3. Security Fixes Already Implemented

### Sprint 18.5 Critical Fixes

#### Fix 1: Vote Escrow Pattern (DAO)
- **Vulnerability:** Snapshot voting allowed flash loan attacks
- **Old Pattern:** `register_vote_snapshot()` - just reads token balance
- **New Pattern:** `deposit_vote_tokens()` - transfers tokens to PDA vault
- **Instructions:** `deposit_vote_tokens`, `cast_vote`, `retract_vote`, `withdraw_vote_tokens`
- **Account:** `VoteEscrow` PDA tracks deposited amount per voter per proposal

#### Fix 2: Rewards Access Control
- **Vulnerability:** Missing authority check on `record_performance`
- **Fix:** Added `has_one = authority` constraint to `RecordPerformance` context

#### Fix 3: Staking-Registry CPI Sync
- **Vulnerability:** Registry stake amounts could diverge from actual stakes
- **Fix:** All stake changes (`stake`, `execute_unstake`, `slash_stake`, `automated_slash`) now call `registry::update_stake()` via CPI
- **New Account:** `staking_authority` PDA for signing CPI calls

---

## 4. Vulnerability Checklist

### 4.1 Account Validation

| Check | DAO | Staking | Registry | Rewards | Token |
|-------|-----|---------|----------|---------|-------|
| Owner checks (`has_one`) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Signer requirements | ✅ | ✅ | ✅ | ✅ | ✅ |
| PDA seed validation | ✅ | ✅ | ✅ | ✅ | ✅ |
| Mint validation | ✅ | N/A | N/A | N/A | ✅ |
| Token account ownership | ✅ | ✅ | N/A | ✅ | ✅ |

### 4.2 Arithmetic Safety

| Check | DAO | Staking | Registry | Rewards | Token |
|-------|-----|---------|----------|---------|-------|
| `checked_add` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `checked_sub` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `checked_mul` | ✅ | ✅ | ✅ | ✅ | ✅ |
| `checked_div` | ✅ | ✅ | N/A | ✅ | ✅ |
| Overflow errors | ✅ | ✅ | ✅ | ✅ | ✅ |
| Underflow errors | ✅ | ✅ | N/A | ✅ | N/A |

### 4.3 Access Control

| Check | DAO | Staking | Registry | Rewards | Token |
|-------|-----|---------|----------|---------|-------|
| Authority validation | ✅ | ✅ | ✅ | ✅ | ✅ |
| Admin-only functions | ✅ | ✅ | ✅ | ✅ | ✅ |
| Timelock for config | ✅ | N/A | N/A | N/A | N/A |
| Multi-sig for critical | ✅* | N/A | N/A | N/A | ✅ |
| Pause mechanism | ✅ | ✅ | N/A | N/A | N/A |

*DAO uses vote + timelock instead of multi-sig

### 4.4 State Machine

| Check | DAO | Staking | Registry | Rewards | Token |
|-------|-----|---------|----------|---------|-------|
| Valid status transitions | ✅ | ✅ | ✅ | N/A | ✅ |
| Can't re-execute | ✅ | N/A | N/A | N/A | ✅ |
| Time constraints | ✅ | ✅ | ✅ | ✅ | N/A |

### 4.5 Economic Security

| Check | DAO | Staking | Registry | Rewards | Token |
|-------|-----|---------|----------|---------|-------|
| Flash loan resistance | ✅* | N/A | N/A | N/A | N/A |
| Front-running protection | ✅** | N/A | N/A | ✅*** | N/A |
| Griefing prevention | ✅ | ✅ | ✅ | ✅ | ✅ |

*Vote Escrow pattern
**3-day execution timelock
***Ed25519 signed attestations

---

## 5. Specific Questions for Auditors

### DAO Contract

1. **Vote Escrow Completeness:** Can tokens be withdrawn while a vote is active and counted?
   - Expected: No, `withdraw_vote_tokens` checks `voting_ended || !has_voted`

2. **Treasury Drain:** Can anyone call `execute_proposal` with arbitrary recipient?
   - Expected: No, recipient validated against `proposal.execution_data.recipient`

3. **Vote Weight Manipulation:** Can vote weight change between deposit and vote?
   - Expected: No, weight is `vote_escrow.deposited_amount` (locked at deposit time)

4. **Proposal Spam Prevention:** What happens if bonds are not returned?
   - Expected: Bonds forfeited to treasury if proposal defeated

5. **Timelock Bypass:** Can `execute_config_update` be called before timelock?
   - Expected: No, checks `clock.unix_timestamp >= pending.execute_after`

### Staking Contract

1. **Slashing Authorization:** Who can call `slash_stake`?
   - Expected: Only `global_config.admin_authority`

2. **Cooldown Reset:** Can cooldown be reset by canceling and re-requesting?
   - Expected: Yes, but stake remains locked in pending_unstake

3. **Registry Sync:** What if registry CPI fails?
   - Expected: Entire transaction reverts, stake state unchanged

4. **Stake Manipulation:** Can pending_unstake be modified externally?
   - Expected: No, only via `request_unstake` and `execute_unstake`

### Registry Contract

1. **Stake Update Authorization:** Can anyone update stake amounts?
   - Expected: No, only `staking_program_id` from config

2. **Reputation Bounds:** Can reputation exceed MAX_REPUTATION (10000)?
   - Expected: No, capped in `heartbeat` and validated in `update_reputation`

3. **Heartbeat Replay:** Can old heartbeats be replayed?
   - Expected: No practical benefit (missed heartbeats calculated from last_heartbeat)

### Rewards Contract

1. **Oracle Collusion:** What if oracle submits false performance data?
   - Expected: Requires Ed25519 signature, oracle can be deactivated

2. **Double Claiming:** Can rewards be claimed twice?
   - Expected: No, `unclaimed_rewards` set to 0 after claim

3. **Emission Overflow:** Can halving schedule cause overflow?
   - Expected: No, uses checked_* arithmetic

### Token Contract

1. **Supply Cap Enforcement:** Can mint exceed TOTAL_SUPPLY?
   - Expected: No, checked before mint CPI

2. **Multi-sig Bypass:** Can single signer execute?
   - Expected: No, `tx.approval_count >= config.threshold` required

3. **Fee Manipulation:** Who can change fee_burn_bps?
   - Expected: Only admin

---

## 6. Test Coverage

### Current Test Status

| Contract | Unit Tests | Integration Tests | Fuzz Tests | Total |
|----------|------------|-------------------|------------|-------|
| DAO | 14 | 0 | 0 | 14 |
| Staking | 10 | 0 | 0 | 10 |
| Registry | 8 | 0 | 0 | 8 |
| Rewards | 12 | 0 | 0 | 12 |
| Token | 8 | 0 | 0 | 8 |
| **Total** | **52** | **0** | **0** | **52** |

### Test Gaps

1. **No integration tests** for CPI between Staking and Registry
2. **No fuzz testing** for arithmetic operations
3. **No invariant tests** for:
   - Total votes ≤ token supply
   - Staked + pending_unstake ≤ vault balance
   - Total claimed ≤ total earned

---

## 7. Recommended Auditors

| Auditor | Specialization | Notable Audits | Est. Cost | Timeline |
|---------|---------------|----------------|-----------|----------|
| **Neodyme** | Solana, Anchor | Marinade, Solend, Raydium | $50-100K | 3-4 weeks |
| **OtterSec** | Solana, DeFi | Jupiter, Drift, Phantom | $60-120K | 4-6 weeks |
| **Trail of Bits** | General blockchain | Compound, Uniswap, Solana | $100-200K | 4-8 weeks |
| **Sec3** | Solana automated | Orca, Serum | $30-60K | 2-3 weeks |
| **Zellic** | Solana, bridges | Wormhole, LayerZero | $80-150K | 4-6 weeks |

### Recommendation

**Primary:** Neodyme or OtterSec (deep Solana/Anchor expertise)
**Secondary:** Sec3 for automated analysis
**Budget:** $80-150K total for comprehensive audit

---

## 8. Audit Scope Proposal

### In Scope

1. **All 5 Anchor programs** (~5,790 LOC total)
2. **CPI security** between Staking↔Registry
3. **Economic attacks** (flash loans, front-running)
4. **Access control** verification
5. **Arithmetic safety** review

### Out of Scope

1. Off-chain infrastructure (Node software, CLI)
2. Frontend applications
3. External dependencies (SPL Token program)
4. Denial of service (rate limiting at infra level)

### Deliverables Expected

1. **Audit report** with severity ratings (Critical/High/Medium/Low/Info)
2. **Fix verification** after remediation
3. **Executive summary** for public disclosure
4. **Private disclosure** of critical findings

---

## 9. Timeline Proposal

| Week | Activity |
|------|----------|
| 1 | Auditor kickoff, documentation handoff |
| 2-3 | Audit in progress |
| 4 | Preliminary findings, remediation begins |
| 5 | Final report, fix verification |
| 6 | Public audit report publication |

---

## 10. Appendix: Program IDs and Deployments

### Devnet Deployments

```
DAO:      9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz
Staking:  85Pd1GRJ1qA3kVTn3ERHsyuUpkr2bbb9L9opwS9UnHEQ
Registry: 4JRL443DxceXsgqqxmBt4tD8TecBBo9Xr5kTLNRupiG6
Rewards:  8nr66XQcjr11HhMP9NU6d8j5iwX3yo59VDawQSmPWgnK
Token:    9uVLmgqJz3nYcCxHVSAJA8bi6412LEZ5uGM5yguvKHRq
```

### PDA Seeds

| Program | PDA | Seeds |
|---------|-----|-------|
| DAO | dao_config | `["dao_config"]` |
| DAO | proposal | `["proposal", proposal_id.to_le_bytes()]` |
| DAO | vote_escrow | `["vote_escrow", proposal_id.to_le_bytes(), voter]` |
| DAO | vote_record | `["vote", proposal_id.to_le_bytes(), voter]` |
| Staking | global_config | `["global_config"]` |
| Staking | stake_account | `["stake", operator]` |
| Staking | staking_authority | `["staking_authority"]` |
| Staking | stake_vault | `["stake_vault"]` |
| Registry | registry_config | `["registry_config"]` |
| Registry | node_account | `["node", operator]` |
| Rewards | reward_pool | `["reward_pool"]` |
| Rewards | operator_rewards | `["operator_rewards", operator]` |
| Rewards | oracle_registry | `["oracle_registry"]` |
| Token | token_config | `["token_config", mint]` |
| Token | multisig_tx | `["multisig_tx", config, nonce.to_le_bytes()]` |
