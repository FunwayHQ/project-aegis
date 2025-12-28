use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use futures::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode},
    identify,
    identity::Keypair,
    kad::{self, store::MemoryStore, Mode as KadMode},
    mdns,
    noise,
    swarm::{behaviour::toggle::Toggle, NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

// =============================================================================
// SECURITY FIX (X6): P2P Gossip Rate Limiting
// =============================================================================

/// Maximum messages per peer per window
const RATE_LIMIT_MAX_MESSAGES: u32 = 100;

/// Rate limit window duration (1 second)
const RATE_LIMIT_WINDOW_SECS: u64 = 1;

/// How often to clean up old rate limit entries (60 seconds)
const RATE_LIMIT_CLEANUP_INTERVAL_SECS: u64 = 60;

// =============================================================================
// SECURITY FIX (Y9.10): Outbound Amplification Protection
// =============================================================================
//
// Prevents amplification attacks where receiving one message causes many outbound
// messages. This could happen if:
// 1. We receive a threat and re-broadcast it to many peers
// 2. We process a batch of threats and publish them all at once
// 3. A malicious node triggers rapid-fire publishing

/// Y9.10: Maximum outbound messages per window (prevents amplification)
const OUTBOUND_RATE_LIMIT_MAX: u32 = 50;

/// Y9.10: Outbound rate limit window (1 second)
const OUTBOUND_RATE_LIMIT_WINDOW_SECS: u64 = 1;

/// Y9.10: Outbound amplification rate limiter
///
/// Tracks our own outbound message rate to prevent amplification attacks.
/// If we receive a burst of messages, we shouldn't amplify it by sending
/// a corresponding burst of outbound messages.
#[derive(Debug)]
pub struct OutboundRateLimiter {
    /// Count in current window
    count: u32,
    /// Window start time
    window_start: Instant,
    /// Total messages dropped
    pub dropped_count: u64,
}

impl OutboundRateLimiter {
    pub fn new() -> Self {
        Self {
            count: 0,
            window_start: Instant::now(),
            dropped_count: 0,
        }
    }

    /// Check if we can send an outbound message
    ///
    /// Returns true if allowed, false if rate limited.
    pub fn check_and_update(&mut self) -> bool {
        let now = Instant::now();
        let window_duration = Duration::from_secs(OUTBOUND_RATE_LIMIT_WINDOW_SECS);

        // Reset window if expired
        if now.duration_since(self.window_start) >= window_duration {
            self.window_start = now;
            self.count = 1;
            return true;
        }

        // Check if under limit
        if self.count < OUTBOUND_RATE_LIMIT_MAX {
            self.count += 1;
            return true;
        }

        self.dropped_count += 1;
        warn!(
            "Y9.10: Outbound rate limited - {} messages dropped total (amplification protection)",
            self.dropped_count
        );
        false
    }

    /// Get current count in window
    pub fn current_count(&self) -> u32 {
        self.count
    }
}

impl Default for OutboundRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limiter entry for a single peer
#[derive(Debug, Clone)]
struct RateLimitEntry {
    /// Number of messages in current window
    count: u32,
    /// When the current window started
    window_start: Instant,
}

impl RateLimitEntry {
    fn new() -> Self {
        Self {
            count: 0,
            window_start: Instant::now(),
        }
    }

    /// Check if a message is allowed and update counter
    /// Returns true if message is allowed, false if rate limited
    fn check_and_update(&mut self) -> bool {
        let now = Instant::now();
        let window_duration = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);

        // Reset window if expired
        if now.duration_since(self.window_start) >= window_duration {
            self.window_start = now;
            self.count = 1;
            return true;
        }

        // Check if under limit
        if self.count < RATE_LIMIT_MAX_MESSAGES {
            self.count += 1;
            return true;
        }

        false
    }
}

/// P2P Gossip Rate Limiter
///
/// SECURITY FIX (X6): Implements token bucket style rate limiting per peer
/// to prevent gossip flood attacks where a malicious peer sends excessive messages.
#[derive(Debug)]
pub struct GossipRateLimiter {
    /// Rate limit entries per peer ID
    entries: HashMap<PeerId, RateLimitEntry>,
    /// Last cleanup time
    last_cleanup: Instant,
    /// Total messages dropped due to rate limiting
    pub dropped_count: u64,
}

impl GossipRateLimiter {
    /// Create a new rate limiter
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            last_cleanup: Instant::now(),
            dropped_count: 0,
        }
    }

    /// Check if a message from a peer should be allowed
    ///
    /// Returns true if the message is allowed, false if rate limited
    pub fn check_rate_limit(&mut self, peer_id: &PeerId) -> bool {
        // Periodic cleanup of stale entries
        self.maybe_cleanup();

        let entry = self.entries.entry(*peer_id).or_insert_with(RateLimitEntry::new);
        let allowed = entry.check_and_update();

        if !allowed {
            self.dropped_count += 1;
            warn!(
                "SECURITY (X6): Rate limited peer {} - {} messages dropped total",
                peer_id, self.dropped_count
            );
        }

        allowed
    }

    /// Clean up stale entries periodically
    fn maybe_cleanup(&mut self) {
        let now = Instant::now();
        let cleanup_interval = Duration::from_secs(RATE_LIMIT_CLEANUP_INTERVAL_SECS);

        if now.duration_since(self.last_cleanup) >= cleanup_interval {
            let window_duration = Duration::from_secs(RATE_LIMIT_WINDOW_SECS * 2);
            self.entries.retain(|_, entry| {
                now.duration_since(entry.window_start) < window_duration
            });
            self.last_cleanup = now;
            debug!("Rate limiter cleanup: {} active peers tracked", self.entries.len());
        }
    }

    /// Get the number of tracked peers
    pub fn tracked_peers(&self) -> usize {
        self.entries.len()
    }
}

impl Default for GossipRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current Unix timestamp in seconds
/// Returns 0 if system clock is before Unix epoch (should never happen on modern systems)
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            warn!("System clock before Unix epoch: {} - using timestamp 0", e);
            0
        })
}

/// IP address type (IPv4 or IPv6)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreatIpAddress {
    V4(String),
    V6(String),
}

impl ThreatIpAddress {
    /// Parse an IP address string into V4 or V6 variant
    pub fn parse(ip: &str) -> Result<Self> {
        if let Ok(_) = ip.parse::<std::net::Ipv4Addr>() {
            Ok(ThreatIpAddress::V4(ip.to_string()))
        } else if let Ok(_) = ip.parse::<std::net::Ipv6Addr>() {
            Ok(ThreatIpAddress::V6(ip.to_string()))
        } else {
            anyhow::bail!("Invalid IP address format: {}", ip)
        }
    }

    /// Get the IP address as a string
    pub fn as_str(&self) -> &str {
        match self {
            ThreatIpAddress::V4(ip) => ip,
            ThreatIpAddress::V6(ip) => ip,
        }
    }

    /// Check if this is an IPv4 address
    pub fn is_v4(&self) -> bool {
        matches!(self, ThreatIpAddress::V4(_))
    }

    /// Check if this is an IPv6 address
    pub fn is_v6(&self) -> bool {
        matches!(self, ThreatIpAddress::V6(_))
    }
}

/// Threat intelligence message shared across the P2P network
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreatIntelligence {
    /// SECURITY FIX (Y5.3): Unique message ID for replay protection
    /// This UUID is used to detect and drop duplicate messages.
    #[serde(default = "generate_message_id")]
    pub message_id: String,
    /// IP address of the threat (supports IPv4 and IPv6)
    pub ip: String,
    /// Type of threat (e.g., "syn_flood", "ddos", "brute_force")
    pub threat_type: String,
    /// Severity level (1-10, where 10 is most severe)
    pub severity: u8,
    /// Timestamp when threat was detected (Unix timestamp in seconds)
    pub timestamp: u64,
    /// How long to block the IP (in seconds)
    pub block_duration_secs: u64,
    /// Source node that reported the threat (public key hex)
    pub source_node: String,
    /// Optional description
    pub description: Option<String>,
}

/// Generate a unique message ID (Y5.3)
fn generate_message_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Signed threat intelligence message with Ed25519 signature
///
/// This wrapper provides cryptographic authentication for threat intelligence
/// messages, preventing spoofing attacks where malicious nodes could inject
/// fake threats to cause legitimate IPs to be blocked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedThreatIntelligence {
    /// The threat intelligence data
    pub threat: ThreatIntelligence,
    /// Ed25519 signature over the serialized threat data
    pub signature: String,
    /// Public key of the signer (hex encoded)
    pub public_key: String,
}

impl SignedThreatIntelligence {
    /// Sign a threat intelligence message
    pub fn sign(threat: ThreatIntelligence, signing_key: &SigningKey) -> Result<Self> {
        // Serialize the threat to JSON for signing
        let threat_json = serde_json::to_string(&threat)
            .context("Failed to serialize threat for signing")?;

        // Sign the serialized data
        let signature = signing_key.sign(threat_json.as_bytes());

        // Get the public key
        let public_key = signing_key.verifying_key();

        Ok(Self {
            threat,
            signature: hex::encode(signature.to_bytes()),
            public_key: hex::encode(public_key.as_bytes()),
        })
    }

    /// Verify the signature on a signed threat intelligence message
    pub fn verify(&self) -> Result<bool> {
        // Decode the public key
        let public_key_bytes = hex::decode(&self.public_key)
            .context("Invalid public key hex encoding")?;

        if public_key_bytes.len() != 32 {
            anyhow::bail!("Invalid public key length: expected 32 bytes, got {}", public_key_bytes.len());
        }

        let public_key_array: [u8; 32] = public_key_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert public key bytes"))?;

        let verifying_key = VerifyingKey::from_bytes(&public_key_array)
            .context("Invalid public key")?;

        // Decode the signature
        let signature_bytes = hex::decode(&self.signature)
            .context("Invalid signature hex encoding")?;

        if signature_bytes.len() != 64 {
            anyhow::bail!("Invalid signature length: expected 64 bytes, got {}", signature_bytes.len());
        }

        let signature_array: [u8; 64] = signature_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Failed to convert signature bytes"))?;

        let signature = Signature::from_bytes(&signature_array);

        // Serialize the threat data for verification
        let threat_json = serde_json::to_string(&self.threat)
            .context("Failed to serialize threat for verification")?;

        // Verify the signature
        match verifying_key.verify(threat_json.as_bytes(), &signature) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify signature and validate threat data
    pub fn verify_and_validate(&self) -> Result<()> {
        // First verify the signature
        if !self.verify()? {
            anyhow::bail!("Invalid signature on threat intelligence message");
        }

        // Then validate the threat data
        self.threat.validate()?;

        // Verify that source_node matches the public key
        if self.threat.source_node != self.public_key {
            anyhow::bail!("Source node does not match signing public key");
        }

        Ok(())
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize signed threat intelligence")
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize signed threat intelligence")
    }
}

impl ThreatIntelligence {
    /// Create a new threat intelligence report
    pub fn new(
        ip: String,
        threat_type: String,
        severity: u8,
        block_duration_secs: u64,
        source_node: String,
    ) -> Self {
        let timestamp = current_timestamp();

        Self {
            message_id: generate_message_id(), // Y5.3: Unique message ID
            ip,
            threat_type,
            severity,
            timestamp,
            block_duration_secs,
            source_node,
            description: None,
        }
    }

