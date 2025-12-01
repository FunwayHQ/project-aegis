//! Sprint 25: Performance Benchmarks for AEGIS Node
//!
//! Run with: cargo bench
//! Generate HTML report: cargo criterion
//!
//! Performance Targets:
//! - WAF analysis: < 100μs per request
//! - TLS fingerprint: < 10μs
//! - Route matching: < 1μs
//! - CRDT operations: < 1μs

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::collections::HashMap;

// Import AEGIS components
use aegis_node::waf::{AegisWaf, WafConfig};
use aegis_node::tls_fingerprint::{ClientHello, TlsFingerprint, TlsVersion};
use aegis_node::distributed_counter::DistributedCounter;
use aegis_node::route_config::{RouteConfig, Route, RoutePattern, MethodMatcher, WasmModuleRef, CompiledRouteConfig};
use aegis_node::wasm_runtime::{WasmRuntime, WasmModuleType};
use aegis_node::cache::{generate_cache_key, CacheControl};

// =============================================================================
// WAF BENCHMARKS
// =============================================================================

/// Benchmark WAF rule matching performance
/// Target: < 100μs per request
fn bench_waf_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("WAF");

    // Create WAF with default config
    let config = WafConfig::default();
    let waf = AegisWaf::new(config);

    // Test headers
    let headers: Vec<(String, String)> = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
    ];

    group.throughput(Throughput::Elements(1));

    // Clean request
    group.bench_function("clean_request", |b| {
        b.iter(|| {
            waf.analyze_request(
                black_box("GET"),
                black_box("/api/users"),
                black_box(&headers),
                black_box(None),
            )
        })
    });

    // SQLi attack
    group.bench_function("sqli_detection", |b| {
        b.iter(|| {
            waf.analyze_request(
                black_box("GET"),
                black_box("/login?username=admin' OR '1'='1"),
                black_box(&headers),
                black_box(None),
            )
        })
    });

    // XSS attack
    group.bench_function("xss_detection", |b| {
        b.iter(|| {
            waf.analyze_request(
                black_box("GET"),
                black_box("/search?q=<script>alert(1)</script>"),
                black_box(&headers),
                black_box(None),
            )
        })
    });

    // With body
    let body = b"username=admin&password=test123";
    group.bench_function("with_body", |b| {
        b.iter(|| {
            waf.analyze_request(
                black_box("POST"),
                black_box("/api/login"),
                black_box(&headers),
                black_box(Some(body.as_slice())),
            )
        })
    });

    // Large body (1KB)
    let large_body = vec![b'a'; 1024];
    group.bench_function("large_body_1kb", |b| {
        b.iter(|| {
            waf.analyze_request(
                black_box("POST"),
                black_box("/api/data"),
                black_box(&headers),
                black_box(Some(large_body.as_slice())),
            )
        })
    });

    group.finish();
}

// =============================================================================
// TLS FINGERPRINT BENCHMARKS
// =============================================================================

/// Create a sample ClientHello for benchmarking
fn create_sample_client_hello() -> ClientHello {
    ClientHello {
        record_version: TlsVersion::Tls12,
        handshake_version: TlsVersion::Tls13,
        cipher_suites: vec![
            0x1301, // TLS_AES_128_GCM_SHA256
            0x1302, // TLS_AES_256_GCM_SHA384
            0x1303, // TLS_CHACHA20_POLY1305_SHA256
            0xc02c, // TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384
            0xc02b, // TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256
            0xc030, // TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384
            0xc02f, // TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
        ],
        extensions: vec![
            0x0000, // server_name
            0x0017, // extended_master_secret
            0x0023, // session_ticket
            0x000d, // signature_algorithms
            0x002b, // supported_versions
            0x002d, // psk_key_exchange_modes
            0x0033, // key_share
        ],
        elliptic_curves: vec![0x001d, 0x0017, 0x0018], // x25519, secp256r1, secp384r1
        ec_point_formats: vec![0], // uncompressed
        sni: Some("api.example.com".to_string()),
        alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
        signature_algorithms: vec![
            0x0403, // ecdsa_secp256r1_sha256
            0x0503, // ecdsa_secp384r1_sha384
            0x0804, // rsa_pss_rsae_sha256
        ],
        supported_versions: vec![0x0304, 0x0303], // TLS 1.3, TLS 1.2
    }
}

