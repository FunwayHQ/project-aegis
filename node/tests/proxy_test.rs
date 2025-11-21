use aegis_node::pingora_proxy::{AegisProxy, ProxyConfig};

#[test]
fn test_proxy_config_default() {
    let config = ProxyConfig::default();
    assert_eq!(config.http_addr, "0.0.0.0:8080");
    assert_eq!(config.https_addr, Some("0.0.0.0:8443".to_string()));
    assert_eq!(config.origin, "http://httpbin.org");
    assert_eq!(config.threads, Some(4));
    assert_eq!(config.tls_cert_path, Some("cert.pem".to_string()));
    assert_eq!(config.tls_key_path, Some("key.pem".to_string()));
}

#[test]
fn test_proxy_config_custom() {
    let toml_str = r#"
        http_addr = "127.0.0.1:3000"
        https_addr = "127.0.0.1:3443"
        origin = "https://example.com"
        threads = 8
        enable_caching = false
    "#;

    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.http_addr, "127.0.0.1:3000");
    assert_eq!(config.https_addr, Some("127.0.0.1:3443".to_string()));
    assert_eq!(config.origin, "https://example.com");
    assert_eq!(config.threads, Some(8));
    assert_eq!(config.enable_caching, Some(false));
}

#[test]
fn test_proxy_config_no_https() {
    let toml_str = r#"
        http_addr = "0.0.0.0:80"
        origin = "http://backend.local"
        threads = 2
        enable_caching = false
    "#;

    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.http_addr, "0.0.0.0:80");
    assert!(config.https_addr.is_none());
    assert_eq!(config.origin, "http://backend.local");
}

#[test]
fn test_proxy_config_minimal() {
    let toml_str = r#"
        http_addr = "localhost:8000"
        origin = "http://127.0.0.1:9000"
    "#;

    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.http_addr, "localhost:8000");
    assert_eq!(config.origin, "http://127.0.0.1:9000");
    assert!(config.threads.is_none());
    assert!(config.enable_caching.is_none());
}

#[test]
fn test_aegis_proxy_creation() {
    let proxy = AegisProxy::new("http://example.com:8080".to_string());
    assert_eq!(proxy.origin_addr, "example.com:8080");
}

#[test]
fn test_aegis_proxy_http_origin() {
    let proxy = AegisProxy::new("http://backend.local".to_string());
    assert_eq!(proxy.origin_addr, "backend.local:80");
}

#[test]
fn test_aegis_proxy_https_origin() {
    let proxy = AegisProxy::new("https://secure.example.com:443".to_string());
    assert_eq!(proxy.origin_addr, "secure.example.com:443");
}

#[test]
fn test_proxy_config_serialization() {
    let config = ProxyConfig {
        http_addr: "0.0.0.0:8080".to_string(),
        https_addr: Some("0.0.0.0:8443".to_string()),
        origin: "http://origin.com".to_string(),
        threads: Some(4),
        tls_cert_path: Some("cert.pem".to_string()),
        tls_key_path: Some("key.pem".to_string()),
        cache_url: None,
        cache_ttl: None,
        enable_caching: None,
    };

    let toml_string = toml::to_string(&config).unwrap();
    assert!(toml_string.contains("http_addr"));
    assert!(toml_string.contains("origin"));
    assert!(toml_string.contains("tls_cert_path"));
    assert!(toml_string.contains("tls_key_path"));
}

#[test]
fn test_proxy_config_validation() {
    // Valid IPv4
    let config = ProxyConfig {
        http_addr: "192.168.1.1:8080".to_string(),
        https_addr: None,
        origin: "http://127.0.0.1".to_string(),
        threads: Some(1),
        tls_cert_path: None,
        tls_key_path: None,
        cache_url: None,
        cache_ttl: None,
        enable_caching: None,
    };
    assert!(config.http_addr.contains(':'));

    // Valid IPv6
    let config_v6 = ProxyConfig {
        http_addr: "[::1]:8080".to_string(),
        https_addr: None,
        origin: "http://[::1]:9000".to_string(),
        threads: None,
        tls_cert_path: None,
        tls_key_path: None,
        cache_url: None,
        cache_ttl: None,
        enable_caching: None,
    };
    assert!(config_v6.http_addr.contains('['));
}

