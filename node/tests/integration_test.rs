use hyper::{Body, Client, Method, Request, StatusCode};

/// Integration tests for AEGIS HTTP server
/// These tests verify end-to-end functionality

const BASE_URL: &str = "http://127.0.0.1:8080";

/// Helper to create HTTP client
fn create_client() -> Client<hyper::client::HttpConnector> {
    Client::new()
}

#[tokio::test]
async fn test_server_health_check() {
    let client = create_client();

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("{}/health", BASE_URL))
        .body(Body::empty())
        .unwrap();

    // Note: This test requires the server to be running
    // In CI, we'll start the server in the background first
    match client.request(req).await {
        Ok(response) => {
            assert_eq!(response.status(), StatusCode::OK);

            let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
            let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

            let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();
            assert_eq!(json["status"], "healthy");
            assert_eq!(json["version"], "0.1.0");
        }
        Err(_) => {
            // Server not running - skip test
            eprintln!("Skipping integration test: server not running");
        }
    }
}

#[tokio::test]
async fn test_server_metrics_endpoint() {
    let client = create_client();

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("{}/metrics", BASE_URL))
        .body(Body::empty())
        .unwrap();

    match client.request(req).await {
        Ok(response) => {
            assert_eq!(response.status(), StatusCode::OK);

            let content_type = response.headers().get("content-type").unwrap();
            assert_eq!(content_type, "application/json");

            let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
            let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

            let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();
            assert!(json.get("requests_total").is_some());
            assert!(json.get("cache_hit_rate").is_some());
        }
        Err(_) => {
            eprintln!("Skipping integration test: server not running");
        }
    }
}

#[tokio::test]
async fn test_server_404_response() {
    let client = create_client();

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("{}/nonexistent", BASE_URL))
        .body(Body::empty())
        .unwrap();

    match client.request(req).await {
        Ok(response) => {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        Err(_) => {
            eprintln!("Skipping integration test: server not running");
        }
    }
}

#[tokio::test]
async fn test_concurrent_requests() {
    let client = create_client();
    let mut handles = vec![];

    // Send 10 concurrent requests
    for _ in 0..10 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let req = Request::builder()
                .method(Method::GET)
                .uri(format!("{}/health", BASE_URL))
                .body(Body::empty())
                .unwrap();

            client.request(req).await
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    for handle in handles {
        match handle.await {
            Ok(Ok(response)) => {
                assert_eq!(response.status(), StatusCode::OK);
            }
            Ok(Err(_)) => {
                eprintln!("Skipping integration test: server not running");
                return;
            }
            Err(e) => panic!("Task failed: {}", e),
        }
    }
}

#[tokio::test]
async fn test_server_performance_baseline() {
    let client = create_client();
    let start = std::time::Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("{}/health", BASE_URL))
            .body(Body::empty())
            .unwrap();

        match client.request(req).await {
            Ok(_) => {}
            Err(_) => {
                eprintln!("Skipping performance test: server not running");
                return;
            }
        }
    }

    let duration = start.elapsed();
    let avg_latency = duration.as_millis() / iterations;

    println!(
        "Average latency: {}ms for {} requests",
        avg_latency, iterations
    );

    // Sprint 1 baseline: should be < 10ms for local requests
    assert!(avg_latency < 10, "Latency too high: {}ms", avg_latency);
}
