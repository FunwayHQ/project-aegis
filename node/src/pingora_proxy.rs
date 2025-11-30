use crate::bot_management::{BotAction, BotManager};
use crate::cache::{generate_cache_key, CacheClient, CacheControl};
use crate::challenge::{
    ChallengeManager, ChallengeType,
    CHALLENGE_TOKEN_COOKIE, CHALLENGE_TOKEN_HEADER,
};
use crate::enhanced_bot_detection::EnhancedBotDetector;
use crate::ip_extraction::{extract_client_ip, IpExtractionConfig};
use crate::module_dispatcher::ModuleDispatcher;
use crate::route_config::RouteConfig;
use crate::tls_fingerprint::TlsFingerprint;
use crate::tls_intercept::FingerprintStore;
use crate::wasm_runtime::{WasmRuntime, WasmExecutionContext};
use async_trait::async_trait;
use hyper::body::Bytes;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use std::sync::Arc;
use std::time::Instant;

/// Proxy context - tracks request metadata
pub struct ProxyContext {
    pub start_time: Instant,
    pub cache_hit: bool,
    pub cache_key: Option<String>,
    pub cache_ttl: Option<u64>, // Custom TTL from Cache-Control
    pub waf_blocked: bool,       // Whether request was blocked by WAF
    pub bot_blocked: bool,       // Whether request was blocked by bot management
    pub request_body: Vec<u8>,   // Buffered request body for WAF inspection
    pub tls_fingerprint: Option<TlsFingerprint>, // Sprint 19: TLS fingerprint from intercept layer
    pub challenge_issued: bool,  // Sprint 20: Whether a JS challenge was issued
    pub challenge_verified: bool, // Sprint 20: Whether client has valid challenge token
}

/// AEGIS Pingora-based reverse proxy
pub struct AegisProxy {
    /// Origin server to proxy requests to
    pub origin_addr: String,
    /// Cache client (optional)
    pub cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
    /// Cache TTL in seconds
    pub cache_ttl: u64,
    /// Whether caching is enabled
    pub caching_enabled: bool,
    /// Sprint 15.5: Wasm Runtime for WAF and Edge Functions (unified dispatch)
    pub wasm_runtime: Option<Arc<WasmRuntime>>,
    /// Sprint 15.5: WAF module ID for Wasm execution (legacy - use route_config instead)
    pub waf_module_id: Option<String>,
    /// Bot Management System (Sprint 9)
    pub bot_manager: Option<Arc<BotManager>>,
    /// IP extraction configuration (Sprint 12.5)
    pub ip_extraction_config: IpExtractionConfig,
    /// Sprint 16: Route-based configuration for Wasm module dispatch
    pub route_config: Option<Arc<RouteConfig>>,
    /// Sprint 16: Module dispatcher for executing Wasm pipelines
    pub module_dispatcher: Option<Arc<ModuleDispatcher>>,
    /// Sprint 19: Shared fingerprint store from TLS intercept layer
    pub fingerprint_store: Option<Arc<FingerprintStore>>,
    /// Sprint 19: Enhanced bot detector with TLS fingerprinting
    pub enhanced_bot_detector: Option<Arc<EnhancedBotDetector>>,
    /// Sprint 20: JavaScript Challenge Manager
    pub challenge_manager: Option<Arc<ChallengeManager>>,
}

impl AegisProxy {
    pub fn new(origin: String) -> Self {
        Self::new_with_cache(origin, None, 60, false)
    }

    pub fn new_with_cache(
        origin: String,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
    ) -> Self {
        Self::new_with_wasm(origin, cache_client, cache_ttl, caching_enabled, None, None)
    }

    /// Sprint 15.5: Create proxy with Wasm runtime for WAF and edge functions
    pub fn new_with_wasm(
        origin: String,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
        wasm_runtime: Option<Arc<WasmRuntime>>,
        waf_module_id: Option<String>,
    ) -> Self {
        Self::new_with_bot_manager(origin, cache_client, cache_ttl, caching_enabled, wasm_runtime, waf_module_id, None)
    }