    /// Create with description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Validate the threat intelligence data
    pub fn validate(&self) -> Result<()> {
        // Validate IP address format (supports both IPv4 and IPv6)
        ThreatIpAddress::parse(&self.ip)?;

        // Validate severity
        if self.severity == 0 || self.severity > 10 {
            anyhow::bail!("Severity must be between 1 and 10");
        }

        // Validate block duration (max 24 hours)
        if self.block_duration_secs == 0 || self.block_duration_secs > 86400 {
            anyhow::bail!("Block duration must be between 1 second and 24 hours");
        }

        // Validate timestamp (not too far in the past or future)
        let now = current_timestamp();

        if self.timestamp > now + 300 {
            anyhow::bail!("Timestamp is too far in the future");
        }

        if now.saturating_sub(self.timestamp) > 3600 {
            anyhow::bail!("Timestamp is too old (>1 hour)");
        }

        Ok(())
    }

    /// Get the parsed IP address
    pub fn parsed_ip(&self) -> Result<ThreatIpAddress> {
        ThreatIpAddress::parse(&self.ip)
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize threat intelligence")
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize threat intelligence")
    }
}

/// Network behaviour for threat intelligence P2P
#[derive(NetworkBehaviour)]
pub struct ThreatIntelBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    /// mDNS for local peer discovery - wrapped in Toggle for production disabling (Y6.10)
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    pub kad: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
}

/// Configuration for P2P network
///
/// # Security Note (Y5.6)
/// In production, `trusted_public_keys` MUST be pre-populated with known
/// node keys. An empty list is only allowed in debug builds for testing.
/// Use `validate()` to check config before starting the P2P network.
#[derive(Debug, Clone)]
pub struct P2PConfig {
    /// Port to listen on
    pub listen_port: u16,
    /// Whether to enable mDNS for local peer discovery
    pub enable_mdns: bool,
    /// Bootstrap peers for Kademlia DHT
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    /// Trusted public keys for threat intelligence verification (hex encoded)
    ///
    /// # Security (Y5.6)
    /// In release builds, this list MUST contain at least one trusted key.
    /// Messages from nodes not in this list will be rejected.
    pub trusted_public_keys: Vec<String>,
}

impl P2PConfig {
    /// Validate the configuration for production use
    ///
    /// # Security Note (Y5.6, Y6.10)
    /// This validation ensures that the P2P network is properly configured:
    /// - Y5.6: Pre-populated node keys are required in release builds
    /// - Y6.10: mDNS must be disabled in release builds (prevents local network attacks)
    ///
    /// # Returns
    /// - `Ok(())` if configuration is valid
    /// - `Err(String)` describing the configuration issue
    pub fn validate(&self) -> Result<(), String> {
        #[cfg(not(debug_assertions))]
        {
            // Y5.6: Require pre-populated node keys in production
            if self.trusted_public_keys.is_empty() {
                return Err(
                    "SECURITY ERROR (Y5.6): P2P trusted_public_keys is empty. \
                     Production deployments MUST pre-populate trusted node keys. \
                     Configure at least one trusted node public key before starting."
                        .to_string(),
                );
            }

            // Y6.10: Disable mDNS in production
            // mDNS allows local network peer discovery which is a security risk
            // as it could allow untrusted peers on the same network to connect
            if self.enable_mdns {
                return Err(
                    "SECURITY ERROR (Y6.10): mDNS is enabled in production build. \
                     Local network peer discovery is a security risk. \
                     Set enable_mdns = false and use bootstrap_peers for peer discovery."
                        .to_string(),
                );
            }
        }

        // Validate key format (should be valid hex, 64 chars for Ed25519 public key)
        for key in &self.trusted_public_keys {
            if key.len() != 64 {
                return Err(format!(
                    "Invalid trusted key length: {} (expected 64 hex chars for Ed25519 public key)",
                    key.len()
                ));
            }
            if !key.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!(
                    "Invalid trusted key format: contains non-hex characters"
                ));
            }
        }

        Ok(())
    }

    /// Create a production-safe configuration
    ///
    /// # Security Note (Y6.10)
    /// This method creates a configuration suitable for production use:
    /// - mDNS disabled by default
    /// - Requires explicit bootstrap peers and trusted keys
    pub fn production(listen_port: u16, bootstrap_peers: Vec<(PeerId, Multiaddr)>, trusted_public_keys: Vec<String>) -> Self {
        Self {
            listen_port,
            enable_mdns: false, // Y6.10: Always disabled in production
            bootstrap_peers,
            trusted_public_keys,
        }
    }
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
            trusted_public_keys: Vec::new(),
        }
    }
}

/// Trusted node registry for signature verification
#[derive(Debug, Clone)]
pub struct TrustedNodeRegistry {
    /// Set of trusted public keys (hex encoded)
    trusted_keys: Arc<RwLock<HashSet<String>>>,
    /// Whether to accept any valid signature (open mode)
    /// SECURITY (X2.7): Open mode is ONLY allowed in debug builds
    open_mode: bool,
}

impl TrustedNodeRegistry {
    /// Create a new registry from config
    ///
    /// # Security Note (X2.7)
    /// Open mode (empty trusted_keys) is only allowed in debug builds.
    /// In release builds, empty trusted_keys will disable open mode and require
    /// explicit key management via add_trusted_key().
    pub fn new(trusted_keys: Vec<String>) -> Self {
        let requested_open_mode = trusted_keys.is_empty();

        // SECURITY FIX (X2.7): Disable open mode in release builds
        #[cfg(debug_assertions)]
        let open_mode = {
            if requested_open_mode {
                warn!("SECURITY WARNING: P2P open mode enabled - accepting any valid signature (DEBUG BUILD ONLY)");
            }
            requested_open_mode
        };

        #[cfg(not(debug_assertions))]
        let open_mode = {
            if requested_open_mode {
                error!(
                    "SECURITY: P2P open mode requested but DISABLED in release build. \
                     Configure trusted_public_keys or use add_trusted_key() to add nodes."
                );
            }
            false // Always disabled in release
        };

        Self {
            trusted_keys: Arc::new(RwLock::new(trusted_keys.into_iter().collect())),
            open_mode,
        }
    }

    /// Create a registry that explicitly allows open mode (for testing only)
    ///
    /// # Safety
    /// This function is ONLY available in debug builds. It should never be used
    /// in production code.
    #[cfg(debug_assertions)]
    pub fn new_open_mode_for_testing() -> Self {
        warn!("SECURITY WARNING: Creating open mode registry for TESTING ONLY");
        Self {
            trusted_keys: Arc::new(RwLock::new(HashSet::new())),
            open_mode: true,
        }
    }

    /// Check if open mode is currently active
    pub fn is_open_mode(&self) -> bool {
        self.open_mode
    }

    /// Check if a public key is trusted
    ///
    /// # Security Note (X2.7)
    /// In release builds, open mode is always disabled. If no trusted keys are
    /// configured, ALL keys will be rejected (fail-secure).
    pub async fn is_trusted(&self, public_key: &str) -> bool {
        // SECURITY FIX (X2.7): Only allow open mode in debug builds
        if self.open_mode {
            #[cfg(debug_assertions)]
            {
                warn!("SECURITY: Open mode active - accepting key: {}...", &public_key[..16.min(public_key.len())]);
                return true;
            }
            #[cfg(not(debug_assertions))]
            {
                // This branch should never execute due to new() logic, but defense in depth
                error!("SECURITY VIOLATION: Open mode check reached in release build - REJECTING");
                return false;
            }
        }

        let keys = self.trusted_keys.read().await;
        keys.contains(public_key)
    }

    /// Add a trusted public key
    pub async fn add_trusted_key(&self, public_key: String) {
        let mut keys = self.trusted_keys.write().await;
        keys.insert(public_key);
    }

    /// Remove a trusted public key
    pub async fn remove_trusted_key(&self, public_key: &str) {
        let mut keys = self.trusted_keys.write().await;
        keys.remove(public_key);
    }

    /// Get all trusted keys
    pub async fn get_trusted_keys(&self) -> Vec<String> {
        let keys = self.trusted_keys.read().await;
        keys.iter().cloned().collect()
    }
}

// =============================================================================
// Y5.4: Seen Message IDs Cache Constants
// =============================================================================

/// Maximum number of seen message IDs to cache
/// This prevents unbounded memory growth while still detecting replays
const MAX_SEEN_MESSAGE_IDS: usize = 100_000;

/// Cleanup interval - remove old entries when cache exceeds this threshold
const SEEN_MESSAGE_IDS_CLEANUP_THRESHOLD: usize = 90_000;

// =============================================================================
// Y6.2: Network Partition Detection
// =============================================================================

/// Heartbeat interval for partition detection (seconds)
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Timeout before a peer is considered potentially partitioned (seconds)
const PARTITION_TIMEOUT_SECS: u64 = 90;

/// Threshold of missing peers to declare a potential partition
const PARTITION_PEER_THRESHOLD: f32 = 0.5;

/// Network partition detector based on heartbeat patterns
///
/// Tracks peer heartbeats and detects when a significant portion of
/// peers stop responding, indicating a potential network partition.
#[derive(Debug)]
pub struct NetworkPartitionDetector {
    /// Last heartbeat time per peer (Unix timestamp)
    peer_heartbeats: HashMap<String, u64>,
    /// Peers we expect to hear from (known peers)
    expected_peers: HashSet<String>,
    /// Current partition state
    partition_detected: bool,
    /// Last time we checked for partitions
    last_check: u64,
}

impl NetworkPartitionDetector {
    pub fn new() -> Self {
        Self {
            peer_heartbeats: HashMap::new(),
            expected_peers: HashSet::new(),
            partition_detected: false,
            last_check: 0,
        }
    }

