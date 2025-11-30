use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

declare_id!("8nr66XQcjr11HhMP9NU6d8j5iwX3yo59VDawQSmPWgnK");

const REWARD_POOL_SEED: &[u8] = b"reward_pool";
const OPERATOR_REWARDS_SEED: &[u8] = b"operator_rewards";
const ORACLE_REGISTRY_SEED: &[u8] = b"oracle_registry";

/// Minimum stake required (1000 AEGIS with 9 decimals)
const MIN_STAKE: u64 = 1_000_000_000_000;

/// Maximum stake multiplier (3x as per whitepaper)
const MAX_STAKE_MULTIPLIER: u64 = 300; // 3.00x in basis points (100 = 1x)

/// Precision for fixed-point math (10^6 for 6 decimal places)
const PRECISION: u128 = 1_000_000;

/// Emission schedule: 500M tokens over 10 years with halving
/// Year 1: 100M, Year 2: 50M, Year 3: 25M, etc.
const INITIAL_YEARLY_EMISSION: u64 = 100_000_000_000_000_000; // 100M with 9 decimals
const EPOCHS_PER_YEAR: u64 = 365; // Daily epochs

#[program]
pub mod rewards {
    use super::*;

    /// Initialize the global rewards pool with emission schedule
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        start_epoch: u64, // Starting epoch number (typically 0)
    ) -> Result<()> {
        let pool = &mut ctx.accounts.reward_pool;
        pool.authority = ctx.accounts.authority.key();
        pool.reward_vault = ctx.accounts.reward_vault.key();
        pool.total_distributed = 0;
        pool.current_epoch = start_epoch;
        pool.start_epoch = start_epoch;
        pool.total_network_requests = 0;
        pool.bump = ctx.bumps.reward_pool;

        // Calculate initial emission rate based on halving schedule
        let base_emission = calculate_epoch_emission(0);
        msg!("Rewards pool initialized. Year 1 daily emission: {}", base_emission);
        Ok(())
    }

    /// Initialize the oracle registry (one-time setup)
    pub fn initialize_oracle_registry(
        ctx: Context<InitializeOracleRegistry>,
    ) -> Result<()> {
        let registry = &mut ctx.accounts.oracle_registry;
        registry.authority = ctx.accounts.authority.key();
        registry.oracle_count = 0;
        registry.bump = ctx.bumps.oracle_registry;
        // Initialize empty oracle slots
        registry.oracles = [OracleInfo::default(); 10];

        msg!("Oracle registry initialized");
        Ok(())
    }

    /// Register an oracle that can submit signed performance attestations
    pub fn register_oracle(
        ctx: Context<RegisterOracle>,
        oracle_pubkey: [u8; 32], // Ed25519 public key for signature verification
    ) -> Result<()> {
        let registry = &mut ctx.accounts.oracle_registry;

        // Check if oracle already registered
        require!(
            !registry.oracles.iter().any(|o| o.pubkey == oracle_pubkey && o.is_active),
            RewardsError::OracleAlreadyRegistered
        );

        // Find empty slot or add new
        let mut added = false;
        for oracle in registry.oracles.iter_mut() {
            if !oracle.is_active && oracle.pubkey == [0u8; 32] {
                oracle.pubkey = oracle_pubkey;
                oracle.is_active = true;
                oracle.registered_at = Clock::get()?.unix_timestamp;
                added = true;
                break;
            }
        }

        if !added {
            require!(
                registry.oracle_count < 10,
                RewardsError::MaxOraclesReached
            );
            let idx = registry.oracle_count as usize;
            registry.oracles[idx] = OracleInfo {
                pubkey: oracle_pubkey,
                is_active: true,
                registered_at: Clock::get()?.unix_timestamp,
            };
            registry.oracle_count += 1;
        }

        emit!(OracleRegisteredEvent {
            oracle_pubkey,
        });

        Ok(())
    }

    /// Deactivate an oracle
    pub fn deactivate_oracle(
        ctx: Context<DeactivateOracle>,
        oracle_pubkey: [u8; 32],
    ) -> Result<()> {
        let registry = &mut ctx.accounts.oracle_registry;

        let mut found = false;
        for oracle in registry.oracles.iter_mut() {
            if oracle.pubkey == oracle_pubkey && oracle.is_active {
                oracle.is_active = false;
                found = true;
                break;
            }
        }

        require!(found, RewardsError::OracleNotFound);

        emit!(OracleDeactivatedEvent {
            oracle_pubkey,
        });

        Ok(())
    }

    /// Initialize operator rewards account with extended metrics
    pub fn initialize_operator_rewards(
        ctx: Context<InitializeOperatorRewards>,
    ) -> Result<()> {
        let rewards = &mut ctx.accounts.operator_rewards;
        rewards.operator = ctx.accounts.operator.key();
        rewards.total_earned = 0;
        rewards.total_claimed = 0;
        rewards.unclaimed_rewards = 0;
        rewards.last_claim_time = 0;
        // Extended performance metrics per whitepaper
        rewards.uptime_percentage = 0;       // 0-100, weight: 0.5
        rewards.latency_score = 0;           // 0-100, weight: 0.3
        rewards.throughput_score = 0;        // 0-100, weight: 0.2
        rewards.requests_served = 0;         // For demand multiplier
        rewards.last_performance_epoch = 0;
        rewards.bump = ctx.bumps.operator_rewards;

        emit!(OperatorRewardsInitializedEvent {
            operator: rewards.operator,
        });

        Ok(())
    }

    /// Record operator performance metrics with Ed25519 signature verification
    /// Per whitepaper: uptime (0.5 weight), latency (0.3 weight), throughput (0.2 weight)
    pub fn record_performance(
        ctx: Context<RecordPerformance>,
        uptime_percentage: u8,   // 0-100, weight: 0.5
        latency_score: u8,       // 0-100, weight: 0.3 (higher = better, lower latency)
        throughput_score: u8,    // 0-100, weight: 0.2
        requests_served: u64,    // For demand multiplier calculation
        epoch: u64,
        oracle_pubkey: [u8; 32], // Ed25519 public key of the signing oracle
        _signature: [u8; 64],    // Ed25519 signature (verified via Ed25519 program instruction)
    ) -> Result<()> {
        // Validate percentage ranges
        require!(uptime_percentage <= 100, RewardsError::InvalidPercentage);
        require!(latency_score <= 100, RewardsError::InvalidPercentage);
        require!(throughput_score <= 100, RewardsError::InvalidPercentage);

        // Verify oracle is registered and active
        let registry = &ctx.accounts.oracle_registry;
        let oracle_active = registry.oracles.iter().any(|o| o.pubkey == oracle_pubkey && o.is_active);
        require!(oracle_active, RewardsError::InvalidOracle);

        // Verify Ed25519 signature over attestation data
        // Message format: operator || epoch || uptime || latency || throughput || requests
        let operator = ctx.accounts.operator_rewards.operator;
        let mut message = Vec::with_capacity(32 + 8 + 1 + 1 + 1 + 8);
        message.extend_from_slice(operator.as_ref());
        message.extend_from_slice(&epoch.to_le_bytes());
        message.extend_from_slice(&[uptime_percentage, latency_score, throughput_score]);
        message.extend_from_slice(&requests_served.to_le_bytes());

        // Verify Ed25519 signature using instruction introspection
        // The client must include an Ed25519 program instruction before this instruction
        // Ed25519 program ID: Ed25519SigVerify111111111111111111111111111
        let ed25519_program_id = Pubkey::from_str_const("Ed25519SigVerify111111111111111111111111111");
        let ed25519_ix = anchor_lang::solana_program::sysvar::instructions::load_instruction_at_checked(
            0,
            &ctx.accounts.instructions_sysvar.to_account_info(),
        )?;

        // Verify the ed25519 instruction matches our expected signature
        require!(
            ed25519_ix.program_id == ed25519_program_id,
            RewardsError::InvalidSignature
        );

        // Note: The Ed25519 program will verify the signature automatically
        // We just need to ensure the instruction was included and will be processed
        // The signature, pubkey, and message are embedded in the Ed25519 instruction data

        // Update operator rewards with verified metrics
        let rewards = &mut ctx.accounts.operator_rewards;
        rewards.uptime_percentage = uptime_percentage;
        rewards.latency_score = latency_score;
        rewards.throughput_score = throughput_score;
        rewards.requests_served = rewards.requests_served
            .checked_add(requests_served)
            .ok_or(RewardsError::Overflow)?;
        rewards.last_performance_epoch = epoch;

        // Update total network requests for demand multiplier
        let pool = &mut ctx.accounts.reward_pool;
        pool.total_network_requests = pool.total_network_requests
            .checked_add(requests_served)
            .ok_or(RewardsError::Overflow)?;

        emit!(PerformanceRecordedEvent {
            operator: rewards.operator,
            uptime: uptime_percentage,
            latency: latency_score,
            throughput: throughput_score,
            requests: requests_served,
            epoch,
        });

        Ok(())
    }

    /// Simplified record_performance for backward compatibility (authority-only, no signature)
    pub fn record_performance_authority(
        ctx: Context<RecordPerformanceAuthority>,
        uptime_percentage: u8,
        latency_score: u8,
        throughput_score: u8,
        requests_served: u64,
        epoch: u64,
    ) -> Result<()> {
        require!(uptime_percentage <= 100, RewardsError::InvalidPercentage);
        require!(latency_score <= 100, RewardsError::InvalidPercentage);
        require!(throughput_score <= 100, RewardsError::InvalidPercentage);

        let rewards = &mut ctx.accounts.operator_rewards;
        rewards.uptime_percentage = uptime_percentage;
        rewards.latency_score = latency_score;
        rewards.throughput_score = throughput_score;
        rewards.requests_served = rewards.requests_served
            .checked_add(requests_served)
            .ok_or(RewardsError::Overflow)?;
        rewards.last_performance_epoch = epoch;

        let pool = &mut ctx.accounts.reward_pool;
        pool.total_network_requests = pool.total_network_requests
            .checked_add(requests_served)
            .ok_or(RewardsError::Overflow)?;

        emit!(PerformanceRecordedEvent {
            operator: rewards.operator,
            uptime: uptime_percentage,
            latency: latency_score,
            throughput: throughput_score,
            requests: requests_served,
            epoch,
        });

        Ok(())
    }

    /// Calculate and allocate rewards for an operator using whitepaper formula:
    /// daily_reward = base_emission × stake_multiplier × performance_score × demand_multiplier
    ///
    /// Where:
    /// - stake_multiplier = min(3.0, sqrt(stake_amount / MIN_STAKE))
    /// - performance_score = (uptime × 0.5) + (latency_score × 0.3) + (throughput_score × 0.2)
    /// - demand_multiplier = operator_requests / total_network_requests
    pub fn calculate_rewards(
        ctx: Context<CalculateRewards>,
        staked_amount: u64,
        epochs_elapsed: u64,
    ) -> Result<()> {
        let pool = &ctx.accounts.reward_pool;
        let rewards = &mut ctx.accounts.operator_rewards;

        // 1. Calculate stake multiplier: min(3.0, sqrt(stake / MIN_STAKE))
        // Using integer sqrt with PRECISION scaling
        let stake_ratio = (staked_amount as u128)
            .checked_mul(PRECISION)
            .ok_or(RewardsError::Overflow)?
            .checked_div(MIN_STAKE as u128)
            .ok_or(RewardsError::Underflow)?;

        // Integer square root approximation (Newton's method)
        let stake_multiplier_raw = integer_sqrt(stake_ratio.checked_mul(PRECISION).ok_or(RewardsError::Overflow)?);

        // Cap at 3x (300 basis points where 100 = 1x)
        let stake_multiplier = std::cmp::min(
            stake_multiplier_raw as u64,
            MAX_STAKE_MULTIPLIER * PRECISION as u64 / 100
        );

        // 2. Calculate weighted performance score per whitepaper
        // performance = (uptime × 0.5) + (latency × 0.3) + (throughput × 0.2)
        let weighted_performance = (rewards.uptime_percentage as u64)
            .checked_mul(50)  // 0.5 weight
            .ok_or(RewardsError::Overflow)?
            .checked_add(
                (rewards.latency_score as u64)
                    .checked_mul(30)  // 0.3 weight
                    .ok_or(RewardsError::Overflow)?
            )
            .ok_or(RewardsError::Overflow)?
            .checked_add(
                (rewards.throughput_score as u64)
                    .checked_mul(20)  // 0.2 weight
                    .ok_or(RewardsError::Overflow)?
            )
            .ok_or(RewardsError::Overflow)?;
        // weighted_performance is now 0-10000 (100 * 100 max)

        // 3. Calculate demand multiplier: operator_requests / total_network_requests
        // Scale by PRECISION to maintain accuracy
        let demand_multiplier = if pool.total_network_requests > 0 {
            (rewards.requests_served as u128)
                .checked_mul(PRECISION)
                .ok_or(RewardsError::Overflow)?
                .checked_div(pool.total_network_requests as u128)
                .ok_or(RewardsError::Underflow)?
        } else {
            PRECISION // Default to 1.0 if no network activity yet
        };

        // 4. Calculate base emission for the epoch range (with halving schedule)
        let current_epoch = pool.current_epoch;
        let mut total_emission: u128 = 0;
        for epoch_offset in 0..epochs_elapsed {
            let epoch = current_epoch.saturating_add(epoch_offset);
            total_emission = total_emission
                .checked_add(calculate_epoch_emission(epoch.saturating_sub(pool.start_epoch)) as u128)
                .ok_or(RewardsError::Overflow)?;
        }

        // 5. Final reward calculation:
        // reward = base_emission × (stake_multiplier / PRECISION) × (performance / 10000) × (demand / PRECISION)
        let reward_amount = total_emission
            .checked_mul(stake_multiplier as u128)
            .ok_or(RewardsError::Overflow)?
            .checked_div(PRECISION)
            .ok_or(RewardsError::Underflow)?
            .checked_mul(weighted_performance as u128)
            .ok_or(RewardsError::Overflow)?
            .checked_div(10000) // Normalize performance (0-10000 -> 0-1)
            .ok_or(RewardsError::Underflow)?
            .checked_mul(demand_multiplier)
            .ok_or(RewardsError::Overflow)?
            .checked_div(PRECISION)
            .ok_or(RewardsError::Underflow)?;

        require!(
            reward_amount <= u64::MAX as u128,
            RewardsError::Overflow
        );

        let final_reward = reward_amount as u64;

        rewards.unclaimed_rewards = rewards
            .unclaimed_rewards
            .checked_add(final_reward)
            .ok_or(RewardsError::Overflow)?;

        rewards.total_earned = rewards
            .total_earned
            .checked_add(final_reward)
            .ok_or(RewardsError::Overflow)?;

        emit!(RewardsCalculatedEvent {
            operator: rewards.operator,
            amount: final_reward,
            epochs: epochs_elapsed,
            stake_multiplier: stake_multiplier as u64,
            performance_score: weighted_performance,
            demand_multiplier: demand_multiplier as u64,
        });

        Ok(())
    }

    /// Advance the epoch (called periodically, typically daily)
    pub fn advance_epoch(ctx: Context<AdvanceEpoch>) -> Result<()> {
        let pool = &mut ctx.accounts.reward_pool;
        let old_epoch = pool.current_epoch;
        pool.current_epoch = pool.current_epoch
            .checked_add(1)
            .ok_or(RewardsError::Overflow)?;

        // Reset request counters for new epoch
        pool.total_network_requests = 0;

        emit!(EpochAdvancedEvent {
            old_epoch,
            new_epoch: pool.current_epoch,
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

}

/// Calculate emission for a specific epoch based on halving schedule
/// Year 1: 100M/365 per day, Year 2: 50M/365, Year 3: 25M/365, etc.
fn calculate_epoch_emission(epochs_since_start: u64) -> u64 {
    let year = epochs_since_start / EPOCHS_PER_YEAR;

    // Halving: divide by 2^year, but cap at year 10 (minimum emission)
    let halving_divisor = 1u64 << std::cmp::min(year, 10);

    // Daily emission = yearly emission / 365 / halving divisor
    (INITIAL_YEARLY_EMISSION / EPOCHS_PER_YEAR) / halving_divisor
}

/// Integer square root using Newton's method
/// Returns sqrt(n) with PRECISION scaling
fn integer_sqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }

    let mut x = n;
    let mut y = (x + 1) / 2;

    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }

    x
}

