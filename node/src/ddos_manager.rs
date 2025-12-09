/// DDoS Protection Manager
///
/// Central orchestration layer for DDoS protection that integrates:
/// - eBPF/XDP kernel-level filtering (Sprint 7)
/// - P2P threat intelligence (Sprint 10)
/// - Distributed rate limiting (Sprint 11)
/// - JavaScript challenges (Sprint 20)
///
/// This manager provides a unified API for:
/// - Policy management (CRUD operations)
/// - Blocklist/allowlist management
/// - Rate limit checking
/// - Statistics aggregation

use crate::ddos_policy::{
    AllowlistEntry, BlockSource, BlocklistEntry, DDoSPolicy, DDoSPolicyError,
    DDoSPolicyUpdate, RateLimitPolicy, RateLimitScope,
};
use crate::ddos_stats::{AttackEvent, AttackType, DDoSStats, GlobalStats, SseEvent};
use crate::distributed_rate_limiter::{DistributedRateLimiter, RateLimitDecision, RateLimiterConfig};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

// =============================================================================
// CONFIGURATION
// =============================================================================

/// DDoS Manager configuration
#[derive(Debug, Clone)]
pub struct DDoSManagerConfig {
    /// Default SYN flood threshold
    pub default_syn_threshold: u64,
    /// Default UDP flood threshold
    pub default_udp_threshold: u64,
    /// Default block duration in seconds
    pub default_block_duration_secs: u64,
    /// Enable P2P threat intelligence
    pub enable_threat_intel: bool,
    /// Enable eBPF integration (Linux only)
    pub enable_ebpf: bool,
    /// Rate limiter actor ID for this node
    pub rate_limiter_actor_id: u64,
}

impl Default for DDoSManagerConfig {
    fn default() -> Self {
        Self {
            default_syn_threshold: 100,
            default_udp_threshold: 1000,
            default_block_duration_secs: 300,
            enable_threat_intel: true,
            enable_ebpf: cfg!(target_os = "linux"),
            rate_limiter_actor_id: 1,
        }
    }
}

// =============================================================================
// RATE LIMIT CHECK RESULT
// =============================================================================

/// Result of a rate limit check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Current request count
    pub current_count: u64,
    /// Remaining requests in window
    pub remaining: u64,
    /// Seconds until window resets
    pub retry_after_secs: u64,
    /// Rate limit scope used
    pub scope: RateLimitScope,
}

impl RateLimitResult {
    /// Create an allowed result
    pub fn allowed(current: u64, remaining: u64) -> Self {
        Self {
            allowed: true,
            current_count: current,
            remaining,
            retry_after_secs: 0,
            scope: RateLimitScope::PerIp,
        }
    }

    /// Create a denied result
    pub fn denied(current: u64, retry_after: u64) -> Self {
        Self {
            allowed: false,
            current_count: current,
            remaining: 0,
            retry_after_secs: retry_after,
            scope: RateLimitScope::PerIp,
        }
    }
}

// =============================================================================
// BLOCKLIST STORAGE
// =============================================================================

/// In-memory blocklist storage with expiration
struct BlocklistStorage {
    /// IPv4 blocklist
    ipv4: HashMap<String, BlocklistEntry>,
    /// IPv6 blocklist
    ipv6: HashMap<String, BlocklistEntry>,
}

impl BlocklistStorage {
    fn new() -> Self {
        Self {
            ipv4: HashMap::new(),
            ipv6: HashMap::new(),
        }
    }

    fn add(&mut self, entry: BlocklistEntry) {
        // Determine IPv4 or IPv6
        if let Ok(addr) = IpAddr::from_str(&entry.ip) {
            match addr {
                IpAddr::V4(_) => {
                    self.ipv4.insert(entry.ip.clone(), entry);
                }
                IpAddr::V6(_) => {
                    self.ipv6.insert(entry.ip.clone(), entry);
                }
            }
        } else {
            // Handle CIDR or assume IPv4
            if entry.ip.contains(':') {
                self.ipv6.insert(entry.ip.clone(), entry);
            } else {
                self.ipv4.insert(entry.ip.clone(), entry);
            }
        }
    }

    fn remove(&mut self, ip: &str) -> bool {
        self.ipv4.remove(ip).is_some() || self.ipv6.remove(ip).is_some()
    }

    fn get(&self, ip: &str) -> Option<&BlocklistEntry> {
        self.ipv4.get(ip).or_else(|| self.ipv6.get(ip))
    }

