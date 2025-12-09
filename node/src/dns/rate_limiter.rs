//! DNS Rate Limiter
//!
//! Token bucket rate limiter and TCP connection tracker for DNS DoS protection.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::RateLimitConfig;

/// Token bucket rate limiter for DNS queries
pub struct DnsRateLimiter {
    /// Per-IP token buckets
    buckets: Arc<RwLock<HashMap<IpAddr, TokenBucket>>>,
    /// Queries per second rate
    rate: f64,
    /// Maximum burst size
    burst: u32,
    /// Whether rate limiting is enabled
    enabled: bool,
    /// Cleanup interval for old buckets
    cleanup_interval: Duration,
    /// Last cleanup time
    last_cleanup: Arc<RwLock<Instant>>,
}

impl DnsRateLimiter {
    /// Create a new rate limiter
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            rate: config.queries_per_second as f64,
            burst: config.burst_size,
            enabled: config.enabled,
            cleanup_interval: Duration::from_secs(60),
            last_cleanup: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Check if a query from the given IP should be allowed
    pub async fn check(&self, ip: IpAddr) -> bool {
        if !self.enabled {
            return true;
        }

        let mut buckets = self.buckets.write().await;

        // Clean up old buckets periodically
        self.maybe_cleanup(&mut buckets).await;

        // Get or create bucket for this IP
        let bucket = buckets.entry(ip).or_insert_with(|| {
            TokenBucket::new(self.burst, self.rate)
        });

        bucket.try_consume()
    }

    /// Check if rate limited and return remaining tokens
    pub async fn check_with_remaining(&self, ip: IpAddr) -> (bool, u32) {
        if !self.enabled {
            return (true, self.burst);
        }

        let mut buckets = self.buckets.write().await;

        let bucket = buckets.entry(ip).or_insert_with(|| {
            TokenBucket::new(self.burst, self.rate)
        });

        let allowed = bucket.try_consume();
        let remaining = bucket.tokens as u32;

        (allowed, remaining)
    }

    /// Get current stats for an IP
    pub async fn get_stats(&self, ip: IpAddr) -> Option<RateLimitStats> {
        let buckets = self.buckets.read().await;

        buckets.get(&ip).map(|b| RateLimitStats {
            tokens_remaining: b.tokens as u32,
            burst_capacity: self.burst,
            refill_rate: self.rate as u32,
            last_check: b.last_check,
        })
    }

    /// Get total number of tracked IPs
    pub async fn tracked_ips(&self) -> usize {
        let buckets = self.buckets.read().await;
        buckets.len()
    }

    /// Clean up old buckets
    async fn maybe_cleanup(&self, buckets: &mut HashMap<IpAddr, TokenBucket>) {
        let mut last_cleanup = self.last_cleanup.write().await;

        if last_cleanup.elapsed() < self.cleanup_interval {
            return;
        }

        // Remove buckets that are at full capacity (not actively rate limited)
        let now = Instant::now();
        buckets.retain(|_, bucket| {
            // Keep buckets that were checked recently
            now.duration_since(bucket.last_check) < Duration::from_secs(300)
        });

        *last_cleanup = now;
    }

    /// Reset rate limit for a specific IP (admin function)
    pub async fn reset(&self, ip: IpAddr) {
        let mut buckets = self.buckets.write().await;
        buckets.remove(&ip);
    }

    /// Clear all rate limit state
    pub async fn clear_all(&self) {
        let mut buckets = self.buckets.write().await;
        buckets.clear();
    }
}

impl Default for DnsRateLimiter {
    fn default() -> Self {
        Self::new(&RateLimitConfig::default())
    }
}

/// Token bucket implementation
#[derive(Debug)]
struct TokenBucket {
    /// Current token count
    tokens: f64,
    /// Maximum tokens (burst capacity)
    capacity: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last time tokens were checked/refilled
    last_check: Instant,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            tokens: capacity as f64,
            capacity: capacity as f64,
            refill_rate,
            last_check: Instant::now(),
        }
    }

    /// Try to consume a token, returns true if allowed
    fn try_consume(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_check).as_secs_f64();
        self.last_check = now;

        // Add tokens based on time elapsed
        self.tokens += elapsed * self.refill_rate;

        // Cap at capacity
        if self.tokens > self.capacity {
            self.tokens = self.capacity;
        }
    }
}

