// Sprint 20: Challenge System HTTP API
//
// Provides HTTP endpoints for JavaScript challenge verification.
// This runs alongside the main proxy to handle challenge-related requests.

use crate::challenge::{
    ChallengeManager, ChallengeSolution, ChallengeType, VerificationResult,
    CHALLENGE_TOKEN_COOKIE,
};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

/// Challenge API server state
pub struct ChallengeApi {
    challenge_manager: Arc<ChallengeManager>,
}

impl ChallengeApi {
    /// Create new Challenge API
    pub fn new(challenge_manager: Arc<ChallengeManager>) -> Self {
        Self { challenge_manager }
    }

    /// Get shared reference to challenge manager
    pub fn challenge_manager(&self) -> &Arc<ChallengeManager> {
        &self.challenge_manager
    }
}

/// Handle incoming HTTP requests
async fn handle_request(
    req: Request<Body>,
    api: Arc<ChallengeApi>,
) -> Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Extract client IP from headers or connection
    let client_ip = extract_client_ip(&req);

    let response = match (method, path.as_str()) {
        // Issue a new challenge
        (Method::GET, "/aegis/challenge/issue") => {
            let challenge_type = req.uri().query()
                .and_then(|q| {
                    for part in q.split('&') {
                        if let Some(value) = part.strip_prefix("type=") {
                            return match value {
                                "invisible" => Some(ChallengeType::Invisible),
                                "managed" => Some(ChallengeType::Managed),
                                "interactive" => Some(ChallengeType::Interactive),
                                _ => None,
                            };
                        }
                    }
                    None
                })
                .unwrap_or(ChallengeType::Managed);

            // SECURITY FIX (X2.1): Handle challenge creation errors
            let challenge = match api.challenge_manager.issue_challenge(&client_ip, challenge_type).await {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to issue challenge: {}", e);
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header("Content-Type", "application/json")
                        .body(Body::from(format!(r#"{{"error":"{}"}}"#, e)))
                        .unwrap());
                }
            };

            log::info!(
                "Challenge issued via API: id={}, type={:?}, ip={}",
                challenge.id,
                challenge_type,
                client_ip
            );

            let json = serde_json::to_string(&challenge).unwrap_or_else(|_| "{}".to_string());

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Cache-Control", "no-store")
                .body(Body::from(json))
                .unwrap()
        }

        // Get challenge page HTML
        (Method::GET, "/aegis/challenge/page") => {
            // SECURITY FIX (X2.1): Handle challenge creation errors
            let challenge = match api.challenge_manager.issue_challenge(&client_ip, ChallengeType::Managed).await {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to issue challenge: {}", e);
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header("Content-Type", "application/json")
                        .body(Body::from(format!(r#"{{"error":"{}"}}"#, e)))
                        .unwrap());
                }
            };
            let html = api.challenge_manager.generate_challenge_page(&challenge);

            log::info!(
                "Challenge page served: id={}, ip={}",
                challenge.id,
                client_ip
            );

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-store")
                .header("X-Aegis-Challenge", &challenge.id)
                .body(Body::from(html))
                .unwrap()
        }

        // Verify challenge solution
        (Method::POST, "/aegis/challenge/verify") => {
            match parse_challenge_solution(req).await {
                Ok(solution) => {
                    let result = api.challenge_manager.verify_solution(&solution, &client_ip).await;

                    log::info!(
                        "Challenge verification: id={}, success={}, score={}, ip={}",
                        solution.challenge_id,
                        result.success,
                        result.score,
                        client_ip
                    );

                    build_verification_response(result)
                }
                Err(e) => {
                    log::warn!("Failed to parse challenge solution: {}", e);

                    let result = VerificationResult {
                        success: false,
                        token: None,
                        error: Some(format!("Invalid request: {}", e)),
                        score: 0,
                        issues: vec!["parse_error".to_string()],
                    };

                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Body::from(serde_json::to_string(&result).unwrap()))
                        .unwrap()
                }
            }
        }

        // Verify existing token
        (Method::POST, "/aegis/challenge/verify-token") => {
            match parse_token_from_request(req).await {
                Ok(token_str) => {
                    match api.challenge_manager.verify_token(&token_str, &client_ip) {
                        Ok(token) => {
                            let response = serde_json::json!({
                                "valid": true,
                                "score": token.score,
                                "expires_at": token.exp,
                                "challenge_type": token.ctype,
                            });

                            Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "application/json")
                                .body(Body::from(response.to_string()))
                                .unwrap()
                        }
                        Err(e) => {
                            let response = serde_json::json!({
                                "valid": false,
                                "error": e.to_string(),
                            });

                            Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "application/json")
                                .body(Body::from(response.to_string()))
                                .unwrap()
                        }
                    }
                }
                Err(e) => {
                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Body::from(format!(r#"{{"error":"{}"}}"#, e)))
                        .unwrap()
                }
            }
        }

        // Get public key for external verification
        (Method::GET, "/aegis/challenge/public-key") => {
            let public_key = api.challenge_manager.public_key_hex();
            let response = serde_json::json!({
                "algorithm": "Ed25519",
                "public_key": public_key,
            });

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(response.to_string()))
                .unwrap()
        }

        // Health check
        (Method::GET, "/aegis/challenge/health") => {
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"healthy"}"#))
                .unwrap()
        }

        // 404 for unknown paths
        _ => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"error":"Not found"}"#))
                .unwrap()
        }
    };

    Ok(response)
}

