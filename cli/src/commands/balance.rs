use anyhow::Result;
use colored::Colorize;
use anchor_client::Cluster;
use solana_sdk::signature::Signer;
use crate::{contracts, wallet};

/// Execute balance check
pub async fn execute() -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!("{}", "        Wallet Balance".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!();
    println!("  Wallet: {}", keypair.pubkey().to_string().bright_yellow());
    println!();

    println!("{}", "Fetching balances from Solana Devnet...".dimmed());
    println!();

    // Get AEGIS token balance
    let aegis_balance = match contracts::get_token_balance(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(balance) => balance,
        Err(e) => {
            println!("{}", "⚠ Failed to fetch AEGIS balance".yellow());
            println!("  Error: {}", e);
            0.0
        }
    };

    // Get SOL balance
    let sol_balance = match contracts::get_sol_balance(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(balance) => balance,
        Err(e) => {
            println!("{}", "⚠ Failed to fetch SOL balance".yellow());
            println!("  Error: {}", e);
            0.0
        }
    };

    println!("{}", "═══ Balances ═══".bright_cyan());

    let aegis_display = if aegis_balance > 0.0 {
        format!("{:.2}", aegis_balance).bright_green()
    } else {
        format!("{:.2}", aegis_balance).dimmed()
    };
    println!("  AEGIS:  {} AEGIS", aegis_display);

    let sol_display = if sol_balance > 0.0 {
        format!("{:.4}", sol_balance).bright_white()
    } else {
        format!("{:.4}", sol_balance).yellow()
    };
    println!("  SOL:    {} SOL", sol_display);

    println!();

    // Show warnings if balances are low
    if sol_balance < 0.01 {
        println!("{}", "⚠ Warning: Low SOL balance".yellow());
        println!("  {}",  "You need SOL for transaction fees".dimmed());
        println!("  {}",  "Get SOL from faucet: solana airdrop 1".dimmed());
    }

    if aegis_balance < 100.0 {
        println!("{}", "⚠ Notice: Insufficient AEGIS for staking".yellow());
        println!("  {}",  "Minimum stake requirement: 100 AEGIS".dimmed());
    }

    println!();
    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_command_exists() {
        // Ensures the module compiles
        assert!(true);
    }
}
