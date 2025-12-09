/// DDoS Protection Policy Definitions
///
/// Defines the configuration structures for DDoS protection policies
/// including rate limiting, challenge modes, and threshold settings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

// =============================================================================
// VALIDATION CONSTANTS
// =============================================================================

/// Maximum SYN flood threshold (packets per second)
pub const MAX_SYN_THRESHOLD: u64 = 100_000;

/// Minimum SYN flood threshold
pub const MIN_SYN_THRESHOLD: u64 = 10;

/// Maximum UDP flood threshold
pub const MAX_UDP_THRESHOLD: u64 = 1_000_000;

/// Minimum UDP flood threshold
pub const MIN_UDP_THRESHOLD: u64 = 100;

/// Maximum block duration (24 hours)
pub const MAX_BLOCK_DURATION_SECS: u64 = 86_400;

/// Minimum block duration (10 seconds)
pub const MIN_BLOCK_DURATION_SECS: u64 = 10;

/// Maximum rate limit requests per minute
pub const MAX_RATE_LIMIT_RPM: u64 = 100_000;

/// Minimum rate limit requests per minute
pub const MIN_RATE_LIMIT_RPM: u64 = 1;

/// Maximum rate limit window duration (1 hour)
pub const MAX_RATE_LIMIT_WINDOW_SECS: u64 = 3_600;

/// Minimum rate limit window duration (1 second)
pub const MIN_RATE_LIMIT_WINDOW_SECS: u64 = 1;

/// Maximum domain name length
pub const MAX_DOMAIN_LENGTH: usize = 253;

// =============================================================================
// ERROR TYPES
// =============================================================================

/// Validation errors for DDoS policies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DDoSPolicyError {
    /// Domain name is empty
    EmptyDomain,
    /// Domain name exceeds maximum length
    DomainTooLong { length: usize, max: usize },
    /// SYN threshold out of range
    InvalidSynThreshold { value: u64, min: u64, max: u64 },
    /// UDP threshold out of range
    InvalidUdpThreshold { value: u64, min: u64, max: u64 },
    /// Block duration out of range
    InvalidBlockDuration { value: u64, min: u64, max: u64 },
    /// Rate limit requests per minute out of range
    InvalidRateLimitRpm { value: u64, min: u64, max: u64 },
    /// Rate limit window duration out of range
    InvalidRateLimitWindow { value: u64, min: u64, max: u64 },
    /// Invalid IP address format
    InvalidIpAddress(String),
    /// Invalid CIDR notation
    InvalidCidr(String),
}

impl std::fmt::Display for DDoSPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DDoSPolicyError::EmptyDomain => write!(f, "Domain name cannot be empty"),
            DDoSPolicyError::DomainTooLong { length, max } => {
                write!(f, "Domain name too long: {} chars (max {})", length, max)
            }
            DDoSPolicyError::InvalidSynThreshold { value, min, max } => {
                write!(f, "SYN threshold {} out of range [{}, {}]", value, min, max)
            }
            DDoSPolicyError::InvalidUdpThreshold { value, min, max } => {
                write!(f, "UDP threshold {} out of range [{}, {}]", value, min, max)
            }
            DDoSPolicyError::InvalidBlockDuration { value, min, max } => {
                write!(f, "Block duration {} out of range [{}, {}]s", value, min, max)
            }
            DDoSPolicyError::InvalidRateLimitRpm { value, min, max } => {
                write!(f, "Rate limit {} RPM out of range [{}, {}]", value, min, max)
            }
            DDoSPolicyError::InvalidRateLimitWindow { value, min, max } => {
                write!(f, "Rate limit window {}s out of range [{}, {}]s", value, min, max)
            }
            DDoSPolicyError::InvalidIpAddress(ip) => {
                write!(f, "Invalid IP address: {}", ip)
            }
            DDoSPolicyError::InvalidCidr(cidr) => {
                write!(f, "Invalid CIDR notation: {}", cidr)
            }
        }
    }
}

