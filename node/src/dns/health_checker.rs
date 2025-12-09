//! Edge Node Health Checker
//!
//! Sprint 30.3: Geo-Aware DNS Resolution
//!
//! Background health monitoring for edge nodes. Performs periodic
//! HTTP health checks and updates the EdgeRegistry accordingly.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{interval, Instant};
use tracing::{debug, error, info, warn};

use crate::dns::edge_registry::EdgeRegistry;

/// Health check result for a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Node ID that was checked
    pub node_id: String,
    /// Whether the check was successful
    pub healthy: bool,
    /// Response time in milliseconds (None if failed)
    pub response_time_ms: Option<u64>,
    /// HTTP status code (None if connection failed)
    pub status_code: Option<u16>,
    /// Error message if check failed
    pub error: Option<String>,
    /// Timestamp of the check
    pub timestamp: u64,
}

/// Tracker for consecutive failures per node
#[derive(Debug, Default)]
struct FailureTracker {
    /// Map of node_id -> consecutive failure count
    failures: HashMap<String, u32>,
}

impl FailureTracker {
    fn new() -> Self {
        Self {
            failures: HashMap::new(),
        }
    }

    /// Record a success, resetting failure count
    fn record_success(&mut self, node_id: &str) {
        self.failures.remove(node_id);
    }

    /// Record a failure, returning the new consecutive count
    fn record_failure(&mut self, node_id: &str) -> u32 {
        let count = self.failures.entry(node_id.to_string()).or_insert(0);
        *count += 1;
        *count
    }

    /// Get current failure count for a node
    fn get_failures(&self, node_id: &str) -> u32 {
        self.failures.get(node_id).copied().unwrap_or(0)
    }
}

/// Configuration for health checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Timeout for each health check request
    pub timeout: Duration,
    /// Number of consecutive failures before marking unhealthy
    pub unhealthy_threshold: u32,
    /// Number of consecutive successes before marking healthy again
    pub healthy_threshold: u32,
    /// Health check endpoint path
    pub health_path: String,
    /// Port to check (usually the node's HTTP port)
    pub health_port: u16,
    /// Use HTTPS for health checks
    pub use_https: bool,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            health_path: "/health".to_string(),
            health_port: 8080,
            use_https: false,
        }
    }
}

/// Background health checker for edge nodes
pub struct HealthChecker {
    /// Reference to the edge registry
    registry: Arc<EdgeRegistry>,
    /// Health check configuration
    config: HealthCheckConfig,
    /// HTTP client for health checks
    client: reqwest::Client,
    /// Failure tracker for consecutive failures
    failure_tracker: Arc<RwLock<FailureTracker>>,
    /// Recovery tracker for consecutive successes after failure
    recovery_tracker: Arc<RwLock<HashMap<String, u32>>>,
    /// Latest health check results
    results: Arc<RwLock<HashMap<String, HealthCheckResult>>>,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(registry: Arc<EdgeRegistry>) -> Self {
        Self::with_config(registry, HealthCheckConfig::default())
    }

    /// Create a health checker with custom configuration
    pub fn with_config(registry: Arc<EdgeRegistry>, config: HealthCheckConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.timeout)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            registry,
            config,
            client,
            failure_tracker: Arc::new(RwLock::new(FailureTracker::new())),
            recovery_tracker: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start background health checking (runs forever)
    pub async fn run(&self) {
        info!(
            "Starting health checker with {}s interval",
            self.config.check_interval.as_secs()
        );

        let mut ticker = interval(self.config.check_interval);

        loop {
            ticker.tick().await;
            self.check_all_nodes().await;
        }
    }