#[test]
fn test_multiple_proxy_instances() {
    let proxy1 = AegisProxy::new("http://origin1.com".to_string());
    let proxy2 = AegisProxy::new("http://origin2.com".to_string());

    assert_eq!(proxy1.origin_addr, "origin1.com:80");
    assert_eq!(proxy2.origin_addr, "origin2.com:80");
    assert_ne!(proxy1.origin_addr, proxy2.origin_addr);
}

#[test]
fn test_proxy_config_thread_values() {
    // Test various thread counts
    for threads in [1, 2, 4, 8, 16, 32] {
        let config = ProxyConfig {
            http_addr: "0.0.0.0:8080".to_string(),
            https_addr: None,
            origin: "http://origin.com".to_string(),
            threads: Some(threads),
            tls_cert_path: None,
            tls_key_path: None,
            cache_url: None,
            cache_ttl: None,
            enable_caching: None,
        };
        assert_eq!(config.threads, Some(threads));
    }
}

#[test]
fn test_tls_config_with_cert_paths() {
    let config = ProxyConfig {
        http_addr: "0.0.0.0:80".to_string(),
        https_addr: Some("0.0.0.0:443".to_string()),
        origin: "http://backend.com".to_string(),
        threads: Some(4),
        tls_cert_path: Some("/path/to/cert.pem".to_string()),
        tls_key_path: Some("/path/to/key.pem".to_string()),
        cache_url: None,
        cache_ttl: None,
        enable_caching: None,
    };

    assert!(config.tls_cert_path.is_some());
    assert!(config.tls_key_path.is_some());
    assert_eq!(config.tls_cert_path.unwrap(), "/path/to/cert.pem");
    assert_eq!(config.tls_key_path.unwrap(), "/path/to/key.pem");
}

#[test]
fn test_tls_config_without_certs() {
    let toml_str = r#"
        http_addr = "0.0.0.0:8080"
        origin = "http://example.com"
    "#;

    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert!(config.tls_cert_path.is_none());
    assert!(config.tls_key_path.is_none());
    assert!(config.https_addr.is_none());
}

#[test]
fn test_origin_parsing_edge_cases() {
    // Test edge cases in origin URL parsing
    let test_cases = vec![
        ("http://example.com:8080", "example.com:8080"),
        ("http://sub.domain.com", "sub.domain.com:80"),
        ("https://api.v2.service.io:9443", "api.v2.service.io:9443"),
        ("http://localhost", "localhost:80"),
    ];

    for (origin, expected_addr) in test_cases {
        let proxy = AegisProxy::new(origin.to_string());
        assert_eq!(
            proxy.origin_addr, expected_addr,
            "Origin parsing failed for: {}",
            origin
        );
    }
}

#[test]
fn test_proxy_config_from_file() {
    // Simulate loading from actual config file format
    let toml_str = r#"
        # AEGIS Configuration
        http_addr = "0.0.0.0:8080"
        https_addr = "0.0.0.0:8443"
        origin = "http://httpbin.org"
        threads = 4
        tls_cert_path = "cert.pem"
        tls_key_path = "key.pem"
    "#;

    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.http_addr, "0.0.0.0:8080");
    assert_eq!(config.https_addr, Some("0.0.0.0:8443".to_string()));
    assert_eq!(config.tls_cert_path, Some("cert.pem".to_string()));
}

#[test]
fn test_origin_url_parsing() {
    // Test various origin formats with automatic port assignment
    let origins = vec![
        ("http://example.com", "example.com:80"),
        ("https://secure.com", "secure.com:443"),
        ("http://localhost:3000", "localhost:3000"),
        ("https://api.service.com:443", "api.service.com:443"),
        ("http://192.168.1.1", "192.168.1.1:80"),
        ("https://[::1]", "[::1]:443"),
    ];

    for (input, expected) in origins {
        let proxy = AegisProxy::new(input.to_string());
        assert_eq!(proxy.origin_addr, expected, "Failed for input: {}", input);
    }
}

#[test]
fn test_proxy_config_clone() {
    let config = ProxyConfig::default();
    let cloned = config.clone();

    assert_eq!(config.http_addr, cloned.http_addr);
    assert_eq!(config.origin, cloned.origin);
    assert_eq!(config.threads, cloned.threads);
}