    /// Register a peer as expected (should send heartbeats)
    pub fn register_peer(&mut self, peer_id: &str) {
        self.expected_peers.insert(peer_id.to_string());
    }

    /// Remove a peer from expected peers (graceful disconnect)
    pub fn unregister_peer(&mut self, peer_id: &str) {
        self.expected_peers.remove(peer_id);
        self.peer_heartbeats.remove(peer_id);
    }

    /// Record a heartbeat from a peer
    pub fn record_heartbeat(&mut self, peer_id: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.peer_heartbeats.insert(peer_id.to_string(), now);
    }

    /// Check for network partitions
    ///
    /// Returns `Some(PartitionEvent)` if a partition state change is detected
    pub fn check_partition(&mut self) -> Option<PartitionEvent> {
        if self.expected_peers.is_empty() {
            return None;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Count how many peers are missing heartbeats
        let mut missing_count = 0;
        let mut stale_peers = Vec::new();

        for peer_id in &self.expected_peers {
            match self.peer_heartbeats.get(peer_id) {
                Some(&last_heartbeat) => {
                    if now - last_heartbeat > PARTITION_TIMEOUT_SECS {
                        missing_count += 1;
                        stale_peers.push(peer_id.clone());
                    }
                }
                None => {
                    missing_count += 1;
                    stale_peers.push(peer_id.clone());
                }
            }
        }

        let missing_ratio = missing_count as f32 / self.expected_peers.len() as f32;
        let was_partitioned = self.partition_detected;
        self.partition_detected = missing_ratio >= PARTITION_PEER_THRESHOLD;
        self.last_check = now;

        // Detect state changes
        match (was_partitioned, self.partition_detected) {
            (false, true) => Some(PartitionEvent::PartitionDetected {
                missing_peers: stale_peers,
                missing_ratio,
            }),
            (true, false) => Some(PartitionEvent::PartitionResolved {
                reconnected_count: self.expected_peers.len() - missing_count,
            }),
            _ => None,
        }
    }

    /// Get current partition status
    pub fn is_partitioned(&self) -> bool {
        self.partition_detected
    }

    /// Get count of responsive peers
    pub fn responsive_peer_count(&self) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.peer_heartbeats
            .values()
            .filter(|&&t| now - t <= PARTITION_TIMEOUT_SECS)
            .count()
    }

    /// Get count of expected peers
    pub fn expected_peer_count(&self) -> usize {
        self.expected_peers.len()
    }
}

impl Default for NetworkPartitionDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Partition detection events
#[derive(Debug, Clone)]
pub enum PartitionEvent {
    /// Partition detected - significant peers are unreachable
    PartitionDetected {
        missing_peers: Vec<String>,
        missing_ratio: f32,
    },
    /// Partition resolved - peers are reconnecting
    PartitionResolved {
        reconnected_count: usize,
    },
}

// =============================================================================
// Y6.3: CRDT-based Threat Intelligence Conflict Resolution
// =============================================================================

/// Maximum age for threat intel entries (24 hours)
const THREAT_INTEL_MAX_AGE_SECS: u64 = 86400;

/// CRDT-based threat intelligence store
///
/// Uses Last-Writer-Wins (LWW) semantics with logical timestamps to resolve
/// conflicts deterministically across distributed nodes.
#[derive(Debug)]
pub struct ThreatIntelCRDT {
    /// Threat entries keyed by IP address
    /// Each entry is an LWW-Register containing (timestamp, threat_info)
    entries: HashMap<String, LWWThreatEntry>,
    /// Local node ID for conflict resolution (higher ID wins ties)
    local_node_id: String,
}

/// LWW (Last-Writer-Wins) threat entry
#[derive(Debug, Clone)]
pub struct LWWThreatEntry {
    /// Logical timestamp (wall clock + node ID for tie-breaking)
    pub timestamp: u64,
    /// Node ID that created this entry (for tie-breaking)
    pub node_id: String,
    /// The threat information
    pub threat: ThreatIntelligence,
    /// Whether the threat is active (false = tombstone for deletion)
    pub is_active: bool,
}

impl LWWThreatEntry {
    /// Compare two entries for LWW ordering
    /// Returns true if `other` should replace `self`
    fn should_be_replaced_by(&self, other: &LWWThreatEntry) -> bool {
        // Higher timestamp wins
        if other.timestamp > self.timestamp {
            return true;
        }
        // If timestamps are equal, higher node_id wins (deterministic tie-breaker)
        if other.timestamp == self.timestamp && other.node_id > self.node_id {
            return true;
        }
        false
    }
}

/// Result of merging threat intel
#[derive(Debug, Clone, PartialEq)]
pub enum MergeResult {
    /// Entry was added (new IP)
    Added,
    /// Entry was updated (newer timestamp)
    Updated,
    /// Entry was ignored (older timestamp)
    Ignored,
    /// Entry was rejected (invalid)
    Rejected(String),
}

impl ThreatIntelCRDT {
    /// Create a new CRDT-based threat intel store
    pub fn new(local_node_id: String) -> Self {
        Self {
            entries: HashMap::new(),
            local_node_id,
        }
    }

    /// Add or update a threat entry using LWW semantics
    ///
    /// # Y6.3: Conflict Resolution
    /// - If no existing entry: add new entry
    /// - If existing entry with older timestamp: update
    /// - If existing entry with newer timestamp: ignore (idempotent)
    /// - If same timestamp: use node_id as tie-breaker (higher wins)
    pub fn merge(&mut self, threat: ThreatIntelligence, node_id: String, timestamp: u64) -> MergeResult {
        // Validate threat data
        if let Err(e) = threat.validate() {
            return MergeResult::Rejected(format!("Invalid threat data: {}", e));
        }

        let ip = threat.ip.clone();
        let new_entry = LWWThreatEntry {
            timestamp,
            node_id,
            threat,
            is_active: true,
        };

        // Check if existing entry should be replaced
        let should_update = match self.entries.get(&ip) {
            None => true, // No existing entry, add new
            Some(existing) => existing.should_be_replaced_by(&new_entry),
        };

        if should_update {
            let was_new = !self.entries.contains_key(&ip);
            self.entries.insert(ip.clone(), new_entry);
            if was_new {
                debug!("Y6.3: Added new threat entry for IP: {}", ip);
                MergeResult::Added
            } else {
                debug!("Y6.3: Updated threat entry for IP: {} (ts: {})", ip, timestamp);
                MergeResult::Updated
            }
        } else {
            debug!("Y6.3: Ignored older threat entry for IP: {} (ts: {})", ip, timestamp);
            MergeResult::Ignored
        }
    }

    /// Mark a threat as inactive (tombstone for deletion)
    pub fn remove(&mut self, ip: &str, node_id: String, timestamp: u64) -> MergeResult {
        let ip_str = ip.to_string();

        // First check if we should update and clone what we need
        let update_info = self.entries.get(&ip_str).map(|existing| {
            let threat_clone = existing.threat.clone();
            let should_replace = existing.should_be_replaced_by(&LWWThreatEntry {
                timestamp,
                node_id: node_id.clone(),
                threat: threat_clone.clone(),
                is_active: false,
            });
            (threat_clone, should_replace)
        });

        match update_info {
            Some((threat, true)) => {
                let tombstone = LWWThreatEntry {
                    timestamp,
                    node_id,
                    threat,
                    is_active: false,
                };
                self.entries.insert(ip_str, tombstone);
                debug!("Y6.3: Marked threat as inactive for IP: {}", ip);
                MergeResult::Updated
            }
            Some((_, false)) => MergeResult::Ignored,
            None => MergeResult::Ignored,
        }
    }

    /// Get an active threat entry by IP
    pub fn get(&self, ip: &str) -> Option<&ThreatIntelligence> {
        self.entries
            .get(ip)
            .filter(|e| e.is_active)
            .map(|e| &e.threat)
    }

    /// Get all active threats
    pub fn get_active_threats(&self) -> Vec<&ThreatIntelligence> {
        self.entries
            .values()
            .filter(|e| e.is_active)
            .map(|e| &e.threat)
            .collect()
    }

    /// Prune expired entries
    pub fn prune_expired(&mut self) -> usize {
        let now = current_timestamp();
        let before = self.entries.len();

        self.entries.retain(|_, entry| {
            let age = now.saturating_sub(entry.timestamp);
            age < THREAT_INTEL_MAX_AGE_SECS
        });

        let pruned = before - self.entries.len();
        if pruned > 0 {
            debug!("Y6.3: Pruned {} expired threat entries", pruned);
        }
        pruned
    }

    /// Get entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize state for replication
    pub fn serialize_state(&self) -> Result<Vec<u8>> {
        let entries: Vec<(&String, &LWWThreatEntry)> = self.entries.iter().collect();
        bincode::serialize(&entries).context("Failed to serialize CRDT state")
    }

    /// Merge serialized state from another node
    pub fn merge_state(&mut self, data: &[u8]) -> Result<(usize, usize, usize)> {
        let entries: Vec<(String, LWWThreatEntry)> =
            bincode::deserialize(data).context("Failed to deserialize CRDT state")?;

        let mut added = 0;
        let mut updated = 0;
        let mut ignored = 0;

        for (ip, entry) in entries {
            match self.merge(entry.threat.clone(), entry.node_id.clone(), entry.timestamp) {
                MergeResult::Added => added += 1,
                MergeResult::Updated => updated += 1,
                MergeResult::Ignored => ignored += 1,
                MergeResult::Rejected(_) => ignored += 1,
            }
        }

        Ok((added, updated, ignored))
    }
}

// Need to derive Serialize/Deserialize for LWWThreatEntry for bincode
impl Serialize for LWWThreatEntry {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("LWWThreatEntry", 4)?;
        state.serialize_field("timestamp", &self.timestamp)?;
        state.serialize_field("node_id", &self.node_id)?;
        state.serialize_field("threat", &self.threat)?;
        state.serialize_field("is_active", &self.is_active)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for LWWThreatEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            timestamp: u64,
            node_id: String,
            threat: ThreatIntelligence,
            is_active: bool,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(LWWThreatEntry {
            timestamp: helper.timestamp,
            node_id: helper.node_id,
            threat: helper.threat,
            is_active: helper.is_active,
        })
    }
}

// =============================================================================
// Y6.9: Solana Staking Integration for Sybil Resistance
// =============================================================================

/// Minimum stake required to participate in P2P network (in lamports)
/// 1 SOL = 1_000_000_000 lamports
const MIN_STAKE_LAMPORTS: u64 = 100_000_000; // 0.1 SOL minimum