    /// Run a single round of health checks
    pub async fn check_all_nodes(&self) {
        let nodes = self.registry.get_all_nodes().await;
        debug!("Running health checks on {} nodes", nodes.len());

        // Run health checks concurrently with a semaphore to limit parallelism
        let semaphore = Arc::new(tokio::sync::Semaphore::new(50)); // Max 50 concurrent checks

        let handles: Vec<_> = nodes
            .into_iter()
            .map(|node| {
                let semaphore = Arc::clone(&semaphore);
                let checker = self.clone_for_check();
                let node_id = node.id.clone();
                let ip = node.ipv4.map(IpAddr::V4).or(node.ipv6.map(IpAddr::V6));

                tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    if let Some(ip) = ip {
                        checker.check_node(&node_id, ip).await
                    } else {
                        HealthCheckResult {
                            node_id,
                            healthy: false,
                            response_time_ms: None,
                            status_code: None,
                            error: Some("No IP address configured".to_string()),
                            timestamp: current_timestamp(),
                        }
                    }
                })
            })
            .collect();

        // Wait for all checks to complete
        for handle in handles {
            if let Ok(result) = handle.await {
                self.process_result(result).await;
            }
        }
    }

    /// Clone references needed for async health check
    fn clone_for_check(&self) -> HealthCheckRef {
        HealthCheckRef {
            client: self.client.clone(),
            config: self.config.clone(),
        }
    }

    /// Process a health check result
    async fn process_result(&self, result: HealthCheckResult) {
        let node_id = result.node_id.clone();

        // Store result
        self.results.write().await.insert(node_id.clone(), result.clone());

        if result.healthy {
            // Record success
            self.failure_tracker.write().await.record_success(&node_id);

            // Check if node was previously unhealthy and needs recovery tracking
            let mut recovery = self.recovery_tracker.write().await;
            let recovery_count = recovery.entry(node_id.clone()).or_insert(0);
            *recovery_count += 1;

            if *recovery_count >= self.config.healthy_threshold {
                // Node has recovered
                if let Err(e) = self.registry.update_health(&node_id, true).await {
                    warn!("Failed to update node {} health: {}", node_id, e);
                } else {
                    info!(
                        "Node {} marked healthy after {} successful checks",
                        node_id, recovery_count
                    );
                }
                recovery.remove(&node_id);
            }
        } else {
            // Record failure
            let failures = self.failure_tracker.write().await.record_failure(&node_id);

            // Reset recovery tracking
            self.recovery_tracker.write().await.remove(&node_id);

            if failures >= self.config.unhealthy_threshold {
                // Mark node as unhealthy
                if let Err(e) = self.registry.update_health(&node_id, false).await {
                    warn!("Failed to update node {} health: {}", node_id, e);
                } else {
                    warn!(
                        "Node {} marked unhealthy after {} consecutive failures: {:?}",
                        node_id, failures, result.error
                    );
                }
            } else {
                debug!(
                    "Node {} failed check ({}/{}): {:?}",
                    node_id, failures, self.config.unhealthy_threshold, result.error
                );
            }
        }
    }

    /// Get the latest health check result for a node
    pub async fn get_result(&self, node_id: &str) -> Option<HealthCheckResult> {
        self.results.read().await.get(node_id).cloned()
    }

    /// Get all health check results
    pub async fn get_all_results(&self) -> Vec<HealthCheckResult> {
        self.results.read().await.values().cloned().collect()
    }

    /// Get health summary statistics
    pub async fn get_summary(&self) -> HealthSummary {
        let results = self.results.read().await;
        let total = results.len();
        let healthy = results.values().filter(|r| r.healthy).count();
        let unhealthy = total - healthy;

        let avg_response_time = if healthy > 0 {
            let sum: u64 = results
                .values()
                .filter_map(|r| r.response_time_ms)
                .sum();
            Some(sum / healthy as u64)
        } else {
            None
        };

        HealthSummary {
            total_nodes: total,
            healthy_nodes: healthy,
            unhealthy_nodes: unhealthy,
            average_response_time_ms: avg_response_time,
        }
    }

    /// Manually trigger a health check for a specific node
    pub async fn check_single_node(&self, node_id: &str) -> Option<HealthCheckResult> {
        let node = self.registry.get_node(node_id).await?;
        let ip = node.ipv4.map(IpAddr::V4).or(node.ipv6.map(IpAddr::V6))?;

        let checker = self.clone_for_check();
        let result = checker.check_node(node_id, ip).await;
        self.process_result(result.clone()).await;
        Some(result)
    }
}

/// Reference holder for async health checks
struct HealthCheckRef {
    client: reqwest::Client,
    config: HealthCheckConfig,
}

impl HealthCheckRef {
    /// Perform health check on a single node
    async fn check_node(&self, node_id: &str, ip: IpAddr) -> HealthCheckResult {
        let scheme = if self.config.use_https { "https" } else { "http" };
        let url = format!(
            "{}://{}:{}{}",
            scheme, ip, self.config.health_port, self.config.health_path
        );

        let start = Instant::now();

        match self.client.get(&url).send().await {
            Ok(response) => {
                let elapsed = start.elapsed().as_millis() as u64;
                let status = response.status().as_u16();
                let healthy = response.status().is_success();

                HealthCheckResult {
                    node_id: node_id.to_string(),
                    healthy,
                    response_time_ms: Some(elapsed),
                    status_code: Some(status),
                    error: if healthy {
                        None
                    } else {
                        Some(format!("HTTP {}", status))
                    },
                    timestamp: current_timestamp(),
                }
            }
            Err(e) => {
                let error_msg = if e.is_timeout() {
                    "Connection timeout".to_string()
                } else if e.is_connect() {
                    "Connection refused".to_string()
                } else {
                    e.to_string()
                };

                HealthCheckResult {
                    node_id: node_id.to_string(),
                    healthy: false,
                    response_time_ms: None,
                    status_code: None,
                    error: Some(error_msg),
                    timestamp: current_timestamp(),
                }
            }
        }
    }
}

