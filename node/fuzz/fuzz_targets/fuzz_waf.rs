//! Additional: Fuzz target for WAF rule matching
//!
//! This fuzzer tests the WAF with arbitrary HTTP request data to find
//! crashes, ReDoS vulnerabilities, or memory safety issues.
//!
//! Run with: cargo +nightly fuzz run fuzz_waf

#![no_main]

use libfuzzer_sys::fuzz_target;
use aegis_node::waf::{AegisWaf, WafConfig};

// Create a static WAF instance to avoid recreation overhead
thread_local! {
    static WAF: AegisWaf = AegisWaf::new(WafConfig::default());
}

fuzz_target!(|data: &[u8]| {
    // Try to interpret data as UTF-8 for URI
    if let Ok(uri) = std::str::from_utf8(data) {
        WAF.with(|waf: &AegisWaf| {
            // Test WAF with arbitrary URI
            let _ = waf.analyze_request("GET", uri, &[], None);
            let _ = waf.analyze_request("POST", uri, &[], None);

            // Test with URI as header value
            let headers = vec![
                ("User-Agent".to_string(), uri.to_string()),
                ("X-Custom".to_string(), uri.to_string()),
            ];
            let _ = waf.analyze_request("GET", "/", &headers, None);
        });
    }

    // Test with arbitrary body bytes
    WAF.with(|waf: &AegisWaf| {
        let _ = waf.analyze_request("POST", "/api/data", &[], Some(data));
    });
});