impl std::error::Error for DDoSPolicyError {}

// =============================================================================
// RATE LIMIT SCOPE
// =============================================================================

/// Scope for rate limiting
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitScope {
    /// Rate limit per client IP address
    #[default]
    PerIp,
    /// Rate limit per route/endpoint
    PerRoute,
    /// Global rate limit across all requests
    Global,
}

// =============================================================================
// CHALLENGE TYPE
// =============================================================================

/// Type of challenge to issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeType {
    /// Invisible challenge (JS fingerprinting only)
    Invisible,
    /// Managed challenge (system decides)
    #[default]
    Managed,
    /// Interactive challenge (visible CAPTCHA)
    Interactive,
}

// =============================================================================
// RATE LIMIT POLICY
// =============================================================================

/// Rate limiting configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitPolicy {
    /// Whether rate limiting is enabled
    pub enabled: bool,

    /// Maximum requests per minute
    #[serde(default = "default_max_requests_per_minute")]
    pub max_requests_per_minute: u64,

    /// Window duration in seconds
    #[serde(default = "default_window_duration_secs")]
    pub window_duration_secs: u64,

    /// Rate limit scope
    #[serde(default)]
    pub scope: RateLimitScope,

    /// Burst allowance (extra requests above limit)
    #[serde(default)]
    pub burst_allowance: u64,
}

fn default_max_requests_per_minute() -> u64 { 100 }
fn default_window_duration_secs() -> u64 { 60 }

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_requests_per_minute: 100,
            window_duration_secs: 60,
            scope: RateLimitScope::PerIp,
            burst_allowance: 0,
        }
    }
}

impl RateLimitPolicy {
    /// Validate the rate limit policy
    pub fn validate(&self) -> Result<(), DDoSPolicyError> {
        if self.max_requests_per_minute < MIN_RATE_LIMIT_RPM
            || self.max_requests_per_minute > MAX_RATE_LIMIT_RPM
        {
            return Err(DDoSPolicyError::InvalidRateLimitRpm {
                value: self.max_requests_per_minute,
                min: MIN_RATE_LIMIT_RPM,
                max: MAX_RATE_LIMIT_RPM,
            });
        }

        if self.window_duration_secs < MIN_RATE_LIMIT_WINDOW_SECS
            || self.window_duration_secs > MAX_RATE_LIMIT_WINDOW_SECS
        {
            return Err(DDoSPolicyError::InvalidRateLimitWindow {
                value: self.window_duration_secs,
                min: MIN_RATE_LIMIT_WINDOW_SECS,
                max: MAX_RATE_LIMIT_WINDOW_SECS,
            });
        }

        Ok(())
    }
}

// =============================================================================
// CHALLENGE POLICY
// =============================================================================

/// Challenge mode configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengePolicy {
    /// Whether challenge mode is enabled
    pub enabled: bool,

    /// Threshold before issuing challenge (requests per minute)
    #[serde(default = "default_trigger_threshold")]
    pub trigger_threshold: u64,

    /// Type of challenge to issue
    #[serde(default)]
    pub challenge_type: ChallengeType,

    /// Challenge validity duration in seconds
    #[serde(default = "default_challenge_validity_secs")]
    pub validity_secs: u64,
}

fn default_trigger_threshold() -> u64 { 50 }
fn default_challenge_validity_secs() -> u64 { 900 } // 15 minutes

impl Default for ChallengePolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_threshold: 50,
            challenge_type: ChallengeType::Managed,
            validity_secs: 900,
        }
    }
}

// =============================================================================
// DDOS POLICY
// =============================================================================