    pub fn new_with_bot_manager(
        origin: String,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
        wasm_runtime: Option<Arc<WasmRuntime>>,
        waf_module_id: Option<String>,
        bot_manager: Option<Arc<BotManager>>,
    ) -> Self {
        // Parse origin to get address with port for HttpPeer
        // Example: "http://httpbin.org" -> "httpbin.org:80"
        let is_https = origin.starts_with("https://");
        let mut origin_addr = origin.replace("http://", "").replace("https://", "");

        // Add default port if not specified
        // For IPv6 addresses like [::1], check for ] instead of just :
        let needs_port = if origin_addr.starts_with('[') {
            !origin_addr.contains("]:")
        } else {
            !origin_addr.contains(':')
        };

        if needs_port {
            if is_https {
                origin_addr.push_str(":443");
            } else {
                origin_addr.push_str(":80");
            }
        }

        Self {
            origin_addr,
            cache_client,
            cache_ttl,
            caching_enabled,
            wasm_runtime,
            waf_module_id,
            bot_manager,
            ip_extraction_config: IpExtractionConfig::default(),
            route_config: None,
            module_dispatcher: None,
            fingerprint_store: None,
            enhanced_bot_detector: None,
            challenge_manager: None,
        }
    }

    /// Sprint 16: Create proxy with route-based dispatch
    pub fn new_with_routes(
        origin: String,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
        wasm_runtime: Option<Arc<WasmRuntime>>,
        route_config: Option<Arc<RouteConfig>>,
        bot_manager: Option<Arc<BotManager>>,
    ) -> Self {
        // Parse origin address
        let is_https = origin.starts_with("https://");
        let mut origin_addr = origin.replace("http://", "").replace("https://", "");

        let needs_port = if origin_addr.starts_with('[') {
            !origin_addr.contains("]:")
        } else {
            !origin_addr.contains(':')
        };

        if needs_port {
            if is_https {
                origin_addr.push_str(":443");
            } else {
                origin_addr.push_str(":80");
            }
        }

        // Create module dispatcher if we have both wasm_runtime and route_config
        let module_dispatcher = if let (Some(runtime), Some(config)) = (&wasm_runtime, &route_config) {
            let settings = config.settings.clone().unwrap_or_default();
            Some(Arc::new(ModuleDispatcher::new(
                Arc::clone(runtime),
                cache_client.clone(),
                settings,
            )))
        } else {
            None
        };

        Self {
            origin_addr,
            cache_client,
            cache_ttl,
            caching_enabled,
            wasm_runtime,
            waf_module_id: None, // Use route_config instead
            bot_manager,
            ip_extraction_config: IpExtractionConfig::default(),
            route_config,
            module_dispatcher,
            fingerprint_store: None,
            enhanced_bot_detector: None,
            challenge_manager: None,
        }
    }

    /// Sprint 19: Set fingerprint store for TLS fingerprint extraction
    pub fn with_fingerprint_store(mut self, store: Arc<FingerprintStore>) -> Self {
        self.fingerprint_store = Some(store);
        self
    }

    /// Sprint 19: Set enhanced bot detector for composite scoring
    pub fn with_enhanced_bot_detector(mut self, detector: Arc<EnhancedBotDetector>) -> Self {
        self.enhanced_bot_detector = Some(detector);
        self
    }

    /// Sprint 20: Set challenge manager for JavaScript challenges
    pub fn with_challenge_manager(mut self, manager: Arc<ChallengeManager>) -> Self {
        self.challenge_manager = Some(manager);
        self
    }

