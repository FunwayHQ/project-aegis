use anyhow::Result;
use colored::Colorize;
use anchor_client::Cluster;
use solana_sdk::signature::Signer;
use crate::{contracts, wallet};

/// Execute claim rewards command
pub async fn execute() -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "Claiming AEGIS Rewards...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());
    println!();

    // First, check rewards info
    println!("{}", "Checking rewards balance...".dimmed());

    let rewards_info = match contracts::get_rewards_info(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(info) => info,
        Err(e) => {
            println!("{}", "❌ Failed to fetch rewards information".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Troubleshooting:".bright_yellow());
            println!("  • Ensure you are registered and staking");
            println!("  • Verify your node has recorded performance");
            println!("  • Check you're connected to Solana Devnet");
            return Err(e);
        }
    };

    if rewards_info.unclaimed_rewards == 0 {
        println!("{}", "No rewards available to claim".yellow());
        println!();
        println!("  Total Earned:  {:.2} AEGIS", rewards_info.total_earned as f64 / 1_000_000_000.0);
        println!("  Total Claimed: {:.2} AEGIS", rewards_info.total_claimed as f64 / 1_000_000_000.0);
        println!();
        println!("{}", "Keep your node running to earn more rewards!".dimmed());
        return Ok(());
    }

    let reward_amount = rewards_info.unclaimed_rewards as f64 / 1_000_000_000.0;
    println!("  Unclaimed: {} AEGIS", format!("{:.2}", reward_amount).bright_green());
    println!();

    println!("{}", "Sending claim transaction to Solana Devnet...".dimmed());

    // Claim rewards
    match contracts::claim_rewards(&keypair, Cluster::Devnet).await {
        Ok(signature) => {
            println!();
            println!("{}", "✅ Rewards claimed successfully!".bright_green());
            println!();
            println!("  Amount:      {} AEGIS", format!("{:.2}", reward_amount).bright_green());
            println!("  Transaction: {}", signature.bright_yellow());
            println!(
                "  Explorer: {}",
                format!("https://explorer.solana.com/tx/{}?cluster=devnet", signature).bright_blue()
            );
            println!();
            println!("{}", format!("You received {:.2} AEGIS tokens!", reward_amount).bright_green());
        }
        Err(e) => {
            println!();
            println!("{}", "❌ Claim failed".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Troubleshooting:".bright_yellow());
            println!("  • Ensure you have SOL for transaction fees");
            println!("  • Verify your operator rewards account is initialized");
            println!("  • Check the reward pool has sufficient funds");
            println!("  • Ensure you're connected to Solana Devnet");
            return Err(e);
        }
    }

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
