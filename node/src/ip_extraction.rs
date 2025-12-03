use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;

/// Configuration for IP extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpExtractionConfig {
    /// Ordered list of headers to check for client IP
    /// Example: ["X-Forwarded-For", "X-Real-IP", "CF-Connecting-IP"]
    pub trusted_headers: Vec<String>,

    /// List of trusted proxy IP addresses/ranges
    /// If the direct connection IP is not in this list, we don't trust the headers
    pub trusted_proxies: Vec<String>,

    /// Whether to enable trusted proxy validation
    pub validate_trusted_proxies: bool,
}

impl Default for IpExtractionConfig {
    fn default() -> Self {
        Self {
            trusted_headers: vec![
                "X-Forwarded-For".to_string(),
                "X-Real-IP".to_string(),
                "CF-Connecting-IP".to_string(),
            ],
            trusted_proxies: vec![
                "127.0.0.1".to_string(),
                "::1".to_string(),
                // Common private ranges (for local development)
                "10.0.0.0/8".to_string(),
                "172.16.0.0/12".to_string(),
                "192.168.0.0/16".to_string(),
            ],
            validate_trusted_proxies: true,
        }
    }
}

/// IP extraction result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpSource {
    /// IP extracted from a trusted header
    TrustedHeader { ip: String, header: String },
    /// IP from direct connection (no header or untrusted proxy)
    DirectConnection { ip: String },
}

impl IpSource {
    pub fn ip(&self) -> &str {
        match self {
            IpSource::TrustedHeader { ip, .. } => ip,
            IpSource::DirectConnection { ip } => ip,
        }
    }
}

/// Extract client IP address from request, considering X-Forwarded-For and other headers
pub fn extract_client_ip(
    config: &IpExtractionConfig,
    connection_ip: &str,
    headers: &[(String, String)],
) -> IpSource {
    // First, check if connection IP is from a trusted proxy
    let is_trusted_proxy = if config.validate_trusted_proxies {
        is_trusted_ip(connection_ip, &config.trusted_proxies)
    } else {
        // If validation is disabled, trust all proxies
        true
    };

    // If not from a trusted proxy, use the connection IP directly
    if !is_trusted_proxy {
        log::debug!(
            "Connection from untrusted proxy {}, using direct IP",
            connection_ip
        );
        return IpSource::DirectConnection {
            ip: connection_ip.to_string(),
        };
    }

    // Check each trusted header in order
    for header_name in &config.trusted_headers {
        if let Some(header_value) = find_header(headers, header_name) {
            // X-Forwarded-For can contain multiple IPs: "client, proxy1, proxy2"
            // We want the leftmost (original client) IP
            if let Some(client_ip) = extract_leftmost_ip(&header_value) {
                // Validate it's a valid IP
                if is_valid_ip(&client_ip) {
                    log::debug!(
                        "Extracted client IP {} from header {}",
                        client_ip,
                        header_name
                    );
                    return IpSource::TrustedHeader {
                        ip: client_ip,
                        header: header_name.clone(),
                    };
                } else {
                    log::warn!(
                        "Invalid IP {} in header {}, skipping",
                        client_ip,
                        header_name
                    );
                }
            }
        }
    }

    // No trusted header found or all invalid, fall back to connection IP
    log::debug!("No valid IP in trusted headers, using connection IP");
    IpSource::DirectConnection {
        ip: connection_ip.to_string(),
    }
}

/// Check if an IP address is in the trusted proxy list
fn is_trusted_ip(ip: &str, trusted_proxies: &[String]) -> bool {
    // Simple implementation: exact match or CIDR range check
    for trusted in trusted_proxies {
        if trusted.contains('/') {
            // CIDR notation - simplified check
            // For production, use a proper CIDR library like ipnet
            if ip_in_cidr(ip, trusted) {
                return true;
            }
        } else {
            // Exact match
            if ip == trusted {
                return true;
            }
        }
    }
    false
}

