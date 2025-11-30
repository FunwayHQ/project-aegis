import chalk from "chalk";
import BN from "bn.js";
import { PublicKey } from "@solana/web3.js";
import type { DaoConfig, Proposal, VoteEscrow, VoteRecord } from "@aegis/dao-sdk";
import { AEGIS_DECIMALS } from "@aegis/dao-sdk";

/**
 * Format token amount with decimals
 */
export function formatTokenAmount(amount: BN | bigint, decimals: number = AEGIS_DECIMALS): string {
  const amountBn = BN.isBN(amount) ? amount : new BN(amount.toString());
  const divisor = new BN(10).pow(new BN(decimals));
  const whole = amountBn.div(divisor);
  const remainder = amountBn.mod(divisor);

  if (remainder.isZero()) {
    return whole.toString();
  }

  const remainderStr = remainder.toString().padStart(decimals, "0");
  const trimmed = remainderStr.replace(/0+$/, "");
  return `${whole}.${trimmed}`;
}

/**
 * Format timestamp to human readable
 */
export function formatTimestamp(timestamp: BN): string {
  const date = new Date(timestamp.toNumber() * 1000);
  return date.toLocaleString();
}

/**
 * Format duration in seconds to human readable
 */
export function formatDuration(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  const parts: string[] = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);

  return parts.length > 0 ? parts.join(" ") : "0m";
}

/**
 * Shorten a public key for display
 */
export function shortAddress(pubkey: PublicKey | string): string {
  const str = typeof pubkey === "string" ? pubkey : pubkey.toString();
  return `${str.slice(0, 4)}...${str.slice(-4)}`;
}

/**
 * Display DAO config info (Whitepaper Compliant)
 */
export function displayDaoConfig(config: DaoConfig): void {
  console.log(chalk.bold("\nDAO Configuration (Whitepaper Compliant)"));
  console.log(chalk.gray("─".repeat(50)));
  console.log(`  Authority:          ${config.authority.toString()}`);
  console.log(`  Treasury:           ${config.treasury.toString()}`);
  console.log(`  Bond Escrow:        ${config.bondEscrow.toString()}`);
  console.log(`  Vote Vault:         ${config.voteVault.toString()}`);
  console.log(`  Governance Token:   ${config.governanceTokenMint.toString()}`);
  console.log(chalk.gray("─".repeat(50)));
  console.log(chalk.cyan("  Governance Periods:"));
  console.log(`    Discussion Period:   ${formatDuration(config.discussionPeriod?.toNumber() || 0)} ${chalk.gray("(before voting)")}`);
  console.log(`    Voting Period:       ${formatDuration(config.votingPeriod.toNumber())}`);
  console.log(`    Execution Timelock:  ${chalk.gray("3 days (after voting ends)")}`);
  console.log(chalk.gray("─".repeat(50)));
  console.log(`  Proposal Bond:      ${formatTokenAmount(config.proposalBond)} AEGIS`);
  console.log(`  Quorum:             ${config.quorumPercentage}%`);
  console.log(`  Approval Threshold: ${config.approvalThreshold}%`);
  console.log(chalk.gray("─".repeat(50)));
  console.log(`  Proposal Count:     ${config.proposalCount.toString()}`);
  console.log(`  Treasury Deposits:  ${formatTokenAmount(config.totalTreasuryDeposits)} AEGIS`);
  console.log(`  Status:             ${config.paused ? chalk.red("PAUSED") : chalk.green("ACTIVE")}`);

  if (config.pendingConfigChange) {
    console.log(chalk.yellow("\n  Pending Config Change:"));
    const change = config.pendingConfigChange;
    if (change.newVotingPeriod) {
      console.log(`    New Voting Period:      ${formatDuration(change.newVotingPeriod.toNumber())}`);
    }
    if (change.newProposalBond) {
      console.log(`    New Proposal Bond:      ${formatTokenAmount(change.newProposalBond)} AEGIS`);
    }
    if (change.newQuorumPercentage !== null) {
      console.log(`    New Quorum:             ${change.newQuorumPercentage}%`);
    }
    if (change.newApprovalThreshold !== null) {
      console.log(`    New Approval Threshold: ${change.newApprovalThreshold}%`);
    }
    console.log(`    Execute After:          ${formatTimestamp(change.executeAfter)}`);
  }
  console.log();
}

/**
 * Display proposal info (Whitepaper Compliant)
 */
