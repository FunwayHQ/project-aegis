use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("9zQDZPNyDqVxevUAwaWTGGvCGwLSpfvkMn6aDKx7x6hz");

/// Voting period duration (3 days in seconds)
const DEFAULT_VOTING_PERIOD: i64 = 3 * 24 * 60 * 60;

/// Minimum voting period (1 day)
const MIN_VOTING_PERIOD: i64 = 24 * 60 * 60;

/// Maximum voting period (14 days)
const MAX_VOTING_PERIOD: i64 = 14 * 24 * 60 * 60;

/// Proposal bond amount (100 AEGIS tokens - prevents spam)
const DEFAULT_PROPOSAL_BOND: u64 = 100_000_000_000;

/// Minimum proposal bond (1 AEGIS token)
const MIN_PROPOSAL_BOND: u64 = 1_000_000_000;

/// Quorum percentage (10% of total supply must vote)
const DEFAULT_QUORUM_PERCENTAGE: u8 = 10;

/// Approval threshold percentage (51% of votes must be FOR)
const DEFAULT_APPROVAL_THRESHOLD: u8 = 51;

/// Maximum title length
const MAX_TITLE_LENGTH: usize = 128;

/// Maximum description CID length (IPFS CID)
const MAX_DESCRIPTION_CID_LENGTH: usize = 64;

/// Timelock delay for config changes (48 hours)
const CONFIG_TIMELOCK_DELAY: i64 = 48 * 60 * 60;

/// PDA seeds for vote vault
const VOTE_VAULT_SEED: &[u8] = b"vote_vault";

#[program]
pub mod dao {
    use super::*;

