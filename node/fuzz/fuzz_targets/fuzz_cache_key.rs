//! Additional: Fuzz target for cache key generation
//!
//! This fuzzer tests the cache key generation with arbitrary strings to find
//! crashes, injection vulnerabilities, or memory safety issues.
//!
//! Run with: cargo +nightly fuzz run fuzz_cache_key

#![no_main]

use libfuzzer_sys::fuzz_target;
use aegis_node::cache::{generate_cache_key, generate_cache_key_unchecked, sanitize_cache_key_component};

fuzz_target!(|data: &[u8]| {
    // Try to interpret data as UTF-8 for URI
    if let Ok(uri) = std::str::from_utf8(data) {
        // Test cache key generation with various methods
        let methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

        for method in methods {
            // Test checked generation (returns Result)
            let result = generate_cache_key(method, uri);

            // Verify the result doesn't contain dangerous characters
            if let Ok(key) = &result {
                assert!(!key.contains('\r'), "Key contains CR");
                assert!(!key.contains('\n'), "Key contains LF");
                assert!(!key.contains('\0'), "Key contains NULL");
            }

            // Test unchecked generation (always returns a string)
            let key = generate_cache_key_unchecked(method, uri);
            assert!(!key.contains('\r'), "Unchecked key contains CR");
            assert!(!key.contains('\n'), "Unchecked key contains LF");
            assert!(!key.contains('\0'), "Unchecked key contains NULL");
        }

        // Test standalone sanitization
        let sanitized = sanitize_cache_key_component(uri, 1024);
        assert!(!sanitized.contains('\r'));
        assert!(!sanitized.contains('\n'));
        assert!(!sanitized.contains('\0'));
        assert!(sanitized.len() <= 1024);

        // Test with very small max length
        let tiny = sanitize_cache_key_component(uri, 10);
        assert!(tiny.len() <= 10);
    }
});