/// Benchmark TLS fingerprint computation
/// Target: < 10μs per fingerprint
fn bench_tls_fingerprint(c: &mut Criterion) {
    let mut group = c.benchmark_group("TLSFingerprint");

    let client_hello = create_sample_client_hello();

    group.throughput(Throughput::Elements(1));

    // Full fingerprint (JA3 + JA4)
    group.bench_function("compute_full", |b| {
        b.iter(|| {
            TlsFingerprint::from_client_hello(black_box(&client_hello))
        })
    });

    // Minimal ClientHello
    let minimal_ch = ClientHello {
        record_version: TlsVersion::Tls12,
        handshake_version: TlsVersion::Tls12,
        cipher_suites: vec![0xc02f],
        extensions: vec![0x0000],
        elliptic_curves: vec![],
        ec_point_formats: vec![],
        sni: None,
        alpn_protocols: vec![],
        signature_algorithms: vec![],
        supported_versions: vec![],
    };

    group.bench_function("compute_minimal", |b| {
        b.iter(|| {
            TlsFingerprint::from_client_hello(black_box(&minimal_ch))
        })
    });

    // Large ClientHello (many ciphers/extensions)
    let mut large_ch = client_hello.clone();
    large_ch.cipher_suites = (0..50).map(|i| 0x1301 + i).collect();
    large_ch.extensions = (0..30).collect();

    group.bench_function("compute_large", |b| {
        b.iter(|| {
            TlsFingerprint::from_client_hello(black_box(&large_ch))
        })
    });

    group.finish();
}

// =============================================================================
// ROUTE MATCHING BENCHMARKS
// =============================================================================

/// Create sample routes for benchmarking
fn create_sample_routes() -> RouteConfig {
    RouteConfig {
        routes: vec![
            // High priority exact match
            Route {
                name: Some("health_check".to_string()),
                path: RoutePattern::Exact("/health".to_string()),
                methods: MethodMatcher::Single("GET".to_string()),
                headers: None,
                wasm_modules: vec![],
                priority: 100,
                enabled: true,
            },
            // API prefix routes
            Route {
                name: Some("api_v1".to_string()),
                path: RoutePattern::Prefix("/api/v1/*".to_string()),
                methods: MethodMatcher::Multiple(vec![
                    "GET".to_string(),
                    "POST".to_string(),
                    "PUT".to_string(),
                    "DELETE".to_string(),
                ]),
                headers: None,
                wasm_modules: vec![
                    WasmModuleRef {
                        module_type: "waf".to_string(),
                        module_id: "security-waf".to_string(),
                        ipfs_cid: None,
                        required_public_key: None,
                    },
                ],
                priority: 50,
                enabled: true,
            },
            // Regex route
            Route {
                name: Some("versioned_api".to_string()),
                path: RoutePattern::Regex(r"^/api/v[0-9]+/users/[0-9]+$".to_string()),
                methods: MethodMatcher::default(),
                headers: None,
                wasm_modules: vec![],
                priority: 75,
                enabled: true,
            },
            // Catch-all
            Route {
                name: Some("catch_all".to_string()),
                path: RoutePattern::Prefix("/*".to_string()),
                methods: MethodMatcher::default(),
                headers: None,
                wasm_modules: vec![],
                priority: 1,
                enabled: true,
            },
        ],
        default_modules: None,
        settings: None,
    }
}