/// Complete DDoS protection policy for a domain
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DDoSPolicy {
    /// Domain name this policy applies to (can be omitted in API - extracted from URL path)
    #[serde(default)]
    pub domain: String,

    /// Whether DDoS protection is enabled
    pub enabled: bool,

    /// SYN flood threshold (packets per second per IP)
    #[serde(default = "default_syn_threshold")]
    pub syn_threshold: u64,

    /// UDP flood threshold (packets per second per IP)
    #[serde(default = "default_udp_threshold")]
    pub udp_threshold: u64,

    /// Duration to block offending IPs (seconds)
    #[serde(default = "default_block_duration_secs")]
    pub block_duration_secs: u64,

    /// Rate limiting policy
    #[serde(default)]
    pub rate_limit: Option<RateLimitPolicy>,

    /// Challenge mode policy
    #[serde(default)]
    pub challenge_mode: Option<ChallengePolicy>,

    /// Custom allowlist IPs for this domain
    #[serde(default)]
    pub allowlist: Vec<String>,

    /// Custom blocklist IPs for this domain
    #[serde(default)]
    pub blocklist: Vec<String>,

    /// Creation timestamp (Unix seconds)
    #[serde(default)]
    pub created_at: u64,

    /// Last update timestamp (Unix seconds)
    #[serde(default)]
    pub updated_at: u64,
}

fn default_syn_threshold() -> u64 { 100 }
fn default_udp_threshold() -> u64 { 1000 }
fn default_block_duration_secs() -> u64 { 300 }

impl Default for DDoSPolicy {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            domain: String::new(),
            enabled: true,
            syn_threshold: 100,
            udp_threshold: 1000,
            block_duration_secs: 300,
            rate_limit: Some(RateLimitPolicy::default()),
            challenge_mode: None,
            allowlist: Vec::new(),
            blocklist: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

impl DDoSPolicy {
    /// Create a new DDoS policy for a domain
    pub fn new(domain: String) -> Self {
        let mut policy = Self::default();
        policy.domain = domain;
        policy
    }

    /// Validate the policy configuration
    pub fn validate(&self) -> Result<(), DDoSPolicyError> {
        // Validate domain
        if self.domain.is_empty() {
            return Err(DDoSPolicyError::EmptyDomain);
        }

        if self.domain.len() > MAX_DOMAIN_LENGTH {
            return Err(DDoSPolicyError::DomainTooLong {
                length: self.domain.len(),
                max: MAX_DOMAIN_LENGTH,
            });
        }

        // Validate SYN threshold
        if self.syn_threshold < MIN_SYN_THRESHOLD || self.syn_threshold > MAX_SYN_THRESHOLD {
            return Err(DDoSPolicyError::InvalidSynThreshold {
                value: self.syn_threshold,
                min: MIN_SYN_THRESHOLD,
                max: MAX_SYN_THRESHOLD,
            });
        }

        // Validate UDP threshold
        if self.udp_threshold < MIN_UDP_THRESHOLD || self.udp_threshold > MAX_UDP_THRESHOLD {
            return Err(DDoSPolicyError::InvalidUdpThreshold {
                value: self.udp_threshold,
                min: MIN_UDP_THRESHOLD,
                max: MAX_UDP_THRESHOLD,
            });
        }

        // Validate block duration
        if self.block_duration_secs < MIN_BLOCK_DURATION_SECS
            || self.block_duration_secs > MAX_BLOCK_DURATION_SECS
        {
            return Err(DDoSPolicyError::InvalidBlockDuration {
                value: self.block_duration_secs,
                min: MIN_BLOCK_DURATION_SECS,
                max: MAX_BLOCK_DURATION_SECS,
            });
        }

        // Validate rate limit policy
        if let Some(ref rate_limit) = self.rate_limit {
            rate_limit.validate()?;
        }

        // Validate allowlist IPs
        for ip in &self.allowlist {
            validate_ip_or_cidr(ip)?;
        }

        // Validate blocklist IPs
        for ip in &self.blocklist {
            validate_ip_or_cidr(ip)?;
        }

        Ok(())
    }

