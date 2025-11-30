use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("8nr66XQcjr11HhMP9NU6d8j5iwX3yo59VDawQSmPWgnK");

const REWARD_POOL_SEED: &[u8] = b"reward_pool";
const OPERATOR_REWARDS_SEED: &[u8] = b"operator_rewards";

#[program]
pub mod rewards {
    use super::*;

    /// Initialize the global rewards pool
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        reward_rate_per_epoch: u64, // Rewards per epoch per staked token
    ) -> Result<()> {
        let pool = &mut ctx.accounts.reward_pool;
        pool.authority = ctx.accounts.authority.key();
        pool.reward_vault = ctx.accounts.reward_vault.key();
        pool.total_distributed = 0;
        pool.reward_rate_per_epoch = reward_rate_per_epoch;
        pool.current_epoch = 0;
        pool.bump = ctx.bumps.reward_pool;

        msg!("Rewards pool initialized with rate: {} per epoch", reward_rate_per_epoch);
        Ok(())
    }

    /// Initialize operator rewards account
    pub fn initialize_operator_rewards(
        ctx: Context<InitializeOperatorRewards>,
    ) -> Result<()> {
        let rewards = &mut ctx.accounts.operator_rewards;
        rewards.operator = ctx.accounts.operator.key();
        rewards.total_earned = 0;
        rewards.total_claimed = 0;
        rewards.unclaimed_rewards = 0;
        rewards.last_claim_time = 0;
        rewards.performance_score = 100; // Start at 100% performance
        rewards.uptime_percentage = 0;
        rewards.bump = ctx.bumps.operator_rewards;

        emit!(OperatorRewardsInitializedEvent {
            operator: rewards.operator,
        });

        Ok(())
    }

    /// Record operator performance metrics (called by oracles/validators)
    pub fn record_performance(
        ctx: Context<RecordPerformance>,
        uptime_percentage: u8, // 0-100
        performance_score: u8,  // 0-100
        epoch: u64,
    ) -> Result<()> {
        require!(
            uptime_percentage <= 100,
            RewardsError::InvalidPercentage
        );
        require!(
            performance_score <= 100,
            RewardsError::InvalidPercentage
        );

        let rewards = &mut ctx.accounts.operator_rewards;
        rewards.uptime_percentage = uptime_percentage;
        rewards.performance_score = performance_score;

        emit!(PerformanceRecordedEvent {
            operator: rewards.operator,
            uptime: uptime_percentage,
            performance: performance_score,
            epoch,
        });

        Ok(())
    }

    /// Calculate and allocate rewards for an operator
    pub fn calculate_rewards(
        ctx: Context<CalculateRewards>,
        staked_amount: u64,
        epochs_elapsed: u64,
    ) -> Result<()> {
        let pool = &ctx.accounts.reward_pool;
        let rewards = &mut ctx.accounts.operator_rewards;

        // Calculate rewards: (staked_amount * rate * epochs * uptime * performance) / (10^9 * 10000)
        // Multiply everything first, then divide once to avoid precision loss
        let total = (staked_amount as u128)
            .checked_mul(pool.reward_rate_per_epoch as u128)
            .ok_or(RewardsError::Overflow)?
            .checked_mul(epochs_elapsed as u128)
            .ok_or(RewardsError::Overflow)?
            .checked_mul(rewards.uptime_percentage as u128)
            .ok_or(RewardsError::Overflow)?
            .checked_mul(rewards.performance_score as u128)
            .ok_or(RewardsError::Overflow)?;

        // Divide by: 10^9 (decimals) * 100 (uptime %) * 100 (performance %) = 10^13
        let final_rewards = total
            .checked_div(10_000_000_000_000) // 10^13
            .ok_or(RewardsError::Underflow)?;

        require!(
            final_rewards <= u64::MAX as u128,
            RewardsError::Overflow
        );

        let reward_amount = final_rewards as u64;

        rewards.unclaimed_rewards = rewards
            .unclaimed_rewards
            .checked_add(reward_amount)
            .ok_or(RewardsError::Overflow)?;

        rewards.total_earned = rewards
            .total_earned
            .checked_add(reward_amount)
            .ok_or(RewardsError::Overflow)?;

        emit!(RewardsCalculatedEvent {
            operator: rewards.operator,
            amount: reward_amount,
            epochs: epochs_elapsed,
        });

        Ok(())
    }

    /// Claim pending rewards
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let clock = Clock::get()?;
        let rewards = &mut ctx.accounts.operator_rewards;

        require!(
            rewards.unclaimed_rewards > 0,
            RewardsError::NoRewardsToClaim
        );

        let amount = rewards.unclaimed_rewards;

        // Transfer rewards from vault to operator
        let pool_seeds = &[
            REWARD_POOL_SEED,
            &[ctx.accounts.reward_pool.bump],
        ];
        let signer = &[&pool_seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.reward_vault.to_account_info(),
            to: ctx.accounts.operator_token_account.to_account_info(),
            authority: ctx.accounts.reward_pool.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        // Update state
        rewards.unclaimed_rewards = 0;
        rewards.total_claimed = rewards
            .total_claimed
            .checked_add(amount)
            .ok_or(RewardsError::Overflow)?;
        rewards.last_claim_time = clock.unix_timestamp;

        // Update pool stats
        let pool = &mut ctx.accounts.reward_pool;
        pool.total_distributed = pool
            .total_distributed
            .checked_add(amount)
            .ok_or(RewardsError::Overflow)?;

        emit!(RewardsClaimedEvent {
            operator: rewards.operator,
            amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Fund the rewards pool (admin only)
    pub fn fund_pool(ctx: Context<FundPool>, amount: u64) -> Result<()> {
        require!(amount > 0, RewardsError::InvalidAmount);

        let cpi_accounts = Transfer {
            from: ctx.accounts.funder_token_account.to_account_info(),
            to: ctx.accounts.reward_vault.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        emit!(PoolFundedEvent {
            amount,
            funder: ctx.accounts.authority.key(),
        });

        Ok(())
    }

    /// Update reward rate (admin only)
    pub fn update_reward_rate(
        ctx: Context<UpdateRewardRate>,
        new_rate: u64,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.reward_pool;
        let old_rate = pool.reward_rate_per_epoch;
        pool.reward_rate_per_epoch = new_rate;

        emit!(RewardRateUpdatedEvent {
            old_rate,
            new_rate,
        });

        Ok(())
    }
}

/// Reward Pool - Global state
#[account]
pub struct RewardPool {
    pub authority: Pubkey,           // Admin authority (32)
    pub reward_vault: Pubkey,         // Token account holding rewards (32)
    pub total_distributed: u64,       // Total rewards distributed (8)
    pub reward_rate_per_epoch: u64,   // Reward rate per staked token per epoch (8)
    pub current_epoch: u64,           // Current epoch number (8)
    pub bump: u8,                     // PDA bump (1)
}

impl RewardPool {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +  // authority
        32 +  // reward_vault
        8 +   // total_distributed
        8 +   // reward_rate_per_epoch
        8 +   // current_epoch
        1;    // bump
}

/// Operator Rewards - Per-operator state
#[account]
pub struct OperatorRewards {
    pub operator: Pubkey,             // Operator pubkey (32)
    pub total_earned: u64,            // Lifetime rewards earned (8)
    pub total_claimed: u64,           // Lifetime rewards claimed (8)
    pub unclaimed_rewards: u64,       // Pending rewards (8)
    pub last_claim_time: i64,         // Last claim timestamp (8)
    pub performance_score: u8,        // 0-100 performance score (1)
    pub uptime_percentage: u8,        // 0-100 uptime percentage (1)
    pub bump: u8,                     // PDA bump (1)
}

impl OperatorRewards {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +  // operator
        8 +   // total_earned
        8 +   // total_claimed
        8 +   // unclaimed_rewards
        8 +   // last_claim_time
        1 +   // performance_score
        1 +   // uptime_percentage
        1;    // bump
}

/// Initialize reward pool
#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        space = RewardPool::MAX_SIZE,
        seeds = [REWARD_POOL_SEED],
        bump
    )]
    pub reward_pool: Account<'info, RewardPool>,

    pub reward_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Initialize operator rewards
#[derive(Accounts)]
pub struct InitializeOperatorRewards<'info> {
    #[account(
        init,
        payer = operator,
        space = OperatorRewards::MAX_SIZE,
        seeds = [OPERATOR_REWARDS_SEED, operator.key().as_ref()],
        bump
    )]
    pub operator_rewards: Account<'info, OperatorRewards>,

    #[account(mut)]
    pub operator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Record performance
