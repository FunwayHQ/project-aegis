use anyhow::Result;
use colored::Colorize;
use solana_sdk::signature::Signer;
use crate::wallet;

const COOLDOWN_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days

/// Execute unstaking command
pub async fn execute(amount: Option<u64>) -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "Unstaking AEGIS tokens...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());

    if let Some(amt) = amount {
        println!("  Amount:   {} AEGIS", format_tokens(amt));
    } else {
        println!("  Amount:   All staked tokens");
    }

    println!("  Cooldown: {} days", COOLDOWN_PERIOD_SECONDS / (24 * 60 * 60));

    // TODO: Call Staking contract when implemented
    println!();
    println!("{}", "⚠ Staking contract not yet deployed".yellow());
    println!("{}", "  Tokens will be available after 7-day cooldown".dimmed());

    Ok(())
}

fn format_tokens(amount: u64) -> String {
    let tokens = amount as f64 / 1_000_000_000.0;
    format!("{:.2}", tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooldown_period_is_7_days() {
        assert_eq!(COOLDOWN_PERIOD_SECONDS, 604800);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(1_000_000_000), "1.00");
    }
}
