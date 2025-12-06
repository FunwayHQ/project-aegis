// Sprint 24: Distributed Enforcement & Global Blocklist Sync
//
// This module implements coordinated security enforcement across the edge network:
// 1. Global blocklist synchronization via P2P threat intel
// 2. IPv6 support for threat intelligence
// 3. Distributed trust score sharing
// 4. Coordinated challenge issuance (prevent re-challenges)
// 5. eBPF blocklist integration (interface for real-time updates)

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{info, warn};

// SECURITY FIX (X2.6): Import lock recovery utilities for std::sync::RwLock
use crate::lock_utils::{read_lock_or_recover, write_lock_or_recover};

// ============================================
// IPv6-Enabled Threat Intelligence
// ============================================

/// IP address that supports both IPv4 and IPv6
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThreatIpAddress {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

impl ThreatIpAddress {
    /// Parse from string (supports both IPv4 and IPv6)
    pub fn parse(s: &str) -> Result<Self, String> {
        // Try IPv4 first
        if let Ok(v4) = s.parse::<Ipv4Addr>() {
            return Ok(ThreatIpAddress::V4(v4));
        }
        // Try IPv6
        if let Ok(v6) = s.parse::<Ipv6Addr>() {
            return Ok(ThreatIpAddress::V6(v6));
        }
        Err(format!("Invalid IP address: {}", s))
    }

    /// Check if this is an IPv4 address
    pub fn is_ipv4(&self) -> bool {
        matches!(self, ThreatIpAddress::V4(_))
    }

    /// Check if this is an IPv6 address
    pub fn is_ipv6(&self) -> bool {
        matches!(self, ThreatIpAddress::V6(_))
    }

    /// Convert to bytes for eBPF map
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ThreatIpAddress::V4(v4) => v4.octets().to_vec(),
            ThreatIpAddress::V6(v6) => v6.octets().to_vec(),
        }
    }

    /// Get the IP version (4 or 6)
    pub fn version(&self) -> u8 {
        match self {
            ThreatIpAddress::V4(_) => 4,
            ThreatIpAddress::V6(_) => 6,
        }
    }
}

impl std::fmt::Display for ThreatIpAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreatIpAddress::V4(v4) => write!(f, "{}", v4),
            ThreatIpAddress::V6(v6) => write!(f, "{}", v6),
        }
    }
}

impl From<IpAddr> for ThreatIpAddress {
    fn from(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(v4) => ThreatIpAddress::V4(v4),
            IpAddr::V6(v6) => ThreatIpAddress::V6(v6),
        }
    }
}

impl From<Ipv4Addr> for ThreatIpAddress {
    fn from(addr: Ipv4Addr) -> Self {
        ThreatIpAddress::V4(addr)
    }
}

impl From<Ipv6Addr> for ThreatIpAddress {
    fn from(addr: Ipv6Addr) -> Self {
        ThreatIpAddress::V6(addr)
    }
}

