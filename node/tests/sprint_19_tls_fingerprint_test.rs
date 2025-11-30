// Sprint 19: TLS Fingerprinting Integration Tests
//
// Tests for JA3/JA4 fingerprint computation, database integration,
// and enhanced bot detection with composite scoring.

use aegis_node::tls_fingerprint::{
    ClientHello, ClientType, FingerprintDatabase, FingerprintEntry, TlsAnalysisResult,
    TlsFingerprint, TlsFingerprintAnalyzer, TlsSuspicionLevel, TlsVersion,
};

// ============================================================================
// TLS Version Tests
// ============================================================================

#[test]
fn test_tls_version_from_u16() {
    assert_eq!(TlsVersion::from(0x0300), TlsVersion::Ssl30);
    assert_eq!(TlsVersion::from(0x0301), TlsVersion::Tls10);
    assert_eq!(TlsVersion::from(0x0302), TlsVersion::Tls11);
    assert_eq!(TlsVersion::from(0x0303), TlsVersion::Tls12);
    assert_eq!(TlsVersion::from(0x0304), TlsVersion::Tls13);

    // Unknown versions
    if let TlsVersion::Unknown(v) = TlsVersion::from(0x0305) {
        assert_eq!(v, 0x0305);
    } else {
        panic!("Expected Unknown variant");
    }
}

#[test]
fn test_tls_version_as_u16() {
    assert_eq!(TlsVersion::Ssl30.as_u16(), 0x0300);
    assert_eq!(TlsVersion::Tls10.as_u16(), 0x0301);
    assert_eq!(TlsVersion::Tls11.as_u16(), 0x0302);
    assert_eq!(TlsVersion::Tls12.as_u16(), 0x0303);
    assert_eq!(TlsVersion::Tls13.as_u16(), 0x0304);
}

#[test]
fn test_tls_version_ja4_code() {
    assert_eq!(TlsVersion::Ssl30.ja4_code(), 's');
    assert_eq!(TlsVersion::Tls10.ja4_code(), '0');
    assert_eq!(TlsVersion::Tls11.ja4_code(), '1');
    assert_eq!(TlsVersion::Tls12.ja4_code(), '2');
    assert_eq!(TlsVersion::Tls13.ja4_code(), '3');
    assert_eq!(TlsVersion::Unknown(0x0305).ja4_code(), 'x');
}

// ============================================================================
// ClientHello Parsing Tests
// ============================================================================

#[test]
fn test_client_hello_default() {
    let ch = ClientHello::default();
    assert_eq!(ch.record_version, TlsVersion::Tls12);
    assert_eq!(ch.handshake_version, TlsVersion::Tls12);
    assert!(ch.cipher_suites.is_empty());
    assert!(ch.extensions.is_empty());
    assert!(ch.sni.is_none());
}

#[test]
fn test_parse_minimal_client_hello() {
    // Minimal valid ClientHello
    let data = build_client_hello(
        TlsVersion::Tls12,
        &[0x1301, 0x1302], // Cipher suites
        &[],               // Extensions
        None,              // SNI
    );

    let ch = ClientHello::parse(&data);
    assert!(ch.is_some(), "Should parse valid ClientHello");

    let ch = ch.unwrap();
    assert_eq!(ch.handshake_version, TlsVersion::Tls12);
    assert_eq!(ch.cipher_suites.len(), 2);
    assert!(ch.cipher_suites.contains(&0x1301));
    assert!(ch.cipher_suites.contains(&0x1302));
}

#[test]
fn test_parse_client_hello_with_sni() {
    let data = build_client_hello_with_sni(
        TlsVersion::Tls13,
        &[0x1301, 0x1302, 0x1303],
        "example.com",
    );

    let ch = ClientHello::parse(&data);
    assert!(ch.is_some(), "Should parse ClientHello with SNI");

    let ch = ch.unwrap();
    assert_eq!(ch.sni, Some("example.com".to_string()));
}