    /// Sprint 20: Handle challenge verification endpoint
    async fn handle_challenge_verification(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
        challenge_manager: &Arc<ChallengeManager>,
        client_ip: &str,
    ) -> Result<bool> {
        // Read request body (challenge solution)
        // Note: In a production implementation, we'd buffer the body properly
        // For now, we'll parse from available body data

        // Create a simple JSON response
        let response_json = match self.parse_and_verify_challenge(session, challenge_manager, client_ip).await {
            Ok(result) => {
                ctx.challenge_verified = result.success;
                serde_json::to_string(&result).unwrap_or_else(|_| {
                    r#"{"success":false,"error":"Internal error"}"#.to_string()
                })
            }
            Err(e) => {
                log::error!("Challenge verification error: {}", e);
                format!(r#"{{"success":false,"error":"{}"}}"#, e)
            }
        };

        // Send response
        let mut header = pingora::http::ResponseHeader::build(200, Some(4))?;
        header.insert_header("Content-Type", "application/json")?;
        header.insert_header("Cache-Control", "no-store")?;
        session.write_response_header(Box::new(header), false).await?;
        session
            .write_response_body(Some(Bytes::from(response_json)), true)
            .await?;

        Ok(true) // Request handled, skip upstream
    }

    /// Sprint 20: Parse challenge solution from request body and verify
    async fn parse_and_verify_challenge(
        &self,
        _session: &mut Session,
        _challenge_manager: &Arc<ChallengeManager>,
        client_ip: &str,
    ) -> anyhow::Result<crate::challenge::VerificationResult> {
        // For now, return a placeholder - body parsing in Pingora requires careful handling
        // In production, we'd use request_body_filter to buffer the body first
        //
        // The actual flow would be:
        // 1. Client POSTs JSON with challenge_id, pow_nonce, and fingerprint
        // 2. We parse the JSON into ChallengeSolution
        // 3. We call challenge_manager.verify_solution()
        // 4. Return the VerificationResult

        // Placeholder implementation - in real usage, parse from buffered body
        log::debug!("Challenge verification requested from {}", client_ip);

        // For testing, create a mock failed result
        // Real implementation would parse the POST body
        Ok(crate::challenge::VerificationResult {
            success: false,
            token: None,
            error: Some("Body parsing not implemented in proxy layer - use standalone endpoint".to_string()),
            score: 0,
            issues: vec!["proxy_body_parsing_todo".to_string()],
        })
    }

    /// Sprint 20: Issue challenge page response
    async fn issue_challenge_response(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
        challenge_manager: &Arc<ChallengeManager>,
        client_ip: &str,
    ) -> Result<bool> {
        // Issue a new challenge
        let challenge = challenge_manager.issue_challenge(client_ip, ChallengeType::Managed).await;

        // Generate challenge page HTML
        let challenge_page = challenge_manager.generate_challenge_page(&challenge);

        ctx.challenge_issued = true;

        log::info!(
            "Challenge issued: id={}, difficulty={}, expires_at={}, ip={}",
            challenge.id,
            challenge.pow_difficulty,
            challenge.expires_at,
            client_ip
        );

        // Send challenge page
        let mut header = pingora::http::ResponseHeader::build(403, Some(4))?;
        header.insert_header("Content-Type", "text/html; charset=utf-8")?;
        header.insert_header("Cache-Control", "no-store")?;
        header.insert_header("X-Aegis-Challenge", &challenge.id)?;
        session.write_response_header(Box::new(header), false).await?;
        session
            .write_response_body(Some(Bytes::from(challenge_page)), true)
            .await?;

        Ok(true) // Request handled, skip upstream
    }
}

#[async_trait]
impl ProxyHttp for AegisProxy {
    type CTX = ProxyContext;

    fn new_ctx(&self) -> Self::CTX {
        ProxyContext {
            start_time: Instant::now(),
            cache_hit: false,
            cache_key: None,
            cache_ttl: None,
            waf_blocked: false,
            bot_blocked: false,
            request_body: Vec::new(),
            tls_fingerprint: None,
            challenge_issued: false,
            challenge_verified: false,
        }
    }

    /// Request filter - check bot management, WAF, and cache before proxying
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        // Extract common request info
        let method = session.req_header().method.as_str();
        let uri = session.req_header().uri.path();

        // Extract client IP using Sprint 12.5 IP extraction
        let connection_ip = session
            .downstream_session
            .client_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Collect headers for IP extraction
        let headers: Vec<(String, String)> = session
            .req_header()
            .headers
            .iter()
            .filter_map(|(name, value)| {
                value.to_str().ok().map(|v| (name.to_string(), v.to_string()))
            })
            .collect();

        let ip_source = extract_client_ip(&self.ip_extraction_config, &connection_ip, &headers);
        let ip = ip_source.ip();

        // ============================================
        // PHASE -1: Sprint 20 Challenge Verification Endpoint
        // ============================================
        if let Some(challenge_manager) = &self.challenge_manager {
            // Handle challenge verification endpoint
            if method == "POST" && uri == "/aegis/challenge/verify" {
                return self.handle_challenge_verification(session, ctx, challenge_manager, &ip).await;
            }

            // Check for existing valid challenge token (cookie or header)
            let token = session
                .req_header()
                .headers
                .get(CHALLENGE_TOKEN_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .or_else(|| {
                    // Check cookie
                    session
                        .req_header()
                        .headers
                        .get("Cookie")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|cookies| {
                            for cookie in cookies.split(';') {
                                let cookie = cookie.trim();
                                if let Some(value) = cookie.strip_prefix(&format!("{}=", CHALLENGE_TOKEN_COOKIE)) {
                                    return Some(value.to_string());
                                }
                            }
                            None
                        })
                });

            if let Some(token_str) = token {
                match challenge_manager.verify_token(&token_str, &ip) {
                    Ok(token) => {
                        ctx.challenge_verified = true;
                        log::debug!(
                            "Valid challenge token: score={}, type={:?}",
                            token.score,
                            token.ctype
                        );
                    }
                    Err(e) => {
                        log::debug!("Invalid challenge token: {}", e);
                    }
                }
            }
        }

