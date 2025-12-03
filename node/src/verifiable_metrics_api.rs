/// HTTP API for verifiable metrics
///
/// Provides REST API endpoints for retrieving signed metric reports
use anyhow::Result;
use hyper::{Body, Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::verifiable_metrics::VerifiableMetricsAggregator;

/// Response for the /verifiable-metrics endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsApiResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl MetricsApiResponse {
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: "Success".to_string(),
            data: Some(data),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            message,
            data: None,
        }
    }
}

/// Handle HTTP requests for verifiable metrics
pub async fn handle_verifiable_metrics_request(
    req: Request<Body>,
    aggregator: Arc<VerifiableMetricsAggregator>,
) -> Result<Response<Body>> {
    match (req.method(), req.uri().path()) {
        // GET /verifiable-metrics - Get recent signed reports
        (&Method::GET, "/verifiable-metrics") | (&Method::GET, "/verifiable-metrics/") => {
            handle_get_recent_reports(aggregator).await
        }

        // GET /verifiable-metrics/latest - Get the most recent report
        (&Method::GET, "/verifiable-metrics/latest") => {
            handle_get_latest_report(aggregator).await
        }

        // GET /verifiable-metrics/public-key - Get the node's public key
        (&Method::GET, "/verifiable-metrics/public-key") => {
            handle_get_public_key(aggregator).await
        }

        // GET /verifiable-metrics/range?start=X&end=Y - Get reports in time range
        (&Method::GET, path) if path.starts_with("/verifiable-metrics/range") => {
            handle_get_range_reports(req, aggregator).await
        }

        // Not found
        _ => {
            let response = MetricsApiResponse::error("Endpoint not found".to_string());
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        }
    }
}

/// Handle GET /verifiable-metrics - Get recent signed reports
async fn handle_get_recent_reports(
    aggregator: Arc<VerifiableMetricsAggregator>,
) -> Result<Response<Body>> {
    debug!("Handling GET /verifiable-metrics");

    match aggregator.get_recent_reports(10) {
        Ok(reports) => {
            let data = serde_json::to_value(&reports)?;
            let response = MetricsApiResponse::success(data);

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string_pretty(&response)?))?)
        }
        Err(e) => {
            // SECURITY FIX (X5.2): Log error internally, return generic message
            warn!("Failed to get recent reports: {}", e);
            let response = MetricsApiResponse::error("Failed to retrieve metrics".to_string());

            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        }
    }
}

/// Handle GET /verifiable-metrics/latest - Get the most recent report
async fn handle_get_latest_report(
    aggregator: Arc<VerifiableMetricsAggregator>,
) -> Result<Response<Body>> {
    debug!("Handling GET /verifiable-metrics/latest");

    match aggregator.get_recent_reports(1) {
        Ok(reports) if !reports.is_empty() => {
            let data = serde_json::to_value(&reports[0])?;
            let response = MetricsApiResponse::success(data);

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string_pretty(&response)?))?)
        }
        Ok(_) => {
            let response = MetricsApiResponse::error("No reports available".to_string());

            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        }
        Err(e) => {
            // SECURITY FIX (X5.2): Log error internally, return generic message
            warn!("Failed to get latest report: {}", e);
            let response = MetricsApiResponse::error("Failed to retrieve metrics".to_string());

            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        }
    }
}

/// Handle GET /verifiable-metrics/public-key - Get the node's public key
async fn handle_get_public_key(
    aggregator: Arc<VerifiableMetricsAggregator>,
) -> Result<Response<Body>> {
    debug!("Handling GET /verifiable-metrics/public-key");

    let public_key = aggregator.public_key_hex();

    let data = serde_json::json!({
        "public_key": public_key,
        "algorithm": "Ed25519",
        "format": "hex"
    });

    let response = MetricsApiResponse::success(data);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string_pretty(&response)?))?)
}

/// Handle GET /verifiable-metrics/range?start=X&end=Y - Get reports in time range
async fn handle_get_range_reports(
    req: Request<Body>,
    aggregator: Arc<VerifiableMetricsAggregator>,
) -> Result<Response<Body>> {
    debug!("Handling GET /verifiable-metrics/range");

    // Parse query parameters
    let query = req.uri().query().unwrap_or("");
    let params: std::collections::HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();

    let start = params
        .get("start")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let end = params
        .get("end")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(u64::MAX);

    match aggregator.get_reports_in_range(start, end) {
        Ok(reports) => {
            let data = serde_json::json!({
                "start": start,
                "end": end,
                "count": reports.len(),
                "reports": reports
            });

            let response = MetricsApiResponse::success(data);

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string_pretty(&response)?))?)
        }
        Err(e) => {
            // SECURITY FIX (X5.2): Log error internally, return generic message
            warn!("Failed to get range reports: {}", e);
            let response = MetricsApiResponse::error("Failed to retrieve metrics".to_string());

            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricsCollector;
    use crate::verifiable_metrics::{MetricsSigner, VerifiableMetricsAggregator};
    use tempfile::NamedTempFile;

    async fn create_test_aggregator() -> (Arc<VerifiableMetricsAggregator>, tempfile::TempPath) {
        let collector = Arc::new(MetricsCollector::new());
        let signer = MetricsSigner::generate();
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.into_temp_path();

        let aggregator = VerifiableMetricsAggregator::new(
            collector,
            signer,
            temp_path.to_str().unwrap(),
            300,
        )
        .unwrap();

        (Arc::new(aggregator), temp_path)
    }

    #[tokio::test]
    async fn test_get_public_key() {
        let (aggregator, _temp_path) = create_test_aggregator().await;

        let req = Request::builder()
            .method(Method::GET)
            .uri("/verifiable-metrics/public-key")
            .body(Body::empty())
            .unwrap();

        let response = handle_verifiable_metrics_request(req, aggregator)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

        assert!(body_str.contains("public_key"));
        assert!(body_str.contains("Ed25519"));
    }

    #[tokio::test]
    async fn test_get_recent_reports_empty() {
        let (aggregator, _temp_path) = create_test_aggregator().await;

        let req = Request::builder()
            .method(Method::GET)
            .uri("/verifiable-metrics")
            .body(Body::empty())
            .unwrap();

        let response = handle_verifiable_metrics_request(req, aggregator)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_latest_report_with_data() {
        let (aggregator, _temp_path) = create_test_aggregator().await;

        // Generate a report first
        aggregator.aggregate_and_sign().await.unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/verifiable-metrics/latest")
            .body(Body::empty())
            .unwrap();

        let response = handle_verifiable_metrics_request(req, aggregator)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

        assert!(body_str.contains("signature"));
        assert!(body_str.contains("metrics"));
    }

    #[tokio::test]
    async fn test_not_found() {
        let (aggregator, _temp_path) = create_test_aggregator().await;

        let req = Request::builder()
            .method(Method::GET)
            .uri("/verifiable-metrics/invalid")
            .body(Body::empty())
            .unwrap();

        let response = handle_verifiable_metrics_request(req, aggregator)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