    fn contains(&self, ip: &str) -> bool {
        self.ipv4.contains_key(ip) || self.ipv6.contains_key(ip)
    }

    fn is_blocked(&self, ip: &str) -> bool {
        if let Some(entry) = self.get(ip) {
            !entry.is_expired()
        } else {
            false
        }
    }

    fn all_entries(&self) -> Vec<&BlocklistEntry> {
        self.ipv4.values().chain(self.ipv6.values()).collect()
    }

    fn remove_expired(&mut self) -> usize {
        let mut removed = 0;

        self.ipv4.retain(|_, entry| {
            if entry.is_expired() {
                removed += 1;
                false
            } else {
                true
            }
        });

        self.ipv6.retain(|_, entry| {
            if entry.is_expired() {
                removed += 1;
                false
            } else {
                true
            }
        });

        removed
    }

    fn count(&self) -> usize {
        self.ipv4.len() + self.ipv6.len()
    }
}

// =============================================================================
// ALLOWLIST STORAGE
// =============================================================================

/// In-memory allowlist storage
struct AllowlistStorage {
    entries: HashMap<String, AllowlistEntry>,
}

impl AllowlistStorage {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn add(&mut self, entry: AllowlistEntry) {
        self.entries.insert(entry.ip.clone(), entry);
    }

    fn remove(&mut self, ip: &str) -> bool {
        self.entries.remove(ip).is_some()
    }

    fn contains(&self, ip: &str) -> bool {
        // Check exact match
        if self.entries.contains_key(ip) {
            return true;
        }

        // Check CIDR matches
        if let Ok(addr) = IpAddr::from_str(ip) {
            for (pattern, _) in &self.entries {
                if matches_cidr(&addr, pattern) {
                    return true;
                }
            }
        }

        false
    }

    fn all_entries(&self) -> Vec<&AllowlistEntry> {
        self.entries.values().collect()
    }

    fn count(&self) -> usize {
        self.entries.len()
    }
}

/// Check if an IP matches a CIDR pattern
fn matches_cidr(ip: &IpAddr, pattern: &str) -> bool {
    // Simple CIDR matching
    if let Some((network, prefix_str)) = pattern.split_once('/') {
        if let (Ok(network_ip), Ok(prefix)) = (IpAddr::from_str(network), prefix_str.parse::<u8>()) {
            match (ip, network_ip) {
                (IpAddr::V4(ip_v4), IpAddr::V4(net_v4)) => {
                    if prefix > 32 {
                        return false;
                    }
                    let mask = if prefix == 0 { 0 } else { !0u32 << (32 - prefix) };
                    let ip_bits = u32::from(*ip_v4);
                    let net_bits = u32::from(net_v4);
                    (ip_bits & mask) == (net_bits & mask)
                }
                (IpAddr::V6(ip_v6), IpAddr::V6(net_v6)) => {
                    if prefix > 128 {
                        return false;
                    }
                    let ip_bits = u128::from(*ip_v6);
                    let net_bits = u128::from(net_v6);
                    let mask = if prefix == 0 { 0 } else { !0u128 << (128 - prefix) };
                    (ip_bits & mask) == (net_bits & mask)
                }
                _ => false,
            }
        } else {
            false
        }
    } else {
        // Exact match
        pattern == &ip.to_string()
    }
}

// =============================================================================
// DDOS MANAGER
// =============================================================================

/// Central DDoS protection manager
pub struct DDoSManager {
    /// Configuration
    config: DDoSManagerConfig,

    /// Per-domain policies
    policies: Arc<RwLock<HashMap<String, DDoSPolicy>>>,

    /// Global blocklist
    blocklist: Arc<RwLock<BlocklistStorage>>,

    /// Global allowlist
    allowlist: Arc<RwLock<AllowlistStorage>>,

    /// Statistics collector
    stats: Arc<DDoSStats>,

    /// Distributed rate limiter (optional)
    rate_limiter: Option<Arc<DistributedRateLimiter>>,

    /// SSE event broadcaster
    event_tx: broadcast::Sender<SseEvent>,
}

impl DDoSManager {
    /// Create a new DDoS manager with default configuration
    pub fn new() -> Self {
        Self::with_config(DDoSManagerConfig::default())
    }

