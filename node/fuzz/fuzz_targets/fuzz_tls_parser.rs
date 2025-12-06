//! Y10.1: Fuzz target for TLS ClientHello parser
//!
//! This fuzzer tests the TLS fingerprint parser with arbitrary byte sequences
//! to find crashes, panics, or memory safety issues in the parsing logic.
//!
//! Run with: cargo +nightly fuzz run fuzz_tls_parser

#![no_main]

use libfuzzer_sys::fuzz_target;
use aegis_node::tls_fingerprint::{ClientHello, TlsFingerprint};

fuzz_target!(|data: &[u8]| {
    // Try to parse arbitrary bytes as a ClientHello
    // This should never panic, only return None for invalid data
    let _ = ClientHello::parse(data);

    // If parsing succeeded, try to generate fingerprints
    if let Some(client_hello) = ClientHello::parse(data) {
        // This computes JA3 and JA4 fingerprints internally
        let fingerprint = TlsFingerprint::from_client_hello(&client_hello);

        // Access all fields to ensure they're valid
        let _ = fingerprint.ja3.len();
        let _ = fingerprint.ja3_raw.len();
        let _ = fingerprint.ja4.len();
        let _ = fingerprint.tls_version;
        let _ = fingerprint.cipher_count;
        let _ = fingerprint.extension_count;
        let _ = fingerprint.has_sni;
        let _ = fingerprint.has_alpn;
    }
});