        // ============================================
        // PHASE 0: Bot Management (Sprint 9 + Sprint 19 TLS Fingerprinting)
        // ============================================

        // Extract User-Agent (needed for both legacy and enhanced bot detection)
        let user_agent = session
            .req_header()
            .headers
            .get("User-Agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Sprint 19: Try to get TLS fingerprint from intercept layer
        if let Some(store) = &self.fingerprint_store {
            if let Some(client_addr) = session.downstream_session.client_addr() {
                // Convert Pingora SocketAddr to std::net::SocketAddr
                if let Some(std_addr) = client_addr.as_inet() {
                    // Try to retrieve fingerprint captured by TLS intercept layer
                    ctx.tls_fingerprint = store.take(std_addr).await;
                    if ctx.tls_fingerprint.is_some() {
                        log::debug!(
                            "TLS fingerprint retrieved for {}: JA3={}",
                            std_addr,
                            ctx.tls_fingerprint.as_ref().unwrap().ja3
                        );
                    }
                }
            }
        }

        // Sprint 19: Use enhanced bot detector with TLS fingerprinting if available
        if let Some(enhanced_detector) = &self.enhanced_bot_detector {
            match enhanced_detector.analyze(user_agent, &ip, ctx.tls_fingerprint.as_ref()) {
                Ok((trust_score, verdict, action)) => {
                    log::debug!(
                        "Enhanced bot detection: score={}, verdict={:?}, action={:?}, UA={}, IP={}, TLS={}",
                        trust_score.score,
                        verdict,
                        action,
                        user_agent,
                        ip,
                        ctx.tls_fingerprint.as_ref().map(|f| f.ja3.as_str()).unwrap_or("none")
                    );

                    match action {
                        BotAction::Block => {
                            ctx.bot_blocked = true;
                            log::warn!(
                                "BOT BLOCKED (score={}): {:?} - UA: {}, Reasons: {:?}",
                                trust_score.score,
                                verdict,
                                user_agent,
                                trust_score.reasons
                            );

                            let mut header = pingora::http::ResponseHeader::build(403, Some(3))?;
                            header.insert_header("Content-Type", "text/plain")?;
                            session.write_response_header(Box::new(header), true).await?;
                            session
                                .write_response_body(
                                    Some("403 Forbidden - Bot detected".into()),
                                    true,
                                )
                                .await?;

                            return Ok(true);
                        }
                        BotAction::Challenge => {
                            // Sprint 20: Issue JS challenge if client doesn't have valid token
                            if !ctx.challenge_verified {
                                if let Some(challenge_manager) = &self.challenge_manager {
                                    log::info!(
                                        "BOT CHALLENGE (score={}): {:?} - Issuing JS challenge for UA: {}",
                                        trust_score.score,
                                        verdict,
                                        user_agent
                                    );
                                    return self.issue_challenge_response(session, ctx, challenge_manager, &ip).await;
                                }
                            }
                            log::info!(
                                "BOT CHALLENGE (score={}): {:?} - UA: {} (token verified, allowing)",
                                trust_score.score,
                                verdict,
                                user_agent
                            );
                        }
                        BotAction::Log => {
                            log::info!(
                                "BOT LOGGED (score={}): {:?} - UA: {} (allowed)",
                                trust_score.score,
                                verdict,
                                user_agent
                            );
                        }
                        BotAction::Allow => {}
                    }
                }
                Err(e) => {
                    log::error!("Enhanced bot detection error: {} - allowing request", e);
                }
            }
        }
        // Fallback to legacy bot manager if enhanced detector not available
        else if let Some(bot_manager) = &self.bot_manager {
            match bot_manager.analyze_request(user_agent, &ip) {
                Ok((verdict, action)) => {
                    log::debug!(
                        "Bot detection: {:?} verdict, {:?} action for UA: {} from IP: {}",
                        verdict,
                        action,
                        user_agent,
                        ip
                    );

                    match action {
                        BotAction::Block => {
                            ctx.bot_blocked = true;
                            log::warn!("BOT BLOCKED: {:?} - User-Agent: {}", verdict, user_agent);

                            let mut header = pingora::http::ResponseHeader::build(403, Some(3))?;
                            header.insert_header("Content-Type", "text/plain")?;
                            session.write_response_header(Box::new(header), true).await?;
                            session
                                .write_response_body(
                                    Some("403 Forbidden - Bot detected".into()),
                                    true,
                                )
                                .await?;

                            return Ok(true);
                        }
                        BotAction::Challenge => {
                            // Sprint 20: Issue JS challenge if client doesn't have valid token
                            if !ctx.challenge_verified {
                                if let Some(challenge_manager) = &self.challenge_manager {
                                    log::info!(
                                        "BOT CHALLENGE: {:?} - Issuing JS challenge for UA: {}",
                                        verdict,
                                        user_agent
                                    );
                                    return self.issue_challenge_response(session, ctx, challenge_manager, &ip).await;
                                }
                            }
                            log::info!(
                                "BOT CHALLENGE: {:?} - UA: {} (token verified, allowing)",
                                verdict,
                                user_agent
                            );
                        }
                        BotAction::Log => {
                            log::info!(
                                "BOT LOGGED: {:?} - User-Agent: {} (allowed)",
                                verdict,
                                user_agent
                            );
                        }
                        BotAction::Allow => {}
                    }
                }
                Err(e) => {
                    log::error!("Bot detection error: {} - allowing request", e);
                }
            }
        }

        // ============================================
        // PHASE 1: Sprint 16 Route-based Dispatch
        // ============================================
        if let (Some(route_config), Some(module_dispatcher)) = (&self.route_config, &self.module_dispatcher) {
            let method = session.req_header().method.as_str();
            let uri = session.req_header().uri.path();

            // Collect headers for route matching
            let headers: Vec<(String, String)> = session
                .req_header()
                .headers
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_string(),
                        v.to_str().unwrap_or("").to_string(),
                    )
                })
                .collect();

