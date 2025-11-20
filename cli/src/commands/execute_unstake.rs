use anyhow::Result;
use colored::Colorize;
use anchor_client::Cluster;
use solana_sdk::signature::Signer;
use crate::{contracts, wallet};
use chrono::{DateTime, Utc, Duration};

/// Execute unstake after cooldown period
pub async fn execute() -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "Executing Unstake...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());
    println!();

    // Check stake info
    println!("{}", "Checking unstake status...".dimmed());

    let stake_info = match contracts::get_stake_info(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(info) => info,
        Err(e) => {
            println!("{}", "❌ Failed to fetch stake information".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Ensure you have requested unstake first".yellow());
            return Err(e);
        }
    };

    // Check if there's a pending unstake
    if stake_info.pending_unstake == 0 {
        println!("{}", "❌ No pending unstake found".bright_red());
        println!();
        println!("  You need to request unstake first:");
        println!("  {}",  "aegis-cli unstake --amount <amount>".bright_white());
        return Ok(());
    }

    let unstake_amount = stake_info.pending_unstake as f64 / 1_000_000_000.0;

    // Check if cooldown period has passed
    let unstake_time = DateTime::from_timestamp(stake_info.unstake_request_time, 0)
        .unwrap_or_else(|| Utc::now());
    let available_time = unstake_time + Duration::days(7);
    let now = Utc::now();

    if now < available_time {
        let remaining = available_time - now;
        println!("{}", "❌ Cooldown period not complete".bright_red());
        println!();
        println!("  Pending Amount:  {} AEGIS", format!("{:.2}", unstake_amount).yellow());
        println!("  Requested:       {}", unstake_time.format("%Y-%m-%d %H:%M UTC").to_string().dimmed());
        println!("  Available:       {}", available_time.format("%Y-%m-%d %H:%M UTC").to_string().bright_white());
        println!("  Remaining:       {} days", remaining.num_days().to_string().yellow());
        println!();
        println!("{}", "Please wait for the cooldown period to complete".yellow());
        return Ok(());
    }

    println!("  ✓ Cooldown period complete!".bright_green());
    println!("  Amount to withdraw: {} AEGIS", format!("{:.2}", unstake_amount).bright_green());
    println!();

    println!("{}", "Sending execute unstake transaction to Solana Devnet...".dimmed());

    // Execute unstake
    match contracts::execute_unstake(&keypair, Cluster::Devnet).await {
        Ok(signature) => {
            println!();
            println!("{}", "✅ Unstake executed successfully!".bright_green());
            println!();
            println!("  Amount:      {} AEGIS", format!("{:.2}", unstake_amount).bright_green());
            println!("  Transaction: {}", signature.bright_yellow());
            println!(
                "  Explorer: {}",
                format!("https://explorer.solana.com/tx/{}?cluster=devnet", signature).bright_blue()
            );
            println!();
            println!("{}", format!("Your {:.2} AEGIS tokens have been returned to your wallet!", unstake_amount).bright_green());
        }
        Err(e) => {
            println!();
            println!("{}", "❌ Execute unstake failed".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Troubleshooting:".bright_yellow());
            println!("  • Ensure you have SOL for transaction fees");
            println!("  • Verify the cooldown period has passed");
            println!("  • Check your pending unstake amount is > 0");
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
    fn test_execute_unstake_command_exists() {
        // Ensures the module compiles
        assert!(true);
    }
}
