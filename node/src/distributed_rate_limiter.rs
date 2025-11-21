use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::distributed_counter::{ActorId, CounterOp, DistributedCounter};
use crate::nats_sync::{NatsConfig, NatsSync};

/// Configuration for distributed rate limiter
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Actor ID for this node
    pub actor_id: ActorId,
    /// NATS configuration
    pub nats_config: NatsConfig,
    /// Rate limit window duration (seconds)
    pub window_duration_secs: u64,
    /// Maximum requests per window
    pub max_requests: u64,
    /// Whether to auto-sync counters via NATS
    pub auto_sync: bool,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            actor_id: 1,
            nats_config: NatsConfig::default(),
            window_duration_secs: 60,
            max_requests: 100,
            auto_sync: true,
        }
    }
}

/// Rate limit decision
#[derive(Debug, Clone, PartialEq)]
pub enum RateLimitDecision {
    /// Request is allowed
    Allowed {
        /// Current request count in window
        current_count: u64,
        /// Remaining requests in window
        remaining: u64,
    },
    /// Request exceeds rate limit
    Denied {
        /// Current request count in window
        current_count: u64,
        /// Time until window resets (seconds)
        retry_after_secs: u64,
    },
}

/// Window tracking for rate limiting
struct RateLimitWindow {
    /// The distributed counter for this window
    counter: Arc<DistributedCounter>,
    /// Window start time
    started_at: Instant,
    /// Window duration
    duration: Duration,
}

impl RateLimitWindow {
    fn new(actor_id: ActorId, duration: Duration) -> Self {
        Self {
            counter: Arc::new(DistributedCounter::new(actor_id)),
            started_at: Instant::now(),
            duration,
        }
    }

    fn is_expired(&self) -> bool {
        self.started_at.elapsed() > self.duration
    }

    fn remaining_secs(&self) -> u64 {
        let elapsed = self.started_at.elapsed();
        if elapsed >= self.duration {
            0
        } else {
            (self.duration - elapsed).as_secs()
        }
    }

    fn reset(&mut self, actor_id: ActorId) {
        self.counter = Arc::new(DistributedCounter::new(actor_id));
        self.started_at = Instant::now();
    }
}

/// Distributed rate limiter using CRDTs and NATS
pub struct DistributedRateLimiter {
    config: RateLimiterConfig,
    nats: Option<Arc<NatsSync>>,
    /// Rate limit windows per resource (e.g., per IP address)
    windows: Arc<RwLock<HashMap<String, RateLimitWindow>>>,
    /// Channel to receive sync status updates
    sync_status_rx: Option<mpsc::UnboundedReceiver<String>>,
    /// Cleanup task handle (Sprint 12.5)
    cleanup_task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DistributedRateLimiter {
    /// Create a new distributed rate limiter
    pub fn new(config: RateLimiterConfig) -> Self {
        info!(
            "Creating distributed rate limiter for actor {} (window: {}s, max: {})",
            config.actor_id, config.window_duration_secs, config.max_requests
        );

        Self {
            config,
            nats: None,
            windows: Arc::new(RwLock::new(HashMap::new())),
            sync_status_rx: None,
            cleanup_task_handle: None,
        }
    }

    /// Start background cleanup task for expired windows (Sprint 12.5)
    /// This prevents memory leaks from stale entries
    pub fn start_cleanup_task(&mut self) {
        let windows = self.windows.clone();
        let cleanup_interval = Duration::from_secs(self.config.window_duration_secs * 2); // Run cleanup twice per window duration

        info!(
            "Starting cleanup task (interval: {:?})",
            cleanup_interval
        );

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);

            loop {
                interval.tick().await;

                // Perform cleanup
                match windows.write() {
                    Ok(mut windows_guard) => {
                        let before_count = windows_guard.len();

                        // Remove expired windows
                        windows_guard.retain(|resource_id, window| {
                            let keep = !window.is_expired();
                            if !keep {
                                debug!("Cleaning up expired window for: {}", resource_id);
                            }
                            keep
                        });

                        let after_count = windows_guard.len();
                        let cleaned = before_count - after_count;

                        if cleaned > 0 {
                            info!(
                                "Cleanup task removed {} expired windows ({} -> {})",
                                cleaned,
                                before_count,
                                after_count
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Failed to acquire write lock for cleanup: {}", e);
                    }
                }
            }
        });

        self.cleanup_task_handle = Some(handle);
    }