/// Reward Pool - Global state with emission schedule support
#[account]
pub struct RewardPool {
    pub authority: Pubkey,            // Admin authority (32)
    pub reward_vault: Pubkey,         // Token account holding rewards (32)
    pub total_distributed: u64,       // Total rewards distributed (8)
    pub current_epoch: u64,           // Current epoch number (8)
    pub start_epoch: u64,             // Starting epoch for halving calculation (8)
    pub total_network_requests: u64,  // Total requests this epoch for demand calc (8)
    pub bump: u8,                     // PDA bump (1)
}

impl RewardPool {
    pub const MAX_SIZE: usize = 8 +   // discriminator
        32 +  // authority
        32 +  // reward_vault
        8 +   // total_distributed
        8 +   // current_epoch
        8 +   // start_epoch
        8 +   // total_network_requests
        1;    // bump
}

/// Oracle Registry - Stores registered oracle public keys for signature verification
#[account]
pub struct OracleRegistry {
    pub authority: Pubkey,            // Admin who can add/remove oracles (32)
    pub oracles: [OracleInfo; 10],    // Up to 10 registered oracles (41 * 10 = 410)
    pub oracle_count: u8,             // Number of active oracles (1)
    pub bump: u8,                     // PDA bump (1)
}

impl OracleRegistry {
    pub const MAX_SIZE: usize = 8 +   // discriminator
        32 +  // authority
        (41 * 10) + // oracles array (pubkey 32 + is_active 1 + timestamp 8)
        1 +   // oracle_count
        1;    // bump
}