/// Rate limit statistics for an IP
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    /// Remaining tokens
    pub tokens_remaining: u32,
    /// Maximum burst capacity
    pub burst_capacity: u32,
    /// Tokens refilled per second
    pub refill_rate: u32,
    /// Last check time
    pub last_check: Instant,
}

/// TCP connection tracker for DoS protection
pub struct TcpConnectionTracker {
    /// Per-IP connection counts
    connections: Arc<RwLock<HashMap<IpAddr, usize>>>,
    /// Total connection count
    total: Arc<RwLock<usize>>,
    /// Maximum connections per IP
    max_per_ip: usize,
    /// Maximum total connections
    max_total: usize,
}

impl TcpConnectionTracker {
    /// Create a new connection tracker
    pub fn new(max_per_ip: usize, max_total: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            total: Arc::new(RwLock::new(0)),
            max_per_ip,
            max_total,
        }
    }

    /// Try to accept a new connection from an IP
    pub async fn try_accept(&self, ip: IpAddr) -> bool {
        let mut connections = self.connections.write().await;
        let mut total = self.total.write().await;

        // Check total limit
        if *total >= self.max_total {
            return false;
        }

        // Check per-IP limit
        let count = connections.entry(ip).or_insert(0);
        if *count >= self.max_per_ip {
            return false;
        }

        *count += 1;
        *total += 1;
        true
    }

    /// Release a connection slot
    pub async fn release(&self, ip: IpAddr) {
        let mut connections = self.connections.write().await;
        let mut total = self.total.write().await;

        if let Some(count) = connections.get_mut(&ip) {
            if *count > 0 {
                *count -= 1;
                if *total > 0 {
                    *total -= 1;
                }
            }
            if *count == 0 {
                connections.remove(&ip);
            }
        }
    }

    /// Get connection count for an IP
    pub async fn get_count(&self, ip: IpAddr) -> usize {
        let connections = self.connections.read().await;
        *connections.get(&ip).unwrap_or(&0)
    }

    /// Get total connection count
    pub async fn total_connections(&self) -> usize {
        let total = self.total.read().await;
        *total
    }

    /// Get number of tracked IPs
    pub async fn tracked_ips(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// Get statistics
    pub async fn stats(&self) -> TcpTrackerStats {
        let connections = self.connections.read().await;
        let total = self.total.read().await;

        TcpTrackerStats {
            total_connections: *total,
            unique_ips: connections.len(),
            max_total: self.max_total,
            max_per_ip: self.max_per_ip,
        }
    }
}

impl Default for TcpConnectionTracker {
    fn default() -> Self {
        Self::new(10, 10000)
    }
}

