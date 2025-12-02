use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("85Pd1GRJ1qA3kVTn3ERHsyuUpkr2bbb9L9opwS9UnHEQ");

// DEPRECATED: These constants are now stored in GlobalConfig for flexibility
// Kept for backward compatibility during migration
#[allow(dead_code)]
const _UNSTAKE_COOLDOWN_PERIOD: i64 = 7 * 24 * 60 * 60; // 7 days in seconds
#[allow(dead_code)]
const _MIN_STAKE_AMOUNT: u64 = 100_000_000_000; // 100 AEGIS tokens

/// SECURITY FIX: Timelock period for slash requests (24 hours in seconds)
/// This allows node operators to dispute false positives before execution
pub const SLASH_TIMELOCK_PERIOD: i64 = 24 * 60 * 60; // 24 hours

#[program]
pub mod staking {
    use super::*;

    /// SECURITY FIX: Initialize global configuration (one-time, by deployer)
    /// This stores the admin authority who can slash stakes and update parameters
    /// Now also stores registry_program_id for CPI integration
    pub fn initialize_global_config(
        ctx: Context<InitializeGlobalConfig>,
        admin_authority: Pubkey,
        min_stake_amount: u64,
        unstake_cooldown_period: i64,
        registry_program_id: Pubkey,
    ) -> Result<()> {
        let config = &mut ctx.accounts.global_config;

        config.admin_authority = admin_authority;
        config.min_stake_amount = min_stake_amount;
        config.unstake_cooldown_period = unstake_cooldown_period;
        config.treasury = ctx.accounts.treasury.key();
        config.registry_program_id = registry_program_id;
        config.paused = false;
        config.bump = ctx.bumps.global_config;

        msg!(
            "Global config initialized: admin={}, min_stake={}, cooldown={}, registry={}",
            admin_authority,
            min_stake_amount,
            unstake_cooldown_period,
            registry_program_id
        );

        Ok(())
    }

