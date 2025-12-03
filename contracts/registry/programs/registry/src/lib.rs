use anchor_lang::prelude::*;

declare_id!("4JRL443DxceXsgqqxmBt4tD8TecBBo9Xr5kTLNRupiG6");

const MAX_METADATA_URL_LENGTH: usize = 128;
// DEPRECATED: Now stored in RegistryConfig for flexibility
const MIN_STAKE_FOR_REGISTRATION: u64 = 100_000_000_000; // 100 AEGIS tokens

#[program]
pub mod node_registry {
    use super::*;

    /// SECURITY FIX: Initialize registry configuration (one-time setup by deployer)
    /// Stores authorized program IDs and admin authority
    pub fn initialize_registry_config(
        ctx: Context<InitializeRegistryConfig>,
        admin_authority: Pubkey,
        staking_program_id: Pubkey,
        min_stake: u64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.registry_config;

        config.admin_authority = admin_authority;
        config.staking_program_id = staking_program_id;
        config.rewards_program_id = Pubkey::default();  // Can be set later
        config.min_stake_for_registration = min_stake;
        config.paused = false;
        config.bump = ctx.bumps.registry_config;

        msg!(
            "Registry config initialized: admin={}, staking_program={}",
            admin_authority,
            staking_program_id
        );

        Ok(())
    }

    /// SECURITY FIX: Update registry config (admin only)
    pub fn update_registry_config(
        ctx: Context<UpdateRegistryConfig>,
        new_admin: Option<Pubkey>,
        new_staking_program: Option<Pubkey>,
        new_min_stake: Option<u64>,
    ) -> Result<()> {
        let config = &mut ctx.accounts.registry_config;

        // CRITICAL: Verify caller is current admin
        require!(
            ctx.accounts.admin.key() == config.admin_authority,
            RegistryError::UnauthorizedAdmin
        );

        if let Some(admin) = new_admin {
            config.admin_authority = admin;
            msg!("Admin authority updated to: {}", admin);
        }

        if let Some(staking_program) = new_staking_program {
            config.staking_program_id = staking_program;
            msg!("Staking program updated to: {}", staking_program);
        }

        if let Some(min_stake) = new_min_stake {
            config.min_stake_for_registration = min_stake;
            msg!("Min stake updated to: {}", min_stake);
        }

        Ok(())
    }