    /// Stop the cleanup task (Sprint 12.5)
    pub fn stop_cleanup_task(&mut self) {
        if let Some(handle) = self.cleanup_task_handle.take() {
            handle.abort();
            info!("Cleanup task stopped");
        }
    }

    /// Sprint 13.5: Start background CRDT compaction task
    /// Periodically compacts G-Counters to prevent unbounded memory growth
    /// from accumulating actor IDs over time
    pub fn start_compaction_task(&mut self, compact_interval_secs: u64) {
        let windows = self.windows.clone();
        let compact_interval = Duration::from_secs(compact_interval_secs);

        info!(
            "Starting CRDT compaction task (interval: {}s)",
            compact_interval_secs
        );

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(compact_interval);

            loop {
                interval.tick().await;

                // Perform compaction on all windows
                match windows.read() {
                    Ok(windows_guard) => {
                        let mut total_compacted = 0;

                        for (resource_id, window) in windows_guard.iter() {
                            // Check if compaction is needed (size threshold)
                            if let Ok(size) = window.counter.estimated_size() {
                                // Compact if size > 1KB (indicates many actors)
                                if size > 1024 {
                                    if let Err(e) = window.counter.compact() {
                                        warn!("Failed to compact counter for {}: {}", resource_id, e);
                                    } else {
                                        total_compacted += 1;
                                        debug!(
                                            "Compacted counter for {} (size was {} bytes)",
                                            resource_id, size
                                        );
                                    }
                                }
                            }
                        }

                        if total_compacted > 0 {
                            info!(
                                "Compaction completed: compacted {} counters across {} resources",
                                total_compacted,
                                windows_guard.len()
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Failed to acquire read lock for compaction: {}", e);
                    }
                }
            }
        });

        // Store handle for cleanup
        if let Some(old_handle) = self.cleanup_task_handle.replace(handle) {
            old_handle.abort();
        }
    }

    /// Connect to NATS and start synchronization
    pub async fn connect_and_sync(&mut self) -> Result<()> {
        if !self.config.auto_sync {
            info!("Auto-sync disabled, skipping NATS connection");
            return Ok(());
        }

        info!("Connecting to NATS for distributed state sync");

        let nats = NatsSync::connect(self.config.nats_config.clone())
            .await
            .context("Failed to connect to NATS")?;

        self.nats = Some(Arc::new(nats));
        info!("Connected to NATS");

        Ok(())
    }

    /// Check if a request should be allowed (rate limit check)
    pub async fn check_rate_limit(&self, resource_id: &str) -> Result<RateLimitDecision> {
        let mut windows = self
            .windows
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        let window = windows
            .entry(resource_id.to_string())
            .or_insert_with(|| {
                RateLimitWindow::new(
                    self.config.actor_id,
                    Duration::from_secs(self.config.window_duration_secs),
                )
            });

        // Check if window expired and reset if needed
        if window.is_expired() {
            debug!("Window expired for {}, resetting", resource_id);
            window.reset(self.config.actor_id);
        }

        // Get current count
        let current_count = window.counter.value()?;

        // Check if request would exceed limit
        if current_count >= self.config.max_requests {
            let remaining_secs = window.remaining_secs();
            warn!(
                "Rate limit exceeded for {} ({}/{})",
                resource_id, current_count, self.config.max_requests
            );

            return Ok(RateLimitDecision::Denied {
                current_count,
                retry_after_secs: remaining_secs,
            });
        }

        // Increment counter locally
        let op = window.counter.increment(1)?;

        debug!(
            "Rate limit check passed for {} ({}/{})",
            resource_id,
            current_count + 1,
            self.config.max_requests
        );

        // Publish increment to NATS if connected
        if let Some(nats) = &self.nats {
            if let Err(e) = nats.publish(self.config.actor_id, op).await {
                warn!("Failed to publish counter increment: {}", e);
                // Don't fail the request if NATS publish fails
            }
        }

        Ok(RateLimitDecision::Allowed {
            current_count: current_count + 1,
            remaining: self.config.max_requests - (current_count + 1),
        })
    }

