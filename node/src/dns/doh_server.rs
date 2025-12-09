//! DNS over HTTPS (DoH) Server
//!
//! Sprint 30.5: Encrypted DNS Protocols
//!
//! Implements DNS over HTTPS (RFC 8484) for encrypted DNS queries over HTTP/2.
//! DoH provides privacy by encrypting DNS traffic and blending with HTTPS traffic.
//!
//! Supports both GET (query parameter) and POST (body) methods as per RFC 8484.

use std::collections::HashMap;
use std::convert::Infallible;
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

use crate::dns::DohConfig;

/// Content-Type for DNS messages (RFC 8484)
const DNS_MESSAGE_CONTENT_TYPE: &str = "application/dns-message";

/// Maximum DNS message size
const MAX_DNS_MESSAGE_SIZE: usize = 65535;

/// Default TTL for cache control header (in seconds)
const DEFAULT_CACHE_TTL: u32 = 300;

/// DNS message handler trait (same as DoT)
#[async_trait::async_trait]
pub trait DnsHandler: Send + Sync {
    /// Handle a DNS query and return a response
    async fn handle_query(&self, query: &[u8], client_ip: IpAddr) -> Result<Vec<u8>, DohError>;
}

/// DNS over HTTPS Server
pub struct DohServer {
    config: DohConfig,
    dns_handler: Arc<dyn DnsHandler>,
}

impl DohServer {
    /// Create a new DoH server
    pub fn new(config: DohConfig, dns_handler: Arc<dyn DnsHandler>) -> Self {
        Self { config, dns_handler }
    }

    /// Get the listen address
    pub fn addr(&self) -> SocketAddr {
        self.config.addr
    }

    /// Get the DoH path (usually /dns-query)
    pub fn path(&self) -> &str {
        &self.config.path
    }

    /// Start the DoH server with TLS
    pub async fn run(&self) -> Result<(), DohError> {
        let cert_path = self.config.cert_path.as_ref()
            .ok_or_else(|| DohError::ConfigError("cert_path is required".to_string()))?;
        let key_path = self.config.key_path.as_ref()
            .ok_or_else(|| DohError::ConfigError("key_path is required".to_string()))?;

        let tls_config = build_tls_config(cert_path, key_path)?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

        let listener = TcpListener::bind(self.config.addr).await
            .map_err(|e| DohError::ServerError(format!("Failed to bind: {}", e)))?;

        info!("DoH server listening on https://{}{}", self.config.addr, self.config.path);

        loop {
            let (stream, peer_addr) = listener.accept().await
                .map_err(|e| DohError::ServerError(format!("Accept error: {}", e)))?;

            let tls_acceptor = tls_acceptor.clone();
            let dns_handler = self.dns_handler.clone();
            let path = self.config.path.clone();
            let client_ip = peer_addr.ip();

            tokio::spawn(async move {
                match tls_acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        // Create hyper service for this connection
                        let service = service_fn(move |req| {
                            let dns_handler = dns_handler.clone();
                            let path = path.clone();
                            handle_request(req, dns_handler, path, client_ip)
                        });

                        // Use hyper's HTTP/1.1 server on the TLS stream
                        let conn = hyper::server::conn::Http::new()
                            .http1_only(true)
                            .serve_connection(tls_stream, service);

                        if let Err(e) = conn.await {
                            debug!("Connection error: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("TLS handshake error: {}", e);
                    }
                }
            });
        }
    }

    /// Start the DoH server without TLS (for testing or behind a reverse proxy)
    pub async fn run_http(&self) -> Result<(), DohError> {
        let addr = self.config.addr;
        let path = self.config.path.clone();
        let dns_handler = self.dns_handler.clone();

        let make_svc = make_service_fn(move |conn: &hyper::server::conn::AddrStream| {
            let dns_handler = dns_handler.clone();
            let path = path.clone();
            let client_ip = conn.remote_addr().ip();

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let dns_handler = dns_handler.clone();
                    let path = path.clone();
                    handle_request(req, dns_handler, path, client_ip)
                }))
            }
        });

        info!("DoH server (HTTP) listening on http://{}{}", addr, self.config.path);

        let server = Server::bind(&addr).serve(make_svc);
        server.await.map_err(|e| DohError::ServerError(format!("Server error: {}", e)))?;

        Ok(())
    }

    /// Check if DoH is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