export function displayProposal(proposal: Proposal, verbose: boolean = false): void {
  const statusColors: Record<string, (s: string) => string> = {
    active: chalk.blue,
    passed: chalk.green,
    defeated: chalk.red,
    executed: chalk.cyan,
    cancelled: chalk.gray,
  };

  const statusColor = statusColors[proposal.status] || chalk.white;

  console.log(chalk.bold(`\nProposal #${proposal.proposalId.toString()}`));
  console.log(chalk.gray("─".repeat(50)));
  console.log(`  Title:      ${proposal.title}`);
  console.log(`  Type:       ${proposal.proposalType}`);
  console.log(`  Status:     ${statusColor(proposal.status.toUpperCase())}`);
  console.log(`  Proposer:   ${shortAddress(proposal.proposer)}`);

  if (verbose) {
    console.log(`  CID:        ${proposal.descriptionCid}`);
    console.log(chalk.cyan("  Timeline:"));
    console.log(`    Created:              ${formatTimestamp(proposal.createdAt)}`);
    console.log(`    Voting Start:         ${formatTimestamp(proposal.voteStart)}`);
    console.log(`    Voting End:           ${formatTimestamp(proposal.voteEnd)}`);
    if (proposal.executionEligibleAt) {
      console.log(`    Execution Eligible:   ${formatTimestamp(proposal.executionEligibleAt)} ${chalk.gray("(3-day timelock)")}`);
    }
  }

  console.log(chalk.gray("─".repeat(50)));
  console.log(`  For:        ${formatTokenAmount(proposal.forVotes)} AEGIS`);
  console.log(`  Against:    ${formatTokenAmount(proposal.againstVotes)} AEGIS`);
  console.log(`  Abstain:    ${formatTokenAmount(proposal.abstainVotes)} AEGIS`);

  if (proposal.executionData) {
    console.log(chalk.gray("─".repeat(50)));
    console.log(chalk.yellow("  Execution Data:"));
    console.log(`    Recipient: ${proposal.executionData.recipient.toString()}`);
    console.log(`    Amount:    ${formatTokenAmount(proposal.executionData.amount)} AEGIS`);
  }

  console.log();
}

/**
 * Display vote escrow info
 */
export function displayVoteEscrow(escrow: VoteEscrow): void {
  console.log(chalk.bold("\nVote Escrow"));
  console.log(chalk.gray("─".repeat(50)));
  console.log(`  Proposal ID:   ${escrow.proposalId.toString()}`);
  console.log(`  Voter:         ${shortAddress(escrow.voter)}`);
  console.log(`  Deposited:     ${formatTokenAmount(escrow.depositedAmount)} AEGIS`);
  console.log(`  Deposited At:  ${formatTimestamp(escrow.depositedAt)}`);
  console.log(`  Has Voted:     ${escrow.hasVoted ? chalk.green("Yes") : chalk.yellow("No")}`);
  if (escrow.voteChoice) {
    console.log(`  Vote Choice:   ${escrow.voteChoice}`);
  }
  console.log(`  Withdrawn:     ${escrow.withdrawn ? chalk.green("Yes") : chalk.yellow("No")}`);
  console.log();
}

/**
 * Display vote record info
 */
export function displayVoteRecord(record: VoteRecord): void {
  const choiceColors: Record<string, (s: string) => string> = {
    for: chalk.green,
    against: chalk.red,
    abstain: chalk.gray,
  };

  const choiceColor = choiceColors[record.voteChoice] || chalk.white;

  console.log(chalk.bold("\nVote Record"));
  console.log(chalk.gray("─".repeat(50)));
  console.log(`  Proposal ID: ${record.proposalId.toString()}`);
  console.log(`  Voter:       ${shortAddress(record.voter)}`);
  console.log(`  Choice:      ${choiceColor(record.voteChoice.toUpperCase())}`);
  console.log(`  Weight:      ${formatTokenAmount(record.voteWeight)} AEGIS`);
  console.log(`  Voted At:    ${formatTimestamp(record.votedAt)}`);
  console.log();
}

/**
 * Display success message
 */
export function success(message: string, signature?: string): void {
  console.log(chalk.green(`\n✓ ${message}`));
  if (signature) {
    console.log(chalk.gray(`  Transaction: ${signature}`));
    console.log(chalk.gray(`  Explorer: https://explorer.solana.com/tx/${signature}?cluster=devnet`));
  }
  console.log();
}

/**
 * Display error message
 */
export function error(message: string, err?: Error): void {
  console.log(chalk.red(`\n✗ ${message}`));
  if (err) {
    console.log(chalk.gray(`  ${err.message}`));
  }
  console.log();
}

/**
 * Display info message
 */
export function info(message: string): void {
  console.log(chalk.blue(`ℹ ${message}`));
}

/**
 * Display warning message
 */
export function warn(message: string): void {
  console.log(chalk.yellow(`⚠ ${message}`));
}