#[derive(Accounts)]
pub struct RecordPerformance<'info> {
    /// SECURITY FIX: Added has_one constraint to verify authority matches reward_pool.authority
    /// This prevents unauthorized users from reporting fake performance metrics
    #[account(
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        seeds = [OPERATOR_REWARDS_SEED, operator_rewards.operator.as_ref()],
        bump = operator_rewards.bump
    )]
    pub operator_rewards: Account<'info, OperatorRewards>,

    /// SECURITY FIX: Authority that can record performance (oracle/validator)
    /// Must match reward_pool.authority - enforced by has_one constraint above
    pub authority: Signer<'info>,
}

/// Calculate rewards
#[derive(Accounts)]
pub struct CalculateRewards<'info> {
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        seeds = [OPERATOR_REWARDS_SEED, operator_rewards.operator.as_ref()],
        bump = operator_rewards.bump
    )]
    pub operator_rewards: Account<'info, OperatorRewards>,

    pub authority: Signer<'info>,
}

/// Claim rewards
#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(
        mut,
        seeds = [REWARD_POOL_SEED],
        bump = reward_pool.bump,
        has_one = reward_vault @ RewardsError::InvalidRewardVault
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        seeds = [OPERATOR_REWARDS_SEED, operator.key().as_ref()],
        bump = operator_rewards.bump,
        has_one = operator @ RewardsError::UnauthorizedOperator
    )]
    pub operator_rewards: Account<'info, OperatorRewards>,

    #[account(mut)]
    pub reward_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub operator_token_account: Account<'info, TokenAccount>,

    pub operator: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

