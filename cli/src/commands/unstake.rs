use anyhow::Result;
use colored::Colorize;
use anchor_client::Cluster;
use solana_sdk::signature::Signer;
use crate::{contracts, wallet};

const COOLDOWN_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days

/// Execute unstaking command
pub async fn execute(amount: Option<u64>) -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "Requesting unstake...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());

    // Get current stake info
    println!();
    println!("{}", "Fetching stake information...".dimmed());

    let stake_info = match contracts::get_stake_info(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(info) => info,
        Err(e) => {
            println!("{}", "❌ Failed to fetch stake information".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Ensure you have staked tokens first".yellow());
            return Err(e);
        }
    };

    let unstake_amount = amount.unwrap_or(stake_info.staked_amount);

    if unstake_amount == 0 {
        println!("{}", "❌ No tokens to unstake".bright_red());
        return Ok(());
    }

    if unstake_amount > stake_info.staked_amount {
        println!("{}", "❌ Insufficient staked amount".bright_red());
        println!("  Requested: {} AEGIS", format_tokens(unstake_amount));
        println!("  Available: {} AEGIS", format_tokens(stake_info.staked_amount));
        return Ok(());
    }

    println!("  Amount:   {} AEGIS", format_tokens(unstake_amount));
    println!("  Cooldown: {} days", COOLDOWN_PERIOD_SECONDS / (24 * 60 * 60));

    println!();
    println!("{}", "Sending unstake request to Solana Devnet...".dimmed());

    // Call the Staking contract
    match contracts::request_unstake(&keypair, unstake_amount, Cluster::Devnet).await {
        Ok(signature) => {
            println!();
            println!("{}", "✅ Unstake request submitted!".bright_green());
            println!();
            println!("  Transaction: {}", signature.bright_yellow());
            println!(
                "  Explorer: {}",
                format!("https://explorer.solana.com/tx/{}?cluster=devnet", signature).bright_blue()
            );
            println!();
            println!("{}", format!("Unstaking {} AEGIS tokens", format_tokens(unstake_amount)).bright_green());
            println!();
            println!("{}", "⏳ 7-day cooldown period has started".yellow());
            println!("{}", "   You can execute the unstake after the cooldown period".dimmed());
            println!("{}", "   Command: aegis-cli execute-unstake".dimmed());
        }
        Err(e) => {
            println!();
            println!("{}", "❌ Unstake request failed".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Troubleshooting:".bright_yellow());
            println!("  • Ensure you have SOL for transaction fees");
            println!("  • Check you have staked tokens");
            println!("  • Verify you're connected to Solana Devnet");
            return Err(e);
        }
    }

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
