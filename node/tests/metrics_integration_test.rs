use aegis_node::metrics::{MetricsCollector, NodeMetrics};
use std::sync::Arc;

#[tokio::test]
async fn test_metrics_collector_initialization() {
    let collector = MetricsCollector::new();
    let metrics = collector.get_metrics().await;

    assert_eq!(metrics.requests_total, 0);
    assert_eq!(metrics.cache_hits, 0);
    assert_eq!(metrics.cache_misses, 0);
    assert_eq!(metrics.active_connections, 0);
}

#[tokio::test]
async fn test_request_tracking() {
    let collector = MetricsCollector::new();

    // Record 10 requests with varying latencies
    for i in 1..=10 {
        collector.record_request(i as f64 * 10.0).await;
    }

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 10);
    assert!(metrics.avg_latency_ms > 0.0);
    assert!(metrics.p50_latency_ms > 0.0);
    assert!(metrics.p95_latency_ms > 0.0);
}

#[tokio::test]
async fn test_cache_metrics_tracking() {
    let collector = MetricsCollector::new();

    // Simulate 80% hit rate (8 hits, 2 misses)
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
async fn test_cache_hit_rate_zero_operations() {
    let collector = MetricsCollector::new();
    let metrics = collector.get_metrics().await;

    // No operations = 0% hit rate
    assert_eq!(metrics.cache_hit_rate, 0.0);
}

#[tokio::test]
async fn test_cache_hit_rate_100_percent() {
    let collector = MetricsCollector::new();

    // All hits, no misses = 100%
    for _ in 0..10 {
        collector.record_cache_hit().await;
    }

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.cache_hits, 10);
    assert_eq!(metrics.cache_misses, 0);
    assert!((metrics.cache_hit_rate - 100.0).abs() < 0.1);
}

#[tokio::test]
async fn test_system_metrics_collection() {
    let collector = MetricsCollector::new();

    // Update system metrics
    collector.update_system_metrics().await;

    let metrics = collector.get_metrics().await;

    // Should have collected real system data
    assert!(metrics.memory_total_mb > 0);
    assert!(metrics.uptime_seconds >= 0);
    // CPU and memory percent should be valid ranges
    assert!(metrics.cpu_usage_percent >= 0.0);
    assert!(metrics.cpu_usage_percent <= 100.0);
    assert!(metrics.memory_percent >= 0.0);
    assert!(metrics.memory_percent <= 100.0);
}

#[tokio::test]
async fn test_status_updates() {
    let collector = MetricsCollector::new();

    collector.set_proxy_status("running").await;
    collector.set_cache_status("connected").await;
    collector.set_active_connections(5).await;
    collector.set_cache_memory(128).await;

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.proxy_status, "running");
    assert_eq!(metrics.cache_status, "connected");
    assert_eq!(metrics.active_connections, 5);
    assert_eq!(metrics.cache_memory_mb, 128);
}

#[tokio::test]
async fn test_latency_percentile_calculation() {
    let collector = MetricsCollector::new();

    // Add known latencies
    let latencies = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
    for latency in latencies {
        collector.record_request(latency).await;
    }

    let metrics = collector.get_metrics().await;

    // P50 should be around 50ms (median of 10-100)
    assert!(metrics.p50_latency_ms >= 40.0 && metrics.p50_latency_ms <= 60.0);

    // P95 should be around 95ms
    assert!(metrics.p95_latency_ms >= 85.0 && metrics.p95_latency_ms <= 100.0);

    // Average should be 55ms
    assert!((metrics.avg_latency_ms - 55.0).abs() < 5.0);
}

#[tokio::test]
async fn test_requests_per_second_calculation() {
    let collector = MetricsCollector::new();

    // Record 10 requests
    for _ in 0..10 {
        collector.record_request(10.0).await;
    }

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    collector.calculate_rps().await;
    let metrics = collector.get_metrics().await;

    // Should have calculated some RPS
    assert!(metrics.requests_per_second > 0.0);
    assert_eq!(metrics.requests_total, 10);
}