    /// SECURITY FIX: Update global config (admin only)
    pub fn update_global_config(
        ctx: Context<UpdateGlobalConfig>,
        new_admin: Option<Pubkey>,
        new_min_stake: Option<u64>,
        new_cooldown: Option<i64>,
        new_registry_program: Option<Pubkey>,
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

        if let Some(registry) = new_registry_program {
            config.registry_program_id = registry;
            msg!("Registry program updated to: {}", registry);
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
    /// SECURITY FIX: Now calls Registry via CPI to keep stake amounts synchronized
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

        // SECURITY FIX: Call Registry to update stake via CPI
        // This ensures the registry stays in sync with staking state
        let new_total_stake = stake_account.staked_amount;

        let cpi_program = ctx.accounts.registry_program.to_account_info();
        let cpi_accounts = registry::cpi::accounts::UpdateStake {
            registry_config: ctx.accounts.registry_config.to_account_info(),
            node_account: ctx.accounts.node_account.to_account_info(),
            authority: ctx.accounts.staking_authority.to_account_info(),
        };

        // Sign with staking program PDA
        let seeds = &[
            b"staking_authority".as_ref(),
            &[ctx.bumps.staking_authority],
        ];
        let signer = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        registry::cpi::update_stake(cpi_ctx, new_total_stake)?;

        msg!("Staked {} tokens for operator: {}", amount, stake_account.operator);
        msg!("Registry updated via CPI with new stake: {}", new_total_stake);

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
    /// SECURITY FIX: Now calls Registry via CPI to keep stake amounts synchronized
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

        // SECURITY FIX: Call Registry to update stake via CPI
        let new_total_stake = stake_account.staked_amount;

        let registry_cpi_program = ctx.accounts.registry_program.to_account_info();
        let registry_cpi_accounts = registry::cpi::accounts::UpdateStake {
            registry_config: ctx.accounts.registry_config.to_account_info(),
            node_account: ctx.accounts.node_account.to_account_info(),
            authority: ctx.accounts.staking_authority.to_account_info(),
        };

        // Sign with staking program PDA
        let staking_seeds = &[
            b"staking_authority".as_ref(),
            &[ctx.bumps.staking_authority],
        ];
        let staking_signer = &[&staking_seeds[..]];

        let registry_cpi_ctx = CpiContext::new_with_signer(
            registry_cpi_program,
            registry_cpi_accounts,
            staking_signer
        );
        registry::cpi::update_stake(registry_cpi_ctx, new_total_stake)?;

        msg!("Unstaked {} tokens for operator: {}", amount, operator);
        msg!("Registry updated via CPI with new stake: {}", new_total_stake);

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

    /// SECURITY FIX: Request slash (timelock pattern - Phase 1)
    /// Creates a pending slash request that can be executed after 24 hours.
    /// This prevents instant 100% slashing and allows for dispute resolution.
    ///
    /// Slashing triggers per whitepaper:
    /// 1. Offline48Hours - Node offline for 48+ hours (10% slash)
    /// 2. LowUptime - Below 90% uptime (5% slash)
    /// 3. ChallengeFailed - Failed security challenge (15% slash)
    /// 4. DataIntegrityViolation - Corrupted/invalid data (25% slash)
    /// 5. MaliciousBehavior - Attack detection (100% slash)
    pub fn request_slash(
        ctx: Context<RequestSlash>,
        violation_type: SlashingViolation,
        evidence_cid: String, // IPFS CID of evidence
    ) -> Result<()> {
        let config = &ctx.accounts.global_config;
        let stake_account = &ctx.accounts.stake_account;
        let slash_request = &mut ctx.accounts.slash_request;
        let clock = Clock::get()?;

        // Verify oracle is authorized
        require!(
            ctx.accounts.oracle.key() == config.admin_authority,
            StakingError::UnauthorizedOracle
        );

        // Validate evidence CID length
        require!(evidence_cid.len() <= 128, StakingError::EvidenceCidTooLong);

        // Calculate slash amount based on violation type (per whitepaper)
        let slash_percentage = match violation_type {
            SlashingViolation::Offline48Hours => 10,         // 10% for extended offline
            SlashingViolation::LowUptime => 5,               // 5% for <90% uptime
            SlashingViolation::ChallengeFailed => 15,        // 15% for failed challenge
            SlashingViolation::DataIntegrityViolation => 25, // 25% for data corruption
            SlashingViolation::MaliciousBehavior => 100,     // 100% for attacks
        };

        let slash_amount = stake_account
            .staked_amount
            .checked_mul(slash_percentage)
            .ok_or(StakingError::Overflow)?
            .checked_div(100)
            .ok_or(StakingError::Underflow)?;

        require!(
            slash_amount > 0,
            StakingError::InvalidAmount
        );

        require!(
            stake_account.staked_amount >= slash_amount,
            StakingError::InsufficientStakedBalance
        );

        let operator = stake_account.operator;

        // Initialize the slash request PDA
        slash_request.operator = operator;
        slash_request.amount = slash_amount;
        slash_request.violation_type = violation_type;
        slash_request.evidence_cid = evidence_cid.clone();
        slash_request.request_time = clock.unix_timestamp;
        slash_request.authority = ctx.accounts.oracle.key();
        slash_request.executed = false;
        slash_request.cancelled = false;
        slash_request.bump = ctx.bumps.slash_request;

        let execute_after = clock.unix_timestamp + SLASH_TIMELOCK_PERIOD;

        msg!(
            "Slash requested: {} tokens ({}%) from {} for {:?} - Evidence: {} - Executable after: {}",
            slash_amount, slash_percentage, operator, violation_type, evidence_cid, execute_after
        );

        emit!(SlashRequestedEvent {
            operator,
            amount: slash_amount,
            violation_type,
            evidence_cid,
            request_time: clock.unix_timestamp,
            execute_after,
        });

        Ok(())
    }

    /// SECURITY FIX: Execute slash (timelock pattern - Phase 2)
    /// Can only be called 24 hours after request_slash was called.
    /// This gives node operators time to dispute false positives.
    pub fn execute_slash(ctx: Context<ExecuteSlash>) -> Result<()> {
        let config = &ctx.accounts.global_config;
        let stake_account = &mut ctx.accounts.stake_account;
        let slash_request = &mut ctx.accounts.slash_request;
        let clock = Clock::get()?;

        // Verify slash request is not already executed or cancelled
        require!(
            !slash_request.executed,
            StakingError::SlashAlreadyExecuted
        );
        require!(
            !slash_request.cancelled,
            StakingError::SlashCancelled
        );

        // CRITICAL: Enforce 24-hour timelock
        let execute_after = slash_request.request_time + SLASH_TIMELOCK_PERIOD;
        require!(
            clock.unix_timestamp >= execute_after,
            StakingError::SlashTimelockNotExpired
        );

        // Verify there's still enough stake to slash
        require!(
            stake_account.staked_amount >= slash_request.amount,
            StakingError::InsufficientStakedBalance
        );

        let operator = slash_request.operator;
        let slash_amount = slash_request.amount;

        // Transfer slashed tokens to treasury
        let vault_seeds: &[&[u8]] = &[
            b"stake_vault",
            &[ctx.bumps.stake_vault],
        ];
        let signer = &[vault_seeds];

        let cpi_accounts = Transfer {
            from: ctx.accounts.stake_vault.to_account_info(),
            to: ctx.accounts.treasury.to_account_info(),
            authority: ctx.accounts.stake_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, slash_amount)?;

        // Update stake account
        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_sub(slash_amount)
            .ok_or(StakingError::Underflow)?;
        stake_account.updated_at = clock.unix_timestamp;

        // Mark slash as executed
        slash_request.executed = true;

        // Call Registry to update stake via CPI
        let new_total_stake = stake_account.staked_amount;

        let registry_cpi_program = ctx.accounts.registry_program.to_account_info();
        let registry_cpi_accounts = registry::cpi::accounts::UpdateStake {
            registry_config: ctx.accounts.registry_config.to_account_info(),
            node_account: ctx.accounts.node_account.to_account_info(),
            authority: ctx.accounts.staking_authority.to_account_info(),
        };

        let staking_seeds = &[
            b"staking_authority".as_ref(),
            &[ctx.bumps.staking_authority],
        ];
        let staking_signer = &[&staking_seeds[..]];

        let registry_cpi_ctx = CpiContext::new_with_signer(
            registry_cpi_program,
            registry_cpi_accounts,
            staking_signer
        );
        registry::cpi::update_stake(registry_cpi_ctx, new_total_stake)?;

        msg!(
            "Slash executed: {} tokens from {} for {:?} - Evidence: {}",
            slash_amount, operator, slash_request.violation_type, slash_request.evidence_cid
        );

        emit!(SlashExecutedEvent {
            operator,
            amount: slash_amount,
            violation_type: slash_request.violation_type,
            evidence_cid: slash_request.evidence_cid.clone(),
            remaining_staked: stake_account.staked_amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX: Cancel slash request (admin only)
    /// Allows admin to cancel a pending slash in case of false positives
    /// or successful dispute resolution. Can only cancel before execution.
    pub fn cancel_slash(ctx: Context<CancelSlash>) -> Result<()> {
        let config = &ctx.accounts.global_config;
        let slash_request = &mut ctx.accounts.slash_request;
        let clock = Clock::get()?;

        // Verify caller is admin
        require!(
            ctx.accounts.admin.key() == config.admin_authority,
            StakingError::UnauthorizedAdmin
        );

        // Verify slash request is not already executed or cancelled
        require!(
            !slash_request.executed,
            StakingError::SlashAlreadyExecuted
        );
        require!(
            !slash_request.cancelled,
            StakingError::SlashAlreadyCancelled
        );

        // Mark as cancelled
        slash_request.cancelled = true;

        msg!(
            "Slash cancelled: {} tokens for operator {} - Violation: {:?}",
            slash_request.amount, slash_request.operator, slash_request.violation_type
        );

        emit!(SlashCancelledEvent {
            operator: slash_request.operator,
            amount: slash_request.amount,
            violation_type: slash_request.violation_type,
            cancelled_by: ctx.accounts.admin.key(),
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX: Slash stake (NOW REQUIRES ADMIN AUTHORIZATION)
    /// Only the admin_authority from GlobalConfig can slash stakes
    /// SECURITY FIX: Now calls Registry via CPI to keep stake amounts synchronized
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
        let vault_seeds: &[&[u8]] = &[
            b"stake_vault",
            &[ctx.bumps.stake_vault],
        ];
        let signer = &[vault_seeds];

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

        // SECURITY FIX: Call Registry to update stake via CPI
        let new_total_stake = stake_account.staked_amount;

        let registry_cpi_program = ctx.accounts.registry_program.to_account_info();
        let registry_cpi_accounts = registry::cpi::accounts::UpdateStake {
            registry_config: ctx.accounts.registry_config.to_account_info(),
            node_account: ctx.accounts.node_account.to_account_info(),
            authority: ctx.accounts.staking_authority.to_account_info(),
        };

        // Sign with staking program PDA
        let staking_seeds = &[
            b"staking_authority".as_ref(),
            &[ctx.bumps.staking_authority],
        ];
        let staking_signer = &[&staking_seeds[..]];

        let registry_cpi_ctx = CpiContext::new_with_signer(
            registry_cpi_program,
            registry_cpi_accounts,
            staking_signer
        );
        registry::cpi::update_stake(registry_cpi_ctx, new_total_stake)?;

        msg!("Slashed {} tokens from operator: {} - Reason: {} - By: {}",
            amount, operator, reason, ctx.accounts.authority.key());
        msg!("Registry updated via CPI with new stake: {}", new_total_stake);

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
/// Now includes registry_program_id for CPI integration
#[account]
pub struct GlobalConfig {
    pub admin_authority: Pubkey,        // Admin who can slash and update config (32 bytes)
    pub min_stake_amount: u64,          // Minimum stake required (8 bytes)
    pub unstake_cooldown_period: i64,   // Cooldown in seconds (8 bytes)
    pub treasury: Pubkey,               // Treasury for slashed tokens (32 bytes)
    pub registry_program_id: Pubkey,    // SECURITY FIX: Registry program for CPI (32 bytes)
    pub paused: bool,                   // Emergency pause flag (1 byte)
    pub bump: u8,                       // PDA bump (1 byte)
}

impl GlobalConfig {
    pub const MAX_SIZE: usize = 8 +  // discriminator
        32 +                          // admin_authority
        8 +                           // min_stake_amount
        8 +                           // unstake_cooldown_period
        32 +                          // treasury
        32 +                          // registry_program_id
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

/// SECURITY FIX: Slash request account - implements timelock pattern
/// Stores pending slash requests that can be executed after 24 hours
/// This prevents instant slashing and allows for dispute resolution
#[account]
pub struct SlashRequest {
    pub operator: Pubkey,             // Target operator to slash (32 bytes)
    pub amount: u64,                  // Amount to slash (8 bytes)
    pub violation_type: SlashingViolation, // Type of violation (1 byte)
    pub evidence_cid: String,         // IPFS CID of evidence (4 + 128 bytes max)
    pub request_time: i64,            // When request was created (8 bytes)
    pub authority: Pubkey,            // Oracle/admin who requested (32 bytes)
    pub executed: bool,               // Whether slash has been executed (1 byte)
    pub cancelled: bool,              // Whether slash was cancelled (1 byte)
    pub bump: u8,                     // PDA bump (1 byte)
}

impl SlashRequest {
    /// Maximum evidence CID length (IPFS CIDv1)
    pub const MAX_EVIDENCE_CID_LEN: usize = 128;

    pub const MAX_SIZE: usize = 8 +   // discriminator
        32 +                          // operator
        8 +                           // amount
        1 +                           // violation_type (enum)
        4 + Self::MAX_EVIDENCE_CID_LEN + // evidence_cid (string)
        8 +                           // request_time
        32 +                          // authority
        1 +                           // executed
        1 +                           // cancelled
        1;                            // bump
}

/// SECURITY FIX: Initialize global config (one-time setup)
/// Now includes registry_program_id parameter
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
/// SECURITY FIX: Now includes registry accounts for CPI integration
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

    // SECURITY FIX: Registry CPI accounts
    /// CHECK: Registry program validated against global_config.registry_program_id
    #[account(
        constraint = registry_program.key() == global_config.registry_program_id @ StakingError::InvalidRegistryProgram
    )]
    pub registry_program: AccountInfo<'info>,

    /// CHECK: Registry config PDA
    #[account(mut)]
    pub registry_config: AccountInfo<'info>,

    /// CHECK: Node account in registry
    #[account(mut)]
    pub node_account: AccountInfo<'info>,

    /// SECURITY FIX: Staking program PDA that acts as authority for registry CPI
    #[account(
        seeds = [b"staking_authority"],
        bump
    )]
    pub staking_authority: SystemAccount<'info>,
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
/// SECURITY FIX: Now includes registry accounts for CPI integration
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

    // SECURITY FIX: Registry CPI accounts
    /// CHECK: Registry program validated against global_config.registry_program_id
    #[account(
        constraint = registry_program.key() == global_config.registry_program_id @ StakingError::InvalidRegistryProgram
    )]
    pub registry_program: AccountInfo<'info>,

    /// CHECK: Registry config PDA
    #[account(mut)]
    pub registry_config: AccountInfo<'info>,

    /// CHECK: Node account in registry
    #[account(mut)]
    pub node_account: AccountInfo<'info>,

    /// SECURITY FIX: Staking program PDA that acts as authority for registry CPI
    #[account(
        seeds = [b"staking_authority"],
        bump
    )]
    pub staking_authority: SystemAccount<'info>,
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

/// SECURITY FIX: Slash stake (now with admin verification and registry CPI)
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

    // SECURITY FIX: Registry CPI accounts
    /// CHECK: Registry program validated against global_config.registry_program_id
    #[account(
        constraint = registry_program.key() == global_config.registry_program_id @ StakingError::InvalidRegistryProgram
    )]
    pub registry_program: AccountInfo<'info>,

    /// CHECK: Registry config PDA
    #[account(mut)]
    pub registry_config: AccountInfo<'info>,

    /// CHECK: Node account in registry
    #[account(mut)]
    pub node_account: AccountInfo<'info>,

    /// SECURITY FIX: Staking program PDA that acts as authority for registry CPI
    #[account(
        seeds = [b"staking_authority"],
        bump
    )]
    pub staking_authority: SystemAccount<'info>,
}

