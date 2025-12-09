/// DDoS Protection Statistics Aggregation
///
/// Collects and aggregates statistics about DDoS attacks, rate limiting,
/// and protection events for monitoring and reporting.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info};

// =============================================================================
// CONSTANTS
// =============================================================================

/// Maximum number of attack events to keep in history
const MAX_ATTACK_HISTORY: usize = 1000;

/// Maximum number of top attackers to track
const MAX_TOP_ATTACKERS: usize = 100;

/// Time window for recent statistics (5 minutes)
const RECENT_STATS_WINDOW_SECS: u64 = 300;

// =============================================================================
// ATTACK TYPES
// =============================================================================

/// Type of DDoS attack detected
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackType {
    /// SYN flood attack
    SynFlood,
    /// UDP flood attack
    UdpFlood,
    /// HTTP flood attack
    HttpFlood,
    /// Slowloris attack
    Slowloris,
    /// DNS amplification
    DnsAmplification,
    /// NTP amplification
    NtpAmplification,
    /// Generic volumetric attack
    Volumetric,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Challenge failed
    ChallengeFailed,
    /// Unknown attack type
    Unknown,
}

impl std::fmt::Display for AttackType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttackType::SynFlood => write!(f, "SYN Flood"),
            AttackType::UdpFlood => write!(f, "UDP Flood"),
            AttackType::HttpFlood => write!(f, "HTTP Flood"),
            AttackType::Slowloris => write!(f, "Slowloris"),
            AttackType::DnsAmplification => write!(f, "DNS Amplification"),
            AttackType::NtpAmplification => write!(f, "NTP Amplification"),
            AttackType::Volumetric => write!(f, "Volumetric"),
            AttackType::RateLimitExceeded => write!(f, "Rate Limit Exceeded"),
            AttackType::ChallengeFailed => write!(f, "Challenge Failed"),
            AttackType::Unknown => write!(f, "Unknown"),
        }
    }
}

// =============================================================================
// ATTACK EVENT
// =============================================================================

/// A single attack event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackEvent {
    /// Unique event ID
    pub id: String,

    /// Type of attack
    pub attack_type: AttackType,

    /// Source IP address
    pub source_ip: String,

    /// Target domain (if known)
    pub target_domain: Option<String>,

    /// Attack severity (1-10)
    pub severity: u8,

    /// Number of packets/requests in this attack
    pub packet_count: u64,

    /// Timestamp when attack was detected (Unix seconds)
    pub detected_at: u64,

    /// Whether the attack was mitigated
    pub mitigated: bool,

    /// Action taken (blocked, challenged, rate_limited)
    pub action: String,
}

impl AttackEvent {
    /// Create a new attack event
    pub fn new(
        attack_type: AttackType,
        source_ip: String,
        severity: u8,
        packet_count: u64,
        action: &str,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: generate_event_id(),
            attack_type,
            source_ip,
            target_domain: None,
            severity: severity.min(10),
            packet_count,
            detected_at: now,
            mitigated: true,
            action: action.to_string(),
        }
    }
}

/// Generate a unique event ID
fn generate_event_id() -> String {
    use std::time::SystemTime;
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("atk_{:x}", timestamp)
}

// =============================================================================
// GLOBAL STATISTICS
// =============================================================================

/// Global DDoS protection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalStats {
    /// Total packets processed
    pub total_packets: u64,

    /// Total packets dropped
    pub dropped_packets: u64,

    /// Total SYN packets
    pub syn_packets: u64,

    /// Total UDP packets
    pub udp_packets: u64,

    /// Total IPv6 packets
    pub ipv6_packets: u64,

    /// Total IPv6 packets dropped
    pub ipv6_dropped: u64,

    /// Total requests rate limited
    pub rate_limited_requests: u64,

    /// Total challenges issued
    pub challenges_issued: u64,

    /// Total challenges passed
    pub challenges_passed: u64,

    /// Total challenges failed
    pub challenges_failed: u64,

    /// Total IPs blocked
    pub ips_blocked: u64,

    /// Currently blocked IPs count
    pub active_blocks: u64,

    /// Total threat intel events received
    pub threat_intel_received: u64,

    /// Total threat intel events published
    pub threat_intel_published: u64,

    /// Timestamp of stats collection
    pub timestamp: u64,

    /// Uptime in seconds
    pub uptime_secs: u64,
}

