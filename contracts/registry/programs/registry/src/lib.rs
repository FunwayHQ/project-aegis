use anchor_lang::prelude::*;

declare_id!("GLpPpGCANeD7mLuY7XdJ2mAXX7MSLEdaLr91MMjoscno");

const MAX_METADATA_URL_LENGTH: usize = 128;
const MIN_STAKE_FOR_REGISTRATION: u64 = 100_000_000_000; // 100 AEGIS tokens

#[program]
pub mod node_registry {
    use super::*;

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

    /// Update stake amount (called by staking contract)
    pub fn update_stake(
        ctx: Context<UpdateStake>,
        new_stake_amount: u64,
    ) -> Result<()> {
        let node_account = &mut ctx.accounts.node_account;
        let clock = Clock::get()?;

        node_account.stake_amount = new_stake_amount;
        node_account.updated_at = clock.unix_timestamp;

        msg!("Stake updated for node: {} to {}", node_account.operator, new_stake_amount);

        Ok(())
    }
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
        1;                          // bump
}

/// Node status enum
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Active,
    Inactive,
    Slashed,
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

/// Update stake amount
#[derive(Accounts)]
pub struct UpdateStake<'info> {
    #[account(
        mut,
        seeds = [b"node", node_account.operator.as_ref()],
        bump = node_account.bump
    )]
    pub node_account: Account<'info, NodeAccount>,

    /// Authority (staking contract or operator)
    pub authority: Signer<'info>,
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
}
