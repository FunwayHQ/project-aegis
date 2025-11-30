// Core client
export { DaoClient } from "./client";

// PDA helpers
export {
  getDaoConfigPDA,
  getProposalPDA,
  getVoteEscrowPDA,
  getVoteRecordPDA,
} from "./pda";

// Types
export {
  // Enums
  ProposalType,
  ProposalStatus,
  VoteChoice,
  // Account types
  type DaoConfig,
  type Proposal,
  type VoteEscrow,
  type VoteRecord,
  type PendingConfigChange,
  type ExecutionData,
  // Instruction params
  type InitializeDaoParams,
  type CreateProposalParams,
  type QueueConfigUpdateParams,
  type DepositVoteTokensParams,
  type CastVoteParams,
  type DepositToTreasuryParams,
  // Filters
  type ProposalFilter,
  // Helpers
  parseProposalType,
  parseProposalStatus,
  parseVoteChoice,
  toAnchorProposalType,
  toAnchorVoteChoice,
} from "./types";

// Constants
export {
  DAO_PROGRAM_ID,
  SEEDS,
  DEFAULTS,
  AEGIS_DECIMALS,
  CLUSTERS,
  type ClusterName,
} from "./constants";