/// Benchmark route matching
/// Target: < 1μs per match
fn bench_route_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("RouteMatching");

    let config = create_sample_routes();
    let empty_headers: Vec<(String, String)> = vec![];

    group.throughput(Throughput::Elements(1));

    // Exact match (fastest)
    group.bench_function("exact_match", |b| {
        b.iter(|| {
            config.find_matching_route(
                black_box("GET"),
                black_box("/health"),
                black_box(&empty_headers),
            )
        })
    });

    // Prefix match
    group.bench_function("prefix_match", |b| {
        b.iter(|| {
            config.find_matching_route(
                black_box("POST"),
                black_box("/api/v1/users"),
                black_box(&empty_headers),
            )
        })
    });

    // Regex match
    group.bench_function("regex_match", |b| {
        b.iter(|| {
            config.find_matching_route(
                black_box("GET"),
                black_box("/api/v2/users/12345"),
                black_box(&empty_headers),
            )
        })
    });

    // Catch-all (worst case - checks all routes)
    group.bench_function("catch_all", |b| {
        b.iter(|| {
            config.find_matching_route(
                black_box("GET"),
                black_box("/some/random/path"),
                black_box(&empty_headers),
            )
        })
    });

    // No match
    let config_no_catchall = RouteConfig {
        routes: config.routes.iter()
            .filter(|r| r.name != Some("catch_all".to_string()))
            .cloned()
            .collect(),
        default_modules: None,
        settings: None,
    };

    group.bench_function("no_match", |b| {
        b.iter(|| {
            config_no_catchall.find_matching_route(
                black_box("DELETE"),
                black_box("/nonexistent/path"),
                black_box(&empty_headers),
            )
        })
    });

    // With header matching
    let mut header_map = HashMap::new();
    header_map.insert("X-API-Key".to_string(), "secret".to_string());

    let config_with_headers = RouteConfig {
        routes: vec![
            Route {
                name: Some("authenticated".to_string()),
                path: RoutePattern::Prefix("/admin/*".to_string()),
                methods: MethodMatcher::default(),
                headers: Some(header_map),
                wasm_modules: vec![],
                priority: 100,
                enabled: true,
            },
        ],
        default_modules: None,
        settings: None,
    };

    let request_headers = vec![
        ("X-API-Key".to_string(), "secret".to_string()),
        ("User-Agent".to_string(), "test".to_string()),
    ];

    group.bench_function("header_match", |b| {
        b.iter(|| {
            config_with_headers.find_matching_route(
                black_box("GET"),
                black_box("/admin/dashboard"),
                black_box(&request_headers),
            )
        })
    });

    group.finish();
}