    /// Update the policy with a partial update
    pub fn merge(&mut self, update: DDoSPolicyUpdate) {
        if let Some(enabled) = update.enabled {
            self.enabled = enabled;
        }
        if let Some(syn_threshold) = update.syn_threshold {
            self.syn_threshold = syn_threshold;
        }
        if let Some(udp_threshold) = update.udp_threshold {
            self.udp_threshold = udp_threshold;
        }
        if let Some(block_duration_secs) = update.block_duration_secs {
            self.block_duration_secs = block_duration_secs;
        }
        if let Some(rate_limit) = update.rate_limit {
            self.rate_limit = Some(rate_limit);
        }
        if let Some(challenge_mode) = update.challenge_mode {
            self.challenge_mode = Some(challenge_mode);
        }
        if let Some(allowlist) = update.allowlist {
            self.allowlist = allowlist;
        }
        if let Some(blocklist) = update.blocklist {
            self.blocklist = blocklist;
        }

        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

/// Partial update for a DDoS policy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DDoSPolicyUpdate {
    pub enabled: Option<bool>,
    pub syn_threshold: Option<u64>,
    pub udp_threshold: Option<u64>,
    pub block_duration_secs: Option<u64>,
    pub rate_limit: Option<RateLimitPolicy>,
    pub challenge_mode: Option<ChallengePolicy>,
    pub allowlist: Option<Vec<String>>,
    pub blocklist: Option<Vec<String>>,
}

// =============================================================================
// BLOCKLIST ENTRY
// =============================================================================

/// Entry in the IP blocklist
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlocklistEntry {
    /// IP address (IPv4 or IPv6)
    pub ip: String,

    /// Reason for blocking
    pub reason: String,

    /// Block expiration time (Unix seconds, 0 = permanent)
    pub expires_at: u64,

    /// When the block was added
    pub added_at: u64,

    /// Source of the block (manual, auto, threat_intel)
    #[serde(default)]
    pub source: BlockSource,

    /// Domain this block applies to (None = global)
    pub domain: Option<String>,
}

/// Source of a blocklist entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockSource {
    /// Manually added by operator
    #[default]
    Manual,
    /// Automatically detected attack
    Auto,
    /// Received from P2P threat intelligence
    ThreatIntel,
}

impl BlocklistEntry {
    /// Create a new blocklist entry
    pub fn new(ip: String, reason: String, duration_secs: u64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            ip,
            reason,
            expires_at: if duration_secs == 0 { 0 } else { now + duration_secs },
            added_at: now,
            source: BlockSource::Manual,
            domain: None,
        }
    }

    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        if self.expires_at == 0 {
            return false; // Permanent block
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now > self.expires_at
    }

    /// Get remaining time in seconds (0 if expired or permanent)
    pub fn remaining_secs(&self) -> u64 {
        if self.expires_at == 0 {
            return 0; // Permanent
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.expires_at.saturating_sub(now)
    }
}

// =============================================================================
// ALLOWLIST ENTRY
// =============================================================================

/// Entry in the IP allowlist
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowlistEntry {
    /// IP address or CIDR (e.g., "192.168.1.1" or "10.0.0.0/8")
    pub ip: String,

    /// Description/reason for allowing
    pub description: String,

    /// When the entry was added
    pub added_at: u64,

    /// Domain this allowlist applies to (None = global)
    pub domain: Option<String>,
}

impl AllowlistEntry {
    /// Create a new allowlist entry
    pub fn new(ip: String, description: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            ip,
            description,
            added_at: now,
            domain: None,
        }
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Validate an IP address or CIDR notation
pub fn validate_ip_or_cidr(input: &str) -> Result<(), DDoSPolicyError> {
    // Check for CIDR notation
    if let Some((ip_part, prefix_part)) = input.split_once('/') {
        // Validate prefix
        let prefix: u8 = prefix_part.parse().map_err(|_| {
            DDoSPolicyError::InvalidCidr(input.to_string())
        })?;

        // Validate IP part and prefix range
        if let Ok(addr) = IpAddr::from_str(ip_part) {
            let max_prefix = match addr {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };

            if prefix > max_prefix {
                return Err(DDoSPolicyError::InvalidCidr(input.to_string()));
            }

            return Ok(());
        }

        return Err(DDoSPolicyError::InvalidCidr(input.to_string()));
    }

    // Plain IP address
    IpAddr::from_str(input).map_err(|_| {
        DDoSPolicyError::InvalidIpAddress(input.to_string())
    })?;

    Ok(())
}

/// Parse an IP address string into IpAddr
pub fn parse_ip(input: &str) -> Result<IpAddr, DDoSPolicyError> {
    // Handle CIDR - just take the IP part
    let ip_str = input.split('/').next().unwrap_or(input);

    IpAddr::from_str(ip_str).map_err(|_| {
        DDoSPolicyError::InvalidIpAddress(input.to_string())
    })
}

// =============================================================================
// ROUTE INTEGRATION
// =============================================================================

/// DDoS protection config for route-based dispatch (Sprint 16 integration)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DDoSRouteConfig {
    /// SYN flood threshold
    #[serde(default = "default_syn_threshold")]
    pub syn_threshold: u64,