#[test]
fn test_parse_invalid_data() {
    // Too short
    assert!(ClientHello::parse(&[0x16, 0x03]).is_none());

    // Wrong content type (not handshake)
    assert!(ClientHello::parse(&[0x15, 0x03, 0x03, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00]).is_none());

    // Empty
    assert!(ClientHello::parse(&[]).is_none());

    // Random garbage
    assert!(ClientHello::parse(&[0xff, 0xff, 0xff, 0xff]).is_none());
}

#[test]
fn test_parse_non_client_hello() {
    // ServerHello (handshake type 0x02) instead of ClientHello (0x01)
    let data = vec![
        0x16, 0x03, 0x03, 0x00, 0x05, // Record header
        0x02, 0x00, 0x00, 0x01, 0x00, // ServerHello
    ];
    assert!(ClientHello::parse(&data).is_none());
}

// ============================================================================
// JA3/JA4 Fingerprint Tests
// ============================================================================

#[test]
fn test_fingerprint_from_client_hello() {
    let ch = ClientHello {
        record_version: TlsVersion::Tls12,
        handshake_version: TlsVersion::Tls13,
        cipher_suites: vec![0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f],
        extensions: vec![0x0000, 0x000a, 0x000b, 0x000d, 0x0010, 0x002b],
        elliptic_curves: vec![0x001d, 0x0017, 0x0018],
        ec_point_formats: vec![0x00],
        sni: Some("example.com".to_string()),
        alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
        signature_algorithms: vec![0x0403, 0x0503],
        supported_versions: vec![0x0304, 0x0303],
    };

    let fp = TlsFingerprint::from_client_hello(&ch);

    // JA3 should be non-empty MD5 hash (32 hex chars)
    assert_eq!(fp.ja3.len(), 32);
    assert!(fp.ja3.chars().all(|c| c.is_ascii_hexdigit()));

    // JA3 raw should contain version and cipher info
    assert!(fp.ja3_raw.contains("772")); // TLS 1.3 = 0x0304 = 772
    assert!(!fp.ja3_raw.is_empty());

    // JA4 should start with 't' (TCP) and version
    assert!(fp.ja4.starts_with("t3")); // t for TCP, 3 for TLS 1.3
    assert!(fp.ja4.contains("d")); // 'd' for domain/SNI present

    // Metadata
    assert_eq!(fp.cipher_count, 5);
    assert_eq!(fp.extension_count, 6);
    assert!(fp.has_sni);
    assert!(fp.has_alpn);
}

#[test]
fn test_fingerprint_without_sni() {
    let ch = ClientHello {
        handshake_version: TlsVersion::Tls12,
        cipher_suites: vec![0x0035, 0x002f],
        extensions: vec![0x000a],
        sni: None,
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);

    assert!(!fp.has_sni);
    assert!(fp.ja4.contains("i")); // 'i' for no SNI
}

#[test]
fn test_fingerprint_grease_filtering() {
    // GREASE values should be filtered out of JA3/JA4
    let ch = ClientHello {
        handshake_version: TlsVersion::Tls13,
        cipher_suites: vec![0x0a0a, 0x1301, 0x1a1a, 0x1302], // GREASE values mixed in
        extensions: vec![0x2a2a, 0x0000, 0xfafa], // GREASE in extensions
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);

    // JA3 raw should NOT contain GREASE values
    assert!(!fp.ja3_raw.contains("2570"));  // 0x0a0a = 2570
    assert!(!fp.ja3_raw.contains("6682"));  // 0x1a1a = 6682
    assert!(!fp.ja3_raw.contains("64250")); // 0xfafa = 64250

    // Should contain non-GREASE values
    assert!(fp.ja3_raw.contains("4865")); // 0x1301 = 4865
    assert!(fp.ja3_raw.contains("4866")); // 0x1302 = 4866
}

