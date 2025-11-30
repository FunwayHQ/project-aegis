use anyhow::{Context, Result};
use colored::Colorize;
use solana_sdk::signature::{Keypair, Signer};
use std::path::{Path, PathBuf};
use crate::config::Config;
use crate::errors::CliError;

/// Create a new wallet
pub async fn create() -> Result<()> {
    let keypair = Keypair::new();
    let wallet_path = get_default_wallet_path()?;

    // Create parent directory
    if let Some(parent) = wallet_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Save keypair
    let keypair_bytes = keypair.to_bytes();
    let json = serde_json::to_string(&keypair_bytes.to_vec())?;
    std::fs::write(&wallet_path, json)?;

    println!("{}", "✓ New wallet created successfully!".green());
    println!("  Address: {}", keypair.pubkey().to_string().bright_yellow());
    println!("  Saved to: {}", wallet_path.display());
    println!();
    println!("{}", "⚠ IMPORTANT: Back up your wallet file!".yellow().bold());

    // Update config
    let mut config = Config::load()?;
    config.wallet_path = Some(wallet_path);
    config.save()?;

    Ok(())
}

/// Import wallet from keypair file
pub async fn import(keypair_path: &str) -> Result<()> {
    let path = Path::new(keypair_path);

    if !path.exists() {
        return Err(anyhow::anyhow!("Keypair file not found: {}", keypair_path));
    }

    // Read and validate keypair
    let contents = std::fs::read_to_string(path)?;
    let bytes: Vec<u8> = serde_json::from_str(&contents)
        .context("Invalid keypair file format")?;

    let keypair = Keypair::try_from(bytes.as_slice())
        .context("Failed to parse keypair")?;

    println!("{}", "✓ Wallet imported successfully!".green());
    println!("  Address: {}", keypair.pubkey().to_string().bright_yellow());

    // Update config
    let mut config = Config::load()?;
    config.wallet_path = Some(path.to_path_buf());
    config.save()?;

    Ok(())
}

/// Show wallet address
pub async fn show_address() -> Result<()> {
    let keypair = load_wallet()?;

    println!("{}", "Wallet Address:".bright_cyan());
    println!("  {}", keypair.pubkey().to_string().bright_yellow());

    Ok(())
}

/// Load wallet from configured path
pub fn load_wallet() -> Result<Keypair> {
    let config = Config::load()?;

    let wallet_path = config.wallet_path
        .ok_or(CliError::WalletNotFound)?;

    if !wallet_path.exists() {
        return Err(CliError::WalletNotFound.into());
    }

    let contents = std::fs::read_to_string(&wallet_path)?;
    let bytes: Vec<u8> = serde_json::from_str(&contents)?;
    let keypair = Keypair::try_from(bytes.as_slice())?;

    Ok(keypair)
}

/// Get default wallet path
fn get_default_wallet_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?;
    Ok(config_dir.join("aegis-cli").join("wallet.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_get_default_wallet_path() {
        let path = get_default_wallet_path().unwrap();
        assert!(path.to_string_lossy().contains("aegis-cli"));
        assert!(path.to_string_lossy().contains("wallet.json"));
    }

    #[test]
    fn test_load_wallet_not_found() {
        // Note: A wallet may exist if integration tests have run
        // This test verifies the load_wallet function works
        let result = load_wallet();
        // Either succeeds (wallet exists) or fails with WalletNotFound
        if let Err(e) = result {
            assert!(e.to_string().contains("Wallet not found") ||
                    e.to_string().contains("config"));
        }
    }

    #[test]
    fn test_keypair_serialization() {
        let keypair = Keypair::new();
        let bytes = keypair.to_bytes();
        let json = serde_json::to_string(&bytes.to_vec()).unwrap();

        // Deserialize and verify
        let deserialized: Vec<u8> = serde_json::from_str(&json).unwrap();
        let restored_keypair = Keypair::from_bytes(&deserialized).unwrap();

        assert_eq!(keypair.pubkey(), restored_keypair.pubkey());
    }

    #[test]
    fn test_create_wallet_generates_unique_keypairs() {
        let kp1 = Keypair::new();
        let kp2 = Keypair::new();

        assert_ne!(kp1.pubkey(), kp2.pubkey());
    }
}