/// Enhanced threat intelligence with IPv6 support
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnhancedThreatIntel {
    /// IP address (IPv4 or IPv6)
    pub ip: ThreatIpAddress,
    /// Type of threat
    pub threat_type: ThreatType,
    /// Severity level (1-10)
    pub severity: u8,
    /// Detection timestamp (Unix timestamp in milliseconds for precision)
    pub timestamp_ms: u64,
    /// Block duration in seconds
    pub block_duration_secs: u64,
    /// Source node ID (peer ID)
    pub source_node: String,
    /// Optional description
    pub description: Option<String>,
    /// Signature from source node (for authenticity)
    pub signature: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Threat type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatType {
    /// SYN flood attack
    SynFlood,
    /// UDP flood attack
    UdpFlood,
    /// HTTP flood (Layer 7 DDoS)
    HttpFlood,
    /// Brute force login attempts
    BruteForce,
    /// Credential stuffing
    CredentialStuffing,
    /// Account enumeration
    AccountEnumeration,
    /// API scraping
    ApiScraping,
    /// SQL injection attempt
    SqlInjection,
    /// XSS attempt
    Xss,
    /// Path traversal
    PathTraversal,
    /// Bot activity
    Bot,
    /// Scanner/probe
    Scanner,
    /// Generic malicious activity
    Malicious,
}

impl EnhancedThreatIntel {
    /// Create a new threat intelligence report
    pub fn new(
        ip: ThreatIpAddress,
        threat_type: ThreatType,
        severity: u8,
        block_duration_secs: u64,
        source_node: String,
    ) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            ip,
            threat_type,
            severity: severity.clamp(1, 10),
            timestamp_ms,
            block_duration_secs,
            source_node,
            description: None,
            signature: None,
            metadata: HashMap::new(),
        }
    }

    /// Add description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Sign the threat intel with Ed25519 key
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let payload = self.signing_payload();
        let signature = signing_key.sign(payload.as_bytes());
        self.signature = Some(STANDARD.encode(signature.to_bytes()));
    }

    /// Get the payload used for signing (excludes signature field)
    fn signing_payload(&self) -> String {
        format!(
            "{}:{}:{:?}:{}:{}:{}",
            self.ip,
            self.timestamp_ms,
            self.threat_type,
            self.severity,
            self.block_duration_secs,
            self.source_node
        )
    }

    /// Verify signature
    pub fn verify_signature(&self, public_key: &VerifyingKey) -> bool {
        let Some(sig_str) = &self.signature else {
            return false;
        };

        let Ok(sig_bytes) = STANDARD.decode(sig_str) else {
            return false;
        };

        let Ok(sig_array): Result<[u8; 64], _> = sig_bytes.try_into() else {
            return false;
        };

        let signature = Signature::from_bytes(&sig_array);
        let payload = self.signing_payload();

        public_key.verify(payload.as_bytes(), &signature).is_ok()
    }

    /// Validate threat intel data
    pub fn validate(&self) -> Result<(), String> {
        // Validate severity
        if self.severity == 0 || self.severity > 10 {
            return Err("Severity must be between 1 and 10".to_string());
        }

        // Validate block duration (max 24 hours)
        if self.block_duration_secs == 0 || self.block_duration_secs > 86400 {
            return Err("Block duration must be between 1 second and 24 hours".to_string());
        }

        // Validate timestamp (not too far in past or future)
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Allow 5 minutes clock skew in future
        if self.timestamp_ms > now_ms + 300_000 {
            return Err("Timestamp is too far in the future".to_string());
        }

        // Allow up to 1 hour old
        if now_ms.saturating_sub(self.timestamp_ms) > 3_600_000 {
            return Err("Timestamp is too old (>1 hour)".to_string());
        }

        Ok(())
    }

    /// Check if the block has expired
    pub fn is_expired(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let expiry_ms = self.timestamp_ms + (self.block_duration_secs * 1000);
        now_ms > expiry_ms
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// ============================================
// Global Blocklist
// ============================================

/// Entry in the global blocklist
#[derive(Debug, Clone)]
pub struct BlocklistEntry {
    pub ip: ThreatIpAddress,
    pub threat_type: ThreatType,
    pub severity: u8,
    pub added_at: Instant,
    pub expires_at: Instant,
    pub source_node: String,
}

/// Global blocklist with automatic expiration
pub struct GlobalBlocklist {
    /// IPv4 entries
    ipv4_entries: Arc<RwLock<HashMap<Ipv4Addr, BlocklistEntry>>>,
    /// IPv6 entries
    ipv6_entries: Arc<RwLock<HashMap<Ipv6Addr, BlocklistEntry>>>,
    /// Callback for eBPF updates
    ebpf_callback: Option<Box<dyn Fn(&ThreatIpAddress, bool) + Send + Sync>>,
    /// Statistics
    stats: Arc<RwLock<BlocklistStats>>,
}

/// Blocklist statistics
#[derive(Debug, Clone, Default)]
pub struct BlocklistStats {
    pub total_entries: usize,
    pub ipv4_entries: usize,
    pub ipv6_entries: usize,
    pub total_additions: u64,
    pub total_expirations: u64,
    pub total_blocks: u64,
}

impl GlobalBlocklist {
    pub fn new() -> Self {
        Self {
            ipv4_entries: Arc::new(RwLock::new(HashMap::new())),
            ipv6_entries: Arc::new(RwLock::new(HashMap::new())),
            ebpf_callback: None,
            stats: Arc::new(RwLock::new(BlocklistStats::default())),
        }
    }

    /// Set eBPF update callback
    pub fn set_ebpf_callback<F>(&mut self, callback: F)
    where
        F: Fn(&ThreatIpAddress, bool) + Send + Sync + 'static,
    {
        self.ebpf_callback = Some(Box::new(callback));
    }

    /// Add an IP to the blocklist (requires pre-validation)
    ///
    /// # Security Note (Y5.1)
    ///
    /// This method is `pub(crate)` to restrict access to internal code only.
    /// External consumers MUST use `add_verified()` for P2P threat intel to prevent
    /// spoofing attacks where malicious nodes inject fake threats.
    ///
    /// Only use this method internally after signature verification has been completed,
    /// or in trusted internal code paths (tests, local threat detection).
    ///
    /// For P2P-received threats, ALWAYS use `add_verified()`.
    pub(crate) async fn add(&self, threat: &EnhancedThreatIntel) {
        let now = Instant::now();
        let expires_at = now + Duration::from_secs(threat.block_duration_secs);

        let entry = BlocklistEntry {
            ip: threat.ip.clone(),
            threat_type: threat.threat_type,
            severity: threat.severity,
            added_at: now,
            expires_at,
            source_node: threat.source_node.clone(),
        };

        match &threat.ip {
            ThreatIpAddress::V4(v4) => {
                let mut entries = self.ipv4_entries.write().await;
                entries.insert(*v4, entry);
            }
            ThreatIpAddress::V6(v6) => {
                let mut entries = self.ipv6_entries.write().await;
                entries.insert(*v6, entry);
            }
        }

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_additions += 1;
            self.update_entry_counts(&mut stats).await;
        }

        // Notify eBPF
        if let Some(callback) = &self.ebpf_callback {
            callback(&threat.ip, true);
        }
    }

    /// Add an IP to the blocklist with signature verification (Sprint 29 Security Fix)
    ///
    /// This method MUST be used for all P2P-received threat intelligence to prevent
    /// spoofing attacks where malicious nodes inject fake threats.
    ///
    /// # Arguments
    /// * `threat` - The threat intelligence to add
    /// * `public_key` - The public key of the expected signer (source node)
    ///
    /// # Returns
    /// * `Ok(())` if the threat was verified and added
    /// * `Err(String)` if verification failed
    pub async fn add_verified(
        &self,
        threat: &EnhancedThreatIntel,
        public_key: &VerifyingKey,
    ) -> Result<(), String> {
        // Step 1: Validate the threat data
        threat.validate()?;

        // Step 2: Check if signature exists
        if threat.signature.is_none() {
            return Err("Missing signature on threat intelligence".to_string());
        }

        // Step 3: Verify signature
        if !threat.verify_signature(public_key) {
            return Err("Invalid signature on threat intelligence".to_string());
        }

        // Step 4: Verify source_node matches the public key
        let expected_source = hex::encode(public_key.as_bytes());
        if threat.source_node != expected_source {
            return Err(format!(
                "Source node mismatch: expected {}, got {}",
                &expected_source[..16],
                &threat.source_node[..threat.source_node.len().min(16)]
            ));
        }

        // Step 5: Check if already expired (don't add stale threats)
        if threat.is_expired() {
            return Err("Threat intelligence has already expired".to_string());
        }

        // All checks passed, add to blocklist
        self.add(threat).await;

        Ok(())
    }

    /// Add an IP with signature verification using hex-encoded public key
    pub async fn add_verified_hex(
        &self,
        threat: &EnhancedThreatIntel,
        public_key_hex: &str,
    ) -> Result<(), String> {
        // Decode the public key
        let public_key_bytes = hex::decode(public_key_hex)
            .map_err(|e| format!("Invalid public key hex: {}", e))?;

        if public_key_bytes.len() != 32 {
            return Err(format!(
                "Invalid public key length: expected 32 bytes, got {}",
                public_key_bytes.len()
            ));
        }

        let public_key_array: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| "Failed to convert public key bytes")?;

        let public_key = VerifyingKey::from_bytes(&public_key_array)
            .map_err(|e| format!("Invalid public key: {}", e))?;

        self.add_verified(threat, &public_key).await
    }

    /// Remove an IP from the blocklist
    pub async fn remove(&self, ip: &ThreatIpAddress) -> bool {
        let removed = match ip {
            ThreatIpAddress::V4(v4) => {
                let mut entries = self.ipv4_entries.write().await;
                entries.remove(v4).is_some()
            }
            ThreatIpAddress::V6(v6) => {
                let mut entries = self.ipv6_entries.write().await;
                entries.remove(v6).is_some()
            }
        };

        if removed {
            // Update stats
            {
                let mut stats = self.stats.write().await;
                self.update_entry_counts(&mut stats).await;
            }

            // Notify eBPF
            if let Some(callback) = &self.ebpf_callback {
                callback(ip, false);
            }
        }

        removed
    }

    /// Check if an IP is blocked
    pub async fn is_blocked(&self, ip: &ThreatIpAddress) -> bool {
        let now = Instant::now();

        match ip {
            ThreatIpAddress::V4(v4) => {
                let entries = self.ipv4_entries.read().await;
                if let Some(entry) = entries.get(v4) {
                    if entry.expires_at > now {
                        return true;
                    }
                }
            }
            ThreatIpAddress::V6(v6) => {
                let entries = self.ipv6_entries.read().await;
                if let Some(entry) = entries.get(v6) {
                    if entry.expires_at > now {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if an IP string is blocked
    pub async fn is_blocked_str(&self, ip_str: &str) -> bool {
        if let Ok(ip) = ThreatIpAddress::parse(ip_str) {
            self.is_blocked(&ip).await
        } else {
            false
        }
    }

    /// Get entry details for an IP
    pub async fn get_entry(&self, ip: &ThreatIpAddress) -> Option<BlocklistEntry> {
        match ip {
            ThreatIpAddress::V4(v4) => {
                let entries = self.ipv4_entries.read().await;
                entries.get(v4).cloned()
            }
            ThreatIpAddress::V6(v6) => {
                let entries = self.ipv6_entries.read().await;
                entries.get(v6).cloned()
            }
        }
    }

    /// Remove expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let now = Instant::now();
        let mut expired_count = 0;
        let mut expired_ips = Vec::new();

        // Collect expired IPv4 entries
        {
            let mut entries = self.ipv4_entries.write().await;
            let before = entries.len();
            entries.retain(|_, entry| entry.expires_at > now);
            let removed = before - entries.len();
            expired_count += removed;

            // Collect IPs for eBPF notification
            for (ip, entry) in entries.iter() {
                if entry.expires_at <= now {
                    expired_ips.push(ThreatIpAddress::V4(*ip));
                }
            }
        }

        // Collect expired IPv6 entries
        {
            let mut entries = self.ipv6_entries.write().await;
            let before = entries.len();
            entries.retain(|_, entry| entry.expires_at > now);
            let removed = before - entries.len();
            expired_count += removed;
        }

        // Update stats
        if expired_count > 0 {
            let mut stats = self.stats.write().await;
            stats.total_expirations += expired_count as u64;
            self.update_entry_counts(&mut stats).await;
        }

        // Notify eBPF about removals
        if let Some(callback) = &self.ebpf_callback {
            for ip in expired_ips {
                callback(&ip, false);
            }
        }

        expired_count
    }

    /// Record a block event (for statistics)
    pub async fn record_block(&self) {
        let mut stats = self.stats.write().await;
        stats.total_blocks += 1;
    }

    /// Get statistics
    pub async fn get_stats(&self) -> BlocklistStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Get all blocked IPs (for synchronization)
    pub async fn get_all_entries(&self) -> Vec<BlocklistEntry> {
        let mut entries = Vec::new();

        {
            let ipv4 = self.ipv4_entries.read().await;
            entries.extend(ipv4.values().cloned());
        }

        {
            let ipv6 = self.ipv6_entries.read().await;
            entries.extend(ipv6.values().cloned());
        }

        entries
    }

    async fn update_entry_counts(&self, stats: &mut BlocklistStats) {
        let ipv4_count = self.ipv4_entries.read().await.len();
        let ipv6_count = self.ipv6_entries.read().await.len();
        stats.ipv4_entries = ipv4_count;
        stats.ipv6_entries = ipv6_count;
        stats.total_entries = ipv4_count + ipv6_count;
    }
}

impl Default for GlobalBlocklist {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// Distributed Trust Score Sharing
// ============================================

/// Verified trust token that can be shared across nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustToken {
    /// Client identifier (fingerprint hash or IP)
    pub client_id: String,
    /// Trust score (0-100)
    pub trust_score: u8,
    /// Challenge type that was completed
    pub challenge_type: ChallengeType,
    /// Node that verified the challenge
    pub verifying_node: String,
    /// Verification timestamp
    pub verified_at_ms: u64,
    /// Token expiration (Unix timestamp ms)
    pub expires_at_ms: u64,
    /// Ed25519 signature from verifying node
    pub signature: String,
}

/// Type of challenge completed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeType {
    /// Invisible JavaScript challenge
    Invisible,
    /// Managed challenge (browser checks)
    Managed,
    /// Interactive challenge (captcha-like)
    Interactive,
    /// Behavioral analysis passed
    Behavioral,
}

impl TrustToken {
    /// Create a new trust token
    pub fn new(
        client_id: String,
        trust_score: u8,
        challenge_type: ChallengeType,
        verifying_node: String,
        ttl_seconds: u64,
    ) -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            client_id,
            trust_score: trust_score.min(100),
            challenge_type,
            verifying_node,
            verified_at_ms: now_ms,
            expires_at_ms: now_ms + (ttl_seconds * 1000),
            signature: String::new(),
        }
    }

    /// Sign the token
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let payload = self.signing_payload();
        let signature = signing_key.sign(payload.as_bytes());
        self.signature = STANDARD.encode(signature.to_bytes());
    }

    fn signing_payload(&self) -> String {
        format!(
            "{}:{}:{:?}:{}:{}:{}",
            self.client_id,
            self.trust_score,
            self.challenge_type,
            self.verifying_node,
            self.verified_at_ms,
            self.expires_at_ms
        )
    }

    /// Verify the token signature
    pub fn verify(&self, public_key: &VerifyingKey) -> bool {
        let Ok(sig_bytes) = STANDARD.decode(&self.signature) else {
            return false;
        };

        let Ok(sig_array): Result<[u8; 64], _> = sig_bytes.try_into() else {
            return false;
        };

        let signature = Signature::from_bytes(&sig_array);
        let payload = self.signing_payload();

        public_key.verify(payload.as_bytes(), &signature).is_ok()
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        now_ms > self.expires_at_ms
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Distributed trust score cache
///
/// # Security Note (Y6.7)
/// This cache now tracks revoked tokens and nodes to prevent
/// compromised credentials from being reused.
#[derive(Debug)]
pub struct TrustScoreCache {
    /// Cached trust tokens by client ID
    tokens: Arc<RwLock<HashMap<String, TrustToken>>>,
    /// Known node public keys for verification
    node_public_keys: Arc<RwLock<HashMap<String, VerifyingKey>>>,
    /// Default trust score for unknown clients
    default_trust_score: u8,
    /// Minimum trust score to skip challenge
    skip_challenge_threshold: u8,
    /// Y6.7: Revoked client IDs (with revocation timestamp)
    revoked_clients: Arc<RwLock<HashMap<String, u64>>>,
    /// Y6.7: Revoked node public keys
    revoked_nodes: Arc<RwLock<HashSet<String>>>,
}

impl TrustScoreCache {
    pub fn new(default_trust_score: u8, skip_challenge_threshold: u8) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            node_public_keys: Arc::new(RwLock::new(HashMap::new())),
            default_trust_score,
            skip_challenge_threshold,
            revoked_clients: Arc::new(RwLock::new(HashMap::new())),
            revoked_nodes: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Add a node's public key for verification
    pub async fn add_node_public_key(&self, node_id: &str, public_key: VerifyingKey) {
        let mut keys = self.node_public_keys.write().await;
        keys.insert(node_id.to_string(), public_key);
    }

    /// Store a trust token (after verification)
    ///
    /// # Security Note (Y5.5)
    /// This method REQUIRES the verifying node's public key to be pre-registered.
    /// Tokens from unknown nodes are REJECTED. There is no "bootstrap mode" that
    /// trusts tokens without verification - this prevents token spoofing attacks.
    pub async fn store_token(&self, token: TrustToken) -> Result<(), String> {
        // Check if expired
        if token.is_expired() {
            return Err("Token is expired".to_string());
        }

        // 🔒 SECURITY FIX (Y5.5): Require node key - no bootstrap mode
        // Verify signature - REQUIRE the node's public key
        let keys = self.node_public_keys.read().await;
        let public_key = keys.get(&token.verifying_node).ok_or_else(|| {
            format!(
                "Unknown verifying node: {}. Register node public key before accepting tokens.",
                token.verifying_node
            )
        })?;

        if !token.verify(public_key) {
            return Err("Invalid signature".to_string());
        }
        drop(keys);

        // Store token
        let mut tokens = self.tokens.write().await;
        tokens.insert(token.client_id.clone(), token);

        Ok(())
    }

    /// Get trust score for a client
    pub async fn get_trust_score(&self, client_id: &str) -> u8 {
        let tokens = self.tokens.read().await;

        if let Some(token) = tokens.get(client_id) {
            if !token.is_expired() {
                return token.trust_score;
            }
        }

        self.default_trust_score
    }

    /// Check if client should skip challenge
    pub async fn should_skip_challenge(&self, client_id: &str) -> bool {
        self.get_trust_score(client_id).await >= self.skip_challenge_threshold
    }

    /// Get token for a client
    pub async fn get_token(&self, client_id: &str) -> Option<TrustToken> {
        let tokens = self.tokens.read().await;
        tokens.get(client_id).cloned()
    }

    /// Remove expired tokens
    pub async fn cleanup_expired(&self) -> usize {
        let mut tokens = self.tokens.write().await;
        let before = tokens.len();
        tokens.retain(|_, token| !token.is_expired());
        before - tokens.len()
    }

    /// Get all valid tokens (for synchronization)
    pub async fn get_all_tokens(&self) -> Vec<TrustToken> {
        let tokens = self.tokens.read().await;
        tokens
            .values()
            .filter(|t| !t.is_expired())
            .cloned()
            .collect()
    }

    // ========================================
    // Y6.7: Token/Node Revocation Methods
    // ========================================

    /// Revoke a client's trust token
    ///
    /// After revocation, the client will need to complete a new challenge
    /// to regain trust status.
    pub async fn revoke_client(&self, client_id: &str, revoked_at: u64) {
        // Remove existing token
        let mut tokens = self.tokens.write().await;
        tokens.remove(client_id);
        drop(tokens);

        // Add to revoked list
        let mut revoked = self.revoked_clients.write().await;
        revoked.insert(client_id.to_string(), revoked_at);

        info!("Revoked trust token for client: {}", client_id);
    }

    /// Check if a client's token has been revoked
    pub async fn is_client_revoked(&self, client_id: &str) -> bool {
        let revoked = self.revoked_clients.read().await;
        revoked.contains_key(client_id)
    }

    /// Revoke a node (mark as untrusted)
    ///
    /// After revocation, messages from this node will be rejected.
    pub async fn revoke_node(&self, public_key: &str) {
        // Remove from trusted keys if present
        let mut keys = self.node_public_keys.write().await;
        keys.retain(|_, v| hex::encode(v.as_bytes()) != public_key);
        drop(keys);

        // Add to revoked list
        let mut revoked = self.revoked_nodes.write().await;
        revoked.insert(public_key.to_string());

        info!("Revoked node with public key: {}...", &public_key[..16.min(public_key.len())]);
    }

    /// Check if a node has been revoked
    pub async fn is_node_revoked(&self, public_key: &str) -> bool {
        let revoked = self.revoked_nodes.read().await;
        revoked.contains(public_key)
    }

    /// Get count of revoked clients
    pub async fn revoked_client_count(&self) -> usize {
        let revoked = self.revoked_clients.read().await;
        revoked.len()
    }

    /// Get count of revoked nodes
    pub async fn revoked_node_count(&self) -> usize {
        let revoked = self.revoked_nodes.read().await;
        revoked.len()
    }
}

impl Default for TrustScoreCache {
    fn default() -> Self {
        Self::new(50, 60) // Default score 50, skip challenge at 60+
    }
}

// ============================================
// Coordinated Challenge System
// ============================================

/// Challenge completion record shared across nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeCompletion {
    /// Client fingerprint or IP
    pub client_id: String,
    /// Challenge type completed
    pub challenge_type: ChallengeType,
    /// Node that issued and verified the challenge
    pub node_id: String,
    /// Completion timestamp
    pub completed_at_ms: u64,
    /// Result trust score
    pub trust_score: u8,
    /// TTL in seconds
    pub ttl_seconds: u64,
    /// Signature
    pub signature: String,
}