/// Handle an incoming DoH request
async fn handle_request(
    req: Request<Body>,
    dns_handler: Arc<dyn DnsHandler>,
    doh_path: String,
    client_ip: IpAddr,
) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path();
    let method = req.method().clone();

    debug!("DoH request: {} {} from {}", method, path, client_ip);

    // Only handle requests to the DoH path
    if path != doh_path {
        return Ok(response_error(StatusCode::NOT_FOUND, "Not Found"));
    }

    let result = match method {
        Method::GET => handle_get(req, dns_handler, client_ip).await,
        Method::POST => handle_post(req, dns_handler, client_ip).await,
        Method::OPTIONS => {
            // Handle CORS preflight
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                .header("Access-Control-Allow-Headers", "Content-Type")
                .body(Body::empty())
                .unwrap())
        }
        _ => Ok(response_error(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")),
    };

    result.or_else(|e| {
        error!("DoH request error: {}", e);
        Ok(response_error(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"))
    })
}

/// Handle GET request with dns query parameter (RFC 8484)
async fn handle_get(
    req: Request<Body>,
    dns_handler: Arc<dyn DnsHandler>,
    client_ip: IpAddr,
) -> Result<Response<Body>, DohError> {
    // Parse query string
    let query_string = req.uri().query().unwrap_or("");
    let params: HashMap<String, String> = url::form_urlencoded::parse(query_string.as_bytes())
        .into_owned()
        .collect();

    // Get the 'dns' parameter (base64url encoded DNS message)
    let dns_param = params.get("dns")
        .ok_or_else(|| DohError::BadRequest("Missing 'dns' query parameter".to_string()))?;

    // Decode base64url
    let dns_query = URL_SAFE_NO_PAD.decode(dns_param)
        .map_err(|e| DohError::BadRequest(format!("Invalid base64url encoding: {}", e)))?;

    if dns_query.len() > MAX_DNS_MESSAGE_SIZE {
        return Err(DohError::BadRequest("DNS message too large".to_string()));
    }

    // Process DNS query
    let response = dns_handler.handle_query(&dns_query, client_ip).await
        .map_err(|e| DohError::HandlerError(format!("{}", e)))?;

    // Build response
    let ttl = extract_min_ttl(&response).unwrap_or(DEFAULT_CACHE_TTL);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, DNS_MESSAGE_CONTENT_TYPE)
        .header(CACHE_CONTROL, format!("max-age={}", ttl))
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(response))
        .unwrap())
}

/// Handle POST request with DNS message in body (RFC 8484)
async fn handle_post(
    req: Request<Body>,
    dns_handler: Arc<dyn DnsHandler>,
    client_ip: IpAddr,
) -> Result<Response<Body>, DohError> {
    // Check Content-Type
    let content_type = req.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with(DNS_MESSAGE_CONTENT_TYPE) {
        return Err(DohError::BadRequest(format!(
            "Invalid Content-Type: expected {}, got {}",
            DNS_MESSAGE_CONTENT_TYPE, content_type
        )));
    }

    // Read body
    let body_bytes = hyper::body::to_bytes(req.into_body()).await
        .map_err(|e| DohError::BadRequest(format!("Failed to read body: {}", e)))?;

    if body_bytes.is_empty() {
        return Err(DohError::BadRequest("Empty request body".to_string()));
    }

    if body_bytes.len() > MAX_DNS_MESSAGE_SIZE {
        return Err(DohError::BadRequest("DNS message too large".to_string()));
    }

    // Process DNS query
    let response = dns_handler.handle_query(&body_bytes, client_ip).await
        .map_err(|e| DohError::HandlerError(format!("{}", e)))?;

    // Build response
    let ttl = extract_min_ttl(&response).unwrap_or(DEFAULT_CACHE_TTL);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, DNS_MESSAGE_CONTENT_TYPE)
        .header(CACHE_CONTROL, format!("max-age={}", ttl))
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(response))
        .unwrap())
}