    /// Start subscribing to counter updates from other nodes
    pub async fn start_subscription(&mut self, resource_id: &str) -> Result<()> {
        if self.nats.is_none() {
            anyhow::bail!("Not connected to NATS");
        }

        let nats = self.nats.as_ref().unwrap().clone();

        // Get or create window for this resource
        let counter = {
            let mut windows = self
                .windows
                .write()
                .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

            let window = windows
                .entry(resource_id.to_string())
                .or_insert_with(|| {
                    RateLimitWindow::new(
                        self.config.actor_id,
                        Duration::from_secs(self.config.window_duration_secs),
                    )
                });

            window.counter.clone()
        };

        // Subscribe to NATS updates
        let status_rx = nats
            .subscribe_and_sync(counter)
            .await
            .context("Failed to subscribe to NATS")?;

        self.sync_status_rx = Some(status_rx);

        info!("Started subscription for resource: {}", resource_id);
        Ok(())
    }

    /// Get the current count for a resource
    pub fn get_count(&self, resource_id: &str) -> Result<u64> {
        let windows = self
            .windows
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        match windows.get(resource_id) {
            Some(window) => window.counter.value(),
            None => Ok(0),
        }
    }

    /// Get all tracked resources and their counts
    pub fn get_all_counts(&self) -> Result<HashMap<String, u64>> {
        let windows = self
            .windows
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;

        let mut counts = HashMap::new();
        for (resource_id, window) in windows.iter() {
            counts.insert(resource_id.clone(), window.counter.value()?);
        }

        Ok(counts)
    }

    /// Manually merge an operation (for testing)
    pub fn merge_operation(&self, resource_id: &str, op: CounterOp) -> Result<()> {
        let mut windows = self
            .windows
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

        let window = windows
            .entry(resource_id.to_string())
            .or_insert_with(|| {
                RateLimitWindow::new(
                    self.config.actor_id,
                    Duration::from_secs(self.config.window_duration_secs),
                )
            });

        window.counter.merge_op(op)?;
        Ok(())
    }

    /// Get actor ID
    pub fn actor_id(&self) -> ActorId {
        self.config.actor_id
    }

    /// Get configuration
    pub fn config(&self) -> &RateLimiterConfig {
        &self.config
    }
}

impl Drop for DistributedRateLimiter {
    fn drop(&mut self) {
        self.stop_cleanup_task();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = RateLimiterConfig::default();
        assert_eq!(config.actor_id, 1);
        assert_eq!(config.window_duration_secs, 60);
        assert_eq!(config.max_requests, 100);
        assert!(config.auto_sync);
    }

    #[test]
    fn test_rate_limiter_creation() {
        let config = RateLimiterConfig::default();
        let limiter = DistributedRateLimiter::new(config);
        assert_eq!(limiter.actor_id(), 1);
    }

    #[tokio::test]
    async fn test_rate_limit_allowed() {
        let config = RateLimiterConfig {
            actor_id: 1,
            max_requests: 5,
            window_duration_secs: 60,
            auto_sync: false,
            ..Default::default()
        };

        let limiter = DistributedRateLimiter::new(config);

        // First request should be allowed
        let decision = limiter.check_rate_limit("test-resource").await.unwrap();
        match decision {
            RateLimitDecision::Allowed {
                current_count,
                remaining,
            } => {
                assert_eq!(current_count, 1);
                assert_eq!(remaining, 4);
            }
            _ => panic!("Expected Allowed decision"),
        }
    }

    #[tokio::test]
    async fn test_rate_limit_denied() {
        let config = RateLimiterConfig {
            actor_id: 1,
            max_requests: 3,
            window_duration_secs: 60,
            auto_sync: false,
            ..Default::default()
        };

        let limiter = DistributedRateLimiter::new(config);

        // Make 3 requests (max)
        for _ in 0..3 {
            limiter.check_rate_limit("test-resource").await.unwrap();
        }

        // 4th request should be denied
        let decision = limiter.check_rate_limit("test-resource").await.unwrap();
        match decision {
            RateLimitDecision::Denied {
                current_count,
                retry_after_secs,
            } => {
                assert_eq!(current_count, 3);
                assert!(retry_after_secs > 0);
            }
            _ => panic!("Expected Denied decision"),
        }
    }

