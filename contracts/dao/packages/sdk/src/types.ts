import { PublicKey } from "@solana/web3.js";
import BN from "bn.js";

// ============================================================================
// ENUMS
// ============================================================================

/**
 * Proposal type enumeration
 */
export enum ProposalType {
  General = "general",
  TreasuryWithdrawal = "treasuryWithdrawal",
  ParameterChange = "parameterChange",
}

/**
 * Proposal status enumeration
 */
export enum ProposalStatus {
  Active = "active",
  Passed = "passed",
  Defeated = "defeated",
  Executed = "executed",
  Cancelled = "cancelled",
}

/**
 * Vote choice enumeration
 */
export enum VoteChoice {
  For = "for",
  Against = "against",
  Abstain = "abstain",
}

// ============================================================================
// ACCOUNT TYPES
// ============================================================================

/**
 * DAO configuration account (Whitepaper Compliant)
 */
export interface DaoConfig {
  authority: PublicKey;
  treasury: PublicKey;
  governanceTokenMint: PublicKey;
  bondEscrow: PublicKey;
  voteVault: PublicKey;
  votingPeriod: BN;
  discussionPeriod: BN; // 7 days before voting starts (whitepaper)
  proposalBond: BN;
  quorumPercentage: number;
  approvalThreshold: number;
  proposalCount: BN;
  totalTreasuryDeposits: BN;
  paused: boolean;
  pendingConfigChange: PendingConfigChange | null;
  bump: number;
}

/**
 * Proposal account (Whitepaper Compliant)
 */
export interface Proposal {
  proposalId: BN;
  proposer: PublicKey;
  title: string;
  descriptionCid: string;
  proposalType: ProposalType;
  executionData: ExecutionData | null;
  status: ProposalStatus;
  forVotes: BN;
  againstVotes: BN;
  abstainVotes: BN;
  voteStart: BN;
  voteEnd: BN;
  executionEligibleAt: BN; // 3-day timelock after voting ends (whitepaper)
  createdAt: BN;
  executedAt: BN | null;
  bondReturned: boolean;
  snapshotSupply: BN;
  bump: number;
}

/**
 * Vote Escrow account (for flash loan protection)
 */
export interface VoteEscrow {
  proposalId: BN;
  voter: PublicKey;
  depositedAmount: BN;
  depositedAt: BN;
  hasVoted: boolean;
  voteChoice: VoteChoice | null;
  withdrawn: boolean;
  bump: number;
}

/**
 * Vote Record account
 */
export interface VoteRecord {
  proposalId: BN;
  voter: PublicKey;
  voteChoice: VoteChoice;
  voteWeight: BN;
  votedAt: BN;
  bump: number;
}

/**
 * Pending configuration change (subject to timelock)
 */
export interface PendingConfigChange {
  newVotingPeriod: BN | null;
  newProposalBond: BN | null;
  newQuorumPercentage: number | null;
  newApprovalThreshold: number | null;
  queuedAt: BN;
  executeAfter: BN;
}

/**
 * Execution data for treasury withdrawal proposals
 */
export interface ExecutionData {
  recipient: PublicKey;
  amount: BN;
}

// ============================================================================
// INSTRUCTION PARAMS
// ============================================================================

/**
 * Parameters for initializing the DAO (Whitepaper Compliant)
 */
export interface InitializeDaoParams {
  votingPeriod: BN | number;
  discussionPeriod: BN | number; // 7 days default per whitepaper
  proposalBond: BN | number | bigint;
  quorumPercentage: number;
  approvalThreshold: number;
  treasury: PublicKey;
  bondEscrow: PublicKey;
  voteVault: PublicKey;
  governanceTokenMint: PublicKey;
}

/**
 * Parameters for creating a proposal
 */
export interface CreateProposalParams {
  title: string;
  descriptionCid: string;
  proposalType: ProposalType;
  executionData?: {
    recipient: PublicKey;
    amount: BN | number | bigint;
  };
  proposerTokenAccount: PublicKey;
}

/**
 * Parameters for queuing a config update
 */
export interface QueueConfigUpdateParams {
  newVotingPeriod?: BN | number | null;
  newProposalBond?: BN | number | bigint | null;
  newQuorumPercentage?: number | null;
  newApprovalThreshold?: number | null;
}

/**
 * Parameters for depositing vote tokens
 */
export interface DepositVoteTokensParams {
  proposalId: BN | number | bigint;
  amount: BN | number | bigint;
  voterTokenAccount: PublicKey;
}

/**
 * Parameters for casting a vote
 */
export interface CastVoteParams {
  proposalId: BN | number | bigint;
  voteChoice: VoteChoice;
}

/**
 * Parameters for treasury deposit
 */
export interface DepositToTreasuryParams {
  amount: BN | number | bigint;
  depositorTokenAccount: PublicKey;
}

// ============================================================================
// FILTER TYPES
// ============================================================================

/**
 * Filter options for listing proposals
 */
export interface ProposalFilter {
  status?: ProposalStatus;
  proposer?: PublicKey;
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Convert anchor enum to SDK enum for ProposalType
 */
export function parseProposalType(anchorType: object): ProposalType {
  if ("general" in anchorType) return ProposalType.General;
  if ("treasuryWithdrawal" in anchorType) return ProposalType.TreasuryWithdrawal;
  if ("parameterChange" in anchorType) return ProposalType.ParameterChange;
  throw new Error(`Unknown proposal type: ${JSON.stringify(anchorType)}`);
}

/**
 * Convert anchor enum to SDK enum for ProposalStatus
 */
export function parseProposalStatus(anchorStatus: object): ProposalStatus {
  if ("active" in anchorStatus) return ProposalStatus.Active;
  if ("passed" in anchorStatus) return ProposalStatus.Passed;
  if ("defeated" in anchorStatus) return ProposalStatus.Defeated;
  if ("executed" in anchorStatus) return ProposalStatus.Executed;
  if ("cancelled" in anchorStatus) return ProposalStatus.Cancelled;
  throw new Error(`Unknown proposal status: ${JSON.stringify(anchorStatus)}`);
}

/**
 * Convert anchor enum to SDK enum for VoteChoice
 */
export function parseVoteChoice(anchorChoice: object): VoteChoice {
  if ("for" in anchorChoice) return VoteChoice.For;
  if ("against" in anchorChoice) return VoteChoice.Against;
  if ("abstain" in anchorChoice) return VoteChoice.Abstain;
  throw new Error(`Unknown vote choice: ${JSON.stringify(anchorChoice)}`);
}

/**
 * Convert SDK ProposalType to anchor format
 */
export function toAnchorProposalType(type: ProposalType): object {
  switch (type) {
    case ProposalType.General:
      return { general: {} };
    case ProposalType.TreasuryWithdrawal:
      return { treasuryWithdrawal: {} };
    case ProposalType.ParameterChange:
      return { parameterChange: {} };
  }
}

/**
 * Convert SDK VoteChoice to anchor format
 */
export function toAnchorVoteChoice(choice: VoteChoice): object {
  switch (choice) {
    case VoteChoice.For:
      return { for: {} };
    case VoteChoice.Against:
      return { against: {} };
    case VoteChoice.Abstain:
      return { abstain: {} };
  }
}