/// Extract minimum TTL from DNS response for Cache-Control header
fn extract_min_ttl(response: &[u8]) -> Option<u32> {
    // DNS response structure (simplified):
    // - Header (12 bytes)
    // - Question section
    // - Answer section (contains TTL)

    if response.len() < 12 {
        return None;
    }

    // Get answer count from header (bytes 6-7)
    let ancount = u16::from_be_bytes([response[6], response[7]]) as usize;

    if ancount == 0 {
        return None;
    }

    // Skip header (12 bytes) and question section
    // Question section: name (variable) + type (2) + class (2)
    let mut pos = 12;

    // Skip question section
    let qdcount = u16::from_be_bytes([response[4], response[5]]) as usize;
    for _ in 0..qdcount {
        // Skip name
        while pos < response.len() {
            let len = response[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            if len >= 0xC0 {
                // Compression pointer
                pos += 2;
                break;
            }
            pos += len + 1;
        }
        pos += 4; // Skip type and class
    }

    // Read TTLs from answer section
    let mut min_ttl: Option<u32> = None;

    for _ in 0..ancount {
        if pos + 10 > response.len() {
            break;
        }

        // Skip name
        while pos < response.len() {
            let len = response[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            if len >= 0xC0 {
                pos += 2;
                break;
            }
            pos += len + 1;
        }

        if pos + 10 > response.len() {
            break;
        }

        // Read TTL (bytes 4-7 of RR data after name)
        let ttl = u32::from_be_bytes([response[pos + 4], response[pos + 5], response[pos + 6], response[pos + 7]]);

        // Get RDLENGTH and skip RDATA
        let rdlength = u16::from_be_bytes([response[pos + 8], response[pos + 9]]) as usize;
        pos += 10 + rdlength;

        // Track minimum TTL
        min_ttl = Some(min_ttl.map(|m| m.min(ttl)).unwrap_or(ttl));
    }

    min_ttl
}

/// Build error response
fn response_error(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(message.to_string()))
        .unwrap()
}

/// Build TLS server configuration
fn build_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig, DohError> {
    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| DohError::TlsError(format!("Failed to build TLS config: {}", e)))?;

    Ok(config)
}

/// Load certificates from PEM file
fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, DohError> {
    let file = File::open(path)
        .map_err(|e| DohError::CertError(format!("Failed to open cert file: {}", e)))?;
    let mut reader = BufReader::new(file);
    let certs: Vec<CertificateDer<'static>> = certs(&mut reader)
        .filter_map(|r| r.ok())
        .collect();

    if certs.is_empty() {
        return Err(DohError::CertError("No certificates found".to_string()));
    }

    Ok(certs)
}

/// Load private key from PEM file
fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, DohError> {
    let file = File::open(path)
        .map_err(|e| DohError::KeyError(format!("Failed to open key file: {}", e)))?;
    let mut reader = BufReader::new(file);

    private_key(&mut reader)
        .map_err(|e| DohError::KeyError(format!("Failed to read private key: {}", e)))?
        .ok_or_else(|| DohError::KeyError("No private key found in file".to_string()))
}

/// DoH server errors
#[derive(Debug, Clone)]
pub enum DohError {
    ConfigError(String),
    BadRequest(String),
    CertError(String),
    KeyError(String),
    TlsError(String),
    ServerError(String),
    HandlerError(String),
}

impl std::fmt::Display for DohError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DohError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            DohError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            DohError::CertError(msg) => write!(f, "Certificate error: {}", msg),
            DohError::KeyError(msg) => write!(f, "Key error: {}", msg),
            DohError::TlsError(msg) => write!(f, "TLS error: {}", msg),
            DohError::ServerError(msg) => write!(f, "Server error: {}", msg),
            DohError::HandlerError(msg) => write!(f, "Handler error: {}", msg),
        }
    }
}

impl std::error::Error for DohError {}

/// Statistics for DoH server
#[derive(Debug, Clone, Default)]
pub struct DohStats {
    pub requests_total: u64,
    pub get_requests: u64,
    pub post_requests: u64,
    pub errors_total: u64,
    pub cache_hits: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock DNS handler for testing
    struct MockDnsHandler;

    #[async_trait::async_trait]
    impl DnsHandler for MockDnsHandler {
        async fn handle_query(&self, query: &[u8], _client_ip: IpAddr) -> Result<Vec<u8>, DohError> {
            // Echo back the query as response (for testing)
            Ok(query.to_vec())
        }
    }

    #[test]
    fn test_doh_error_display() {
        let err = DohError::ConfigError("test".to_string());
        assert!(err.to_string().contains("Config error"));

        let err = DohError::BadRequest("test".to_string());
        assert!(err.to_string().contains("Bad request"));

        let err = DohError::TlsError("test".to_string());
        assert!(err.to_string().contains("TLS error"));
    }

    #[test]
    fn test_doh_stats_default() {
        let stats = DohStats::default();
        assert_eq!(stats.requests_total, 0);
        assert_eq!(stats.get_requests, 0);
        assert_eq!(stats.post_requests, 0);
    }

