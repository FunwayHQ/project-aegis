use async_trait::async_trait;
use pingora_core::prelude::*;
use pingora_proxy::{ProxyHttp, Session};
use std::sync::Arc;
use std::time::SystemTime;
use log::{info, warn};

/// AEGIS Pingora-based reverse proxy
pub struct AegisProxy {
    /// Origin server to proxy requests to
    origin: String,
}

impl AegisProxy {
    pub fn new(origin: String) -> Self {
        Self { origin }
    }
}

#[async_trait]
impl ProxyHttp for AegisProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    /// Determine where to send the request (upstream selection)
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // Parse origin URL to get host and port
        let peer = HttpPeer::new(&self.origin, true, String::new());
        Ok(Box::new(peer))
    }

    /// Modify request before sending to upstream
    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Add X-Forwarded-For header
        if let Some(client_addr) = session.client_addr() {
            upstream_request
                .insert_header("X-Forwarded-For", client_addr.to_string())
                .unwrap();
        }

        // Add X-Forwarded-Proto header
        let proto = if session.is_https() { "https" } else { "http" };
        upstream_request
            .insert_header("X-Forwarded-Proto", proto)
            .unwrap();

        Ok(())
    }

    /// Modify response before sending to client
    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        // Add AEGIS server identification header
        upstream_response
            .insert_header("X-Served-By", "AEGIS-Edge-Node")
            .unwrap();

        Ok(())
    }

    /// Log access after request completes
    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&pingora_core::Error>,
        ctx: &mut Self::CTX,
    ) {
        let method = session.req_header().method.as_str();
        let path = session.req_header().uri.path();
        let status = session
            .response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);

        // Calculate request duration
        let duration_ms = session
            .downstream_session
            .timing_digest()
            .map(|t| {
                t.get_timing("total")
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        // Access log format: timestamp method path status latency_ms
        info!(
            "{} {} {} {} {}ms",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            method,
            path,
            status,
            duration_ms
        );
    }

    /// Handle errors
    async fn fail_to_connect(
        &self,
        _session: &mut Session,
        _peer: &HttpPeer,
        _ctx: &mut Self::CTX,
        e: Box<pingora_core::Error>,
    ) -> Result<bool> {
        warn!("Failed to connect to upstream: {:?}", e);
        // Return 502 Bad Gateway
        Ok(false)
    }

    /// Handle upstream errors
    async fn error_while_proxy(
        &self,
        _peer: &HttpPeer,
        _session: &mut Session,
        e: Box<pingora_core::Error>,
        _ctx: &mut Self::CTX,
        _client_reused: bool,
    ) -> Result<bool> {
        warn!("Error while proxying: {:?}", e);
        Ok(false)
    }
}

/// Server configuration
#[derive(serde::Deserialize, Clone)]
pub struct ProxyConfig {
    /// Listen address for HTTP
    pub http_addr: String,
    /// Listen address for HTTPS
    pub https_addr: String,
    /// Origin server URL
    pub origin: String,
    /// Path to TLS certificate
    pub tls_cert_path: Option<String>,
    /// Path to TLS key
    pub tls_key_path: Option<String>,
    /// Number of worker threads
    pub threads: Option<usize>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            http_addr: "0.0.0.0:8080".to_string(),
            https_addr: "0.0.0.0:8443".to_string(),
            origin: "http://httpbin.org".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
            threads: Some(4),
        }
    }
}

/// Initialize and run the Pingora proxy server
pub fn run_proxy(config: ProxyConfig) -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Create server configuration
    let mut server = Server::new(None)?;
    server.bootstrap();

    // Create proxy instance
    let proxy = AegisProxy::new(config.origin.clone());
    let proxy_service = ProxyService::new(Arc::new(proxy), server.configuration.clone());

    // Add HTTP listener
    let mut http_service = pingora_proxy::http_proxy_service(
        &server.configuration,
        proxy_service.clone(),
    );
    http_service.add_tcp(&config.http_addr);

    info!("HTTP listener configured on {}", config.http_addr);

    // Add HTTPS listener if TLS is configured
    if let (Some(cert), Some(key)) = (&config.tls_cert_path, &config.tls_key_path) {
        let mut tls_settings = TlsSettings::intermediate(cert, key)?;
        tls_settings.enable_h2();

        let mut https_service = pingora_proxy::http_proxy_service(
            &server.configuration,
            proxy_service,
        );
        https_service.add_tls_with_settings(&config.https_addr, None, tls_settings)?;

        info!("HTTPS listener configured on {} with TLS", config.https_addr);
        server.add_service(https_service);
    }

    server.add_service(http_service);

    info!("AEGIS Pingora proxy starting...");
    info!("Proxying to origin: {}", config.origin);

    // Run the server
    server.run_forever();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_default() {
        let config = ProxyConfig::default();
        assert_eq!(config.http_addr, "0.0.0.0:8080");
        assert_eq!(config.https_addr, "0.0.0.0:8443");
        assert_eq!(config.origin, "http://httpbin.org");
        assert_eq!(config.threads, Some(4));
    }

    #[test]
    fn test_proxy_config_parsing() {
        let toml_str = r#"
            http_addr = "127.0.0.1:8080"
            https_addr = "127.0.0.1:8443"
            origin = "https://example.com"
            threads = 8
        "#;

        let config: ProxyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.http_addr, "127.0.0.1:8080");
        assert_eq!(config.origin, "https://example.com");
        assert_eq!(config.threads, Some(8));
    }

    #[test]
    fn test_aegis_proxy_creation() {
        let proxy = AegisProxy::new("http://example.com".to_string());
        assert_eq!(proxy.origin, "http://example.com");
    }
}
