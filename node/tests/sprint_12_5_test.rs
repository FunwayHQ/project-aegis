// Sprint 12.5: Critical Security Polish & Resilience - Integration Tests

use aegis_node::ip_extraction::{extract_client_ip, IpExtractionConfig};
use aegis_node::blocklist_persistence::{BlocklistEntry, BlocklistPersistence};
use aegis_node::waf::{AegisWaf, WafConfig};

#[test]
fn test_waf_body_inspection() {
    let waf = AegisWaf::new(WafConfig::default());

    // Test SQL injection in body
    let body = b"username=admin&password=' OR '1'='1";
    let matches = waf.analyze_request("POST", "/login", &[], Some(body));

    assert!(!matches.is_empty(), "WAF should detect SQL injection in body");
    assert_eq!(matches[0].category, "sqli");
    assert_eq!(matches[0].location, "Body");
}

#[test]
fn test_waf_xss_in_body() {
    let waf = AegisWaf::new(WafConfig::default());

    // Test XSS in body
    let body = b"comment=<script>alert('XSS')</script>";
    let matches = waf.analyze_request("POST", "/comment", &[], Some(body));

    assert!(!matches.is_empty(), "WAF should detect XSS in body");
    assert_eq!(matches[0].category, "xss");
}

#[test]
fn test_waf_rce_in_body() {
    let waf = AegisWaf::new(WafConfig::default());

    // Test RCE in body
    let body = b"cmd=; ls -la /etc";
    let matches = waf.analyze_request("POST", "/execute", &[], Some(body));

    assert!(!matches.is_empty(), "WAF should detect RCE in body");
    assert_eq!(matches[0].category, "rce");
}

#[test]
fn test_ip_extraction_from_x_forwarded_for() {
    let config = IpExtractionConfig::default();
    let headers = vec![
        ("X-Forwarded-For".to_string(), "203.0.113.195, 198.51.100.178".to_string()),
    ];

    let result = extract_client_ip(&config, "127.0.0.1", &headers);
    assert_eq!(result.ip(), "203.0.113.195");
}

#[test]
fn test_ip_extraction_from_x_real_ip() {
    let config = IpExtractionConfig::default();
    let headers = vec![
        ("X-Real-IP".to_string(), "203.0.113.42".to_string()),
    ];

    let result = extract_client_ip(&config, "127.0.0.1", &headers);
    assert_eq!(result.ip(), "203.0.113.42");
}

#[test]
fn test_ip_extraction_trusted_proxy_validation() {
    let config = IpExtractionConfig {
        trusted_proxies: vec!["10.0.0.1".to_string()],
        validate_trusted_proxies: true,
        ..Default::default()
    };

    let headers = vec![
        ("X-Forwarded-For".to_string(), "203.0.113.1".to_string()),
    ];

    // Untrusted proxy - should use connection IP
    let result = extract_client_ip(&config, "1.2.3.4", &headers);
    assert_eq!(result.ip(), "1.2.3.4");

    // Trusted proxy - should use header
    let result = extract_client_ip(&config, "10.0.0.1", &headers);
    assert_eq!(result.ip(), "203.0.113.1");
}

#[test]
fn test_ip_extraction_header_priority() {
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

#[test]
fn test_ip_extraction_cidr_matching() {
    let config = IpExtractionConfig {
        trusted_proxies: vec!["192.168.0.0/16".to_string()],
        validate_trusted_proxies: true,
        ..Default::default()
    };

    let headers = vec![
        ("X-Forwarded-For".to_string(), "203.0.113.1".to_string()),
    ];

    // IP in the CIDR range should be trusted
    let result = extract_client_ip(&config, "192.168.1.100", &headers);
    assert_eq!(result.ip(), "203.0.113.1");

    // IP outside the CIDR range should not be trusted
    let result = extract_client_ip(&config, "10.0.0.1", &headers);
    assert_eq!(result.ip(), "10.0.0.1");
}

#[test]
fn test_blocklist_persistence_add_and_retrieve() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_blocklist.db");
    let persistence = BlocklistPersistence::new(&db_path).unwrap();

    let entry = BlocklistEntry::new(
        "192.168.1.100".to_string(),
        60,
        "Test block".to_string(),
    );

    persistence.add_entry(&entry).unwrap();
    assert_eq!(persistence.count().unwrap(), 1);

    let entries = persistence.get_active_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].ip, "192.168.1.100");
}

