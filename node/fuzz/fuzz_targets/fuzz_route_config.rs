//! Y10.3: Fuzz target for Route Config parser
//!
//! This fuzzer tests the YAML and TOML route config parsers with arbitrary
//! strings to find crashes, panics, or memory safety issues.
//!
//! Run with: cargo +nightly fuzz run fuzz_route_config

#![no_main]

use libfuzzer_sys::fuzz_target;
use aegis_node::route_config::RouteConfig;

fuzz_target!(|data: &[u8]| {
    // Try to interpret data as UTF-8 string
    if let Ok(config_str) = std::str::from_utf8(data) {
        // Try parsing as YAML
        if let Ok(config) = RouteConfig::from_yaml(config_str) {
            // If parsing succeeded, validate and compile
            let _ = config.validate();
            let compiled = config.compile();

            // Try matching some requests against the compiled config
            let _ = compiled.find_matching_route("GET", "/api/test", &[]);
            let _ = compiled.find_matching_route("POST", "/", &[]);
            let _ = compiled.find_matching_route(
                "DELETE",
                "/api/users/123/profile",
                &[("Authorization".to_string(), "Bearer token".to_string())],
            );
        }

        // Try parsing as TOML
        if let Ok(config) = RouteConfig::from_toml(config_str) {
            // If parsing succeeded, validate and compile
            let _ = config.validate();
            let compiled = config.compile();

            // Try matching some requests against the compiled config
            let _ = compiled.find_matching_route("GET", "/", &[]);
        }
    }
});