/// Summary of health check statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    /// Total number of nodes being monitored
    pub total_nodes: usize,
    /// Number of healthy nodes
    pub healthy_nodes: usize,
    /// Number of unhealthy nodes
    pub unhealthy_nodes: usize,
    /// Average response time across healthy nodes
    pub average_response_time_ms: Option<u64>,
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use crate::dns::edge_registry::EdgeNode;

    fn create_test_node(id: &str, ip: Ipv4Addr) -> EdgeNode {
        EdgeNode {
            id: id.to_string(),
            ipv4: Some(ip),
            ipv6: None,
            region: "us-east".to_string(),
            country: "US".to_string(),
            city: Some("New York".to_string()),
            latitude: 40.7128,
            longitude: -74.0060,
            capacity: 100,
            healthy: true,
            last_health_check: current_timestamp(),
            consecutive_failures: 0,
            registered_at: current_timestamp(),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_failure_tracker() {
        let mut tracker = FailureTracker::new();

        assert_eq!(tracker.get_failures("node1"), 0);

        assert_eq!(tracker.record_failure("node1"), 1);
        assert_eq!(tracker.record_failure("node1"), 2);
        assert_eq!(tracker.record_failure("node1"), 3);

        assert_eq!(tracker.get_failures("node1"), 3);

        tracker.record_success("node1");
        assert_eq!(tracker.get_failures("node1"), 0);
    }

    #[tokio::test]
    async fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();

        assert_eq!(config.check_interval, Duration::from_secs(10));
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.unhealthy_threshold, 3);
        assert_eq!(config.healthy_threshold, 2);
        assert_eq!(config.health_path, "/health");
        assert_eq!(config.health_port, 8080);
        assert!(!config.use_https);
    }

    #[tokio::test]
    async fn test_health_checker_creation() {
        let registry = Arc::new(EdgeRegistry::new());
        let checker = HealthChecker::new(registry);

        assert_eq!(checker.config.unhealthy_threshold, 3);
        assert_eq!(checker.config.healthy_threshold, 2);
    }

    #[tokio::test]
    async fn test_health_checker_with_custom_config() {
        let registry = Arc::new(EdgeRegistry::new());
        let config = HealthCheckConfig {
            check_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(10),
            unhealthy_threshold: 5,
            healthy_threshold: 3,
            health_path: "/healthz".to_string(),
            health_port: 9090,
            use_https: true,
        };

        let checker = HealthChecker::with_config(registry, config);

        assert_eq!(checker.config.unhealthy_threshold, 5);
        assert_eq!(checker.config.health_path, "/healthz");
        assert!(checker.config.use_https);
    }

    #[tokio::test]
    async fn test_health_check_result_structure() {
        let result = HealthCheckResult {
            node_id: "node-1".to_string(),
            healthy: true,
            response_time_ms: Some(25),
            status_code: Some(200),
            error: None,
            timestamp: current_timestamp(),
        };

        assert!(result.healthy);
        assert_eq!(result.response_time_ms, Some(25));
        assert_eq!(result.status_code, Some(200));
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_health_check_failed_result() {
        let result = HealthCheckResult {
            node_id: "node-1".to_string(),
            healthy: false,
            response_time_ms: None,
            status_code: None,
            error: Some("Connection refused".to_string()),
            timestamp: current_timestamp(),
        };

        assert!(!result.healthy);
        assert!(result.response_time_ms.is_none());
        assert_eq!(result.error, Some("Connection refused".to_string()));
    }

    #[tokio::test]
    async fn test_get_summary_empty() {
        let registry = Arc::new(EdgeRegistry::new());
        let checker = HealthChecker::new(registry);

        let summary = checker.get_summary().await;

        assert_eq!(summary.total_nodes, 0);
        assert_eq!(summary.healthy_nodes, 0);
        assert_eq!(summary.unhealthy_nodes, 0);
        assert!(summary.average_response_time_ms.is_none());
    }

    #[tokio::test]
    async fn test_process_healthy_result() {
        let registry = Arc::new(EdgeRegistry::new());

        // Register a node first
        let node = create_test_node("node-1", Ipv4Addr::new(10, 0, 0, 1));
        registry.register(node).await.unwrap();

        let checker = HealthChecker::new(registry.clone());

        // Simulate healthy check result
        let result = HealthCheckResult {
            node_id: "node-1".to_string(),
            healthy: true,
            response_time_ms: Some(25),
            status_code: Some(200),
            error: None,
            timestamp: current_timestamp(),
        };

        checker.process_result(result).await;

        // Verify result is stored
        let stored = checker.get_result("node-1").await;
        assert!(stored.is_some());
        assert!(stored.unwrap().healthy);
    }

    #[tokio::test]
    async fn test_process_unhealthy_results_threshold() {
        let registry = Arc::new(EdgeRegistry::new());

        // Register a healthy node
        let node = create_test_node("node-1", Ipv4Addr::new(10, 0, 0, 1));
        registry.register(node).await.unwrap();

        let config = HealthCheckConfig {
            unhealthy_threshold: 3,
            ..Default::default()
        };
        let checker = HealthChecker::with_config(registry.clone(), config);

        // First two failures shouldn't mark unhealthy
        for i in 1..=2 {
            let result = HealthCheckResult {
                node_id: "node-1".to_string(),
                healthy: false,
                response_time_ms: None,
                status_code: None,
                error: Some(format!("Failure {}", i)),
                timestamp: current_timestamp(),
            };
            checker.process_result(result).await;
        }

        // Node should still be healthy in registry (threshold not reached)
        let node = registry.get_node("node-1").await.unwrap();
        assert!(node.healthy);

        // Third failure should mark unhealthy
        let result = HealthCheckResult {
            node_id: "node-1".to_string(),
            healthy: false,
            response_time_ms: None,
            status_code: None,
            error: Some("Failure 3".to_string()),
            timestamp: current_timestamp(),
        };
        checker.process_result(result).await;

        // Now node should be unhealthy
        let node = registry.get_node("node-1").await.unwrap();
        assert!(!node.healthy);
    }

    #[tokio::test]
    async fn test_recovery_threshold() {
        let registry = Arc::new(EdgeRegistry::new());

        // Register an unhealthy node
        let mut node = create_test_node("node-1", Ipv4Addr::new(10, 0, 0, 1));
        node.healthy = false;
        registry.register(node).await.unwrap();

        let config = HealthCheckConfig {
            healthy_threshold: 2,
            ..Default::default()
        };
        let checker = HealthChecker::with_config(registry.clone(), config);

        // First success shouldn't mark healthy yet
        let result = HealthCheckResult {
            node_id: "node-1".to_string(),
            healthy: true,
            response_time_ms: Some(25),
            status_code: Some(200),
            error: None,
            timestamp: current_timestamp(),
        };
        checker.process_result(result.clone()).await;

        // Still unhealthy (threshold not reached)
        let node = registry.get_node("node-1").await.unwrap();
        assert!(!node.healthy);

        // Second success should mark healthy
        checker.process_result(result).await;

        let node = registry.get_node("node-1").await.unwrap();
        assert!(node.healthy);
    }

    #[tokio::test]
    async fn test_get_all_results() {
        let registry = Arc::new(EdgeRegistry::new());
        let checker = HealthChecker::new(registry);

        // Add some results manually
        {
            let mut results = checker.results.write().await;
            results.insert(
                "node-1".to_string(),
                HealthCheckResult {
                    node_id: "node-1".to_string(),
                    healthy: true,
                    response_time_ms: Some(25),
                    status_code: Some(200),
                    error: None,
                    timestamp: current_timestamp(),
                },
            );
            results.insert(
                "node-2".to_string(),
                HealthCheckResult {
                    node_id: "node-2".to_string(),
                    healthy: false,
                    response_time_ms: None,
                    status_code: None,
                    error: Some("Timeout".to_string()),
                    timestamp: current_timestamp(),
                },
            );
        }

        let all = checker.get_all_results().await;
        assert_eq!(all.len(), 2);

        let summary = checker.get_summary().await;
        assert_eq!(summary.total_nodes, 2);
        assert_eq!(summary.healthy_nodes, 1);
        assert_eq!(summary.unhealthy_nodes, 1);
    }

    #[tokio::test]
    async fn test_check_all_nodes_no_nodes() {
        let registry = Arc::new(EdgeRegistry::new());
        let checker = HealthChecker::new(registry);

        // Should complete without error even with no nodes
        checker.check_all_nodes().await;

        let summary = checker.get_summary().await;
        assert_eq!(summary.total_nodes, 0);
    }

    #[tokio::test]
    async fn test_health_summary_serialization() {
        let summary = HealthSummary {
            total_nodes: 10,
            healthy_nodes: 8,
            unhealthy_nodes: 2,
            average_response_time_ms: Some(42),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: HealthSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.total_nodes, 10);
        assert_eq!(parsed.healthy_nodes, 8);
        assert_eq!(parsed.average_response_time_ms, Some(42));
    }
}
