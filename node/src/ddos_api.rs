/// DDoS Protection HTTP API
///
/// Provides REST API endpoints for DDoS protection management:
/// - Policy CRUD operations
/// - Blocklist/allowlist management
/// - Rate limit configuration
/// - Real-time statistics and SSE streaming
///
/// Base path: /aegis/ddos/api

use crate::ddos_manager::DDoSManager;
use crate::ddos_policy::{
    AllowlistEntry, BlockSource, BlocklistEntry, DDoSPolicy, DDoSPolicyUpdate,
};
use crate::ddos_stats::{AttackEvent, GlobalStats, SseEvent};

use anyhow::Result;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

// =============================================================================
// SECURITY: Request body size limits
// =============================================================================

/// Maximum allowed request body size (64KB for API requests)
/// Prevents memory exhaustion from maliciously large request bodies
const MAX_REQUEST_BODY_SIZE: usize = 64 * 1024;

/// Read request body with size limit
async fn read_body_limited(body: Body) -> Result<Vec<u8>, String> {
    use futures::StreamExt;

    let mut total_size = 0usize;
    let mut result = Vec::new();

    let mut stream = body;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Error reading body: {}", e))?;
        total_size += chunk.len();
        if total_size > MAX_REQUEST_BODY_SIZE {
            return Err(format!(
                "Request body size exceeds maximum allowed {} bytes",
                MAX_REQUEST_BODY_SIZE
            ));
        }
        result.extend_from_slice(&chunk);
    }

    Ok(result)
}

// =============================================================================
// API RESPONSE
// =============================================================================

/// Standard API response format
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ApiResponse {
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: "Success".to_string(),
            data: Some(data),
        }
    }

    pub fn success_message(message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: None,
        }
    }

    pub fn error(message: &str) -> Self {
        Self {
            success: false,
            message: message.to_string(),
            data: None,
        }
    }
}

// =============================================================================
// REQUEST BODIES
// =============================================================================

/// Request to add IP to blocklist
#[derive(Debug, Deserialize)]
pub struct BlockIpRequest {
    pub ip: String,
    pub reason: String,
    #[serde(default = "default_block_duration")]
    pub duration_secs: u64,
    #[serde(default)]
    pub source: Option<BlockSource>,
}

fn default_block_duration() -> u64 { 300 }

/// Request to add IP to allowlist
#[derive(Debug, Deserialize)]
pub struct AllowIpRequest {
    pub ip: String,
    pub description: String,
}

// =============================================================================
// PAGINATION
// =============================================================================

/// Pagination parameters
#[derive(Debug, Default)]
struct Pagination {
    offset: usize,
    limit: usize,
}

impl Pagination {
    fn from_query(query: &str) -> Self {
        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        Self {
            offset: params
                .get("offset")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            limit: params
                .get("limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(100)
                .min(1000), // Max 1000 items
        }
    }
}

// =============================================================================
// API HANDLER
// =============================================================================

/// DDoS API handler
pub struct DDoSApi {
    manager: Arc<DDoSManager>,
}

impl DDoSApi {
    /// Create a new DDoS API handler
    pub fn new(manager: Arc<DDoSManager>) -> Self {
        Self { manager }
    }

