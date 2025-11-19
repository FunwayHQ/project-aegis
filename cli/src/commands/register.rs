use anyhow::Result;
use colored::Colorize;
use solana_sdk::signature::Signer;
use crate::wallet;
use crate::errors::CliError;

const MINIMUM_STAKE: u64 = 100_000_000_000; // 100 AEGIS tokens (with 9 decimals)

/// Execute node registration
pub async fn execute(metadata_url: String, stake: Option<u64>) -> Result<()> {
    // Validate inputs
    validate_metadata_url(&metadata_url)?;

    if let Some(stake_amount) = stake {
        validate_stake_amount(stake_amount)?;
    }

    // Load wallet
    let keypair = wallet::load_wallet()?;

    println!("{}", "Registering node...".bright_cyan());
    println!("  Operator: {}", keypair.pubkey().to_string().bright_yellow());
    println!("  Metadata: {}", metadata_url);

    if let Some(stake_amount) = stake {
        println!("  Initial Stake: {} AEGIS", format_tokens(stake_amount));
    }

    // TODO: Call Node Registry contract when implemented
    println!();
    println!("{}", "⚠ Node Registry contract not yet deployed".yellow());
    println!("{}", "  This will be implemented after contract deployment".dimmed());

    Ok(())
}

/// Validate metadata URL (should be IPFS CID)
fn validate_metadata_url(url: &str) -> Result<()> {
    // Basic IPFS CID validation
    if !url.starts_with("Qm") && !url.starts_with("bafy") {
        return Err(CliError::InvalidMetadataUrl(url.to_string()).into());
    }

    if url.len() < 40 {
        return Err(CliError::InvalidMetadataUrl("CID too short".to_string()).into());
    }

    Ok(())
}

/// Validate stake amount
fn validate_stake_amount(amount: u64) -> Result<()> {
    if amount < MINIMUM_STAKE {
        return Err(CliError::InvalidStakeAmount(amount).into());
    }

    Ok(())
}

/// Format token amount (divide by 10^9 for display)
fn format_tokens(amount: u64) -> String {
    let tokens = amount as f64 / 1_000_000_000.0;
    format!("{:.2}", tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_metadata_url_valid_qm() {
        let result = validate_metadata_url("QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_metadata_url_valid_bafy() {
        let result = validate_metadata_url("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_metadata_url_too_short() {
        let result = validate_metadata_url("Qm123");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_metadata_url_invalid_prefix() {
        let result = validate_metadata_url("http://example.com/metadata");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_stake_amount_valid() {
        let result = validate_stake_amount(MINIMUM_STAKE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_stake_amount_too_low() {
        let result = validate_stake_amount(MINIMUM_STAKE - 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_stake_amount_zero() {
        let result = validate_stake_amount(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(1_000_000_000), "1.00");
        assert_eq!(format_tokens(100_000_000_000), "100.00");
        assert_eq!(format_tokens(1_500_000_000), "1.50");
    }

    #[test]
    fn test_format_tokens_large_amount() {
        let billion = 1_000_000_000_000_000_000;
        assert_eq!(format_tokens(billion), "1000000000.00");
    }

    #[test]
    fn test_minimum_stake_constant() {
        assert_eq!(MINIMUM_STAKE, 100_000_000_000);
    }
}
