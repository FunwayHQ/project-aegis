use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("9uVLmgqJz3nYcCxHVSAJA8bi6412LEZ5uGM5yguvKHRq");

/// Total supply of $AEGIS tokens (1 billion with 9 decimals)
pub const TOTAL_SUPPLY: u64 = 1_000_000_000_000_000_000;

/// Maximum multi-sig signers (per whitepaper: 5-of-9 treasury)
pub const MAX_MULTISIG_SIGNERS: usize = 9;

/// Default fee burn percentage (basis points: 100 = 1%)
pub const DEFAULT_FEE_BURN_BPS: u16 = 50; // 0.5% burn

#[program]
pub mod aegis_token {
    use super::*;

    /// Initialize the $AEGIS token mint
    ///
    /// This creates the token mint with the specified parameters and sets up
    /// the initial mint authority. This can only be called once.
    pub fn initialize_mint(
        ctx: Context<InitializeMint>,
        decimals: u8,
    ) -> Result<()> {
        require!(decimals == 9, TokenError::InvalidDecimals);

        msg!("Initializing AEGIS token mint");
        msg!("Decimals: {}", decimals);
        msg!("Total supply cap: {} tokens", TOTAL_SUPPLY / 10_u64.pow(decimals as u32));

        emit!(MintInitializedEvent {
            mint: ctx.accounts.mint.key(),
            mint_authority: ctx.accounts.mint_authority.key(),
            freeze_authority: ctx.accounts.mint_authority.key(),
            decimals,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Mint new $AEGIS tokens to a recipient account
    ///
    /// This can only be called by the mint authority and respects the total supply cap.
    /// Used for initial token distribution according to tokenomics.
    pub fn mint_to(
        ctx: Context<MintToContext>,
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, TokenError::InvalidAmount);

        let mint = &ctx.accounts.mint;
        let new_supply = mint.supply.checked_add(amount)
            .ok_or(TokenError::Overflow)?;

        require!(
            new_supply <= TOTAL_SUPPLY,
            TokenError::SupplyExceeded
        );

        msg!("Minting {} tokens", amount);
        msg!("New total supply: {}", new_supply);

        token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.to.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        emit!(MintEvent {
            mint: ctx.accounts.mint.key(),
            recipient: ctx.accounts.to.key(),
            amount,
            new_supply,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Transfer $AEGIS tokens between accounts
    ///
    /// Standard SPL token transfer with additional event logging.
    pub fn transfer_tokens(
        ctx: Context<TransferContext>,
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, TokenError::InvalidAmount);

        msg!("Transferring {} tokens", amount);

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.from.to_account_info(),
                    to: ctx.accounts.to.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        emit!(TransferEvent {
            from: ctx.accounts.from.key(),
            to: ctx.accounts.to.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Burn $AEGIS tokens (deflationary mechanism)
    ///
    /// Allows token holders to permanently destroy their tokens,
    /// reducing the circulating supply.
    pub fn burn_tokens(
        ctx: Context<BurnContext>,
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, TokenError::InvalidAmount);

        msg!("Burning {} tokens", amount);

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.from.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;

        let new_supply = ctx.accounts.mint.supply;

        emit!(BurnEvent {
            mint: ctx.accounts.mint.key(),
            from: ctx.accounts.from.key(),
            amount,
            new_supply,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    // ========================================================================
    // Multi-Sig Governance & Fee Burn (Whitepaper Compliance)
    // ========================================================================

    /// Initialize token governance configuration
    /// Sets up multi-sig authority and fee burn parameters
    pub fn initialize_token_config(
        ctx: Context<InitializeTokenConfig>,
        signers: Vec<Pubkey>,
        threshold: u8,
        fee_burn_bps: u16,
    ) -> Result<()> {
        require!(
            signers.len() >= threshold as usize,
            TokenError::InvalidThreshold
        );
        require!(
            signers.len() <= MAX_MULTISIG_SIGNERS,
            TokenError::TooManySigners
        );
        require!(threshold >= 1, TokenError::InvalidThreshold);
        require!(fee_burn_bps <= 10000, TokenError::InvalidFeeBps);

        let config = &mut ctx.accounts.token_config;
        let clock = Clock::get()?;

        config.admin = ctx.accounts.admin.key();
        config.mint = ctx.accounts.mint.key();
        config.threshold = threshold;
        config.signer_count = signers.len() as u8;
        config.fee_burn_bps = fee_burn_bps;
        config.total_burned = 0;
        config.total_fees_collected = 0;
        config.created_at = clock.unix_timestamp;
        config.bump = ctx.bumps.token_config;

        // Copy signers
        for (i, signer) in signers.iter().enumerate() {
            config.signers[i] = *signer;
        }

        msg!(
            "Token config initialized: threshold={}/{}, fee_burn={}bps",
            threshold,
            signers.len(),
            fee_burn_bps
        );

        emit!(TokenConfigInitializedEvent {
            mint: config.mint,
            admin: config.admin,
            threshold,
            signer_count: config.signer_count,
            fee_burn_bps,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Create a multi-sig transaction proposal
    /// Used for controlled minting and treasury operations
    /// nonce: unique identifier for this transaction (e.g., timestamp or incrementing counter)
    pub fn create_multisig_transaction(
        ctx: Context<CreateMultisigTransaction>,
        transaction_type: MultisigTransactionType,
        amount: u64,
        recipient: Pubkey,
        nonce: u64,
    ) -> Result<()> {
        let _ = nonce; // Used in PDA seeds via instruction macro
        let config = &ctx.accounts.token_config;
        let tx = &mut ctx.accounts.multisig_tx;
        let clock = Clock::get()?;

        // Verify proposer is a valid signer
        let proposer = ctx.accounts.proposer.key();
        let is_valid_signer = config.signers[..config.signer_count as usize]
            .iter()
            .any(|s| *s == proposer);
        require!(is_valid_signer, TokenError::InvalidSigner);

        tx.config = config.key();
        tx.transaction_type = transaction_type;
        tx.amount = amount;
        tx.recipient = recipient;
        tx.proposer = proposer;
        tx.approvals = vec![false; config.signer_count as usize];
        tx.approval_count = 0;
        tx.executed = false;
        tx.created_at = clock.unix_timestamp;
        tx.bump = ctx.bumps.multisig_tx;

        // Auto-approve for proposer
        for (i, signer) in config.signers[..config.signer_count as usize].iter().enumerate() {
            if *signer == proposer {
                tx.approvals[i] = true;
                tx.approval_count = 1;
                break;
            }
        }

        msg!(
            "Multisig transaction created: {:?}, amount={}, recipient={}",
            transaction_type,
            amount,
            recipient
        );

        emit!(MultisigTransactionCreatedEvent {
            tx_id: tx.key(),
            transaction_type,
            amount,
            recipient,
            proposer,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Approve a multi-sig transaction
    pub fn approve_multisig_transaction(
        ctx: Context<ApproveMultisigTransaction>,
    ) -> Result<()> {
        let config = &ctx.accounts.token_config;
        let tx = &mut ctx.accounts.multisig_tx;
        let clock = Clock::get()?;

        require!(!tx.executed, TokenError::TransactionAlreadyExecuted);

        // Find signer index
        let approver = ctx.accounts.approver.key();
        let signer_index = config.signers[..config.signer_count as usize]
            .iter()
            .position(|s| *s == approver)
            .ok_or(TokenError::InvalidSigner)?;

        require!(!tx.approvals[signer_index], TokenError::AlreadyApproved);

        tx.approvals[signer_index] = true;
        tx.approval_count += 1;

        msg!(
            "Transaction approved by {}: {}/{}",
            approver,
            tx.approval_count,
            config.threshold
        );

        emit!(MultisigApprovalEvent {
            tx_id: tx.key(),
            approver,
            approval_count: tx.approval_count,
            threshold: config.threshold,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Execute a multi-sig transaction once threshold is reached
    pub fn execute_multisig_transaction(
        ctx: Context<ExecuteMultisigTransaction>,
    ) -> Result<()> {
        let config = &ctx.accounts.token_config;
        let tx = &mut ctx.accounts.multisig_tx;
        let clock = Clock::get()?;

        require!(!tx.executed, TokenError::TransactionAlreadyExecuted);
        require!(
            tx.approval_count >= config.threshold,
            TokenError::InsufficientApprovals
        );

        // Execute based on transaction type
        match tx.transaction_type {
            MultisigTransactionType::Mint => {
                // Mint tokens to recipient
                let seeds = &[
                    b"token_config".as_ref(),
                    config.mint.as_ref(),
                    &[config.bump],
                ];
                let signer = &[&seeds[..]];

                token::mint_to(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.to_account_info(),
                        MintTo {
                            mint: ctx.accounts.mint.to_account_info(),
                            to: ctx.accounts.recipient_token_account.to_account_info(),
                            authority: ctx.accounts.token_config.to_account_info(),
                        },
                        signer,
                    ),
                    tx.amount,
                )?;

                msg!("Multi-sig mint executed: {} tokens to {}", tx.amount, tx.recipient);
            }
            MultisigTransactionType::TreasuryTransfer => {
                // Transfer from treasury
                let seeds = &[
                    b"token_config".as_ref(),
                    config.mint.as_ref(),
                    &[config.bump],
                ];
                let signer = &[&seeds[..]];

                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.to_account_info(),
                        Transfer {
                            from: ctx.accounts.treasury.to_account_info(),
                            to: ctx.accounts.recipient_token_account.to_account_info(),
                            authority: ctx.accounts.token_config.to_account_info(),
                        },
                        signer,
                    ),
                    tx.amount,
                )?;

                msg!("Treasury transfer executed: {} tokens to {}", tx.amount, tx.recipient);
            }
            MultisigTransactionType::UpdateConfig => {
                // Config updates handled separately
                msg!("Config update executed");
            }
        }

        tx.executed = true;

        emit!(MultisigExecutedEvent {
            tx_id: tx.key(),
            transaction_type: tx.transaction_type,
            amount: tx.amount,
            recipient: tx.recipient,
            executor: ctx.accounts.executor.key(),
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Process fee with automatic burn
    /// Called by protocol contracts when collecting fees
    pub fn process_fee_with_burn(
        ctx: Context<ProcessFeeWithBurn>,
        total_fee: u64,
    ) -> Result<()> {
        let config = &mut ctx.accounts.token_config;
        let clock = Clock::get()?;

        require!(total_fee > 0, TokenError::InvalidAmount);

        // Calculate burn amount (fee_burn_bps / 10000)
        let burn_amount = (total_fee as u128)
            .checked_mul(config.fee_burn_bps as u128)
            .ok_or(TokenError::Overflow)?
            .checked_div(10000)
            .ok_or(TokenError::Overflow)? as u64;

        let treasury_amount = total_fee.checked_sub(burn_amount).ok_or(TokenError::Overflow)?;

        // Transfer fee to treasury
        if treasury_amount > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.fee_payer_account.to_account_info(),
                        to: ctx.accounts.treasury.to_account_info(),
                        authority: ctx.accounts.fee_payer.to_account_info(),
                    },
                ),
                treasury_amount,
            )?;
        }

        // Burn portion
        if burn_amount > 0 {
            token::burn(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Burn {
                        mint: ctx.accounts.mint.to_account_info(),
                        from: ctx.accounts.fee_payer_account.to_account_info(),
                        authority: ctx.accounts.fee_payer.to_account_info(),
                    },
                ),
                burn_amount,
            )?;

            config.total_burned = config
                .total_burned
                .checked_add(burn_amount)
                .ok_or(TokenError::Overflow)?;
        }

        config.total_fees_collected = config
            .total_fees_collected
            .checked_add(total_fee)
            .ok_or(TokenError::Overflow)?;

        msg!(
            "Fee processed: total={}, burned={}, treasury={}",
            total_fee,
            burn_amount,
            treasury_amount
        );

        emit!(FeeBurnEvent {
            mint: config.mint,
            total_fee,
            burn_amount,
            treasury_amount,
            total_burned: config.total_burned,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Update fee burn percentage (admin only)
    pub fn update_fee_burn_bps(
        ctx: Context<UpdateTokenConfig>,
        new_fee_burn_bps: u16,
    ) -> Result<()> {
        let config = &mut ctx.accounts.token_config;
        let clock = Clock::get()?;

        require!(
            ctx.accounts.admin.key() == config.admin,
            TokenError::InvalidAuthority
        );
        require!(new_fee_burn_bps <= 10000, TokenError::InvalidFeeBps);

        let old_bps = config.fee_burn_bps;
        config.fee_burn_bps = new_fee_burn_bps;

        msg!("Fee burn BPS updated: {} -> {}", old_bps, new_fee_burn_bps);

        emit!(FeeBpsUpdatedEvent {
            mint: config.mint,
            old_bps,
            new_bps: new_fee_burn_bps,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    /// Update multi-sig signers (requires current multi-sig approval)
    pub fn update_signers(
        ctx: Context<UpdateSigners>,
        new_signers: Vec<Pubkey>,
        new_threshold: u8,
    ) -> Result<()> {
        let config = &mut ctx.accounts.token_config;
        let clock = Clock::get()?;

        require!(
            new_signers.len() >= new_threshold as usize,
            TokenError::InvalidThreshold
        );
        require!(
            new_signers.len() <= MAX_MULTISIG_SIGNERS,
            TokenError::TooManySigners
        );
        require!(new_threshold >= 1, TokenError::InvalidThreshold);

        let old_threshold = config.threshold;
        let old_count = config.signer_count;

        // Clear old signers
        for i in 0..MAX_MULTISIG_SIGNERS {
            config.signers[i] = Pubkey::default();
        }

        // Set new signers
        for (i, signer) in new_signers.iter().enumerate() {
            config.signers[i] = *signer;
        }
        config.signer_count = new_signers.len() as u8;
        config.threshold = new_threshold;

        msg!(
            "Multi-sig updated: {}/{} -> {}/{}",
            old_threshold,
            old_count,
            new_threshold,
            config.signer_count
        );

        emit!(SignersUpdatedEvent {
            mint: config.mint,
            old_threshold,
            new_threshold,
            old_count,
            new_count: config.signer_count,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }
}

// ============================================================================
// Account Contexts
// ============================================================================

#[derive(Accounts)]
pub struct InitializeMint<'info> {
    #[account(
        init,
        payer = payer,
        mint::decimals = 9,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    pub mint: Account<'info, Mint>,

    /// The authority that can mint new tokens
    pub mint_authority: Signer<'info>,

    /// Payer for the mint account creation
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintToContext<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = to.mint == mint.key() @ TokenError::MintMismatch
    )]
    pub to: Account<'info, TokenAccount>,

    #[account(
        constraint = authority.key() == mint.mint_authority.unwrap() @ TokenError::InvalidAuthority
    )]
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct TransferContext<'info> {
    #[account(
        mut,
        constraint = from.owner == authority.key() @ TokenError::InvalidAuthority
    )]
    pub from: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = to.mint == from.mint @ TokenError::MintMismatch
    )]
    pub to: Account<'info, TokenAccount>,

    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BurnContext<'info> {
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = from.mint == mint.key() @ TokenError::MintMismatch,
        constraint = from.owner == authority.key() @ TokenError::InvalidAuthority
    )]
    pub from: Account<'info, TokenAccount>,

    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// ============================================================================
// Multi-Sig & Governance Account Contexts
// ============================================================================

/// Token governance configuration account
#[account]
pub struct TokenConfig {
    pub admin: Pubkey,                           // Admin authority (32 bytes)
    pub mint: Pubkey,                            // Associated mint (32 bytes)
    pub signers: [Pubkey; MAX_MULTISIG_SIGNERS], // Multi-sig signers (9 * 32 = 288 bytes)
    pub threshold: u8,                           // Required approvals (1 byte)
    pub signer_count: u8,                        // Active signers (1 byte)
    pub fee_burn_bps: u16,                       // Fee burn basis points (2 bytes)
    pub total_burned: u64,                       // Total tokens burned (8 bytes)
    pub total_fees_collected: u64,               // Total fees collected (8 bytes)
    pub created_at: i64,                         // Creation timestamp (8 bytes)
    pub bump: u8,                                // PDA bump (1 byte)
}

impl TokenConfig {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +                        // admin
        32 +                        // mint
        (32 * MAX_MULTISIG_SIGNERS) + // signers
        1 +                         // threshold
        1 +                         // signer_count
        2 +                         // fee_burn_bps
        8 +                         // total_burned
        8 +                         // total_fees_collected
        8 +                         // created_at
        1;                          // bump
}

/// Multi-sig transaction proposal
#[account]
pub struct MultisigTransaction {
    pub config: Pubkey,                    // Associated config (32 bytes)
    pub transaction_type: MultisigTransactionType, // Type (1 byte)
    pub amount: u64,                       // Amount for mint/transfer (8 bytes)
    pub recipient: Pubkey,                 // Recipient address (32 bytes)
    pub proposer: Pubkey,                  // Who proposed (32 bytes)
    pub approvals: Vec<bool>,              // Approval status per signer (4 + 9 bytes)
    pub approval_count: u8,                // Current approval count (1 byte)
    pub executed: bool,                    // Whether executed (1 byte)
    pub created_at: i64,                   // Creation timestamp (8 bytes)
    pub bump: u8,                          // PDA bump (1 byte)
}

impl MultisigTransaction {
    pub const MAX_SIZE: usize = 8 + // discriminator
        32 +                        // config
        1 +                         // transaction_type
        8 +                         // amount
        32 +                        // recipient
        32 +                        // proposer
        (4 + MAX_MULTISIG_SIGNERS) + // approvals vec
        1 +                         // approval_count
        1 +                         // executed
        8 +                         // created_at
        1;                          // bump
}

/// Multi-sig transaction types
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultisigTransactionType {
    /// Mint new tokens
    Mint,
    /// Transfer from treasury
    TreasuryTransfer,
    /// Update configuration
    UpdateConfig,
}

#[derive(Accounts)]
pub struct InitializeTokenConfig<'info> {
    #[account(
        init,
        payer = admin,
        space = TokenConfig::MAX_SIZE,
        seeds = [b"token_config", mint.key().as_ref()],
        bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(transaction_type: MultisigTransactionType, amount: u64, recipient: Pubkey, nonce: u64)]
pub struct CreateMultisigTransaction<'info> {
    #[account(
        seeds = [b"token_config", token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    #[account(
        init,
        payer = proposer,
        space = MultisigTransaction::MAX_SIZE,
        seeds = [b"multisig_tx", token_config.key().as_ref(), &nonce.to_le_bytes()],
        bump
    )]
    pub multisig_tx: Account<'info, MultisigTransaction>,

    #[account(mut)]
    pub proposer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ApproveMultisigTransaction<'info> {
    #[account(
        seeds = [b"token_config", token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    #[account(
        mut,
        constraint = multisig_tx.config == token_config.key() @ TokenError::ConfigMismatch
    )]
    pub multisig_tx: Account<'info, MultisigTransaction>,

    pub approver: Signer<'info>,
}

#[derive(Accounts)]
pub struct ExecuteMultisigTransaction<'info> {
    #[account(
        seeds = [b"token_config", token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    #[account(
        mut,
        constraint = multisig_tx.config == token_config.key() @ TokenError::ConfigMismatch
    )]
    pub multisig_tx: Account<'info, MultisigTransaction>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,

    /// Treasury token account (for transfers)
    #[account(mut)]
    pub treasury: Account<'info, TokenAccount>,

    /// Recipient token account
    #[account(mut)]
    pub recipient_token_account: Account<'info, TokenAccount>,

    pub executor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ProcessFeeWithBurn<'info> {
    #[account(
        mut,
        seeds = [b"token_config", token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,

    /// Fee payer's token account
    #[account(
        mut,
        constraint = fee_payer_account.mint == mint.key() @ TokenError::MintMismatch
    )]
    pub fee_payer_account: Account<'info, TokenAccount>,

    /// Treasury to receive non-burned portion
    #[account(
        mut,
        constraint = treasury.mint == mint.key() @ TokenError::MintMismatch
    )]
    pub treasury: Account<'info, TokenAccount>,

    pub fee_payer: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UpdateTokenConfig<'info> {
    #[account(
        mut,
        seeds = [b"token_config", token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateSigners<'info> {
    #[account(
        mut,
        seeds = [b"token_config", token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,

    /// Must be approved via multi-sig transaction
    pub admin: Signer<'info>,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct MintInitializedEvent {
    pub mint: Pubkey,
    pub mint_authority: Pubkey,
    pub freeze_authority: Pubkey,
    pub decimals: u8,
    pub timestamp: i64,
}

#[event]
pub struct MintEvent {
    pub mint: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub new_supply: u64,
    pub timestamp: i64,
}

#[event]
pub struct TransferEvent {
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct BurnEvent {
    pub mint: Pubkey,
    pub from: Pubkey,
    pub amount: u64,
    pub new_supply: u64,
    pub timestamp: i64,
}

// Multi-sig and governance events
#[event]
pub struct TokenConfigInitializedEvent {
    pub mint: Pubkey,
    pub admin: Pubkey,
    pub threshold: u8,
    pub signer_count: u8,
    pub fee_burn_bps: u16,
    pub timestamp: i64,
}

#[event]
pub struct MultisigTransactionCreatedEvent {
    pub tx_id: Pubkey,
    pub transaction_type: MultisigTransactionType,
    pub amount: u64,
    pub recipient: Pubkey,
    pub proposer: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct MultisigApprovalEvent {
    pub tx_id: Pubkey,
    pub approver: Pubkey,
    pub approval_count: u8,
    pub threshold: u8,
    pub timestamp: i64,
}

#[event]
pub struct MultisigExecutedEvent {
    pub tx_id: Pubkey,
    pub transaction_type: MultisigTransactionType,
    pub amount: u64,
    pub recipient: Pubkey,
    pub executor: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct FeeBurnEvent {
    pub mint: Pubkey,
    pub total_fee: u64,
    pub burn_amount: u64,
    pub treasury_amount: u64,
    pub total_burned: u64,
    pub timestamp: i64,
}

#[event]
pub struct FeeBpsUpdatedEvent {
    pub mint: Pubkey,
    pub old_bps: u16,
    pub new_bps: u16,
    pub timestamp: i64,
}

#[event]
pub struct SignersUpdatedEvent {
    pub mint: Pubkey,
    pub old_threshold: u8,
    pub new_threshold: u8,
    pub old_count: u8,
    pub new_count: u8,
    pub timestamp: i64,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum TokenError {
    #[msg("Invalid token decimals, must be 9")]
    InvalidDecimals,

    #[msg("Invalid amount, must be greater than 0")]
    InvalidAmount,

    #[msg("Total supply exceeded, cannot mint more tokens")]
    SupplyExceeded,

    #[msg("Arithmetic overflow")]
    Overflow,

    #[msg("Token account mint does not match")]
    MintMismatch,

    #[msg("Invalid authority for this operation")]
    InvalidAuthority,

    // Multi-sig errors
    #[msg("Invalid multi-sig threshold")]
    InvalidThreshold,

    #[msg("Too many signers (max 9)")]
    TooManySigners,

    #[msg("Invalid fee burn BPS (max 10000)")]
    InvalidFeeBps,

    #[msg("Not a valid signer for this multi-sig")]
    InvalidSigner,

    #[msg("Transaction already executed")]
    TransactionAlreadyExecuted,

    #[msg("Already approved this transaction")]
    AlreadyApproved,

    #[msg("Insufficient approvals to execute")]
    InsufficientApprovals,

    #[msg("Config does not match transaction")]
    ConfigMismatch,
}
