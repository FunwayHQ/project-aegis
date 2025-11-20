use aegis_node::server::handle_metrics_with_collector;
use aegis_node::metrics::MetricsCollector;
use hyper::body;
use std::sync::Arc;

#[tokio::test]
async fn test_metrics_endpoint_json_format() {
    let collector = Arc::new(MetricsCollector::new());

    // Set some test data
    collector.set_proxy_status("running").await;
    collector.set_cache_status("connected").await;
    collector.record_request(25.5).await;
    collector.record_cache_hit().await;

    // Get metrics in JSON format
    let response = handle_metrics_with_collector(collector, "json").await.unwrap();

    assert_eq!(response.status(), hyper::StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body_bytes = body::to_bytes(response.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Parse JSON
    let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();

    // Verify structure
    assert!(json.get("system").is_some());
    assert!(json.get("network").is_some());
    assert!(json.get("performance").is_some());
    assert!(json.get("cache").is_some());
    assert!(json.get("status").is_some());
    assert!(json.get("timestamp").is_some());

    // Verify values
    assert_eq!(json["network"]["requests_total"], 1);
    assert_eq!(json["cache"]["hits"], 1);
    assert_eq!(json["status"]["proxy"], "running");
    assert_eq!(json["status"]["cache"], "connected");
}

#[tokio::test]
async fn test_metrics_endpoint_prometheus_format() {
    let collector = Arc::new(MetricsCollector::new());

    collector.set_proxy_status("running").await;
    collector.record_request(10.0).await;

    // Get metrics in Prometheus format
    let response = handle_metrics_with_collector(collector, "prometheus").await.unwrap();

    assert_eq!(response.status(), hyper::StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/plain; version=0.0.4"
    );

    let body_bytes = body::to_bytes(response.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Verify Prometheus format
    assert!(body_str.contains("# HELP"));
    assert!(body_str.contains("# TYPE"));
    assert!(body_str.contains("aegis_"));
    assert!(body_str.contains("gauge"));
    assert!(body_str.contains("counter"));
}

#[tokio::test]
async fn test_metrics_endpoint_updates_system_metrics() {
    let collector = Arc::new(MetricsCollector::new());

    // Get metrics (should trigger system update)
    let response = handle_metrics_with_collector(Arc::clone(&collector), "json").await.unwrap();
    assert_eq!(response.status(), hyper::StatusCode::OK);

    let metrics = collector.get_metrics().await;

    // System metrics should be populated
    assert!(metrics.memory_total_mb > 0);
}

#[tokio::test]
async fn test_metrics_endpoint_calculates_rps() {
    let collector = Arc::new(MetricsCollector::new());

    // Record some requests
    for _ in 0..10 {
        collector.record_request(15.0).await;
    }

    // Wait a bit for uptime
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Get metrics (should calculate RPS)
    let response = handle_metrics_with_collector(Arc::clone(&collector), "json").await.unwrap();
    assert_eq!(response.status(), hyper::StatusCode::OK);

    let body_bytes = body::to_bytes(response.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();

    assert!(json["network"]["requests_per_second"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_metrics_endpoint_json_structure() {
    let collector = Arc::new(MetricsCollector::new());

    collector.record_request(100.0).await;
    collector.record_cache_hit().await;
    collector.record_cache_miss().await;

    let response = handle_metrics_with_collector(collector, "json").await.unwrap();
    let body_bytes = body::to_bytes(response.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();

    // Verify all expected fields exist
    assert!(json["system"]["cpu_usage_percent"].is_number());
    assert!(json["system"]["memory_used_mb"].is_number());
    assert!(json["system"]["memory_total_mb"].is_number());
    assert!(json["system"]["memory_percent"].is_number());

    assert!(json["network"]["active_connections"].is_number());
    assert!(json["network"]["requests_total"].is_number());
    assert!(json["network"]["requests_per_second"].is_number());

    assert!(json["performance"]["avg_latency_ms"].is_number());
    assert!(json["performance"]["p50_latency_ms"].is_number());
    assert!(json["performance"]["p95_latency_ms"].is_number());
    assert!(json["performance"]["p99_latency_ms"].is_number());

    assert!(json["cache"]["hit_rate"].is_number());
    assert!(json["cache"]["hits"].is_number());
    assert!(json["cache"]["misses"].is_number());
    assert!(json["cache"]["memory_mb"].is_number());

    assert!(json["status"]["proxy"].is_string());
    assert!(json["status"]["cache"].is_string());
    assert!(json["status"]["uptime_seconds"].is_number());

    assert!(json["timestamp"].is_number());
}

#[tokio::test]
async fn test_prometheus_format_all_metrics_present() {
    let collector = Arc::new(MetricsCollector::new());

    collector.set_proxy_status("running").await;
    collector.set_cache_status("connected").await;

    let response = handle_metrics_with_collector(collector, "prometheus").await.unwrap();
    let body_bytes = body::to_bytes(response.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Verify all metrics are present
    let expected_metrics = vec![
        "aegis_cpu_usage_percent",
        "aegis_memory_used_bytes",
        "aegis_memory_percent",
        "aegis_active_connections",
        "aegis_requests_total",
        "aegis_requests_per_second",
        "aegis_latency_milliseconds",
        "aegis_latency_p50_milliseconds",
        "aegis_latency_p95_milliseconds",
        "aegis_latency_p99_milliseconds",
        "aegis_cache_hit_rate",
        "aegis_cache_hits_total",
        "aegis_cache_misses_total",
        "aegis_cache_memory_bytes",
        "aegis_uptime_seconds",
        "aegis_proxy_status",
        "aegis_cache_status",
    ];

    for metric in expected_metrics {
        assert!(
            body_str.contains(metric),
            "Missing metric: {}",
            metric
        );
    }
}

#[tokio::test]
async fn test_metrics_response_headers() {
    let collector = Arc::new(MetricsCollector::new());

    // JSON format
    let json_response = handle_metrics_with_collector(Arc::clone(&collector), "json").await.unwrap();
    assert_eq!(
        json_response.headers().get("content-type").unwrap(),
        "application/json"
    );

    // Prometheus format
    let prom_response = handle_metrics_with_collector(collector, "prometheus").await.unwrap();
    assert_eq!(
        prom_response.headers().get("content-type").unwrap(),
        "text/plain; version=0.0.4"
    );
}

#[tokio::test]
async fn test_high_traffic_metrics() {
    let collector = Arc::new(MetricsCollector::new());

    // Simulate high traffic
    for i in 0..1000 {
        collector.record_request((i % 100) as f64).await;

        if i % 2 == 0 {
            collector.record_cache_hit().await;
        } else {
            collector.record_cache_miss().await;
        }
    }

    let response = handle_metrics_with_collector(collector, "json").await.unwrap();
    assert_eq!(response.status(), hyper::StatusCode::OK);

    let body_bytes = body::to_bytes(response.into_body()).await.unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();

    assert_eq!(json["network"]["requests_total"], 1000);
    assert_eq!(json["cache"]["hits"], 500);
    assert_eq!(json["cache"]["misses"], 500);
    assert!((json["cache"]["hit_rate"].as_f64().unwrap() - 50.0).abs() < 0.1);
}

#[tokio::test]
async fn test_metrics_concurrent_access() {
    let collector = Arc::new(MetricsCollector::new());

    // Spawn multiple tasks accessing metrics
    let mut handles = vec![];

    for i in 0..10 {
        let collector_clone = Arc::clone(&collector);
        let handle = tokio::spawn(async move {
            collector_clone.record_request((i * 5) as f64).await;

            // Get metrics concurrently
            let response = handle_metrics_with_collector(collector_clone, "json").await.unwrap();
            assert_eq!(response.status(), hyper::StatusCode::OK);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Final check
    let metrics = collector.get_metrics().await;
    assert_eq!(metrics.requests_total, 10);
}