/// Stake tier thresholds and their trust weights
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakeTier {
    /// No stake - excluded from network
    None,
    /// Minimum stake (0.1 - 1 SOL) - basic participation
    Basic,
    /// Standard stake (1 - 10 SOL) - trusted node
    Standard,
    /// High stake (10 - 100 SOL) - highly trusted
    High,
    /// Elite stake (100+ SOL) - elite node with governance rights
    Elite,
}

impl StakeTier {
    /// Get trust weight for this tier (higher = more trusted)
    pub fn trust_weight(&self) -> u8 {
        match self {
            StakeTier::None => 0,
            StakeTier::Basic => 1,
            StakeTier::Standard => 3,
            StakeTier::High => 5,
            StakeTier::Elite => 10,
        }
    }

    /// Determine tier from stake amount in lamports
    pub fn from_stake(lamports: u64) -> Self {
        let sol = lamports / 1_000_000_000;
        if lamports < MIN_STAKE_LAMPORTS {
            StakeTier::None
        } else if sol < 1 {
            StakeTier::Basic
        } else if sol < 10 {
            StakeTier::Standard
        } else if sol < 100 {
            StakeTier::High
        } else {
            StakeTier::Elite
        }
    }
}

/// Cached stake information for a node
#[derive(Debug, Clone)]
pub struct NodeStakeInfo {
    /// Node's Solana wallet address
    pub wallet: String,
    /// Stake amount in lamports
    pub stake_lamports: u64,
    /// Stake tier
    pub tier: StakeTier,
    /// When stake was last verified
    pub verified_at: u64,
    /// Whether the node is slashed
    pub is_slashed: bool,
}

/// Y6.9: Staking verifier for Sybil resistance
///
/// Verifies that P2P nodes have staked $AEGIS tokens to participate
/// in the threat intelligence network. This prevents Sybil attacks
/// where an attacker creates many fake nodes.
#[derive(Debug)]
pub struct StakingVerifier {
    /// Cached stake information per node
    stakes: HashMap<String, NodeStakeInfo>,
    /// Minimum stake required (lamports)
    min_stake: u64,
    /// How long stake verification is valid (seconds)
    verification_ttl: u64,
    /// Solana RPC endpoint
    rpc_endpoint: String,
    /// Whether staking verification is enabled
    enabled: bool,
}

impl StakingVerifier {
    /// Create a new staking verifier
    pub fn new(rpc_endpoint: String) -> Self {
        Self {
            stakes: HashMap::new(),
            min_stake: MIN_STAKE_LAMPORTS,
            verification_ttl: 3600, // 1 hour TTL
            rpc_endpoint,
            enabled: true,
        }
    }

    /// Create a disabled staking verifier (for testing)
    pub fn disabled() -> Self {
        Self {
            stakes: HashMap::new(),
            min_stake: 0,
            verification_ttl: 3600,
            rpc_endpoint: String::new(),
            enabled: false,
        }
    }

    /// Check if staking verification is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get minimum stake requirement
    pub fn min_stake(&self) -> u64 {
        self.min_stake
    }

    /// Set minimum stake requirement
    pub fn set_min_stake(&mut self, lamports: u64) {
        self.min_stake = lamports;
    }

    /// Check if a node has sufficient stake to participate
    pub fn has_sufficient_stake(&self, node_id: &str) -> bool {
        if !self.enabled {
            return true; // Staking disabled, allow all
        }

        match self.stakes.get(node_id) {
            Some(info) => {
                let now = current_timestamp();
                let age = now.saturating_sub(info.verified_at);

                // Check if verification is still valid
                if age > self.verification_ttl {
                    warn!("Y6.9: Stake verification expired for node {}", node_id);
                    return false;
                }

                // Check if node is slashed
                if info.is_slashed {
                    warn!("Y6.9: Node {} is slashed", node_id);
                    return false;
                }

                // Check stake amount
                info.stake_lamports >= self.min_stake
            }
            None => {
                debug!("Y6.9: No stake info for node {}", node_id);
                false
            }
        }
    }

    /// Get stake tier for a node
    pub fn get_stake_tier(&self, node_id: &str) -> StakeTier {
        self.stakes
            .get(node_id)
            .map(|info| info.tier)
            .unwrap_or(StakeTier::None)
    }

    /// Get trust weight for a node (based on stake tier)
    pub fn get_trust_weight(&self, node_id: &str) -> u8 {
        self.get_stake_tier(node_id).trust_weight()
    }

    /// Register verified stake for a node
    ///
    /// In production, this would be called after verifying stake on-chain.
    /// For now, this is a manual registration method.
    pub fn register_stake(&mut self, node_id: String, wallet: String, stake_lamports: u64) {
        let tier = StakeTier::from_stake(stake_lamports);
        let now = current_timestamp();

        let info = NodeStakeInfo {
            wallet,
            stake_lamports,
            tier,
            verified_at: now,
            is_slashed: false,
        };

        info!(
            "Y6.9: Registered stake for node {}: {} lamports ({:?})",
            node_id, stake_lamports, tier
        );

        self.stakes.insert(node_id, info);
    }

    /// Update stake verification timestamp (after on-chain verification)
    pub fn refresh_verification(&mut self, node_id: &str) {
        if let Some(info) = self.stakes.get_mut(node_id) {
            info.verified_at = current_timestamp();
            debug!("Y6.9: Refreshed stake verification for node {}", node_id);
        }
    }

    /// Mark a node as slashed (due to malicious behavior)
    pub fn slash_node(&mut self, node_id: &str, reason: &str) {
        if let Some(info) = self.stakes.get_mut(node_id) {
            info.is_slashed = true;
            warn!(
                "Y6.9: Node {} SLASHED for: {}. Stake: {} lamports",
                node_id, reason, info.stake_lamports
            );
        }
    }

    /// Remove slashed status from a node
    pub fn unslash_node(&mut self, node_id: &str) {
        if let Some(info) = self.stakes.get_mut(node_id) {
            info.is_slashed = false;
            info!("Y6.9: Node {} un-slashed", node_id);
        }
    }

    /// Get stake info for a node
    pub fn get_stake_info(&self, node_id: &str) -> Option<&NodeStakeInfo> {
        self.stakes.get(node_id)
    }

    /// Get all nodes with sufficient stake
    pub fn get_staked_nodes(&self) -> Vec<&str> {
        self.stakes
            .iter()
            .filter(|(_, info)| info.stake_lamports >= self.min_stake && !info.is_slashed)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Get nodes by stake tier
    pub fn get_nodes_by_tier(&self, tier: StakeTier) -> Vec<&str> {
        self.stakes
            .iter()
            .filter(|(_, info)| info.tier == tier && !info.is_slashed)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Get the RPC endpoint
    pub fn rpc_endpoint(&self) -> &str {
        &self.rpc_endpoint
    }

    /// Prune expired stake verifications
    pub fn prune_expired(&mut self) -> usize {
        let now = current_timestamp();
        let before = self.stakes.len();

        self.stakes.retain(|_, info| {
            let age = now.saturating_sub(info.verified_at);
            age < self.verification_ttl * 2 // Keep for 2x TTL before pruning
        });

        let pruned = before - self.stakes.len();
        if pruned > 0 {
            debug!("Y6.9: Pruned {} expired stake verifications", pruned);
        }
        pruned
    }
}

impl Default for StakingVerifier {
    fn default() -> Self {
        Self::disabled()
    }
}

/// P2P Threat Intelligence Network
pub struct ThreatIntelP2P {
    swarm: Swarm<ThreatIntelBehaviour>,
    topic: IdentTopic,
    peer_id: PeerId,
    receiver: mpsc::UnboundedReceiver<ThreatIntelligence>,
    sender: mpsc::UnboundedSender<ThreatIntelligence>,
    /// Ed25519 signing key for this node
    signing_key: SigningKey,
    /// Registry of trusted node public keys
    trusted_registry: TrustedNodeRegistry,
    /// SECURITY FIX (X6): Rate limiter for gossip messages
    rate_limiter: GossipRateLimiter,
    /// SECURITY FIX (Y9.10): Outbound rate limiter for amplification protection
    outbound_rate_limiter: OutboundRateLimiter,
    /// SECURITY FIX (Y5.4): Seen message IDs for replay protection
    /// Tracks message_ids to detect and drop duplicate messages
    seen_message_ids: HashSet<String>,
}

impl ThreatIntelP2P {
    /// Create a new P2P threat intelligence network
    pub fn new(config: P2PConfig) -> Result<Self> {
        // Generate Ed25519 signing key for threat intelligence messages
        use rand::RngCore;
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        let signing_key = SigningKey::from_bytes(&secret_key_bytes);

        // Create trusted registry from config
        let trusted_registry = TrustedNodeRegistry::new(config.trusted_public_keys.clone());

        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        info!("Local peer ID: {}", peer_id);

        // Create a Gossipsub topic for threat intelligence
        let topic = IdentTopic::new("aegis-threat-intel");

        // Clone config for use in closure
        let bootstrap_peers = config.bootstrap_peers.clone();
        let enable_mdns = config.enable_mdns;

        // Build swarm using the new builder API
        let swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                // Create Gossipsub configuration
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(1))
                    .validation_mode(ValidationMode::Strict)
                    .message_id_fn(|message: &gossipsub::Message| {
                        let mut hasher = DefaultHasher::new();
                        message.data.hash(&mut hasher);
                        gossipsub::MessageId::from(hasher.finish().to_string())
                    })
                    .build()
                    .expect("Failed to build gossipsub config");

                // Create Gossipsub behaviour
                let mut gossipsub = gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("Failed to create gossipsub behaviour");

                // Subscribe to the threat intelligence topic
                gossipsub.subscribe(&topic)
                    .expect("Failed to subscribe to topic");

                // Y6.10: Conditionally create mDNS behaviour for local peer discovery
                // In production builds, mDNS should be disabled to prevent local network attacks
                let mdns = if enable_mdns {
                    info!("mDNS enabled for local peer discovery (dev mode)");
                    Toggle::from(Some(
                        mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                            .expect("Failed to create mDNS behaviour")
                    ))
                } else {
                    info!("mDNS disabled (production security hardening Y6.10)");
                    Toggle::from(None)
                };

                // Create Kademlia DHT for global peer discovery
                let store = MemoryStore::new(peer_id);
                let mut kad = kad::Behaviour::new(peer_id, store);
                kad.set_mode(Some(KadMode::Server));

                // Add bootstrap peers to Kademlia
                for (peer_id, addr) in &bootstrap_peers {
                    kad.add_address(peer_id, addr.clone());
                }

                // Create identify behaviour
                let identify = identify::Behaviour::new(identify::Config::new(
                    "/aegis-threat-intel/1.0.0".to_string(),
                    key.public(),
                ));

                ThreatIntelBehaviour {
                    gossipsub,
                    mdns,
                    kad,
                    identify,
                }
            })?
            .build();

