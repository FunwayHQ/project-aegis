use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H");

// DEPRECATED: These constants are now stored in GlobalConfig for flexibility
// Kept for backward compatibility during migration
const UNSTAKE_COOLDOWN_PERIOD: i64 = 7 * 24 * 60 * 60; // 7 days in seconds
const MIN_STAKE_AMOUNT: u64 = 100_000_000_000; // 100 AEGIS tokens

#[program]
pub mod staking {
    use super::*;

    /// SECURITY FIX: Initialize global configuration (one-time, by deployer)
    /// This stores the admin authority who can slash stakes and update parameters
    pub fn initialize_global_config(
        ctx: Context<InitializeGlobalConfig>,
        admin_authority: Pubkey,
        min_stake_amount: u64,
        unstake_cooldown_period: i64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.global_config;

        config.admin_authority = admin_authority;
        config.min_stake_amount = min_stake_amount;
        config.unstake_cooldown_period = unstake_cooldown_period;
        config.treasury = ctx.accounts.treasury.key();
        config.paused = false;
        config.bump = ctx.bumps.global_config;

        msg!(
            "Global config initialized: admin={}, min_stake={}, cooldown={}",
            admin_authority,
            min_stake_amount,
            unstake_cooldown_period
        );

        Ok(())
    }

    /// SECURITY FIX: Update global config (admin only)
    pub fn update_global_config(
        ctx: Context<UpdateGlobalConfig>,
        new_admin: Option<Pubkey>,
        new_min_stake: Option<u64>,
        new_cooldown: Option<i64>,
    ) -> Result<()> {
        let config = &mut ctx.accounts.global_config;

        // CRITICAL: Verify caller is current admin
        require!(
            ctx.accounts.admin.key() == config.admin_authority,
            StakingError::UnauthorizedAdmin
        );

        // Update fields if provided
        if let Some(admin) = new_admin {
            config.admin_authority = admin;
            msg!("Admin authority updated to: {}", admin);
        }

        if let Some(min_stake) = new_min_stake {
            config.min_stake_amount = min_stake;
            msg!("Min stake amount updated to: {}", min_stake);
        }

        if let Some(cooldown) = new_cooldown {
            config.unstake_cooldown_period = cooldown;
            msg!("Unstake cooldown updated to: {}s", cooldown);
        }

        Ok(())
    }

    /// SECURITY FIX: Emergency pause (admin only)
    pub fn set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
        let config = &mut ctx.accounts.global_config;

        // CRITICAL: Verify caller is admin
        require!(
            ctx.accounts.admin.key() == config.admin_authority,
            StakingError::UnauthorizedAdmin
        );

        config.paused = paused;

        msg!("Staking paused status set to: {}", paused);