/// Simplified CIDR matching (for demonstration)
/// For production use, integrate a proper library like `ipnet`
fn ip_in_cidr(ip: &str, cidr: &str) -> bool {
    // Parse CIDR
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return false;
    }

    let network_addr = parts[0];
    let prefix_len: u32 = parts[1].parse().unwrap_or(32);

    // Parse IPs
    let ip_addr = match IpAddr::from_str(ip) {
        Ok(addr) => addr,
        Err(_) => return false,
    };

    let network = match IpAddr::from_str(network_addr) {
        Ok(addr) => addr,
        Err(_) => return false,
    };

    // Only support IPv4 for this simplified implementation
    match (ip_addr, network) {
        (IpAddr::V4(ip_v4), IpAddr::V4(net_v4)) => {
            let ip_u32 = u32::from(ip_v4);
            let net_u32 = u32::from(net_v4);
            let mask = if prefix_len == 0 {
                0
            } else {
                !0u32 << (32 - prefix_len)
            };

            (ip_u32 & mask) == (net_u32 & mask)
        }
        _ => false, // IPv6 not supported in this simple implementation
    }
}

/// Extract the leftmost IP from a comma-separated list
/// Example: "203.0.113.1, 198.51.100.2" -> "203.0.113.1"
fn extract_leftmost_ip(header_value: &str) -> Option<String> {
    header_value
        .split(',')
        .next()
        .map(|s| s.trim().to_string())
}

/// Find a header value (case-insensitive)
fn find_header(headers: &[(String, String)], name: &str) -> Option<String> {
    let name_lower = name.to_lowercase();
    headers
        .iter()
        .find(|(header_name, _)| header_name.to_lowercase() == name_lower)
        .map(|(_, value)| value.clone())
}