impl ChallengeCompletion {
    /// Create a new challenge completion record
    pub fn new(
        client_id: String,
        challenge_type: ChallengeType,
        node_id: String,
        trust_score: u8,
        ttl_seconds: u64,
    ) -> Self {
        let completed_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            client_id,
            challenge_type,
            node_id,
            completed_at_ms,
            trust_score,
            ttl_seconds,
            signature: String::new(),
        }
    }

    /// Sign the completion record
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let payload = format!(
            "{}:{:?}:{}:{}:{}:{}",
            self.client_id,
            self.challenge_type,
            self.node_id,
            self.completed_at_ms,
            self.trust_score,
            self.ttl_seconds
        );
        let signature = signing_key.sign(payload.as_bytes());
        self.signature = STANDARD.encode(signature.to_bytes());
    }

    /// Check if expired
    pub fn is_expired(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        now_ms > self.completed_at_ms + (self.ttl_seconds * 1000)
    }
}

/// Coordinated challenge manager
#[derive(Debug)]
pub struct CoordinatedChallengeManager {
    /// Completed challenges by client ID
    completions: Arc<RwLock<HashMap<String, ChallengeCompletion>>>,
    /// Challenge in progress by client ID
    in_progress: Arc<RwLock<HashMap<String, (ChallengeType, Instant)>>>,
    /// Challenge timeout
    challenge_timeout: Duration,
    /// TTL for completed challenges
    completion_ttl_seconds: u64,
}

