use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// System and application metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    // System metrics
    pub cpu_usage_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub memory_percent: f32,

    // Network metrics
    pub active_connections: u64,
    pub requests_total: u64,
    pub requests_per_second: f64,

    // Performance metrics
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,

    // Cache metrics
    pub cache_hit_rate: f64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_memory_mb: u64,

    // Proxy metrics
    pub proxy_status: String,
    pub cache_status: String,
    pub uptime_seconds: u64,

    // WAF metrics (Sprint 8)
    pub waf_requests_analyzed: u64,
    pub waf_requests_blocked: u64,
    pub waf_requests_logged: u64,
    pub waf_rules_triggered: u64,

    // Timestamp
    pub timestamp: i64,
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            cpu_usage_percent: 0.0,
            memory_used_mb: 0,
            memory_total_mb: 0,
            memory_percent: 0.0,
            active_connections: 0,
            requests_total: 0,
            requests_per_second: 0.0,
            avg_latency_ms: 0.0,
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            cache_hit_rate: 0.0,
            cache_hits: 0,
            cache_misses: 0,
            cache_memory_mb: 0,
            proxy_status: "unknown".to_string(),
            cache_status: "unknown".to_string(),
            uptime_seconds: 0,
            waf_requests_analyzed: 0,
            waf_requests_blocked: 0,
            waf_requests_logged: 0,
            waf_rules_triggered: 0,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

impl NodeMetrics {
    /// Format metrics in Prometheus format
    pub fn to_prometheus_format(&self) -> String {
        format!(
            "# HELP aegis_cpu_usage_percent CPU usage percentage\n\
             # TYPE aegis_cpu_usage_percent gauge\n\
             aegis_cpu_usage_percent {}\n\
             \n\
             # HELP aegis_memory_used_bytes Memory used in bytes\n\
             # TYPE aegis_memory_used_bytes gauge\n\
             aegis_memory_used_bytes {}\n\
             \n\
             # HELP aegis_memory_percent Memory usage percentage\n\
             # TYPE aegis_memory_percent gauge\n\
             aegis_memory_percent {}\n\
             \n\
             # HELP aegis_active_connections Active connections count\n\
             # TYPE aegis_active_connections gauge\n\
             aegis_active_connections {}\n\
             \n\
             # HELP aegis_requests_total Total requests processed\n\
             # TYPE aegis_requests_total counter\n\
             aegis_requests_total {}\n\
             \n\
             # HELP aegis_requests_per_second Current requests per second\n\
             # TYPE aegis_requests_per_second gauge\n\
             aegis_requests_per_second {}\n\
             \n\
             # HELP aegis_latency_milliseconds Average latency in milliseconds\n\
             # TYPE aegis_latency_milliseconds gauge\n\
             aegis_latency_milliseconds {}\n\
             \n\
             # HELP aegis_latency_p50_milliseconds P50 latency in milliseconds\n\
             # TYPE aegis_latency_p50_milliseconds gauge\n\
             aegis_latency_p50_milliseconds {}\n\
             \n\
             # HELP aegis_latency_p95_milliseconds P95 latency in milliseconds\n\
             # TYPE aegis_latency_p95_milliseconds gauge\n\
             aegis_latency_p95_milliseconds {}\n\
             \n\
             # HELP aegis_latency_p99_milliseconds P99 latency in milliseconds\n\
             # TYPE aegis_latency_p99_milliseconds gauge\n\
             aegis_latency_p99_milliseconds {}\n\
             \n\
             # HELP aegis_cache_hit_rate Cache hit rate percentage\n\
             # TYPE aegis_cache_hit_rate gauge\n\
             aegis_cache_hit_rate {}\n\
             \n\
             # HELP aegis_cache_hits_total Total cache hits\n\
             # TYPE aegis_cache_hits_total counter\n\
             aegis_cache_hits_total {}\n\
             \n\
             # HELP aegis_cache_misses_total Total cache misses\n\
             # TYPE aegis_cache_misses_total counter\n\
             aegis_cache_misses_total {}\n\
             \n\
             # HELP aegis_cache_memory_bytes Cache memory usage in bytes\n\
             # TYPE aegis_cache_memory_bytes gauge\n\
             aegis_cache_memory_bytes {}\n\
             \n\
             # HELP aegis_uptime_seconds Node uptime in seconds\n\
             # TYPE aegis_uptime_seconds counter\n\
             aegis_uptime_seconds {}\n\
             \n\
             # HELP aegis_proxy_status Proxy status (1=running, 0=stopped)\n\
             # TYPE aegis_proxy_status gauge\n\
             aegis_proxy_status {}\n\
             \n\
             # HELP aegis_cache_status Cache status (1=connected, 0=disconnected)\n\
             # TYPE aegis_cache_status gauge\n\
             aegis_cache_status {}\n",
            self.cpu_usage_percent,
            self.memory_used_mb * 1024 * 1024, // Convert to bytes
            self.memory_percent,
            self.active_connections,
            self.requests_total,
            self.requests_per_second,
            self.avg_latency_ms,
            self.p50_latency_ms,
            self.p95_latency_ms,
            self.p99_latency_ms,
            self.cache_hit_rate,
            self.cache_hits,
            self.cache_misses,
            self.cache_memory_mb * 1024 * 1024, // Convert to bytes
            self.uptime_seconds,
            if self.proxy_status == "running" { 1 } else { 0 },
            if self.cache_status == "connected" {
                1
            } else {
                0
            },
        )
    }
}

