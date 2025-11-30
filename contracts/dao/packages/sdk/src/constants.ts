import { PublicKey } from "@solana/web3.js";

/**
 * DAO Program ID on Devnet
 */
export const DAO_PROGRAM_ID = new PublicKey(
  "9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz"
);

/**
 * PDA Seeds
 */
export const SEEDS = {
  DAO_CONFIG: Buffer.from("dao_config"),
  PROPOSAL: Buffer.from("proposal"),
  VOTE_ESCROW: Buffer.from("vote_escrow"),
  VOTE_RECORD: Buffer.from("vote"),
} as const;

/**
 * Default configuration values (Whitepaper Compliant)
 */
export const DEFAULTS = {
  // Voting periods per whitepaper
  DISCUSSION_PERIOD_DAYS: 7,
  DISCUSSION_PERIOD_SECONDS: 7 * 24 * 60 * 60, // 7 days before voting starts
  VOTING_PERIOD_DAYS: 7,
  VOTING_PERIOD_SECONDS: 7 * 24 * 60 * 60, // 7 days voting window
  MIN_VOTING_PERIOD_SECONDS: 3 * 24 * 60 * 60, // 3 days minimum
  MAX_VOTING_PERIOD_SECONDS: 14 * 24 * 60 * 60, // 14 days maximum

  // Execution timelock per whitepaper
  EXECUTION_TIMELOCK_DAYS: 3,
  EXECUTION_TIMELOCK_SECONDS: 3 * 24 * 60 * 60, // 3 days after voting ends

  // Proposal requirements
  PROPOSAL_BOND: 100_000_000_000n, // 100 AEGIS (9 decimals)
  MIN_PROPOSAL_BOND: 1_000_000_000n, // 1 AEGIS

  // Voting thresholds
  QUORUM_PERCENTAGE: 10,
  APPROVAL_THRESHOLD: 51,

  // Config change timelock
  CONFIG_TIMELOCK_SECONDS: 48 * 60 * 60, // 48 hours

  // Limits
  MAX_TITLE_LENGTH: 128,
  MAX_DESCRIPTION_CID_LENGTH: 64,
} as const;

/**
 * Token decimals for AEGIS
 */
export const AEGIS_DECIMALS = 9;

/**
 * Cluster endpoints
 */
export const CLUSTERS = {
  devnet: "https://api.devnet.solana.com",
  mainnet: "https://api.mainnet-beta.solana.com",
  localnet: "http://localhost:8899",
} as const;

export type ClusterName = keyof typeof CLUSTERS;