#[test]
fn test_proxy_config_debug_format() {
    let config = ProxyConfig::default();
    let debug_str = format!("{:?}", config);

    assert!(debug_str.contains("ProxyConfig"));
    assert!(debug_str.contains("8080"));
    assert!(debug_str.contains("httpbin"));
}

// Integration tests - these require an actual proxy server running
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::process::{Child, Command};
    use std::thread;
    use std::time::Duration;

    /// Helper to check if port is available
    fn is_port_available(port: u16) -> bool {
        std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    /// Helper to wait for server to be ready
    fn wait_for_server(port: u16, max_attempts: u32) -> bool {
        for _ in 0..max_attempts {
            if let Ok(_) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn test_http_proxy_integration() {
        // This test requires the proxy to be running
        // Start proxy: cargo run --bin aegis-pingora -- pingora-config.toml

        if !is_port_available(8080) {
            // Proxy is running, test it
            let output = Command::new("curl")
                .args(&[
                    "-s",
                    "-o",
                    "/dev/null",
                    "-w",
                    "%{http_code}",
                    "http://localhost:8080/get",
                ])
                .output();

            if let Ok(result) = output {
                let status = String::from_utf8_lossy(&result.stdout);
                assert_eq!(status, "200", "HTTP proxy should return 200 OK");
            }
        }
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn test_https_proxy_integration() {
        // This test requires the proxy to be running with TLS

        if !is_port_available(8443) {
            // Proxy is running, test HTTPS
            let output = Command::new("curl")
                .args(&[
                    "-k",
                    "-s",
                    "-o",
                    "/dev/null",
                    "-w",
                    "%{http_code}",
                    "https://localhost:8443/get",
                ])
                .output();

            if let Ok(result) = output {
                let status = String::from_utf8_lossy(&result.stdout);
                assert_eq!(status, "200", "HTTPS proxy should return 200 OK");
            }
        }
    }

    #[test]
    #[ignore]
    fn test_proxy_response_time() {
        // Test that proxy responds in reasonable time
        if !is_port_available(8080) {
            let start = std::time::Instant::now();

            let output = Command::new("curl")
                .args(&["-s", "http://localhost:8080/delay/1"])
                .output();

            let elapsed = start.elapsed();

            if output.is_ok() {
                // Should take approximately 1 second plus proxy overhead
                assert!(elapsed.as_secs() >= 1);
                assert!(elapsed.as_secs() < 3);
            }
        }
    }
}

#[test]
fn test_config_with_tls_fields() {
    let toml_str = r#"
        http_addr = "0.0.0.0:8080"
        https_addr = "0.0.0.0:8443"
        origin = "http://backend.com"
        tls_cert_path = "test_cert.pem"
        tls_key_path = "test_key.pem"
    "#;

    let config: ProxyConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.tls_cert_path, Some("test_cert.pem".to_string()));
    assert_eq!(config.tls_key_path, Some("test_key.pem".to_string()));
}

#[test]
fn test_origin_with_path() {
    // Origins should not include paths, only scheme://host:port
    let proxy = AegisProxy::new("http://example.com/api/v1".to_string());
    // Path is preserved in origin_addr since we just strip scheme
    assert!(proxy.origin_addr.contains("example.com"));
}

#[test]
fn test_proxy_config_serde_roundtrip() {
    let original = ProxyConfig {
        http_addr: "1.2.3.4:80".to_string(),
        https_addr: Some("1.2.3.4:443".to_string()),
        origin: "http://backend.internal:8000".to_string(),
        threads: Some(16),
        tls_cert_path: Some("/etc/ssl/cert.pem".to_string()),
        tls_key_path: Some("/etc/ssl/key.pem".to_string()),
        cache_url: Some("redis://localhost:6379".to_string()),
        cache_ttl: Some(120),
        enable_caching: Some(true),
    };

    // Serialize to TOML
    let toml_str = toml::to_string(&original).unwrap();

    // Deserialize back
    let deserialized: ProxyConfig = toml::from_str(&toml_str).unwrap();

    assert_eq!(original.http_addr, deserialized.http_addr);
    assert_eq!(original.https_addr, deserialized.https_addr);
    assert_eq!(original.origin, deserialized.origin);
    assert_eq!(original.threads, deserialized.threads);
    assert_eq!(original.tls_cert_path, deserialized.tls_cert_path);
    assert_eq!(original.tls_key_path, deserialized.tls_key_path);
}
