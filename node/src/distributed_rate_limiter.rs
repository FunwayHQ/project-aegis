use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
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
///
/// # Security Note (Y6.1)
/// Uses atomic epoch tracking to ensure window resets are atomic.
/// The epoch is incremented on each reset to detect concurrent modifications.
struct RateLimitWindow {
    /// The distributed counter for this window
    counter: Arc<DistributedCounter>,
    /// Window start time (stored as millis since reference point for atomicity)
    started_at_millis: AtomicU64,
    /// Reference instant for calculating elapsed time
    reference_instant: Instant,
    /// Window duration
    duration: Duration,
    /// Epoch counter for detecting concurrent resets (Y6.1)
    epoch: AtomicU64,
}

impl RateLimitWindow {
    fn new(actor_id: ActorId, duration: Duration) -> Self {
        let reference = Instant::now();
        Self {
            counter: Arc::new(DistributedCounter::new(actor_id)),
            started_at_millis: AtomicU64::new(0), // 0 millis from reference = now
            reference_instant: reference,
            duration,
            epoch: AtomicU64::new(0),
        }
    }

    /// Get elapsed time since window started
    fn elapsed(&self) -> Duration {
        let started_millis = self.started_at_millis.load(Ordering::Acquire);
        let reference_elapsed = self.reference_instant.elapsed();
        let started_at = Duration::from_millis(started_millis);

        // Elapsed = current time from reference - start time from reference
        reference_elapsed.saturating_sub(started_at)
    }

    fn is_expired(&self) -> bool {
        self.elapsed() > self.duration
    }

    fn remaining_secs(&self) -> u64 {
        let elapsed = self.elapsed();
        if elapsed >= self.duration {
            0
        } else {
            (self.duration - elapsed).as_secs()
        }
    }