/// Benchmark COMPILED route matching (Sprint 25 optimization)
/// Target: < 1μs per match (even for regex)
fn bench_compiled_route_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("CompiledRouteMatching");

    let config = create_sample_routes();
    let compiled = config.compile();
    let empty_headers: Vec<(String, String)> = vec![];

    group.throughput(Throughput::Elements(1));

    // Exact match (fastest)
    group.bench_function("exact_match", |b| {
        b.iter(|| {
            compiled.find_matching_route(
                black_box("GET"),
                black_box("/health"),
                black_box(&empty_headers),
            )
        })
    });

    // Prefix match
    group.bench_function("prefix_match", |b| {
        b.iter(|| {
            compiled.find_matching_route(
                black_box("POST"),
                black_box("/api/v1/users"),
                black_box(&empty_headers),
            )
        })
    });

    // Regex match (this should be much faster with compiled regex!)
    group.bench_function("regex_match", |b| {
        b.iter(|| {
            compiled.find_matching_route(
                black_box("GET"),
                black_box("/api/v2/users/12345"),
                black_box(&empty_headers),
            )
        })
    });

    // Catch-all
    group.bench_function("catch_all", |b| {
        b.iter(|| {
            compiled.find_matching_route(
                black_box("GET"),
                black_box("/some/random/path"),
                black_box(&empty_headers),
            )
        })
    });

    // No match scenario
    let config_no_catchall = RouteConfig {
        routes: config.routes.iter()
            .filter(|r| r.name != Some("catch_all".to_string()))
            .cloned()
            .collect(),
        default_modules: None,
        settings: None,
    };
    let compiled_no_catchall = config_no_catchall.compile();

    group.bench_function("no_match", |b| {
        b.iter(|| {
            compiled_no_catchall.find_matching_route(
                black_box("DELETE"),
                black_box("/nonexistent/path"),
                black_box(&empty_headers),
            )
        })
    });

    // With header matching
    let mut header_map = HashMap::new();
    header_map.insert("X-API-Key".to_string(), "secret".to_string());

    let config_with_headers = RouteConfig {
        routes: vec![
            Route {
                name: Some("authenticated".to_string()),
                path: RoutePattern::Prefix("/admin/*".to_string()),
                methods: MethodMatcher::default(),
                headers: Some(header_map),
                wasm_modules: vec![],
                priority: 100,
                enabled: true,
            },
        ],
        default_modules: None,
        settings: None,
    };
    let compiled_with_headers = config_with_headers.compile();

    let request_headers = vec![
        ("X-API-Key".to_string(), "secret".to_string()),
        ("User-Agent".to_string(), "test".to_string()),
    ];

    group.bench_function("header_match", |b| {
        b.iter(|| {
            compiled_with_headers.find_matching_route(
                black_box("GET"),
                black_box("/admin/dashboard"),
                black_box(&request_headers),
            )
        })
    });

    group.finish();
}

// =============================================================================
// CRDT BENCHMARKS
// =============================================================================

/// Benchmark CRDT operations (distributed counter)
/// Target: < 1μs per operation
fn bench_crdt_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("CRDT");

    group.throughput(Throughput::Elements(1));

    // Increment
    group.bench_function("increment", |b| {
        let counter = DistributedCounter::new(1);
        b.iter(|| {
            counter.increment(black_box(1)).unwrap()
        })
    });

    // Read
    group.bench_function("read", |b| {
        let counter = DistributedCounter::new(1);
        // Pre-populate
        for _ in 0..100 {
            counter.increment(1).ok();
        }
        b.iter(|| {
            counter.value().unwrap()
        })
    });

    // Merge
    group.bench_function("merge", |b| {
        let counter1 = DistributedCounter::new(1);
        let counter2 = DistributedCounter::new(2);

        let op = counter2.increment(5).unwrap();

        b.iter(|| {
            counter1.merge_op(black_box(op.clone())).unwrap()
        })
    });

    // Burst increments
    group.bench_function("burst_increments", |b| {
        b.iter(|| {
            let counter = DistributedCounter::new(1);
            for _ in 0..100 {
                counter.increment(black_box(1)).ok();
            }
            counter.value().unwrap()
        })
    });

    group.finish();
}

// =============================================================================
// HTTP OPERATIONS BENCHMARKS
// =============================================================================