/// Build verification response with Set-Cookie if successful
fn build_verification_response(result: VerificationResult) -> Response<Body> {
    let json = serde_json::to_string(&result).unwrap_or_else(|_| r#"{"success":false}"#.to_string());

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store");

    // Set cookie if verification succeeded
    if let Some(ref token) = result.token {
        let cookie = format!(
            "{}={}; Path=/; Max-Age=900; SameSite=Strict; HttpOnly",
            CHALLENGE_TOKEN_COOKIE,
            token
        );
        builder = builder.header("Set-Cookie", cookie);
    }

    builder.body(Body::from(json)).unwrap()
}

/// Parse challenge solution from request body
async fn parse_challenge_solution(req: Request<Body>) -> anyhow::Result<ChallengeSolution> {
    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
    let solution: ChallengeSolution = serde_json::from_slice(&body_bytes)?;
    Ok(solution)
}

/// Parse token from request body
async fn parse_token_from_request(req: Request<Body>) -> anyhow::Result<String> {
    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;

    #[derive(serde::Deserialize)]
    struct TokenRequest {
        token: String,
    }

    let parsed: TokenRequest = serde_json::from_slice(&body_bytes)?;
    Ok(parsed.token)
}

/// Extract client IP from request
fn extract_client_ip(req: &Request<Body>) -> String {
    // Try X-Forwarded-For first
    if let Some(xff) = req.headers().get("X-Forwarded-For") {
        if let Ok(xff_str) = xff.to_str() {
            if let Some(first_ip) = xff_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = req.headers().get("X-Real-IP") {
        if let Ok(ip) = real_ip.to_str() {
            return ip.to_string();
        }
    }

    // Default to unknown (in practice, you'd get this from the connection)
    "unknown".to_string()
}

/// Start the Challenge API HTTP server
pub async fn run_challenge_api(addr: SocketAddr, api: Arc<ChallengeApi>) -> anyhow::Result<()> {
    let make_svc = make_service_fn(move |_conn| {
        let api = api.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let api = api.clone();
                handle_request(req, api)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    log::info!("Challenge API listening on http://{}", addr);
    log::info!("Endpoints:");
    log::info!("  GET  /aegis/challenge/issue       - Issue new challenge");
    log::info!("  GET  /aegis/challenge/page        - Get challenge HTML page");
    log::info!("  POST /aegis/challenge/verify      - Verify solution");
    log::info!("  POST /aegis/challenge/verify-token - Verify existing token");
    log::info!("  GET  /aegis/challenge/public-key  - Get signing public key");
    log::info!("  GET  /aegis/challenge/health      - Health check");

    server.await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::{BrowserFingerprint, ScreenInfo};
    use sha2::Digest;

    #[tokio::test]
    async fn test_challenge_api_creation() {
        let manager = Arc::new(ChallengeManager::new());
        let api = ChallengeApi::new(manager.clone());

        assert!(Arc::ptr_eq(&api.challenge_manager, &manager));
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let manager = Arc::new(ChallengeManager::new());
        let api = Arc::new(ChallengeApi::new(manager));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/challenge/health")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, api).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("healthy"));
    }

    #[tokio::test]
    async fn test_public_key_endpoint() {
        let manager = Arc::new(ChallengeManager::new());
        let api = Arc::new(ChallengeApi::new(manager.clone()));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/challenge/public-key")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, api).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);

        // Should contain the public key
        assert!(body_str.contains("Ed25519"));
        assert!(body_str.contains("public_key"));

        // Public key should match manager's key
        let expected_key = manager.public_key_hex();
        assert!(body_str.contains(&expected_key));
    }

    #[tokio::test]
    async fn test_issue_challenge_endpoint() {
        let manager = Arc::new(ChallengeManager::new());
        let api = Arc::new(ChallengeApi::new(manager));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/challenge/issue")
            .header("X-Real-IP", "192.168.1.100")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, api).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);

        // Should contain challenge fields
        assert!(body_str.contains("id"));
        assert!(body_str.contains("pow_challenge"));
        assert!(body_str.contains("pow_difficulty"));
    }

    #[tokio::test]
    async fn test_challenge_page_endpoint() {
        let manager = Arc::new(ChallengeManager::new());
        let api = Arc::new(ChallengeApi::new(manager));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/aegis/challenge/page")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, api).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Check content type
        let content_type = response.headers().get("Content-Type").unwrap();
        assert!(content_type.to_str().unwrap().contains("text/html"));

        // Check for challenge header
        assert!(response.headers().contains_key("X-Aegis-Challenge"));

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);

        // Should contain HTML challenge page
        assert!(body_str.contains("<!DOCTYPE html>"));
        assert!(body_str.contains("AEGIS_CHALLENGE"));
        assert!(body_str.contains("solvePoW"));
    }

    #[tokio::test]
    async fn test_verify_endpoint_full_flow() {
        let manager = Arc::new(ChallengeManager::new());
        let api = Arc::new(ChallengeApi::new(manager.clone()));
        let client_ip = "192.168.1.50";

        // First, issue a challenge
        let challenge = manager.issue_challenge(client_ip, ChallengeType::Managed).await.unwrap();

        // Solve the PoW (find valid nonce)
        let mut nonce = 0u64;
        loop {
            let input = format!("{}{}", challenge.pow_challenge, nonce);
            let hash = sha2::Sha256::digest(input.as_bytes());

            // Check for required leading zeros
            let mut zero_bits = 0;
            for byte in hash.iter() {
                if *byte == 0 {
                    zero_bits += 8;
                } else {
                    zero_bits += byte.leading_zeros() as usize;
                    break;
                }
            }

            if zero_bits >= challenge.pow_difficulty as usize {
                break;
            }
            nonce += 1;

            if nonce > 10_000_000 {
                // Skip test if taking too long
                return;
            }
        }

        // Create solution
        let solution = ChallengeSolution {
            challenge_id: challenge.id.clone(),
            pow_nonce: nonce,
            fingerprint: BrowserFingerprint {
                canvas_hash: "test_canvas".to_string(),
                webgl_renderer: Some("Test Renderer".to_string()),
                webgl_vendor: Some("Test Vendor".to_string()),
                audio_hash: Some("test_audio".to_string()),
                screen: ScreenInfo {
                    width: 1920,
                    height: 1080,
                    color_depth: 24,
                    pixel_ratio: 1.0,
                },
                timezone_offset: -480,
                language: "en-US".to_string(),
                platform: "Test".to_string(),
                cpu_cores: Some(8),
                device_memory: Some(16.0),
                touch_support: false,
                webdriver_detected: false,
                plugins_count: 5,
            },
        };

        // Make verification request
        let req = Request::builder()
            .method(Method::POST)
            .uri("/aegis/challenge/verify")
            .header("Content-Type", "application/json")
            .header("X-Real-IP", client_ip)
            .body(Body::from(serde_json::to_string(&solution).unwrap()))
            .unwrap();

        let response = handle_request(req, api).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let result: VerificationResult = serde_json::from_slice(&body).unwrap();

        assert!(result.success, "Verification should succeed: {:?}", result.error);
        assert!(result.token.is_some(), "Should receive token");
        assert!(result.score >= 30, "Score should be above threshold");
    }

    #[tokio::test]
    async fn test_not_found() {
        let manager = Arc::new(ChallengeManager::new());
        let api = Arc::new(ChallengeApi::new(manager));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/unknown/path")
            .body(Body::empty())
            .unwrap();

        let response = handle_request(req, api).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