#[test]
fn test_blocklist_persistence_cleanup() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_blocklist.db");
    let persistence = BlocklistPersistence::new(&db_path).unwrap();

    // Add multiple entries
    for i in 1..=5 {
        let entry = BlocklistEntry::new(
            format!("192.168.1.{}", i),
            60,
            format!("Block {}", i),
        );
        persistence.add_entry(&entry).unwrap();
    }

    assert_eq!(persistence.count().unwrap(), 5);
    assert_eq!(persistence.count_active().unwrap(), 5);

    // Add an entry that expires quickly
    let entry = BlocklistEntry::new(
        "192.168.1.254".to_string(),
        1, // 1 second
        "Quick block".to_string(),
    );
    persistence.add_entry(&entry).unwrap();
    assert_eq!(persistence.count().unwrap(), 6);

    // Wait for expiration
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Should have 5 active entries (6th expired)
    assert_eq!(persistence.count_active().unwrap(), 5);

    // Cleanup should remove 1 expired entry
    let deleted = persistence.cleanup_expired().unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(persistence.count().unwrap(), 5);
}

#[test]
fn test_waf_multiple_attacks_in_body() {
    let waf = AegisWaf::new(WafConfig::default());

    // Body with both SQL injection and XSS
    let body = b"data=' OR '1'='1 <script>alert(1)</script>";
    let matches = waf.analyze_request("POST", "/api", &[], Some(body));

    assert!(matches.len() >= 2, "WAF should detect multiple attacks");

    let categories: Vec<String> = matches.iter().map(|m| m.category.clone()).collect();
    assert!(categories.contains(&"sqli".to_string()));
    assert!(categories.contains(&"xss".to_string()));
}

#[test]
fn test_waf_clean_body_passes() {
    let waf = AegisWaf::new(WafConfig::default());

    // Clean body
    let body = b"username=john&password=secure123";
    let matches = waf.analyze_request("POST", "/login", &[], Some(body));

    assert!(matches.is_empty(), "WAF should not flag clean body");
}

#[test]
fn test_ip_extraction_case_insensitive_headers() {
    let config = IpExtractionConfig::default();

    // Test with lowercase header name
    let headers = vec![
        ("x-forwarded-for".to_string(), "203.0.113.1".to_string()),
    ];
    let result = extract_client_ip(&config, "127.0.0.1", &headers);
    assert_eq!(result.ip(), "203.0.113.1");

    // Test with uppercase header name
    let headers = vec![
        ("X-FORWARDED-FOR".to_string(), "203.0.113.2".to_string()),
    ];
    let result = extract_client_ip(&config, "127.0.0.1", &headers);
    assert_eq!(result.ip(), "203.0.113.2");
}

#[test]
fn test_ip_extraction_multiple_ips_in_xff() {
    let config = IpExtractionConfig::default();

    // X-Forwarded-For with multiple IPs (client, proxy1, proxy2)
    let headers = vec![
        ("X-Forwarded-For".to_string(), "203.0.113.1, 198.51.100.2, 192.0.2.3".to_string()),
    ];

    let result = extract_client_ip(&config, "127.0.0.1", &headers);
    // Should extract the leftmost (original client) IP
    assert_eq!(result.ip(), "203.0.113.1");
}

#[test]
fn test_blocklist_entry_expiration() {
    let entry = BlocklistEntry::new(
        "192.168.1.100".to_string(),
        2, // 2 seconds
        "Test".to_string(),
    );

    assert!(!entry.is_expired());
    assert!(entry.remaining_secs() > 0);

    std::thread::sleep(std::time::Duration::from_secs(3));

    assert!(entry.is_expired());
    assert_eq!(entry.remaining_secs(), 0);
}