impl CoordinatedChallengeManager {
    pub fn new(challenge_timeout_seconds: u64, completion_ttl_seconds: u64) -> Self {
        Self {
            completions: Arc::new(RwLock::new(HashMap::new())),
            in_progress: Arc::new(RwLock::new(HashMap::new())),
            challenge_timeout: Duration::from_secs(challenge_timeout_seconds),
            completion_ttl_seconds,
        }
    }

    /// Check if client has completed a challenge recently
    pub async fn has_completed_challenge(&self, client_id: &str) -> Option<ChallengeCompletion> {
        let completions = self.completions.read().await;

        if let Some(completion) = completions.get(client_id) {
            if !completion.is_expired() {
                return Some(completion.clone());
            }
        }

        None
    }

    /// Check if client needs a challenge
    pub async fn needs_challenge(&self, client_id: &str) -> bool {
        // Check if already completed
        if self.has_completed_challenge(client_id).await.is_some() {
            return false;
        }

        // Check if challenge in progress
        let in_progress = self.in_progress.read().await;
        if let Some((_, started)) = in_progress.get(client_id) {
            // If challenge timed out, need new one
            if started.elapsed() < self.challenge_timeout {
                return false; // Challenge in progress
            }
        }

        true
    }

    /// Start a challenge for a client
    pub async fn start_challenge(&self, client_id: &str, challenge_type: ChallengeType) {
        let mut in_progress = self.in_progress.write().await;
        in_progress.insert(client_id.to_string(), (challenge_type, Instant::now()));
    }

    /// Record a completed challenge
    pub async fn record_completion(&self, completion: ChallengeCompletion) {
        // Remove from in-progress
        {
            let mut in_progress = self.in_progress.write().await;
            in_progress.remove(&completion.client_id);
        }

        // Add to completions
        {
            let mut completions = self.completions.write().await;
            completions.insert(completion.client_id.clone(), completion);
        }
    }

    /// Import completion from another node
    pub async fn import_completion(&self, completion: ChallengeCompletion) {
        if !completion.is_expired() {
            let mut completions = self.completions.write().await;
            completions.insert(completion.client_id.clone(), completion);
        }
    }

    /// Cleanup expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut removed = 0;

        // Cleanup completions
        {
            let mut completions = self.completions.write().await;
            let before = completions.len();
            completions.retain(|_, c| !c.is_expired());
            removed += before - completions.len();
        }

        // Cleanup timed out in-progress
        {
            let mut in_progress = self.in_progress.write().await;
            let before = in_progress.len();
            in_progress.retain(|_, (_, started)| started.elapsed() < self.challenge_timeout);
            removed += before - in_progress.len();
        }

        removed
    }

    /// Get all completions for synchronization
    pub async fn get_all_completions(&self) -> Vec<ChallengeCompletion> {
        let completions = self.completions.read().await;
        completions
            .values()
            .filter(|c| !c.is_expired())
            .cloned()
            .collect()
    }
}

impl Default for CoordinatedChallengeManager {
    fn default() -> Self {
        Self::new(300, 900) // 5 minute timeout, 15 minute TTL
    }
}

// ============================================
// eBPF Integration Interface
// ============================================

/// Interface for eBPF blocklist updates
pub trait EbpfBlocklistUpdater: Send + Sync {
    /// Add IP to eBPF blocklist map
    fn add_to_blocklist(&self, ip: &ThreatIpAddress, block_info: BlockInfo) -> Result<(), String>;

    /// Remove IP from eBPF blocklist map
    fn remove_from_blocklist(&self, ip: &ThreatIpAddress) -> Result<(), String>;

    /// Check if eBPF is available
    fn is_available(&self) -> bool;
}

/// Block information for eBPF map
#[derive(Debug, Clone, Copy)]
pub struct BlockInfo {
    /// Block expiration time (Unix timestamp)
    pub expires_at: u64,
    /// Threat type (encoded as u8)
    pub threat_type: u8,
    /// Severity
    pub severity: u8,
}

impl BlockInfo {
    /// Convert to bytes for eBPF map value
    pub fn to_bytes(&self) -> [u8; 10] {
        let mut bytes = [0u8; 10];
        bytes[0..8].copy_from_slice(&self.expires_at.to_le_bytes());
        bytes[8] = self.threat_type;
        bytes[9] = self.severity;
        bytes
    }
}

/// Mock eBPF updater for testing/non-Linux systems
#[derive(Debug, Default)]
pub struct MockEbpfUpdater {
    blocked_v4: std::sync::Arc<std::sync::RwLock<HashMap<Ipv4Addr, BlockInfo>>>,
    blocked_v6: std::sync::Arc<std::sync::RwLock<HashMap<Ipv6Addr, BlockInfo>>>,
}

