use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use colored::Colorize;
use crate::errors::CliError;

/// Configuration for AEGIS CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub cluster: String,
    pub wallet_path: Option<PathBuf>,
    pub rpc_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cluster: "devnet".to_string(),
            wallet_path: None,
            rpc_url: "https://api.devnet.solana.com".to_string(),
        }
    }
}

impl Config {
    /// Get config file path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?;
        Ok(config_dir.join("aegis-cli").join("config.toml"))
    }

    /// Load config from file
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            // Create default config
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let contents = std::fs::read_to_string(&path)
            .context("Failed to read config file")?;

        let config: Config = toml::from_str(&contents)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;

        Ok(())
    }

    /// Update cluster configuration
    pub fn set_cluster(&mut self, cluster: &str) -> Result<()> {
        let rpc_url = match cluster {
            "devnet" => "https://api.devnet.solana.com",
            "mainnet-beta" => "https://api.mainnet-beta.solana.com",
            _ => return Err(CliError::InvalidCluster(cluster.to_string()).into()),
        };

        self.cluster = cluster.to_string();
        self.rpc_url = rpc_url.to_string();
        self.save()?;

        Ok(())
    }
}

/// Set cluster configuration
pub fn set_cluster(cluster: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.set_cluster(cluster)?;

    println!("{}", format!("✓ Cluster set to: {}", cluster).green());
    println!("  RPC URL: {}", config.rpc_url);

    Ok(())
}

/// Show current configuration
pub fn show() -> Result<()> {
    let config = Config::load()?;

    println!("{}", "AEGIS CLI Configuration".bright_cyan().bold());
    println!("  Cluster:      {}", config.cluster.bright_yellow());
    println!("  RPC URL:      {}", config.rpc_url);
    println!("  Wallet Path:  {}",
        config.wallet_path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "Not set".to_string())
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.cluster, "devnet");
        assert_eq!(config.rpc_url, "https://api.devnet.solana.com");
        assert!(config.wallet_path.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();

        assert!(toml_str.contains("cluster"));
        assert!(toml_str.contains("devnet"));
        assert!(toml_str.contains("rpc_url"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            cluster = "devnet"
            rpc_url = "https://api.devnet.solana.com"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.cluster, "devnet");
    }

    #[test]
    fn test_set_cluster_devnet() {
        let mut config = Config::default();
        config.set_cluster("devnet").unwrap();

        assert_eq!(config.cluster, "devnet");
        assert_eq!(config.rpc_url, "https://api.devnet.solana.com");
    }

    #[test]
    fn test_set_cluster_mainnet() {
        let mut config = Config::default();
        config.set_cluster("mainnet-beta").unwrap();

        assert_eq!(config.cluster, "mainnet-beta");
        assert_eq!(config.rpc_url, "https://api.mainnet-beta.solana.com");
    }

    #[test]
    fn test_set_cluster_invalid() {
        let mut config = Config::default();
        let result = config.set_cluster("invalid");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid cluster"));
    }

    #[test]
    fn test_config_roundtrip() {
        let mut config = Config::default();
        config.cluster = "mainnet-beta".to_string();
        config.rpc_url = "https://api.mainnet-beta.solana.com".to_string();

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.cluster, deserialized.cluster);
        assert_eq!(config.rpc_url, deserialized.rpc_url);
    }
}