#[tokio::test]
async fn test_prometheus_format_output() {
    let collector = MetricsCollector::new();

    collector.set_proxy_status("running").await;
    collector.set_cache_status("connected").await;
    collector.record_request(25.5).await;
    collector.record_cache_hit().await;
    collector.record_cache_miss().await;

    let metrics = collector.get_metrics().await;
    let prometheus = metrics.to_prometheus_format();

    // Verify Prometheus format structure
    assert!(prometheus.contains("# HELP aegis_cpu_usage_percent"));
    assert!(prometheus.contains("# TYPE aegis_cpu_usage_percent gauge"));
    assert!(prometheus.contains("aegis_requests_total"));
    assert!(prometheus.contains("aegis_cache_hit_rate"));
    assert!(prometheus.contains("aegis_uptime_seconds"));
    assert!(prometheus.contains("aegis_proxy_status 1")); // running = 1
    assert!(prometheus.contains("aegis_cache_status 1")); // connected = 1
}

#[tokio::test]
async fn test_prometheus_format_status_stopped() {
    let collector = MetricsCollector::new();

    collector.set_proxy_status("stopped").await;
    collector.set_cache_status("disconnected").await;

    let metrics = collector.get_metrics().await;
    let prometheus = metrics.to_prometheus_format();

    // Stopped proxy and disconnected cache should be 0
    assert!(prometheus.contains("aegis_proxy_status 0"));
    assert!(prometheus.contains("aegis_cache_status 0"));
}

#[tokio::test]
async fn test_concurrent_metric_updates() {
    let collector = Arc::new(MetricsCollector::new());

    // Spawn multiple tasks updating metrics concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let collector_clone = Arc::clone(&collector);
        let handle = tokio::spawn(async move {
            collector_clone.record_request((i * 10) as f64).await;
            if i % 2 == 0 {
                collector_clone.record_cache_hit().await;
            } else {
                collector_clone.record_cache_miss().await;
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 10);
    assert_eq!(metrics.cache_hits, 5);
    assert_eq!(metrics.cache_misses, 5);
    assert!((metrics.cache_hit_rate - 50.0).abs() < 0.1);
}

#[tokio::test]
async fn test_latency_samples_limit() {
    let collector = MetricsCollector::new();

    // Add more than 1000 samples
    for i in 0..1500 {
        collector.record_request((i % 100) as f64).await;
    }

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 1500);

    // Latency percentiles should still be calculated correctly
    assert!(metrics.avg_latency_ms >= 0.0);
    assert!(metrics.p99_latency_ms >= metrics.p95_latency_ms);
}

#[tokio::test]
async fn test_uptime_tracking() {
    let collector = MetricsCollector::new();

    // Initial uptime should be near 0
    let metrics1 = collector.get_metrics().await;
    assert_eq!(metrics1.uptime_seconds, 0);

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Update and check uptime increased
    collector.update_system_metrics().await;
    let metrics2 = collector.get_metrics().await;
    assert!(metrics2.uptime_seconds >= 2);
}

#[tokio::test]
async fn test_timestamp_updates() {
    let collector = MetricsCollector::new();

    let metrics1 = collector.get_metrics().await;
    let timestamp1 = metrics1.timestamp;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    // Update system metrics (updates timestamp)
    collector.update_system_metrics().await;
    let metrics2 = collector.get_metrics().await;
    let timestamp2 = metrics2.timestamp;

    // Timestamp should have increased
    assert!(timestamp2 > timestamp1);
}

#[tokio::test]
async fn test_metrics_reset_behavior() {
    let collector = MetricsCollector::new();

    // Record some metrics
    collector.record_request(50.0).await;
    collector.record_cache_hit().await;

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 1);
    assert_eq!(metrics.cache_hits, 1);

    // Metrics should persist (not reset)
    let metrics2 = collector.get_metrics().await;
    assert_eq!(metrics2.requests_total, 1);
    assert_eq!(metrics2.cache_hits, 1);
}