/// Benchmark HTTP header operations (common hot path)
/// Target: < 1μs per operation
fn bench_http_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("HTTPOps");

    let headers: Vec<(&str, &str)> = vec![
        ("Host", "example.com"),
        ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"),
        ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Accept-Encoding", "gzip, deflate, br"),
        ("Connection", "keep-alive"),
        ("Cookie", "session=abc123xyz789; user=john; theme=dark"),
        ("X-Forwarded-For", "192.168.1.1, 10.0.0.1, 172.16.0.1"),
        ("X-Request-ID", "550e8400-e29b-41d4-a716-446655440000"),
        ("Authorization", "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"),
    ];

    group.throughput(Throughput::Elements(1));

    // Linear search
    group.bench_function("header_lookup_linear", |b| {
        b.iter(|| {
            headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(black_box("Authorization")))
        })
    });

    // HashMap construction and lookup
    group.bench_function("header_to_hashmap_and_lookup", |b| {
        b.iter(|| {
            let map: HashMap<String, String> = headers
                .iter()
                .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                .collect();
            map.get(black_box("authorization")).cloned()
        })
    });

    // Pre-built HashMap lookup
    let header_map: HashMap<String, &str> = headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), *v))
        .collect();

    group.bench_function("header_hashmap_lookup_prebuilt", |b| {
        b.iter(|| {
            header_map.get(black_box("authorization"))
        })
    });

    // URL query parsing
    let urls = vec![
        "/api/users?page=1&limit=10",
        "/search?q=rust+programming&category=books&sort=relevance&order=desc",
        "/products/123/reviews?rating=5&verified=true&sort=newest&page=1&limit=25",
    ];

    for (i, url) in urls.iter().enumerate() {
        group.bench_with_input(BenchmarkId::new("parse_query", i), url, |b, u| {
            b.iter(|| {
                let query_start = u.find('?').unwrap_or(u.len());
                let query = &u[query_start..];
                let params: Vec<(&str, &str)> = query
                    .trim_start_matches('?')
                    .split('&')
                    .filter_map(|p| {
                        let mut parts = p.splitn(2, '=');
                        Some((parts.next()?, parts.next().unwrap_or("")))
                    })
                    .collect();
                black_box(params)
            })
        });
    }

    // IP extraction from X-Forwarded-For
    group.bench_function("extract_client_ip", |b| {
        let xff = "192.168.1.1, 10.0.0.1, 172.16.0.1";
        b.iter(|| {
            xff.split(',')
                .next()
                .map(|s| s.trim())
        })
    });

    group.finish();
}

// =============================================================================
// JSON OPERATIONS BENCHMARKS
// =============================================================================

/// Benchmark JSON operations
fn bench_json_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("JSONOps");

    // Small JSON
    let json_small = r#"{"id": 123, "name": "test"}"#;

    // Medium JSON
    let json_medium = r#"{
        "user": {"id": 123, "name": "John Doe", "email": "john@example.com"},
        "action": "update",
        "timestamp": 1700000000,
        "metadata": {"source": "api", "version": "1.0"}
    }"#;

    // Large JSON
    let json_large = serde_json::json!({
        "users": (0..100).map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("User {}", i),
                "email": format!("user{}@example.com", i),
                "roles": ["user", "member"]
            })
        }).collect::<Vec<_>>(),
        "total": 100,
        "page": 1
    }).to_string();

    group.bench_function("parse_json_small", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(black_box(json_small)).unwrap()
        })
    });

    group.bench_function("parse_json_medium", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(black_box(json_medium)).unwrap()
        })
    });

    group.bench_function("parse_json_large", |b| {
        b.iter(|| {
            serde_json::from_str::<serde_json::Value>(black_box(&json_large)).unwrap()
        })
    });

    // Serialization benchmarks
    let waf_config = WafConfig::default();
    let route_config = create_sample_routes();
    let fingerprint = TlsFingerprint::from_client_hello(&create_sample_client_hello());

    group.bench_function("serialize_waf_config", |b| {
        b.iter(|| {
            serde_json::to_string(black_box(&waf_config)).unwrap()
        })
    });

    group.bench_function("serialize_route_config", |b| {
        b.iter(|| {
            serde_json::to_string(black_box(&route_config)).unwrap()
        })
    });

    group.bench_function("serialize_fingerprint", |b| {
        b.iter(|| {
            serde_json::to_string(black_box(&fingerprint)).unwrap()
        })
    });

    group.finish();
}

// =============================================================================
// WASM RUNTIME BENCHMARKS
// =============================================================================