            // Find matching route
            if let Some(route) = route_config.find_matching_route(method, uri, &headers) {
                log::info!(
                    "Route matched: '{}' for {} {}",
                    route.name.as_deref().unwrap_or("unnamed"),
                    method,
                    uri
                );

                // Build execution context
                let execution_context = WasmExecutionContext {
                    request_method: method.to_string(),
                    request_uri: uri.to_string(),
                    request_headers: headers.clone(),
                    request_body: ctx.request_body.clone(),
                    response_status: None,
                    response_headers: Vec::new(),
                    response_body: Vec::new(),
                    terminate_early: false,
                };

                // Execute pipeline
                match module_dispatcher.execute_pipeline(route, execution_context) {
                    Ok(pipeline_result) => {
                        // Log execution times
                        for (module_id, duration_us) in &pipeline_result.execution_times {
                            log::debug!("Module '{}' executed in {}μs", module_id, duration_us);
                        }

                        if pipeline_result.blocked {
                            // Request was blocked by pipeline (WAF or edge function)
                            ctx.waf_blocked = true;
                            log::error!(
                                "Pipeline BLOCKED: {} {} (status: {})",
                                method,
                                uri,
                                pipeline_result.status_code
                            );

                            // Send blocking response
                            let mut header = pingora::http::ResponseHeader::build(
                                pipeline_result.status_code,
                                Some(3),
                            )?;
                            header.insert_header("Content-Type", "text/plain")?;
                            session.write_response_header(Box::new(header), true).await?;
                            session
                                .write_response_body(Some(Bytes::from(pipeline_result.response_body)), true)
                                .await?;

                            // Return true to skip upstream
                            return Ok(true);
                        } else {
                            log::info!(
                                "Pipeline PASSED: {} {} ({} modules executed)",
                                method,
                                uri,
                                pipeline_result.modules_executed
                            );

                            // Pipeline passed - update context with modifications
                            // In future sprints, we could apply response modifications here
                            // For now, just continue to cache/upstream
                        }
                    }
                    Err(e) => {
                        // Pipeline execution failed
                        log::error!(
                            "Pipeline execution error for {} {}: {} - allowing request (fail open)",
                            method, uri, e
                        );
                        // Continue to cache/upstream (fail open)
                    }
                }

                // Route matched and pipeline executed - skip legacy WAF logic
                // Continue to cache lookup phase
            } else {
                log::debug!("No route matched for {} {}, using legacy WAF", method, uri);
            }
        }

        // ============================================
        // PHASE 1.5: Legacy WAF Analysis (Sprint 15.5) - fallback if no route matched
        // ============================================
        if let (Some(wasm_runtime), Some(waf_module_id)) = (&self.wasm_runtime, &self.waf_module_id) {
            let method = session.req_header().method.as_str();
            let uri = session.req_header().uri.path();

            // Collect headers
            let headers: Vec<(String, String)> = session
                .req_header()
                .headers
                .iter()
                .filter_map(|(name, value)| {
                    value.to_str().ok().map(|v| (name.to_string(), v.to_string()))
                })
                .collect();

            // Build execution context for WAF
            let waf_context = WasmExecutionContext {
                request_method: method.to_string(),
                request_uri: uri.to_string(),
                request_headers: headers,
                request_body: Vec::new(), // Body will be added in body_filter if needed
                response_status: None,
                response_headers: Vec::new(),
                response_body: Vec::new(),
                terminate_early: false,
            };

            // Sprint 15.5: Execute WAF through generic Wasm dispatch
            match wasm_runtime.execute_waf(waf_module_id, &waf_context) {
                Ok(waf_result) => {
                    // Log matches
                    for waf_match in &waf_result.matches {
                        log::warn!(
                            "WAF Rule {} triggered: {} (severity: {}, location: {}, value: {})",
                            waf_match.rule_id,
                            waf_match.description,
                            waf_match.severity,
                            waf_match.location,
                            waf_match.matched_value
                        );
                    }

                    if waf_result.blocked {
                        ctx.waf_blocked = true;
                        log::error!("WAF BLOCKED: {} {} - {} rule(s) triggered", method, uri, waf_result.matches.len());

                        // Send 403 Forbidden response
                        let mut header = pingora::http::ResponseHeader::build(403, Some(3))?;
                        header.insert_header("Content-Type", "text/plain")?;
                        session.write_response_header(Box::new(header), true).await?;
                        session
                            .write_response_body(Some("403 Forbidden - Request blocked by WAF".into()), true)
                            .await?;

                        // Return true to skip upstream
                        return Ok(true);
                    } else if !waf_result.matches.is_empty() {
                        log::warn!("WAF LOGGED: {} {} - {} rule(s) triggered (action: log)", method, uri, waf_result.matches.len());
                    }
                }
                Err(e) => {
                    // Fail open: log error but don't block request
                    log::error!("WAF Wasm execution error: {} - allowing request (fail open)", e);
                }
            }
        }

        // ============================================
        // PHASE 2: Cache Lookup
        // ============================================

        // Only cache GET requests
        if session.req_header().method != "GET" {
            return Ok(false);
        }

        // Check if caching is enabled and we have a cache client
        if !self.caching_enabled || self.cache_client.is_none() {
            return Ok(false);
        }

        // Generate cache key
        let path = session.req_header().uri.path();
        let query = session.req_header().uri.query().unwrap_or("");
        let full_path = if query.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query)
        };
        let cache_key = generate_cache_key("GET", &full_path);
        ctx.cache_key = Some(cache_key.clone());

        // Try to get from cache
        if let Some(cache) = &self.cache_client {
            let mut cache_lock = cache.lock().await;
            if let Ok(Some(cached_response)) = cache_lock.get(&cache_key).await {
                // Cache hit! Serve from cache
                ctx.cache_hit = true;
                log::info!("CACHE HIT: {}", full_path);

                // Send cached response
                session
                    .write_response_header(
                        Box::new(pingora::http::ResponseHeader::build(200, Some(4))?),
                        true,
                    )
                    .await?;
                session
                    .write_response_body(Some(cached_response.into()), true)
                    .await?;

                // Return true to skip upstream
                return Ok(true);
            } else {
                log::debug!("CACHE MISS: {}", full_path);
            }
        }

        Ok(false)
    }

    /* Temporarily disabled - Sprint 12.5 body filter has lifetime issues
    /// Request body filter - buffer request body for WAF inspection
    fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<std::time::Duration>> {
        // Buffer request body chunks for WAF analysis
        if let Some(body_chunk) = body {
            ctx.request_body.extend_from_slice(body_chunk);
        }

        // If end of stream and we have a WAF, analyze the complete body
        if end_of_stream && !ctx.request_body.is_empty() {
            if let Some(waf) = &self.waf {
                // We need to re-check with the body now
                // Note: This is a simplified approach. In production, you'd want to
                // handle this more elegantly, possibly by deferring the entire WAF
                // check until after body is received.

                // Since we can't easily return a response from this callback,
                // we'll set a flag if body inspection finds threats.
                // The actual blocking will need to happen in request_filter
                // for headers/URI and here we just log for now.

                let matches = waf.analyze_request("", "", &[], Some(&ctx.request_body));

                if !matches.is_empty() {
                    let action = waf.determine_action(&matches);

                    for rule_match in &matches {
                        log::warn!(
                            "WAF Rule {} triggered in body: {} (severity: {:?}, value: {})",
                            rule_match.rule_id,
                            rule_match.rule_description,
                            rule_match.severity,
                            rule_match.matched_value
                        );
                    }

                    if matches!(action, WafAction::Block) {
                        ctx.waf_blocked = true;
                        log::error!("WAF BLOCKED: Request body contains malicious content - {} rule(s) triggered", matches.len());

                        // Note: Pingora's current design makes it difficult to send a response
                        // from request_body_filter. The proper way is to check the body in
                        // request_filter by buffering first. For now, we log the violation.
                        // In Sprint 13 (Wasm migration), this will be handled properly.
                    }
                }
            }
        }

        Ok(None)
    }
    */

    /// Determine where to send the request (upstream selection)
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // Create upstream peer
        let peer = Box::new(HttpPeer::new(
            self.origin_addr.clone(),
            false,          // TLS
            "".to_string(), // SNI
        ));
        Ok(peer)
    }

    /// Response filter - check Cache-Control headers and determine if we should cache
    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut pingora::http::ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Only cache if:
        // 1. Caching is enabled
        // 2. We have a cache key (GET request)
        // 3. This was a cache miss (don't re-cache hits)
        // 4. Response is successful (2xx status)
        if !self.caching_enabled || ctx.cache_key.is_none() || ctx.cache_hit {
            return Ok(());
        }

        let status = upstream_response.status.as_u16();
        if !(200..300).contains(&status) {
            return Ok(()); // Only cache successful responses
        }

        // Check Cache-Control header from upstream
        if let Some(cache_control_value) = upstream_response.headers.get("cache-control") {
            if let Ok(header_str) = cache_control_value.to_str() {
                let cache_control = CacheControl::parse(header_str);

                // Respect Cache-Control directives
                if !cache_control.should_cache() {
                    log::debug!("Cache-Control prevents caching: {}", header_str);
                    // Clear cache key to prevent caching in body filter
                    ctx.cache_key = None;
                    return Ok(());
                }

                // Use max-age from Cache-Control if present
                if let Some(ttl) = cache_control.effective_ttl(self.cache_ttl) {
                    // Store custom TTL in context for body filter
                    ctx.cache_ttl = Some(ttl);
                    log::debug!("Cache-Control allows caching with TTL: {}s", ttl);
                }
            }
        }

        // Cache will be stored during body processing
        Ok(())
    }

    /// Cache response body chunks as they arrive
    fn upstream_response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<std::time::Duration>> {
        // If we should cache this response and we have a complete body
        if end_of_stream && ctx.cache_key.is_some() && !ctx.cache_hit {
            if let Some(cache_key) = &ctx.cache_key {
                if let (Some(cache), Some(body_data)) = (&self.cache_client, body) {
                    // Clone data for async task
                    let cache_clone = cache.clone();
                    let cache_key_clone = cache_key.clone();
                    let body_bytes = body_data.clone();
                    let ttl = ctx.cache_ttl.unwrap_or(self.cache_ttl);

                    // Spawn async task to store in cache (non-blocking)
                    tokio::spawn(async move {
                        let mut cache_lock = cache_clone.lock().await;
                        if let Err(e) = cache_lock
                            .set(&cache_key_clone, &body_bytes, Some(ttl))
                            .await
                        {
                            log::warn!("Failed to cache response for {}: {}", cache_key_clone, e);
                        } else {
                            log::debug!("CACHE STORED: {} (TTL: {}s)", cache_key_clone, ttl);
                        }
                    });
                }
            }
        }

        Ok(None)
    }

    /// Access logging after request completes
    async fn logging(
        &self,
        session: &mut Session,
        e: Option<&pingora::Error>,
        ctx: &mut Self::CTX,
    ) {
        let method = session.req_header().method.as_str();
        let path = session.req_header().uri.path();
        let status = session
            .response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);

        // Get client IP
        let client_ip = session
            .client_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Calculate total request duration from context (start time)
        let duration_ms = ctx.start_time.elapsed().as_millis();

        // Get bytes sent
        let bytes_sent = session.body_bytes_sent();

        // Cache status indicator
        let cache_status = if ctx.cache_hit {
            "[CACHE HIT]"
        } else if ctx.cache_key.is_some() {
            "[CACHE MISS]"
        } else {
            ""
        };

        // Enhanced access log with cache status
        if let Some(error) = e {
            log::error!(
                "{} {} {} {} {}ms {} bytes {} - ERROR: {}",
                client_ip,
                method,
                path,
                status,
                duration_ms,
                bytes_sent,
                cache_status,
                error
            );
        } else {
            log::info!(
                "{} {} {} {} {}ms {} bytes {}",
                client_ip,
                method,
                path,
                status,
                duration_ms,
                bytes_sent,
                cache_status
            );
        }
    }
}

