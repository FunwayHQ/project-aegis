use crate::bot_management::{BotAction, BotManager};
use crate::cache::{generate_cache_key, CacheClient, CacheControl};
use crate::waf::{AegisWaf, WafAction};
use async_trait::async_trait;
use hyper::body::Bytes;
use pingora::prelude::*;
use pingora::proxy::{ProxyHttp, Session};
use std::time::Instant;

/// Proxy context - tracks request metadata
pub struct ProxyContext {
    pub start_time: Instant,
    pub cache_hit: bool,
    pub cache_key: Option<String>,
    pub cache_ttl: Option<u64>, // Custom TTL from Cache-Control
    pub waf_blocked: bool,       // Whether request was blocked by WAF
    pub bot_blocked: bool,       // Whether request was blocked by bot management
}

/// AEGIS Pingora-based reverse proxy
pub struct AegisProxy {
    /// Origin server to proxy requests to
    pub origin_addr: String,
    /// Cache client (optional)
    pub cache_client: Option<std::sync::Arc<tokio::sync::Mutex<CacheClient>>>,
    /// Cache TTL in seconds
    pub cache_ttl: u64,
    /// Whether caching is enabled
    pub caching_enabled: bool,
    /// Web Application Firewall
    pub waf: Option<AegisWaf>,
    /// Bot Management System (Sprint 9)
    pub bot_manager: Option<std::sync::Arc<BotManager>>,
}

impl AegisProxy {
    pub fn new(origin: String) -> Self {
        Self::new_with_cache(origin, None, 60, false)
    }

    pub fn new_with_cache(
        origin: String,
        cache_client: Option<std::sync::Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
    ) -> Self {
        Self::new_with_waf(origin, cache_client, cache_ttl, caching_enabled, None)
    }

    pub fn new_with_waf(
        origin: String,
        cache_client: Option<std::sync::Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
        waf: Option<AegisWaf>,
    ) -> Self {
        Self::new_with_bot_manager(origin, cache_client, cache_ttl, caching_enabled, waf, None)
    }

    pub fn new_with_bot_manager(
        origin: String,
        cache_client: Option<std::sync::Arc<tokio::sync::Mutex<CacheClient>>>,
        cache_ttl: u64,
        caching_enabled: bool,
        waf: Option<AegisWaf>,
        bot_manager: Option<std::sync::Arc<BotManager>>,
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
            waf,
            bot_manager,
        }
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
        }
    }

    /// Request filter - check bot management, WAF, and cache before proxying
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        // ============================================
        // PHASE 0: Bot Management (Sprint 9)
        // ============================================
        if let Some(bot_manager) = &self.bot_manager {
            // Extract User-Agent and IP
            let user_agent = session
                .req_header()
                .headers
                .get("User-Agent")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            // Get client IP (simplified - in production use X-Forwarded-For with validation)
            let ip = session
                .downstream_session
                .client_addr()
                .map(|addr| addr.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            // Analyze request
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

                            // Send 403 Forbidden response
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
                            // For PoC, just log the challenge
                            log::info!(
                                "BOT CHALLENGE: {:?} - Would issue JS challenge for UA: {}",
                                verdict,
                                user_agent
                            );
                            // In production, you would return a challenge page here
                            // For now, allow the request to continue
                        }
                        BotAction::Log => {
                            log::info!(
                                "BOT LOGGED: {:?} - User-Agent: {} (allowed)",
                                verdict,
                                user_agent
                            );
                        }
                        BotAction::Allow => {
                            // Continue normally
                        }
                    }
                }
                Err(e) => {
                    // Log error but don't block (fail open)
                    log::error!("Bot detection error: {} - allowing request", e);
                }
            }
        }

        // ============================================
        // PHASE 1: WAF Analysis (Security First!)
        // ============================================
        if let Some(waf) = &self.waf {
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

            // Analyze request (body analysis will happen in request_body_filter if needed)
            let matches = waf.analyze_request(method, uri, &headers, None);

            if !matches.is_empty() {
                let action = waf.determine_action(&matches);

                // Log all matches
                for rule_match in &matches {
                    log::warn!(
                        "WAF Rule {} triggered: {} (severity: {:?}, location: {}, value: {})",
                        rule_match.rule_id,
                        rule_match.rule_description,
                        rule_match.severity,
                        rule_match.location,
                        rule_match.matched_value
                    );
                }

                match action {
                    WafAction::Block => {
                        ctx.waf_blocked = true;
                        log::error!("WAF BLOCKED: {} {} - {} rule(s) triggered", method, uri, matches.len());

                        // Send 403 Forbidden response
                        let mut header = pingora::http::ResponseHeader::build(403, Some(3))?;
                        header.insert_header("Content-Type", "text/plain")?;
                        session.write_response_header(Box::new(header), true).await?;
                        session
                            .write_response_body(Some("403 Forbidden - Request blocked by WAF".into()), true)
                            .await?;

                        // Return true to skip upstream
                        return Ok(true);
                    }
                    WafAction::Log => {
                        log::warn!("WAF LOGGED: {} {} - {} rule(s) triggered (action: log)", method, uri, matches.len());
                        // Continue processing
                    }
                    WafAction::Allow => {
                        // Explicitly allowed, continue
                    }
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

    // Create proxy instance with cache
    let proxy = AegisProxy::new_with_cache(
        config.origin.clone(),
        cache_client,
        config.cache_ttl.unwrap_or(60),
        config.enable_caching.unwrap_or(false),
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