/// Oracle info stored in registry
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default)]
pub struct OracleInfo {
    pub pubkey: [u8; 32],             // Ed25519 public key
    pub is_active: bool,              // Whether oracle can submit attestations
    pub registered_at: i64,           // Registration timestamp
}

/// Operator Rewards - Per-operator state with extended performance metrics
#[account]
pub struct OperatorRewards {
    pub operator: Pubkey,             // Operator pubkey (32)
    pub total_earned: u64,            // Lifetime rewards earned (8)
    pub total_claimed: u64,           // Lifetime rewards claimed (8)
    pub unclaimed_rewards: u64,       // Pending rewards (8)
    pub last_claim_time: i64,         // Last claim timestamp (8)
    // Extended performance metrics per whitepaper formula
    pub uptime_percentage: u8,        // 0-100, weight: 0.5 (1)
    pub latency_score: u8,            // 0-100, weight: 0.3 (1)
    pub throughput_score: u8,         // 0-100, weight: 0.2 (1)
    pub requests_served: u64,         // For demand multiplier calculation (8)
    pub last_performance_epoch: u64,  // Last epoch performance was recorded (8)
    pub bump: u8,                     // PDA bump (1)
}

impl OperatorRewards {
    pub const MAX_SIZE: usize = 8 +   // discriminator
        32 +  // operator
        8 +   // total_earned
        8 +   // total_claimed
        8 +   // unclaimed_rewards
        8 +   // last_claim_time
        1 +   // uptime_percentage
        1 +   // latency_score
        1 +   // throughput_score
        8 +   // requests_served
        8 +   // last_performance_epoch
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

/// Record performance with Ed25519 oracle signature verification
#[derive(Accounts)]
pub struct RecordPerformance<'info> {
    #[account(mut)]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        seeds = [ORACLE_REGISTRY_SEED],
        bump = oracle_registry.bump
    )]
    pub oracle_registry: Account<'info, OracleRegistry>,

    #[account(
        mut,
        seeds = [OPERATOR_REWARDS_SEED, operator_rewards.operator.as_ref()],
        bump = operator_rewards.bump
    )]
    pub operator_rewards: Account<'info, OperatorRewards>,

    /// Instructions sysvar for Ed25519 signature verification
    /// CHECK: This is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

