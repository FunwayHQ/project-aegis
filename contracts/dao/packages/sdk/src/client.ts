import {
  Connection,
  PublicKey,
  TransactionSignature,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import BN from "bn.js";
import { DAO_PROGRAM_ID } from "./constants";
import {
  getDaoConfigPDA,
  getProposalPDA,
  getVoteEscrowPDA,
  getVoteRecordPDA,
} from "./pda";
import {
  DaoConfig,
  Proposal,
  VoteEscrow,
  VoteRecord,
  ProposalStatus,
  ProposalType,
  VoteChoice,
  InitializeDaoParams,
  CreateProposalParams,
  QueueConfigUpdateParams,
  DepositVoteTokensParams,
  CastVoteParams,
  DepositToTreasuryParams,
  ProposalFilter,
  parseProposalType,
  parseProposalStatus,
  parseVoteChoice,
  toAnchorProposalType,
  toAnchorVoteChoice,
} from "./types";

// Import IDL (will be copied during build)
// eslint-disable-next-line @typescript-eslint/no-explicit-any
import idl from "./idl/dao.json";

/**
 * DaoClient - Main client for interacting with the AEGIS DAO program
 */
export class DaoClient {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  public readonly program: Program<any>;
  public readonly connection: Connection;
  public readonly programId: PublicKey;

  constructor(
    connection: Connection,
    wallet: Wallet,
    programId: PublicKey = DAO_PROGRAM_ID
  ) {
    this.connection = connection;
    this.programId = programId;

    const provider = new AnchorProvider(connection, wallet, {
      commitment: "confirmed",
    });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    this.program = new Program(idl as any, provider);
  }

  // ============================================================================
  // READ OPERATIONS
  // ============================================================================

  /**
   * Get DAO configuration
   */
  async getDaoConfig(): Promise<DaoConfig> {
    const [pda] = getDaoConfigPDA(this.programId);
    const account = await (this.program.account as any).daoConfig.fetch(pda);
    return this.parseDaoConfig(account);
  }

  /**
   * Get a single proposal by ID
   */
  async getProposal(proposalId: BN | number | bigint): Promise<Proposal> {
    const id = this.toBN(proposalId);
    const [pda] = getProposalPDA(id, this.programId);
    const account = await (this.program.account as any).proposal.fetch(pda);
    return this.parseProposal(account);
  }

  /**
   * Get all proposals, optionally filtered
   */
  async getProposals(filter?: ProposalFilter): Promise<Proposal[]> {
    const accounts = await (this.program.account as any).proposal.all();
    let proposals = accounts.map((a: any) => this.parseProposal(a.account));

    if (filter?.status) {
      proposals = proposals.filter((p: Proposal) => p.status === filter.status);
    }
    if (filter?.proposer) {
      proposals = proposals.filter((p: Proposal) =>
        p.proposer.equals(filter.proposer!)
      );
    }

    // Sort by proposal ID descending (newest first)
    return proposals.sort((a: Proposal, b: Proposal) => b.proposalId.cmp(a.proposalId));
  }

  /**
   * Get active proposals
   */
  async getActiveProposals(): Promise<Proposal[]> {
    return this.getProposals({ status: ProposalStatus.Active });
  }

  /**
   * Get vote escrow for a voter on a proposal
   */
  async getVoteEscrow(
    proposalId: BN | number | bigint,
    voter: PublicKey
  ): Promise<VoteEscrow | null> {
    const id = this.toBN(proposalId);
    const [pda] = getVoteEscrowPDA(id, voter, this.programId);
    try {
      const account = await (this.program.account as any).voteEscrow.fetch(pda);
      return this.parseVoteEscrow(account);
    } catch {
      return null;
    }
  }

  /**
   * Get vote record for a voter on a proposal
   */
  async getVoteRecord(
    proposalId: BN | number | bigint,
    voter: PublicKey
  ): Promise<VoteRecord | null> {
    const id = this.toBN(proposalId);
    const [pda] = getVoteRecordPDA(id, voter, this.programId);
    try {
      const account = await (this.program.account as any).voteRecord.fetch(pda);
      return this.parseVoteRecord(account);
    } catch {
      return null;
    }
  }

  /**
   * Get treasury balance
   */
  async getTreasuryBalance(): Promise<bigint> {
    const config = await this.getDaoConfig();
    const account = await this.connection.getTokenAccountBalance(config.treasury);
    return BigInt(account.value.amount);
  }

  // ============================================================================
  // WRITE OPERATIONS
  // ============================================================================

  /**
   * Initialize the DAO (one-time setup)
   */
  async initializeDao(params: InitializeDaoParams): Promise<TransactionSignature> {
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .initializeDao(
        new BN(params.votingPeriod),
        this.toBN(params.proposalBond),
        params.quorumPercentage,
        params.approvalThreshold
      )
      .accounts({
        daoConfig: daoConfigPDA,
        treasury: params.treasury,
        bondEscrow: params.bondEscrow,
        voteVault: params.voteVault,
        governanceTokenMint: params.governanceTokenMint,
        authority: this.provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  /**
   * Create a new proposal
   */
  async createProposal(params: CreateProposalParams): Promise<TransactionSignature> {
    const config = await this.getDaoConfig();
    const proposalId = config.proposalCount;
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(proposalId, this.programId);

    const executionData = params.executionData
      ? {
          recipient: params.executionData.recipient,
          amount: this.toBN(params.executionData.amount),
        }
      : null;

    return (this.program.methods as any)
      .createProposal(
        params.title,
        params.descriptionCid,
        toAnchorProposalType(params.proposalType) as any,
        executionData
      )
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        bondEscrow: config.bondEscrow,
        proposerTokenAccount: params.proposerTokenAccount,
        governanceTokenMint: config.governanceTokenMint,
        proposer: this.provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  /**
   * Deposit tokens to vote escrow (required before voting)
   */
  async depositVoteTokens(params: DepositVoteTokensParams): Promise<TransactionSignature> {
    const id = this.toBN(params.proposalId);
    const config = await this.getDaoConfig();
    const voter = this.provider.wallet.publicKey;

    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);
    const [voteEscrowPDA] = getVoteEscrowPDA(id, voter, this.programId);

    return (this.program.methods as any)
      .depositVoteTokens(this.toBN(params.amount))
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        voteEscrow: voteEscrowPDA,
        voteVault: config.voteVault,
        voterTokenAccount: params.voterTokenAccount,
        voter,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  /**
   * Cast a vote using escrowed tokens
   */
  async castVote(params: CastVoteParams): Promise<TransactionSignature> {
    const id = this.toBN(params.proposalId);
    const voter = this.provider.wallet.publicKey;

    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);
    const [voteEscrowPDA] = getVoteEscrowPDA(id, voter, this.programId);
    const [voteRecordPDA] = getVoteRecordPDA(id, voter, this.programId);

    return (this.program.methods as any)
      .castVote(toAnchorVoteChoice(params.voteChoice) as any)
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        voteEscrow: voteEscrowPDA,
        voteRecord: voteRecordPDA,
        voter,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  /**
   * Retract a vote (allows token withdrawal)
   */
  async retractVote(proposalId: BN | number | bigint): Promise<TransactionSignature> {
    const id = this.toBN(proposalId);
    const voter = this.provider.wallet.publicKey;

    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);
    const [voteEscrowPDA] = getVoteEscrowPDA(id, voter, this.programId);
    const [voteRecordPDA] = getVoteRecordPDA(id, voter, this.programId);

    return (this.program.methods as any)
      .retractVote()
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        voteEscrow: voteEscrowPDA,
        voteRecord: voteRecordPDA,
        voter,
      })
      .rpc();
  }

  /**
   * Withdraw escrowed vote tokens
   */
  async withdrawVoteTokens(
    proposalId: BN | number | bigint,
    voterTokenAccount: PublicKey
  ): Promise<TransactionSignature> {
    const id = this.toBN(proposalId);
    const config = await this.getDaoConfig();
    const voter = this.provider.wallet.publicKey;

    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);
    const [voteEscrowPDA] = getVoteEscrowPDA(id, voter, this.programId);

    return (this.program.methods as any)
      .withdrawVoteTokens()
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        voteEscrow: voteEscrowPDA,
        voteVault: config.voteVault,
        voterTokenAccount,
        voter,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  /**
   * Finalize a proposal after voting ends
   */
  async finalizeProposal(proposalId: BN | number | bigint): Promise<TransactionSignature> {
    const id = this.toBN(proposalId);
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);

    return (this.program.methods as any)
      .finalizeProposal()
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
      })
      .rpc();
  }

  /**
   * Execute a passed treasury withdrawal proposal
   */
  async executeProposal(
    proposalId: BN | number | bigint,
    recipientTokenAccount: PublicKey
  ): Promise<TransactionSignature> {
    const id = this.toBN(proposalId);
    const config = await this.getDaoConfig();
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);

    return (this.program.methods as any)
      .executeProposal()
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        treasury: config.treasury,
        recipientTokenAccount,
        executor: this.provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  /**
   * Cancel a proposal (proposer only)
   */
  async cancelProposal(
    proposalId: BN | number | bigint,
    proposerTokenAccount: PublicKey
  ): Promise<TransactionSignature> {
    const id = this.toBN(proposalId);
    const config = await this.getDaoConfig();
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);

    return (this.program.methods as any)
      .cancelProposal()
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        bondEscrow: config.bondEscrow,
        proposerTokenAccount,
        proposer: this.provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  /**
   * Deposit tokens to treasury
   */
  async depositToTreasury(params: DepositToTreasuryParams): Promise<TransactionSignature> {
    const config = await this.getDaoConfig();
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .depositToTreasury(this.toBN(params.amount))
      .accounts({
        daoConfig: daoConfigPDA,
        treasury: config.treasury,
        depositorTokenAccount: params.depositorTokenAccount,
        depositor: this.provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  /**
   * Return proposal bond (for passed proposals)
   */
  async returnProposalBond(
    proposalId: BN | number | bigint,
    proposerTokenAccount: PublicKey
  ): Promise<TransactionSignature> {
    const id = this.toBN(proposalId);
    const config = await this.getDaoConfig();
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);
    const [proposalPDA] = getProposalPDA(id, this.programId);

    return (this.program.methods as any)
      .returnProposalBond()
      .accounts({
        daoConfig: daoConfigPDA,
        proposal: proposalPDA,
        bondEscrow: config.bondEscrow,
        proposerTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  }

  // ============================================================================
  // ADMIN OPERATIONS
  // ============================================================================

  /**
   * Pause or unpause the DAO
   */
  async setDaoPaused(paused: boolean): Promise<TransactionSignature> {
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .setDaoPaused(paused)
      .accounts({
        daoConfig: daoConfigPDA,
        authority: this.provider.wallet.publicKey,
      })
      .rpc();
  }

  /**
   * Queue a config update (subject to 48h timelock)
   */
  async queueConfigUpdate(params: QueueConfigUpdateParams): Promise<TransactionSignature> {
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .queueConfigUpdate(
        params.newVotingPeriod ? new BN(params.newVotingPeriod) : null,
        params.newProposalBond ? this.toBN(params.newProposalBond) : null,
        params.newQuorumPercentage ?? null,
        params.newApprovalThreshold ?? null
      )
      .accounts({
        daoConfig: daoConfigPDA,
        authority: this.provider.wallet.publicKey,
      })
      .rpc();
  }

  /**
   * Execute a queued config update (after timelock)
   */
  async executeConfigUpdate(): Promise<TransactionSignature> {
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .executeConfigUpdate()
      .accounts({
        daoConfig: daoConfigPDA,
        authority: this.provider.wallet.publicKey,
      })
      .rpc();
  }

  /**
   * Cancel a queued config update
   */
  async cancelConfigUpdate(): Promise<TransactionSignature> {
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .cancelConfigUpdate()
      .accounts({
        daoConfig: daoConfigPDA,
        authority: this.provider.wallet.publicKey,
      })
      .rpc();
  }

  /**
   * Close DAO config (destructive - for migration only)
   */
  async closeDaoConfig(): Promise<TransactionSignature> {
    const [daoConfigPDA] = getDaoConfigPDA(this.programId);

    return (this.program.methods as any)
      .closeDaoConfig()
      .accounts({
        daoConfig: daoConfigPDA,
        authority: this.provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  }

  // ============================================================================
  // HELPERS
  // ============================================================================

  private get provider(): AnchorProvider {
    return this.program.provider as AnchorProvider;
  }

  private toBN(value: BN | number | bigint): BN {
    if (BN.isBN(value)) return value;
    if (typeof value === "bigint") return new BN(value.toString());
    return new BN(value);
  }

  private parseDaoConfig(account: any): DaoConfig {
    return {
      authority: account.authority,
      treasury: account.treasury,
      governanceTokenMint: account.governanceTokenMint,
      bondEscrow: account.bondEscrow,
      voteVault: account.voteVault,
      votingPeriod: account.votingPeriod,
      proposalBond: account.proposalBond,
      quorumPercentage: account.quorumPercentage,
      approvalThreshold: account.approvalThreshold,
      proposalCount: account.proposalCount,
      totalTreasuryDeposits: account.totalTreasuryDeposits,
      paused: account.paused,
      pendingConfigChange: account.pendingConfigChange,
      bump: account.bump,
    };
  }

  private parseProposal(account: any): Proposal {
    return {
      proposalId: account.proposalId,
      proposer: account.proposer,
      title: account.title,
      descriptionCid: account.descriptionCid,
      proposalType: parseProposalType(account.proposalType),
      executionData: account.executionData,
      status: parseProposalStatus(account.status),
      forVotes: account.forVotes,
      againstVotes: account.againstVotes,
      abstainVotes: account.abstainVotes,
      voteStart: account.voteStart,
      voteEnd: account.voteEnd,
      createdAt: account.createdAt,
      executedAt: account.executedAt,
      bondReturned: account.bondReturned,
      snapshotSupply: account.snapshotSupply,
      bump: account.bump,
    };
  }

  private parseVoteEscrow(account: any): VoteEscrow {
    return {
      proposalId: account.proposalId,
      voter: account.voter,
      depositedAmount: account.depositedAmount,
      depositedAt: account.depositedAt,
      hasVoted: account.hasVoted,
      voteChoice: account.voteChoice ? parseVoteChoice(account.voteChoice) : null,
      withdrawn: account.withdrawn,
      bump: account.bump,
    };
  }

  private parseVoteRecord(account: any): VoteRecord {
    return {
      proposalId: account.proposalId,
      voter: account.voter,
      voteChoice: parseVoteChoice(account.voteChoice),
      voteWeight: account.voteWeight,
      votedAt: account.votedAt,
      bump: account.bump,
    };
  }
}