#[test]
fn test_fingerprint_consistency() {
    // Same ClientHello should produce same fingerprint
    let ch = ClientHello {
        handshake_version: TlsVersion::Tls13,
        cipher_suites: vec![0x1301, 0x1302],
        extensions: vec![0x0000, 0x000a],
        sni: Some("test.com".to_string()),
        ..Default::default()
    };

    let fp1 = TlsFingerprint::from_client_hello(&ch);
    let fp2 = TlsFingerprint::from_client_hello(&ch);

    assert_eq!(fp1.ja3, fp2.ja3);
    assert_eq!(fp1.ja4, fp2.ja4);
    assert_eq!(fp1.ja3_raw, fp2.ja3_raw);
}

// ============================================================================
// Fingerprint Database Tests
// ============================================================================

#[test]
fn test_fingerprint_database_creation() {
    let db = FingerprintDatabase::new();
    let (ja3_count, ja4_count) = db.stats();

    // Should have built-in fingerprints loaded
    assert!(ja3_count > 0, "Should have JA3 fingerprints");
    assert!(ja4_count > 0, "Should have JA4 fingerprints");
}

#[test]
fn test_fingerprint_database_lookup() {
    let db = FingerprintDatabase::new();

    // Look up a known Chrome fingerprint (from built-ins)
    let chrome_ja3 = "cd08e31494f9531f560d64c695473da9";
    let result = db.lookup(&TlsFingerprint {
        ja3: chrome_ja3.to_string(),
        ja3_raw: String::new(),
        ja4: String::new(),
        tls_version: TlsVersion::Tls13,
        cipher_count: 0,
        extension_count: 0,
        has_sni: true,
        has_alpn: true,
    });

    assert!(result.is_some(), "Should find Chrome fingerprint");
    let entry = result.unwrap();
    assert_eq!(entry.client_type, ClientType::Browser);
}

#[test]
fn test_fingerprint_database_upsert() {
    let db = FingerprintDatabase::new();

    let fp = TlsFingerprint {
        ja3: "test_custom_fingerprint_123".to_string(),
        ja3_raw: String::new(),
        ja4: "t3d050400_custom".to_string(),
        tls_version: TlsVersion::Tls13,
        cipher_count: 5,
        extension_count: 4,
        has_sni: true,
        has_alpn: true,
    };

    let entry = FingerprintEntry {
        client_type: ClientType::AutomationTool,
        client_name: "Custom Tool".to_string(),
        confidence: 0.85,
        first_seen: 0,
        last_seen: 0,
        request_count: 1,
    };

    db.upsert(&fp, entry).unwrap();

    let result = db.lookup(&fp);
    assert!(result.is_some());
    assert_eq!(result.unwrap().client_name, "Custom Tool");
}

// ============================================================================
// TLS Fingerprint Analyzer Tests
// ============================================================================

#[test]
fn test_analyzer_creation() {
    let analyzer = TlsFingerprintAnalyzer::new();
    let (ja3_count, _) = analyzer.database().stats();
    assert!(ja3_count > 0);
}

#[test]
fn test_analyzer_browser_fingerprint() {
    let analyzer = TlsFingerprintAnalyzer::new();

    let ch = ClientHello {
        handshake_version: TlsVersion::Tls13,
        cipher_suites: vec![0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f, 0xc02c, 0xc030],
        extensions: vec![0x0000, 0x000a, 0x000b, 0x000d, 0x0010, 0x002b, 0x0033],
        sni: Some("example.com".to_string()),
        alpn_protocols: vec!["h2".to_string()],
        supported_versions: vec![0x0304],
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);
    let result = analyzer.analyze(&fp, Some("Mozilla/5.0 Chrome/120.0.0.0"));

    // Should not be high/critical suspicion for browser-like profile
    assert!(
        !matches!(result.suspicion_level, TlsSuspicionLevel::Critical),
        "Browser-like fingerprint should not be critical"
    );
}

