# Sprint 18: DAO Governance Smart Contracts

**Status:** ✅ COMPLETE (Security Hardened)
**Date:** November 30, 2025
**Program ID:** `9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz` (Solana Devnet)

## Overview

Sprint 18 implements decentralized governance for the AEGIS network, enabling token holders to create proposals, vote on network changes, and manage the DAO treasury. The contract has undergone a comprehensive security review and includes protections against flash loan attacks, recipient spoofing, and centralization risks.

## Security Features (Hardened)

### Critical Protections

| Vulnerability | Protection | Implementation |
|---------------|------------|----------------|
| **Flash Loan Vote Manipulation** | Snapshot-based voting | `VoteSnapshot` account locks vote weight before voting |
| **Treasury Recipient Spoofing** | Recipient validation | Execute validates `recipient == execution_data.recipient` |
| **Token Account Spoofing** | Ownership validation | All token accounts verify `owner == signer` |
| **Wrong Token Mint** | Mint validation | All accounts verify `mint == governance_token_mint` |
| **Authority Centralization** | 48-hour timelock | Config changes require queuing + waiting period |
| **Spam Proposals** | Minimum bond | Bond must be >= 1 token (MIN_PROPOSAL_BOND) |
| **Stuck Proposals** | Cancellation | Proposers can cancel before voting ends |

### Voting Security Flow

```
1. Proposal Created → Snapshot supply recorded
2. Voter calls register_vote_snapshot() → Vote weight locked in VoteSnapshot PDA
3. (Flash loan attack impossible - weight already locked)
4. Voter calls cast_vote() → Uses snapshot weight, marks snapshot as used
5. Double voting prevented by has_voted flag + VoteRecord PDA
```

### Config Change Timelock Flow

```
1. Authority calls queue_config_update() → Changes queued
2. Community has 48 hours to review proposed changes
3. Authority calls execute_config_update() → Changes applied (or cancel_config_update())
```

## Deliverables

### 1. DAO Governance Smart Contract (`contracts/dao/`)

Full-featured Anchor program with security hardening:

#### Core Accounts

| Account | Purpose | Key Fields |
|---------|---------|------------|
| **DaoConfig** | Global configuration | `authority`, `treasury`, `bond_escrow`, `governance_token_mint`, `pending_config_change` |
| **Proposal** | Individual proposal | `proposal_id`, `proposer`, `status`, `for_votes`, `against_votes`, `abstain_votes`, `snapshot_supply` |
| **VoteSnapshot** | Locked vote weight | `voter`, `vote_weight`, `has_voted`, `registered_at` |
| **VoteRecord** | Permanent vote record | `proposal_id`, `voter`, `vote_choice`, `vote_weight` |

#### Instructions (13 total)

| Instruction | Description | Access |
|-------------|-------------|--------|
| `initialize_dao` | One-time DAO setup with PDA-owned treasury/escrow | Deployer |
| `queue_config_update` | Queue config changes (48h timelock) | Authority only |
| `execute_config_update` | Execute after timelock expires | Authority only |
| `cancel_config_update` | Cancel pending changes | Authority only |
| `set_dao_paused` | Emergency pause/unpause | Authority only |
| `create_proposal` | Submit proposal (requires bond) | Any token holder |
| `cancel_proposal` | Cancel and reclaim bond | Proposer only (before voting ends) |
| `register_vote_snapshot` | Lock vote weight for proposal | Any token holder |
| `cast_vote` | Vote using snapshot weight | Token holders with snapshot |
| `finalize_proposal` | Close voting, determine outcome | Anyone (after voting ends) |
| `execute_proposal` | Execute treasury withdrawal | Anyone (validates recipient) |
| `return_proposal_bond` | Return bond to proposer | Proposer (after passing) |
| `deposit_to_treasury` | Add funds to DAO treasury | Anyone |

### 2. Proposal Types

```rust
pub enum ProposalType {
    General,            // Non-executable governance proposal
    TreasuryWithdrawal, // Executable fund transfer (recipient validated)
    ParameterChange,    // Config updates (future)
}
```

### 3. Proposal Lifecycle