/// SECURITY FIX: Request slash context (Phase 1 of timelock pattern)
/// Creates a pending slash request PDA with 24-hour execution delay
#[derive(Accounts)]
#[instruction(violation_type: SlashingViolation, evidence_cid: String)]
pub struct RequestSlash<'info> {
    /// Global config stores authorized oracle (admin_authority)
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Stake account to slash (read-only to calculate slash amount)
    #[account(
        seeds = [b"stake", stake_account.operator.as_ref()],
        bump = stake_account.bump
    )]
    pub stake_account: Account<'info, StakeAccount>,

    /// SECURITY FIX: Slash request PDA (initialized here)
    /// Seeds include operator and timestamp to allow multiple requests
    #[account(
        init,
        payer = oracle,
        space = SlashRequest::MAX_SIZE,
        seeds = [b"slash_request", stake_account.operator.as_ref(), &Clock::get()?.unix_timestamp.to_le_bytes()],
        bump
    )]
    pub slash_request: Account<'info, SlashRequest>,

    /// Oracle authorized to request slashing
    /// Must match global_config.admin_authority
    #[account(mut)]
    pub oracle: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// SECURITY FIX: Execute slash context (Phase 2 of timelock pattern)
/// Can only be called 24 hours after request_slash
#[derive(Accounts)]
pub struct ExecuteSlash<'info> {
    /// Global config stores treasury for slashed tokens
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump,
        has_one = treasury @ StakingError::InvalidTreasury
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Stake account to slash
    #[account(
        mut,
        seeds = [b"stake", slash_request.operator.as_ref()],
        bump = stake_account.bump
    )]
    pub stake_account: Account<'info, StakeAccount>,

    /// Slash request PDA (verified for timelock)
    #[account(
        mut,
        seeds = [b"slash_request", slash_request.operator.as_ref(), &slash_request.request_time.to_le_bytes()],
        bump = slash_request.bump
    )]
    pub slash_request: Account<'info, SlashRequest>,

    #[account(
        mut,
        seeds = [b"stake_vault"],
        bump
    )]
    pub stake_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub treasury: Account<'info, TokenAccount>,

    /// Anyone can execute after timelock expires
    pub executor: Signer<'info>,

    pub token_program: Program<'info, Token>,

    // Registry CPI accounts for stake synchronization
    /// CHECK: Registry program validated against global_config.registry_program_id
    #[account(
        constraint = registry_program.key() == global_config.registry_program_id @ StakingError::InvalidRegistryProgram
    )]
    pub registry_program: AccountInfo<'info>,

    /// CHECK: Registry config PDA
    #[account(mut)]
    pub registry_config: AccountInfo<'info>,

    /// CHECK: Node account in registry
    #[account(mut)]
    pub node_account: AccountInfo<'info>,

    /// Staking program PDA for registry CPI signing
    #[account(
        seeds = [b"staking_authority"],
        bump
    )]
    pub staking_authority: SystemAccount<'info>,
}