impl MockEbpfUpdater {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_blocked_count(&self) -> usize {
        // SECURITY FIX (X2.6): Use lock recovery to prevent panics
        let v4 = read_lock_or_recover(&self.blocked_v4, "mock eBPF blocklist v4").len();
        let v6 = read_lock_or_recover(&self.blocked_v6, "mock eBPF blocklist v6").len();
        v4 + v6
    }
}

impl EbpfBlocklistUpdater for MockEbpfUpdater {
    fn add_to_blocklist(&self, ip: &ThreatIpAddress, block_info: BlockInfo) -> Result<(), String> {
        // SECURITY FIX (X2.6): Use lock recovery to prevent panics
        match ip {
            ThreatIpAddress::V4(v4) => {
                let mut blocked = write_lock_or_recover(&self.blocked_v4, "mock eBPF blocklist v4");
                blocked.insert(*v4, block_info);
            }
            ThreatIpAddress::V6(v6) => {
                let mut blocked = write_lock_or_recover(&self.blocked_v6, "mock eBPF blocklist v6");
                blocked.insert(*v6, block_info);
            }
        }
        Ok(())
    }

    fn remove_from_blocklist(&self, ip: &ThreatIpAddress) -> Result<(), String> {
        // SECURITY FIX (X2.6): Use lock recovery to prevent panics
        match ip {
            ThreatIpAddress::V4(v4) => {
                let mut blocked = write_lock_or_recover(&self.blocked_v4, "mock eBPF blocklist v4");
                blocked.remove(v4);
            }
            ThreatIpAddress::V6(v6) => {
                let mut blocked = write_lock_or_recover(&self.blocked_v6, "mock eBPF blocklist v6");
                blocked.remove(v6);
            }
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }
}

// ============================================
// Distributed Enforcement Engine
// ============================================

/// P2P message types for distributed enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnforcementMessage {
    /// Threat intelligence report
    ThreatIntel(EnhancedThreatIntel),
    /// Trust token sharing
    TrustToken(TrustToken),
    /// Challenge completion
    ChallengeComplete(ChallengeCompletion),
    /// Blocklist sync request
    BlocklistSyncRequest { node_id: String },
    /// Blocklist sync response
    BlocklistSyncResponse {
        node_id: String,
        entries: Vec<EnhancedThreatIntel>,
    },
    /// Y6.8: Revoke a specific trust token
    RevokeToken(TokenRevocation),
    /// Y6.8: Revoke a node entirely (mark as untrusted)
    RevokeNode(NodeRevocation),
}

/// Y6.7-Y6.8: Token revocation message
///
/// Allows revoking a specific trust token, preventing replay attacks
/// where a compromised token might be reused after being invalidated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRevocation {
    /// The client ID whose token is being revoked
    pub client_id: String,
    /// Reason for revocation
    pub reason: RevocationReason,
    /// Timestamp of revocation (for ordering)
    pub revoked_at: u64,
    /// Node ID that issued the revocation
    pub issuing_node: String,
    /// Ed25519 signature of the revocation
    pub signature: Option<String>,
}

/// Y6.7-Y6.8: Node revocation message
///
/// Allows revoking an entire node's trust status, typically used when
/// a node is compromised or behaving maliciously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRevocation {
    /// The node ID being revoked
    pub node_id: String,
    /// The node's public key (to prevent future messages)
    pub public_key: String,
    /// Reason for revocation
    pub reason: RevocationReason,
    /// Timestamp of revocation
    pub revoked_at: u64,
    /// Node ID that issued the revocation
    pub issuing_node: String,
    /// Ed25519 signature of the revocation
    pub signature: Option<String>,
}

/// Reason for revocation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RevocationReason {
    /// Token/node was compromised
    Compromised,
    /// Malicious behavior detected
    MaliciousBehavior,
    /// Node went offline permanently
    Offline,
    /// Administrative revocation
    Administrative,
    /// Stake slashed (for nodes)
    StakeSlashed,
    /// Custom reason
    Other(String),
}

impl EnforcementMessage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Configuration for distributed enforcement
#[derive(Debug, Clone)]
pub struct DistributedEnforcementConfig {
    /// Node ID (peer ID)
    pub node_id: String,
    /// Signing key for this node
    pub signing_key: Option<SigningKey>,
    /// Default trust score for unknown clients
    pub default_trust_score: u8,
    /// Trust score threshold to skip challenge
    pub skip_challenge_threshold: u8,
    /// Challenge timeout in seconds
    pub challenge_timeout_seconds: u64,
    /// Challenge completion TTL in seconds
    pub completion_ttl_seconds: u64,
    /// Enable eBPF integration
    pub enable_ebpf: bool,
}

impl Default for DistributedEnforcementConfig {
    fn default() -> Self {
        Self {
            node_id: "unknown".to_string(),
            signing_key: None,
            default_trust_score: 50,
            skip_challenge_threshold: 60,
            challenge_timeout_seconds: 300,
            completion_ttl_seconds: 900,
            enable_ebpf: true,
        }
    }
}

/// Main distributed enforcement engine
pub struct DistributedEnforcementEngine {
    config: DistributedEnforcementConfig,
    blocklist: GlobalBlocklist,
    trust_cache: TrustScoreCache,
    challenge_manager: CoordinatedChallengeManager,
    ebpf_updater: Option<Arc<dyn EbpfBlocklistUpdater>>,
}

impl DistributedEnforcementEngine {
    pub fn new(config: DistributedEnforcementConfig) -> Self {
        let trust_cache =
            TrustScoreCache::new(config.default_trust_score, config.skip_challenge_threshold);
        let challenge_manager = CoordinatedChallengeManager::new(
            config.challenge_timeout_seconds,
            config.completion_ttl_seconds,
        );

        Self {
            config,
            blocklist: GlobalBlocklist::new(),
            trust_cache,
            challenge_manager,
            ebpf_updater: None,
        }
    }

    /// Set eBPF updater
    pub fn set_ebpf_updater(&mut self, updater: Arc<dyn EbpfBlocklistUpdater>) {
        self.ebpf_updater = Some(updater);
    }

    /// Process incoming P2P message
    ///
    /// # Security Note (Y6.7-Y6.8)
    /// This method now handles revocation messages for tokens and nodes.
    pub async fn process_message(&self, message: EnforcementMessage) -> Result<(), String> {
        match message {
            EnforcementMessage::ThreatIntel(threat) => {
                self.handle_threat_intel(threat).await?;
            }
            EnforcementMessage::TrustToken(token) => {
                // Y6.7: Check if client or issuing node is revoked
                if self.trust_cache.is_client_revoked(&token.client_id).await {
                    return Err(format!("Client {} is revoked", token.client_id));
                }
                self.trust_cache.store_token(token).await?;
            }
            EnforcementMessage::ChallengeComplete(completion) => {
                self.challenge_manager.import_completion(completion).await;
            }
            EnforcementMessage::BlocklistSyncRequest { .. } => {
                // This should trigger a sync response (handled by caller)
            }
            EnforcementMessage::BlocklistSyncResponse { entries, .. } => {
                for entry in entries {
                    if entry.validate().is_ok() && !entry.is_expired() {
                        self.blocklist.add(&entry).await;
                    }
                }
            }
            // Y6.8: Handle token revocation
            EnforcementMessage::RevokeToken(revocation) => {
                self.handle_token_revocation(revocation).await?;
            }
            // Y6.8: Handle node revocation
            EnforcementMessage::RevokeNode(revocation) => {
                self.handle_node_revocation(revocation).await?;
            }
        }

        Ok(())
    }

    /// Y6.7: Handle token revocation message
    async fn handle_token_revocation(&self, revocation: TokenRevocation) -> Result<(), String> {
        // TODO: Verify signature of revocation message
        // For now, accept revocations from any source (should be restricted)

        info!(
            "Processing token revocation for client {} (reason: {:?}, from: {})",
            revocation.client_id, revocation.reason, revocation.issuing_node
        );

        self.trust_cache
            .revoke_client(&revocation.client_id, revocation.revoked_at)
            .await;

        Ok(())
    }