    /// Create a new DDoS manager with custom configuration
    pub fn with_config(config: DDoSManagerConfig) -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            config,
            policies: Arc::new(RwLock::new(HashMap::new())),
            blocklist: Arc::new(RwLock::new(BlocklistStorage::new())),
            allowlist: Arc::new(RwLock::new(AllowlistStorage::new())),
            stats: Arc::new(DDoSStats::new()),
            rate_limiter: None,
            event_tx,
        }
    }

    /// Set the distributed rate limiter
    pub fn set_rate_limiter(&mut self, rate_limiter: Arc<DistributedRateLimiter>) {
        self.rate_limiter = Some(rate_limiter);
    }

    /// Get the statistics collector
    pub fn stats(&self) -> &Arc<DDoSStats> {
        &self.stats
    }

    /// Subscribe to SSE events
    pub fn subscribe_events(&self) -> broadcast::Receiver<SseEvent> {
        self.event_tx.subscribe()
    }

    // =========================================================================
    // POLICY MANAGEMENT
    // =========================================================================

    /// Get a policy by domain
    pub async fn get_policy(&self, domain: &str) -> Option<DDoSPolicy> {
        let policies = self.policies.read().await;
        policies.get(domain).cloned()
    }

    /// Create or update a policy
    pub async fn set_policy(&self, policy: DDoSPolicy) -> Result<(), DDoSPolicyError> {
        // Validate policy
        policy.validate()?;

        let domain = policy.domain.clone();
        let mut policies = self.policies.write().await;
        policies.insert(domain.clone(), policy);

        info!("DDoS policy set for domain: {}", domain);

        // Broadcast event
        let _ = self.event_tx.send(SseEvent::new(
            "policy_updated",
            serde_json::json!({"domain": domain}),
        ));

        Ok(())
    }

    /// Update an existing policy with partial update
    pub async fn update_policy(
        &self,
        domain: &str,
        update: DDoSPolicyUpdate,
    ) -> Result<DDoSPolicy, DDoSPolicyError> {
        let mut policies = self.policies.write().await;

        let policy = policies
            .get_mut(domain)
            .ok_or(DDoSPolicyError::EmptyDomain)?;

        policy.merge(update);
        policy.validate()?;

        info!("DDoS policy updated for domain: {}", domain);

        // Broadcast event
        let _ = self.event_tx.send(SseEvent::new(
            "policy_updated",
            serde_json::json!({"domain": domain}),
        ));

        Ok(policy.clone())
    }

    /// Delete a policy
    pub async fn delete_policy(&self, domain: &str) -> bool {
        let mut policies = self.policies.write().await;
        let removed = policies.remove(domain).is_some();

        if removed {
            info!("DDoS policy deleted for domain: {}", domain);

            let _ = self.event_tx.send(SseEvent::new(
                "policy_deleted",
                serde_json::json!({"domain": domain}),
            ));
        }

        removed
    }

    /// List all policies
    pub async fn list_policies(&self) -> Vec<DDoSPolicy> {
        let policies = self.policies.read().await;
        policies.values().cloned().collect()
    }

    // =========================================================================
    // BLOCKLIST MANAGEMENT
    // =========================================================================

    /// Add an IP to the blocklist
    pub async fn block_ip(
        &self,
        ip: &str,
        reason: &str,
        duration_secs: u64,
        source: BlockSource,
    ) -> Result<BlocklistEntry> {
        // Validate IP
        crate::ddos_policy::validate_ip_or_cidr(ip)?;

        let mut entry = BlocklistEntry::new(ip.to_string(), reason.to_string(), duration_secs);
        entry.source = source.clone();

        let mut blocklist = self.blocklist.write().await;
        blocklist.add(entry.clone());

        self.stats.record_ip_blocked();

        info!(
            "IP blocked: {} (reason: {}, duration: {}s, source: {:?})",
            ip, reason, duration_secs, source
        );

        // Broadcast event
        let _ = self.event_tx.send(SseEvent::new(
            "ip_blocked",
            serde_json::json!({
                "ip": ip,
                "reason": reason,
                "duration_secs": duration_secs,
                "source": source
            }),
        ));

        Ok(entry)
    }

    /// Remove an IP from the blocklist
    pub async fn unblock_ip(&self, ip: &str) -> bool {
        let mut blocklist = self.blocklist.write().await;
        let removed = blocklist.remove(ip);

        if removed {
            self.stats.record_ip_unblocked();
            info!("IP unblocked: {}", ip);

            let _ = self.event_tx.send(SseEvent::new(
                "ip_unblocked",
                serde_json::json!({"ip": ip}),
            ));
        }

        removed
    }

    /// Check if an IP is blocked
    pub async fn is_blocked(&self, ip: &str) -> bool {
        // Check allowlist first
        let allowlist = self.allowlist.read().await;
        if allowlist.contains(ip) {
            return false;
        }
        drop(allowlist);

        // Check blocklist
        let blocklist = self.blocklist.read().await;
        blocklist.is_blocked(ip)
    }

    /// Get blocklist entries (with pagination)
    pub async fn get_blocklist(&self, offset: usize, limit: usize) -> Vec<BlocklistEntry> {
        let blocklist = self.blocklist.read().await;
        blocklist
            .all_entries()
            .into_iter()
            .filter(|e| !e.is_expired())
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get blocklist count
    pub async fn blocklist_count(&self) -> usize {
        let blocklist = self.blocklist.read().await;
        blocklist.count()
    }

    /// Remove expired blocklist entries
    pub async fn cleanup_expired_blocks(&self) -> usize {
        let mut blocklist = self.blocklist.write().await;
        let removed = blocklist.remove_expired();

        if removed > 0 {
            debug!("Removed {} expired blocklist entries", removed);
        }

        removed
    }

    // =========================================================================
    // ALLOWLIST MANAGEMENT
    // =========================================================================

    /// Add an IP to the allowlist
    pub async fn allow_ip(&self, ip: &str, description: &str) -> Result<AllowlistEntry> {
        // Validate IP
        crate::ddos_policy::validate_ip_or_cidr(ip)?;

        let entry = AllowlistEntry::new(ip.to_string(), description.to_string());

        let mut allowlist = self.allowlist.write().await;
        allowlist.add(entry.clone());

        info!("IP allowlisted: {} ({})", ip, description);

        let _ = self.event_tx.send(SseEvent::new(
            "ip_allowlisted",
            serde_json::json!({
                "ip": ip,
                "description": description
            }),
        ));

        Ok(entry)
    }

    /// Remove an IP from the allowlist
    pub async fn remove_from_allowlist(&self, ip: &str) -> bool {
        let mut allowlist = self.allowlist.write().await;
        let removed = allowlist.remove(ip);

        if removed {
            info!("IP removed from allowlist: {}", ip);

            let _ = self.event_tx.send(SseEvent::new(
                "ip_allowlist_removed",
                serde_json::json!({"ip": ip}),
            ));
        }

        removed
    }

    /// Check if an IP is allowlisted
    pub async fn is_allowlisted(&self, ip: &str) -> bool {
        let allowlist = self.allowlist.read().await;
        allowlist.contains(ip)
    }

    /// Get allowlist entries
    pub async fn get_allowlist(&self) -> Vec<AllowlistEntry> {
        let allowlist = self.allowlist.read().await;
        allowlist.all_entries().into_iter().cloned().collect()
    }

    // =========================================================================
    // RATE LIMITING
    // =========================================================================

    /// Check rate limit for a request
    pub async fn check_rate_limit(
        &self,
        domain: &str,
        client_ip: &str,
    ) -> RateLimitResult {
        // Check if allowlisted (bypass rate limit)
        if self.is_allowlisted(client_ip).await {
            return RateLimitResult::allowed(0, u64::MAX);
        }

        // Get domain policy
        let policies = self.policies.read().await;
        let rate_limit_config = policies
            .get(domain)
            .and_then(|p| p.rate_limit.as_ref())
            .filter(|rl| rl.enabled);

        let Some(config) = rate_limit_config else {
            // No rate limit configured, allow
            return RateLimitResult::allowed(0, u64::MAX);
        };

        // Build resource ID based on scope
        let resource_id = match config.scope {
            RateLimitScope::PerIp => format!("{}:{}", domain, client_ip),
            RateLimitScope::PerRoute => domain.to_string(),
            RateLimitScope::Global => "global".to_string(),
        };

        // Use distributed rate limiter if available
        if let Some(ref rate_limiter) = self.rate_limiter {
            match rate_limiter.check_rate_limit(&resource_id).await {
                Ok(RateLimitDecision::Allowed { current_count, remaining }) => {
                    RateLimitResult {
                        allowed: true,
                        current_count,
                        remaining,
                        retry_after_secs: 0,
                        scope: config.scope.clone(),
                    }
                }
                Ok(RateLimitDecision::Denied { current_count, retry_after_secs }) => {
                    self.stats.record_rate_limited_for_domain(domain);

                    RateLimitResult {
                        allowed: false,
                        current_count,
                        remaining: 0,
                        retry_after_secs,
                        scope: config.scope.clone(),
                    }
                }
                Err(e) => {
                    warn!("Rate limiter error: {}", e);
                    // Fail open
                    RateLimitResult::allowed(0, u64::MAX)
                }
            }
        } else {
            // No rate limiter, allow
            RateLimitResult::allowed(0, u64::MAX)
        }
    }

    // =========================================================================
    // ATTACK RECORDING
    // =========================================================================

    /// Record a detected attack
    pub async fn record_attack(
        &self,
        attack_type: AttackType,
        source_ip: &str,
        severity: u8,
        packet_count: u64,
        action: &str,
        target_domain: Option<&str>,
    ) {
        let mut event = AttackEvent::new(
            attack_type.clone(),
            source_ip.to_string(),
            severity,
            packet_count,
            action,
        );

        event.target_domain = target_domain.map(|s| s.to_string());

        self.stats.record_attack(event.clone());

        // Broadcast event
        let _ = self.event_tx.send(SseEvent::new(
            "attack",
            serde_json::json!({
                "id": event.id,
                "type": event.attack_type,
                "source_ip": source_ip,
                "severity": severity,
                "action": action
            }),
        ));

        // Auto-block severe attacks
        if severity >= 8 && action == "blocked" {
            let _ = self
                .block_ip(
                    source_ip,
                    &format!("{:?} attack detected", attack_type),
                    self.config.default_block_duration_secs,
                    BlockSource::Auto,
                )
                .await;
        }
    }

    // =========================================================================
    // STATISTICS
    // =========================================================================

    /// Get global statistics
    pub fn get_global_stats(&self) -> GlobalStats {
        self.stats.get_global_stats()
    }

    /// Get recent attacks
    pub fn get_recent_attacks(&self, limit: usize) -> Vec<AttackEvent> {
        self.stats.get_recent_attacks(limit)
    }

    /// Get top attackers
    pub async fn get_top_attackers(&self, limit: usize) -> Vec<crate::ddos_stats::TopAttacker> {
        let blocklist = self.blocklist.read().await;
        let blocked_ips: Vec<String> = blocklist
            .all_entries()
            .iter()
            .map(|e| e.ip.clone())
            .collect();
        drop(blocklist);

        self.stats.get_top_attackers(limit, &blocked_ips)
    }

    /// Update stats from eBPF (Linux only)
    #[cfg(target_os = "linux")]
    pub fn update_from_ebpf(&self, ebpf_stats: &crate::ebpf_loader::DDoSStats) {
        self.stats.record_packets(ebpf_stats.total_packets);
        self.stats.record_dropped(ebpf_stats.dropped_packets);
        self.stats.record_syn_packets(ebpf_stats.syn_packets);
        self.stats.record_udp_packets(ebpf_stats.udp_packets);
        self.stats.record_ipv6_packets(ebpf_stats.ipv6_packets);
        self.stats.record_ipv6_dropped(ebpf_stats.ipv6_dropped);
    }
}