    /// UDP flood threshold
    #[serde(default = "default_udp_threshold")]
    pub udp_threshold: u64,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: Option<RateLimitRouteConfig>,

    /// Challenge mode configuration
    #[serde(default)]
    pub challenge_mode: Option<ChallengeRouteConfig>,
}

/// Rate limit config for route-based dispatch
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitRouteConfig {
    /// Maximum requests per minute
    pub max_requests_per_minute: u64,

    /// Window duration in seconds
    #[serde(default = "default_window_duration_secs")]
    pub window_duration_secs: u64,

    /// Rate limit scope
    #[serde(default)]
    pub scope: RateLimitScope,
}

/// Challenge config for route-based dispatch
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeRouteConfig {
    /// Whether challenges are enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Threshold before issuing challenge
    pub trigger_threshold: u64,

    /// Challenge type
    #[serde(default)]
    pub challenge_type: ChallengeType,
}

fn default_true() -> bool { true }

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_validation_valid() {
        let policy = DDoSPolicy::new("example.com".to_string());
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_policy_validation_empty_domain() {
        let policy = DDoSPolicy::new(String::new());
        assert!(matches!(policy.validate(), Err(DDoSPolicyError::EmptyDomain)));
    }

    #[test]
    fn test_policy_validation_domain_too_long() {
        let long_domain = "a".repeat(300);
        let policy = DDoSPolicy::new(long_domain);
        assert!(matches!(
            policy.validate(),
            Err(DDoSPolicyError::DomainTooLong { .. })
        ));
    }

    #[test]
    fn test_policy_validation_syn_threshold_too_low() {
        let mut policy = DDoSPolicy::new("example.com".to_string());
        policy.syn_threshold = 1; // Below MIN_SYN_THRESHOLD
        assert!(matches!(
            policy.validate(),
            Err(DDoSPolicyError::InvalidSynThreshold { .. })
        ));
    }

    #[test]
    fn test_policy_validation_syn_threshold_too_high() {
        let mut policy = DDoSPolicy::new("example.com".to_string());
        policy.syn_threshold = 1_000_000; // Above MAX_SYN_THRESHOLD
        assert!(matches!(
            policy.validate(),
            Err(DDoSPolicyError::InvalidSynThreshold { .. })
        ));
    }

    #[test]
    fn test_rate_limit_validation() {
        let rate_limit = RateLimitPolicy {
            enabled: true,
            max_requests_per_minute: 100,
            window_duration_secs: 60,
            scope: RateLimitScope::PerIp,
            burst_allowance: 10,
        };
        assert!(rate_limit.validate().is_ok());
    }

    #[test]
    fn test_rate_limit_validation_invalid_rpm() {
        let rate_limit = RateLimitPolicy {
            enabled: true,
            max_requests_per_minute: 0, // Below minimum
            window_duration_secs: 60,
            scope: RateLimitScope::PerIp,
            burst_allowance: 0,
        };
        assert!(matches!(
            rate_limit.validate(),
            Err(DDoSPolicyError::InvalidRateLimitRpm { .. })
        ));
    }

    #[test]
    fn test_validate_ip_v4() {
        assert!(validate_ip_or_cidr("192.168.1.1").is_ok());
        assert!(validate_ip_or_cidr("10.0.0.1").is_ok());
        assert!(validate_ip_or_cidr("0.0.0.0").is_ok());
        assert!(validate_ip_or_cidr("255.255.255.255").is_ok());
    }

    #[test]
    fn test_validate_ip_v6() {
        assert!(validate_ip_or_cidr("::1").is_ok());
        assert!(validate_ip_or_cidr("fe80::1").is_ok());
        assert!(validate_ip_or_cidr("2001:db8::1").is_ok());
    }

    #[test]
    fn test_validate_cidr_v4() {
        assert!(validate_ip_or_cidr("192.168.0.0/24").is_ok());
        assert!(validate_ip_or_cidr("10.0.0.0/8").is_ok());
        assert!(validate_ip_or_cidr("0.0.0.0/0").is_ok());
        assert!(validate_ip_or_cidr("192.168.1.1/32").is_ok());
    }

    #[test]
    fn test_validate_cidr_v6() {
        assert!(validate_ip_or_cidr("2001:db8::/32").is_ok());
        assert!(validate_ip_or_cidr("fe80::/10").is_ok());
        assert!(validate_ip_or_cidr("::1/128").is_ok());
    }

    #[test]
    fn test_validate_invalid_ip() {
        assert!(validate_ip_or_cidr("invalid").is_err());
        assert!(validate_ip_or_cidr("256.1.1.1").is_err());
        assert!(validate_ip_or_cidr("192.168.1").is_err());
    }

    #[test]
    fn test_validate_invalid_cidr() {
        assert!(validate_ip_or_cidr("192.168.1.1/33").is_err()); // Prefix too large for IPv4
        assert!(validate_ip_or_cidr("192.168.1.1/abc").is_err()); // Non-numeric prefix
        assert!(validate_ip_or_cidr("::1/129").is_err()); // Prefix too large for IPv6
    }

    #[test]
    fn test_blocklist_entry_expiration() {
        let entry = BlocklistEntry::new(
            "192.168.1.1".to_string(),
            "Test".to_string(),
            0, // Permanent
        );
        assert!(!entry.is_expired());

        let entry = BlocklistEntry::new(
            "192.168.1.1".to_string(),
            "Test".to_string(),
            1, // 1 second
        );
        assert!(!entry.is_expired()); // Should not be expired immediately
    }

    #[test]
    fn test_policy_merge() {
        let mut policy = DDoSPolicy::new("example.com".to_string());
        assert!(policy.enabled);
        assert_eq!(policy.syn_threshold, 100);

        let update = DDoSPolicyUpdate {
            enabled: Some(false),
            syn_threshold: Some(200),
            ..Default::default()
        };

        policy.merge(update);

        assert!(!policy.enabled);
        assert_eq!(policy.syn_threshold, 200);
    }

    #[test]
    fn test_policy_with_allowlist() {
        let mut policy = DDoSPolicy::new("example.com".to_string());
        policy.allowlist = vec![
            "192.168.1.1".to_string(),
            "10.0.0.0/8".to_string(),
        ];
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_policy_with_invalid_allowlist() {
        let mut policy = DDoSPolicy::new("example.com".to_string());
        policy.allowlist = vec!["invalid-ip".to_string()];
        assert!(matches!(
            policy.validate(),
            Err(DDoSPolicyError::InvalidIpAddress(_))
        ));
    }

    #[test]
    fn test_serialization() {
        let policy = DDoSPolicy::new("example.com".to_string());
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: DDoSPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy.domain, parsed.domain);
        assert_eq!(policy.enabled, parsed.enabled);
    }

    #[test]
    fn test_rate_limit_scope_serialization() {
        let scope = RateLimitScope::PerIp;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"per_ip\"");

        let scope = RateLimitScope::PerRoute;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"per_route\"");
    }
}