    /// Y6.7: Handle node revocation message
    async fn handle_node_revocation(&self, revocation: NodeRevocation) -> Result<(), String> {
        // TODO: Verify signature of revocation message
        // TODO: Require quorum of nodes for node revocation

        warn!(
            "Processing node revocation for {} (reason: {:?}, from: {})",
            &revocation.public_key[..16.min(revocation.public_key.len())],
            revocation.reason,
            revocation.issuing_node
        );

        self.trust_cache.revoke_node(&revocation.public_key).await;

        Ok(())
    }

    /// Handle incoming threat intelligence
    async fn handle_threat_intel(&self, threat: EnhancedThreatIntel) -> Result<(), String> {
        // Validate
        threat.validate()?;

        // Check if expired
        if threat.is_expired() {
            return Err("Threat intel is expired".to_string());
        }

        // Add to blocklist
        self.blocklist.add(&threat).await;

        // Update eBPF if available
        if let Some(updater) = &self.ebpf_updater {
            let block_info = BlockInfo {
                expires_at: threat.timestamp_ms / 1000 + threat.block_duration_secs,
                threat_type: threat.threat_type as u8,
                severity: threat.severity,
            };
            let _ = updater.add_to_blocklist(&threat.ip, block_info);
        }

        Ok(())
    }

    /// Report a threat (to be broadcast to P2P network)
    pub fn create_threat_report(
        &self,
        ip: ThreatIpAddress,
        threat_type: ThreatType,
        severity: u8,
        block_duration_secs: u64,
        description: Option<String>,
    ) -> EnhancedThreatIntel {
        let mut threat = EnhancedThreatIntel::new(
            ip,
            threat_type,
            severity,
            block_duration_secs,
            self.config.node_id.clone(),
        );

        if let Some(desc) = description {
            threat = threat.with_description(desc);
        }

        // Sign if we have a key
        if let Some(signing_key) = &self.config.signing_key {
            threat.sign(signing_key);
        }

        threat
    }

    /// Check if an IP should be blocked
    pub async fn should_block(&self, ip_str: &str) -> bool {
        self.blocklist.is_blocked_str(ip_str).await
    }

    /// Check if client should be challenged
    pub async fn should_challenge(&self, client_id: &str) -> bool {
        // Check trust score first
        if self.trust_cache.should_skip_challenge(client_id).await {
            return false;
        }

        // Check if already challenged
        self.challenge_manager.needs_challenge(client_id).await
    }

    /// Create a trust token for a verified client
    pub fn create_trust_token(
        &self,
        client_id: String,
        trust_score: u8,
        challenge_type: ChallengeType,
        ttl_seconds: u64,
    ) -> TrustToken {
        let mut token = TrustToken::new(
            client_id,
            trust_score,
            challenge_type,
            self.config.node_id.clone(),
            ttl_seconds,
        );

        if let Some(signing_key) = &self.config.signing_key {
            token.sign(signing_key);
        }

        token
    }

    /// Record a challenge completion
    pub async fn record_challenge_completion(
        &self,
        client_id: String,
        challenge_type: ChallengeType,
        trust_score: u8,
    ) -> ChallengeCompletion {
        let mut completion = ChallengeCompletion::new(
            client_id,
            challenge_type,
            self.config.node_id.clone(),
            trust_score,
            self.config.completion_ttl_seconds,
        );

        if let Some(signing_key) = &self.config.signing_key {
            completion.sign(signing_key);
        }

        self.challenge_manager.record_completion(completion.clone()).await;

        completion
    }

    /// Get blocklist for synchronization
    pub async fn get_blocklist_for_sync(&self) -> Vec<EnhancedThreatIntel> {
        let entries = self.blocklist.get_all_entries().await;
        let now = Instant::now();

        entries
            .into_iter()
            .filter(|e| e.expires_at > now)
            .map(|e| {
                let remaining_secs = e.expires_at.duration_since(now).as_secs();
                EnhancedThreatIntel::new(
                    e.ip,
                    e.threat_type,
                    e.severity,
                    remaining_secs,
                    e.source_node,
                )
            })
            .collect()
    }

    /// Cleanup expired entries
    pub async fn cleanup(&self) -> (usize, usize, usize) {
        let blocklist_cleaned = self.blocklist.cleanup_expired().await;
        let trust_cleaned = self.trust_cache.cleanup_expired().await;
        let challenge_cleaned = self.challenge_manager.cleanup_expired().await;
        (blocklist_cleaned, trust_cleaned, challenge_cleaned)
    }

    /// Get statistics
    pub async fn get_stats(&self) -> BlocklistStats {
        self.blocklist.get_stats().await
    }

    /// Get access to blocklist
    pub fn blocklist(&self) -> &GlobalBlocklist {
        &self.blocklist
    }

    /// Get access to trust cache
    pub fn trust_cache(&self) -> &TrustScoreCache {
        &self.trust_cache
    }

    /// Get access to challenge manager
    pub fn challenge_manager(&self) -> &CoordinatedChallengeManager {
        &self.challenge_manager
    }
}

impl Default for DistributedEnforcementEngine {
    fn default() -> Self {
        Self::new(DistributedEnforcementConfig::default())
    }
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    /// Helper to generate a random signing key for tests
    fn generate_test_signing_key() -> SigningKey {
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        SigningKey::from_bytes(&secret_key_bytes)
    }

    // IPv6 Support Tests

    #[test]
    fn test_threat_ip_address_parsing() {
        // IPv4
        let v4 = ThreatIpAddress::parse("192.168.1.1").unwrap();
        assert!(v4.is_ipv4());
        assert_eq!(v4.version(), 4);

        // IPv6
        let v6 = ThreatIpAddress::parse("2001:db8::1").unwrap();
        assert!(v6.is_ipv6());
        assert_eq!(v6.version(), 6);

        // Full IPv6
        let v6_full = ThreatIpAddress::parse("2001:0db8:85a3:0000:0000:8a2e:0370:7334").unwrap();
        assert!(v6_full.is_ipv6());

        // Invalid
        assert!(ThreatIpAddress::parse("invalid").is_err());
    }

    #[test]
    fn test_threat_ip_to_bytes() {
        let v4 = ThreatIpAddress::parse("192.168.1.1").unwrap();
        let bytes = v4.to_bytes();
        assert_eq!(bytes.len(), 4);
        assert_eq!(bytes, vec![192, 168, 1, 1]);

        let v6 = ThreatIpAddress::parse("::1").unwrap();
        let bytes = v6.to_bytes();
        assert_eq!(bytes.len(), 16);
    }

    // Enhanced Threat Intel Tests

    #[test]
    fn test_enhanced_threat_intel_creation() {
        let threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            "node-1".to_string(),
        );