    #[tokio::test]
    async fn test_get_count() {
        let config = RateLimiterConfig {
            actor_id: 1,
            max_requests: 10,
            window_duration_secs: 60,
            auto_sync: false,
            ..Default::default()
        };

        let limiter = DistributedRateLimiter::new(config);

        // Initially should be 0
        assert_eq!(limiter.get_count("test-resource").unwrap(), 0);

        // After one request
        limiter.check_rate_limit("test-resource").await.unwrap();
        assert_eq!(limiter.get_count("test-resource").unwrap(), 1);

        // After three more requests
        for _ in 0..3 {
            limiter.check_rate_limit("test-resource").await.unwrap();
        }
        assert_eq!(limiter.get_count("test-resource").unwrap(), 4);
    }

    #[tokio::test]
    async fn test_multiple_resources() {
        let config = RateLimiterConfig {
            actor_id: 1,
            max_requests: 5,
            window_duration_secs: 60,
            auto_sync: false,
            ..Default::default()
        };

        let limiter = DistributedRateLimiter::new(config);

        // Different resources should have independent counters
        limiter.check_rate_limit("resource-1").await.unwrap();
        limiter.check_rate_limit("resource-1").await.unwrap();

        limiter.check_rate_limit("resource-2").await.unwrap();

        assert_eq!(limiter.get_count("resource-1").unwrap(), 2);
        assert_eq!(limiter.get_count("resource-2").unwrap(), 1);
    }

    #[tokio::test]
    async fn test_merge_operation() {
        let config = RateLimiterConfig {
            actor_id: 1,
            max_requests: 10,
            window_duration_secs: 60,
            auto_sync: false,
            ..Default::default()
        };

        let limiter = DistributedRateLimiter::new(config);

        // Simulate receiving operation from remote node (actor 2)
        let op = CounterOp::Increment {
            actor: 2,
            value: 5,
        };

        limiter.merge_operation("test-resource", op).unwrap();

        // Count should reflect merged operation
        assert_eq!(limiter.get_count("test-resource").unwrap(), 5);

        // Local increment
        limiter.check_rate_limit("test-resource").await.unwrap();

        // Should be 6 total (5 from remote + 1 local)
        assert_eq!(limiter.get_count("test-resource").unwrap(), 6);
    }

    #[tokio::test]
    async fn test_get_all_counts() {
        let config = RateLimiterConfig {
            actor_id: 1,
            max_requests: 10,
            window_duration_secs: 60,
            auto_sync: false,
            ..Default::default()
        };

        let limiter = DistributedRateLimiter::new(config);

        limiter.check_rate_limit("resource-a").await.unwrap();
        limiter.check_rate_limit("resource-a").await.unwrap();

        limiter.check_rate_limit("resource-b").await.unwrap();
        limiter.check_rate_limit("resource-b").await.unwrap();
        limiter.check_rate_limit("resource-b").await.unwrap();

        let counts = limiter.get_all_counts().unwrap();
        assert_eq!(counts.get("resource-a"), Some(&2));
        assert_eq!(counts.get("resource-b"), Some(&3));
    }

    #[test]
    fn test_window_expiration() {
        let window = RateLimitWindow::new(1, Duration::from_millis(100));

        assert!(!window.is_expired());

        std::thread::sleep(Duration::from_millis(150));

        assert!(window.is_expired());
    }

    #[test]
    fn test_window_remaining_secs() {
        let window = RateLimitWindow::new(1, Duration::from_secs(60));

        let remaining = window.remaining_secs();
        assert!(remaining <= 60);
        assert!(remaining > 58); // Should be close to 60

        std::thread::sleep(Duration::from_millis(500));

        let remaining = window.remaining_secs();
        assert!(remaining < 60);
    }
}
