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
    fn test_cooldown_period_duration() {
        let cooldown_days = 7;
        let cooldown_seconds = cooldown_days * 24 * 60 * 60;
        assert_eq!(cooldown_seconds, 604800);
    }

    #[test]
    fn test_unstake_amount_conversion() {
        let lamports: u64 = 100_000_000_000; // 100 AEGIS
        let aegis = lamports as f64 / 1_000_000_000.0;
        assert_eq!(format!("{:.2}", aegis), "100.00");
    }

    #[test]
    fn test_pending_unstake_zero_check() {
        let pending_unstake: u64 = 0;
        assert_eq!(pending_unstake, 0);

        let has_pending = pending_unstake > 0;
        assert!(!has_pending);
    }

    #[test]
    fn test_pending_unstake_positive() {
        let pending_unstake: u64 = 100_000_000_000;
        assert!(pending_unstake > 0);
    }

    #[test]
    fn test_cooldown_time_calculation() {
        use chrono::Duration;

        let request_time = Utc::now();
        let available_time = request_time + Duration::days(7);

        let duration = available_time - request_time;
        assert_eq!(duration.num_days(), 7);
    }

    #[test]
    fn test_cooldown_not_complete_logic() {
        use chrono::Duration;

        let request_time = Utc::now() - Duration::days(3); // 3 days ago
        let available_time = request_time + Duration::days(7);
        let now = Utc::now();

        let cooldown_complete = now >= available_time;
        assert!(!cooldown_complete); // Only 3 days passed, need 7

        let remaining = available_time - now;
        assert_eq!(remaining.num_days(), 4); // 4 days remaining
    }

    #[test]
    fn test_cooldown_complete_logic() {
        use chrono::Duration;

        let request_time = Utc::now() - Duration::days(8); // 8 days ago
        let available_time = request_time + Duration::days(7);
        let now = Utc::now();

        let cooldown_complete = now >= available_time;
        assert!(cooldown_complete); // 8 days passed, more than 7
    }

    #[test]
    fn test_cooldown_exactly_7_days() {
        use chrono::Duration;

        let request_time = Utc::now() - Duration::days(7);
        let available_time = request_time + Duration::days(7);
        let now = Utc::now();

        let cooldown_complete = now >= available_time;
        assert!(cooldown_complete); // Exactly 7 days
    }

    #[test]
    fn test_timestamp_formatting() {
        let timestamp = 1700491530_i64;
        let dt = DateTime::from_timestamp(timestamp, 0);
        assert!(dt.is_some());

        let formatted = dt.unwrap().format("%Y-%m-%d %H:%M UTC").to_string();
        assert!(formatted.contains("2023")); // Timestamp is from 2023
        assert!(formatted.contains("UTC"));
    }

    #[test]
    fn test_remaining_days_calculation() {
        use chrono::Duration;

        let now = Utc::now();
        let future = now + Duration::days(5);

        let remaining = future - now;
        assert_eq!(remaining.num_days(), 5);
    }

    #[test]
    fn test_unstake_amount_display_formatting() {
        let amounts = vec![
            (100_000_000_000, "100.00"),
            (50_000_000_000, "50.00"),
            (1_000_000_000, "1.00"),
            (500_000_000, "0.50"),
        ];

        for (lamports, expected) in amounts {
            let aegis = lamports as f64 / 1_000_000_000.0;
            let formatted = format!("{:.2}", aegis);
            assert_eq!(formatted, expected);
        }
    }

    #[test]
    fn test_cooldown_period_boundaries() {
        use chrono::Duration;

        // Test various cooldown periods
        let test_cases = vec![
            (0, 7, false),  // Just started
            (3, 7, false),  // 3 days in
            (6, 7, false),  // 6 days in
            (7, 7, true),   // Exactly 7 days
            (8, 7, true),   // 8 days (past cooldown)
        ];

        for (days_passed, cooldown_days, should_be_complete) in test_cases {
            let request_time = Utc::now() - Duration::days(days_passed);
            let available_time = request_time + Duration::days(cooldown_days);
            let now = Utc::now();

            let is_complete = now >= available_time;
            assert_eq!(
                is_complete, should_be_complete,
                "Failed for {} days passed with {} day cooldown",
                days_passed, cooldown_days
            );
        }
    }
}
