use anyhow::Result;
use colored::Colorize;
use solana_sdk::signature::Signer;
use crate::wallet;

/// Execute claim rewards command
pub async fn execute() -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "Claiming rewards...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());

    // TODO: Call Rewards contract when implemented
    println!();
    println!("{}", "⚠ Rewards contract not yet deployed".yellow());
    println!("{}", "  Available rewards: 0.00 AEGIS".dimmed());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_rewards_command_exists() {
        // Ensures the module compiles
        assert!(true);
    }
}