    #[test]
    fn test_base64url_encoding() {
        // Test base64url decoding
        let encoded = "AAABAAABAAAAAAABCGV4YW1wbGUDY29tAAABAAE";
        let decoded = URL_SAFE_NO_PAD.decode(encoded);
        assert!(decoded.is_ok());
    }

    #[test]
    fn test_extract_min_ttl_empty() {
        let response: Vec<u8> = vec![];
        assert_eq!(extract_min_ttl(&response), None);

        let short_response = vec![0u8; 10];
        assert_eq!(extract_min_ttl(&short_response), None);
    }

    #[test]
    fn test_extract_min_ttl_no_answers() {
        // DNS response header with 0 answers
        let response = vec![
            0x00, 0x00, // ID
            0x81, 0x80, // Flags (response, no error)
            0x00, 0x01, // QDCOUNT = 1
            0x00, 0x00, // ANCOUNT = 0
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
        ];
        assert_eq!(extract_min_ttl(&response), None);
    }

    #[tokio::test]
    async fn test_handle_post_empty_body() {
        let handler = Arc::new(MockDnsHandler);

        let req = Request::builder()
            .method(Method::POST)
            .header(CONTENT_TYPE, DNS_MESSAGE_CONTENT_TYPE)
            .body(Body::empty())
            .unwrap();

        let result = handle_post(req, handler, "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_err());

        if let Err(DohError::BadRequest(msg)) = result {
            assert!(msg.contains("Empty"));
        } else {
            panic!("Expected BadRequest error");
        }
    }

    #[tokio::test]
    async fn test_handle_post_wrong_content_type() {
        let handler = Arc::new(MockDnsHandler);

        let req = Request::builder()
            .method(Method::POST)
            .header(CONTENT_TYPE, "text/plain")
            .body(Body::from(vec![0u8; 10]))
            .unwrap();

        let result = handle_post(req, handler, "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_err());

        if let Err(DohError::BadRequest(msg)) = result {
            assert!(msg.contains("Content-Type"));
        } else {
            panic!("Expected BadRequest error");
        }
    }

    #[tokio::test]
    async fn test_handle_post_valid() {
        let handler = Arc::new(MockDnsHandler);

        let dns_query = vec![0x01, 0x02, 0x03, 0x04];
        let req = Request::builder()
            .method(Method::POST)
            .header(CONTENT_TYPE, DNS_MESSAGE_CONTENT_TYPE)
            .body(Body::from(dns_query.clone()))
            .unwrap();

        let result = handle_post(req, handler, "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response.headers().get(CONTENT_TYPE).unwrap();
        assert_eq!(content_type, DNS_MESSAGE_CONTENT_TYPE);
    }

    #[tokio::test]
    async fn test_handle_get_missing_param() {
        let handler = Arc::new(MockDnsHandler);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/?foo=bar")
            .body(Body::empty())
            .unwrap();

        let result = handle_get(req, handler, "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_err());

        if let Err(DohError::BadRequest(msg)) = result {
            assert!(msg.contains("dns"));
        } else {
            panic!("Expected BadRequest error");
        }
    }

    #[tokio::test]
    async fn test_handle_get_valid() {
        let handler = Arc::new(MockDnsHandler);

        // Base64url encode a mock DNS query
        let dns_query = vec![0x01, 0x02, 0x03, 0x04];
        let encoded = URL_SAFE_NO_PAD.encode(&dns_query);

        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/?dns={}", encoded))
            .body(Body::empty())
            .unwrap();

        let result = handle_get(req, handler, "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_handle_request_wrong_path() {
        let handler = Arc::new(MockDnsHandler);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/wrong-path")
            .body(Body::empty())
            .unwrap();

        let result = handle_request(req, handler, "/dns-query".to_string(), "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_handle_request_options() {
        let handler = Arc::new(MockDnsHandler);

        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/dns-query")
            .body(Body::empty())
            .unwrap();

        let result = handle_request(req, handler, "/dns-query".to_string(), "127.0.0.1".parse().unwrap()).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Check CORS headers
        let cors = response.headers().get("Access-Control-Allow-Origin").unwrap();
        assert_eq!(cors, "*");
    }

    #[test]
    fn test_doh_config_default() {
        let config = DohConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.addr.port(), 443);
        assert_eq!(config.path, "/dns-query");
    }

    #[test]
    fn test_response_error() {
        let response = response_error(StatusCode::NOT_FOUND, "Not Found");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = response_error(StatusCode::BAD_REQUEST, "Bad Request");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
