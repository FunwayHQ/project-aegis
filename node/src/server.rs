use hyper::{Body, Method, Request, Response, StatusCode};
use std::convert::Infallible;
use std::sync::Arc;
use tracing::{info, warn};
use crate::metrics::MetricsCollector;

/// Server statistics (will be enhanced in future sprints)
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub requests_total: u64,
    pub cache_hit_rate: f64,
    pub avg_latency_ms: u64,
    pub active_connections: u64,
}

impl Default for ServerStats {
    fn default() -> Self {
        Self {
            requests_total: 0,
            cache_hit_rate: 0.0,
            avg_latency_ms: 0,
            active_connections: 0,
        }
    }
}

/// Handle incoming HTTP requests
pub async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let method = req.method();
    let path = req.uri().path();

    info!("{} {}", method, path);

    match (method, path) {
        (&Method::GET, "/") => handle_root(),
        (&Method::GET, "/health") => handle_health(),
        (&Method::GET, "/metrics") => handle_metrics(),
        _ => handle_not_found(method, path),
    }
}

/// Root endpoint - node information
fn handle_root() -> Result<Response<Body>, Infallible> {
    Ok(Response::new(Body::from(
        "AEGIS Decentralized Edge Network - Sprint 1 PoC\n\
         Node Status: Active\n\
         Version: 0.1.0\n"
    )))
}

/// Health check endpoint - JSON response
fn handle_health() -> Result<Response<Body>, Infallible> {
    let health_response = serde_json::json!({
        "status": "healthy",
        "version": "0.1.0",
        "sprint": 1,
        "uptime_seconds": 0, // TODO: Track actual uptime in future sprint
        "node_type": "proof-of-concept"
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(health_response.to_string()))
        .unwrap())
}

/// Metrics endpoint - performance data (enhanced for Sprint 5)
pub async fn handle_metrics_with_collector(
    collector: Arc<MetricsCollector>,
    format: &str,
) -> Result<Response<Body>, Infallible> {
    // Update system metrics before returning
    collector.update_system_metrics().await;
    collector.calculate_rps().await;

    let metrics = collector.get_metrics().await;

    match format {
        "prometheus" => {
            let prometheus_text = metrics.to_prometheus_format();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain; version=0.0.4")
                .body(Body::from(prometheus_text))
                .unwrap())
        }
        _ => {
            // Default: JSON format
            let json = serde_json::json!({
                "system": {
                    "cpu_usage_percent": metrics.cpu_usage_percent,
                    "memory_used_mb": metrics.memory_used_mb,
                    "memory_total_mb": metrics.memory_total_mb,
                    "memory_percent": metrics.memory_percent,
                },
                "network": {
                    "active_connections": metrics.active_connections,
                    "requests_total": metrics.requests_total,
                    "requests_per_second": metrics.requests_per_second,
                },
                "performance": {
                    "avg_latency_ms": metrics.avg_latency_ms,
                    "p50_latency_ms": metrics.p50_latency_ms,
                    "p95_latency_ms": metrics.p95_latency_ms,
                    "p99_latency_ms": metrics.p99_latency_ms,
                },
                "cache": {
                    "hit_rate": metrics.cache_hit_rate,
                    "hits": metrics.cache_hits,
                    "misses": metrics.cache_misses,
                    "memory_mb": metrics.cache_memory_mb,
                },
                "status": {
                    "proxy": metrics.proxy_status,
                    "cache": metrics.cache_status,
                    "uptime_seconds": metrics.uptime_seconds,
                },
                "timestamp": metrics.timestamp,
            });

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(json.to_string()))
                .unwrap())
        }
    }
}

/// Metrics endpoint - performance data (legacy, for backward compatibility)
fn handle_metrics() -> Result<Response<Body>, Infallible> {
    let stats = ServerStats::default();

    let metrics = serde_json::json!({
        "requests_total": stats.requests_total,
        "cache_hit_rate": stats.cache_hit_rate,
        "avg_latency_ms": stats.avg_latency_ms,
        "active_connections": stats.active_connections
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(metrics.to_string()))
        .unwrap())
}

/// 404 handler
fn handle_not_found(method: &Method, path: &str) -> Result<Response<Body>, Infallible> {
    warn!("404 Not Found: {} {}", method, path);
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("404 Not Found"))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Uri;

    /// Helper to create a test request
    fn create_request(method: Method, uri: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    /// Helper to read response body as string
    async fn body_to_string(body: Body) -> String {
        let bytes = hyper::body::to_bytes(body).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_root_endpoint() {
        let req = create_request(Method::GET, "/");
        let response = handle_request(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_to_string(response.into_body()).await;
        assert!(body.contains("AEGIS Decentralized Edge Network"));
        assert!(body.contains("Sprint 1 PoC"));
        assert!(body.contains("Active"));
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_json() {
        let req = create_request(Method::GET, "/health");
        let response = handle_request(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body = body_to_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["status"], "healthy");
        assert_eq!(json["version"], "0.1.0");
        assert_eq!(json["sprint"], 1);
        assert_eq!(json["node_type"], "proof-of-concept");
    }

    #[tokio::test]
    async fn test_metrics_endpoint_returns_json() {
        let req = create_request(Method::GET, "/metrics");
        let response = handle_request(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body = body_to_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert!(json.get("requests_total").is_some());
        assert!(json.get("cache_hit_rate").is_some());
        assert!(json.get("avg_latency_ms").is_some());
        assert!(json.get("active_connections").is_some());
    }

    #[tokio::test]
    async fn test_404_not_found() {
        let req = create_request(Method::GET, "/nonexistent");
        let response = handle_request(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = body_to_string(response.into_body()).await;
        assert_eq!(body, "404 Not Found");
    }

    #[tokio::test]
    async fn test_post_method_not_found() {
        let req = create_request(Method::POST, "/");
        let response = handle_request(req).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_multiple_sequential_requests() {
        // Simulate multiple requests
        for _ in 0..10 {
            let req = create_request(Method::GET, "/health");
            let response = handle_request(req).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_server_stats_default() {
        let stats = ServerStats::default();
        assert_eq!(stats.requests_total, 0);
        assert_eq!(stats.cache_hit_rate, 0.0);
        assert_eq!(stats.avg_latency_ms, 0);
        assert_eq!(stats.active_connections, 0);
    }
}