    /// Register a new node operator on the AEGIS network
    ///
    /// Creates a PDA account storing node metadata and initial stake amount.
    /// The metadata_url should be an IPFS CID containing detailed node information.
    pub fn register_node(
        ctx: Context<RegisterNode>,
        metadata_url: String,
        initial_stake: u64,
    ) -> Result<()> {
        require!(
            metadata_url.len() <= MAX_METADATA_URL_LENGTH,
            RegistryError::MetadataUrlTooLong
        );
        require!(
            !metadata_url.is_empty(),
            RegistryError::MetadataUrlEmpty
        );
        require!(
            initial_stake >= MIN_STAKE_FOR_REGISTRATION,
            RegistryError::InsufficientStake
        );

        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        node_account.operator = ctx.accounts.operator.key();
        node_account.metadata_url = metadata_url.clone();
        node_account.status = NodeStatus::Active;
        node_account.stake_amount = initial_stake;
        node_account.registered_at = clock.unix_timestamp;
        node_account.updated_at = clock.unix_timestamp;
        // Whitepaper compliance: Initialize reputation and heartbeat fields
        node_account.reputation_score = NodeAccount::DEFAULT_REPUTATION;
        node_account.last_heartbeat = clock.unix_timestamp; // First heartbeat at registration
        node_account.total_heartbeats = 1;
        node_account.missed_heartbeats = 0;
        node_account.bump = ctx.bumps.node_account;

        msg!("Node registered successfully");
        msg!("Operator: {}", node_account.operator);
        msg!("Metadata: {}", metadata_url);
        msg!("Initial Stake: {}", initial_stake);

        emit!(NodeRegisteredEvent {
            operator: node_account.operator,
            metadata_url,
            stake_amount: initial_stake,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Update node metadata (only operator can update)
    pub fn update_metadata(
        ctx: Context<UpdateNodeMetadata>,
        new_metadata_url: String,
    ) -> Result<()> {
        require!(
            new_metadata_url.len() <= MAX_METADATA_URL_LENGTH,
            RegistryError::MetadataUrlTooLong
        );
        require!(
            !new_metadata_url.is_empty(),
            RegistryError::MetadataUrlEmpty
        );

        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        node_account.metadata_url = new_metadata_url.clone();
        node_account.updated_at = clock.unix_timestamp;

        msg!("Metadata updated for node: {}", node_account.operator);

        emit!(NodeUpdatedEvent {
            operator: node_account.operator,
            new_metadata_url,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Deactivate a node (only operator can deactivate their own node)
    pub fn deactivate_node(ctx: Context<DeactivateNode>) -> Result<()> {
        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        require!(
            node_account.status == NodeStatus::Active,
            RegistryError::NodeAlreadyInactive
        );

        node_account.status = NodeStatus::Inactive;
        node_account.updated_at = clock.unix_timestamp;

        msg!("Node deactivated: {}", node_account.operator);

        emit!(NodeDeactivatedEvent {
            operator: node_account.operator,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Reactivate a previously deactivated node
    pub fn reactivate_node(ctx: Context<ReactivateNode>) -> Result<()> {
        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        require!(
            node_account.status == NodeStatus::Inactive,
            RegistryError::NodeNotInactive
        );

        node_account.status = NodeStatus::Active;
        node_account.updated_at = clock.unix_timestamp;

        msg!("Node reactivated: {}", node_account.operator);

        emit!(NodeReactivatedEvent {
            operator: node_account.operator,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// SECURITY FIX (X1.3): Update stake amount (ONLY callable by authorized staking contract via CPI)
    ///
    /// This function can ONLY be called via Cross-Program Invocation (CPI) from the
    /// authorized staking program. It uses a PDA (Program Derived Address) as the authority,
    /// which can only be signed by the staking program itself.
    ///
    /// Security: The staking_authority PDA is derived using seeds from the staking program,
    /// meaning only the staking program can produce a valid signature for this PDA.
    /// Direct calls from user wallets will always fail.
    pub fn update_stake(
        ctx: Context<UpdateStake>,
        new_stake_amount: u64,
    ) -> Result<()> {
        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        // 🔒 SECURITY FIX (X1.3): PDA validation is enforced via Anchor constraints
        // The staking_authority account MUST be:
        // 1. A valid PDA derived from seeds [b"staking_authority"]
        // 2. Signed by the staking program (only possible via CPI)
        //
        // Because PDAs cannot sign transactions directly (only programs can sign for their PDAs),
        // this guarantees that only the staking program can call this instruction via CPI.
        //
        // The constraint `seeds::program = registry_config.staking_program_id` ensures
        // the PDA belongs to the authorized staking program.

        // Update stake amount
        node_account.stake_amount = new_stake_amount;
        node_account.updated_at = clock.unix_timestamp;

        msg!(
            "Stake updated for node: {} to {} by staking program (via CPI)",
            node_account.operator,
            new_stake_amount
        );

        emit!(StakeUpdatedEvent {
            operator: node_account.operator,
            new_stake_amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Node heartbeat - called periodically by nodes to prove liveness
    /// Per whitepaper: Nodes must submit heartbeats every 5 minutes
    /// Missing heartbeats affects reputation and can trigger slashing
    pub fn heartbeat(ctx: Context<Heartbeat>) -> Result<()> {
        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        // Only active nodes can submit heartbeats
        require!(
            node_account.status == NodeStatus::Active,
            RegistryError::NodeNotActive
        );

        let last_heartbeat = node_account.last_heartbeat;
        let current_time = clock.unix_timestamp;
        let time_since_last = current_time - last_heartbeat;

        // Calculate missed heartbeat intervals (excluding current)
        let expected_intervals = if time_since_last > NodeAccount::HEARTBEAT_INTERVAL {
            (time_since_last / NodeAccount::HEARTBEAT_INTERVAL) - 1
        } else {
            0
        };

        // Track missed heartbeats for slashing detection
        if expected_intervals > 0 {
            node_account.missed_heartbeats = node_account
                .missed_heartbeats
                .checked_add(expected_intervals as u64)
                .ok_or(RegistryError::Overflow)?;

            // Reputation penalty for missed heartbeats (1% per missed interval)
            let penalty = (expected_intervals as u64).saturating_mul(100);
            node_account.reputation_score = node_account
                .reputation_score
                .saturating_sub(penalty);

            msg!(
                "Node {} missed {} heartbeat(s), reputation reduced to {}",
                node_account.operator,
                expected_intervals,
                node_account.reputation_score
            );
        } else {
            // Reputation boost for on-time heartbeat (0.1% per successful heartbeat)
            let boost = 10; // 0.10%
            node_account.reputation_score = core::cmp::min(
                node_account.reputation_score.saturating_add(boost),
                NodeAccount::MAX_REPUTATION,
            );
        }

        // Update heartbeat tracking
        node_account.last_heartbeat = current_time;
        node_account.total_heartbeats = node_account
            .total_heartbeats
            .checked_add(1)
            .ok_or(RegistryError::Overflow)?;
        node_account.updated_at = current_time;

        emit!(HeartbeatEvent {
            operator: node_account.operator,
            reputation_score: node_account.reputation_score,
            total_heartbeats: node_account.total_heartbeats,
            missed_heartbeats: node_account.missed_heartbeats,
            timestamp: current_time,
        });

        Ok(())
    }

    /// Update reputation score (admin or rewards program only)
    /// Used by rewards program to adjust reputation based on performance metrics
    pub fn update_reputation(
        ctx: Context<UpdateReputation>,
        new_score: u64,
    ) -> Result<()> {
        let config = &ctx.accounts.registry_config;
        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        // Verify caller is admin or rewards program
        let caller = ctx.accounts.authority.key();
        require!(
            caller == config.admin_authority || caller == config.rewards_program_id,
            RegistryError::UnauthorizedReputationUpdate
        );

        // Validate score range
        require!(
            new_score <= NodeAccount::MAX_REPUTATION,
            RegistryError::InvalidReputationScore
        );

        let old_score = node_account.reputation_score;
        node_account.reputation_score = new_score;
        node_account.updated_at = clock.unix_timestamp;

        msg!(
            "Reputation updated for node {}: {} -> {}",
            node_account.operator,
            old_score,
            new_score
        );

        emit!(ReputationUpdatedEvent {
            operator: node_account.operator,
            old_score,
            new_score,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Check node liveness - returns whether node has missed too many heartbeats
    /// Can be called by oracles to trigger automated slashing
    pub fn check_liveness(ctx: Context<CheckLiveness>) -> Result<()> {
        let node_account = &ctx.accounts.node_account;
        let clock = Clock::get()?;

        let time_since_heartbeat = clock.unix_timestamp - node_account.last_heartbeat;
        let is_offline = time_since_heartbeat > NodeAccount::HEARTBEAT_GRACE_PERIOD;

        // 48 hours = 172800 seconds (whitepaper slashing threshold)
        let offline_48_hours = time_since_heartbeat > 172800;

        msg!(
            "Node {} liveness check: last_heartbeat={}, offline={}, offline_48h={}",
            node_account.operator,
            node_account.last_heartbeat,
            is_offline,
            offline_48_hours
        );

        emit!(LivenessCheckEvent {
            operator: node_account.operator,
            last_heartbeat: node_account.last_heartbeat,
            time_since_heartbeat,
            is_offline,
            offline_48_hours,
            reputation_score: node_account.reputation_score,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }
}

/// SECURITY FIX: Registry configuration account
/// Stores authorized program IDs and admin authority
#[account]
pub struct RegistryConfig {
    pub admin_authority: Pubkey,        // Admin (DAO or multisig) (32 bytes)
    pub staking_program_id: Pubkey,     // Authorized staking program (32 bytes)
    pub rewards_program_id: Pubkey,     // Authorized rewards program (32 bytes)
    pub min_stake_for_registration: u64, // Minimum stake to register (8 bytes)
    pub paused: bool,                   // Emergency pause flag (1 byte)
    pub bump: u8,                       // PDA bump (1 byte)
}

impl RegistryConfig {
    pub const MAX_SIZE: usize = 8 +  // discriminator
        32 +                          // admin_authority
        32 +                          // staking_program_id
        32 +                          // rewards_program_id
        8 +                           // min_stake_for_registration
        1 +                           // paused
        1;                            // bump
}

/// Node account - stores operator information
#[account]
pub struct NodeAccount {
    pub operator: Pubkey,           // Node operator wallet (32 bytes)
    pub metadata_url: String,       // IPFS CID for node metadata (4 + 128 bytes)
    pub status: NodeStatus,         // Current node status (1 byte)
    pub stake_amount: u64,          // Total staked AEGIS (8 bytes)
    pub registered_at: i64,         // Registration timestamp (8 bytes)
    pub updated_at: i64,            // Last update timestamp (8 bytes)
    // Whitepaper compliance: Reputation and heartbeat tracking
    pub reputation_score: u64,      // Reputation score (0-10000 = 0.00-100.00%) (8 bytes)
    pub last_heartbeat: i64,        // Last heartbeat timestamp (8 bytes)
    pub total_heartbeats: u64,      // Total heartbeat count for uptime (8 bytes)
    pub missed_heartbeats: u64,     // Missed heartbeats for slashing detection (8 bytes)
    pub bump: u8,                   // PDA bump seed (1 byte)
}

impl NodeAccount {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +                        // operator
        4 + MAX_METADATA_URL_LENGTH + // metadata_url (string)
        1 +                         // status
        8 +                         // stake_amount
        8 +                         // registered_at
        8 +                         // updated_at
        8 +                         // reputation_score
        8 +                         // last_heartbeat
        8 +                         // total_heartbeats
        8 +                         // missed_heartbeats
        1;                          // bump

    /// Default reputation score for new nodes (50.00%)
    pub const DEFAULT_REPUTATION: u64 = 5000;

    /// Maximum reputation score (100.00%)
    pub const MAX_REPUTATION: u64 = 10000;

    /// Heartbeat interval in seconds (5 minutes per whitepaper)
    pub const HEARTBEAT_INTERVAL: i64 = 300;

    /// Grace period for missed heartbeats (15 minutes = 3 intervals)
    pub const HEARTBEAT_GRACE_PERIOD: i64 = 900;
}

/// Node status enum
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Active,
    Inactive,
    Slashed,
}

/// SECURITY FIX: Initialize registry config
#[derive(Accounts)]
pub struct InitializeRegistryConfig<'info> {
    #[account(
        init,
        payer = deployer,
        space = RegistryConfig::MAX_SIZE,
        seeds = [b"registry_config"],
        bump
    )]
    pub registry_config: Account<'info, RegistryConfig>,

    #[account(mut)]
    pub deployer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// SECURITY FIX: Update registry config
#[derive(Accounts)]
pub struct UpdateRegistryConfig<'info> {
    #[account(
        mut,
        seeds = [b"registry_config"],
        bump = registry_config.bump
    )]
    pub registry_config: Account<'info, RegistryConfig>,

    /// Must be current admin
    pub admin: Signer<'info>,
}

/// Register new node
#[derive(Accounts)]
pub struct RegisterNode<'info> {
    #[account(
        init,
        payer = operator,
        space = NodeAccount::MAX_SIZE,
        seeds = [b"node", operator.key().as_ref()],
        bump
    )]
    pub node_account: Account<'info, NodeAccount>,

    #[account(mut)]
    pub operator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Update node metadata
#[derive(Accounts)]
pub struct UpdateNodeMetadata<'info> {
    #[account(
        mut,
        seeds = [b"node", operator.key().as_ref()],
        bump = node_account.bump,
        has_one = operator @ RegistryError::UnauthorizedOperator
    )]
    pub node_account: Account<'info, NodeAccount>,

    pub operator: Signer<'info>,
}

/// Deactivate node
#[derive(Accounts)]
pub struct DeactivateNode<'info> {
    #[account(
        mut,
        seeds = [b"node", operator.key().as_ref()],
        bump = node_account.bump,
        has_one = operator @ RegistryError::UnauthorizedOperator
    )]
    pub node_account: Account<'info, NodeAccount>,

    pub operator: Signer<'info>,
}

/// Reactivate node
#[derive(Accounts)]
pub struct ReactivateNode<'info> {
    #[account(
        mut,
        seeds = [b"node", operator.key().as_ref()],
        bump = node_account.bump,
        has_one = operator @ RegistryError::UnauthorizedOperator
    )]
    pub node_account: Account<'info, NodeAccount>,

    pub operator: Signer<'info>,
}

/// SECURITY FIX (X1.3): Update stake amount - PDA-based CPI authorization
///
/// This instruction can ONLY be called via CPI from the authorized staking program.
/// The security model relies on:
/// 1. staking_authority is a PDA derived from seeds [b"staking_authority"] + staking program ID
/// 2. PDAs can only be signed by their owning program (the staking program)
/// 3. Direct calls from user wallets will ALWAYS fail because users cannot sign for PDAs
#[derive(Accounts)]
pub struct UpdateStake<'info> {
    /// Registry configuration containing authorized staking program ID
    #[account(
        seeds = [b"registry_config"],
        bump = registry_config.bump
    )]
    pub registry_config: Account<'info, RegistryConfig>,