/// Validate if a string is a valid IP address
/// SECURITY (X5.9): Strict validation - reject any control characters or whitespace
fn is_valid_ip(ip: &str) -> bool {
    // Reject strings with control characters (newlines, tabs, null bytes, etc.)
    // This prevents HTTP header injection attacks
    if ip.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    // Standard IP parsing
    IpAddr::from_str(ip).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_x_forwarded_for() {
        let config = IpExtractionConfig::default();
        let headers = vec![
            ("X-Forwarded-For".to_string(), "203.0.113.1, 198.51.100.2".to_string()),
        ];

        let result = extract_client_ip(&config, "127.0.0.1", &headers);
        assert_eq!(result.ip(), "203.0.113.1");
    }

    #[test]
    fn test_extract_from_x_real_ip() {
        let config = IpExtractionConfig::default();
        let headers = vec![
            ("X-Real-IP".to_string(), "203.0.113.5".to_string()),
        ];

        let result = extract_client_ip(&config, "127.0.0.1", &headers);
        assert_eq!(result.ip(), "203.0.113.5");
    }

    #[test]
    fn test_untrusted_proxy_uses_connection_ip() {
        let config = IpExtractionConfig {
            trusted_proxies: vec!["10.0.0.1".to_string()],
            validate_trusted_proxies: true,
            ..Default::default()
        };

        let headers = vec![
            ("X-Forwarded-For".to_string(), "203.0.113.1".to_string()),
        ];

        // Connection from 1.2.3.4 which is NOT in trusted_proxies
        let result = extract_client_ip(&config, "1.2.3.4", &headers);

        // Should use connection IP, not header
        assert_eq!(result.ip(), "1.2.3.4");
        assert!(matches!(result, IpSource::DirectConnection { .. }));
    }

    #[test]
    fn test_trusted_proxy_uses_header() {
        let config = IpExtractionConfig {
            trusted_proxies: vec!["10.0.0.1".to_string()],
            validate_trusted_proxies: true,
            ..Default::default()
        };

        let headers = vec![
            ("X-Forwarded-For".to_string(), "203.0.113.1".to_string()),
        ];

        // Connection from 10.0.0.1 which IS in trusted_proxies
        let result = extract_client_ip(&config, "10.0.0.1", &headers);

        // Should use header
        assert_eq!(result.ip(), "203.0.113.1");
        assert!(matches!(result, IpSource::TrustedHeader { .. }));
    }

    #[test]
    fn test_no_header_uses_connection_ip() {
        let config = IpExtractionConfig::default();
        let headers = vec![];

        let result = extract_client_ip(&config, "192.168.1.100", &headers);
        assert_eq!(result.ip(), "192.168.1.100");
    }

    #[test]
    fn test_invalid_ip_in_header_falls_back() {
        let config = IpExtractionConfig::default();
        let headers = vec![
            ("X-Forwarded-For".to_string(), "not-an-ip".to_string()),
        ];

        let result = extract_client_ip(&config, "192.168.1.100", &headers);
        assert_eq!(result.ip(), "192.168.1.100");
    }

    #[test]
    fn test_cidr_matching() {
        assert!(ip_in_cidr("192.168.1.100", "192.168.0.0/16"));
        assert!(ip_in_cidr("192.168.255.255", "192.168.0.0/16"));
        assert!(!ip_in_cidr("192.169.1.1", "192.168.0.0/16"));

        assert!(ip_in_cidr("10.0.0.1", "10.0.0.0/8"));
        assert!(ip_in_cidr("10.255.255.255", "10.0.0.0/8"));
        assert!(!ip_in_cidr("11.0.0.1", "10.0.0.0/8"));
    }

    #[test]
    fn test_leftmost_ip_extraction() {
        assert_eq!(
            extract_leftmost_ip("203.0.113.1, 198.51.100.2, 192.0.2.1"),
            Some("203.0.113.1".to_string())
        );

        assert_eq!(
            extract_leftmost_ip("203.0.113.1"),
            Some("203.0.113.1".to_string())
        );

        assert_eq!(
            extract_leftmost_ip("  203.0.113.1  "),
            Some("203.0.113.1".to_string())
        );
    }

    #[test]
    fn test_case_insensitive_header_lookup() {
        let headers = vec![
            ("x-forwarded-for".to_string(), "203.0.113.1".to_string()),
        ];

        assert_eq!(
            find_header(&headers, "X-Forwarded-For"),
            Some("203.0.113.1".to_string())
        );

        assert_eq!(
            find_header(&headers, "X-FORWARDED-FOR"),
            Some("203.0.113.1".to_string())
        );
    }

    #[test]
    fn test_header_priority() {
        let config = IpExtractionConfig {
            trusted_headers: vec![
                "X-Forwarded-For".to_string(),
                "X-Real-IP".to_string(),
            ],
            ..Default::default()
        };

        let headers = vec![
            ("X-Real-IP".to_string(), "203.0.113.2".to_string()),
            ("X-Forwarded-For".to_string(), "203.0.113.1".to_string()),
        ];

        let result = extract_client_ip(&config, "127.0.0.1", &headers);
        // Should use X-Forwarded-For (first in priority list)
        assert_eq!(result.ip(), "203.0.113.1");
    }

    // ============================================
    // SECURITY TESTS (X5.9): IP format validation
    // ============================================

    #[test]
    fn test_x59_ipv6_validation() {
        let config = IpExtractionConfig::default();

        // Valid IPv6 addresses
        let headers = vec![
            ("X-Forwarded-For".to_string(), "2001:db8::1".to_string()),
        ];
        let result = extract_client_ip(&config, "127.0.0.1", &headers);
        assert_eq!(result.ip(), "2001:db8::1");

        // Full IPv6
        let headers2 = vec![
            ("X-Forwarded-For".to_string(), "2001:0db8:85a3:0000:0000:8a2e:0370:7334".to_string()),
        ];
        let result2 = extract_client_ip(&config, "127.0.0.1", &headers2);
        assert_eq!(result2.ip(), "2001:0db8:85a3:0000:0000:8a2e:0370:7334");
    }

    #[test]
    fn test_x59_malformed_ip_rejection() {
        let config = IpExtractionConfig::default();

        // Various malformed IPs - note that leading/trailing whitespace is trimmed by extract_leftmost_ip
        let malformed_ips = [
            "256.1.1.1",           // Octet out of range
            "192.168.1",           // Missing octet
            "192.168.1.1.1",       // Extra octet
            "192.168.1.1:80",      // Port appended
            "::ffff:300.1.1.1",    // Invalid IPv4-mapped IPv6
            "not-an-ip",
            "",
            "   ",
            "192.168.1\n.1",       // Embedded newline (mid-string)
            "192.168\r.1.1",       // Embedded CR (mid-string)
            "192.168\x00.1.1",     // Embedded null byte (mid-string)
            "192 .168.1.1",        // Embedded space (mid-string)
            "192.\t168.1.1",       // Embedded tab (mid-string)
        ];

        for bad_ip in malformed_ips {
            let headers = vec![
                ("X-Forwarded-For".to_string(), bad_ip.to_string()),
            ];
            let result = extract_client_ip(&config, "192.168.1.100", &headers);
            // Should fall back to connection IP for invalid IPs
            assert_eq!(
                result.ip(),
                "192.168.1.100",
                "Failed to reject malformed IP: '{:?}'",
                bad_ip
            );
        }
    }

    #[test]
    fn test_x59_trailing_whitespace_trimmed() {
        let config = IpExtractionConfig::default();

        // Trailing whitespace should be trimmed successfully
        // This is handled by extract_leftmost_ip's trim() call
        let acceptable_ips = [
            ("192.168.1.1\n", "192.168.1.1"),     // Trailing newline trimmed
            ("192.168.1.1\r\n", "192.168.1.1"),   // Trailing CRLF trimmed
            ("192.168.1.1  ", "192.168.1.1"),     // Trailing spaces trimmed
            ("  192.168.1.1", "192.168.1.1"),     // Leading spaces trimmed
        ];

        for (input, expected) in acceptable_ips {
            let headers = vec![
                ("X-Forwarded-For".to_string(), input.to_string()),
            ];
            let result = extract_client_ip(&config, "127.0.0.1", &headers);
            assert_eq!(
                result.ip(),
                expected,
                "Expected IP '{}' after trimming '{:?}'",
                expected,
                input
            );
        }
    }

    #[test]
    fn test_x59_multiple_ips_with_malformed() {
        let config = IpExtractionConfig::default();

        // First IP is malformed, second is valid
        let headers = vec![
            ("X-Forwarded-For".to_string(), "not-valid".to_string()),
            ("X-Real-IP".to_string(), "203.0.113.5".to_string()),
        ];
        let result = extract_client_ip(&config, "127.0.0.1", &headers);
        // Should skip X-Forwarded-For and use X-Real-IP
        assert_eq!(result.ip(), "203.0.113.5");
    }

    #[test]
    fn test_x59_whitespace_handling() {
        let config = IpExtractionConfig::default();

        // Various whitespace scenarios
        let headers = vec![
            ("X-Forwarded-For".to_string(), "  203.0.113.1  , 10.0.0.1  ".to_string()),
        ];
        let result = extract_client_ip(&config, "127.0.0.1", &headers);
        assert_eq!(result.ip(), "203.0.113.1");
    }

    #[test]
    fn test_x59_empty_header_value() {
        let config = IpExtractionConfig::default();

        let headers = vec![
            ("X-Forwarded-For".to_string(), "".to_string()),
            ("X-Real-IP".to_string(), "".to_string()),
        ];
        let result = extract_client_ip(&config, "192.168.1.100", &headers);
        // Should fall back to connection IP
        assert_eq!(result.ip(), "192.168.1.100");
    }

    #[test]
    fn test_x59_connection_ip_validation() {
        // The is_valid_ip function is used for header IPs but connection_ip is trusted
        // This test ensures the connection_ip is passed through as-is
        let config = IpExtractionConfig::default();
        let headers = vec![];

        let result = extract_client_ip(&config, "192.168.1.100", &headers);
        assert_eq!(result.ip(), "192.168.1.100");

        // Even if connection_ip looks weird, it's passed through
        // (in production, this would be validated by the network layer)
        let result2 = extract_client_ip(&config, "0.0.0.0", &headers);
        assert_eq!(result2.ip(), "0.0.0.0");
    }
}
