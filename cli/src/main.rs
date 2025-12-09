mod commands;
mod config;
mod wallet;
mod errors;
mod contracts;

use clap::{Parser, Subcommand};
use colored::Colorize;
use anyhow::Result;

#[derive(Parser)]
#[command(name = "aegis-cli")]
#[command(author = "AEGIS Team")]
#[command(version = "0.1.0")]
#[command(about = "CLI tool for AEGIS node operators", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new node on the AEGIS network
    Register {
        /// IPFS CID containing node metadata
        #[arg(long)]
        metadata_url: String,

        /// Initial stake amount in AEGIS tokens
        #[arg(long)]
        stake: Option<u64>,
    },

    /// Stake AEGIS tokens for your node
    Stake {
        /// Amount of AEGIS tokens to stake
        #[arg(long)]
        amount: u64,
    },

    /// Unstake AEGIS tokens (with cooldown period)
    Unstake {
        /// Amount to unstake (optional, defaults to all)
        #[arg(long)]
        amount: Option<u64>,
    },

    /// Execute unstake after cooldown period completes
    ExecuteUnstake,

    /// Check node and wallet status
    Status,

    /// Check AEGIS token balance
    Balance,

    /// Claim accumulated rewards
    ClaimRewards,

    /// Display real-time node metrics
    Metrics {
        /// Node URL (defaults to http://127.0.0.1:8080)
        #[arg(long)]
        node_url: Option<String>,
    },

    /// Wallet management commands
    Wallet {
        #[command(subcommand)]
        action: WalletCommands,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },

    /// DNS zone and record management
    Dns {
        #[command(subcommand)]
        action: commands::dns::DnsCommands,
    },
}

#[derive(Subcommand)]
enum WalletCommands {
    /// Create a new wallet
    Create,

    /// Import wallet from keypair file
    Import {
        /// Path to keypair JSON file
        #[arg(long)]
        keypair: String,
    },

    /// Show wallet address
    Address,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Set Solana cluster (devnet/mainnet-beta)
    SetCluster {
        /// Cluster name
        cluster: String,
    },

    /// Show current configuration
    Show,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Print banner
    println!("{}", "╔════════════════════════════════════════════╗".bright_cyan());
    println!("{}", "║     AEGIS CLI - Node Operator Tool        ║".bright_cyan());
    println!("{}", "║       Sprint 5: Metrics & Monitoring       ║".bright_cyan());
    println!("{}", "╚════════════════════════════════════════════╝".bright_cyan());
    println!();

    let cli = Cli::parse();

    match cli.command {
        Commands::Register { metadata_url, stake } => {
            commands::register::execute(metadata_url, stake).await?;
        }
        Commands::Stake { amount } => {
            commands::stake::execute(amount).await?;
        }
        Commands::Unstake { amount } => {
            commands::unstake::execute(amount).await?;
        }
        Commands::ExecuteUnstake => {
            commands::execute_unstake::execute().await?;
        }
        Commands::Status => {
            commands::status::execute().await?;
        }
        Commands::Balance => {
            commands::balance::execute().await?;
        }
        Commands::ClaimRewards => {
            commands::claim_rewards::execute().await?;
        }
        Commands::Metrics { node_url } => {
            commands::metrics::execute(node_url).await?;
        }
        Commands::Wallet { action } => match action {
            WalletCommands::Create => wallet::create().await?,
            WalletCommands::Import { keypair } => wallet::import(&keypair).await?,
            WalletCommands::Address => wallet::show_address().await?,
        },
        Commands::Config { action } => match action {
            ConfigCommands::SetCluster { cluster } => config::set_cluster(&cluster)?,
            ConfigCommands::Show => config::show()?,
        },
        Commands::Dns { action } => {
            commands::dns::execute(action).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_verification() {
        // Verifies that the CLI structure is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_cli_has_version() {
        let cmd = Cli::command();
        assert!(cmd.get_version().is_some());
        assert_eq!(cmd.get_version().unwrap(), "0.1.0");
    }

    #[test]
    fn test_cli_has_about() {
        let cmd = Cli::command();
        assert!(cmd.get_about().is_some());
    }
}
