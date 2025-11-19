use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host address to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// Maximum concurrent connections
    pub max_connections: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,
    /// Default TTL in seconds
    pub default_ttl: u64,
    /// Maximum cache size in MB
    pub max_size_mb: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable eBPF/XDP filtering (future sprint)
    pub ebpf_enabled: bool,
    /// Enable WAF (future sprint)
    pub waf_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                max_connections: 10000,
            },
            cache: CacheConfig {
                enabled: false, // Not implemented in Sprint 1
                default_ttl: 3600,
                max_size_mb: 1024,
            },
            security: SecurityConfig {
                ebpf_enabled: false, // Future sprint
                waf_enabled: false,  // Future sprint
            },
        }
    }
}

impl Config {
    /// Load configuration from TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to TOML file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(&self)?;
        fs::write(path, contents)?;
        Ok(())
    }

    /// Validate configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.server.port == 0 {
            anyhow::bail!("Invalid port: cannot be 0");
        }

        if self.server.max_connections == 0 {
            anyhow::bail!("Invalid max_connections: must be > 0");
        }

        if self.cache.max_size_mb == 0 {
            anyhow::bail!("Invalid cache max_size_mb: must be > 0");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.max_connections, 10000);
        assert!(!config.cache.enabled);
        assert!(!config.security.ebpf_enabled);
    }

    #[test]
    fn test_config_validation_succeeds() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_fails_zero_port() {
        let mut config = Config::default();
        config.server.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_fails_zero_connections() {
        let mut config = Config::default();
        config.server.max_connections = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("host"));
        assert!(toml_str.contains("port"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            [server]
            host = "0.0.0.0"
            port = 9090
            max_connections = 5000

            [cache]
            enabled = true
            default_ttl = 7200
            max_size_mb = 2048

            [security]
            ebpf_enabled = false
            waf_enabled = false
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9090);
        assert_eq!(config.server.max_connections, 5000);
        assert!(config.cache.enabled);
        assert_eq!(config.cache.default_ttl, 7200);
    }

    #[test]
    fn test_config_round_trip() {
        let original = Config::default();
        let toml_str = toml::to_string(&original).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(original.server.port, deserialized.server.port);
        assert_eq!(original.server.host, deserialized.server.host);
    }
}