impl Default for DDoSManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_crud() {
        let manager = DDoSManager::new();

        // Create policy
        let policy = DDoSPolicy::new("example.com".to_string());
        manager.set_policy(policy.clone()).await.unwrap();

        // Get policy
        let retrieved = manager.get_policy("example.com").await.unwrap();
        assert_eq!(retrieved.domain, "example.com");

        // Update policy
        let update = DDoSPolicyUpdate {
            enabled: Some(false),
            ..Default::default()
        };
        let updated = manager.update_policy("example.com", update).await.unwrap();
        assert!(!updated.enabled);

        // Delete policy
        assert!(manager.delete_policy("example.com").await);
        assert!(manager.get_policy("example.com").await.is_none());
    }

    #[tokio::test]
    async fn test_blocklist_operations() {
        let manager = DDoSManager::new();

        // Block IP
        let entry = manager
            .block_ip("192.168.1.100", "Test block", 300, BlockSource::Manual)
            .await
            .unwrap();
        assert_eq!(entry.ip, "192.168.1.100");

        // Check blocked
        assert!(manager.is_blocked("192.168.1.100").await);

        // Unblock
        assert!(manager.unblock_ip("192.168.1.100").await);
        assert!(!manager.is_blocked("192.168.1.100").await);
    }

    #[tokio::test]
    async fn test_allowlist_operations() {
        let manager = DDoSManager::new();

        // Allow IP
        let entry = manager
            .allow_ip("10.0.0.1", "Internal server")
            .await
            .unwrap();
        assert_eq!(entry.ip, "10.0.0.1");

        // Check allowlisted
        assert!(manager.is_allowlisted("10.0.0.1").await);

        // Remove from allowlist
        assert!(manager.remove_from_allowlist("10.0.0.1").await);
        assert!(!manager.is_allowlisted("10.0.0.1").await);
    }

    #[tokio::test]
    async fn test_allowlist_bypasses_blocklist() {
        let manager = DDoSManager::new();

        // Block IP
        manager
            .block_ip("192.168.1.50", "Test", 300, BlockSource::Manual)
            .await
            .unwrap();

        // IP is blocked
        assert!(manager.is_blocked("192.168.1.50").await);

        // Add to allowlist
        manager.allow_ip("192.168.1.50", "Override").await.unwrap();

        // IP is no longer blocked (allowlist takes priority)
        assert!(!manager.is_blocked("192.168.1.50").await);
    }

    #[tokio::test]
    async fn test_cidr_allowlist() {
        let manager = DDoSManager::new();

        // Allow entire subnet
        manager
            .allow_ip("192.168.1.0/24", "Internal network")
            .await
            .unwrap();

        // IPs in subnet should be allowlisted
        assert!(manager.is_allowlisted("192.168.1.1").await);
        assert!(manager.is_allowlisted("192.168.1.255").await);

        // IPs outside subnet should not be allowlisted
        assert!(!manager.is_allowlisted("192.168.2.1").await);
    }

    #[tokio::test]
    async fn test_rate_limit_without_policy() {
        let manager = DDoSManager::new();

        // No policy = allow
        let result = manager.check_rate_limit("example.com", "192.168.1.1").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_attack_recording() {
        let manager = DDoSManager::new();

        manager
            .record_attack(
                AttackType::SynFlood,
                "10.0.0.1",
                7,
                1000,
                "blocked",
                Some("example.com"),
            )
            .await;

        let attacks = manager.get_recent_attacks(10);
        assert_eq!(attacks.len(), 1);
        assert_eq!(attacks[0].source_ip, "10.0.0.1");
    }

    #[tokio::test]
    async fn test_severe_attack_auto_blocks() {
        let manager = DDoSManager::new();

        // Severe attack (severity >= 8) should auto-block
        manager
            .record_attack(AttackType::SynFlood, "10.0.0.99", 9, 10000, "blocked", None)
            .await;

        assert!(manager.is_blocked("10.0.0.99").await);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let manager = DDoSManager::new();
        let mut receiver = manager.subscribe_events();

        // Trigger an event
        let policy = DDoSPolicy::new("test.com".to_string());
        manager.set_policy(policy).await.unwrap();

        // Receive event
        let event = receiver.try_recv().unwrap();
        assert_eq!(event.event_type, "policy_updated");
    }

    #[test]
    fn test_cidr_matching() {
        let ip = IpAddr::from_str("192.168.1.50").unwrap();
        assert!(matches_cidr(&ip, "192.168.1.0/24"));
        assert!(matches_cidr(&ip, "192.168.0.0/16"));
        assert!(!matches_cidr(&ip, "192.168.2.0/24"));
        assert!(matches_cidr(&ip, "192.168.1.50"));
    }

    #[test]
    fn test_cidr_matching_ipv6() {
        let ip = IpAddr::from_str("2001:db8::1").unwrap();
        assert!(matches_cidr(&ip, "2001:db8::/32"));
        assert!(!matches_cidr(&ip, "2001:db9::/32"));
    }

    #[tokio::test]
    async fn test_invalid_ip_blocked() {
        let manager = DDoSManager::new();

        let result = manager
            .block_ip("invalid-ip", "Test", 300, BlockSource::Manual)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_ip_allowlisted() {
        let manager = DDoSManager::new();

        let result = manager.allow_ip("not-an-ip", "Test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_policies() {
        let manager = DDoSManager::new();

        manager
            .set_policy(DDoSPolicy::new("a.com".to_string()))
            .await
            .unwrap();
        manager
            .set_policy(DDoSPolicy::new("b.com".to_string()))
            .await
            .unwrap();

        let policies = manager.list_policies().await;
        assert_eq!(policies.len(), 2);
    }
}