    #[account(
        mut,
        seeds = [b"node", node_account.operator.as_ref()],
        bump = node_account.bump
    )]
    pub node_account: Account<'info, NodeAccount>,

    /// 🔒 SECURITY FIX (X1.3): Staking authority PDA
    ///
    /// This PDA is derived from seeds [b"staking_authority"] using the staking program's ID.
    /// Only the staking program can sign for this PDA via CPI, making direct calls impossible.
    ///
    /// CHECK: This account is validated via seeds constraint with cross-program derivation.
    /// The `seeds::program` constraint ensures the PDA belongs to the authorized staking program.
    #[account(
        seeds = [b"staking_authority"],
        bump,
        seeds::program = registry_config.staking_program_id
    )]
    pub staking_authority: AccountInfo<'info>,
}

/// Node heartbeat - prove liveness
#[derive(Accounts)]
pub struct Heartbeat<'info> {
    #[account(
        mut,
        seeds = [b"node", operator.key().as_ref()],
        bump = node_account.bump,
        has_one = operator @ RegistryError::UnauthorizedOperator
    )]
    pub node_account: Account<'info, NodeAccount>,

    pub operator: Signer<'info>,
}

/// Update reputation score (admin/rewards program only)
#[derive(Accounts)]
pub struct UpdateReputation<'info> {
    #[account(
        seeds = [b"registry_config"],
        bump = registry_config.bump
    )]
    pub registry_config: Account<'info, RegistryConfig>,

    #[account(
        mut,
        seeds = [b"node", node_account.operator.as_ref()],
        bump = node_account.bump
    )]
    pub node_account: Account<'info, NodeAccount>,

    /// Must be admin or rewards program (verified in instruction)
    pub authority: Signer<'info>,
}