        Ok(())
    }

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
        let config = &ctx.accounts.global_config;

        // SECURITY FIX: Check if staking is paused
        require!(!config.paused, StakingError::StakingPaused);

        // SECURITY FIX: Use config min_stake instead of hardcoded constant
        require!(
            amount >= config.min_stake_amount,
            StakingError::InsufficientStakeAmount
        );

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
        let config = &ctx.accounts.global_config;
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

        // SECURITY FIX: Use config cooldown period instead of hardcoded
        let cooldown_end = clock.unix_timestamp + config.unstake_cooldown_period;

        msg!(
            "Unstake requested: {} tokens, cooldown ends at: {}",
            amount,
            cooldown_end
        );

        emit!(UnstakeRequestedEvent {
            operator: stake_account.operator,
            amount,
            cooldown_ends_at: cooldown_end,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Execute unstake after cooldown period
    pub fn execute_unstake(ctx: Context<ExecuteUnstake>) -> Result<()> {
        let config = &ctx.accounts.global_config;
        let clock = Clock::get()?;

        require!(
            ctx.accounts.stake_account.pending_unstake > 0,
            StakingError::NoPendingUnstake
        );

        // SECURITY FIX: Use config cooldown period
        let cooldown_end = ctx.accounts.stake_account.unstake_request_time + config.unstake_cooldown_period;
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

    /// SECURITY FIX: Slash stake (NOW REQUIRES ADMIN AUTHORIZATION)
    /// Only the admin_authority from GlobalConfig can slash stakes
    pub fn slash_stake(
        ctx: Context<SlashStake>,
        amount: u64,
        reason: String,
    ) -> Result<()> {
        let config = &ctx.accounts.global_config;

        // 🔒 CRITICAL SECURITY FIX: Verify authority is the admin
        // This prevents ANY random user from slashing ANY node operator
        require!(
            ctx.accounts.authority.key() == config.admin_authority,
            StakingError::UnauthorizedSlashing
        );

        require!(amount > 0, StakingError::InvalidAmount);
        require!(reason.len() <= 128, StakingError::ReasonTooLong);

        let clock = Clock::get()?;

        require!(
            ctx.accounts.stake_account.staked_amount >= amount,
            StakingError::InsufficientStakedBalance
        );

        let operator = ctx.accounts.stake_account.operator;

        // Use vault bump for signing the transfer
        let vault_seeds = &[
            b"stake_vault",
            &[ctx.bumps.stake_vault],
        ];
        let signer = &[&vault_seeds[..]];

        // Transfer slashed tokens to treasury from config
        let cpi_accounts = Transfer {
            from: ctx.accounts.stake_vault.to_account_info(),
            to: ctx.accounts.treasury.to_account_info(),
            authority: ctx.accounts.stake_vault.to_account_info(),
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

        msg!("Slashed {} tokens from operator: {} - Reason: {} - By: {}",
            amount, operator, reason, ctx.accounts.authority.key());

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

/// SECURITY FIX: Global configuration for staking program
/// Stores admin authority and configurable parameters
#[account]
pub struct GlobalConfig {
    pub admin_authority: Pubkey,        // Admin who can slash and update config (32 bytes)
    pub min_stake_amount: u64,          // Minimum stake required (8 bytes)
    pub unstake_cooldown_period: i64,   // Cooldown in seconds (8 bytes)
    pub treasury: Pubkey,               // Treasury for slashed tokens (32 bytes)
    pub paused: bool,                   // Emergency pause flag (1 byte)
    pub bump: u8,                       // PDA bump (1 byte)
}

impl GlobalConfig {
    pub const MAX_SIZE: usize = 8 +  // discriminator
        32 +                          // admin_authority
        8 +                           // min_stake_amount
        8 +                           // unstake_cooldown_period
        32 +                          // treasury
        1 +                           // paused
        1;                            // bump
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

/// SECURITY FIX: Initialize global config (one-time setup)
#[derive(Accounts)]
pub struct InitializeGlobalConfig<'info> {
    #[account(
        init,
        payer = deployer,
        space = GlobalConfig::MAX_SIZE,
        seeds = [b"global_config"],
        bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Treasury account for slashed tokens
    pub treasury: Account<'info, TokenAccount>,

    #[account(mut)]
    pub deployer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// SECURITY FIX: Update global config
#[derive(Accounts)]
pub struct UpdateGlobalConfig<'info> {
    #[account(
        mut,
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Must be current admin
    pub admin: Signer<'info>,
}

/// SECURITY FIX: Set paused status
#[derive(Accounts)]
pub struct SetPaused<'info> {
    #[account(
        mut,
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Must be admin
    pub admin: Signer<'info>,
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
    /// SECURITY FIX: Added global_config for min_stake and pause check
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

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
    /// SECURITY FIX: Added for cooldown period
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

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
    /// SECURITY FIX: Added for cooldown period
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

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

/// SECURITY FIX: Slash stake (now with admin verification)
#[derive(Accounts)]
pub struct SlashStake<'info> {
    /// CRITICAL: Global config stores authorized admin
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
        has_one = treasury @ StakingError::InvalidTreasury
    )]
    pub global_config: Account<'info, GlobalConfig>,

    #[account(
        mut,
        seeds = [b"stake", stake_account.operator.as_ref()],
        bump = stake_account.bump
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        mut,
        seeds = [b"stake_vault"],
        bump
    )]
    pub stake_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub treasury: Account<'info, TokenAccount>,

    /// CRITICAL: This signer must match global_config.admin_authority
    /// Verified in slash_stake instruction
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

    /// SECURITY FIX: New error codes for access control
    #[msg("Unauthorized: Only admin can perform this action")]
    UnauthorizedAdmin,

    #[msg("Unauthorized: Only admin can slash stakes")]
    UnauthorizedSlashing,

    #[msg("Invalid treasury account")]
    InvalidTreasury,

    #[msg("Staking is currently paused by admin")]
    StakingPaused,
}