    /// Initialize the DAO configuration (one-time setup by deployer)
    pub fn initialize_dao(
        ctx: Context<InitializeDao>,
        voting_period: i64,
        proposal_bond: u64,
        quorum_percentage: u8,
        approval_threshold: u8,
    ) -> Result<()> {
        // Validate parameters
        require!(
            voting_period >= MIN_VOTING_PERIOD && voting_period <= MAX_VOTING_PERIOD,
            DaoError::InvalidVotingPeriod
        );
        require!(
            proposal_bond >= MIN_PROPOSAL_BOND,
            DaoError::InvalidProposalBond
        );
        require!(
            quorum_percentage > 0 && quorum_percentage <= 100,
            DaoError::InvalidQuorumPercentage
        );
        require!(
            approval_threshold > 0 && approval_threshold <= 100,
            DaoError::InvalidApprovalThreshold
        );

        let dao_config = &mut ctx.accounts.dao_config;

        dao_config.authority = ctx.accounts.authority.key();
        dao_config.treasury = ctx.accounts.treasury.key();
        dao_config.governance_token_mint = ctx.accounts.governance_token_mint.key();
        dao_config.bond_escrow = ctx.accounts.bond_escrow.key();
        dao_config.vote_vault = ctx.accounts.vote_vault.key();
        dao_config.voting_period = voting_period;
        dao_config.proposal_bond = proposal_bond;
        dao_config.quorum_percentage = quorum_percentage;
        dao_config.approval_threshold = approval_threshold;
        dao_config.proposal_count = 0;
        dao_config.total_treasury_deposits = 0;
        dao_config.paused = false;
        dao_config.pending_config_change = None;
        dao_config.bump = ctx.bumps.dao_config;

        msg!(
            "DAO initialized: voting_period={}s, bond={}, quorum={}%, threshold={}%",
            voting_period,
            proposal_bond,
            quorum_percentage,
            approval_threshold
        );

        emit!(DaoInitializedEvent {
            authority: dao_config.authority,
            treasury: dao_config.treasury,
            voting_period,
            proposal_bond,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Close the DAO config account (returns rent to authority)
    /// WARNING: This is destructive and should only be used for migration/cleanup
    pub fn close_dao_config(ctx: Context<CloseDaoConfig>) -> Result<()> {
        let dao_config = &ctx.accounts.dao_config;
        let authority = &ctx.accounts.authority;

        // Verify the account is owned by this program
        require!(
            dao_config.owner == &crate::ID,
            DaoError::UnauthorizedAuthority
        );

        // Transfer lamports from dao_config to authority
        let lamports = dao_config.lamports();
        **dao_config.try_borrow_mut_lamports()? = 0;
        **authority.try_borrow_mut_lamports()? = authority
            .lamports()
            .checked_add(lamports)
            .ok_or(DaoError::Overflow)?;

        // Zero out the account data to mark it as closed
        dao_config.try_borrow_mut_data()?.fill(0);

        // Assign to system program to fully close
        dao_config.assign(&anchor_lang::system_program::ID);

        msg!("DAO config closed, {} lamports returned to authority", lamports);

        emit!(DaoConfigClosedEvent {
            authority: ctx.accounts.authority.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Queue a DAO config update (subject to timelock)
    pub fn queue_config_update(
        ctx: Context<QueueConfigUpdate>,
        new_voting_period: Option<i64>,
        new_proposal_bond: Option<u64>,
        new_quorum_percentage: Option<u8>,
        new_approval_threshold: Option<u8>,
    ) -> Result<()> {
        let dao_config = &mut ctx.accounts.dao_config;
        let clock = Clock::get()?;

        // Validate new parameters if provided
        if let Some(period) = new_voting_period {
            require!(
                period >= MIN_VOTING_PERIOD && period <= MAX_VOTING_PERIOD,
                DaoError::InvalidVotingPeriod
            );
        }
        if let Some(bond) = new_proposal_bond {
            require!(bond >= MIN_PROPOSAL_BOND, DaoError::InvalidProposalBond);
        }
        if let Some(quorum) = new_quorum_percentage {
            require!(
                quorum > 0 && quorum <= 100,
                DaoError::InvalidQuorumPercentage
            );
        }
        if let Some(threshold) = new_approval_threshold {
            require!(
                threshold > 0 && threshold <= 100,
                DaoError::InvalidApprovalThreshold
            );
        }

        // Queue the config change with timelock
        let execute_after = clock.unix_timestamp + CONFIG_TIMELOCK_DELAY;
        dao_config.pending_config_change = Some(PendingConfigChange {
            new_voting_period,
            new_proposal_bond,
            new_quorum_percentage,
            new_approval_threshold,
            queued_at: clock.unix_timestamp,
            execute_after,
        });

        msg!(
            "Config update queued, executable after: {}",
            execute_after
        );

        emit!(ConfigUpdateQueuedEvent {
            new_voting_period,
            new_proposal_bond,
            new_quorum_percentage,
            new_approval_threshold,
            execute_after,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Execute a queued config update (after timelock expires)
    pub fn execute_config_update(ctx: Context<ExecuteConfigUpdate>) -> Result<()> {
        let dao_config = &mut ctx.accounts.dao_config;
        let clock = Clock::get()?;

        // Check there's a pending change
        let pending = dao_config
            .pending_config_change
            .clone()
            .ok_or(DaoError::NoPendingConfigChange)?;

        // Check timelock has expired
        require!(
            clock.unix_timestamp >= pending.execute_after,
            DaoError::TimelockNotExpired
        );

        // Apply changes
        if let Some(period) = pending.new_voting_period {
            dao_config.voting_period = period;
            msg!("Voting period updated to: {}s", period);
        }
        if let Some(bond) = pending.new_proposal_bond {
            dao_config.proposal_bond = bond;
            msg!("Proposal bond updated to: {}", bond);
        }
        if let Some(quorum) = pending.new_quorum_percentage {
            dao_config.quorum_percentage = quorum;
            msg!("Quorum percentage updated to: {}%", quorum);
        }
        if let Some(threshold) = pending.new_approval_threshold {
            dao_config.approval_threshold = threshold;
            msg!("Approval threshold updated to: {}%", threshold);
        }

        // Clear pending change
        dao_config.pending_config_change = None;

        emit!(ConfigUpdateExecutedEvent {
            voting_period: dao_config.voting_period,
            proposal_bond: dao_config.proposal_bond,
            quorum_percentage: dao_config.quorum_percentage,
            approval_threshold: dao_config.approval_threshold,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Cancel a queued config update
    pub fn cancel_config_update(ctx: Context<CancelConfigUpdate>) -> Result<()> {
        let dao_config = &mut ctx.accounts.dao_config;

        require!(
            dao_config.pending_config_change.is_some(),
            DaoError::NoPendingConfigChange
        );

        dao_config.pending_config_change = None;

        msg!("Pending config update cancelled");

        emit!(ConfigUpdateCancelledEvent {
            authority: ctx.accounts.authority.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Pause/unpause the DAO (authority only - emergency use)
    pub fn set_dao_paused(ctx: Context<SetDaoPaused>, paused: bool) -> Result<()> {
        let dao_config = &mut ctx.accounts.dao_config;
        dao_config.paused = paused;

        msg!("DAO paused status set to: {}", paused);

        emit!(DaoPausedEvent {
            paused,
            authority: ctx.accounts.authority.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Create a new proposal
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        title: String,
        description_cid: String,
        proposal_type: ProposalType,
        execution_data: Option<ExecutionData>,
    ) -> Result<()> {
        let dao_config = &mut ctx.accounts.dao_config;
        let clock = Clock::get()?;

        // Check if DAO is paused
        require!(!dao_config.paused, DaoError::DaoPaused);

        // Validate inputs
        require!(
            !title.is_empty() && title.len() <= MAX_TITLE_LENGTH,
            DaoError::InvalidTitleLength
        );
        require!(
            !description_cid.is_empty() && description_cid.len() <= MAX_DESCRIPTION_CID_LENGTH,
            DaoError::InvalidDescriptionCidLength
        );

        // Transfer proposal bond from proposer to bond escrow (PDA)
        let cpi_accounts = Transfer {
            from: ctx.accounts.proposer_token_account.to_account_info(),
            to: ctx.accounts.bond_escrow.to_account_info(),
            authority: ctx.accounts.proposer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, dao_config.proposal_bond)?;

        // Increment proposal count
        dao_config.proposal_count = dao_config
            .proposal_count
            .checked_add(1)
            .ok_or(DaoError::Overflow)?;

        // Get current token supply for snapshot
        let snapshot_supply = ctx.accounts.governance_token_mint.supply;

        // Initialize proposal
        let proposal = &mut ctx.accounts.proposal;
        proposal.proposal_id = dao_config.proposal_count;
        proposal.proposer = ctx.accounts.proposer.key();
        proposal.title = title.clone();
        proposal.description_cid = description_cid.clone();
        proposal.proposal_type = proposal_type;
        proposal.execution_data = execution_data;
        proposal.status = ProposalStatus::Active;
        proposal.for_votes = 0;
        proposal.against_votes = 0;
        proposal.abstain_votes = 0;
        proposal.vote_start = clock.unix_timestamp;
        proposal.vote_end = clock.unix_timestamp + dao_config.voting_period;
        proposal.created_at = clock.unix_timestamp;
        proposal.executed_at = None;
        proposal.bond_returned = false;
        proposal.snapshot_supply = snapshot_supply;
        proposal.bump = ctx.bumps.proposal;

        msg!(
            "Proposal {} created: '{}' by {}",
            proposal.proposal_id,
            title,
            proposal.proposer
        );

        emit!(ProposalCreatedEvent {
            proposal_id: proposal.proposal_id,
            proposer: proposal.proposer,
            title,
            description_cid,
            vote_end: proposal.vote_end,
            snapshot_supply,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Cancel a proposal (proposer only, before voting ends)
    pub fn cancel_proposal(ctx: Context<CancelProposal>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let dao_config = &ctx.accounts.dao_config;
        let clock = Clock::get()?;

        // Check proposal is still active
        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        // Check voting hasn't ended (allow cancellation during voting)
        require!(
            clock.unix_timestamp <= proposal.vote_end,
            DaoError::VotingEnded
        );

        // Return bond to proposer
        let dao_bump = dao_config.bump;
        let seeds = &[b"dao_config".as_ref(), &[dao_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.bond_escrow.to_account_info(),
            to: ctx.accounts.proposer_token_account.to_account_info(),
            authority: ctx.accounts.dao_config.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, dao_config.proposal_bond)?;

        // Mark as cancelled
        proposal.status = ProposalStatus::Cancelled;
        proposal.bond_returned = true;

        msg!("Proposal {} cancelled by proposer", proposal.proposal_id);

        emit!(ProposalCancelledEvent {
            proposal_id: proposal.proposal_id,
            proposer: proposal.proposer,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX: Deposit tokens to vote vault (Vote Escrow Pattern)
    ///
    /// This replaces the vulnerable `register_vote_snapshot` function.
    /// Instead of just recording a balance snapshot, tokens are TRANSFERRED
    /// to a PDA-owned vault, preventing:
    /// 1. Double voting (can't transfer tokens to another wallet and vote again)
    /// 2. Flash loan attacks (borrowed tokens are locked until proposal ends)
    ///
    /// Tokens are locked until EITHER:
    /// - The proposal's vote_end time has passed, OR
    /// - The voter retracts their vote (which removes their vote weight)
    pub fn deposit_vote_tokens(
        ctx: Context<DepositVoteTokens>,
        amount: u64,
    ) -> Result<()> {
        let proposal = &ctx.accounts.proposal;
        let clock = Clock::get()?;

        require!(amount > 0, DaoError::InvalidAmount);

        // Check proposal is active
        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        // Check within voting period
        require!(
            clock.unix_timestamp >= proposal.vote_start && clock.unix_timestamp <= proposal.vote_end,
            DaoError::VotingNotActive
        );

        // Transfer tokens from voter to vote vault (ESCROW)
        // This is the key security fix - tokens are now LOCKED, not just read
        let cpi_accounts = Transfer {
            from: ctx.accounts.voter_token_account.to_account_info(),
            to: ctx.accounts.vote_vault.to_account_info(),
            authority: ctx.accounts.voter.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Initialize vote escrow record
        let vote_escrow = &mut ctx.accounts.vote_escrow;
        vote_escrow.proposal_id = proposal.proposal_id;
        vote_escrow.voter = ctx.accounts.voter.key();
        vote_escrow.deposited_amount = amount;
        vote_escrow.deposited_at = clock.unix_timestamp;
        vote_escrow.has_voted = false;
        vote_escrow.vote_choice = None;
        vote_escrow.withdrawn = false;
        vote_escrow.bump = ctx.bumps.vote_escrow;

        msg!(
            "Vote tokens deposited for proposal {}: voter={}, amount={}",
            proposal.proposal_id,
            vote_escrow.voter,
            amount
        );

        emit!(VoteTokensDepositedEvent {
            proposal_id: proposal.proposal_id,
            voter: vote_escrow.voter,
            amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX: Cast a vote using escrowed tokens
    ///
    /// Vote weight is determined by the tokens locked in the vote escrow,
    /// not by current wallet balance. This prevents flash loan attacks.
    pub fn cast_vote(ctx: Context<CastVote>, vote_choice: VoteChoice) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let vote_escrow = &mut ctx.accounts.vote_escrow;
        let clock = Clock::get()?;

        // Check proposal is active
        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        // Check voting period
        require!(
            clock.unix_timestamp >= proposal.vote_start,
            DaoError::VotingNotStarted
        );
        require!(
            clock.unix_timestamp <= proposal.vote_end,
            DaoError::VotingEnded
        );

        // Check voter hasn't already voted (escrow-based double-vote prevention)
        require!(!vote_escrow.has_voted, DaoError::AlreadyVoted);

        // SECURITY FIX: Get vote weight from ESCROWED tokens (prevents flash loan attacks!)
        // The voter had to lock these tokens before voting, so they can't borrow and return
        let vote_weight = vote_escrow.deposited_amount;
        require!(vote_weight > 0, DaoError::NoVotingPower);

        // Initialize vote record
        let vote_record = &mut ctx.accounts.vote_record;
        vote_record.proposal_id = proposal.proposal_id;
        vote_record.voter = ctx.accounts.voter.key();
        vote_record.vote_choice = vote_choice;
        vote_record.vote_weight = vote_weight;
        vote_record.voted_at = clock.unix_timestamp;
        vote_record.bump = ctx.bumps.vote_record;

        // Mark escrow as used and record vote choice
        vote_escrow.has_voted = true;
        vote_escrow.vote_choice = Some(vote_choice);

        // Update proposal vote counts
        match vote_choice {
            VoteChoice::For => {
                proposal.for_votes = proposal
                    .for_votes
                    .checked_add(vote_weight)
                    .ok_or(DaoError::Overflow)?;
            }
            VoteChoice::Against => {
                proposal.against_votes = proposal
                    .against_votes
                    .checked_add(vote_weight)
                    .ok_or(DaoError::Overflow)?;
            }
            VoteChoice::Abstain => {
                proposal.abstain_votes = proposal
                    .abstain_votes
                    .checked_add(vote_weight)
                    .ok_or(DaoError::Overflow)?;
            }
        }

        msg!(
            "Vote cast on proposal {}: {:?} with weight {} by {}",
            proposal.proposal_id,
            vote_choice,
            vote_weight,
            vote_record.voter
        );

        emit!(VoteCastEvent {
            proposal_id: proposal.proposal_id,
            voter: vote_record.voter,
            vote_choice,
            vote_weight,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX: Retract a vote and allow token withdrawal
    ///
    /// This allows voters to change their mind before the voting period ends.
    /// When a vote is retracted:
    /// 1. The vote weight is removed from the proposal's vote counts
    /// 2. The vote_escrow is updated to allow withdrawal
    /// 3. The vote_record is closed (rent returned to voter)
    pub fn retract_vote(ctx: Context<RetractVote>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let vote_escrow = &mut ctx.accounts.vote_escrow;
        let vote_record = &ctx.accounts.vote_record;
        let clock = Clock::get()?;

        // Check proposal is still active
        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        // Check voting period hasn't ended
        require!(
            clock.unix_timestamp <= proposal.vote_end,
            DaoError::VotingEnded
        );

        // Check the voter has actually voted
        require!(vote_escrow.has_voted, DaoError::NotVoted);

        // Get the vote weight and choice from the escrow
        let vote_weight = vote_escrow.deposited_amount;
        let vote_choice = vote_escrow.vote_choice.ok_or(DaoError::NotVoted)?;

        // Decrement proposal vote counts
        match vote_choice {
            VoteChoice::For => {
                proposal.for_votes = proposal
                    .for_votes
                    .checked_sub(vote_weight)
                    .ok_or(DaoError::Underflow)?;
            }
            VoteChoice::Against => {
                proposal.against_votes = proposal
                    .against_votes
                    .checked_sub(vote_weight)
                    .ok_or(DaoError::Underflow)?;
            }
            VoteChoice::Abstain => {
                proposal.abstain_votes = proposal
                    .abstain_votes
                    .checked_sub(vote_weight)
                    .ok_or(DaoError::Underflow)?;
            }
        }

        // Mark escrow as not voted (allows re-voting or withdrawal)
        vote_escrow.has_voted = false;
        vote_escrow.vote_choice = None;

        msg!(
            "Vote retracted on proposal {}: weight {} by {}",
            proposal.proposal_id,
            vote_weight,
            vote_escrow.voter
        );

        emit!(VoteRetractedEvent {
            proposal_id: proposal.proposal_id,
            voter: vote_escrow.voter,
            vote_weight,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX: Withdraw escrowed vote tokens
    ///
    /// Tokens can only be withdrawn if:
    /// 1. The proposal's voting period has ended (vote_end has passed), OR
    /// 2. The voter has NOT voted (or has retracted their vote)
    ///
    /// This prevents voters from:
    /// - Voting with borrowed tokens and returning them before the vote counts
    /// - Double voting by transferring tokens between wallets
    pub fn withdraw_vote_tokens(ctx: Context<WithdrawVoteTokens>) -> Result<()> {
        let proposal = &ctx.accounts.proposal;
        let vote_escrow = &mut ctx.accounts.vote_escrow;
        let dao_config = &ctx.accounts.dao_config;
        let clock = Clock::get()?;

        // Check tokens haven't already been withdrawn
        require!(!vote_escrow.withdrawn, DaoError::AlreadyWithdrawn);

        // SECURITY CHECK: Tokens can only be withdrawn if:
        // 1. Voting period has ended, OR
        // 2. The voter hasn't voted (or retracted their vote)
        let voting_ended = clock.unix_timestamp > proposal.vote_end;
        let not_voted = !vote_escrow.has_voted;

        require!(
            voting_ended || not_voted,
            DaoError::TokensLockedDuringVoting
        );

        let amount = vote_escrow.deposited_amount;

        // Transfer tokens back to voter from vote vault
        let dao_bump = dao_config.bump;
        let seeds = &[b"dao_config".as_ref(), &[dao_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vote_vault.to_account_info(),
            to: ctx.accounts.voter_token_account.to_account_info(),
            authority: ctx.accounts.dao_config.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        // Mark as withdrawn
        vote_escrow.withdrawn = true;

        msg!(
            "Vote tokens withdrawn for proposal {}: voter={}, amount={}",
            proposal.proposal_id,
            vote_escrow.voter,
            amount
        );

        emit!(VoteTokensWithdrawnEvent {
            proposal_id: proposal.proposal_id,
            voter: vote_escrow.voter,
            amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Finalize a proposal after voting ends
    pub fn finalize_proposal(ctx: Context<FinalizeProposal>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let dao_config = &ctx.accounts.dao_config;
        let clock = Clock::get()?;

        // Check proposal is still active
        require!(
            proposal.status == ProposalStatus::Active,
            DaoError::ProposalNotActive
        );

        // Check voting period has ended
        require!(
            clock.unix_timestamp > proposal.vote_end,
            DaoError::VotingNotEnded
        );

        // Calculate total votes (for + against only for quorum, abstain excluded)
        let total_participation = proposal
            .for_votes
            .checked_add(proposal.against_votes)
            .ok_or(DaoError::Overflow)?
            .checked_add(proposal.abstain_votes)
            .ok_or(DaoError::Overflow)?;

        // Use snapshot supply for quorum calculation (prevents manipulation)
        let quorum_required = proposal
            .snapshot_supply
            .checked_mul(dao_config.quorum_percentage as u64)
            .ok_or(DaoError::Overflow)?
            .checked_div(100)
            .ok_or(DaoError::Overflow)?;

        // Check quorum (total participation must meet threshold)
        let quorum_met = total_participation >= quorum_required;

        // Calculate approval percentage (for votes / (for + against))
        // Abstain votes don't count towards approval calculation
        let votes_cast = proposal
            .for_votes
            .checked_add(proposal.against_votes)
            .ok_or(DaoError::Overflow)?;

        let approval_met = if votes_cast > 0 {
            let approval_percentage = proposal
                .for_votes
                .checked_mul(100)
                .ok_or(DaoError::Overflow)?
                .checked_div(votes_cast)
                .ok_or(DaoError::Overflow)?;

            approval_percentage >= dao_config.approval_threshold as u64
        } else {
            false
        };

        // Determine final status
        if quorum_met && approval_met {
            proposal.status = ProposalStatus::Passed;
            msg!("Proposal {} PASSED", proposal.proposal_id);
        } else if !quorum_met {
            proposal.status = ProposalStatus::Defeated;
            msg!(
                "Proposal {} DEFEATED (quorum not met: {} < {})",
                proposal.proposal_id,
                total_participation,
                quorum_required
            );
        } else {
            proposal.status = ProposalStatus::Defeated;
            msg!(
                "Proposal {} DEFEATED (insufficient approval)",
                proposal.proposal_id
            );
        }

        emit!(ProposalFinalizedEvent {
            proposal_id: proposal.proposal_id,
            status: proposal.status,
            for_votes: proposal.for_votes,
            against_votes: proposal.against_votes,
            abstain_votes: proposal.abstain_votes,
            quorum_met,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Execute a passed proposal (for treasury withdrawals)
    pub fn execute_proposal(ctx: Context<ExecuteProposal>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let clock = Clock::get()?;

        // Check proposal passed
        require!(
            proposal.status == ProposalStatus::Passed,
            DaoError::ProposalNotPassed
        );

        // Check not already executed
        require!(
            proposal.executed_at.is_none(),
            DaoError::ProposalAlreadyExecuted
        );

        // Check proposal type allows execution
        require!(
            proposal.proposal_type == ProposalType::TreasuryWithdrawal,
            DaoError::ProposalNotExecutable
        );

        // Get execution data
        let execution_data = proposal
            .execution_data
            .clone()
            .ok_or(DaoError::NoExecutionData)?;

        // CRITICAL FIX: Validate recipient matches proposal's intended recipient
        require!(
            ctx.accounts.recipient.key() == execution_data.recipient,
            DaoError::InvalidRecipient
        );

        // Check treasury has sufficient balance
        require!(
            ctx.accounts.treasury.amount >= execution_data.amount,
            DaoError::InsufficientTreasuryBalance
        );

        let proposal_id = proposal.proposal_id;

        // Execute treasury withdrawal
        let dao_bump = ctx.accounts.dao_config.bump;
        let seeds = &[b"dao_config".as_ref(), &[dao_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.treasury.to_account_info(),
            to: ctx.accounts.recipient.to_account_info(),
            authority: ctx.accounts.dao_config.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, execution_data.amount)?;

        // Mark as executed
        proposal.status = ProposalStatus::Executed;
        proposal.executed_at = Some(clock.unix_timestamp);

        msg!(
            "Proposal {} executed: {} tokens transferred to {}",
            proposal_id,
            execution_data.amount,
            execution_data.recipient
        );

        emit!(ProposalExecutedEvent {
            proposal_id,
            executor: ctx.accounts.executor.key(),
            amount: execution_data.amount,
            recipient: execution_data.recipient,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Return proposal bond to proposer (after finalization, if passed)
    pub fn return_proposal_bond(ctx: Context<ReturnProposalBond>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let dao_config = &ctx.accounts.dao_config;

        // Check proposal is finalized
        require!(
            proposal.status != ProposalStatus::Active,
            DaoError::ProposalStillActive
        );

        // Check bond not already returned
        require!(!proposal.bond_returned, DaoError::BondAlreadyReturned);

        // Only return bond for passed/executed proposals
        // Defeated proposals forfeit bond to treasury
        require!(
            proposal.status == ProposalStatus::Passed
                || proposal.status == ProposalStatus::Executed,
            DaoError::BondForfeited
        );

        // Transfer bond back to proposer
        let dao_bump = dao_config.bump;
        let seeds = &[b"dao_config".as_ref(), &[dao_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.bond_escrow.to_account_info(),
            to: ctx.accounts.proposer_token_account.to_account_info(),
            authority: ctx.accounts.dao_config.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, dao_config.proposal_bond)?;

        proposal.bond_returned = true;

        msg!(
            "Proposal {} bond returned to {}",
            proposal.proposal_id,
            proposal.proposer
        );

        emit!(BondReturnedEvent {
            proposal_id: proposal.proposal_id,
            proposer: proposal.proposer,
            amount: dao_config.proposal_bond,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Deposit tokens to DAO treasury
    pub fn deposit_to_treasury(ctx: Context<DepositToTreasury>, amount: u64) -> Result<()> {
        let dao_config = &mut ctx.accounts.dao_config;
        let clock = Clock::get()?;

        require!(amount > 0, DaoError::InvalidAmount);

        // Transfer tokens to treasury
        let cpi_accounts = Transfer {
            from: ctx.accounts.depositor_token_account.to_account_info(),
            to: ctx.accounts.treasury.to_account_info(),
            authority: ctx.accounts.depositor.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update treasury total
        dao_config.total_treasury_deposits = dao_config
            .total_treasury_deposits
            .checked_add(amount)
            .ok_or(DaoError::Overflow)?;

        msg!(
            "Deposited {} tokens to DAO treasury by {}",
            amount,
            ctx.accounts.depositor.key()
        );

        emit!(TreasuryDepositEvent {
            depositor: ctx.accounts.depositor.key(),
            amount,
            total_deposits: dao_config.total_treasury_deposits,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }
}

// ============================================================================
// ACCOUNT STRUCTURES
// ============================================================================

/// Pending configuration change (for timelock)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PendingConfigChange {
    pub new_voting_period: Option<i64>,
    pub new_proposal_bond: Option<u64>,
    pub new_quorum_percentage: Option<u8>,
    pub new_approval_threshold: Option<u8>,
    pub queued_at: i64,
    pub execute_after: i64,
}

impl PendingConfigChange {
    pub const MAX_SIZE: usize = 1 + 8 + // Option<i64>
        1 + 8 +  // Option<u64>
        1 + 1 +  // Option<u8>
        1 + 1 +  // Option<u8>
        8 +      // queued_at
        8; // execute_after
}

/// DAO configuration account
#[account]
pub struct DaoConfig {
    /// Authority who can update DAO config
    pub authority: Pubkey,
    /// Treasury token account
    pub treasury: Pubkey,
    /// Governance token mint
    pub governance_token_mint: Pubkey,
    /// Bond escrow token account (PDA-owned)
    pub bond_escrow: Pubkey,
    /// SECURITY FIX: Vote vault for escrowed voting tokens (PDA-owned)
    pub vote_vault: Pubkey,
    /// Voting period in seconds
    pub voting_period: i64,
    /// Bond required to create proposal (in tokens)
    pub proposal_bond: u64,
    /// Quorum percentage (0-100)
    pub quorum_percentage: u8,
    /// Approval threshold percentage (0-100)
    pub approval_threshold: u8,
    /// Total number of proposals created
    pub proposal_count: u64,
    /// Total tokens deposited to treasury
    pub total_treasury_deposits: u64,
    /// Emergency pause flag
    pub paused: bool,
    /// Pending configuration change (with timelock)
    pub pending_config_change: Option<PendingConfigChange>,
    /// PDA bump
    pub bump: u8,
}

impl DaoConfig {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +                         // authority
        32 +                         // treasury
        32 +                         // governance_token_mint
        32 +                         // bond_escrow
        32 +                         // vote_vault (SECURITY FIX)
        8 +                          // voting_period
        8 +                          // proposal_bond
        1 +                          // quorum_percentage
        1 +                          // approval_threshold
        8 +                          // proposal_count
        8 +                          // total_treasury_deposits
        1 +                          // paused
        1 + PendingConfigChange::MAX_SIZE + // pending_config_change (Option)
        1; // bump
}

/// Proposal account
#[account]
pub struct Proposal {
    /// Unique proposal ID
    pub proposal_id: u64,
    /// Proposer's public key
    pub proposer: Pubkey,
    /// Proposal title
    pub title: String,
    /// IPFS CID for detailed description
    pub description_cid: String,
    /// Type of proposal
    pub proposal_type: ProposalType,
    /// Execution data (for treasury withdrawals)
    pub execution_data: Option<ExecutionData>,
    /// Current status
    pub status: ProposalStatus,
    /// Total FOR votes (token-weighted)
    pub for_votes: u64,
    /// Total AGAINST votes (token-weighted)
    pub against_votes: u64,
    /// Total ABSTAIN votes (token-weighted)
    pub abstain_votes: u64,
    /// Voting start timestamp
    pub vote_start: i64,
    /// Voting end timestamp
    pub vote_end: i64,
    /// Creation timestamp
    pub created_at: i64,
    /// Execution timestamp (if executed)
    pub executed_at: Option<i64>,
    /// Whether bond has been returned
    pub bond_returned: bool,
    /// Token supply snapshot at proposal creation (for quorum calculation)
    pub snapshot_supply: u64,
    /// PDA bump
    pub bump: u8,
}

impl Proposal {
    pub const MAX_SIZE: usize = 8 + // discriminator
        8 +                          // proposal_id
        32 +                         // proposer
        4 + MAX_TITLE_LENGTH +       // title (string prefix + data)
        4 + MAX_DESCRIPTION_CID_LENGTH + // description_cid
        1 +                          // proposal_type
        1 + ExecutionData::MAX_SIZE + // execution_data (Option)
        1 +                          // status
        8 +                          // for_votes
        8 +                          // against_votes
        8 +                          // abstain_votes
        8 +                          // vote_start
        8 +                          // vote_end
        8 +                          // created_at
        1 + 8 +                      // executed_at (Option<i64>)
        1 +                          // bond_returned
        8 +                          // snapshot_supply
        1; // bump
}

/// SECURITY FIX: Vote escrow account - tracks deposited tokens for voting
/// Replaces the vulnerable VoteSnapshot that only recorded balance
#[account]
pub struct VoteEscrow {
    /// Proposal ID
    pub proposal_id: u64,
    /// Voter's public key
    pub voter: Pubkey,
    /// Amount of tokens deposited (escrowed)
    pub deposited_amount: u64,
    /// When tokens were deposited
    pub deposited_at: i64,
    /// Whether the voter has cast their vote
    pub has_voted: bool,
    /// Vote choice (if voted)
    pub vote_choice: Option<VoteChoice>,
    /// Whether tokens have been withdrawn
    pub withdrawn: bool,
    /// PDA bump
    pub bump: u8,
}

impl VoteEscrow {
    pub const MAX_SIZE: usize = 8 + // discriminator
        8 +                          // proposal_id
        32 +                         // voter
        8 +                          // deposited_amount
        8 +                          // deposited_at
        1 +                          // has_voted
        1 + 1 +                      // vote_choice (Option<enum>)
        1 +                          // withdrawn
        1; // bump
}

/// Vote record for a single voter on a proposal
#[account]
pub struct VoteRecord {
    /// Proposal ID
    pub proposal_id: u64,
    /// Voter's public key
    pub voter: Pubkey,
    /// Vote choice
    pub vote_choice: VoteChoice,
    /// Vote weight (from escrow)
    pub vote_weight: u64,
    /// When the vote was cast
    pub voted_at: i64,
    /// PDA bump
    pub bump: u8,
}

impl VoteRecord {
    pub const MAX_SIZE: usize = 8 + // discriminator
        8 +                          // proposal_id
        32 +                         // voter
        1 +                          // vote_choice
        8 +                          // vote_weight
        8 +                          // voted_at
        1; // bump
}

/// Execution data for treasury withdrawal proposals
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ExecutionData {
    /// Recipient of the withdrawal
    pub recipient: Pubkey,
    /// Amount to transfer
    pub amount: u64,
}

impl ExecutionData {
    pub const MAX_SIZE: usize = 32 + 8;
}

/// Proposal types
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalType {
    /// General governance proposal (no on-chain execution)
    General,
    /// Treasury withdrawal proposal
    TreasuryWithdrawal,
    /// Parameter change proposal (requires authority execution)
    ParameterChange,
}

/// Proposal status
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Voting is active
    Active,
    /// Proposal passed (quorum met + approval threshold met)
    Passed,
    /// Proposal defeated (quorum not met or approval threshold not met)
    Defeated,
    /// Proposal executed
    Executed,
    /// Proposal cancelled by proposer
    Cancelled,
}

/// Vote choices
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteChoice {
    For,
    Against,
    Abstain,
}

// ============================================================================
// ACCOUNT CONTEXTS
// ============================================================================

/// Initialize DAO configuration
#[derive(Accounts)]
pub struct InitializeDao<'info> {
    #[account(
        init,
        payer = authority,
        space = DaoConfig::MAX_SIZE,
        seeds = [b"dao_config"],
        bump
    )]
    pub dao_config: Account<'info, DaoConfig>,

    /// Treasury token account (must be owned by DAO PDA)
    #[account(
        mut,
        constraint = treasury.mint == governance_token_mint.key() @ DaoError::InvalidMint,
        constraint = treasury.owner == dao_config.key() @ DaoError::InvalidTreasuryOwner
    )]
    pub treasury: Account<'info, TokenAccount>,

    /// Bond escrow token account (must be owned by DAO PDA)
    #[account(
        mut,
        constraint = bond_escrow.mint == governance_token_mint.key() @ DaoError::InvalidMint,
        constraint = bond_escrow.owner == dao_config.key() @ DaoError::InvalidBondEscrowOwner
    )]
    pub bond_escrow: Account<'info, TokenAccount>,

    /// SECURITY FIX: Vote vault token account for escrowing vote tokens (must be owned by DAO PDA)
    #[account(
        mut,
        constraint = vote_vault.mint == governance_token_mint.key() @ DaoError::InvalidMint,
        constraint = vote_vault.owner == dao_config.key() @ DaoError::InvalidVoteVaultOwner
    )]
    pub vote_vault: Account<'info, TokenAccount>,

    /// Governance token mint ($AEGIS)
    pub governance_token_mint: Account<'info, Mint>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Close DAO configuration (for migration/cleanup)
#[derive(Accounts)]
pub struct CloseDaoConfig<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump
    )]
    /// CHECK: We intentionally don't deserialize the old account since it may have incompatible structure.
    /// This is safe because we only transfer lamports and zero the account data.
    pub dao_config: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Queue DAO configuration update (with timelock)
#[derive(Accounts)]
pub struct QueueConfigUpdate<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = authority @ DaoError::UnauthorizedAuthority
    )]
    pub dao_config: Account<'info, DaoConfig>,

    pub authority: Signer<'info>,
}

/// Execute queued configuration update
#[derive(Accounts)]
pub struct ExecuteConfigUpdate<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = authority @ DaoError::UnauthorizedAuthority
    )]
    pub dao_config: Account<'info, DaoConfig>,

    pub authority: Signer<'info>,
}

/// Cancel queued configuration update
#[derive(Accounts)]
pub struct CancelConfigUpdate<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = authority @ DaoError::UnauthorizedAuthority
    )]
    pub dao_config: Account<'info, DaoConfig>,

    pub authority: Signer<'info>,
}

/// Set DAO paused status
#[derive(Accounts)]
pub struct SetDaoPaused<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = authority @ DaoError::UnauthorizedAuthority
    )]
    pub dao_config: Account<'info, DaoConfig>,

    pub authority: Signer<'info>,
}

/// Create a new proposal
#[derive(Accounts)]
#[instruction(title: String, description_cid: String)]
pub struct CreateProposal<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = bond_escrow @ DaoError::InvalidBondEscrow,
        has_one = governance_token_mint @ DaoError::InvalidMint
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        init,
        payer = proposer,
        space = Proposal::MAX_SIZE,
        seeds = [b"proposal", (dao_config.proposal_count + 1).to_le_bytes().as_ref()],
        bump
    )]
    pub proposal: Account<'info, Proposal>,

    /// Bond escrow account (PDA-owned)
    #[account(
        mut,
        constraint = bond_escrow.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub bond_escrow: Account<'info, TokenAccount>,

    /// Governance token mint
    pub governance_token_mint: Account<'info, Mint>,

    /// Proposer's token account
    #[account(
        mut,
        constraint = proposer_token_account.owner == proposer.key() @ DaoError::InvalidTokenOwner,
        constraint = proposer_token_account.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub proposer_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub proposer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Cancel a proposal
#[derive(Accounts)]
pub struct CancelProposal<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = bond_escrow @ DaoError::InvalidBondEscrow
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump,
        has_one = proposer @ DaoError::NotProposer
    )]
    pub proposal: Account<'info, Proposal>,

    /// Bond escrow account
    #[account(mut)]
    pub bond_escrow: Account<'info, TokenAccount>,

    /// Proposer's token account
    #[account(
        mut,
        constraint = proposer_token_account.owner == proposer.key() @ DaoError::InvalidTokenOwner,
        constraint = proposer_token_account.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub proposer_token_account: Account<'info, TokenAccount>,

    pub proposer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

/// SECURITY FIX: Deposit vote tokens (Vote Escrow pattern)
#[derive(Accounts)]
pub struct DepositVoteTokens<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = vote_vault @ DaoError::InvalidVoteVault
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        init,
        payer = voter,
        space = VoteEscrow::MAX_SIZE,
        seeds = [b"vote_escrow", proposal.proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump
    )]
    pub vote_escrow: Account<'info, VoteEscrow>,

    /// Vote vault token account (PDA-owned)
    #[account(
        mut,
        constraint = vote_vault.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub vote_vault: Account<'info, TokenAccount>,

    /// Voter's token account
    #[account(
        mut,
        constraint = voter_token_account.owner == voter.key() @ DaoError::InvalidTokenOwner,
        constraint = voter_token_account.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub voter_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub voter: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// SECURITY FIX: Cast a vote (using escrowed tokens)
#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        mut,
        seeds = [b"vote_escrow", proposal.proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump = vote_escrow.bump,
        constraint = vote_escrow.voter == voter.key() @ DaoError::InvalidVoter,
        constraint = vote_escrow.proposal_id == proposal.proposal_id @ DaoError::InvalidProposal,
        constraint = !vote_escrow.withdrawn @ DaoError::AlreadyWithdrawn
    )]
    pub vote_escrow: Account<'info, VoteEscrow>,

    #[account(
        init,
        payer = voter,
        space = VoteRecord::MAX_SIZE,
        seeds = [b"vote", proposal.proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump
    )]
    pub vote_record: Account<'info, VoteRecord>,

    #[account(mut)]
    pub voter: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// SECURITY FIX: Retract a vote
#[derive(Accounts)]
pub struct RetractVote<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        mut,
        seeds = [b"vote_escrow", proposal.proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump = vote_escrow.bump,
        constraint = vote_escrow.voter == voter.key() @ DaoError::InvalidVoter,
        constraint = vote_escrow.proposal_id == proposal.proposal_id @ DaoError::InvalidProposal
    )]
    pub vote_escrow: Account<'info, VoteEscrow>,

    /// Vote record (will be closed, returning rent to voter)
    #[account(
        mut,
        seeds = [b"vote", proposal.proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump = vote_record.bump,
        constraint = vote_record.voter == voter.key() @ DaoError::InvalidVoter,
        close = voter
    )]
    pub vote_record: Account<'info, VoteRecord>,

    #[account(mut)]
    pub voter: Signer<'info>,
}

/// SECURITY FIX: Withdraw vote tokens
#[derive(Accounts)]
pub struct WithdrawVoteTokens<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = vote_vault @ DaoError::InvalidVoteVault
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        mut,
        seeds = [b"vote_escrow", proposal.proposal_id.to_le_bytes().as_ref(), voter.key().as_ref()],
        bump = vote_escrow.bump,
        constraint = vote_escrow.voter == voter.key() @ DaoError::InvalidVoter,
        constraint = vote_escrow.proposal_id == proposal.proposal_id @ DaoError::InvalidProposal
    )]
    pub vote_escrow: Account<'info, VoteEscrow>,

    /// Vote vault token account (PDA-owned)
    #[account(
        mut,
        constraint = vote_vault.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub vote_vault: Account<'info, TokenAccount>,

    /// Voter's token account
    #[account(
        mut,
        constraint = voter_token_account.owner == voter.key() @ DaoError::InvalidTokenOwner,
        constraint = voter_token_account.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub voter_token_account: Account<'info, TokenAccount>,

    pub voter: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

/// Finalize a proposal
#[derive(Accounts)]
pub struct FinalizeProposal<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,

    /// Anyone can finalize after voting ends
    pub finalizer: Signer<'info>,
}

/// Execute a passed proposal
#[derive(Accounts)]
pub struct ExecuteProposal<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = treasury @ DaoError::InvalidTreasury
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump
    )]
    pub proposal: Account<'info, Proposal>,

    /// DAO treasury
    #[account(
        mut,
        constraint = treasury.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub treasury: Account<'info, TokenAccount>,

    /// Recipient of treasury withdrawal (validated against proposal.execution_data.recipient)
    #[account(
        mut,
        constraint = recipient.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub recipient: Account<'info, TokenAccount>,

    /// Anyone can execute a passed proposal
    pub executor: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

/// Return proposal bond
#[derive(Accounts)]
pub struct ReturnProposalBond<'info> {
    #[account(
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = bond_escrow @ DaoError::InvalidBondEscrow
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        seeds = [b"proposal", proposal.proposal_id.to_le_bytes().as_ref()],
        bump = proposal.bump,
        has_one = proposer @ DaoError::NotProposer
    )]
    pub proposal: Account<'info, Proposal>,

    /// Bond escrow account
    #[account(mut)]
    pub bond_escrow: Account<'info, TokenAccount>,

    /// Proposer's token account
    #[account(
        mut,
        constraint = proposer_token_account.owner == proposer.key() @ DaoError::InvalidTokenOwner,
        constraint = proposer_token_account.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub proposer_token_account: Account<'info, TokenAccount>,

    /// CHECK: Verified via proposal.proposer
    pub proposer: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

/// Deposit to treasury
#[derive(Accounts)]
pub struct DepositToTreasury<'info> {
    #[account(
        mut,
        seeds = [b"dao_config"],
        bump = dao_config.bump,
        has_one = treasury @ DaoError::InvalidTreasury
    )]
    pub dao_config: Account<'info, DaoConfig>,

    #[account(
        mut,
        constraint = treasury.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub treasury: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = depositor_token_account.owner == depositor.key() @ DaoError::InvalidTokenOwner,
        constraint = depositor_token_account.mint == dao_config.governance_token_mint @ DaoError::InvalidMint
    )]
    pub depositor_token_account: Account<'info, TokenAccount>,

    pub depositor: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

// ============================================================================
// EVENTS
// ============================================================================

#[event]
pub struct DaoInitializedEvent {
    pub authority: Pubkey,
    pub treasury: Pubkey,
    pub voting_period: i64,
    pub proposal_bond: u64,
    pub timestamp: i64,
}

#[event]
pub struct DaoConfigClosedEvent {
    pub authority: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct ConfigUpdateQueuedEvent {
    pub new_voting_period: Option<i64>,
    pub new_proposal_bond: Option<u64>,
    pub new_quorum_percentage: Option<u8>,
    pub new_approval_threshold: Option<u8>,
    pub execute_after: i64,
    pub timestamp: i64,
}

#[event]
pub struct ConfigUpdateExecutedEvent {
    pub voting_period: i64,
    pub proposal_bond: u64,
    pub quorum_percentage: u8,
    pub approval_threshold: u8,
    pub timestamp: i64,
}

#[event]
pub struct ConfigUpdateCancelledEvent {
    pub authority: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct DaoPausedEvent {
    pub paused: bool,
    pub authority: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct ProposalCreatedEvent {
    pub proposal_id: u64,
    pub proposer: Pubkey,
    pub title: String,
    pub description_cid: String,
    pub vote_end: i64,
    pub snapshot_supply: u64,
    pub timestamp: i64,
}

#[event]
pub struct ProposalCancelledEvent {
    pub proposal_id: u64,
    pub proposer: Pubkey,
    pub timestamp: i64,
}

/// SECURITY FIX: New event for vote token deposits
#[event]
pub struct VoteTokensDepositedEvent {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct VoteCastEvent {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub vote_choice: VoteChoice,
    pub vote_weight: u64,
    pub timestamp: i64,
}

/// SECURITY FIX: New event for vote retractions
#[event]
pub struct VoteRetractedEvent {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub vote_weight: u64,
    pub timestamp: i64,
}

/// SECURITY FIX: New event for vote token withdrawals
#[event]
pub struct VoteTokensWithdrawnEvent {
    pub proposal_id: u64,
    pub voter: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct ProposalFinalizedEvent {
    pub proposal_id: u64,
    pub status: ProposalStatus,
    pub for_votes: u64,
    pub against_votes: u64,
    pub abstain_votes: u64,
    pub quorum_met: bool,
    pub timestamp: i64,
}

#[event]
pub struct ProposalExecutedEvent {
    pub proposal_id: u64,
    pub executor: Pubkey,
    pub amount: u64,
    pub recipient: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct BondReturnedEvent {
    pub proposal_id: u64,
    pub proposer: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct TreasuryDepositEvent {
    pub depositor: Pubkey,
    pub amount: u64,
    pub total_deposits: u64,
    pub timestamp: i64,
}

// ============================================================================
// ERRORS
// ============================================================================

#[error_code]
pub enum DaoError {
    #[msg("Voting period must be between 1 and 14 days")]
    InvalidVotingPeriod,

    #[msg("Proposal bond must be at least 1 token")]
    InvalidProposalBond,

    #[msg("Quorum percentage must be between 1 and 100")]
    InvalidQuorumPercentage,

    #[msg("Approval threshold must be between 1 and 100")]
    InvalidApprovalThreshold,

    #[msg("DAO is currently paused")]
    DaoPaused,

    #[msg("Title length must be between 1 and 128 characters")]
    InvalidTitleLength,

    #[msg("Description CID length must be between 1 and 64 characters")]
    InvalidDescriptionCidLength,

    #[msg("Proposal is not active")]
    ProposalNotActive,

    #[msg("Voting has not started yet")]
    VotingNotStarted,

    #[msg("Voting period has ended")]
    VotingEnded,

    #[msg("Voting is not currently active")]
    VotingNotActive,

    #[msg("Voting period has not ended yet")]
    VotingNotEnded,

    #[msg("No voting power (zero token balance at snapshot)")]
    NoVotingPower,

    #[msg("Already voted on this proposal")]
    AlreadyVoted,

    #[msg("Proposal has not passed")]
    ProposalNotPassed,

    #[msg("Proposal has already been executed")]
    ProposalAlreadyExecuted,

    #[msg("This proposal type cannot be executed")]
    ProposalNotExecutable,

    #[msg("No execution data provided")]
    NoExecutionData,

    #[msg("Proposal is still active")]
    ProposalStillActive,

    #[msg("Bond has already been returned")]
    BondAlreadyReturned,

    #[msg("Bond is forfeited for defeated proposals")]
    BondForfeited,

    #[msg("Invalid amount")]
    InvalidAmount,

    #[msg("Unauthorized: Only DAO authority can perform this action")]
    UnauthorizedAuthority,

    #[msg("Invalid treasury account")]
    InvalidTreasury,

    #[msg("Only the proposer can perform this action")]
    NotProposer,

    #[msg("Arithmetic overflow")]
    Overflow,

    #[msg("Arithmetic underflow")]
    Underflow,

    #[msg("Invalid token mint")]
    InvalidMint,

    #[msg("Invalid token account owner")]
    InvalidTokenOwner,

    #[msg("Invalid bond escrow account")]
    InvalidBondEscrow,

    #[msg("Treasury must be owned by DAO PDA")]
    InvalidTreasuryOwner,

    #[msg("Bond escrow must be owned by DAO PDA")]
    InvalidBondEscrowOwner,

    #[msg("Recipient does not match proposal execution data")]
    InvalidRecipient,

    #[msg("Insufficient treasury balance")]
    InsufficientTreasuryBalance,

    #[msg("No pending configuration change")]
    NoPendingConfigChange,

    #[msg("Timelock has not expired yet")]
    TimelockNotExpired,

    #[msg("Invalid voter")]
    InvalidVoter,

    #[msg("Invalid proposal")]
    InvalidProposal,

    // SECURITY FIX: New error codes for Vote Escrow pattern
    #[msg("Vote vault must be owned by DAO PDA")]
    InvalidVoteVaultOwner,

    #[msg("Invalid vote vault account")]
    InvalidVoteVault,

    #[msg("Tokens are locked during active voting - retract vote first or wait for vote_end")]
    TokensLockedDuringVoting,

    #[msg("Tokens have already been withdrawn")]
    AlreadyWithdrawn,

    #[msg("User has not voted on this proposal")]
    NotVoted,
}