#[test]
fn test_analyzer_mismatch_detection() {
    let analyzer = TlsFingerprintAnalyzer::new();

    // Create a curl-like fingerprint
    let ch = ClientHello {
        handshake_version: TlsVersion::Tls12,
        cipher_suites: vec![0x0035, 0x002f, 0x000a],
        extensions: vec![0x000a, 0x000b],
        sni: None,
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);

    // Analyze with Chrome User-Agent (mismatch!)
    let result = analyzer.analyze(&fp, Some("Mozilla/5.0 Chrome/120.0.0.0"));

    // Should detect suspicion
    assert!(
        !result.suspicion_reasons.is_empty(),
        "Should flag suspicious patterns"
    );
}

#[test]
fn test_analyzer_low_cipher_count() {
    let analyzer = TlsFingerprintAnalyzer::new();

    let ch = ClientHello {
        handshake_version: TlsVersion::Tls12,
        cipher_suites: vec![0x0035], // Only 1 cipher
        extensions: vec![],
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);
    let result = analyzer.analyze(&fp, None);

    // Should flag low cipher count
    assert!(
        result.suspicion_reasons.iter().any(|r| r.contains("cipher")),
        "Should flag low cipher count"
    );
    assert!(result.score_adjustment < 0, "Score should be penalized");
}

#[test]
fn test_analyzer_outdated_tls() {
    let analyzer = TlsFingerprintAnalyzer::new();

    let ch = ClientHello {
        handshake_version: TlsVersion::Tls10, // Outdated
        cipher_suites: vec![0x0035, 0x002f],
        extensions: vec![],
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);
    let result = analyzer.analyze(&fp, None);

    // Should flag outdated TLS
    assert!(
        result.suspicion_reasons.iter().any(|r| r.contains("Outdated TLS")),
        "Should flag outdated TLS version"
    );
}

#[test]
fn test_analyzer_no_sni_with_browser_ua() {
    let analyzer = TlsFingerprintAnalyzer::new();

    let ch = ClientHello {
        handshake_version: TlsVersion::Tls13,
        cipher_suites: vec![0x1301, 0x1302],
        extensions: vec![0x000a],
        sni: None, // No SNI
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);

    // Browser UA but no SNI
    let result = analyzer.analyze(&fp, Some("Mozilla/5.0 Firefox/120.0"));

    // Should flag no SNI with browser UA
    assert!(
        result.suspicion_reasons.iter().any(|r| r.contains("SNI") || r.contains("sni")),
        "Should flag no SNI with browser UA"
    );
}

#[test]
fn test_analyzer_score_bounds() {
    let analyzer = TlsFingerprintAnalyzer::new();

    // Very suspicious fingerprint
    let ch = ClientHello {
        handshake_version: TlsVersion::Tls10,
        cipher_suites: vec![0x0001], // Single weak cipher
        extensions: vec![],
        sni: None,
        ..Default::default()
    };

    let fp = TlsFingerprint::from_client_hello(&ch);
    let result = analyzer.analyze(&fp, Some("Mozilla/5.0 Chrome/120"));

    // Score adjustment should be within bounds
    assert!(
        result.score_adjustment >= -50 && result.score_adjustment <= 50,
        "Score adjustment {} should be within [-50, 50]",
        result.score_adjustment
    );
}

// ============================================================================
// Suspicion Level Tests
// ============================================================================

#[test]
fn test_suspicion_levels_ordering() {
    // Just verify the enum variants exist and can be compared
    let low = TlsSuspicionLevel::Low;
    let medium = TlsSuspicionLevel::Medium;
    let high = TlsSuspicionLevel::High;
    let critical = TlsSuspicionLevel::Critical;

    assert_ne!(low, medium);
    assert_ne!(medium, high);
    assert_ne!(high, critical);
}

// ============================================================================
// Helper Functions for Building Test Data
// ============================================================================