/// Server configuration
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct ProxyConfig {
    pub http_addr: String,
    pub https_addr: Option<String>,
    pub origin: String,
    pub threads: Option<usize>,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub cache_url: Option<String>,
    pub cache_ttl: Option<u64>,
    pub enable_caching: Option<bool>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            http_addr: "0.0.0.0:8080".to_string(),
            https_addr: Some("0.0.0.0:8443".to_string()),
            origin: "http://httpbin.org".to_string(),
            threads: Some(4),
            tls_cert_path: Some("cert.pem".to_string()),
            tls_key_path: Some("key.pem".to_string()),
            cache_url: Some("redis://127.0.0.1:6379".to_string()),
            cache_ttl: Some(60),
            enable_caching: Some(true),
        }
    }
}

/// Initialize and run the Pingora proxy server
pub fn run_proxy(config: ProxyConfig) -> Result<()> {
    env_logger::init();

    // Create server with optional config
    let mut server = Server::new(None).unwrap();
    server.bootstrap();

    // Initialize cache client if enabled
    let cache_client = if config.enable_caching.unwrap_or(false) {
        if let Some(cache_url) = &config.cache_url {
            match tokio::runtime::Runtime::new().unwrap().block_on(async {
                CacheClient::new(cache_url, config.cache_ttl.unwrap_or(60)).await
            }) {
                Ok(client) => {
                    log::info!(
                        "Cache connected: {} (TTL: {}s)",
                        cache_url,
                        config.cache_ttl.unwrap_or(60)
                    );
                    Some(std::sync::Arc::new(tokio::sync::Mutex::new(client)))
                }
                Err(e) => {
                    log::warn!("Failed to connect to cache: {}", e);
                    log::warn!("Caching disabled");
                    None
                }
            }
        } else {
            log::warn!("Caching enabled but no cache_url configured");
            None
        }
    } else {
        log::info!("Caching disabled");
        None
    };

    // Sprint 15.5: Create proxy instance with Wasm runtime disabled by default
    // To enable WAF via Wasm, load a WAF module and pass the runtime + module ID
    let proxy = AegisProxy::new_with_wasm(
        config.origin.clone(),
        cache_client,
        config.cache_ttl.unwrap_or(60),
        config.enable_caching.unwrap_or(false),
        None, // wasm_runtime (can be initialized separately)
        None, // waf_module_id (can be set after loading WAF module)
    );

    // Create HTTP proxy service
    let mut proxy_service = pingora::proxy::http_proxy_service(&server.configuration, proxy);

    // Add HTTP listener
    proxy_service.add_tcp(&config.http_addr);
    log::info!("HTTP listener on {}", config.http_addr);

    // Add HTTPS listener if configured
    if let (Some(https_addr), Some(cert_path), Some(key_path)) = (
        &config.https_addr,
        &config.tls_cert_path,
        &config.tls_key_path,
    ) {
        // Check if certificate files exist
        if std::path::Path::new(cert_path).exists() && std::path::Path::new(key_path).exists() {
            // Add TLS listener with cert/key paths
            // Pingora uses BoringSSL under the hood for TLS termination
            if let Err(e) = proxy_service.add_tls(https_addr, cert_path, key_path) {
                log::error!("Failed to add TLS listener: {}", e);
                log::warn!("HTTPS listener disabled");
            } else {
                log::info!(
                    "HTTPS listener on {} (TLS 1.2/1.3 enabled with BoringSSL)",
                    https_addr
                );
            }
        } else {
            log::warn!("TLS certificate not found at {} or {}", cert_path, key_path);
            log::warn!("HTTPS listener disabled. Generate cert with:");
            log::warn!("  openssl req -x509 -newkey rsa:4096 -keyout {} -out {} -days 365 -nodes -subj '/CN=localhost'", key_path, cert_path);
        }
    }

    log::info!("Proxying to origin: {}", config.origin);

    server.add_service(proxy_service);
    server.run_forever();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_default() {
        let config = ProxyConfig::default();
        assert_eq!(config.http_addr, "0.0.0.0:8080");
        assert_eq!(config.origin, "http://httpbin.org");
    }

    #[test]
    fn test_proxy_creation() {
        let proxy = AegisProxy::new("http://example.com:8080".to_string());
        assert_eq!(proxy.origin_addr, "example.com:8080");
    }
}