    /// Handle HTTP requests
    pub async fn handle_request(&self, req: Request<Body>) -> Result<Response<Body>> {
        // Clone path and query to avoid borrow issues when consuming req
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        let method = req.method().clone();

        debug!("DDoS API request: {} {}", method, path);

        // Route requests
        match (method, path.as_str()) {
            // Health check
            (Method::GET, "/aegis/ddos/api/health") => self.handle_health().await,

            // Policies
            (Method::GET, p) if p.starts_with("/aegis/ddos/api/policy/") => {
                let domain = p.trim_start_matches("/aegis/ddos/api/policy/");
                self.handle_get_policy(domain).await
            }
            (Method::POST, p) if p.starts_with("/aegis/ddos/api/policy/") => {
                let domain = p.trim_start_matches("/aegis/ddos/api/policy/").to_string();
                self.handle_set_policy(&domain, req).await
            }
            (Method::PATCH, p) if p.starts_with("/aegis/ddos/api/policy/") => {
                let domain = p.trim_start_matches("/aegis/ddos/api/policy/").to_string();
                self.handle_update_policy(&domain, req).await
            }
            (Method::DELETE, p) if p.starts_with("/aegis/ddos/api/policy/") => {
                let domain = p.trim_start_matches("/aegis/ddos/api/policy/");
                self.handle_delete_policy(domain).await
            }
            (Method::GET, "/aegis/ddos/api/policies") => self.handle_list_policies().await,

            // Blocklist
            (Method::GET, "/aegis/ddos/api/blocklist") => {
                self.handle_get_blocklist(&query).await
            }
            (Method::POST, "/aegis/ddos/api/blocklist") => self.handle_add_to_blocklist(req).await,
            (Method::DELETE, p) if p.starts_with("/aegis/ddos/api/blocklist/") => {
                let ip = path.trim_start_matches("/aegis/ddos/api/blocklist/");
                self.handle_remove_from_blocklist(ip).await
            }

            // Allowlist
            (Method::GET, "/aegis/ddos/api/allowlist") => self.handle_get_allowlist().await,
            (Method::POST, "/aegis/ddos/api/allowlist") => self.handle_add_to_allowlist(req).await,
            (Method::DELETE, p) if p.starts_with("/aegis/ddos/api/allowlist/") => {
                let ip = p.trim_start_matches("/aegis/ddos/api/allowlist/");
                self.handle_remove_from_allowlist(ip).await
            }

            // Statistics
            (Method::GET, "/aegis/ddos/api/stats") => self.handle_get_stats().await,
            (Method::GET, "/aegis/ddos/api/stats/attacks") => {
                self.handle_get_attacks(&query).await
            }
            (Method::GET, "/aegis/ddos/api/stats/attackers") => {
                self.handle_get_top_attackers(&query).await
            }
            (Method::GET, "/aegis/ddos/api/stats/live") => self.handle_sse_stream().await,

            // Not found
            _ => {
                let response = ApiResponse::error("Endpoint not found");
                Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&response)?))?)
            }
        }
    }

    // =========================================================================
    // HEALTH
    // =========================================================================

    async fn handle_health(&self) -> Result<Response<Body>> {
        let response = ApiResponse::success(serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION"),
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&response)?))?)
    }

    // =========================================================================
    // POLICIES
    // =========================================================================

    async fn handle_get_policy(&self, domain: &str) -> Result<Response<Body>> {
        if domain.is_empty() {
            return Ok(self.error_response(StatusCode::BAD_REQUEST, "Domain is required"));
        }

        match self.manager.get_policy(domain).await {
            Some(policy) => {
                let response = ApiResponse::success(serde_json::to_value(&policy)?);
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string_pretty(&response)?))?)
            }
            None => Ok(self.error_response(StatusCode::NOT_FOUND, "Policy not found")),
        }
    }

    async fn handle_set_policy(&self, domain: &str, req: Request<Body>) -> Result<Response<Body>> {
        if domain.is_empty() {
            return Ok(self.error_response(StatusCode::BAD_REQUEST, "Domain is required"));
        }

        // Parse request body with size limit
        let body_bytes = match read_body_limited(req.into_body()).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(self.error_response(StatusCode::PAYLOAD_TOO_LARGE, &e));
            }
        };
        let mut policy: DDoSPolicy = match serde_json::from_slice(&body_bytes) {
            Ok(p) => p,
            Err(e) => {
                return Ok(self.error_response(
                    StatusCode::BAD_REQUEST,
                    &format!("Invalid JSON: {}", e),
                ));
            }
        };

        // Ensure domain matches path
        policy.domain = domain.to_string();

        // Set policy
        match self.manager.set_policy(policy).await {
            Ok(_) => {
                let response = ApiResponse::success_message("Policy created");
                Ok(Response::builder()
                    .status(StatusCode::CREATED)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&response)?))?)
            }
            Err(e) => Ok(self.error_response(StatusCode::BAD_REQUEST, &e.to_string())),
        }
    }

    async fn handle_update_policy(
        &self,
        domain: &str,
        req: Request<Body>,
    ) -> Result<Response<Body>> {
        if domain.is_empty() {
            return Ok(self.error_response(StatusCode::BAD_REQUEST, "Domain is required"));
        }

        // Parse request body with size limit
        let body_bytes = match read_body_limited(req.into_body()).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(self.error_response(StatusCode::PAYLOAD_TOO_LARGE, &e));
            }
        };
        let update: DDoSPolicyUpdate = match serde_json::from_slice(&body_bytes) {
            Ok(u) => u,
            Err(e) => {
                return Ok(self.error_response(
                    StatusCode::BAD_REQUEST,
                    &format!("Invalid JSON: {}", e),
                ));
            }
        };

        // Update policy
        match self.manager.update_policy(domain, update).await {
            Ok(policy) => {
                let response = ApiResponse::success(serde_json::to_value(&policy)?);
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string_pretty(&response)?))?)
            }
            Err(e) => Ok(self.error_response(StatusCode::BAD_REQUEST, &e.to_string())),
        }
    }

    async fn handle_delete_policy(&self, domain: &str) -> Result<Response<Body>> {
        if domain.is_empty() {
            return Ok(self.error_response(StatusCode::BAD_REQUEST, "Domain is required"));
        }

        if self.manager.delete_policy(domain).await {
            let response = ApiResponse::success_message("Policy deleted");
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        } else {
            Ok(self.error_response(StatusCode::NOT_FOUND, "Policy not found"))
        }
    }

    async fn handle_list_policies(&self) -> Result<Response<Body>> {
        let policies = self.manager.list_policies().await;
        let response = ApiResponse::success(serde_json::json!({
            "count": policies.len(),
            "policies": policies,
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string_pretty(&response)?))?)
    }

    // =========================================================================
    // BLOCKLIST
    // =========================================================================

    async fn handle_get_blocklist(&self, query: &str) -> Result<Response<Body>> {
        let pagination = Pagination::from_query(query);
        let entries = self
            .manager
            .get_blocklist(pagination.offset, pagination.limit)
            .await;
        let total = self.manager.blocklist_count().await;

        let response = ApiResponse::success(serde_json::json!({
            "total": total,
            "offset": pagination.offset,
            "limit": pagination.limit,
            "entries": entries,
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string_pretty(&response)?))?)
    }

    async fn handle_add_to_blocklist(&self, req: Request<Body>) -> Result<Response<Body>> {
        // Parse request body with size limit
        let body_bytes = match read_body_limited(req.into_body()).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(self.error_response(StatusCode::PAYLOAD_TOO_LARGE, &e));
            }
        };
        let request: BlockIpRequest = match serde_json::from_slice(&body_bytes) {
            Ok(r) => r,
            Err(e) => {
                return Ok(self.error_response(
                    StatusCode::BAD_REQUEST,
                    &format!("Invalid JSON: {}", e),
                ));
            }
        };

        let source = request.source.unwrap_or(BlockSource::Manual);

        match self
            .manager
            .block_ip(&request.ip, &request.reason, request.duration_secs, source)
            .await
        {
            Ok(entry) => {
                let response = ApiResponse::success(serde_json::to_value(&entry)?);
                Ok(Response::builder()
                    .status(StatusCode::CREATED)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&response)?))?)
            }
            Err(e) => Ok(self.error_response(StatusCode::BAD_REQUEST, &e.to_string())),
        }
    }

    async fn handle_remove_from_blocklist(&self, ip: &str) -> Result<Response<Body>> {
        // URL decode the IP (handles / in CIDR)
        let decoded_ip = urlencoding::decode(ip).unwrap_or_else(|_| ip.into());

        if self.manager.unblock_ip(&decoded_ip).await {
            let response = ApiResponse::success_message("IP unblocked");
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        } else {
            Ok(self.error_response(StatusCode::NOT_FOUND, "IP not found in blocklist"))
        }
    }

    // =========================================================================
    // ALLOWLIST
    // =========================================================================

    async fn handle_get_allowlist(&self) -> Result<Response<Body>> {
        let entries = self.manager.get_allowlist().await;

        let response = ApiResponse::success(serde_json::json!({
            "count": entries.len(),
            "entries": entries,
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string_pretty(&response)?))?)
    }

    async fn handle_add_to_allowlist(&self, req: Request<Body>) -> Result<Response<Body>> {
        // Parse request body with size limit
        let body_bytes = match read_body_limited(req.into_body()).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(self.error_response(StatusCode::PAYLOAD_TOO_LARGE, &e));
            }
        };
        let request: AllowIpRequest = match serde_json::from_slice(&body_bytes) {
            Ok(r) => r,
            Err(e) => {
                return Ok(self.error_response(
                    StatusCode::BAD_REQUEST,
                    &format!("Invalid JSON: {}", e),
                ));
            }
        };

        match self
            .manager
            .allow_ip(&request.ip, &request.description)
            .await
        {
            Ok(entry) => {
                let response = ApiResponse::success(serde_json::to_value(&entry)?);
                Ok(Response::builder()
                    .status(StatusCode::CREATED)
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&response)?))?)
            }
            Err(e) => Ok(self.error_response(StatusCode::BAD_REQUEST, &e.to_string())),
        }
    }

    async fn handle_remove_from_allowlist(&self, ip: &str) -> Result<Response<Body>> {
        let decoded_ip = urlencoding::decode(ip).unwrap_or_else(|_| ip.into());

        if self.manager.remove_from_allowlist(&decoded_ip).await {
            let response = ApiResponse::success_message("IP removed from allowlist");
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&response)?))?)
        } else {
            Ok(self.error_response(StatusCode::NOT_FOUND, "IP not found in allowlist"))
        }
    }

    // =========================================================================
    // STATISTICS
    // =========================================================================

    async fn handle_get_stats(&self) -> Result<Response<Body>> {
        let stats = self.manager.get_global_stats();

        let response = ApiResponse::success(serde_json::to_value(&stats)?);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string_pretty(&response)?))?)
    }

    async fn handle_get_attacks(&self, query: &str) -> Result<Response<Body>> {
        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        let limit = params
            .get("limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(100)
            .min(1000);

        let attacks = self.manager.get_recent_attacks(limit);

        let response = ApiResponse::success(serde_json::json!({
            "count": attacks.len(),
            "attacks": attacks,
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string_pretty(&response)?))?)
    }

    async fn handle_get_top_attackers(&self, query: &str) -> Result<Response<Body>> {
        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        let limit = params
            .get("limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(20)
            .min(100);

        let attackers = self.manager.get_top_attackers(limit).await;

        let response = ApiResponse::success(serde_json::json!({
            "count": attackers.len(),
            "attackers": attackers,
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string_pretty(&response)?))?)
    }

    async fn handle_sse_stream(&self) -> Result<Response<Body>> {
        let mut receiver = self.manager.subscribe_events();

        // Create SSE stream
        let stream = async_stream::stream! {
            // Send initial connection event
            yield Ok::<_, hyper::Error>(format!(
                "event: connected\ndata: {}\n\n",
                serde_json::json!({"status": "connected"})
            ));

            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        yield Ok(event.to_sse_string());
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("SSE stream lagged, skipped {} events", n);
                        yield Ok(format!(
                            "event: warning\ndata: {}\n\n",
                            serde_json::json!({"message": format!("Skipped {} events", n)})
                        ));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        };

        let body = Body::wrap_stream(stream);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)?)
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    fn error_response(&self, status: StatusCode, message: &str) -> Response<Body> {
        let response = ApiResponse::error(message);
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()),
            ))
            .unwrap()
    }
}

