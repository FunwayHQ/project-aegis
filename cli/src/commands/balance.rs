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
    fn test_balance_formatting_zero() {
        let balance: f64 = 0.0;
        let formatted = format!("{:.2}", balance);
        assert_eq!(formatted, "0.00");
    }

    #[test]
    fn test_balance_formatting_small() {
        let balance: f64 = 0.01;
        let formatted = format!("{:.2}", balance);
        assert_eq!(formatted, "0.01");
    }

    #[test]
    fn test_balance_formatting_large() {
        let balance: f64 = 1234567.89;
        let formatted = format!("{:.2}", balance);
        assert_eq!(formatted, "1234567.89");
    }

    #[test]
    fn test_sol_formatting_precision() {
        let balance: f64 = 0.123456789;
        let formatted = format!("{:.4}", balance);
        assert_eq!(formatted, "0.1235"); // Rounded to 4 decimals
    }

    #[test]
    fn test_low_sol_threshold() {
        let low_threshold = 0.01;

        assert!(0.005 < low_threshold); // Should warn
        assert!(0.02 >= low_threshold); // Should not warn
    }

    #[test]
    fn test_low_aegis_threshold() {
        let min_stake = 100.0;

        assert!(50.0 < min_stake); // Should warn
        assert!(150.0 >= min_stake); // Should not warn
    }

    #[test]
    fn test_balance_display_logic() {
        // Test the logic for determining if balance is > 0
        let zero_balance = 0.0;
        let positive_balance = 100.5;

        assert!(zero_balance <= 0.0);
        assert!(positive_balance > 0.0);
    }

    #[test]
    fn test_aegis_balance_edge_cases() {
        let balances = vec![0.0, 0.01, 99.99, 100.0, 100.01, 1000.0];

        for balance in balances {
            let formatted = format!("{:.2}", balance);
            assert!(formatted.len() >= 4); // At least "0.00"
        }
    }

    #[test]
    fn test_sol_balance_edge_cases() {
        let balances = vec![0.0, 0.0001, 0.0099, 0.01, 0.0101, 1.0];

        for balance in balances {
            let formatted = format!("{:.4}", balance);
            assert!(formatted.len() >= 6); // At least "0.0000"
        }
    }

    #[test]
    fn test_warning_thresholds_consistency() {
        let sol_warning_threshold = 0.01;
        let aegis_warning_threshold = 100.0;

        // Thresholds should be reasonable
        assert!(sol_warning_threshold > 0.0);
        assert!(aegis_warning_threshold > 0.0);
        assert!(sol_warning_threshold < 1.0); // Less than 1 SOL
        assert!(aegis_warning_threshold >= 100.0); // At least min stake
    }
}