```
┌─────────────────┐
│  Create Proposal │ ← Proposer deposits bond (100 AEGIS)
└────────┬────────┘   Supply snapshot recorded
         ▼
┌─────────────────┐
│     Active      │ ← Voters register snapshots, then vote
└────────┬────────┘   (Can be cancelled by proposer)
         ▼
┌─────────────────┐
│    Finalize     │ ← Anyone can finalize after voting ends
└────────┬────────┘   Uses snapshot_supply for quorum
         ▼
    ┌────┴────┐
    ▼         ▼
┌───────┐ ┌───────────┐
│Passed │ │ Defeated  │
└───┬───┘ └─────┬─────┘
    │           │
    ▼           ▼
┌───────────┐ ┌─────────────┐
│ Execute   │ │Bond Forfeited│
│(Recipient │ │to Treasury  │
│ Validated)│ │             │
└───────────┘ └─────────────┘
```

### 4. Voting Mechanism (Snapshot-Based)

- **Snapshot Registration**: Voters must register before voting to lock their vote weight
- **Flash Loan Resistant**: Vote weight determined at snapshot time, not vote time
- **Token-Weighted Voting**: Vote weight = token balance at snapshot registration
- **Vote Choices**: For, Against, Abstain
- **Quorum**: Minimum participation required (default 10% of snapshot supply)
- **Approval Threshold**: Percentage of FOR votes needed (default 51%)
- **Double-Vote Prevention**: `has_voted` flag on VoteSnapshot + VoteRecord PDA

### 5. Security Constraints

| Constraint | Validation |
|------------|------------|
| Token account ownership | `constraint = token_account.owner == signer.key()` |
| Token mint | `constraint = token_account.mint == governance_token_mint` |
| Treasury ownership | `constraint = treasury.owner == dao_config.key()` |
| Bond escrow ownership | `constraint = bond_escrow.owner == dao_config.key()` |
| Recipient validation | `require!(recipient.key() == execution_data.recipient)` |
| Treasury balance | `require!(treasury.amount >= execution_data.amount)` |
| Minimum bond | `require!(bond >= MIN_PROPOSAL_BOND)` |

### 6. Parameters

```rust
// Voting period bounds
MIN_VOTING_PERIOD: 1 day (86,400 seconds)
MAX_VOTING_PERIOD: 14 days (1,209,600 seconds)
DEFAULT_VOTING_PERIOD: 3 days (259,200 seconds)

// Bond
MIN_PROPOSAL_BOND: 1 AEGIS token (1_000_000_000 lamports)
DEFAULT_PROPOSAL_BOND: 100 AEGIS tokens

// Governance
DEFAULT_QUORUM_PERCENTAGE: 10%
DEFAULT_APPROVAL_THRESHOLD: 51%

// Timelock
CONFIG_TIMELOCK_DELAY: 48 hours (172,800 seconds)

// Limits
MAX_TITLE_LENGTH: 128 characters
MAX_DESCRIPTION_CID_LENGTH: 64 characters (IPFS CID)
```

## Events Emitted

```rust
DaoInitializedEvent        // DAO setup complete
ConfigUpdateQueuedEvent    // Config change queued with timelock
ConfigUpdateExecutedEvent  // Config change applied
ConfigUpdateCancelledEvent // Config change cancelled
DaoPausedEvent             // Pause status changed
ProposalCreatedEvent       // New proposal with snapshot_supply
ProposalCancelledEvent     // Proposal cancelled by proposer
VoteSnapshotRegisteredEvent // Vote weight locked
VoteCastEvent              // Vote recorded
ProposalFinalizedEvent     // Voting concluded
ProposalExecutedEvent      // Treasury withdrawal executed
BondReturnedEvent          // Bond returned to proposer
TreasuryDepositEvent       // Funds added to treasury
```

## Error Codes

| Error | Description |
|-------|-------------|
| `InvalidVotingPeriod` | Voting period must be 1-14 days |
| `InvalidProposalBond` | Bond must be >= 1 token |
| `InvalidQuorumPercentage` | Quorum must be 1-100% |
| `InvalidApprovalThreshold` | Threshold must be 1-100% |
| `DaoPaused` | DAO is currently paused |
| `ProposalNotActive` | Proposal is not in Active status |
| `VotingNotActive` | Outside voting period |
| `NoVotingPower` | Zero token balance at snapshot |
| `AlreadyVoted` | Snapshot already used for voting |
| `InvalidTokenOwner` | Token account owner doesn't match signer |
| `InvalidMint` | Token account mint doesn't match governance token |
| `InvalidRecipient` | Recipient doesn't match proposal execution data |
| `InsufficientTreasuryBalance` | Treasury doesn't have enough tokens |
| `NoPendingConfigChange` | No config change queued |
| `TimelockNotExpired` | 48 hours haven't passed |