/// SECURITY FIX: Cancel slash context (admin only)
/// Allows admin to cancel pending slash requests (false positive handling)
#[derive(Accounts)]
pub struct CancelSlash<'info> {
    /// Global config stores admin authority
    #[account(
        seeds = [b"global_config"],
        bump = global_config.bump
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Slash request to cancel
    #[account(
        mut,
        seeds = [b"slash_request", slash_request.operator.as_ref(), &slash_request.request_time.to_le_bytes()],
        bump = slash_request.bump
    )]
    pub slash_request: Account<'info, SlashRequest>,

    /// Admin authority (must match global_config.admin_authority)
    pub admin: Signer<'info>,
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

/// SECURITY FIX: Event emitted when slash is requested (Phase 1 of timelock)
#[event]
pub struct SlashRequestedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub violation_type: SlashingViolation,
    pub evidence_cid: String,
    pub request_time: i64,
    pub execute_after: i64,  // Timestamp when slash can be executed
}

/// SECURITY FIX: Event emitted when slash is executed (Phase 2 of timelock)
#[event]
pub struct SlashExecutedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub violation_type: SlashingViolation,
    pub evidence_cid: String,
    pub remaining_staked: u64,
    pub timestamp: i64,
}

/// SECURITY FIX: Event emitted when slash request is cancelled
#[event]
pub struct SlashCancelledEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub violation_type: SlashingViolation,
    pub cancelled_by: Pubkey,
    pub timestamp: i64,
}