/// Fund pool
#[derive(Accounts)]
pub struct FundPool<'info> {
    #[account(
        seeds = [REWARD_POOL_SEED],
        bump = reward_pool.bump,
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(mut)]
    pub funder_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub reward_vault: Account<'info, TokenAccount>,

    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

/// Update reward rate
#[derive(Accounts)]
pub struct UpdateRewardRate<'info> {
    #[account(
        mut,
        seeds = [REWARD_POOL_SEED],
        bump = reward_pool.bump,
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    pub authority: Signer<'info>,
}

/// Events
#[event]
pub struct OperatorRewardsInitializedEvent {
    pub operator: Pubkey,
}

#[event]
pub struct PerformanceRecordedEvent {
    pub operator: Pubkey,
    pub uptime: u8,
    pub performance: u8,
    pub epoch: u64,
}

#[event]
pub struct RewardsCalculatedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub epochs: u64,
}

#[event]
pub struct RewardsClaimedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct PoolFundedEvent {
    pub amount: u64,
    pub funder: Pubkey,
}

#[event]
pub struct RewardRateUpdatedEvent {
    pub old_rate: u64,
    pub new_rate: u64,
}

/// Errors
#[error_code]
pub enum RewardsError {
    #[msg("Invalid percentage value (must be 0-100)")]
    InvalidPercentage,
    #[msg("No rewards available to claim")]
    NoRewardsToClaim,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Arithmetic underflow")]
    Underflow,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Unauthorized operator")]
    UnauthorizedOperator,
    #[msg("Unauthorized authority")]
    UnauthorizedAuthority,
    #[msg("Invalid reward vault")]
    InvalidRewardVault,
}
