use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Instant;
use tracing::{error, info, warn};

/// Proxy configuration
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProxyConfig {
    pub http_addr: String,
    pub https_addr: Option<String>,
    pub origin: String,
    pub log_requests: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            http_addr: "0.0.0.0:8080".to_string(),
            https_addr: Some("0.0.0.0:8443".to_string()),
            origin: "http://httpbin.org".to_string(),
            log_requests: true,
        }
    }
}

/// Handle incoming requests and proxy to origin
///
/// SECURITY: Uses the TCP connection's SocketAddr for client IP, NOT headers.
/// This prevents IP spoofing attacks where malicious actors set X-Real-IP/X-Forwarded-For
/// headers to bypass rate limits, reputation checks, and access controls.
async fn handle_request(
    req: Request<Body>,
    client: Client<hyper::client::HttpConnector>,
    origin: String,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    let start_time = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    // Build upstream URL
    let upstream_uri = format!("{}{}{}", origin, path, query);

    match upstream_uri.parse::<Uri>() {
        Ok(uri) => {
            // Create new request to origin
            let (mut parts, body) = req.into_parts();
            parts.uri = uri;

            // SECURITY FIX: Use the actual TCP connection IP, NOT any client-provided headers
            // This prevents IP spoofing attacks where attackers set fake X-Real-IP or
            // X-Forwarded-For headers to bypass rate limiting, WAF rules, and reputation systems.
            let client_ip = remote_addr.ip().to_string();

            // Remove any client-provided forwarding headers to prevent spoofing
            parts.headers.remove("X-Real-IP");
            parts.headers.remove("X-Forwarded-For");

            // Set forwarding headers from the trusted TCP connection address
            parts.headers.insert(
                "X-Forwarded-For",
                client_ip.parse().unwrap(),
            );
            parts.headers.insert(
                "X-Real-IP",
                client_ip.parse().unwrap(),
            );
            parts
                .headers
                .insert("X-Forwarded-Proto", "http".parse().unwrap());
            parts
                .headers
                .insert("X-Served-By", "AEGIS-Edge-Node".parse().unwrap());

            let upstream_req = Request::from_parts(parts, body);

            // Proxy request to origin
            match client.request(upstream_req).await {
                Ok(mut response) => {
                    // Add AEGIS headers to response
                    response
                        .headers_mut()
                        .insert("X-AEGIS-Node", "edge-node-v0.1".parse().unwrap());

                    let status = response.status();
                    let duration = start_time.elapsed();

                    // Access log
                    info!(
                        "{} {} {} {}ms",
                        method,
                        path,
                        status.as_u16(),
                        duration.as_millis()
                    );

                    Ok(response)
                }
                Err(e) => {
                    error!("Upstream error: {}", e);
                    let duration = start_time.elapsed();

                    info!(
                        "{} {} 502 {}ms [upstream_error]",
                        method,
                        path,
                        duration.as_millis()
                    );

                    Ok(Response::builder()
                        .status(StatusCode::BAD_GATEWAY)
                        .body(Body::from("502 Bad Gateway - Upstream Error"))
                        .unwrap())
                }
            }
        }
        Err(e) => {
            warn!("Invalid URI: {}", e);
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("400 Bad Request"))
                .unwrap())
        }
    }
}

/// Start HTTP proxy server
///
/// SECURITY: Extracts the remote address from the TCP connection (AddrStream)
/// and passes it to handle_request for trusted IP identification.
pub async fn run_http_proxy(config: ProxyConfig) -> anyhow::Result<()> {
    let addr: SocketAddr = config.http_addr.parse()?;
    let origin = config.origin.clone();

    info!("Starting HTTP proxy on {}", addr);
    info!("Proxying to origin: {}", origin);

    let client = Client::new();

    // SECURITY FIX: Extract remote_addr from the TCP connection (AddrStream)
    // This ensures we use the actual TCP peer address, not spoofable headers
    let make_svc = make_service_fn(move |conn: &AddrStream| {
        let client = client.clone();
        let origin = origin.clone();
        // Extract the remote address from the TCP connection
        let remote_addr = conn.remote_addr();

        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_request(req, client.clone(), origin.clone(), remote_addr)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    info!("HTTP proxy listening on http://{}", addr);

    if let Err(e) = server.await {
        error!("Server error: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_default() {
        let config = ProxyConfig::default();
        assert_eq!(config.http_addr, "0.0.0.0:8080");
        assert_eq!(config.origin, "http://httpbin.org");
        assert!(config.log_requests);
    }

    #[test]
    fn test_proxy_config_parse() {
        let toml = r#"
            http_addr = "127.0.0.1:3000"
            origin = "https://example.com"
            log_requests = false
        "#;

        let config: ProxyConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.http_addr, "127.0.0.1:3000");
        assert_eq!(config.origin, "https://example.com");
        assert!(!config.log_requests);
    }
}