// =============================================================================
// SERVER
// =============================================================================

/// Run the DDoS API server
pub async fn run_ddos_api(addr: SocketAddr, api: Arc<DDoSApi>) -> Result<()> {
    info!("Starting DDoS API server on {}", addr);

    let make_svc = make_service_fn(move |_conn| {
        let api = api.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let api = api.clone();
                async move {
                    match api.handle_request(req).await {
                        Ok(response) => Ok::<_, Infallible>(response),
                        Err(e) => {
                            error!("DDoS API error: {}", e);
                            let response = ApiResponse::error("Internal server error");
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .header("Content-Type", "application/json")
                                .body(Body::from(
                                    serde_json::to_string(&response).unwrap_or_default(),
                                ))
                                .unwrap())
                        }
                    }
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    server.await?;

    Ok(())
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_api() -> Arc<DDoSApi> {
        let manager = Arc::new(DDoSManager::new());
        Arc::new(DDoSApi::new(manager))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/health")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_policy_crud() {
        let api = create_test_api();

        // Create policy
        let policy = serde_json::json!({
            "enabled": true,
            "syn_threshold": 100,
            "udp_threshold": 1000
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/ddos/api/policy/example.com")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&policy).unwrap()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Get policy
        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/policy/example.com")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Update policy
        let update = serde_json::json!({
            "enabled": false
        });

        let req = Request::builder()
            .method(Method::PATCH)
            .uri("/aegis/ddos/api/policy/example.com")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&update).unwrap()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Delete policy
        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/aegis/ddos/api/policy/example.com")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_blocklist_operations() {
        let api = create_test_api();

        // Add to blocklist
        let block_req = serde_json::json!({
            "ip": "192.168.1.100",
            "reason": "Test block",
            "duration_secs": 300
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/ddos/api/blocklist")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&block_req).unwrap()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Get blocklist
        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/blocklist")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Remove from blocklist
        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/aegis/ddos/api/blocklist/192.168.1.100")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_allowlist_operations() {
        let api = create_test_api();

        // Add to allowlist
        let allow_req = serde_json::json!({
            "ip": "10.0.0.1",
            "description": "Internal server"
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/ddos/api/allowlist")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&allow_req).unwrap()))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Get allowlist
        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/allowlist")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Remove from allowlist
        let req = Request::builder()
            .method(Method::DELETE)
            .uri("/aegis/ddos/api/allowlist/10.0.0.1")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_stats_endpoint() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/stats")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_attacks_endpoint() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/stats/attacks?limit=10")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_not_found() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/unknown")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_invalid_json() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/ddos/api/policy/example.com")
            .header("Content-Type", "application/json")
            .body(Body::from("invalid json"))
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_policy_not_found() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/policy/nonexistent.com")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_policies() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/policies")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_top_attackers() {
        let api = create_test_api();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/ddos/api/stats/attackers?limit=10")
            .body(Body::empty())
            .unwrap();

        let response = api.handle_request(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_pagination_parsing() {
        let pagination = Pagination::from_query("offset=10&limit=50");
        assert_eq!(pagination.offset, 10);
        assert_eq!(pagination.limit, 50);

        let pagination = Pagination::from_query("limit=5000"); // Over max
        assert_eq!(pagination.limit, 1000); // Clamped to 1000
    }
}