/// TCP connection tracker statistics
#[derive(Debug, Clone)]
pub struct TcpTrackerStats {
    /// Current total connections
    pub total_connections: usize,
    /// Number of unique IPs
    pub unique_ips: usize,
    /// Maximum total connections allowed
    pub max_total: usize,
    /// Maximum connections per IP
    pub max_per_ip: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows() {
        let config = RateLimitConfig {
            queries_per_second: 10,
            burst_size: 10,
            window_secs: 1,
            enabled: true,
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // First 10 requests should pass (burst)
        for _ in 0..10 {
            assert!(limiter.check(ip).await);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks() {
        let config = RateLimitConfig {
            queries_per_second: 10,
            burst_size: 5,
            window_secs: 1,
            enabled: true,
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Exhaust burst
        for _ in 0..5 {
            limiter.check(ip).await;
        }

        // Next request should be blocked
        assert!(!limiter.check(ip).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            queries_per_second: 1,
            burst_size: 1,
            window_secs: 1,
            enabled: false, // Disabled
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // All requests should pass when disabled
        for _ in 0..100 {
            assert!(limiter.check(ip).await);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let config = RateLimitConfig {
            queries_per_second: 1000, // High rate for quick refill
            burst_size: 1,
            window_secs: 1,
            enabled: true,
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Exhaust burst
        assert!(limiter.check(ip).await);
        assert!(!limiter.check(ip).await);

        // Wait a tiny bit for refill (at 1000/sec, 1ms = 1 token)
        tokio::time::sleep(Duration::from_millis(2)).await;

        // Should have refilled
        assert!(limiter.check(ip).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_separate_ips() {
        let config = RateLimitConfig {
            queries_per_second: 10,
            burst_size: 2,
            window_secs: 1,
            enabled: true,
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        // Exhaust IP1's burst
        assert!(limiter.check(ip1).await);
        assert!(limiter.check(ip1).await);
        assert!(!limiter.check(ip1).await);

        // IP2 should still have tokens
        assert!(limiter.check(ip2).await);
    }

    #[tokio::test]
    async fn test_rate_limiter_stats() {
        let config = RateLimitConfig {
            queries_per_second: 10,
            burst_size: 5,
            window_secs: 1,
            enabled: true,
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Use some tokens
        limiter.check(ip).await;
        limiter.check(ip).await;

        let stats = limiter.get_stats(ip).await.unwrap();
        assert!(stats.tokens_remaining <= 3); // Started with 5, used 2
        assert_eq!(stats.burst_capacity, 5);
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let config = RateLimitConfig {
            queries_per_second: 10,
            burst_size: 2,
            window_secs: 1,
            enabled: true,
        };
        let limiter = DnsRateLimiter::new(&config);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Exhaust burst
        limiter.check(ip).await;
        limiter.check(ip).await;
        assert!(!limiter.check(ip).await);

        // Reset
        limiter.reset(ip).await;

        // Should work again
        assert!(limiter.check(ip).await);
    }

    #[tokio::test]
    async fn test_tcp_tracker_allows() {
        let tracker = TcpConnectionTracker::new(5, 100);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // First 5 connections should be allowed
        for _ in 0..5 {
            assert!(tracker.try_accept(ip).await);
        }

        assert_eq!(tracker.get_count(ip).await, 5);
    }

    #[tokio::test]
    async fn test_tcp_tracker_blocks_per_ip() {
        let tracker = TcpConnectionTracker::new(2, 100);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        assert!(tracker.try_accept(ip).await);
        assert!(tracker.try_accept(ip).await);
        assert!(!tracker.try_accept(ip).await); // Blocked
    }

    #[tokio::test]
    async fn test_tcp_tracker_blocks_total() {
        let tracker = TcpConnectionTracker::new(100, 3);

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        assert!(tracker.try_accept(ip1).await);
        assert!(tracker.try_accept(ip1).await);
        assert!(tracker.try_accept(ip2).await);
        assert!(!tracker.try_accept(ip2).await); // Blocked by total limit
    }

    #[tokio::test]
    async fn test_tcp_tracker_release() {
        let tracker = TcpConnectionTracker::new(2, 100);

        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        assert!(tracker.try_accept(ip).await);
        assert!(tracker.try_accept(ip).await);
        assert!(!tracker.try_accept(ip).await);

        // Release one
        tracker.release(ip).await;

        // Now should allow
        assert!(tracker.try_accept(ip).await);
    }

    #[tokio::test]
    async fn test_tcp_tracker_stats() {
        let tracker = TcpConnectionTracker::new(5, 100);

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        tracker.try_accept(ip1).await;
        tracker.try_accept(ip1).await;
        tracker.try_accept(ip2).await;

        let stats = tracker.stats().await;
        assert_eq!(stats.total_connections, 3);
        assert_eq!(stats.unique_ips, 2);
        assert_eq!(stats.max_total, 100);
        assert_eq!(stats.max_per_ip, 5);
    }

    #[tokio::test]
    async fn test_tracked_ips() {
        let limiter = DnsRateLimiter::default();

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        limiter.check(ip1).await;
        limiter.check(ip2).await;

        assert_eq!(limiter.tracked_ips().await, 2);
    }

    #[tokio::test]
    async fn test_clear_all() {
        let limiter = DnsRateLimiter::default();

        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        limiter.check(ip).await;

        assert_eq!(limiter.tracked_ips().await, 1);

        limiter.clear_all().await;

        assert_eq!(limiter.tracked_ips().await, 0);
    }
}