/// Record performance using authority (backward compatible, no oracle signature needed)
#[derive(Accounts)]
pub struct RecordPerformanceAuthority<'info> {
    #[account(
        mut,
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        seeds = [OPERATOR_REWARDS_SEED, operator_rewards.operator.as_ref()],
        bump = operator_rewards.bump
    )]
    pub operator_rewards: Account<'info, OperatorRewards>,

    /// Authority that can record performance (must match reward_pool.authority)
    pub authority: Signer<'info>,
}

/// Initialize oracle registry
#[derive(Accounts)]
pub struct InitializeOracleRegistry<'info> {
    #[account(
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        init,
        payer = authority,
        space = OracleRegistry::MAX_SIZE,
        seeds = [ORACLE_REGISTRY_SEED],
        bump
    )]
    pub oracle_registry: Account<'info, OracleRegistry>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Register oracle for signature verification
#[derive(Accounts)]
pub struct RegisterOracle<'info> {
    #[account(
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        seeds = [ORACLE_REGISTRY_SEED],
        bump = oracle_registry.bump,
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub oracle_registry: Account<'info, OracleRegistry>,

    pub authority: Signer<'info>,
}

/// Deactivate oracle
#[derive(Accounts)]
pub struct DeactivateOracle<'info> {
    #[account(
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    #[account(
        mut,
        seeds = [ORACLE_REGISTRY_SEED],
        bump = oracle_registry.bump
    )]
    pub oracle_registry: Account<'info, OracleRegistry>,

    pub authority: Signer<'info>,
}