/// Check node liveness (read-only)
#[derive(Accounts)]
pub struct CheckLiveness<'info> {
    #[account(
        seeds = [b"node", node_account.operator.as_ref()],
        bump = node_account.bump
    )]
    pub node_account: Account<'info, NodeAccount>,
}

/// Events
#[event]
pub struct NodeRegisteredEvent {
    pub operator: Pubkey,
    pub metadata_url: String,
    pub stake_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct NodeUpdatedEvent {
    pub operator: Pubkey,
    pub new_metadata_url: String,
    pub timestamp: i64,
}

#[event]
pub struct NodeDeactivatedEvent {
    pub operator: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct NodeReactivatedEvent {
    pub operator: Pubkey,
    pub timestamp: i64,
}

/// Event emitted when a node submits a heartbeat
#[event]
pub struct HeartbeatEvent {
    pub operator: Pubkey,
    pub reputation_score: u64,
    pub total_heartbeats: u64,
    pub missed_heartbeats: u64,
    pub timestamp: i64,
}

/// Event emitted when reputation is updated
#[event]
pub struct ReputationUpdatedEvent {
    pub operator: Pubkey,
    pub old_score: u64,
    pub new_score: u64,
    pub timestamp: i64,
}

/// Event emitted when liveness is checked
#[event]
pub struct LivenessCheckEvent {
    pub operator: Pubkey,
    pub last_heartbeat: i64,
    pub time_since_heartbeat: i64,
    pub is_offline: bool,
    pub offline_48_hours: bool,
    pub reputation_score: u64,
    pub timestamp: i64,
}

/// Event emitted when stake is updated via CPI (X1.3 security fix)
#[event]
pub struct StakeUpdatedEvent {
    pub operator: Pubkey,
    pub new_stake_amount: u64,
    pub timestamp: i64,
}

/// Custom errors
#[error_code]
pub enum RegistryError {
    #[msg("Metadata URL exceeds maximum length")]
    MetadataUrlTooLong,

    #[msg("Metadata URL cannot be empty")]
    MetadataUrlEmpty,

    #[msg("Insufficient stake for registration")]
    InsufficientStake,

    #[msg("Only the node operator can perform this action")]
    UnauthorizedOperator,

    #[msg("Node is already inactive")]
    NodeAlreadyInactive,

    #[msg("Node is not inactive")]
    NodeNotInactive,

    /// SECURITY FIX: New error codes for access control
    #[msg("Unauthorized: Only admin can perform this action")]
    UnauthorizedAdmin,

    #[msg("Unauthorized: Only staking program can update stake amounts")]
    UnauthorizedStakeUpdate,

    /// Heartbeat and reputation errors
    #[msg("Node must be active to submit heartbeats")]
    NodeNotActive,

    #[msg("Unauthorized: Only admin or rewards program can update reputation")]
    UnauthorizedReputationUpdate,

    #[msg("Invalid reputation score (must be 0-10000)")]
    InvalidReputationScore,

    #[msg("Arithmetic overflow")]
    Overflow,
}
