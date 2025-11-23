//! AEGIS CDN - Content Publisher CLI
//!
//! Command-line tool for deploying and managing content on the AEGIS decentralized CDN.
//!
//! ## Commands
//! - `init`: Initialize a new CDN project
//! - `upload`: Upload static content to IPFS
//! - `deploy`: Deploy with routing configuration
//! - `status`: Check deployment status and metrics
//! - `config`: Manage CDN configuration

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

mod cdn_commands;
use cdn_commands::*;

#[derive(Parser)]
#[command(name = "aegis-cdn")]
#[command(about = "AEGIS Content Publisher CLI - Deploy websites to decentralized CDN", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new AEGIS CDN project
    Init {
        /// Project name
        name: String,

        /// Project directory (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Upload static content to IPFS
    Upload {
        /// Directory or file to upload
        source: PathBuf,

        /// Project name (from aegis-cdn.yaml)
        #[arg(short, long)]
        project: Option<String>,

        /// Pin content to prevent garbage collection
        #[arg(short = 'P', long, default_value = "true")]
        pin: bool,
    },

    /// Deploy content with routing configuration
    Deploy {
        /// Source directory to deploy
        source: PathBuf,

        /// Configuration file
        #[arg(short, long, default_value = "aegis-cdn.yaml")]
        config: PathBuf,

        /// Environment (production, staging, dev)
        #[arg(short, long, default_value = "production")]
        env: String,
    },

    /// Check deployment status and metrics
    Status {
        /// Project name
        project: String,

        /// Show detailed metrics
        #[arg(short, long)]
        detailed: bool,
    },

    /// Manage CDN configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// List all deployments
    List {
        /// Show only active deployments
        #[arg(short, long)]
        active: bool,
    },

    /// Remove a deployment
    Remove {
        /// Project name to remove
        project: String,

        /// Force removal without confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show {
        /// Project name
        project: String,
    },

    /// Set a configuration value
    Set {
        /// Project name
        project: String,

        /// Configuration key
        key: String,

        /// Configuration value
        value: String,
    },

    /// Generate default configuration file
    Generate {
        /// Output file
        #[arg(short, long, default_value = "aegis-cdn.yaml")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    println!("{}", "🛡️  AEGIS CDN - Decentralized Content Delivery".bright_cyan().bold());
    println!();

    match cli.command {
        Commands::Init { name, path } => {
            init_project(&name, path.as_deref()).await?;
        }

        Commands::Upload { source, project, pin } => {
            upload_content(&source, project.as_deref(), pin).await?;
        }

        Commands::Deploy { source, config, env } => {
            deploy_project(&source, &config, &env).await?;
        }

        Commands::Status { project, detailed } => {
            show_status(&project, detailed).await?;
        }

        Commands::Config { action } => match action {
            ConfigAction::Show { project } => {
                show_config(&project).await?;
            }
            ConfigAction::Set { project, key, value } => {
                set_config(&project, &key, &value).await?;
            }
            ConfigAction::Generate { output } => {
                generate_config(&output).await?;
            }
        },

        Commands::List { active } => {
            list_deployments(active).await?;
        }

        Commands::Remove { project, force } => {
            remove_deployment(&project, force).await?;
        }
    }

    Ok(())
}
