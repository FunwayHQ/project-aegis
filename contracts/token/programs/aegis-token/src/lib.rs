use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("D4URFrSz1UuoC1cKSpp8SiX2E9HeDdY8EvkXHUYHmM4v");

/// Total supply of $AEGIS tokens (1 billion with 9 decimals)
pub const TOTAL_SUPPLY: u64 = 1_000_000_000_000_000_000;

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
}