## Usage Examples

### Register Vote Snapshot (Required Before Voting)

```typescript
// Step 1: Register vote weight snapshot
await program.methods
  .registerVoteSnapshot()
  .accounts({
    daoConfig: daoConfigPda,
    proposal: proposalPda,
    voteSnapshot: voteSnapshotPda,
    voterTokenAccount: voterTokenAccount,
    voter: voter.publicKey,
    systemProgram: SystemProgram.programId,
  })
  .signers([voter])
  .rpc();

// Step 2: Cast vote using locked weight
await program.methods
  .castVote({ for: {} })
  .accounts({
    daoConfig: daoConfigPda,
    proposal: proposalPda,
    voteSnapshot: voteSnapshotPda,
    voteRecord: voteRecordPda,
    voter: voter.publicKey,
    systemProgram: SystemProgram.programId,
  })
  .signers([voter])
  .rpc();
```

### Queue and Execute Config Update (with Timelock)

```typescript
// Step 1: Queue the change
await program.methods
  .queueConfigUpdate(
    new anchor.BN(5 * 24 * 60 * 60), // New voting period: 5 days
    null, // No bond change
    15, // New quorum: 15%
    null // No threshold change
  )
  .accounts({
    daoConfig: daoConfigPda,
    authority: authority.publicKey,
  })
  .signers([authority])
  .rpc();

// Step 2: Wait 48 hours...

// Step 3: Execute the change
await program.methods
  .executeConfigUpdate()
  .accounts({
    daoConfig: daoConfigPda,
    authority: authority.publicKey,
  })
  .signers([authority])
  .rpc();
```

### Cancel a Proposal

```typescript
await program.methods
  .cancelProposal()
  .accounts({
    daoConfig: daoConfigPda,
    proposal: proposalPda,
    bondEscrow: bondEscrow,
    proposerTokenAccount: proposerTokenAccount,
    proposer: proposer.publicKey,
    tokenProgram: TOKEN_PROGRAM_ID,
  })
  .signers([proposer])
  .rpc();
```

## Security Audit Checklist

| Category | Item | Status |
|----------|------|--------|
| **Flash Loan** | Vote weight from snapshot, not current balance | ✅ |
| **Double Voting** | VoteSnapshot.has_voted + VoteRecord PDA | ✅ |
| **Recipient Spoofing** | Validated in execute_proposal | ✅ |
| **Token Ownership** | Constraint on all token accounts | ✅ |
| **Mint Validation** | Constraint on all token accounts | ✅ |
| **Treasury Ownership** | Must be owned by DAO PDA | ✅ |
| **Arithmetic Overflow** | checked_* operations throughout | ✅ |
| **Authority Centralization** | 48h timelock on config changes | ✅ |
| **Spam Prevention** | Minimum proposal bond (1 token) | ✅ |
| **Stuck Proposals** | Cancel before voting ends | ✅ |
| **Treasury Balance** | Checked before execution | ✅ |

## File Structure

```
contracts/dao/
├── Anchor.toml          # Anchor configuration
├── Cargo.toml           # Workspace Cargo file
├── package.json         # Node.js dependencies
├── tsconfig.json        # TypeScript configuration
├── programs/
│   └── dao/
│       ├── Cargo.toml   # Program Cargo file
│       └── src/
│           └── lib.rs   # Main DAO program (1600+ lines, security hardened)
└── tests/
    └── dao.ts           # Comprehensive test suite
```

## Phase 3 Completion

With Sprint 18 complete and security hardened, **Phase 3 is now 100% complete**:

| Sprint | Component | Status |
|--------|-----------|--------|
| 13 | Wasm Edge Functions | ✅ |
| 14 | Extended Host API | ✅ |
| 15 | WAF Wasm + Ed25519 | ✅ |
| 15.5 | Architectural Cleanup | ✅ |
| 16 | Route-based Dispatch | ✅ |
| 17 | IPFS/Filecoin CDN | ✅ |
| **18** | **DAO Governance (Secured)** | ✅ |

**Next:** Phase 4 - Advanced Security & Mainnet Preparation
