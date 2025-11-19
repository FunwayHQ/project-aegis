use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("Wallet not found. Run 'aegis-cli wallet create' first")]
    WalletNotFound,

    #[error("Invalid cluster name: {0}. Valid options: devnet, mainnet-beta")]
    InvalidCluster(String),

    #[error("Invalid stake amount: {0}. Must be greater than minimum stake")]
    InvalidStakeAmount(u64),

    #[error("Node not registered. Run 'aegis-cli register' first")]
    NodeNotRegistered,

    #[error("Insufficient balance: need {need} AEGIS, have {have}")]
    InsufficientBalance { need: u64, have: u64 },

    #[error("Invalid metadata URL: {0}")]
    InvalidMetadataUrl(String),

    #[error("Cooldown period active. {0} seconds remaining")]
    CooldownActive(u64),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Solana RPC error: {0}")]
    SolanaRpcError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_not_found_error() {
        let err = CliError::WalletNotFound;
        assert_eq!(
            err.to_string(),
            "Wallet not found. Run 'aegis-cli wallet create' first"
        );
    }

    #[test]
    fn test_invalid_cluster_error() {
        let err = CliError::InvalidCluster("testnet".to_string());
        assert!(err.to_string().contains("testnet"));
        assert!(err.to_string().contains("devnet"));
    }

    #[test]
    fn test_insufficient_balance_error() {
        let err = CliError::InsufficientBalance { need: 1000, have: 500 };
        assert!(err.to_string().contains("1000"));
        assert!(err.to_string().contains("500"));
    }

    #[test]
    fn test_cooldown_active_error() {
        let err = CliError::CooldownActive(86400);
        assert!(err.to_string().contains("86400"));
    }

    #[test]
    fn test_invalid_stake_amount() {
        let err = CliError::InvalidStakeAmount(0);
        assert!(err.to_string().contains("minimum stake"));
    }
}