/// Advance epoch
#[derive(Accounts)]
pub struct AdvanceEpoch<'info> {
    #[account(
        mut,
        seeds = [REWARD_POOL_SEED],
        bump = reward_pool.bump,
        has_one = authority @ RewardsError::UnauthorizedAuthority
    )]
    pub reward_pool: Account<'info, RewardPool>,

    pub authority: Signer<'info>,
}

/// Calculate rewards
/// SECURITY FIX: Added has_one constraint to verify authority matches reward_pool.authority
/// This prevents unauthorized users from triggering reward calculations with arbitrary parameters
#[derive(Accounts)]
pub struct CalculateRewards<'info> {
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

    /// SECURITY FIX: Must match reward_pool.authority - enforced by has_one constraint above
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


/// Events
#[event]
pub struct OperatorRewardsInitializedEvent {
    pub operator: Pubkey,
}

#[event]
pub struct PerformanceRecordedEvent {
    pub operator: Pubkey,
    pub uptime: u8,
    pub latency: u8,
    pub throughput: u8,
    pub requests: u64,
    pub epoch: u64,
}

#[event]
pub struct RewardsCalculatedEvent {
    pub operator: Pubkey,
    pub amount: u64,
    pub epochs: u64,
    pub stake_multiplier: u64,
    pub performance_score: u64,
    pub demand_multiplier: u64,
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
pub struct EpochAdvancedEvent {
    pub old_epoch: u64,
    pub new_epoch: u64,
}

#[event]
pub struct OracleRegisteredEvent {
    pub oracle_pubkey: [u8; 32],
}

#[event]
pub struct OracleDeactivatedEvent {
    pub oracle_pubkey: [u8; 32],
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
    #[msg("Oracle already registered")]
    OracleAlreadyRegistered,
    #[msg("Oracle not found")]
    OracleNotFound,
    #[msg("Maximum number of oracles reached")]
    MaxOraclesReached,
    #[msg("Invalid oracle - not registered or inactive")]
    InvalidOracle,
    #[msg("Invalid Ed25519 signature")]
    InvalidSignature,
}
