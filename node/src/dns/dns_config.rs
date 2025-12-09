//! DNS Server Configuration
//!
//! Configuration structs for AEGIS DNS server including ports, rate limiting,
//! DNSSEC settings, and DoS protection parameters.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use super::DnsError;

/// Main DNS server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    /// UDP listen address (default: 0.0.0.0:53)
    pub udp_addr: SocketAddr,

    /// TCP listen address (default: 0.0.0.0:53)
    pub tcp_addr: SocketAddr,

    /// DNS over TLS (DoT) configuration
    pub dot: DotConfig,

    /// DNS over HTTPS (DoH) configuration
    pub doh: DohConfig,

    /// DNSSEC signing configuration
    pub dnssec: DnssecConfig,

    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,

    /// TCP connection limits (DoS protection)
    pub tcp_limits: TcpLimitConfig,

    /// Zone transfer (AXFR) settings
    pub axfr: AxfrConfig,

    /// AEGIS edge node settings
    pub edge: EdgeConfig,

    /// API server settings
    pub api: ApiConfig,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            udp_addr: "0.0.0.0:53".parse().unwrap(),
            tcp_addr: "0.0.0.0:53".parse().unwrap(),
            dot: DotConfig::default(),
            doh: DohConfig::default(),
            dnssec: DnssecConfig::default(),
            rate_limit: RateLimitConfig::default(),
            tcp_limits: TcpLimitConfig::default(),
            axfr: AxfrConfig::default(),
            edge: EdgeConfig::default(),
            api: ApiConfig::default(),
        }
    }
}

impl DnsConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), DnsError> {
        // Rate limit validation
        if self.rate_limit.queries_per_second == 0 {
            return Err(DnsError::ConfigError(
                "rate_limit.queries_per_second must be > 0".to_string(),
            ));
        }

        if self.rate_limit.burst_size == 0 {
            return Err(DnsError::ConfigError(
                "rate_limit.burst_size must be > 0".to_string(),
            ));
        }

        // TCP limits validation
        if self.tcp_limits.max_connections == 0 {
            return Err(DnsError::ConfigError(
                "tcp_limits.max_connections must be > 0".to_string(),
            ));
        }

        if self.tcp_limits.max_per_ip == 0 {
            return Err(DnsError::ConfigError(
                "tcp_limits.max_per_ip must be > 0".to_string(),
            ));
        }

        // DoT validation
        if self.dot.enabled {
            if self.dot.cert_path.is_none() {
                return Err(DnsError::ConfigError(
                    "DoT requires cert_path when enabled".to_string(),
                ));
            }
            if self.dot.key_path.is_none() {
                return Err(DnsError::ConfigError(
                    "DoT requires key_path when enabled".to_string(),
                ));
            }
        }

        // DoH validation
        if self.doh.enabled {
            if self.doh.cert_path.is_none() {
                return Err(DnsError::ConfigError(
                    "DoH requires cert_path when enabled".to_string(),
                ));
            }
            if self.doh.key_path.is_none() {
                return Err(DnsError::ConfigError(
                    "DoH requires key_path when enabled".to_string(),
                ));
            }
        }

        // DNSSEC validation
        if self.dnssec.enabled && self.dnssec.key_path.is_none() {
            return Err(DnsError::ConfigError(
                "DNSSEC requires key_path when enabled".to_string(),
            ));
        }

        Ok(())
    }

    /// Load configuration from TOML file
    pub fn from_toml(content: &str) -> Result<Self, DnsError> {
        toml::from_str(content).map_err(|e| DnsError::ConfigError(format!("TOML parse error: {}", e)))
    }

    /// Load configuration from YAML file
    pub fn from_yaml(content: &str) -> Result<Self, DnsError> {
        serde_yaml::from_str(content)
            .map_err(|e| DnsError::ConfigError(format!("YAML parse error: {}", e)))
    }

    /// Serialize to TOML
    pub fn to_toml(&self) -> Result<String, DnsError> {
        toml::to_string_pretty(self)
            .map_err(|e| DnsError::ConfigError(format!("TOML serialize error: {}", e)))
    }
}

/// DNS over TLS (DoT) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotConfig {
    /// Enable DNS over TLS (port 853)
    pub enabled: bool,
    /// Listen address for DoT
    pub addr: SocketAddr,
    /// TLS certificate path
    pub cert_path: Option<String>,
    /// TLS private key path
    pub key_path: Option<String>,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            addr: "0.0.0.0:853".parse().unwrap(),
            cert_path: None,
            key_path: None,
        }
    }
}

/// DNS over HTTPS (DoH) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DohConfig {
    /// Enable DNS over HTTPS
    pub enabled: bool,
    /// Listen address for DoH
    pub addr: SocketAddr,
    /// TLS certificate path
    pub cert_path: Option<String>,
    /// TLS private key path
    pub key_path: Option<String>,
    /// DoH endpoint path (default: /dns-query)
    pub path: String,
}

impl Default for DohConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            addr: "0.0.0.0:443".parse().unwrap(),
            cert_path: None,
            key_path: None,
            path: "/dns-query".to_string(),
        }
    }
}

/// DNSSEC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnssecConfig {
    /// Enable DNSSEC signing
    pub enabled: bool,
    /// Path to DNSSEC signing key
    pub key_path: Option<String>,
    /// Algorithm to use (default: ED25519)
    pub algorithm: DnssecAlgorithm,
    /// Signature validity period in days
    pub signature_validity_days: u32,
}