impl Default for GlobalStats {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            total_packets: 0,
            dropped_packets: 0,
            syn_packets: 0,
            udp_packets: 0,
            ipv6_packets: 0,
            ipv6_dropped: 0,
            rate_limited_requests: 0,
            challenges_issued: 0,
            challenges_passed: 0,
            challenges_failed: 0,
            ips_blocked: 0,
            active_blocks: 0,
            threat_intel_received: 0,
            threat_intel_published: 0,
            timestamp: now,
            uptime_secs: 0,
        }
    }
}

// =============================================================================
// DOMAIN STATISTICS
// =============================================================================

/// Per-domain DDoS protection statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainStats {
    /// Domain name
    pub domain: String,

    /// Total requests
    pub total_requests: u64,

    /// Blocked requests
    pub blocked_requests: u64,

    /// Rate limited requests
    pub rate_limited_requests: u64,

    /// Challenges issued
    pub challenges_issued: u64,

    /// Challenges passed
    pub challenges_passed: u64,

    /// Attack count in last hour
    pub attacks_last_hour: u64,

    /// Cache hit ratio (if applicable)
    pub cache_hit_ratio: f64,

    /// Average response time (ms)
    pub avg_response_time_ms: f64,

    /// Timestamp of stats collection
    pub timestamp: u64,
}

impl DomainStats {
    pub fn new(domain: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            domain,
            total_requests: 0,
            blocked_requests: 0,
            rate_limited_requests: 0,
            challenges_issued: 0,
            challenges_passed: 0,
            attacks_last_hour: 0,
            cache_hit_ratio: 0.0,
            avg_response_time_ms: 0.0,
            timestamp: now,
        }
    }
}

// =============================================================================
// TOP ATTACKER
// =============================================================================

/// Information about a top attacker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopAttacker {
    /// IP address
    pub ip: String,

    /// Number of attacks from this IP
    pub attack_count: u64,

    /// Total packets/requests from this IP
    pub total_packets: u64,

    /// Primary attack type from this IP
    pub primary_attack_type: AttackType,

    /// Last attack timestamp
    pub last_attack_at: u64,

    /// Whether currently blocked
    pub is_blocked: bool,
}

// =============================================================================
// DDOS STATS COLLECTOR
// =============================================================================

/// Atomic counters for high-performance statistics collection
struct AtomicCounters {
    total_packets: AtomicU64,
    dropped_packets: AtomicU64,
    syn_packets: AtomicU64,
    udp_packets: AtomicU64,
    ipv6_packets: AtomicU64,
    ipv6_dropped: AtomicU64,
    rate_limited: AtomicU64,
    challenges_issued: AtomicU64,
    challenges_passed: AtomicU64,
    challenges_failed: AtomicU64,
    ips_blocked: AtomicU64,
    threat_intel_received: AtomicU64,
    threat_intel_published: AtomicU64,
}

impl AtomicCounters {
    fn new() -> Self {
        Self {
            total_packets: AtomicU64::new(0),
            dropped_packets: AtomicU64::new(0),
            syn_packets: AtomicU64::new(0),
            udp_packets: AtomicU64::new(0),
            ipv6_packets: AtomicU64::new(0),
            ipv6_dropped: AtomicU64::new(0),
            rate_limited: AtomicU64::new(0),
            challenges_issued: AtomicU64::new(0),
            challenges_passed: AtomicU64::new(0),
            challenges_failed: AtomicU64::new(0),
            ips_blocked: AtomicU64::new(0),
            threat_intel_received: AtomicU64::new(0),
            threat_intel_published: AtomicU64::new(0),
        }
    }
}

/// DDoS statistics collector and aggregator
pub struct DDoSStats {
    /// Atomic counters for global stats
    counters: AtomicCounters,

    /// Attack event history
    attack_history: RwLock<Vec<AttackEvent>>,

    /// Per-domain statistics
    domain_stats: RwLock<HashMap<String, DomainStatsCollector>>,

    /// Top attackers tracking
    top_attackers: RwLock<HashMap<String, AttackerInfo>>,

    /// Start time for uptime calculation
    start_time: Instant,

    /// Active blocks count
    active_blocks: AtomicU64,
}

/// Internal domain stats collector with atomic counters
struct DomainStatsCollector {
    total_requests: AtomicU64,
    blocked_requests: AtomicU64,
    rate_limited: AtomicU64,
    challenges_issued: AtomicU64,
    challenges_passed: AtomicU64,
    response_time_sum_ms: AtomicU64,
    response_count: AtomicU64,
}

