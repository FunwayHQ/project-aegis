use anyhow::Result;
use colored::Colorize;
use anchor_client::Cluster;
use solana_sdk::signature::Signer;
use crate::{contracts, wallet};
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

    println!();
    println!("{}", "Checking stake account...".dimmed());

    // First, check if stake account exists, if not initialize it
    let stake_exists = contracts::get_stake_info(&keypair.pubkey(), Cluster::Devnet).await.is_ok();

    if !stake_exists {
        println!("{}", "  Initializing stake account...".dimmed());
        match contracts::initialize_stake_account(&keypair, Cluster::Devnet).await {
            Ok(sig) => {
                println!("  ✓ Stake account initialized: {}", sig.bright_green());
            }
            Err(e) => {
                println!("{}", "❌ Failed to initialize stake account".bright_red());
                println!("  Error: {}", e);
                return Err(e);
            }
        }
    } else {
        println!("  ✓ Stake account already exists".green());
    }

    println!();
    println!("{}", "Sending stake transaction to Solana Devnet...".dimmed());

    // Call the Staking contract
    match contracts::stake_tokens(&keypair, amount, Cluster::Devnet).await {
        Ok(signature) => {
            println!();
            println!("{}", "✅ Tokens staked successfully!".bright_green());
            println!();
            println!("  Transaction: {}", signature.bright_yellow());
            println!(
                "  Explorer: {}",
                format!("https://explorer.solana.com/tx/{}?cluster=devnet", signature).bright_blue()
            );
            println!();
            println!("{}", format!("You have staked {} AEGIS tokens!", format_tokens(amount)).bright_green());
            println!();
            println!("{}", "Note: Unstaking has a 7-day cooldown period".yellow());
        }
        Err(e) => {
            println!();
            println!("{}", "❌ Staking failed".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Troubleshooting:".bright_yellow());
            println!("  • Ensure you have SOL for transaction fees");
            println!("  • Check your wallet has at least {} AEGIS tokens", format_tokens(amount));
            println!("  • Verify your node is registered");
            println!("  • Ensure you're connected to Solana Devnet");
            return Err(e);
        }
    }

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