#[tokio::test]
async fn test_high_load_simulation() {
    let collector = Arc::new(MetricsCollector::new());

    // Simulate high load (100 concurrent requests)
    let mut handles = vec![];

    for i in 0..100 {
        let collector_clone = Arc::clone(&collector);
        let handle = tokio::spawn(async move {
            let latency = (i % 50) as f64 + 10.0;
            collector_clone.record_request(latency).await;

            if i % 3 == 0 {
                collector_clone.record_cache_hit().await;
            } else {
                collector_clone.record_cache_miss().await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 100);

    // Cache hit rate should be around 33% (every 3rd is a hit)
    let expected_hits = 34; // ceil(100/3)
    let expected_misses = 66;
    assert!(metrics.cache_hits >= expected_hits - 1 && metrics.cache_hits <= expected_hits + 1);
    assert!(
        metrics.cache_misses >= expected_misses - 1 && metrics.cache_misses <= expected_misses + 1
    );

    // Latencies should be valid
    assert!(metrics.avg_latency_ms > 0.0);
    assert!(metrics.p99_latency_ms >= metrics.p95_latency_ms);
}

#[tokio::test]
async fn test_memory_unit_conversion() {
    let metrics = NodeMetrics {
        memory_used_mb: 1024, // 1GB in MB
        cache_memory_mb: 128, // 128MB
        ..Default::default()
    };

    let prometheus = metrics.to_prometheus_format();

    // Should convert to bytes
    assert!(prometheus.contains("aegis_memory_used_bytes 1073741824")); // 1024 * 1024 * 1024
    assert!(prometheus.contains("aegis_cache_memory_bytes 134217728")); // 128 * 1024 * 1024
}

#[tokio::test]
async fn test_prometheus_counter_vs_gauge() {
    let metrics = NodeMetrics::default();
    let prometheus = metrics.to_prometheus_format();

    // Verify counter types
    assert!(prometheus.contains("# TYPE aegis_requests_total counter"));
    assert!(prometheus.contains("# TYPE aegis_cache_hits_total counter"));
    assert!(prometheus.contains("# TYPE aegis_uptime_seconds counter"));

    // Verify gauge types
    assert!(prometheus.contains("# TYPE aegis_cpu_usage_percent gauge"));
    assert!(prometheus.contains("# TYPE aegis_memory_percent gauge"));
    assert!(prometheus.contains("# TYPE aegis_cache_hit_rate gauge"));
    assert!(prometheus.contains("# TYPE aegis_active_connections gauge"));
}

#[tokio::test]
async fn test_latency_edge_cases() {
    let collector = MetricsCollector::new();

    // Single request
    collector.record_request(42.0).await;

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 1);
    assert_eq!(metrics.avg_latency_ms, 42.0);
    assert_eq!(metrics.p50_latency_ms, 42.0);
    assert_eq!(metrics.p95_latency_ms, 42.0);
    assert_eq!(metrics.p99_latency_ms, 42.0);
}

#[tokio::test]
async fn test_cache_hit_rate_updates_correctly() {
    let collector = MetricsCollector::new();

    // Start with 50% hit rate
    collector.record_cache_hit().await;
    collector.record_cache_miss().await;

    let metrics1 = collector.get_metrics().await;
    assert!((metrics1.cache_hit_rate - 50.0).abs() < 0.1);

    // Add more hits to increase rate
    for _ in 0..8 {
        collector.record_cache_hit().await;
    }

    let metrics2 = collector.get_metrics().await;
    // Now should be 9 hits, 1 miss = 90%
    assert!((metrics2.cache_hit_rate - 90.0).abs() < 0.1);
}

#[tokio::test]
async fn test_multiple_collectors_independent() {
    let collector1 = Arc::new(MetricsCollector::new());
    let collector2 = Arc::new(MetricsCollector::new());

    collector1.record_request(10.0).await;
    collector1.record_request(20.0).await;

    collector2.record_request(30.0).await;

    let metrics1 = collector1.get_metrics().await;
    let metrics2 = collector2.get_metrics().await;

    assert_eq!(metrics1.requests_total, 2);
    assert_eq!(metrics2.requests_total, 1);
}

#[tokio::test]
async fn test_rps_calculation() {
    let collector = MetricsCollector::new();

    // Record requests
    for _ in 0..10 {
        collector.record_request(5.0).await;
    }

    // Wait 1 second
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    collector.calculate_rps().await;
    let metrics = collector.get_metrics().await;

    // Should have RPS calculated
    assert!(metrics.requests_per_second > 0.0);
    // Over 1 second with 10 requests, should be around 10 RPS
    assert!(metrics.requests_per_second <= 10.0);
}

#[tokio::test]
async fn test_json_serialization() {
    let metrics = NodeMetrics {
        cpu_usage_percent: 50.5,
        memory_used_mb: 2048,
        memory_total_mb: 8192,
        memory_percent: 25.0,
        active_connections: 10,
        requests_total: 1000,
        cache_hits: 800,
        cache_misses: 200,
        cache_hit_rate: 80.0,
        proxy_status: "running".to_string(),
        cache_status: "connected".to_string(),
        uptime_seconds: 3600,
        ..Default::default()
    };

    // Should serialize to JSON
    let json = serde_json::to_string(&metrics).unwrap();
    assert!(json.contains("cpu_usage_percent"));
    assert!(json.contains("50.5"));
    assert!(json.contains("running"));
}

#[tokio::test]
async fn test_json_deserialization() {
    let json = r#"{
        "cpu_usage_percent": 25.5,
        "memory_used_mb": 1024,
        "memory_total_mb": 8192,
        "memory_percent": 12.5,
        "active_connections": 5,
        "requests_total": 100,
        "requests_per_second": 2.5,
        "avg_latency_ms": 15.0,
        "p50_latency_ms": 12.0,
        "p95_latency_ms": 30.0,
        "p99_latency_ms": 50.0,
        "cache_hit_rate": 85.0,
        "cache_hits": 85,
        "cache_misses": 15,
        "cache_memory_mb": 128,
        "proxy_status": "running",
        "cache_status": "connected",
        "uptime_seconds": 7200,
        "timestamp": 1700491530
    }"#;

    let metrics: NodeMetrics = serde_json::from_str(json).unwrap();
    assert_eq!(metrics.cpu_usage_percent, 25.5);
    assert_eq!(metrics.memory_used_mb, 1024);
    assert_eq!(metrics.requests_total, 100);
    assert_eq!(metrics.cache_hit_rate, 85.0);
    assert_eq!(metrics.proxy_status, "running");
}

#[tokio::test]
async fn test_background_update_simulation() {
    let collector = Arc::new(MetricsCollector::new());

    // Simulate background task
    let collector_clone = Arc::clone(&collector);
    let handle = tokio::spawn(async move {
        for _ in 0..3 {
            collector_clone.update_system_metrics().await;
            collector_clone.calculate_rps().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Simulate requests while background task runs
    for i in 0..5 {
        collector.record_request((i * 10) as f64).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    handle.await.unwrap();

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 5);
    assert!(metrics.uptime_seconds >= 0);
}

#[tokio::test]
async fn test_cache_memory_tracking() {
    let collector = MetricsCollector::new();

    // Set various cache memory values
    collector.set_cache_memory(0).await;
    let m1 = collector.get_metrics().await;
    assert_eq!(m1.cache_memory_mb, 0);

    collector.set_cache_memory(256).await;
    let m2 = collector.get_metrics().await;
    assert_eq!(m2.cache_memory_mb, 256);

    collector.set_cache_memory(1024).await;
    let m3 = collector.get_metrics().await;
    assert_eq!(m3.cache_memory_mb, 1024);
}

#[tokio::test]
async fn test_metrics_collector_clone_safety() {
    let collector = Arc::new(MetricsCollector::new());

    let c1 = Arc::clone(&collector);
    let c2 = Arc::clone(&collector);

    c1.record_request(10.0).await;
    c2.record_request(20.0).await;

    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 2);
}