/// Benchmark Wasm runtime operations
/// Targets:
/// - Module compilation: < 10ms (one-time cost)
/// - Module instantiation: < 100μs
/// - Simple function call: < 10μs
fn bench_wasm_runtime(c: &mut Criterion) {
    let mut group = c.benchmark_group("WasmRuntime");

    // Create minimal Wasm modules using wat
    let simple_module_wat = r#"
        (module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add
            )
            (func (export "identity") (param i32) (result i32)
                local.get 0
            )
        )
    "#;

    let memory_module_wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "alloc") (param $size i32) (result i32)
                i32.const 0
            )
            (func (export "sum_array") (param $ptr i32) (param $len i32) (result i32)
                (local $sum i32)
                (local $i i32)
                (local.set $sum (i32.const 0))
                (local.set $i (i32.const 0))
                (block $break
                    (loop $continue
                        (br_if $break (i32.ge_u (local.get $i) (local.get $len)))
                        (local.set $sum
                            (i32.add
                                (local.get $sum)
                                (i32.load (i32.add (local.get $ptr) (i32.mul (local.get $i) (i32.const 4))))
                            )
                        )
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $continue)
                    )
                )
                (local.get $sum)
            )
        )
    "#;

    // Compile WAT to Wasm bytes once for benchmarks
    let simple_wasm = wat::parse_str(simple_module_wat).expect("Failed to parse simple WAT");
    let memory_wasm = wat::parse_str(memory_module_wat).expect("Failed to parse memory WAT");

    group.throughput(Throughput::Elements(1));

    // Benchmark runtime creation
    group.bench_function("runtime_creation", |b| {
        b.iter(|| {
            WasmRuntime::new().expect("Failed to create runtime")
        })
    });

    // Benchmark module compilation (wasmtime Engine compilation)
    let runtime = WasmRuntime::new().expect("Failed to create runtime");

    group.bench_function("module_load_simple", |b| {
        b.iter(|| {
            let rt = WasmRuntime::new().expect("Failed to create runtime");
            rt.load_module_from_bytes(
                black_box("bench-simple"),
                black_box(&simple_wasm),
                WasmModuleType::EdgeFunction,
                None,
            ).expect("Failed to load module")
        })
    });

    group.bench_function("module_load_with_memory", |b| {
        b.iter(|| {
            let rt = WasmRuntime::new().expect("Failed to create runtime");
            rt.load_module_from_bytes(
                black_box("bench-memory"),
                black_box(&memory_wasm),
                WasmModuleType::EdgeFunction,
                None,
            ).expect("Failed to load module")
        })
    });

    // Benchmark listing modules (lock acquisition + HashMap iteration)
    runtime.load_module_from_bytes("test-1", &simple_wasm, WasmModuleType::EdgeFunction, None).ok();
    runtime.load_module_from_bytes("test-2", &simple_wasm, WasmModuleType::EdgeFunction, None).ok();
    runtime.load_module_from_bytes("test-3", &simple_wasm, WasmModuleType::EdgeFunction, None).ok();

    group.bench_function("list_modules", |b| {
        b.iter(|| {
            runtime.list_modules().expect("Failed to list modules")
        })
    });

    // Benchmark signature verification (Ed25519)
    use ed25519_dalek::{SigningKey, Signer};
    let signing_key = SigningKey::from_bytes(&[
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60,
        0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
        0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19,
        0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
    ]);
    let verifying_key = signing_key.verifying_key();
    let signature = signing_key.sign(&simple_wasm);
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(verifying_key.to_bytes());

    group.bench_function("signature_verification", |b| {
        b.iter(|| {
            WasmRuntime::verify_module_signature(
                black_box(&simple_wasm),
                black_box(&signature_hex),
                black_box(&public_key_hex),
            ).expect("Verification should succeed")
        })
    });

    // Benchmark module load with signature verification
    group.bench_function("module_load_with_signature", |b| {
        b.iter(|| {
            let rt = WasmRuntime::new().expect("Failed to create runtime");
            rt.load_module_from_bytes_with_signature(
                black_box("bench-signed"),
                black_box(&simple_wasm),
                WasmModuleType::EdgeFunction,
                None,
                Some(signature_hex.clone()),
                Some(public_key_hex.clone()),
            ).expect("Failed to load signed module")
        })
    });

    group.finish();
}