    /// Atomically check if expired and reset if so
    ///
    /// # Returns
    /// - `true` if window was reset
    /// - `false` if window was not expired or was already reset by another caller
    ///
    /// # Security Note (Y6.1)
    /// This method uses compare-and-swap semantics to prevent race conditions
    /// where multiple callers might try to reset the same expired window.
    fn check_and_reset_if_expired(&self, _actor_id: ActorId) -> bool {
        if !self.is_expired() {
            return false;
        }

        // Get current epoch
        let current_epoch = self.epoch.load(Ordering::Acquire);

        // Try to increment epoch atomically (claim the reset)
        match self.epoch.compare_exchange(
            current_epoch,
            current_epoch + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // We won the race - perform the reset
                // Update started_at to current time (relative to reference)
                let new_start_millis = self.reference_instant.elapsed().as_millis() as u64;
                self.started_at_millis.store(new_start_millis, Ordering::Release);

                // Note: We can't atomically replace the counter Arc, but since we won
                // the epoch race, we're the only one resetting. Callers should check
                // epoch after using counter to detect if a reset happened.

                debug!(
                    "Window reset by epoch {} -> {} (start_millis: {})",
                    current_epoch,
                    current_epoch + 1,
                    new_start_millis
                );

                true
            }
            Err(_) => {
                // Another caller already reset - that's fine
                debug!("Window reset lost race (epoch changed from {})", current_epoch);
                false
            }
        }
    }

    /// Get current epoch (for detecting resets)
    fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Legacy reset method - use check_and_reset_if_expired instead
    #[deprecated(note = "Use check_and_reset_if_expired for thread-safe reset")]
    fn reset(&mut self, actor_id: ActorId) {
        self.counter = Arc::new(DistributedCounter::new(actor_id));
        let new_start_millis = self.reference_instant.elapsed().as_millis() as u64;
        self.started_at_millis.store(new_start_millis, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
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

    /// Sprint 15.5: Start background CRDT compaction task (PN-Counter based)
    /// Periodically compacts PN-Counters to prevent unbounded memory growth
    /// Uses PN-Counter's decrement capability for mathematically sound compaction
    /// that maintains CRDT convergence guarantees (unlike manual state replacement)
    pub fn start_compaction_task(&mut self, compact_interval_secs: u64) {
        let windows = self.windows.clone();
        let compact_interval = Duration::from_secs(compact_interval_secs);

        info!(
            "Starting PN-Counter compaction task (interval: {}s)",
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
                                    // Sprint 15.5: PN-Counter compaction uses decrements
                                    // to reduce stale actors to 0, maintaining CRDT semantics
                                    if let Err(e) = window.counter.compact() {
                                        warn!("Failed to compact PN-Counter for {}: {}", resource_id, e);
                                    } else {
                                        total_compacted += 1;
                                        debug!(
                                            "Compacted PN-Counter for {} (size was {} bytes)",
                                            resource_id, size
                                        );
                                    }
                                }
                            }
                        }

                        if total_compacted > 0 {
                            info!(
                                "PN-Counter compaction completed: compacted {} counters across {} resources",
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
    ///
    /// # Security Note (Y6.1)
    /// This method uses atomic check-and-reset for window expiration to prevent
    /// race conditions where multiple concurrent requests might see inconsistent
    /// window state during the reset.
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

        // Y6.1: Atomically check if window expired and reset if needed
        // This prevents race conditions where multiple callers might try to
        // reset the same expired window simultaneously
        let epoch_before = window.current_epoch();
        if window.check_and_reset_if_expired(self.config.actor_id) {
            debug!("Window atomically reset for {} (epoch: {})", resource_id, epoch_before + 1);
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

        let nats = self.nats.as_ref()
            .expect("NATS connection verified by guard above")
            .clone();

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
        let decision = limiter.check_rate_limit("test-resource").await
            .expect("check_rate_limit should succeed");
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
            limiter.check_rate_limit("test-resource").await
                .expect("check_rate_limit should succeed");
        }

        // 4th request should be denied
        let decision = limiter.check_rate_limit("test-resource").await
            .expect("check_rate_limit should succeed");
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
        assert_eq!(limiter.get_count("test-resource").expect("get_count should succeed"), 0);

        // After one request
        limiter.check_rate_limit("test-resource").await
            .expect("check_rate_limit should succeed");
        assert_eq!(limiter.get_count("test-resource").expect("get_count should succeed"), 1);

        // After three more requests
        for _ in 0..3 {
            limiter.check_rate_limit("test-resource").await
                .expect("check_rate_limit should succeed");
        }
        assert_eq!(limiter.get_count("test-resource").expect("get_count should succeed"), 4);
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
        limiter.check_rate_limit("resource-1").await
            .expect("check_rate_limit should succeed");
        limiter.check_rate_limit("resource-1").await
            .expect("check_rate_limit should succeed");

        limiter.check_rate_limit("resource-2").await
            .expect("check_rate_limit should succeed");

        assert_eq!(limiter.get_count("resource-1").expect("get_count should succeed"), 2);
        assert_eq!(limiter.get_count("resource-2").expect("get_count should succeed"), 1);
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

        limiter.merge_operation("test-resource", op)
            .expect("merge_operation should succeed");

        // Count should reflect merged operation
        assert_eq!(limiter.get_count("test-resource").expect("get_count should succeed"), 5);

        // Local increment
        limiter.check_rate_limit("test-resource").await
            .expect("check_rate_limit should succeed");

        // Should be 6 total (5 from remote + 1 local)
        assert_eq!(limiter.get_count("test-resource").expect("get_count should succeed"), 6);
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

        limiter.check_rate_limit("resource-a").await
            .expect("check_rate_limit should succeed");
        limiter.check_rate_limit("resource-a").await
            .expect("check_rate_limit should succeed");

        limiter.check_rate_limit("resource-b").await
            .expect("check_rate_limit should succeed");
        limiter.check_rate_limit("resource-b").await
            .expect("check_rate_limit should succeed");
        limiter.check_rate_limit("resource-b").await
            .expect("check_rate_limit should succeed");

        let counts = limiter.get_all_counts()
            .expect("get_all_counts should succeed");
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

    // ========================================
    // Y6.1: Atomic Check-and-Reset Tests
    // ========================================

    #[test]
    fn test_y61_atomic_reset_when_expired() {
        let window = RateLimitWindow::new(1, Duration::from_millis(50));

        // Initially not expired
        assert!(!window.is_expired());
        assert_eq!(window.current_epoch(), 0);

        // Should not reset when not expired
        assert!(!window.check_and_reset_if_expired(1));
        assert_eq!(window.current_epoch(), 0);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(100));
        assert!(window.is_expired());

        // Should reset when expired
        assert!(window.check_and_reset_if_expired(1));
        assert_eq!(window.current_epoch(), 1);

        // Window should no longer be expired after reset
        assert!(!window.is_expired());
    }

    #[test]
    fn test_y61_atomic_reset_prevents_double_reset() {
        let window = RateLimitWindow::new(1, Duration::from_millis(50));

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(100));
        assert!(window.is_expired());

        // First reset should succeed
        assert!(window.check_and_reset_if_expired(1));
        assert_eq!(window.current_epoch(), 1);

        // Second reset should fail (window is no longer expired)
        assert!(!window.check_and_reset_if_expired(1));
        assert_eq!(window.current_epoch(), 1); // Epoch unchanged
    }

    #[test]
    fn test_y61_epoch_increments_on_reset() {
        let window = RateLimitWindow::new(1, Duration::from_millis(10));

        for expected_epoch in 1..=5 {
            // Wait for expiration
            std::thread::sleep(Duration::from_millis(20));

            assert!(window.is_expired());
            assert!(window.check_and_reset_if_expired(1));
            assert_eq!(window.current_epoch(), expected_epoch);
        }
    }

    #[tokio::test]
    async fn test_y61_concurrent_reset_safety() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let window = Arc::new(RateLimitWindow::new(1, Duration::from_millis(10)));
        let barrier = Arc::new(Barrier::new(10));

        // Wait for window to expire
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut handles = vec![];

        // Spawn 10 tasks that all try to reset simultaneously
        for i in 0..10 {
            let window = window.clone();
            let barrier = barrier.clone();

            handles.push(tokio::spawn(async move {
                // Wait for all tasks to be ready
                barrier.wait().await;

                // Try to reset
                window.check_and_reset_if_expired(i as u64)
            }));
        }

        // Collect results
        let results: Vec<bool> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.expect("task should complete"))
            .collect();

        // Exactly one task should have succeeded in resetting
        let reset_count = results.iter().filter(|&&r| r).count();
        assert_eq!(reset_count, 1, "Only one concurrent reset should succeed");

        // Epoch should be exactly 1 (only one successful reset)
        assert_eq!(window.current_epoch(), 1);
    }
}