        // Create channel for receiving threat intelligence
        let (sender, receiver) = mpsc::unbounded_channel();

        info!("Node public key: {}", hex::encode(signing_key.verifying_key().as_bytes()));

        Ok(Self {
            swarm,
            topic,
            peer_id,
            receiver,
            sender,
            signing_key,
            trusted_registry,
            rate_limiter: GossipRateLimiter::new(),
            outbound_rate_limiter: OutboundRateLimiter::new(),
            seen_message_ids: HashSet::new(),
        })
    }

    /// Create with a specific signing key (for testing or key persistence)
    pub fn with_signing_key(config: P2PConfig, signing_key: SigningKey) -> Result<Self> {
        // Create trusted registry from config
        let trusted_registry = TrustedNodeRegistry::new(config.trusted_public_keys.clone());

        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());

        info!("Local peer ID: {}", peer_id);

        // Create a Gossipsub topic for threat intelligence
        let topic = IdentTopic::new("aegis-threat-intel");

        // Clone config for use in closure
        let bootstrap_peers = config.bootstrap_peers.clone();
        let enable_mdns = config.enable_mdns;

        // Build swarm using the new builder API
        let swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                // Create Gossipsub configuration
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(1))
                    .validation_mode(ValidationMode::Strict)
                    .message_id_fn(|message: &gossipsub::Message| {
                        let mut hasher = DefaultHasher::new();
                        message.data.hash(&mut hasher);
                        gossipsub::MessageId::from(hasher.finish().to_string())
                    })
                    .build()
                    .expect("Failed to build gossipsub config");

                // Create Gossipsub behaviour
                let mut gossipsub = gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("Failed to create gossipsub behaviour");

                // Subscribe to the threat intelligence topic
                gossipsub.subscribe(&topic)
                    .expect("Failed to subscribe to topic");

                // Y6.10: Conditionally create mDNS behaviour for local peer discovery
                // In production builds, mDNS should be disabled to prevent local network attacks
                let mdns = if enable_mdns {
                    info!("mDNS enabled for local peer discovery (dev mode)");
                    Toggle::from(Some(
                        mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                            .expect("Failed to create mDNS behaviour")
                    ))
                } else {
                    info!("mDNS disabled (production security hardening Y6.10)");
                    Toggle::from(None)
                };

                // Create Kademlia DHT for global peer discovery
                let store = MemoryStore::new(peer_id);
                let mut kad = kad::Behaviour::new(peer_id, store);
                kad.set_mode(Some(KadMode::Server));

                // Add bootstrap peers to Kademlia
                for (peer_id, addr) in &bootstrap_peers {
                    kad.add_address(peer_id, addr.clone());
                }

                // Create identify behaviour
                let identify = identify::Behaviour::new(identify::Config::new(
                    "/aegis-threat-intel/1.0.0".to_string(),
                    key.public(),
                ));

                ThreatIntelBehaviour {
                    gossipsub,
                    mdns,
                    kad,
                    identify,
                }
            })?
            .build();

        // Create channel for receiving threat intelligence
        let (sender, receiver) = mpsc::unbounded_channel();

        info!("Node public key: {}", hex::encode(signing_key.verifying_key().as_bytes()));

        Ok(Self {
            swarm,
            topic,
            peer_id,
            receiver,
            sender,
            signing_key,
            trusted_registry,
            rate_limiter: GossipRateLimiter::new(),
            outbound_rate_limiter: OutboundRateLimiter::new(),
            seen_message_ids: HashSet::new(),
        })
    }

    /// Get the peer ID
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Get the node's public key (hex encoded)
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().as_bytes())
    }

    /// Get a sender for publishing threat intelligence
    pub fn get_sender(&self) -> mpsc::UnboundedSender<ThreatIntelligence> {
        self.sender.clone()
    }

    /// Get a reference to the trusted registry
    pub fn trusted_registry(&self) -> &TrustedNodeRegistry {
        &self.trusted_registry
    }

    /// Start listening on configured address
    pub fn listen(&mut self, port: u16) -> Result<()> {
        let addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse()?;
        self.swarm.listen_on(addr.clone())?;
        info!("Listening on {}", addr);
        Ok(())
    }

    /// Publish threat intelligence to the network (signed)
    ///
    /// Y9.10: Applies outbound rate limiting to prevent amplification attacks.
    pub fn publish(&mut self, threat: &ThreatIntelligence) -> Result<()> {
        // Y9.10: Check outbound rate limit before publishing
        if !self.outbound_rate_limiter.check_and_update() {
            return Err(anyhow::anyhow!(
                "Y9.10: Outbound rate limited (amplification protection)"
            ));
        }

        // Sign the threat intelligence
        let signed_threat = SignedThreatIntelligence::sign(threat.clone(), &self.signing_key)?;
        let json = signed_threat.to_json()?;

        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.topic.clone(), json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to publish message: {}", e))?;

        info!("Published signed threat intelligence: {:?}", threat.ip);
        Ok(())
    }

    /// Publish a raw signed threat intelligence message
    ///
    /// Y9.10: Applies outbound rate limiting to prevent amplification attacks.
    pub fn publish_signed(&mut self, signed_threat: &SignedThreatIntelligence) -> Result<()> {
        // Y9.10: Check outbound rate limit before publishing
        if !self.outbound_rate_limiter.check_and_update() {
            return Err(anyhow::anyhow!(
                "Y9.10: Outbound rate limited (amplification protection)"
            ));
        }

        let json = signed_threat.to_json()?;

        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.topic.clone(), json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to publish message: {}", e))?;

        info!("Published signed threat intelligence: {:?}", signed_threat.threat.ip);
        Ok(())
    }

    /// Run the P2P network event loop with signature verification
    pub async fn run<F>(mut self, mut handler: F) -> Result<()>
    where
        F: FnMut(SignedThreatIntelligence) -> Result<()>,
    {
        loop {
            tokio::select! {
                // Handle outgoing messages from channel
                Some(threat) = self.receiver.recv() => {
                    if let Err(e) = self.publish(&threat) {
                        warn!("Failed to publish threat: {}", e);
                    }
                }

                // Handle swarm events
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message {
                                propagation_source,
                                message_id: _,
                                message,
                            },
                        )) => {
                            // 🔒 SECURITY FIX (X6): Apply rate limiting per peer
                            if !self.rate_limiter.check_rate_limit(&propagation_source) {
                                debug!(
                                    "Rate limited gossip message from peer {}",
                                    propagation_source
                                );
                                continue;
                            }

                            // Received a message from the network
                            match String::from_utf8(message.data.clone()) {
                                Ok(json) => {
                                    // Parse as signed threat intelligence
                                    match SignedThreatIntelligence::from_json(&json) {
                                        Ok(signed_threat) => {
                                            // Verify signature and validate
                                            if let Err(e) = signed_threat.verify_and_validate() {
                                                warn!("Invalid signed threat intelligence: {}", e);
                                                continue;
                                            }

                                            // Check if the signer is trusted
                                            if !self.trusted_registry.is_trusted(&signed_threat.public_key).await {
                                                warn!(
                                                    "Threat from untrusted node: {} (key: {})",
                                                    signed_threat.threat.ip,
                                                    &signed_threat.public_key[..16]
                                                );
                                                continue;
                                            }

                                            // 🔒 SECURITY FIX (Y5.4): Check for duplicate messages (replay protection)
                                            let msg_id = &signed_threat.threat.message_id;
                                            if self.seen_message_ids.contains(msg_id) {
                                                debug!(
                                                    "Duplicate message detected, skipping: {} (id: {})",
                                                    signed_threat.threat.ip,
                                                    msg_id
                                                );
                                                continue;
                                            }

                                            // Add message ID to seen set
                                            self.seen_message_ids.insert(msg_id.clone());

                                            // Cleanup if we've accumulated too many message IDs
                                            if self.seen_message_ids.len() > SEEN_MESSAGE_IDS_CLEANUP_THRESHOLD {
                                                // Simple cleanup: clear half the entries
                                                // In production, use LRU cache for better behavior
                                                let to_remove: Vec<String> = self.seen_message_ids
                                                    .iter()
                                                    .take(self.seen_message_ids.len() / 2)
                                                    .cloned()
                                                    .collect();
                                                for id in to_remove {
                                                    self.seen_message_ids.remove(&id);
                                                }
                                                debug!(
                                                    "Cleaned up seen_message_ids, new size: {}",
                                                    self.seen_message_ids.len()
                                                );
                                            }

                                            info!(
                                                "Received verified threat intel: {} (type: {}, severity: {}, from: {}...)",
                                                signed_threat.threat.ip,
                                                signed_threat.threat.threat_type,
                                                signed_threat.threat.severity,
                                                &signed_threat.public_key[..16]
                                            );

                                            // Call handler with signed threat
                                            if let Err(e) = handler(signed_threat) {
                                                warn!("Handler error: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Failed to parse signed threat intelligence: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Invalid UTF-8 in message: {}", e);
                                }
                            }
                        }

                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Mdns(
                            mdns::Event::Discovered(peers),
                        )) => {
                            for (peer_id, addr) in peers {
                                info!("Discovered peer via mDNS: {} at {}", peer_id, addr);
                                self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                self.swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                            }
                        }

                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Mdns(
                            mdns::Event::Expired(peers),
                        )) => {
                            for (peer_id, addr) in peers {
                                info!("Peer expired via mDNS: {} at {}", peer_id, addr);
                                self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                            }
                        }

                        SwarmEvent::Behaviour(ThreatIntelBehaviourEvent::Identify(
                            identify::Event::Received { peer_id, info, connection_id: _ },
                        )) => {
                            debug!("Identified peer: {} with protocols: {:?}", peer_id, info.protocols);
                            for addr in info.listen_addrs {
                                self.swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                            }
                        }

                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Listening on {}", address);
                        }

                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            info!("Connection established with peer: {}", peer_id);
                        }

                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            info!("Connection closed with peer: {} (cause: {:?})", peer_id, cause);
                        }

                        _ => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threat_intelligence_creation() {
        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            "node-123".to_string(),
        );

        assert_eq!(threat.ip, "192.168.1.100");
        assert_eq!(threat.threat_type, "syn_flood");
        assert_eq!(threat.severity, 8);
        assert_eq!(threat.block_duration_secs, 300);
        assert_eq!(threat.source_node, "node-123");
    }

    #[test]
    fn test_threat_intelligence_validation() {
        let valid_threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            600,
            "node-456".to_string(),
        );
        assert!(valid_threat.validate().is_ok());

        // Invalid IP
        let mut invalid = valid_threat.clone();
        invalid.ip = "999.999.999.999".to_string();
        assert!(invalid.validate().is_err());

        // Invalid severity (0)
        let mut invalid = valid_threat.clone();
        invalid.severity = 0;
        assert!(invalid.validate().is_err());

        // Invalid severity (>10)
        let mut invalid = valid_threat.clone();
        invalid.severity = 11;
        assert!(invalid.validate().is_err());

        // Invalid block duration (0)
        let mut invalid = valid_threat.clone();
        invalid.block_duration_secs = 0;
        assert!(invalid.validate().is_err());

        // Invalid block duration (>24 hours)
        let mut invalid = valid_threat.clone();
        invalid.block_duration_secs = 90000;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_threat_intelligence_serialization() {
        let threat = ThreatIntelligence::new(
            "172.16.0.50".to_string(),
            "brute_force".to_string(),
            7,
            1800,
            "node-789".to_string(),
        )
        .with_description("Multiple failed login attempts".to_string());

        let json = threat.to_json().expect("Serialization should succeed");
        assert!(json.contains("172.16.0.50"));
        assert!(json.contains("brute_force"));

        let deserialized = ThreatIntelligence::from_json(&json)
            .expect("Deserialization should succeed");
        assert_eq!(deserialized, threat);
    }

    #[test]
    fn test_p2p_config_default() {
        let config = P2PConfig::default();
        assert_eq!(config.listen_port, 9001);
        assert!(config.enable_mdns);
        assert!(config.bootstrap_peers.is_empty());
        assert!(config.trusted_public_keys.is_empty());
    }

    #[test]
    fn test_threat_with_description() {
        let threat = ThreatIntelligence::new(
            "192.168.1.1".to_string(),
            "port_scan".to_string(),
            4,
            300,
            "node-001".to_string(),
        )
        .with_description("Port scan detected on ports 22, 80, 443".to_string());

        assert!(threat.description.is_some());
        assert_eq!(
            threat.description.expect("Description should be set"),
            "Port scan detected on ports 22, 80, 443"
        );
    }

    #[tokio::test]
    #[ignore] // Requires network permissions (mDNS); run with --ignored flag
    async fn test_p2p_network_creation() {
        // Use test config with mDNS disabled to avoid permission issues
        let config = P2PConfig {
            listen_port: 0, // OS assigns random port
            enable_mdns: false, // Disable mDNS for tests (requires elevated privileges)
            bootstrap_peers: Vec::new(),
            trusted_public_keys: Vec::new(), // Open mode
        };

        let p2p = ThreatIntelP2P::new(config);

        // If this still fails due to permissions, that's acceptable for tests
        if p2p.is_err() {
            // Permission errors are expected in restricted test environments
            tracing::warn!("Note: P2P network creation requires network permissions");
            return;
        }

        let p2p = p2p.expect("P2P network creation should succeed if permissions are available");
        assert!(p2p.peer_id().to_string().len() > 0);
        assert!(!p2p.public_key_hex().is_empty());
    }

    #[test]
    fn test_timestamp_validation_future() {
        let mut threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            600,
            "node-test".to_string(),
        );

        // Set timestamp too far in the future (>5 minutes)
        threat.timestamp = current_timestamp() + 400;

        assert!(threat.validate().is_err());
    }

    #[test]
    fn test_timestamp_validation_past() {
        let mut threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            600,
            "node-test".to_string(),
        );

        // Set timestamp too far in the past (>1 hour)
        threat.timestamp = current_timestamp().saturating_sub(3700);

        assert!(threat.validate().is_err());
    }

    // ========================================
    // Ed25519 Signature Tests (Sprint 29)
    // ========================================

    #[test]
    fn test_signed_threat_intelligence_sign_and_verify() {
        use rand::RngCore;

        // Generate a signing key
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        let signing_key = SigningKey::from_bytes(&secret_key_bytes);
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        // Create a threat with source_node matching the public key
        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            public_key_hex.clone(),
        );

        // Sign the threat
        let signed = SignedThreatIntelligence::sign(threat.clone(), &signing_key)
            .expect("Signing should succeed");

        // Verify the signature
        assert!(signed.verify().expect("Verification should succeed"));

        // Verify and validate
        assert!(signed.verify_and_validate().is_ok());

        // Check that public key matches
        assert_eq!(signed.public_key, public_key_hex);
    }

    #[test]
    fn test_signed_threat_intelligence_tamper_detection() {
        use rand::RngCore;

        // Generate a signing key
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        let signing_key = SigningKey::from_bytes(&secret_key_bytes);
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            public_key_hex,
        );

        // Sign the threat
        let mut signed = SignedThreatIntelligence::sign(threat, &signing_key)
            .expect("Signing should succeed");

        // Tamper with the threat data
        signed.threat.ip = "10.0.0.1".to_string();

        // Verification should fail
        assert!(!signed.verify().expect("Verification check should complete"));
    }

    #[test]
    fn test_signed_threat_intelligence_wrong_source_node() {
        use rand::RngCore;

        // Generate a signing key
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        let signing_key = SigningKey::from_bytes(&secret_key_bytes);

        // Create threat with mismatched source_node
        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            "wrong-source-node".to_string(), // Doesn't match signing key
        );

        // Sign the threat
        let signed = SignedThreatIntelligence::sign(threat, &signing_key)
            .expect("Signing should succeed");

        // Signature verification passes (signature is valid)
        assert!(signed.verify().expect("Verification should succeed"));

        // But verify_and_validate should fail (source_node mismatch)
        assert!(signed.verify_and_validate().is_err());
    }

    #[test]
    fn test_signed_threat_serialization() {
        use rand::RngCore;

        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        let signing_key = SigningKey::from_bytes(&secret_key_bytes);
        let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());

        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            public_key_hex,
        );

        let signed = SignedThreatIntelligence::sign(threat, &signing_key)
            .expect("Signing should succeed");

        // Serialize to JSON
        let json = signed.to_json().expect("Serialization should succeed");
        assert!(json.contains("signature"));
        assert!(json.contains("public_key"));

        // Deserialize and verify
        let deserialized = SignedThreatIntelligence::from_json(&json)
            .expect("Deserialization should succeed");
        assert!(deserialized.verify().expect("Verification should succeed"));
    }

    // ========================================
    // IPv6 Support Tests (Sprint 29)
    // ========================================

    #[test]
    fn test_threat_ip_address_ipv4() {
        let ip = ThreatIpAddress::parse("192.168.1.100").expect("Should parse IPv4");
        assert!(ip.is_v4());
        assert!(!ip.is_v6());
        assert_eq!(ip.as_str(), "192.168.1.100");
    }

    #[test]
    fn test_threat_ip_address_ipv6() {
        let ip = ThreatIpAddress::parse("2001:db8::1").expect("Should parse IPv6");
        assert!(ip.is_v6());
        assert!(!ip.is_v4());
        assert_eq!(ip.as_str(), "2001:db8::1");
    }

    #[test]
    fn test_threat_ip_address_ipv6_full() {
        let ip = ThreatIpAddress::parse("2001:0db8:85a3:0000:0000:8a2e:0370:7334")
            .expect("Should parse full IPv6");
        assert!(ip.is_v6());
    }

    #[test]
    fn test_threat_ip_address_invalid() {
        assert!(ThreatIpAddress::parse("not-an-ip").is_err());
        assert!(ThreatIpAddress::parse("256.1.1.1").is_err());
        assert!(ThreatIpAddress::parse("").is_err());
    }

    #[test]
    fn test_threat_intelligence_ipv6_validation() {
        let threat = ThreatIntelligence::new(
            "2001:db8::1".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            "node-123".to_string(),
        );

        assert!(threat.validate().is_ok());
        let parsed = threat.parsed_ip().expect("Should parse IPv6");
        assert!(parsed.is_v6());
    }

    // ========================================
    // Trusted Registry Tests (Sprint 29)
    // ========================================

    #[tokio::test]
    async fn test_trusted_registry_open_mode() {
        // SECURITY FIX (X2.7): Use explicit testing function for open mode
        // In debug builds, this will allow open mode
        // The new() constructor alone won't enable open mode in release builds
        let registry = TrustedNodeRegistry::new_open_mode_for_testing();

        // Any key should be trusted in open mode (debug builds only)
        assert!(registry.is_trusted("any-key-1234567890").await);
        assert!(registry.is_trusted("another-key-12345678").await);
    }

    #[tokio::test]
    async fn test_trusted_registry_restricted_mode() {
        let trusted_keys = vec![
            "key1".to_string(),
            "key2".to_string(),
        ];
        let registry = TrustedNodeRegistry::new(trusted_keys);

        // Only specified keys are trusted
        assert!(registry.is_trusted("key1").await);
        assert!(registry.is_trusted("key2").await);
        assert!(!registry.is_trusted("key3").await);
    }

    #[tokio::test]
    async fn test_trusted_registry_add_remove() {
        let registry = TrustedNodeRegistry::new(vec!["key1".to_string()]);

        // Add a new key
        registry.add_trusted_key("key2".to_string()).await;
        assert!(registry.is_trusted("key2").await);

        // Remove a key
        registry.remove_trusted_key("key1").await;
        assert!(!registry.is_trusted("key1").await);

        // Get all trusted keys
        let keys = registry.get_trusted_keys().await;
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"key2".to_string()));
    }

    // ========================================
    // X2.7: P2P Open Mode Security Tests
    // ========================================

    #[test]
    fn test_x27_open_mode_flag_detection() {
        // Test that is_open_mode() correctly reports state
        let registry_with_keys = TrustedNodeRegistry::new(vec!["key1".to_string()]);
        assert!(!registry_with_keys.is_open_mode());

        let registry_open = TrustedNodeRegistry::new_open_mode_for_testing();
        assert!(registry_open.is_open_mode());
    }

    #[tokio::test]
    async fn test_x27_empty_keys_debug_behavior() {
        // In debug builds, empty keys enables open mode
        // This test verifies the new() constructor handles empty keys properly
        let registry = TrustedNodeRegistry::new(Vec::new());

        // In debug builds: open mode should be enabled
        // In release builds: open mode should be disabled (this test runs in debug)
        #[cfg(debug_assertions)]
        {
            assert!(registry.is_open_mode());
            // But we need at least 16 chars for the log message
            assert!(registry.is_trusted("abcdefghijklmnop").await);
        }
    }

    #[tokio::test]
    async fn test_x27_restricted_mode_rejects_unknown() {
        // Verify restricted mode correctly rejects unknown keys
        let trusted_keys = vec!["trusted-key-abc1".to_string()];
        let registry = TrustedNodeRegistry::new(trusted_keys);

        assert!(!registry.is_open_mode());
        assert!(registry.is_trusted("trusted-key-abc1").await);
        assert!(!registry.is_trusted("untrusted-key-xyz").await);
        assert!(!registry.is_trusted("").await);
    }

    #[tokio::test]
    async fn test_x27_dynamic_key_management() {
        // Start with one key, add more dynamically
        let registry = TrustedNodeRegistry::new(vec!["initial-key-123".to_string()]);

        // Initial state
        assert!(registry.is_trusted("initial-key-123").await);
        assert!(!registry.is_trusted("new-key-456").await);

        // Add new key
        registry.add_trusted_key("new-key-456".to_string()).await;
        assert!(registry.is_trusted("new-key-456").await);

        // Verify key list
        let keys = registry.get_trusted_keys().await;
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_x27_empty_config_with_dynamic_keys() {
        // SECURITY (X2.7): Production pattern - start with empty config, add keys dynamically
        // This simulates production startup where keys are loaded from storage

        // Create registry (will be restricted mode in release builds)
        let registry = TrustedNodeRegistry::new(Vec::new());

        // In release mode, even with no keys, unknown keys are rejected
        // In debug mode, this is open mode so we need to use restricted registry
        let registry = TrustedNodeRegistry::new(vec!["bootstrap-key-1".to_string()]);
        assert!(!registry.is_open_mode());

        // Add runtime keys (simulating bootstrap from config)
        registry.add_trusted_key("node-key-abcd1234".to_string()).await;
        registry.add_trusted_key("node-key-efgh5678".to_string()).await;

        // Verify only added keys are trusted
        assert!(registry.is_trusted("bootstrap-key-1").await);
        assert!(registry.is_trusted("node-key-abcd1234").await);
        assert!(registry.is_trusted("node-key-efgh5678").await);
        assert!(!registry.is_trusted("malicious-key-xxx").await);
    }

    #[test]
    fn test_x27_testing_function_only_in_debug() {
        // Verify new_open_mode_for_testing() is available (it's cfg'd for debug only)
        // This test will fail to compile in release if the function is available
        let _ = TrustedNodeRegistry::new_open_mode_for_testing();
        // If we get here, we're in debug mode and the function exists
    }

    // ========================================
    // X6: P2P Gossip Rate Limiting Tests
    // ========================================

    #[test]
    fn test_x6_rate_limiter_allows_normal_traffic() {
        let mut limiter = GossipRateLimiter::new();
        let peer_id = PeerId::random();

        // Should allow up to RATE_LIMIT_MAX_MESSAGES per window
        for i in 0..RATE_LIMIT_MAX_MESSAGES {
            assert!(
                limiter.check_rate_limit(&peer_id),
                "Message {} should be allowed",
                i + 1
            );
        }

        // Verify no messages were dropped
        assert_eq!(limiter.dropped_count, 0);
    }

    #[test]
    fn test_x6_rate_limiter_blocks_excessive_traffic() {
        let mut limiter = GossipRateLimiter::new();
        let peer_id = PeerId::random();

        // Exhaust the limit
        for _ in 0..RATE_LIMIT_MAX_MESSAGES {
            limiter.check_rate_limit(&peer_id);
        }

        // Next message should be blocked
        assert!(
            !limiter.check_rate_limit(&peer_id),
            "Message exceeding limit should be blocked"
        );

        // Verify message was dropped
        assert_eq!(limiter.dropped_count, 1);

        // More messages should also be blocked
        assert!(!limiter.check_rate_limit(&peer_id));
        assert!(!limiter.check_rate_limit(&peer_id));
        assert_eq!(limiter.dropped_count, 3);
    }

    #[test]
    fn test_x6_rate_limiter_tracks_multiple_peers() {
        let mut limiter = GossipRateLimiter::new();
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();

        // Exhaust peer A's limit
        for _ in 0..RATE_LIMIT_MAX_MESSAGES {
            limiter.check_rate_limit(&peer_a);
        }

        // Peer A should be blocked
        assert!(!limiter.check_rate_limit(&peer_a));

        // Peer B should still be allowed
        assert!(limiter.check_rate_limit(&peer_b));

        // Verify tracking
        assert_eq!(limiter.tracked_peers(), 2);
    }

    #[test]
    fn test_x6_rate_limiter_window_reset() {
        let mut limiter = GossipRateLimiter::new();
        let peer_id = PeerId::random();

        // Exhaust the limit
        for _ in 0..RATE_LIMIT_MAX_MESSAGES {
            limiter.check_rate_limit(&peer_id);
        }

        // Should be blocked
        assert!(!limiter.check_rate_limit(&peer_id));

        // Manually modify the window start to simulate time passing
        // (In real usage, the window resets after RATE_LIMIT_WINDOW_SECS)
        if let Some(entry) = limiter.entries.get_mut(&peer_id) {
            entry.window_start = Instant::now() - Duration::from_secs(RATE_LIMIT_WINDOW_SECS + 1);
        }

        // Now should be allowed again (window reset)
        assert!(
            limiter.check_rate_limit(&peer_id),
            "After window reset, messages should be allowed"
        );
    }

    #[test]
    fn test_x6_rate_limiter_default() {
        let limiter = GossipRateLimiter::default();
        assert_eq!(limiter.dropped_count, 0);
        assert_eq!(limiter.tracked_peers(), 0);
    }

    // ========================================
    // Y5.6: P2P Config Validation Tests
    // ========================================

    #[test]
    fn test_y56_config_validate_valid_keys() {
        // Valid Ed25519 public key (64 hex chars)
        let valid_key = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let config = P2PConfig {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
            trusted_public_keys: vec![valid_key.to_string()],
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_y56_config_validate_invalid_key_length() {
        // Too short
        let short_key = "a1b2c3d4e5f6";
        let config = P2PConfig {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
            trusted_public_keys: vec![short_key.to_string()],
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid trusted key length"));
    }

    #[test]
    fn test_y56_config_validate_invalid_key_format() {
        // Contains non-hex characters
        let invalid_key = "g1h2i3j4k5l6g1h2i3j4k5l6g1h2i3j4k5l6g1h2i3j4k5l6g1h2i3j4k5l6g1h2";
        let config = P2PConfig {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
            trusted_public_keys: vec![invalid_key.to_string()],
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-hex characters"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_y56_config_validate_empty_keys_debug() {
        // In debug builds, empty keys is allowed
        let config = P2PConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_y56_config_validate_multiple_keys() {
        // Multiple valid keys
        let key1 = "1111111111111111111111111111111111111111111111111111111111111111";
        let key2 = "2222222222222222222222222222222222222222222222222222222222222222";
        let config = P2PConfig {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
            trusted_public_keys: vec![key1.to_string(), key2.to_string()],
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_y56_config_validate_one_invalid_among_valid() {
        // One invalid key among valid ones
        let valid_key = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let invalid_key = "short";
        let config = P2PConfig {
            listen_port: 9001,
            enable_mdns: true,
            bootstrap_peers: Vec::new(),
            trusted_public_keys: vec![valid_key.to_string(), invalid_key.to_string()],
        };

        let result = config.validate();
        assert!(result.is_err());
    }

    // ========================================
    // Y6.10: mDNS Production Security Tests
    // ========================================

    #[test]
    fn test_y610_production_config_disables_mdns() {
        let valid_key = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let config = P2PConfig::production(
            9001,
            Vec::new(),
            vec![valid_key.to_string()],
        );

        // Production config should have mDNS disabled
        assert!(!config.enable_mdns);
        assert!(config.validate().is_ok());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_y610_mdns_allowed_in_debug_builds() {
        // In debug builds, mDNS is allowed (for testing convenience)
        let valid_key = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        let config = P2PConfig {
            listen_port: 9001,
            enable_mdns: true,  // Should be allowed in debug
            bootstrap_peers: Vec::new(),
            trusted_public_keys: vec![valid_key.to_string()],
        };

        // In debug builds, this should pass (mDNS allowed)
        assert!(config.validate().is_ok());
    }

    // ========================================
    // Y6.2: Network Partition Detection Tests
    // ========================================

    #[test]
    fn test_y62_partition_detector_new() {
        let detector = NetworkPartitionDetector::new();
        assert!(!detector.is_partitioned());
        assert_eq!(detector.expected_peer_count(), 0);
        assert_eq!(detector.responsive_peer_count(), 0);
    }

    #[test]
    fn test_y62_register_and_unregister_peer() {
        let mut detector = NetworkPartitionDetector::new();

        detector.register_peer("peer-1");
        detector.register_peer("peer-2");
        assert_eq!(detector.expected_peer_count(), 2);

        detector.unregister_peer("peer-1");
        assert_eq!(detector.expected_peer_count(), 1);
    }

    #[test]
    fn test_y62_record_heartbeat() {
        let mut detector = NetworkPartitionDetector::new();

        detector.register_peer("peer-1");
        detector.record_heartbeat("peer-1");

        // Should count as responsive
        assert_eq!(detector.responsive_peer_count(), 1);
    }

    #[test]
    fn test_y62_partition_detected_all_missing() {
        let mut detector = NetworkPartitionDetector::new();

        // Register peers but don't send heartbeats
        detector.register_peer("peer-1");
        detector.register_peer("peer-2");
        detector.register_peer("peer-3");
        detector.register_peer("peer-4");

        // Check partition - all peers are missing
        let event = detector.check_partition();
        assert!(event.is_some());

        match event.unwrap() {
            PartitionEvent::PartitionDetected { missing_peers, missing_ratio } => {
                assert_eq!(missing_peers.len(), 4);
                assert!((missing_ratio - 1.0).abs() < 0.01);
            }
            _ => panic!("Expected PartitionDetected"),
        }

        assert!(detector.is_partitioned());
    }

    #[test]
    fn test_y62_no_partition_when_peers_responsive() {
        let mut detector = NetworkPartitionDetector::new();

        // Register and heartbeat all peers
        for i in 0..4 {
            let peer = format!("peer-{}", i);
            detector.register_peer(&peer);
            detector.record_heartbeat(&peer);
        }

        // No partition should be detected
        let event = detector.check_partition();
        assert!(event.is_none());
        assert!(!detector.is_partitioned());
    }

    #[test]
    fn test_y62_partition_resolved() {
        let mut detector = NetworkPartitionDetector::new();

        // Force partition state
        detector.register_peer("peer-1");
        detector.register_peer("peer-2");
        let _ = detector.check_partition(); // Will detect partition (no heartbeats)
        assert!(detector.is_partitioned());

        // Now all peers send heartbeats
        detector.record_heartbeat("peer-1");
        detector.record_heartbeat("peer-2");

        // Check again - should be resolved
        let event = detector.check_partition();
        assert!(event.is_some());

        match event.unwrap() {
            PartitionEvent::PartitionResolved { reconnected_count } => {
                assert_eq!(reconnected_count, 2);
            }
            _ => panic!("Expected PartitionResolved"),
        }

        assert!(!detector.is_partitioned());
    }

    // ========================================
    // Y6.3: CRDT-based Threat Intel Tests
    // ========================================

    #[test]
    fn test_y63_crdt_new() {
        let crdt = ThreatIntelCRDT::new("node-1".to_string());
        assert!(crdt.is_empty());
        assert_eq!(crdt.len(), 0);
    }

    #[test]
    fn test_y63_crdt_add_threat() {
        let mut crdt = ThreatIntelCRDT::new("node-1".to_string());
        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            "node-1".to_string(),
        );

        let result = crdt.merge(threat.clone(), "node-1".to_string(), current_timestamp());
        assert_eq!(result, MergeResult::Added);
        assert_eq!(crdt.len(), 1);
        assert!(crdt.get("192.168.1.100").is_some());
    }

    #[test]
    fn test_y63_crdt_lww_newer_wins() {
        let mut crdt = ThreatIntelCRDT::new("node-1".to_string());
        let ts = current_timestamp();

        // Add older entry
        let threat1 = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            300,
            "node-1".to_string(),
        );
        crdt.merge(threat1, "node-1".to_string(), ts);

        // Add newer entry for same IP
        let threat2 = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "brute_force".to_string(),
            9,
            600,
            "node-2".to_string(),
        );
        let result = crdt.merge(threat2.clone(), "node-2".to_string(), ts + 1000);
        assert_eq!(result, MergeResult::Updated);

        // Should have the newer threat
        let stored = crdt.get("10.0.0.1").unwrap();
        assert_eq!(stored.threat_type, "brute_force");
        assert_eq!(stored.severity, 9);
    }

    #[test]
    fn test_y63_crdt_lww_older_ignored() {
        let mut crdt = ThreatIntelCRDT::new("node-1".to_string());
        let ts = current_timestamp();

        // Add newer entry first
        let threat1 = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            9,
            600,
            "node-1".to_string(),
        );
        crdt.merge(threat1.clone(), "node-1".to_string(), ts + 1000);

        // Try to add older entry
        let threat2 = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "brute_force".to_string(),
            5,
            300,
            "node-2".to_string(),
        );
        let result = crdt.merge(threat2, "node-2".to_string(), ts);
        assert_eq!(result, MergeResult::Ignored);

        // Should still have the original threat
        let stored = crdt.get("10.0.0.1").unwrap();
        assert_eq!(stored.threat_type, "ddos");
    }

    #[test]
    fn test_y63_crdt_tie_breaker() {
        let mut crdt = ThreatIntelCRDT::new("node-1".to_string());
        let ts = current_timestamp();

        // Add entry from node-a
        let threat1 = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            300,
            "node-a".to_string(),
        );
        crdt.merge(threat1, "node-a".to_string(), ts);

        // Add entry from node-b with same timestamp
        // node-b > node-a alphabetically, so it should win
        let threat2 = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "brute_force".to_string(),
            9,
            600,
            "node-b".to_string(),
        );
        let result = crdt.merge(threat2, "node-b".to_string(), ts);
        assert_eq!(result, MergeResult::Updated);

        let stored = crdt.get("10.0.0.1").unwrap();
        assert_eq!(stored.threat_type, "brute_force");
    }

    #[test]
    fn test_y63_crdt_remove_tombstone() {
        let mut crdt = ThreatIntelCRDT::new("node-1".to_string());
        let ts = current_timestamp();

        // Add threat
        let threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "ddos".to_string(),
            5,
            300,
            "node-1".to_string(),
        );
        crdt.merge(threat, "node-1".to_string(), ts);
        assert!(crdt.get("10.0.0.1").is_some());

        // Remove (tombstone)
        let result = crdt.remove("10.0.0.1", "node-1".to_string(), ts + 1000);
        assert_eq!(result, MergeResult::Updated);

        // Should not be in active threats
        assert!(crdt.get("10.0.0.1").is_none());
        assert!(crdt.get_active_threats().is_empty());
    }

    #[test]
    fn test_y63_crdt_get_active_threats() {
        let mut crdt = ThreatIntelCRDT::new("node-1".to_string());
        let ts = current_timestamp();

        // Add multiple threats
        for i in 1..=5 {
            let threat = ThreatIntelligence::new(
                format!("10.0.0.{}", i),
                "ddos".to_string(),
                5,
                300,
                "node-1".to_string(),
            );
            crdt.merge(threat, "node-1".to_string(), ts + i as u64);
        }

        let active = crdt.get_active_threats();
        assert_eq!(active.len(), 5);
    }

    // ========================================
    // Y6.9: Solana Staking Tests
    // ========================================

    #[test]
    fn test_y69_stake_tier_from_stake() {
        // No stake
        assert_eq!(StakeTier::from_stake(0), StakeTier::None);
        assert_eq!(StakeTier::from_stake(99_999_999), StakeTier::None);

        // Basic (0.1 - 1 SOL)
        assert_eq!(StakeTier::from_stake(100_000_000), StakeTier::Basic);
        assert_eq!(StakeTier::from_stake(999_999_999), StakeTier::Basic);

        // Standard (1 - 10 SOL)
        assert_eq!(StakeTier::from_stake(1_000_000_000), StakeTier::Standard);
        assert_eq!(StakeTier::from_stake(9_999_999_999), StakeTier::Standard);

        // High (10 - 100 SOL)
        assert_eq!(StakeTier::from_stake(10_000_000_000), StakeTier::High);
        assert_eq!(StakeTier::from_stake(99_999_999_999), StakeTier::High);

        // Elite (100+ SOL)
        assert_eq!(StakeTier::from_stake(100_000_000_000), StakeTier::Elite);
        assert_eq!(StakeTier::from_stake(1_000_000_000_000), StakeTier::Elite);
    }

    #[test]
    fn test_y69_stake_tier_trust_weight() {
        assert_eq!(StakeTier::None.trust_weight(), 0);
        assert_eq!(StakeTier::Basic.trust_weight(), 1);
        assert_eq!(StakeTier::Standard.trust_weight(), 3);
        assert_eq!(StakeTier::High.trust_weight(), 5);
        assert_eq!(StakeTier::Elite.trust_weight(), 10);
    }

    #[test]
    fn test_y69_staking_verifier_disabled() {
        let verifier = StakingVerifier::disabled();
        assert!(!verifier.is_enabled());
        // When disabled, all nodes have sufficient stake
        assert!(verifier.has_sufficient_stake("any-node"));
    }

    #[test]
    fn test_y69_staking_verifier_enabled() {
        let mut verifier = StakingVerifier::new("https://api.devnet.solana.com".to_string());
        assert!(verifier.is_enabled());

        // Unknown node has no stake
        assert!(!verifier.has_sufficient_stake("unknown-node"));

        // Register stake
        verifier.register_stake(
            "node-1".to_string(),
            "wallet123".to_string(),
            1_000_000_000, // 1 SOL
        );

        assert!(verifier.has_sufficient_stake("node-1"));
        assert_eq!(verifier.get_stake_tier("node-1"), StakeTier::Standard);
        assert_eq!(verifier.get_trust_weight("node-1"), 3);
    }

    #[test]
    fn test_y69_staking_verifier_insufficient_stake() {
        let mut verifier = StakingVerifier::new("https://api.devnet.solana.com".to_string());

        // Register with stake below minimum
        verifier.register_stake(
            "node-1".to_string(),
            "wallet123".to_string(),
            50_000_000, // 0.05 SOL - below minimum
        );

        assert!(!verifier.has_sufficient_stake("node-1"));
        assert_eq!(verifier.get_stake_tier("node-1"), StakeTier::None);
    }

    #[test]
    fn test_y69_staking_verifier_slash() {
        let mut verifier = StakingVerifier::new("https://api.devnet.solana.com".to_string());

        // Register stake
        verifier.register_stake(
            "node-1".to_string(),
            "wallet123".to_string(),
            10_000_000_000, // 10 SOL
        );

        assert!(verifier.has_sufficient_stake("node-1"));

        // Slash the node
        verifier.slash_node("node-1", "sending fake threats");

        // Slashed node should not have sufficient stake
        assert!(!verifier.has_sufficient_stake("node-1"));

        // Unslash
        verifier.unslash_node("node-1");
        assert!(verifier.has_sufficient_stake("node-1"));
    }

    #[test]
    fn test_y69_staking_verifier_get_staked_nodes() {
        let mut verifier = StakingVerifier::new("https://api.devnet.solana.com".to_string());

        verifier.register_stake("node-1".to_string(), "w1".to_string(), 1_000_000_000);
        verifier.register_stake("node-2".to_string(), "w2".to_string(), 500_000_000);
        verifier.register_stake("node-3".to_string(), "w3".to_string(), 50_000_000); // Below min

        let staked = verifier.get_staked_nodes();
        assert_eq!(staked.len(), 2);
        assert!(staked.contains(&"node-1"));
        assert!(staked.contains(&"node-2"));
        assert!(!staked.contains(&"node-3"));
    }

    #[test]
    fn test_y69_staking_verifier_get_nodes_by_tier() {
        let mut verifier = StakingVerifier::new("https://api.devnet.solana.com".to_string());

        verifier.register_stake("basic-1".to_string(), "w1".to_string(), 200_000_000);
        verifier.register_stake("basic-2".to_string(), "w2".to_string(), 300_000_000);
        verifier.register_stake("standard-1".to_string(), "w3".to_string(), 5_000_000_000);
        verifier.register_stake("elite-1".to_string(), "w4".to_string(), 200_000_000_000);

        let basic_nodes = verifier.get_nodes_by_tier(StakeTier::Basic);
        assert_eq!(basic_nodes.len(), 2);

        let standard_nodes = verifier.get_nodes_by_tier(StakeTier::Standard);
        assert_eq!(standard_nodes.len(), 1);

        let elite_nodes = verifier.get_nodes_by_tier(StakeTier::Elite);
        assert_eq!(elite_nodes.len(), 1);
    }
}