// =============================================================================
// CACHE OPERATIONS BENCHMARKS
// =============================================================================

/// Benchmark cache-related operations (synchronous parts)
/// Target: < 1μs for key generation and header parsing
fn bench_cache_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("CacheOps");

    group.throughput(Throughput::Elements(1));

    // Benchmark cache key generation
    group.bench_function("generate_key_short", |b| {
        b.iter(|| {
            generate_cache_key(black_box("GET"), black_box("/api/users"))
        })
    });

    group.bench_function("generate_key_long", |b| {
        b.iter(|| {
            generate_cache_key(
                black_box("POST"),
                black_box("/api/v2/organizations/12345/projects/67890/deployments/latest/status"),
            )
        })
    });

    // Benchmark Cache-Control header parsing
    let cache_control_headers = vec![
        ("empty", ""),
        ("simple", "public"),
        ("max_age", "max-age=3600"),
        ("complex", "public, max-age=31536000, immutable"),
        ("no_store", "no-cache, no-store, must-revalidate"),
        ("real_world", "private, max-age=0, no-cache, no-store, must-revalidate, pre-check=0, post-check=0"),
    ];

    for (name, header) in &cache_control_headers {
        group.bench_with_input(BenchmarkId::new("parse_cache_control", *name), header, |b, h| {
            b.iter(|| CacheControl::parse(black_box(h)))
        });
    }

    // Benchmark should_cache decision
    let control = CacheControl::parse("public, max-age=3600");
    group.bench_function("should_cache", |b| {
        b.iter(|| control.should_cache())
    });

    // Benchmark effective_ttl calculation
    group.bench_function("effective_ttl", |b| {
        b.iter(|| control.effective_ttl(black_box(60)))
    });

    group.finish();
}

// =============================================================================
// REGEX OPERATIONS BENCHMARKS
// =============================================================================

/// Benchmark regex operations (used in WAF and routing)
fn bench_regex_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("Regex");

    // Pre-compiled regexes (WAF patterns)
    let sqli_regex = regex::Regex::new(r"(?i)(union\s+select|or\s+1\s*=\s*1|'\s*or\s*'|--\s*$)").unwrap();
    let xss_regex = regex::Regex::new(r"(?i)(<script|javascript:|onerror\s*=|onload\s*=)").unwrap();
    let path_regex = regex::Regex::new(r"\.\.\/|\.\.\\").unwrap();

    let test_inputs = vec![
        ("clean", "SELECT * FROM users WHERE id = 123"),
        ("sqli_union", "1 UNION SELECT username, password FROM users"),
        ("sqli_or", "admin' OR '1'='1"),
        ("xss_script", "<script>alert('xss')</script>"),
        ("xss_event", "<img onerror=alert(1)>"),
        ("path_traversal", "../../../etc/passwd"),
    ];

    group.throughput(Throughput::Elements(1));

    for (name, input) in &test_inputs {
        group.bench_with_input(BenchmarkId::new("sqli_check", *name), input, |b, i| {
            b.iter(|| sqli_regex.is_match(black_box(i)))
        });
    }

    for (name, input) in &test_inputs {
        group.bench_with_input(BenchmarkId::new("xss_check", *name), input, |b, i| {
            b.iter(|| xss_regex.is_match(black_box(i)))
        });
    }

    for (name, input) in &test_inputs {
        group.bench_with_input(BenchmarkId::new("path_check", *name), input, |b, i| {
            b.iter(|| path_regex.is_match(black_box(i)))
        });
    }

    group.finish();
}

// =============================================================================
// CRITERION GROUPS
// =============================================================================

criterion_group!(
    benches,
    bench_waf_analysis,
    bench_tls_fingerprint,
    bench_route_matching,
    bench_compiled_route_matching,
    bench_crdt_operations,
    bench_http_operations,
    bench_json_operations,
    bench_wasm_runtime,
    bench_cache_operations,
    bench_regex_operations,
);

criterion_main!(benches);