/// Slashing violation types per whitepaper
/// Used by automated_slash instruction to determine penalty percentage
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashingViolation {
    /// Node offline for 48+ consecutive hours - 10% slash
    Offline48Hours,
    /// Uptime below 90% threshold - 5% slash
    LowUptime,
    /// Failed security challenge verification - 15% slash
    ChallengeFailed,
    /// Data integrity violation (corrupted/invalid data) - 25% slash
    DataIntegrityViolation,
    /// Malicious behavior detected (attacks, exploits) - 100% slash
    MaliciousBehavior,
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

    /// SECURITY FIX: New error for registry integration
    #[msg("Invalid registry program ID")]
    InvalidRegistryProgram,

    /// Automated slashing: Oracle not authorized
    #[msg("Unauthorized oracle for automated slashing")]
    UnauthorizedOracle,

    /// SECURITY FIX: New error codes for timelock pattern
    #[msg("Slash request already executed")]
    SlashAlreadyExecuted,

    #[msg("Slash request was cancelled")]
    SlashCancelled,

    #[msg("Slash request already cancelled")]
    SlashAlreadyCancelled,

    #[msg("Slash timelock period has not expired (24 hours required)")]
    SlashTimelockNotExpired,

    #[msg("Evidence CID exceeds maximum length (128 characters)")]
    EvidenceCidTooLong,
}