impl DomainStatsCollector {
    fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            blocked_requests: AtomicU64::new(0),
            rate_limited: AtomicU64::new(0),
            challenges_issued: AtomicU64::new(0),
            challenges_passed: AtomicU64::new(0),
            response_time_sum_ms: AtomicU64::new(0),
            response_count: AtomicU64::new(0),
        }
    }

    fn to_stats(&self, domain: &str, attacks_last_hour: u64) -> DomainStats {
        let response_count = self.response_count.load(Ordering::Relaxed);
        let avg_response_time = if response_count > 0 {
            self.response_time_sum_ms.load(Ordering::Relaxed) as f64 / response_count as f64
        } else {
            0.0
        };

        DomainStats {
            domain: domain.to_string(),
            total_requests: self.total_requests.load(Ordering::Relaxed),
            blocked_requests: self.blocked_requests.load(Ordering::Relaxed),
            rate_limited_requests: self.rate_limited.load(Ordering::Relaxed),
            challenges_issued: self.challenges_issued.load(Ordering::Relaxed),
            challenges_passed: self.challenges_passed.load(Ordering::Relaxed),
            attacks_last_hour,
            cache_hit_ratio: 0.0, // Filled in from cache stats
            avg_response_time_ms: avg_response_time,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// Internal attacker tracking info
struct AttackerInfo {
    attack_count: AtomicU64,
    total_packets: AtomicU64,
    primary_attack_type: RwLock<AttackType>,
    last_attack_at: AtomicU64,
}

impl AttackerInfo {
    fn new(attack_type: AttackType) -> Self {
        Self {
            attack_count: AtomicU64::new(1),
            total_packets: AtomicU64::new(0),
            primary_attack_type: RwLock::new(attack_type),
            last_attack_at: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
        }
    }
}

impl DDoSStats {
    /// Create a new DDoS stats collector
    pub fn new() -> Self {
        Self {
            counters: AtomicCounters::new(),
            attack_history: RwLock::new(Vec::new()),
            domain_stats: RwLock::new(HashMap::new()),
            top_attackers: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            active_blocks: AtomicU64::new(0),
        }
    }

    // =========================================================================
    // PACKET COUNTERS
    // =========================================================================

    /// Record packets processed
    pub fn record_packets(&self, count: u64) {
        self.counters.total_packets.fetch_add(count, Ordering::Relaxed);
    }

    /// Record packets dropped
    pub fn record_dropped(&self, count: u64) {
        self.counters.dropped_packets.fetch_add(count, Ordering::Relaxed);
    }

    /// Record SYN packets
    pub fn record_syn_packets(&self, count: u64) {
        self.counters.syn_packets.fetch_add(count, Ordering::Relaxed);
    }

    /// Record UDP packets
    pub fn record_udp_packets(&self, count: u64) {
        self.counters.udp_packets.fetch_add(count, Ordering::Relaxed);
    }

    /// Record IPv6 packets
    pub fn record_ipv6_packets(&self, count: u64) {
        self.counters.ipv6_packets.fetch_add(count, Ordering::Relaxed);
    }

    /// Record IPv6 packets dropped
    pub fn record_ipv6_dropped(&self, count: u64) {
        self.counters.ipv6_dropped.fetch_add(count, Ordering::Relaxed);
    }

    // =========================================================================
    // RATE LIMITING
    // =========================================================================

    /// Record rate limited request
    pub fn record_rate_limited(&self) {
        self.counters.rate_limited.fetch_add(1, Ordering::Relaxed);
    }

    /// Record rate limited request for a domain
    pub fn record_rate_limited_for_domain(&self, domain: &str) {
        self.counters.rate_limited.fetch_add(1, Ordering::Relaxed);

        let mut stats = self.domain_stats.write().unwrap();
        let collector = stats.entry(domain.to_string()).or_insert_with(DomainStatsCollector::new);
        collector.rate_limited.fetch_add(1, Ordering::Relaxed);
    }

    // =========================================================================
    // CHALLENGES
    // =========================================================================

    /// Record challenge issued
    pub fn record_challenge_issued(&self) {
        self.counters.challenges_issued.fetch_add(1, Ordering::Relaxed);
    }

    /// Record challenge passed
    pub fn record_challenge_passed(&self) {
        self.counters.challenges_passed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record challenge failed
    pub fn record_challenge_failed(&self) {
        self.counters.challenges_failed.fetch_add(1, Ordering::Relaxed);
    }

    // =========================================================================
    // BLOCKING
    // =========================================================================

    /// Record IP blocked
    pub fn record_ip_blocked(&self) {
        self.counters.ips_blocked.fetch_add(1, Ordering::Relaxed);
        self.active_blocks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record IP unblocked
    pub fn record_ip_unblocked(&self) {
        self.active_blocks.fetch_sub(1, Ordering::Relaxed);
    }

    /// Set active blocks count (from eBPF stats)
    pub fn set_active_blocks(&self, count: u64) {
        self.active_blocks.store(count, Ordering::Relaxed);
    }

    // =========================================================================
    // THREAT INTELLIGENCE
    // =========================================================================

    /// Record threat intel received
    pub fn record_threat_intel_received(&self) {
        self.counters.threat_intel_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record threat intel published
    pub fn record_threat_intel_published(&self) {
        self.counters.threat_intel_published.fetch_add(1, Ordering::Relaxed);
    }

    // =========================================================================
    // ATTACK EVENTS
    // =========================================================================

    /// Record an attack event
    pub fn record_attack(&self, event: AttackEvent) {
        debug!("Recording attack event: {:?}", event);

        // Update top attackers
        {
            let mut attackers = self.top_attackers.write().unwrap();
            if let Some(info) = attackers.get(&event.source_ip) {
                info.attack_count.fetch_add(1, Ordering::Relaxed);
                info.total_packets.fetch_add(event.packet_count, Ordering::Relaxed);
                info.last_attack_at.store(event.detected_at, Ordering::Relaxed);
            } else if attackers.len() < MAX_TOP_ATTACKERS {
                attackers.insert(
                    event.source_ip.clone(),
                    AttackerInfo::new(event.attack_type.clone()),
                );
            }
        }

        // Add to history
        {
            let mut history = self.attack_history.write().unwrap();
            if history.len() >= MAX_ATTACK_HISTORY {
                history.remove(0); // Remove oldest
            }
            history.push(event);
        }
    }

    /// Get recent attack events
    pub fn get_recent_attacks(&self, limit: usize) -> Vec<AttackEvent> {
        let history = self.attack_history.read().unwrap();
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get attacks in time range
    pub fn get_attacks_in_range(&self, start: u64, end: u64) -> Vec<AttackEvent> {
        let history = self.attack_history.read().unwrap();
        history
            .iter()
            .filter(|e| e.detected_at >= start && e.detected_at <= end)
            .cloned()
            .collect()
    }

    // =========================================================================
    // DOMAIN STATS
    // =========================================================================

    /// Record request for a domain
    pub fn record_request(&self, domain: &str) {
        let mut stats = self.domain_stats.write().unwrap();
        let collector = stats.entry(domain.to_string()).or_insert_with(DomainStatsCollector::new);
        collector.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record blocked request for a domain
    pub fn record_blocked(&self, domain: &str) {
        let mut stats = self.domain_stats.write().unwrap();
        let collector = stats.entry(domain.to_string()).or_insert_with(DomainStatsCollector::new);
        collector.blocked_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record response time for a domain
    pub fn record_response_time(&self, domain: &str, time_ms: u64) {
        let mut stats = self.domain_stats.write().unwrap();
        let collector = stats.entry(domain.to_string()).or_insert_with(DomainStatsCollector::new);
        collector.response_time_sum_ms.fetch_add(time_ms, Ordering::Relaxed);
        collector.response_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get stats for a specific domain
    pub fn get_domain_stats(&self, domain: &str) -> Option<DomainStats> {
        let stats = self.domain_stats.read().unwrap();
        stats.get(domain).map(|c| {
            let attacks_last_hour = self.count_attacks_last_hour(Some(domain));
            c.to_stats(domain, attacks_last_hour)
        })
    }

    /// Get stats for all domains
    pub fn get_all_domain_stats(&self) -> Vec<DomainStats> {
        let stats = self.domain_stats.read().unwrap();
        stats
            .iter()
            .map(|(domain, collector)| {
                let attacks_last_hour = self.count_attacks_last_hour(Some(domain));
                collector.to_stats(domain, attacks_last_hour)
            })
            .collect()
    }

    // =========================================================================
    // AGGREGATION
    // =========================================================================

    /// Get global statistics
    pub fn get_global_stats(&self) -> GlobalStats {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        GlobalStats {
            total_packets: self.counters.total_packets.load(Ordering::Relaxed),
            dropped_packets: self.counters.dropped_packets.load(Ordering::Relaxed),
            syn_packets: self.counters.syn_packets.load(Ordering::Relaxed),
            udp_packets: self.counters.udp_packets.load(Ordering::Relaxed),
            ipv6_packets: self.counters.ipv6_packets.load(Ordering::Relaxed),
            ipv6_dropped: self.counters.ipv6_dropped.load(Ordering::Relaxed),
            rate_limited_requests: self.counters.rate_limited.load(Ordering::Relaxed),
            challenges_issued: self.counters.challenges_issued.load(Ordering::Relaxed),
            challenges_passed: self.counters.challenges_passed.load(Ordering::Relaxed),
            challenges_failed: self.counters.challenges_failed.load(Ordering::Relaxed),
            ips_blocked: self.counters.ips_blocked.load(Ordering::Relaxed),
            active_blocks: self.active_blocks.load(Ordering::Relaxed),
            threat_intel_received: self.counters.threat_intel_received.load(Ordering::Relaxed),
            threat_intel_published: self.counters.threat_intel_published.load(Ordering::Relaxed),
            timestamp: now,
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }

    /// Get top attackers
    pub fn get_top_attackers(&self, limit: usize, blocked_ips: &[String]) -> Vec<TopAttacker> {
        let attackers = self.top_attackers.read().unwrap();
        let blocked_set: std::collections::HashSet<_> = blocked_ips.iter().collect();

        let mut result: Vec<_> = attackers
            .iter()
            .map(|(ip, info)| {
                let attack_type = info.primary_attack_type.read().unwrap().clone();
                TopAttacker {
                    ip: ip.clone(),
                    attack_count: info.attack_count.load(Ordering::Relaxed),
                    total_packets: info.total_packets.load(Ordering::Relaxed),
                    primary_attack_type: attack_type,
                    last_attack_at: info.last_attack_at.load(Ordering::Relaxed),
                    is_blocked: blocked_set.contains(ip),
                }
            })
            .collect();

        // Sort by attack count descending
        result.sort_by(|a, b| b.attack_count.cmp(&a.attack_count));
        result.truncate(limit);
        result
    }

    /// Count attacks in the last hour
    fn count_attacks_last_hour(&self, domain: Option<&str>) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let one_hour_ago = now.saturating_sub(3600);

        let history = self.attack_history.read().unwrap();
        history
            .iter()
            .filter(|e| {
                e.detected_at >= one_hour_ago
                    && (domain.is_none() || e.target_domain.as_deref() == domain)
            })
            .count() as u64
    }

    /// Get drop rate (percentage)
    pub fn get_drop_rate(&self) -> f64 {
        let total = self.counters.total_packets.load(Ordering::Relaxed);
        let dropped = self.counters.dropped_packets.load(Ordering::Relaxed);

        if total == 0 {
            0.0
        } else {
            (dropped as f64 / total as f64) * 100.0
        }
    }

    /// Reset all statistics
    pub fn reset(&self) {
        self.counters.total_packets.store(0, Ordering::Relaxed);
        self.counters.dropped_packets.store(0, Ordering::Relaxed);
        self.counters.syn_packets.store(0, Ordering::Relaxed);
        self.counters.udp_packets.store(0, Ordering::Relaxed);
        self.counters.ipv6_packets.store(0, Ordering::Relaxed);
        self.counters.ipv6_dropped.store(0, Ordering::Relaxed);
        self.counters.rate_limited.store(0, Ordering::Relaxed);
        self.counters.challenges_issued.store(0, Ordering::Relaxed);
        self.counters.challenges_passed.store(0, Ordering::Relaxed);
        self.counters.challenges_failed.store(0, Ordering::Relaxed);
        self.counters.ips_blocked.store(0, Ordering::Relaxed);
        self.counters.threat_intel_received.store(0, Ordering::Relaxed);
        self.counters.threat_intel_published.store(0, Ordering::Relaxed);

        self.attack_history.write().unwrap().clear();
        self.domain_stats.write().unwrap().clear();
        self.top_attackers.write().unwrap().clear();

        info!("DDoS statistics reset");
    }
}

impl Default for DDoSStats {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// SSE EVENT
// =============================================================================

/// Event for Server-Sent Events streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    /// Event type (attack, block, stats, etc.)
    pub event_type: String,

    /// Event data (JSON)
    pub data: serde_json::Value,

    /// Event timestamp
    pub timestamp: u64,
}

impl SseEvent {
    /// Create a new SSE event
    pub fn new(event_type: &str, data: serde_json::Value) -> Self {
        Self {
            event_type: event_type.to_string(),
            data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Format as SSE message
    pub fn to_sse_string(&self) -> String {
        format!(
            "event: {}\ndata: {}\n\n",
            self.event_type,
            serde_json::to_string(&self.data).unwrap_or_default()
        )
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attack_event_creation() {
        let event = AttackEvent::new(
            AttackType::SynFlood,
            "192.168.1.100".to_string(),
            8,
            1000,
            "blocked",
        );

        assert!(event.id.starts_with("atk_"));
        assert_eq!(event.attack_type, AttackType::SynFlood);
        assert_eq!(event.source_ip, "192.168.1.100");
        assert_eq!(event.severity, 8);
        assert_eq!(event.packet_count, 1000);
        assert!(event.mitigated);
    }

    #[test]
    fn test_severity_clamping() {
        let event = AttackEvent::new(
            AttackType::UdpFlood,
            "10.0.0.1".to_string(),
            15, // Above max
            100,
            "rate_limited",
        );
        assert_eq!(event.severity, 10); // Clamped to max
    }

    #[test]
    fn test_stats_recording() {
        let stats = DDoSStats::new();

        stats.record_packets(100);
        stats.record_dropped(10);
        stats.record_syn_packets(50);

        let global = stats.get_global_stats();
        assert_eq!(global.total_packets, 100);
        assert_eq!(global.dropped_packets, 10);
        assert_eq!(global.syn_packets, 50);
    }

    #[test]
    fn test_drop_rate() {
        let stats = DDoSStats::new();

        stats.record_packets(100);
        stats.record_dropped(25);

        let rate = stats.get_drop_rate();
        assert!((rate - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_domain_stats() {
        let stats = DDoSStats::new();

        stats.record_request("example.com");
        stats.record_request("example.com");
        stats.record_blocked("example.com");
        stats.record_response_time("example.com", 100);
        stats.record_response_time("example.com", 200);

        let domain_stats = stats.get_domain_stats("example.com").unwrap();
        assert_eq!(domain_stats.total_requests, 2);
        assert_eq!(domain_stats.blocked_requests, 1);
        assert!((domain_stats.avg_response_time_ms - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_attack_history() {
        let stats = DDoSStats::new();

        for i in 0..5 {
            let event = AttackEvent::new(
                AttackType::SynFlood,
                format!("192.168.1.{}", i),
                5,
                100,
                "blocked",
            );
            stats.record_attack(event);
        }

        let recent = stats.get_recent_attacks(3);
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert_eq!(recent[0].source_ip, "192.168.1.4");
    }

    #[test]
    fn test_top_attackers() {
        let stats = DDoSStats::new();

        // Record multiple attacks from same IP
        for _ in 0..5 {
            let event = AttackEvent::new(
                AttackType::SynFlood,
                "192.168.1.100".to_string(),
                8,
                1000,
                "blocked",
            );
            stats.record_attack(event);
        }

        let top = stats.get_top_attackers(10, &[]);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].ip, "192.168.1.100");
        assert_eq!(top[0].attack_count, 5);
    }

    #[test]
    fn test_sse_event() {
        let event = SseEvent::new(
            "attack",
            serde_json::json!({"ip": "192.168.1.1", "type": "syn_flood"}),
        );

        let sse_str = event.to_sse_string();
        assert!(sse_str.starts_with("event: attack\n"));
        assert!(sse_str.contains("data: "));
        assert!(sse_str.ends_with("\n\n"));
    }

    #[test]
    fn test_reset() {
        let stats = DDoSStats::new();

        stats.record_packets(100);
        stats.record_dropped(10);
        stats.record_ip_blocked();

        stats.reset();

        let global = stats.get_global_stats();
        assert_eq!(global.total_packets, 0);
        assert_eq!(global.dropped_packets, 0);
    }

    #[test]
    fn test_rate_limited_for_domain() {
        let stats = DDoSStats::new();

        stats.record_rate_limited_for_domain("api.example.com");
        stats.record_rate_limited_for_domain("api.example.com");

        let domain_stats = stats.get_domain_stats("api.example.com").unwrap();
        assert_eq!(domain_stats.rate_limited_requests, 2);
    }

    #[test]
    fn test_serialization() {
        let stats = GlobalStats::default();
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: GlobalStats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats.total_packets, parsed.total_packets);
    }

    #[test]
    fn test_attack_type_display() {
        assert_eq!(AttackType::SynFlood.to_string(), "SYN Flood");
        assert_eq!(AttackType::UdpFlood.to_string(), "UDP Flood");
        assert_eq!(AttackType::HttpFlood.to_string(), "HTTP Flood");
    }
}