/// Metrics collector - tracks performance data
pub struct MetricsCollector {
    metrics: Arc<RwLock<NodeMetrics>>,
    start_time: Instant,
    latency_samples: Arc<RwLock<Vec<f64>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(NodeMetrics::default())),
            start_time: Instant::now(),
            latency_samples: Arc::new(RwLock::new(Vec::with_capacity(1000))),
        }
    }

    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> NodeMetrics {
        self.metrics.read().await.clone()
    }

    /// Update system metrics (CPU, memory)
    pub async fn update_system_metrics(&self) {
        use sysinfo::System;

        let mut sys = System::new_all();
        sys.refresh_all();

        // Get CPU usage
        let cpu_usage = sys.global_cpu_info().cpu_usage();

        // Get memory usage
        let used_memory = sys.used_memory() / 1024 / 1024; // Convert to MB
        let total_memory = sys.total_memory() / 1024 / 1024; // Convert to MB
        let memory_percent = (used_memory as f32 / total_memory as f32) * 100.0;

        // Update uptime
        let uptime = self.start_time.elapsed().as_secs();

        let mut metrics = self.metrics.write().await;
        metrics.cpu_usage_percent = cpu_usage;
        metrics.memory_used_mb = used_memory;
        metrics.memory_total_mb = total_memory;
        metrics.memory_percent = memory_percent;
        metrics.uptime_seconds = uptime;
        metrics.timestamp = chrono::Utc::now().timestamp();
    }

    /// Record a request
    pub async fn record_request(&self, latency_ms: f64) {
        let mut metrics = self.metrics.write().await;
        metrics.requests_total += 1;

        // Update latency samples
        let mut samples = self.latency_samples.write().await;
        samples.push(latency_ms);

        // Keep only last 1000 samples
        if samples.len() > 1000 {
            samples.remove(0);
        }

        // Calculate percentiles
        if !samples.is_empty() {
            let mut sorted = samples.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

            metrics.avg_latency_ms = sorted.iter().sum::<f64>() / sorted.len() as f64;
            metrics.p50_latency_ms = sorted[sorted.len() / 2];
            metrics.p95_latency_ms = sorted[(sorted.len() as f64 * 0.95) as usize];
            metrics.p99_latency_ms = sorted[(sorted.len() as f64 * 0.99) as usize];
        }
    }

    /// Record cache hit
    pub async fn record_cache_hit(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_hits += 1;

        let total = metrics.cache_hits + metrics.cache_misses;
        if total > 0 {
            metrics.cache_hit_rate = (metrics.cache_hits as f64 / total as f64) * 100.0;
        }
    }

    /// Record cache miss
    pub async fn record_cache_miss(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_misses += 1;

        let total = metrics.cache_hits + metrics.cache_misses;
        if total > 0 {
            metrics.cache_hit_rate = (metrics.cache_hits as f64 / total as f64) * 100.0;
        }
    }

    /// Update active connections count
    pub async fn set_active_connections(&self, count: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.active_connections = count;
    }

    /// Update proxy status
    pub async fn set_proxy_status(&self, status: &str) {
        let mut metrics = self.metrics.write().await;
        metrics.proxy_status = status.to_string();
    }

    /// Update cache status
    pub async fn set_cache_status(&self, status: &str) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_status = status.to_string();
    }

    /// Update cache memory usage from Redis/DragonflyDB stats
    pub async fn set_cache_memory(&self, memory_mb: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.cache_memory_mb = memory_mb;
    }

    /// Calculate requests per second
    pub async fn calculate_rps(&self) {
        let metrics = self.metrics.read().await;
        let uptime = self.start_time.elapsed().as_secs();

        if uptime > 0 {
            let rps = metrics.requests_total as f64 / uptime as f64;
            drop(metrics); // Release read lock

            let mut metrics = self.metrics.write().await;
            metrics.requests_per_second = rps;
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_initialization() {
        let collector = MetricsCollector::new();
        let metrics = collector.get_metrics().await;

        assert_eq!(metrics.requests_total, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.cache_misses, 0);
    }

    #[tokio::test]
    async fn test_record_request() {
        let collector = MetricsCollector::new();

        collector.record_request(10.5).await;
        collector.record_request(20.3).await;
        collector.record_request(15.7).await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.requests_total, 3);
        assert!(metrics.avg_latency_ms > 0.0);
    }

    #[tokio::test]
    async fn test_cache_hit_rate_calculation() {
        let collector = MetricsCollector::new();

        // Record 8 hits and 2 misses = 80% hit rate
        for _ in 0..8 {
            collector.record_cache_hit().await;
        }
        for _ in 0..2 {
            collector.record_cache_miss().await;
        }

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.cache_hits, 8);
        assert_eq!(metrics.cache_misses, 2);
        assert!((metrics.cache_hit_rate - 80.0).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_prometheus_format() {
        let collector = MetricsCollector::new();
        collector.set_proxy_status("running").await;
        collector.set_cache_status("connected").await;

        let metrics = collector.get_metrics().await;
        let prometheus = metrics.to_prometheus_format();

        assert!(prometheus.contains("# HELP"));
        assert!(prometheus.contains("# TYPE"));
        assert!(prometheus.contains("aegis_cpu_usage_percent"));
        assert!(prometheus.contains("aegis_memory_used_bytes"));
        assert!(prometheus.contains("aegis_cache_hit_rate"));
        assert!(prometheus.contains("aegis_uptime_seconds"));
    }

    #[tokio::test]
    async fn test_latency_percentiles() {
        let collector = MetricsCollector::new();

        // Record various latencies
        let latencies = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        for latency in latencies {
            collector.record_request(latency).await;
        }

        let metrics = collector.get_metrics().await;
        assert!(metrics.avg_latency_ms > 0.0);
        assert!(metrics.p50_latency_ms > 0.0);
        assert!(metrics.p95_latency_ms > metrics.p50_latency_ms);
        assert!(metrics.p99_latency_ms >= metrics.p95_latency_ms);
    }

    #[tokio::test]
    async fn test_system_metrics_update() {
        let collector = MetricsCollector::new();

        collector.update_system_metrics().await;

        let metrics = collector.get_metrics().await;
        assert!(metrics.memory_total_mb > 0);
        // uptime_seconds is u64, so it's always >= 0
    }

    #[tokio::test]
    async fn test_active_connections_tracking() {
        let collector = MetricsCollector::new();

        collector.set_active_connections(10).await;
        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.active_connections, 10);

        collector.set_active_connections(25).await;
        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.active_connections, 25);
    }

    #[tokio::test]
    async fn test_status_tracking() {
        let collector = MetricsCollector::new();

        collector.set_proxy_status("running").await;
        collector.set_cache_status("connected").await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.proxy_status, "running");
        assert_eq!(metrics.cache_status, "connected");
    }
}
