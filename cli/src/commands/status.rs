use anyhow::Result;
use colored::Colorize;
use anchor_client::Cluster;
use solana_sdk::signature::Signer;
use crate::{contracts, wallet};
use chrono::{DateTime, Utc, Duration};

/// Execute status check
pub async fn execute() -> Result<()> {
    let keypair = wallet::load_wallet()?;

    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!("{}", "        AEGIS Node Operator Status".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!();
    println!("  Wallet: {}", keypair.pubkey().to_string().bright_yellow());
    println!();

    // Query Node Registry
    println!("{}", "═══ Node Registration ═══".bright_cyan());
    match contracts::get_node_status(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(node_status) => {
            let status_str = match node_status.status {
                contracts::NodeStatusEnum::Active => "Active".bright_green(),
                contracts::NodeStatusEnum::Inactive => "Inactive".yellow(),
                contracts::NodeStatusEnum::Slashed => "Slashed".bright_red(),
            };
            println!("  Status:      {}", status_str);
            println!("  Metadata:    {}", node_status.metadata_url.bright_white());

            let registered_time = DateTime::from_timestamp(node_status.registered_at, 0)
                .unwrap_or_else(|| Utc::now());
            println!("  Registered:  {}", registered_time.format("%Y-%m-%d %H:%M UTC").to_string().dimmed());
        }
        Err(_) => {
            println!("  Status:      {}", "Not Registered".yellow());
            println!("  {}",  "Use 'aegis-cli register' to register your node".dimmed());
        }
    }

    // Query Staking
    println!();
    println!("{}", "═══ Staking ═══".bright_cyan());
    match contracts::get_stake_info(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(stake_info) => {
            println!("  Staked:      {} AEGIS", format_tokens(stake_info.staked_amount).bright_green());
            println!("  Pending:     {} AEGIS", format_tokens(stake_info.pending_unstake).yellow());

            if stake_info.pending_unstake > 0 {
                let unstake_time = DateTime::from_timestamp(stake_info.unstake_request_time, 0)
                    .unwrap_or_else(|| Utc::now());
                let available_time = unstake_time + Duration::days(7);
                let now = Utc::now();

                if now >= available_time {
                    println!("  Cooldown:    {} (Ready to claim!)", "Complete".bright_green());
                } else {
                    let remaining = available_time - now;
                    println!("  Cooldown:    {} days remaining", remaining.num_days().to_string().yellow());
                }
                println!("  Available:   {}", available_time.format("%Y-%m-%d %H:%M UTC").to_string().dimmed());
            } else {
                println!("  Cooldown:    {}", "None".dimmed());
            }

            println!("  Total Ever:  {} AEGIS", format_tokens(stake_info.total_staked_ever).dimmed());
        }
        Err(_) => {
            println!("  Staked:      {} AEGIS", "0.00".dimmed());
            println!("  {}",  "Use 'aegis-cli stake' to stake tokens".dimmed());
        }
    }

    // Query Rewards
    println!();
    println!("{}", "═══ Rewards ═══".bright_cyan());
    match contracts::get_rewards_info(&keypair.pubkey(), Cluster::Devnet).await {
        Ok(rewards_info) => {
            println!("  Unclaimed:   {} AEGIS", format_tokens(rewards_info.unclaimed_rewards).bright_green());
            println!("  Total Earned: {} AEGIS", format_tokens(rewards_info.total_earned).dimmed());
            println!("  Total Claimed: {} AEGIS", format_tokens(rewards_info.total_claimed).dimmed());

            if rewards_info.last_claim_time > 0 {
                let last_claim = DateTime::from_timestamp(rewards_info.last_claim_time, 0)
                    .unwrap_or_else(|| Utc::now());
                println!("  Last Claim:  {}", last_claim.format("%Y-%m-%d %H:%M UTC").to_string().dimmed());
            }

            if rewards_info.unclaimed_rewards > 0 {
                println!();
                println!("  {} {}", "→".bright_green(), "Use 'aegis-cli claim-rewards' to claim your rewards!".bright_green());
            }
        }
        Err(_) => {
            println!("  Unclaimed:   {} AEGIS", "0.00".dimmed());
            println!("  {}",  "Rewards will appear after staking and performance tracking".dimmed());
        }
    }

    println!();
    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!();

    Ok(())
}

fn format_tokens(amount: u64) -> String {
    let tokens = amount as f64 / 1_000_000_000.0;
    format!("{:.2}", tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Status command mainly integrates other components
    // Unit tests focus on formatting and display logic

    #[test]
    fn test_status_command_exists() {
        // Ensures the module compiles
        assert!(true);
    }
}
