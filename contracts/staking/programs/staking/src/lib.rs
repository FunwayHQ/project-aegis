use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("Stake11111111111111111111111111111111111111");

const UNSTAKE_COOLDOWN_PERIOD: i64 = 7 * 24 * 60 * 60; // 7 days in seconds
const MIN_STAKE_AMOUNT: u64 = 100_000_000_000; // 100 AEGIS tokens

#[program]
pub mod staking {
    use super::*;

    /// Initialize a stake account for a node operator
    pub fn initialize_stake(ctx: Context<InitializeStake>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let clock = Clock::get()?;

        stake_account.operator = ctx.accounts.operator.key();
        stake_account.staked_amount = 0;
        stake_account.pending_unstake = 0;
        stake_account.unstake_request_time = 0;
        stake_account.total_staked_ever = 0;
        stake_account.total_unstaked_ever = 0;
        stake_account.created_at = clock.unix_timestamp;
        stake_account.updated_at = clock.unix_timestamp;
        stake_account.bump = ctx.bumps.stake_account;

        msg!("Stake account initialized for operator: {}", stake_account.operator);

        emit!(StakeAccountCreatedEvent {
            operator: stake_account.operator,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Stake AEGIS tokens
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        require!(amount >= MIN_STAKE_AMOUNT, StakingError::InsufficientStakeAmount);

        let stake_account = &mut ctx.accounts.stake_account;
        let clock = Clock::get()?;

        // Transfer tokens from operator to stake vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.operator_token_account.to_account_info(),
            to: ctx.accounts.stake_vault.to_account_info(),
            authority: ctx.accounts.operator.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update stake account
        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_add(amount)
            .ok_or(StakingError::Overflow)?;
        stake_account.total_staked_ever = stake_account
            .total_staked_ever
            .checked_add(amount)
            .ok_or(StakingError::Overflow)?;
        stake_account.updated_at = clock.unix_timestamp;

        msg!("Staked {} tokens for operator: {}", amount, stake_account.operator);

        emit!(StakedEvent {
            operator: stake_account.operator,
            amount,
            total_staked: stake_account.staked_amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Request unstaking (starts cooldown period)
    pub fn request_unstake(ctx: Context<RequestUnstake>, amount: u64) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let clock = Clock::get()?;

        require!(amount > 0, StakingError::InvalidAmount);
        require!(
            stake_account.staked_amount >= amount,
            StakingError::InsufficientStakedBalance
        );

        // Check if there's already a pending unstake request
        require!(
            stake_account.pending_unstake == 0,
            StakingError::PendingUnstakeExists
        );

        // Move staked amount to pending unstake
        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_sub(amount)
            .ok_or(StakingError::Underflow)?;
        stake_account.pending_unstake = amount;
        stake_account.unstake_request_time = clock.unix_timestamp;
        stake_account.updated_at = clock.unix_timestamp;

        msg!(
            "Unstake requested: {} tokens, cooldown ends at: {}",
            amount,
            clock.unix_timestamp + UNSTAKE_COOLDOWN_PERIOD
        );

        emit!(UnstakeRequestedEvent {
            operator: stake_account.operator,
            amount,
            cooldown_ends_at: clock.unix_timestamp + UNSTAKE_COOLDOWN_PERIOD,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Execute unstake after cooldown period
    pub fn execute_unstake(ctx: Context<ExecuteUnstake>) -> Result<()> {
        let clock = Clock::get()?;

        require!(
            ctx.accounts.stake_account.pending_unstake > 0,
            StakingError::NoPendingUnstake
        );

        let cooldown_end = ctx.accounts.stake_account.unstake_request_time + UNSTAKE_COOLDOWN_PERIOD;
        require!(
            clock.unix_timestamp >= cooldown_end,
            StakingError::CooldownNotComplete
        );

        let amount = ctx.accounts.stake_account.pending_unstake;
        let operator = ctx.accounts.stake_account.operator;
        let bump = ctx.accounts.stake_account.bump;

        // Transfer tokens from vault back to operator
        let seeds = &[
            b"stake",
            operator.as_ref(),
            &[bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.stake_vault.to_account_info(),
            to: ctx.accounts.operator_token_account.to_account_info(),
            authority: ctx.accounts.stake_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        // Update stake account
        let stake_account = &mut ctx.accounts.stake_account;
        stake_account.pending_unstake = 0;
        stake_account.unstake_request_time = 0;
        stake_account.total_unstaked_ever = stake_account
            .total_unstaked_ever
            .checked_add(amount)
            .ok_or(StakingError::Overflow)?;
        stake_account.updated_at = clock.unix_timestamp;

        msg!("Unstaked {} tokens for operator: {}", amount, operator);

        emit!(UnstakedEvent {
            operator,
            amount,
            remaining_staked: stake_account.staked_amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Cancel unstake request (before cooldown completes)
    pub fn cancel_unstake(ctx: Context<CancelUnstake>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let clock = Clock::get()?;

        require!(
            stake_account.pending_unstake > 0,
            StakingError::NoPendingUnstake
        );

        let amount = stake_account.pending_unstake;

        // Return pending unstake back to staked amount
        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_add(amount)
            .ok_or(StakingError::Overflow)?;
        stake_account.pending_unstake = 0;
        stake_account.unstake_request_time = 0;
        stake_account.updated_at = clock.unix_timestamp;

        msg!("Unstake request cancelled for operator: {}", stake_account.operator);

        emit!(UnstakeCancelledEvent {
            operator: stake_account.operator,
            amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Slash stake (callable by governance or slashing authority)
    pub fn slash_stake(
        ctx: Context<SlashStake>,
        amount: u64,
        reason: String,
    ) -> Result<()> {
        require!(amount > 0, StakingError::InvalidAmount);
        require!(reason.len() <= 128, StakingError::ReasonTooLong);

        let clock = Clock::get()?;

        require!(
            ctx.accounts.stake_account.staked_amount >= amount,
            StakingError::InsufficientStakedBalance
        );

        let operator = ctx.accounts.stake_account.operator;
        let bump = ctx.accounts.stake_account.bump;

        // Transfer slashed tokens to treasury
        let seeds = &[
            b"stake",
            operator.as_ref(),
            &[bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.stake_vault.to_account_info(),
            to: ctx.accounts.treasury.to_account_info(),
            authority: ctx.accounts.stake_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        // Update stake account
        let stake_account = &mut ctx.accounts.stake_account;
        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_sub(amount)
            .ok_or(StakingError::Underflow)?;
        stake_account.updated_at = clock.unix_timestamp;

        msg!("Slashed {} tokens from operator: {} - Reason: {}",
            amount, operator, reason);

        emit!(StakeSlashedEvent {
            operator,
            amount,
            reason,
            remaining_staked: stake_account.staked_amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }
}

/// Stake account - tracks operator's staked tokens
#[account]
pub struct StakeAccount {
    pub operator: Pubkey,           // Node operator (32 bytes)
    pub staked_amount: u64,         // Currently staked amount (8 bytes)
    pub pending_unstake: u64,       // Amount pending unstake (8 bytes)
    pub unstake_request_time: i64,  // When unstake was requested (8 bytes)
    pub total_staked_ever: u64,     // Lifetime staking total (8 bytes)
    pub total_unstaked_ever: u64,   // Lifetime unstaking total (8 bytes)
    pub created_at: i64,            // Account creation timestamp (8 bytes)
    pub updated_at: i64,            // Last update timestamp (8 bytes)
    pub bump: u8,                   // PDA bump seed (1 byte)
}

impl StakeAccount {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +                        // operator
        8 +                         // staked_amount
        8 +                         // pending_unstake
        8 +                         // unstake_request_time
        8 +                         // total_staked_ever
        8 +                         // total_unstaked_ever
        8 +                         // created_at
        8 +                         // updated_at
        1;                          // bump
}

/// Initialize stake account
#[derive(Accounts)]
pub struct InitializeStake<'info> {
    #[account(
        init,
        payer = operator,
        space = StakeAccount::MAX_SIZE,
        seeds = [b"stake", operator.key().as_ref()],
        bump
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(mut)]
    pub operator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Stake tokens
#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(
        mut,
        seeds = [b"stake", operator.key().as_ref()],
        bump = stake_account.bump,
        has_one = operator @ StakingError::UnauthorizedOperator
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(mut)]
    pub operator_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub stake_vault: Account<'info, TokenAccount>,

    pub operator: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

/// Request unstake
#[derive(Accounts)]
pub struct RequestUnstake<'info> {
    #[account(
        mut,
        seeds = [b"stake", operator.key().as_ref()],
        bump = stake_account.bump,
        has_one = operator @ StakingError::UnauthorizedOperator
    )]
    pub stake_account: Account<'info, StakeAccount>,

    pub operator: Signer<'info>,
}

/// Execute unstake
#[derive(Accounts)]
pub struct ExecuteUnstake<'info> {
    #[account(
        mut,
        seeds = [b"stake", operator.key().as_ref()],
        bump = stake_account.bump,
        has_one = operator @ StakingError::UnauthorizedOperator
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(mut)]
    pub stake_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub operator_token_account: Account<'info, TokenAccount>,

    pub operator: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

/// Cancel unstake
#[derive(Accounts)]
pub struct CancelUnstake<'info> {
    #[account(
        mut,
        seeds = [b"stake", operator.key().as_ref()],
        bump = stake_account.bump,
        has_one = operator @ StakingError::UnauthorizedOperator
    )]
    pub stake_account: Account<'info, StakeAccount>,

    pub operator: Signer<'info>,
}

/// Slash stake
#[derive(Accounts)]
pub struct SlashStake<'info> {
    #[account(
        mut,
        seeds = [b"stake", stake_account.operator.as_ref()],
        bump = stake_account.bump
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(mut)]
    pub stake_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub treasury: Account<'info, TokenAccount>,

    /// Slashing authority (DAO or governance)
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

/// Events
#[event]
pub struct StakeAccountCreatedEvent {
    pub operator: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct StakedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub total_staked: u64,
    pub timestamp: i64,
}

#[event]
pub struct UnstakeRequestedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub cooldown_ends_at: i64,
    pub timestamp: i64,
}

#[event]
pub struct UnstakedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub remaining_staked: u64,
    pub timestamp: i64,
}

#[event]
pub struct UnstakeCancelledEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct StakeSlashedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub reason: String,
    pub remaining_staked: u64,
    pub timestamp: i64,
}

/// Custom errors
#[error_code]
pub enum StakingError {
    #[msg("Stake amount is below minimum requirement")]
    InsufficientStakeAmount,

    #[msg("Invalid amount")]
    InvalidAmount,

    #[msg("Insufficient staked balance")]
    InsufficientStakedBalance,

    #[msg("Pending unstake request already exists")]
    PendingUnstakeExists,

    #[msg("No pending unstake request")]
    NoPendingUnstake,

    #[msg("Cooldown period not complete")]
    CooldownNotComplete,

    #[msg("Only the operator can perform this action")]
    UnauthorizedOperator,

    #[msg("Arithmetic overflow")]
    Overflow,

    #[msg("Arithmetic underflow")]
    Underflow,

    #[msg("Slash reason exceeds maximum length")]
    ReasonTooLong,
}