fn build_client_hello(
    version: TlsVersion,
    cipher_suites: &[u16],
    extensions: &[u16],
    sni: Option<&str>,
) -> Vec<u8> {
    let mut data = Vec::new();

    // Calculate sizes
    let cipher_len = cipher_suites.len() * 2;
    let ext_len = if sni.is_some() || !extensions.is_empty() {
        let mut len = 0;
        for _ in extensions {
            len += 4; // type (2) + length (2) + no data
        }
        if let Some(s) = sni {
            len += 4 + 5 + s.len(); // SNI extension
        }
        len
    } else {
        0
    };

    let client_hello_len = 2 + 32 + 1 + cipher_len + 2 + 1 + 1 + if ext_len > 0 { ext_len + 2 } else { 0 };
    let handshake_len = 4 + client_hello_len;
    let record_len = handshake_len;

    // Record layer
    data.push(0x16); // Handshake
    data.push(0x03);
    data.push(0x01); // TLS 1.0 record version
    data.push((record_len >> 8) as u8);
    data.push(record_len as u8);

    // Handshake header
    data.push(0x01); // ClientHello
    data.push(0);
    data.push((client_hello_len >> 8) as u8);
    data.push(client_hello_len as u8);

    // Client version
    let v = version.as_u16();
    data.push((v >> 8) as u8);
    data.push(v as u8);

    // Random (32 bytes)
    data.extend_from_slice(&[0u8; 32]);

    // Session ID (0 length)
    data.push(0);

    // Cipher suites
    data.push((cipher_len >> 8) as u8);
    data.push(cipher_len as u8);
    for suite in cipher_suites {
        data.push((suite >> 8) as u8);
        data.push(*suite as u8);
    }

    // Compression methods
    data.push(1);
    data.push(0);

    // Extensions
    if ext_len > 0 {
        data.push((ext_len >> 8) as u8);
        data.push(ext_len as u8);

        // Add SNI if present
        if let Some(s) = sni {
            let sni_ext_len = 5 + s.len();
            data.push(0x00);
            data.push(0x00); // SNI type
            data.push((sni_ext_len >> 8) as u8);
            data.push(sni_ext_len as u8);
            data.push(((sni_ext_len - 2) >> 8) as u8);
            data.push((sni_ext_len - 2) as u8);
            data.push(0x00); // Host name type
            data.push((s.len() >> 8) as u8);
            data.push(s.len() as u8);
            data.extend_from_slice(s.as_bytes());
        }

        // Add other extensions
        for ext in extensions {
            data.push((ext >> 8) as u8);
            data.push(*ext as u8);
            data.push(0);
            data.push(0); // Zero length extension
        }
    }

    data
}

fn build_client_hello_with_sni(
    version: TlsVersion,
    cipher_suites: &[u16],
    sni: &str,
) -> Vec<u8> {
    build_client_hello(version, cipher_suites, &[], Some(sni))
}

// ============================================================================
// Integration Tests (if Wasm module available)
// ============================================================================

#[cfg(feature = "integration_tests")]
mod integration_tests {
    use super::*;
    use aegis_node::bot_management::{BotManager, BotPolicy};
    use aegis_node::enhanced_bot_detection::{EnhancedBotConfig, EnhancedBotDetector};

    fn get_bot_manager() -> Option<BotManager> {
        BotManager::new("bot-detector.wasm", BotPolicy::default()).ok()
    }

    #[test]
    fn test_enhanced_detector_full_flow() {
        let Some(bot_manager) = get_bot_manager() else {
            println!("Skipping - Wasm module not available");
            return;
        };

        let detector = EnhancedBotDetector::new(bot_manager, EnhancedBotConfig::default());

        // Browser request
        let browser_ch = ClientHello {
            handshake_version: TlsVersion::Tls13,
            cipher_suites: vec![0x1301, 0x1302, 0x1303],
            extensions: vec![0x0000, 0x000a],
            sni: Some("example.com".to_string()),
            ..Default::default()
        };
        let browser_fp = TlsFingerprint::from_client_hello(&browser_ch);

        let (score, verdict, action) = detector
            .analyze(
                "Mozilla/5.0 Chrome/120",
                "192.168.1.1",
                Some(&browser_fp),
            )
            .unwrap();

        println!(
            "Browser: score={}, verdict={:?}, action={:?}",
            score.score, verdict, action
        );
    }
}
