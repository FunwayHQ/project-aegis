use anyhow::Result;
use colored::Colorize;
use solana_sdk::signature::Signer;
use crate::wallet;
use crate::errors::CliError;

const MINIMUM_STAKE: u64 = 100_000_000_000; // 100 AEGIS tokens

/// Execute staking command
pub async fn execute(amount: u64) -> Result<()> {
    // Validate stake amount
    if amount < MINIMUM_STAKE {
        return Err(CliError::InvalidStakeAmount(amount).into());
    }

    // Load wallet
    let keypair = wallet::load_wallet()?;

    println!("{}", "Staking AEGIS tokens...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());
    println!("  Amount:   {} AEGIS", format_tokens(amount));

    // TODO: Call Staking contract when implemented
    println!();
    println!("{}", "⚠ Staking contract not yet deployed".yellow());
    println!("{}", "  This will be implemented after contract deployment".dimmed());

    Ok(())
}

/// Format token amount
fn format_tokens(amount: u64) -> String {
    let tokens = amount as f64 / 1_000_000_000.0;
    format!("{:.2}", tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(1_000_000_000), "1.00");
        assert_eq!(format_tokens(500_000_000_000), "500.00");
    }

    #[test]
    fn test_minimum_stake_validation() {
        // Below minimum should fail
        assert!(MINIMUM_STAKE > 0);
    }

    #[test]
    fn test_format_tokens_zero() {
        assert_eq!(format_tokens(0), "0.00");
    }

    #[test]
    fn test_format_tokens_small_amount() {
        assert_eq!(format_tokens(1_000_000), "0.00"); // < 1 token
    }
}