        assert_eq!(threat.severity, 8);
        assert!(threat.validate().is_ok());
        assert!(!threat.is_expired());
    }

    #[test]
    fn test_enhanced_threat_intel_validation() {
        // Invalid severity
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::BruteForce,
            15, // Will be clamped to 10
            3600,
            "node-1".to_string(),
        );
        threat.severity = 0; // Force invalid
        assert!(threat.validate().is_err());

        // Invalid block duration
        let mut threat2 = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::BruteForce,
            5,
            100000, // > 24 hours
            "node-1".to_string(),
        );
        threat2.block_duration_secs = 100000;
        assert!(threat2.validate().is_err());
    }

    #[test]
    fn test_enhanced_threat_intel_signing() {
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();

        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("10.0.0.1").unwrap(),
            ThreatType::HttpFlood,
            7,
            1800,
            "node-1".to_string(),
        );

        threat.sign(&signing_key);
        assert!(threat.signature.is_some());
        assert!(threat.verify_signature(&verifying_key));

        // Tamper with data - should fail verification
        let mut tampered = threat.clone();
        tampered.severity = 1;
        assert!(!tampered.verify_signature(&verifying_key));
    }

    // Global Blocklist Tests

    #[tokio::test]
    async fn test_global_blocklist_add_remove() {
        let blocklist = GlobalBlocklist::new();

        let threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SqlInjection,
            6,
            3600,
            "node-1".to_string(),
        );

        blocklist.add(&threat).await;
        assert!(blocklist.is_blocked(&threat.ip).await);

        blocklist.remove(&threat.ip).await;
        assert!(!blocklist.is_blocked(&threat.ip).await);
    }

    #[tokio::test]
    async fn test_global_blocklist_ipv6() {
        let blocklist = GlobalBlocklist::new();

        let threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("2001:db8::1").unwrap(),
            ThreatType::Scanner,
            5,
            3600,
            "node-1".to_string(),
        );

        blocklist.add(&threat).await;
        assert!(blocklist.is_blocked_str("2001:db8::1").await);
        assert!(!blocklist.is_blocked_str("2001:db8::2").await);
    }

    #[tokio::test]
    async fn test_blocklist_stats() {
        let blocklist = GlobalBlocklist::new();

        // Add IPv4
        blocklist
            .add(&EnhancedThreatIntel::new(
                ThreatIpAddress::parse("192.168.1.1").unwrap(),
                ThreatType::Bot,
                3,
                3600,
                "node-1".to_string(),
            ))
            .await;

        // Add IPv6
        blocklist
            .add(&EnhancedThreatIntel::new(
                ThreatIpAddress::parse("::1").unwrap(),
                ThreatType::Bot,
                3,
                3600,
                "node-1".to_string(),
            ))
            .await;

        let stats = blocklist.get_stats().await;
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.ipv4_entries, 1);
        assert_eq!(stats.ipv6_entries, 1);
        assert_eq!(stats.total_additions, 2);
    }

    // Trust Token Tests

    #[test]
    fn test_trust_token_creation_and_signing() {
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();

        let mut token = TrustToken::new(
            "client-fingerprint-123".to_string(),
            75,
            ChallengeType::Invisible,
            "node-1".to_string(),
            900,
        );

        token.sign(&signing_key);
        assert!(token.verify(&verifying_key));
        assert!(!token.is_expired());
    }

    #[tokio::test]
    async fn test_trust_score_cache() {
        let cache = TrustScoreCache::new(50, 60);

        // Unknown client should get default score
        assert_eq!(cache.get_trust_score("unknown").await, 50);
        assert!(!cache.should_skip_challenge("unknown").await);

        // Create and register node key (Y5.5: no bootstrap mode)
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();
        let node_id = "node-1";
        cache.add_node_public_key(node_id, verifying_key).await;

        // Store a properly signed token
        let mut token = TrustToken::new(
            "client-1".to_string(),
            80,
            ChallengeType::Managed,
            node_id.to_string(),
            900,
        );
        token.sign(&signing_key);

        cache.store_token(token).await.unwrap();

        assert_eq!(cache.get_trust_score("client-1").await, 80);
        assert!(cache.should_skip_challenge("client-1").await);
    }

    #[tokio::test]
    async fn test_trust_score_cache_rejects_unknown_node() {
        let cache = TrustScoreCache::new(50, 60);

        // Try to store a token from unknown node (no registered key)
        let token = TrustToken::new(
            "client-1".to_string(),
            80,
            ChallengeType::Managed,
            "unknown-node".to_string(),
            900,
        );

        // Y5.5: Should reject tokens from unknown nodes
        let result = cache.store_token(token).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown verifying node"));
    }

    // Coordinated Challenge Tests

    #[tokio::test]
    async fn test_coordinated_challenge_manager() {
        let manager = CoordinatedChallengeManager::new(300, 900);

        // New client needs challenge
        assert!(manager.needs_challenge("client-1").await);

        // Start challenge
        manager
            .start_challenge("client-1", ChallengeType::Interactive)
            .await;

        // Challenge in progress - doesn't need new one
        assert!(!manager.needs_challenge("client-1").await);

        // Complete challenge
        let completion = ChallengeCompletion::new(
            "client-1".to_string(),
            ChallengeType::Interactive,
            "node-1".to_string(),
            85,
            900,
        );
        manager.record_completion(completion).await;

        // Now has completed - doesn't need challenge
        assert!(!manager.needs_challenge("client-1").await);
        assert!(manager.has_completed_challenge("client-1").await.is_some());
    }

    // Enforcement Message Tests

    #[test]
    fn test_enforcement_message_serialization() {
        let threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("10.0.0.1").unwrap(),
            ThreatType::CredentialStuffing,
            7,
            3600,
            "node-1".to_string(),
        );

        let msg = EnforcementMessage::ThreatIntel(threat);
        let json = msg.to_json().unwrap();

        let parsed = EnforcementMessage::from_json(&json).unwrap();
        match parsed {
            EnforcementMessage::ThreatIntel(t) => {
                assert_eq!(t.severity, 7);
            }
            _ => panic!("Wrong message type"),
        }
    }

    // Distributed Enforcement Engine Tests

    #[tokio::test]
    async fn test_distributed_enforcement_engine() {
        let config = DistributedEnforcementConfig {
            node_id: "test-node".to_string(),
            signing_key: Some(generate_test_signing_key()),
            ..Default::default()
        };

        let engine = DistributedEnforcementEngine::new(config);

        // Report a threat
        let threat = engine.create_threat_report(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::BruteForce,
            8,
            3600,
            Some("Multiple failed login attempts".to_string()),
        );

        // Process the threat
        let msg = EnforcementMessage::ThreatIntel(threat.clone());
        engine.process_message(msg).await.unwrap();

        // IP should be blocked
        assert!(engine.should_block("192.168.1.100").await);
        assert!(!engine.should_block("192.168.1.101").await);
    }

    #[tokio::test]
    async fn test_challenge_flow() {
        let engine = DistributedEnforcementEngine::default();

        // New client should be challenged
        assert!(engine.should_challenge("client-1").await);

        // Record completion
        let completion = engine
            .record_challenge_completion(
                "client-1".to_string(),
                ChallengeType::Invisible,
                70,
            )
            .await;

        // Should not be challenged again
        assert!(!engine.should_challenge("client-1").await);

        // Completion should be retrievable
        assert!(engine
            .challenge_manager()
            .has_completed_challenge("client-1")
            .await
            .is_some());
    }

    #[tokio::test]
    async fn test_trust_token_flow() {
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();
        let config = DistributedEnforcementConfig {
            node_id: "test-node".to_string(),
            signing_key: Some(signing_key),
            skip_challenge_threshold: 60,
            ..Default::default()
        };

        let engine = DistributedEnforcementEngine::new(config);

        // Y5.5: Register node's public key before processing tokens
        engine
            .trust_cache
            .add_node_public_key("test-node", verifying_key)
            .await;

        // Create and store trust token
        let token = engine.create_trust_token(
            "client-1".to_string(),
            80,
            ChallengeType::Managed,
            900,
        );

        // Process token message
        let msg = EnforcementMessage::TrustToken(token);
        engine.process_message(msg).await.unwrap();

        // Client should skip challenge (trust score 80 > threshold 60)
        assert!(!engine.should_challenge("client-1").await);
    }

    // Mock eBPF Updater Tests

    #[test]
    fn test_mock_ebpf_updater() {
        let updater = MockEbpfUpdater::new();

        let ip = ThreatIpAddress::parse("192.168.1.1").unwrap();
        let block_info = BlockInfo {
            expires_at: 9999999999,
            threat_type: 1,
            severity: 5,
        };

        updater.add_to_blocklist(&ip, block_info).unwrap();
        assert_eq!(updater.get_blocked_count(), 1);

        updater.remove_from_blocklist(&ip).unwrap();
        assert_eq!(updater.get_blocked_count(), 0);
    }

    #[test]
    fn test_block_info_to_bytes() {
        let info = BlockInfo {
            expires_at: 1234567890,
            threat_type: 5,
            severity: 8,
        };

        let bytes = info.to_bytes();
        assert_eq!(bytes.len(), 10);

        // Verify timestamp bytes (little-endian)
        let ts = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        assert_eq!(ts, 1234567890);
        assert_eq!(bytes[8], 5);
        assert_eq!(bytes[9], 8);
    }

    // ============================================
    // Sprint 29: Verified Blocklist Tests
    // ============================================

    #[tokio::test]
    async fn test_add_verified_success() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create a properly signed threat
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            public_key_hex.clone(), // source_node matches public key
        );
        threat.sign(&signing_key);

        // Should succeed
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_ok(), "Expected success, got: {:?}", result);
        assert!(blocklist.is_blocked(&threat.ip).await);
    }

    #[tokio::test]
    async fn test_add_verified_missing_signature() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create a threat WITHOUT signing
        let threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            public_key_hex.clone(),
        );

        // Should fail - no signature
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing signature"));
        assert!(!blocklist.is_blocked(&threat.ip).await);
    }

    #[tokio::test]
    async fn test_add_verified_invalid_signature() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let other_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create and sign with a DIFFERENT key
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            public_key_hex.clone(),
        );
        threat.sign(&other_key); // Signed with wrong key!

        // Should fail - signature doesn't match
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid signature"));
        assert!(!blocklist.is_blocked(&threat.ip).await);
    }

    #[tokio::test]
    async fn test_add_verified_source_node_mismatch() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create threat with wrong source_node
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            "wrong-source-node".to_string(), // Doesn't match public key
        );
        threat.sign(&signing_key);

        // Should fail - source_node doesn't match signing key
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Source node mismatch"));
        assert!(!blocklist.is_blocked(&threat.ip).await);
    }

    #[tokio::test]
    async fn test_add_verified_tampered_data() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create and sign a valid threat
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            public_key_hex.clone(),
        );
        threat.sign(&signing_key);

        // Tamper with the data AFTER signing
        threat.ip = ThreatIpAddress::parse("10.0.0.1").unwrap();

        // Should fail - signature invalid after tampering
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid signature"));
        assert!(!blocklist.is_blocked(&ThreatIpAddress::parse("10.0.0.1").unwrap()).await);
    }

    #[tokio::test]
    async fn test_add_verified_ipv6() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create a properly signed IPv6 threat
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("2001:db8::1").unwrap(),
            ThreatType::HttpFlood,
            7,
            3600,
            public_key_hex.clone(),
        );
        threat.sign(&signing_key);

        // Should succeed with IPv6
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_ok());
        assert!(blocklist.is_blocked_str("2001:db8::1").await);
    }

    #[tokio::test]
    async fn test_add_verified_validation_failure() {
        let blocklist = GlobalBlocklist::new();
        let signing_key = generate_test_signing_key();
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create threat with invalid data
        let mut threat = EnhancedThreatIntel::new(
            ThreatIpAddress::parse("192.168.1.100").unwrap(),
            ThreatType::SynFlood,
            8,
            3600,
            public_key_hex.clone(),
        );
        threat.severity = 0; // Invalid severity!
        threat.sign(&signing_key);

        // Should fail validation before signature check
        let result = blocklist.add_verified_hex(&threat, &public_key_hex).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Severity"));
    }

    // ========================================
    // Y6.7-Y6.8: Revocation Tests
    // ========================================

    #[tokio::test]
    async fn test_y67_revoke_client() {
        let cache = TrustScoreCache::new(50, 60);
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();

        // Register node
        cache.add_node_public_key("node-1", verifying_key).await;

        // Store a token
        let mut token = TrustToken::new(
            "client-1".to_string(),
            80,
            ChallengeType::Managed,
            "node-1".to_string(),
            900,
        );
        token.sign(&signing_key);
        cache.store_token(token).await.unwrap();

        // Verify token exists
        assert_eq!(cache.get_trust_score("client-1").await, 80);

        // Revoke client
        cache.revoke_client("client-1", 1234567890).await;

        // Client should be revoked
        assert!(cache.is_client_revoked("client-1").await);

        // Trust score should fall back to default
        assert_eq!(cache.get_trust_score("client-1").await, 50);
    }

    #[tokio::test]
    async fn test_y67_revoke_node() {
        let cache = TrustScoreCache::new(50, 60);
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();
        let public_key_hex = hex::encode(verifying_key.as_bytes());

        // Register node
        cache.add_node_public_key("node-1", verifying_key).await;

        // Revoke node
        cache.revoke_node(&public_key_hex).await;

        // Node should be revoked
        assert!(cache.is_node_revoked(&public_key_hex).await);
        assert_eq!(cache.revoked_node_count().await, 1);
    }

    #[tokio::test]
    async fn test_y68_process_token_revocation() {
        let config = DistributedEnforcementConfig {
            node_id: "test-node".to_string(),
            signing_key: Some(generate_test_signing_key()),
            ..Default::default()
        };

        let engine = DistributedEnforcementEngine::new(config);

        // Process revocation message
        let revocation = TokenRevocation {
            client_id: "client-to-revoke".to_string(),
            reason: RevocationReason::Compromised,
            revoked_at: 1234567890,
            issuing_node: "admin-node".to_string(),
            signature: None,
        };

        let msg = EnforcementMessage::RevokeToken(revocation);
        engine.process_message(msg).await.unwrap();

        // Client should be revoked
        assert!(engine.trust_cache.is_client_revoked("client-to-revoke").await);
    }

    #[tokio::test]
    async fn test_y68_process_node_revocation() {
        let config = DistributedEnforcementConfig {
            node_id: "test-node".to_string(),
            signing_key: Some(generate_test_signing_key()),
            ..Default::default()
        };

        let engine = DistributedEnforcementEngine::new(config);
        let malicious_key = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

        // Process revocation message
        let revocation = NodeRevocation {
            node_id: "malicious-node".to_string(),
            public_key: malicious_key.to_string(),
            reason: RevocationReason::MaliciousBehavior,
            revoked_at: 1234567890,
            issuing_node: "admin-node".to_string(),
            signature: None,
        };

        let msg = EnforcementMessage::RevokeNode(revocation);
        engine.process_message(msg).await.unwrap();

        // Node should be revoked
        assert!(engine.trust_cache.is_node_revoked(malicious_key).await);
    }

    #[tokio::test]
    async fn test_y68_revoked_client_token_rejected() {
        let signing_key = generate_test_signing_key();
        let verifying_key = signing_key.verifying_key();

        let config = DistributedEnforcementConfig {
            node_id: "test-node".to_string(),
            signing_key: Some(signing_key.clone()),
            ..Default::default()
        };

        let engine = DistributedEnforcementEngine::new(config);

        // Register node key
        engine.trust_cache.add_node_public_key("test-node", verifying_key).await;

        // Revoke client first
        engine.trust_cache.revoke_client("client-1", 1234567890).await;

        // Try to store a token for revoked client
        let mut token = TrustToken::new(
            "client-1".to_string(),
            80,
            ChallengeType::Managed,
            "test-node".to_string(),
            900,
        );
        token.sign(&signing_key);

        // Should reject token for revoked client
        let msg = EnforcementMessage::TrustToken(token);
        let result = engine.process_message(msg).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("revoked"));
    }

    #[test]
    fn test_y68_revocation_reason_serialization() {
        // Test all revocation reasons can be serialized
        let reasons = vec![
            RevocationReason::Compromised,
            RevocationReason::MaliciousBehavior,
            RevocationReason::Offline,
            RevocationReason::Administrative,
            RevocationReason::StakeSlashed,
            RevocationReason::Other("Custom reason".to_string()),
        ];

        for reason in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            let deserialized: RevocationReason = serde_json::from_str(&json).unwrap();
            assert_eq!(reason, deserialized);
        }
    }
}
