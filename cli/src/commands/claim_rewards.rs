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
    fn test_reward_amount_conversion() {
        // Test lamports to AEGIS conversion (9 decimals)
        let lamports: u64 = 5_250_000_000; // 5.25 AEGIS
        let aegis = lamports as f64 / 1_000_000_000.0;
        assert_eq!(format!("{:.2}", aegis), "5.25");
    }

    #[test]
    fn test_reward_amount_edge_cases() {
        let test_cases = vec![
            (0, "0.00"),
            (1_000_000_000, "1.00"),
            (100_000_000_000, "100.00"),
            (1_500_000_000, "1.50"),
            (1, "0.00"), // Less than 1 token rounds to 0.00
        ];

        for (lamports, expected) in test_cases {
            let aegis = lamports as f64 / 1_000_000_000.0;
            let formatted = format!("{:.2}", aegis);
            assert_eq!(formatted, expected, "Failed for {} lamports", lamports);
        }
    }

    #[test]
    fn test_zero_rewards_logic() {
        let unclaimed_rewards: u64 = 0;
        assert_eq!(unclaimed_rewards, 0);

        let has_rewards = unclaimed_rewards > 0;
        assert!(!has_rewards);
    }

    #[test]
    fn test_positive_rewards_logic() {
        let unclaimed_rewards: u64 = 5_250_000_000; // 5.25 AEGIS
        assert!(unclaimed_rewards > 0);

        let has_rewards = unclaimed_rewards > 0;
        assert!(has_rewards);
    }

    #[test]
    fn test_reward_amount_precision() {
        // Test various reward amounts
        let amounts = vec![
            1_000_000_000,      // 1.00
            1_500_000_000,      // 1.50
            123_456_789,        // 0.12
            999_999_999,        // 1.00 (rounds)
            1_234_567_890,      // 1.23
        ];

        for amount in amounts {
            let aegis = amount as f64 / 1_000_000_000.0;
            let formatted = format!("{:.2}", aegis);

            // Should always have 2 decimal places
            assert!(formatted.contains('.'));
            let parts: Vec<&str> = formatted.split('.').collect();
            assert_eq!(parts[1].len(), 2);
        }
    }

    #[test]
    fn test_total_earned_display() {
        let total_earned: u64 = 25_000_000_000; // 25.00 AEGIS
        let aegis = total_earned as f64 / 1_000_000_000.0;
        assert_eq!(format!("{:.2}", aegis), "25.00");
    }

    #[test]
    fn test_total_claimed_display() {
        let total_claimed: u64 = 19_750_000_000; // 19.75 AEGIS
        let aegis = total_claimed as f64 / 1_000_000_000.0;
        assert_eq!(format!("{:.2}", aegis), "19.75");
    }

    #[test]
    fn test_unclaimed_calculation() {
        let total_earned: u64 = 25_000_000_000;
        let total_claimed: u64 = 19_750_000_000;
        let unclaimed = total_earned - total_claimed;

        assert_eq!(unclaimed, 5_250_000_000); // 5.25 AEGIS

        let aegis = unclaimed as f64 / 1_000_000_000.0;
        assert_eq!(format!("{:.2}", aegis), "5.25");
    }

    #[test]
    fn test_large_reward_amounts() {
        let large_amount: u64 = 1_000_000_000_000_000_000; // 1 billion AEGIS
        let aegis = large_amount as f64 / 1_000_000_000.0;
        assert_eq!(aegis, 1_000_000_000.0);
    }

    #[test]
    fn test_reward_amount_overflow_safety() {
        // Test near u64 max
        let near_max: u64 = u64::MAX - 1_000_000_000;
        let aegis = near_max as f64 / 1_000_000_000.0;

        // Should not panic
        assert!(aegis > 0.0);
    }
}