impl Default for DnssecConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key_path: None,
            algorithm: DnssecAlgorithm::Ed25519,
            signature_validity_days: 30,
        }
    }
}

/// DNSSEC signing algorithm
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DnssecAlgorithm {
    /// RSA/SHA-256 (Algorithm 8)
    RsaSha256,
    /// ECDSA P-256/SHA-256 (Algorithm 13)
    EcdsaP256Sha256,
    /// Ed25519 (Algorithm 15) - Recommended
    Ed25519,
}

impl DnssecAlgorithm {
    /// Get the algorithm number
    pub fn algorithm_number(&self) -> u8 {
        match self {
            DnssecAlgorithm::RsaSha256 => 8,
            DnssecAlgorithm::EcdsaP256Sha256 => 13,
            DnssecAlgorithm::Ed25519 => 15,
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Queries per second per IP
    pub queries_per_second: u32,
    /// Burst allowance (token bucket capacity)
    pub burst_size: u32,
    /// Window size for rate limiting in seconds
    pub window_secs: u64,
    /// Enable rate limiting (can be disabled for testing)
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            queries_per_second: 100,
            burst_size: 500,
            window_secs: 1,
            enabled: true,
        }
    }
}

/// TCP connection limits for DoS protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpLimitConfig {
    /// Maximum total TCP connections
    pub max_connections: usize,
    /// Maximum TCP connections per IP
    pub max_per_ip: usize,
    /// Idle connection timeout in seconds
    pub idle_timeout_secs: u64,
}

impl Default for TcpLimitConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            max_per_ip: 10,
            idle_timeout_secs: 60,
        }
    }
}

/// Zone transfer (AXFR) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxfrConfig {
    /// Enable zone transfers
    pub enabled: bool,
    /// IPs allowed to request zone transfers
    pub allowed_ips: Vec<String>,
}

impl Default for AxfrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_ips: vec![],
        }
    }
}

/// AEGIS edge node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// Anycast IPv4 to return for proxied records
    pub anycast_ipv4: Option<String>,
    /// Anycast IPv6 to return for proxied records
    pub anycast_ipv6: Option<String>,
    /// AEGIS nameservers to return when zone is created
    pub nameservers: Vec<String>,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            anycast_ipv4: None,
            anycast_ipv6: None,
            nameservers: vec![
                "ns1.aegis.network".to_string(),
                "ns2.aegis.network".to_string(),
            ],
        }
    }
}

/// DNS API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Enable API server
    pub enabled: bool,
    /// API server listen address
    pub addr: SocketAddr,
    /// Database path for zone persistence
    pub db_path: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            addr: "0.0.0.0:8054".parse().unwrap(),
            db_path: "./aegis-dns.db".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DnsConfig::default();
        assert_eq!(config.udp_addr.port(), 53);
        assert_eq!(config.tcp_addr.port(), 53);
        assert!(!config.dot.enabled);
        assert!(!config.doh.enabled);
        assert!(!config.dnssec.enabled);
        assert!(config.rate_limit.enabled);
    }

    #[test]
    fn test_config_validation_success() {
        let config = DnsConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_rate_limit() {
        let mut config = DnsConfig::default();
        config.rate_limit.queries_per_second = 0;
        assert!(config.validate().is_err());

        config.rate_limit.queries_per_second = 100;
        config.rate_limit.burst_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_tcp_limits() {
        let mut config = DnsConfig::default();
        config.tcp_limits.max_connections = 0;
        assert!(config.validate().is_err());

        config.tcp_limits.max_connections = 10000;
        config.tcp_limits.max_per_ip = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_dot() {
        let mut config = DnsConfig::default();
        config.dot.enabled = true;
        // Missing cert_path
        assert!(config.validate().is_err());

        config.dot.cert_path = Some("/path/to/cert.pem".to_string());
        // Missing key_path
        assert!(config.validate().is_err());

        config.dot.key_path = Some("/path/to/key.pem".to_string());
        // Now valid
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_doh() {
        let mut config = DnsConfig::default();
        config.doh.enabled = true;
        assert!(config.validate().is_err());

        config.doh.cert_path = Some("/path/to/cert.pem".to_string());
        config.doh.key_path = Some("/path/to/key.pem".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_dnssec() {
        let mut config = DnsConfig::default();
        config.dnssec.enabled = true;
        assert!(config.validate().is_err());

        config.dnssec.key_path = Some("/path/to/key.pem".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_dnssec_algorithm_number() {
        assert_eq!(DnssecAlgorithm::RsaSha256.algorithm_number(), 8);
        assert_eq!(DnssecAlgorithm::EcdsaP256Sha256.algorithm_number(), 13);
        assert_eq!(DnssecAlgorithm::Ed25519.algorithm_number(), 15);
    }

    #[test]
    fn test_config_toml_roundtrip() {
        let config = DnsConfig::default();
        let toml = config.to_toml().unwrap();
        let parsed = DnsConfig::from_toml(&toml).unwrap();

        assert_eq!(config.udp_addr, parsed.udp_addr);
        assert_eq!(config.rate_limit.queries_per_second, parsed.rate_limit.queries_per_second);
    }

    #[test]
    fn test_edge_config_nameservers() {
        let config = EdgeConfig::default();
        assert_eq!(config.nameservers.len(), 2);
        assert!(config.nameservers[0].contains("aegis"));
    }

    #[test]
    fn test_api_config_defaults() {
        let config = ApiConfig::default();
        assert!(config.enabled);
        assert_eq!(config.addr.port(), 8054);
        assert!(config.db_path.ends_with(".db"));
    }
}
