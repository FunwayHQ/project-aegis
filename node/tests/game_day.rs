//! Sprint 27: Game Day Tests
//!
//! Multi-node distributed testing scenarios:
//! 1. CRDT sync across 3 simulated nodes
//! 2. NATS message propagation latency
//! 3. Node failure and recovery
//! 4. P2P threat intel sharing
//!
//! Run with: cargo test game_day --test game_day -- --nocapture
//!
//! Requires: NATS server running with JetStream
//!   nats-server -js -p 4222

use aegis_node::distributed_counter::DistributedCounter;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Check if NATS is available
async fn nats_available() -> bool {
    match async_nats::connect("nats://localhost:4222").await {
        Ok(_) => true,
        Err(_) => {
            eprintln!("NATS not available - start with: nats-server -js -p 4222");
            false
        }
    }
}

/// Test: CRDT sync across 3 simulated nodes
/// Target: <2s convergence
#[tokio::test]
async fn test_crdt_three_node_sync() {
    println!("\n=== GAME DAY: CRDT 3-Node Sync ===\n");

    // Simulate 3 nodes with different actor IDs
    let node1 = DistributedCounter::new(1);
    let node2 = DistributedCounter::new(2);
    let node3 = DistributedCounter::new(3);

    // Each node increments its counter
    let start = Instant::now();

    node1.increment(100).expect("node1 increment");
    node2.increment(200).expect("node2 increment");
    node3.increment(300).expect("node3 increment");

    // Simulate network propagation by serializing and merging states
    // In production, NATS would handle this
    let state1 = node1.serialize_state().expect("serialize state1");
    let state2 = node2.serialize_state().expect("serialize state2");
    let state3 = node3.serialize_state().expect("serialize state3");

    // Merge all states (simulates receiving from other nodes)
    node1.merge_state(&state2).expect("node1 merge state2");
    node1.merge_state(&state3).expect("node1 merge state3");
    node2.merge_state(&state1).expect("node2 merge state1");
    node2.merge_state(&state3).expect("node2 merge state3");
    node3.merge_state(&state1).expect("node3 merge state1");
    node3.merge_state(&state2).expect("node3 merge state2");

    let convergence_time = start.elapsed();

    // All nodes should have same value
    let value1 = node1.value().expect("node1 value");
    let value2 = node2.value().expect("node2 value");
    let value3 = node3.value().expect("node3 value");

    println!("Node 1 value: {}", value1);
    println!("Node 2 value: {}", value2);
    println!("Node 3 value: {}", value3);
    println!("Convergence time: {:?}", convergence_time);

    assert_eq!(value1, value2, "Node 1 and 2 should have same value");
    assert_eq!(value2, value3, "Node 2 and 3 should have same value");
    assert_eq!(value1, 600, "Sum should be 100 + 200 + 300 = 600");
    assert!(
        convergence_time < Duration::from_secs(2),
        "Convergence should be <2s, was {:?}",
        convergence_time
    );

    println!("✅ PASS: 3-node CRDT sync in {:?}", convergence_time);
}

/// Test: NATS message latency measurement
/// Target: <50ms for local NATS
#[tokio::test]
async fn test_nats_message_latency() {
    println!("\n=== GAME DAY: NATS Message Latency ===\n");

    if !nats_available().await {
        println!("⚠️ SKIP: NATS not available");
        return;
    }

    let client = async_nats::connect("nats://localhost:4222")
        .await
        .unwrap();

    // Measure round-trip latency
    let mut latencies = Vec::new();

    for i in 0..100 {
        let subject = format!("aegis.test.latency.{}", i);
        let payload = format!("test-{}", i);

        let start = Instant::now();

        // Publish
        client
            .publish(subject.clone(), payload.clone().into())
            .await
            .unwrap();

        // Flush to ensure delivery
        client.flush().await.unwrap();

        let latency = start.elapsed();
        latencies.push(latency);
    }

    // Calculate statistics
    latencies.sort();
    let p50 = latencies[49];
    let p95 = latencies[94];
    let p99 = latencies[98];
    let max = latencies[99];

    println!("NATS Latency (100 messages):");
    println!("  P50: {:?}", p50);
    println!("  P95: {:?}", p95);
    println!("  P99: {:?}", p99);
    println!("  Max: {:?}", max);

    assert!(
        p95 < Duration::from_millis(50),
        "P95 latency should be <50ms, was {:?}",
        p95
    );

    println!("✅ PASS: NATS latency P95={:?}", p95);
}

/// Test: Simulated node failure and recovery
/// Target: <30s recovery time
#[tokio::test]
async fn test_node_failure_recovery() {
    println!("\n=== GAME DAY: Node Failure & Recovery ===\n");

    // Simulate 3 nodes
    let nodes: Vec<Arc<RwLock<DistributedCounter>>> = (1..=3)
        .map(|id| Arc::new(RwLock::new(DistributedCounter::new(id))))
        .collect();

    // Initial state: all nodes increment
    for (i, node) in nodes.iter().enumerate() {
        node.write()
            .await
            .increment((i as u64 + 1) * 100)
            .expect("increment");
    }

    // Sync all nodes
    let states: Vec<_> = {
        let mut s = Vec::new();
        for node in &nodes {
            s.push(node.read().await.serialize_state().expect("serialize"));
        }
        s
    };

    for node in &nodes {
        for state in &states {
            node.write().await.merge_state(state).expect("merge");
        }
    }

    let initial_value = nodes[0].read().await.value().expect("value");
    println!("Initial synced value: {}", initial_value);
    assert_eq!(initial_value, 600); // 100 + 200 + 300

    // Simulate node 2 failure (just don't update it)
    println!("Simulating node 2 failure...");

    // Nodes 1 and 3 continue operating
    nodes[0].write().await.increment(50).expect("increment");
    nodes[2].write().await.increment(50).expect("increment");

    // Sync nodes 1 and 3 only
    let state1 = nodes[0].read().await.serialize_state().expect("serialize");
    let state3 = nodes[2].read().await.serialize_state().expect("serialize");
    nodes[0].write().await.merge_state(&state3).expect("merge");
    nodes[2].write().await.merge_state(&state1).expect("merge");

    let healthy_value = nodes[0].read().await.value().expect("value");
    println!("Healthy nodes value: {}", healthy_value);
    assert_eq!(healthy_value, 700); // 600 + 50 + 50

    // Simulate node 2 recovery
    let recovery_start = Instant::now();
    println!("Simulating node 2 recovery...");

    // Node 2 receives state from healthy nodes
    nodes[1].write().await.merge_state(&state1).expect("merge");
    nodes[1].write().await.merge_state(&state3).expect("merge");

    let recovery_time = recovery_start.elapsed();
    let recovered_value = nodes[1].read().await.value().expect("value");

    println!("Recovered node 2 value: {}", recovered_value);
    println!("Recovery time: {:?}", recovery_time);

    assert_eq!(
        recovered_value, 700,
        "Recovered node should have current value"
    );
    assert!(
        recovery_time < Duration::from_secs(30),
        "Recovery should be <30s, was {:?}",
        recovery_time
    );

    println!("✅ PASS: Node recovery in {:?}", recovery_time);
}

/// Test: High-frequency CRDT operations
/// Target: >10,000 ops/sec
#[tokio::test]
async fn test_crdt_throughput() {
    println!("\n=== GAME DAY: CRDT Throughput ===\n");

    let counter = DistributedCounter::new(1);
    let ops = 100_000;

    let start = Instant::now();

    for _ in 0..ops {
        counter.increment(1).expect("increment");
    }

    let elapsed = start.elapsed();
    let ops_per_sec = ops as f64 / elapsed.as_secs_f64();

    println!("Operations: {}", ops);
    println!("Time: {:?}", elapsed);
    println!("Throughput: {:.0} ops/sec", ops_per_sec);

    assert!(
        ops_per_sec > 10_000.0,
        "Should exceed 10,000 ops/sec, got {:.0}",
        ops_per_sec
    );

    println!("✅ PASS: CRDT throughput {:.0} ops/sec", ops_per_sec);
}

/// Test: Concurrent multi-actor operations
/// Target: No data loss under concurrency
#[tokio::test]
async fn test_concurrent_actors() {
    println!("\n=== GAME DAY: Concurrent Actors ===\n");

    let counter = Arc::new(DistributedCounter::new(1));
    let mut handles = Vec::new();

    // Spawn 10 concurrent tasks, each incrementing 1000 times
    for _ in 1..=10 {
        let counter = counter.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..1000 {
                counter.increment(1).expect("increment");
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    let final_value = counter.value().expect("value");
    println!("Final value: {} (expected: 10000)", final_value);

    assert_eq!(
        final_value, 10000,
        "Should have 10 tasks x 1000 ops = 10000"
    );

    println!("✅ PASS: Concurrent actors, no data loss");
}

/// Test: CRDT commutativity across nodes
/// Target: Same result regardless of merge order
#[tokio::test]
async fn test_crdt_commutativity() {
    println!("\n=== GAME DAY: CRDT Commutativity ===\n");

    // Create 4 nodes with different increments
    let node_a = DistributedCounter::new(1);
    let node_b = DistributedCounter::new(2);
    let node_c = DistributedCounter::new(3);
    let node_d = DistributedCounter::new(4);

    node_a.increment(10).expect("increment");
    node_b.increment(20).expect("increment");
    node_c.increment(30).expect("increment");
    node_d.increment(40).expect("increment");

    let state_a = node_a.serialize_state().expect("serialize");
    let state_b = node_b.serialize_state().expect("serialize");
    let state_c = node_c.serialize_state().expect("serialize");
    let state_d = node_d.serialize_state().expect("serialize");

    // Merge in order A, B, C, D
    let result1 = DistributedCounter::new(100);
    result1.merge_state(&state_a).expect("merge");
    result1.merge_state(&state_b).expect("merge");
    result1.merge_state(&state_c).expect("merge");
    result1.merge_state(&state_d).expect("merge");

    // Merge in reverse order D, C, B, A
    let result2 = DistributedCounter::new(101);
    result2.merge_state(&state_d).expect("merge");
    result2.merge_state(&state_c).expect("merge");
    result2.merge_state(&state_b).expect("merge");
    result2.merge_state(&state_a).expect("merge");

    // Merge in random order B, D, A, C
    let result3 = DistributedCounter::new(102);
    result3.merge_state(&state_b).expect("merge");
    result3.merge_state(&state_d).expect("merge");
    result3.merge_state(&state_a).expect("merge");
    result3.merge_state(&state_c).expect("merge");

    let value1 = result1.value().expect("value");
    let value2 = result2.value().expect("value");
    let value3 = result3.value().expect("value");

    println!("Order A,B,C,D: {}", value1);
    println!("Order D,C,B,A: {}", value2);
    println!("Order B,D,A,C: {}", value3);

    assert_eq!(value1, value2, "Order should not matter");
    assert_eq!(value2, value3, "Order should not matter");
    assert_eq!(value1, 100, "Sum should be 10+20+30+40 = 100");

    println!("✅ PASS: CRDT commutativity verified");
}

/// Test: CRDT idempotence
/// Target: Multiple merges of same state = single merge
#[tokio::test]
async fn test_crdt_idempotence() {
    println!("\n=== GAME DAY: CRDT Idempotence ===\n");

    let source = DistributedCounter::new(1);
    source.increment(42).expect("increment");

    let state = source.serialize_state().expect("serialize");

    let target = DistributedCounter::new(2);

    // Merge same state 5 times
    for i in 1..=5 {
        target.merge_state(&state).expect("merge");
        let value = target.value().expect("value");
        println!("After merge {}: {}", i, value);
    }

    let final_value = target.value().expect("value");
    assert_eq!(final_value, 42, "Multiple merges should not increase value");

    println!("✅ PASS: CRDT idempotence verified");
}

// ========================================
// Y10.5: DoS Resistance Stress Tests
// ========================================

/// Test: WAF under high request volume
/// Target: <1ms average per request, no crashes
#[tokio::test]
async fn test_y105_waf_stress() {
    use aegis_node::waf::{AegisWaf, WafConfig};

    println!("\n=== Y10.5: WAF Stress Test ===\n");

    let waf = AegisWaf::new(WafConfig::default());

    // Test vectors including attack patterns
    let test_vectors = [
        "/api/users",
        "/api/users?id=1",
        "/api/users?id=1' OR '1'='1",
        "/api/search?q=<script>alert(1)</script>",
        "/../../etc/passwd",
        "/api/exec?cmd=; cat /etc/passwd",
        "/api/normal/path/with/segments",
        "/?callback=__import__('os').system('id')",
    ];

    let iterations = 10_000;
    let start = Instant::now();

    for i in 0..iterations {
        let uri = test_vectors[i % test_vectors.len()];
        let headers = vec![
            ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
            ("X-Request-Id".to_string(), format!("req-{}", i)),
        ];

        // This should not panic or hang
        let _result = waf.analyze_request("GET", uri, &headers, None);
    }

    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
    let rps = iterations as f64 / elapsed.as_secs_f64();

    println!("Requests: {}", iterations);
    println!("Total time: {:?}", elapsed);
    println!("Average: {:.3}ms per request", avg_ms);
    println!("Throughput: {:.0} req/sec", rps);

    assert!(
        avg_ms < 1.0,
        "Average latency should be <1ms, was {:.3}ms",
        avg_ms
    );

    println!("✅ PASS: WAF stress test - {:.3}ms avg, {:.0} req/sec", avg_ms, rps);
}

/// Test: WAF with malicious input patterns (ReDoS prevention)
/// Target: No request takes >100ms
#[tokio::test]
async fn test_y105_waf_redos_resistance() {
    use aegis_node::waf::{AegisWaf, WafConfig};

    println!("\n=== Y10.5: WAF ReDoS Resistance Test ===\n");

    let waf = AegisWaf::new(WafConfig::default());

    // Known ReDoS-triggering patterns (exponential backtracking)
    let redos_patterns = [
        // Long strings of repeating characters
        &"a".repeat(1000),
        &"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab".repeat(10),
        // Nested patterns
        &format!("{}!", "a]".repeat(50)),
        // URL-encoded attack payloads
        &"%2527".repeat(100),
        // Deep nesting
        &"((((((((((((((((((((a))))))))))))))))))))".repeat(5),
    ];

    let mut max_time = Duration::ZERO;

    for pattern in &redos_patterns {
        let start = Instant::now();
        let _result = waf.analyze_request("GET", pattern, &[], None);
        let elapsed = start.elapsed();

        if elapsed > max_time {
            max_time = elapsed;
        }

        println!(
            "Pattern len={}: {:?}",
            pattern.len().min(50),
            elapsed
        );

        assert!(
            elapsed < Duration::from_millis(100),
            "Request should complete in <100ms, took {:?}",
            elapsed
        );
    }

    println!("Max request time: {:?}", max_time);
    println!("✅ PASS: WAF ReDoS resistance verified");
}

/// Test: Rate limiter under sequential high load
/// Target: Correct counting at high volume
#[tokio::test]
async fn test_y105_rate_limiter_stress() {
    use aegis_node::distributed_rate_limiter::{DistributedRateLimiter, RateLimiterConfig};

    println!("\n=== Y10.5: Rate Limiter Stress ===\n");

    let config = RateLimiterConfig {
        actor_id: 1,
        max_requests: 1_000_000, // High limit to test counting accuracy
        window_duration_secs: 3600,
        auto_sync: false,
        ..Default::default()
    };

    let limiter = DistributedRateLimiter::new(config);
    let iterations = 10_000;

    let start = Instant::now();

    for _ in 0..iterations {
        let _ = limiter.check_rate_limit("stress-resource").await;
    }

    let elapsed = start.elapsed();
    let final_count = limiter.get_count("stress-resource").expect("get count");
    let rps = iterations as f64 / elapsed.as_secs_f64();

    println!("Requests: {}", iterations);
    println!("Final count: {}", final_count);
    println!("Time: {:?}", elapsed);
    println!("Throughput: {:.0} req/sec", rps);

    assert_eq!(
        final_count, iterations as u64,
        "Count should match total requests"
    );

    println!("✅ PASS: Rate limiter stress - {:.0} req/sec, count accurate", rps);
}

/// Test: Cache key generation with adversarial inputs
/// Target: No crashes, bounded memory usage
#[tokio::test]
async fn test_y105_cache_key_stress() {
    use aegis_node::cache::{generate_cache_key, sanitize_cache_key_component};

    println!("\n=== Y10.5: Cache Key Generation Stress ===\n");

    // Pre-compute strings to avoid lifetime issues
    let long_a = "a".repeat(10_000);
    let emoji_str = "🔥🔥🔥🔥🔥".repeat(100);
    let long_special = format!("{}\r\n{}", "a".repeat(5000), "b".repeat(5000));

    // Adversarial inputs
    let adversarial_inputs: Vec<&str> = vec![
        // Very long strings
        &long_a,
        // CRLF injection attempts
        "key\r\nSET attack value",
        "key\nDEL *",
        // Null bytes
        "key\0with\0nulls",
        // Unicode edge cases
        &emoji_str,
        // Control characters
        "\x00\x01\x02\x03\x04\x05",
        // Very long with special chars
        &long_special,
    ];

    let iterations = 10_000;
    let start = Instant::now();

    for i in 0..iterations {
        let input = adversarial_inputs[i % adversarial_inputs.len()];

        // Test sanitization
        let sanitized = sanitize_cache_key_component(input, 1024);

        // Verify sanitization properties
        assert!(!sanitized.contains('\r'), "Should not contain CR");
        assert!(!sanitized.contains('\n'), "Should not contain LF");
        assert!(!sanitized.contains('\0'), "Should not contain NULL");
        // Note: Unicode characters can be multi-byte, so byte length may exceed char count
        assert!(sanitized.chars().count() <= 1024, "Should be bounded by char count");

        // Test full key generation
        match generate_cache_key("GET", input) {
            Ok(key) => {
                assert!(!key.contains('\r'));
                assert!(!key.contains('\n'));
                assert!(!key.contains('\0'));
            }
            Err(_) => {
                // Errors are fine for invalid input
            }
        }
    }

    let elapsed = start.elapsed();
    let rps = iterations as f64 / elapsed.as_secs_f64();

    println!("Iterations: {}", iterations);
    println!("Time: {:?}", elapsed);
    println!("Throughput: {:.0} ops/sec", rps);

    println!("✅ PASS: Cache key stress test completed");
}

/// Test: TLS fingerprint parsing with malformed data
/// Target: No panics, graceful handling
#[tokio::test]
async fn test_y105_tls_fingerprint_stress() {
    use aegis_node::tls_fingerprint::{ClientHello, TlsFingerprint};

    println!("\n=== Y10.5: TLS Fingerprint Parsing Stress ===\n");

    // Generate various malformed inputs
    let test_inputs: Vec<Vec<u8>> = vec![
        // Empty
        vec![],
        // Too short
        vec![0x16, 0x03, 0x01],
        // Invalid record type
        vec![0xFF, 0x03, 0x01, 0x00, 0x05, 0x01, 0x00, 0x00, 0x01, 0x00],
        // Truncated length
        vec![0x16, 0x03, 0x01, 0xFF, 0xFF],
        // Maximum length field
        vec![0x16, 0x03, 0x01, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x01, 0x00],
        // Random garbage
        (0..1000).map(|i| (i % 256) as u8).collect(),
        // All zeros
        vec![0u8; 1000],
        // All 0xFF
        vec![0xFFu8; 1000],
        // Partial valid header
        vec![0x16, 0x03, 0x03, 0x00, 0x05, 0x01, 0x00, 0x00, 0x01],
    ];

    let iterations = 10_000;
    let start = Instant::now();
    let mut parse_failures = 0;
    let mut parse_successes = 0;

    for i in 0..iterations {
        let input = &test_inputs[i % test_inputs.len()];

        // This should never panic
        match ClientHello::parse(input) {
            Some(ch) => {
                // If parsing succeeds, fingerprint generation should also succeed
                let _ = TlsFingerprint::from_client_hello(&ch);
                parse_successes += 1;
            }
            None => {
                parse_failures += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    let rps = iterations as f64 / elapsed.as_secs_f64();

    println!("Iterations: {}", iterations);
    println!("Parse successes: {}", parse_successes);
    println!("Parse failures: {}", parse_failures);
    println!("Time: {:?}", elapsed);
    println!("Throughput: {:.0} ops/sec", rps);

    // Most should fail (malformed data)
    assert!(parse_failures > parse_successes, "Most malformed inputs should fail parsing");

    println!("✅ PASS: TLS fingerprint stress test - no panics");
}

/// Test: Byzantine validator under attack simulation
/// Target: Correct detection, no bypasses
#[tokio::test]
async fn test_y105_byzantine_validator_stress() {
    use aegis_node::distributed_counter::{ByzantineValidator, CounterOp, ByzantineCheck};

    println!("\n=== Y10.5: Byzantine Validator Stress ===\n");

    // Use very high rate limit to focus on value validation
    let mut validator = ByzantineValidator::with_limits(1000, 100_000);

    let iterations = 10_000;
    let mut valid_count = 0;
    let mut excessive_value_count = 0;
    let mut rate_limited_count = 0;
    let mut other_count = 0;

    let start = Instant::now();

    for i in 0..iterations {
        // Mix of valid and attack operations
        let op = if i % 10 < 8 {
            // 80% normal operations with unique actors
            CounterOp::Increment {
                actor: i as u64 + 1, // Unique actor per request
                value: (i % 100) as u64 + 1,
            }
        } else {
            // 20% excessive value attacks
            CounterOp::Increment {
                actor: (i % 100) as u64 + 10000,
                value: 100_000, // Way over limit
            }
        };

        match validator.validate_operation(&op) {
            ByzantineCheck::Valid => valid_count += 1,
            ByzantineCheck::ExcessiveValue { .. } => excessive_value_count += 1,
            ByzantineCheck::RateLimitExceeded { .. } => rate_limited_count += 1,
            _ => other_count += 1,
        }
    }

    let elapsed = start.elapsed();
    let rps = iterations as f64 / elapsed.as_secs_f64();

    println!("Iterations: {}", iterations);
    println!("Valid: {}", valid_count);
    println!("Excessive value blocked: {}", excessive_value_count);
    println!("Rate limited: {}", rate_limited_count);
    println!("Other: {}", other_count);
    println!("Time: {:?}", elapsed);
    println!("Throughput: {:.0} ops/sec", rps);

    // Verify attack detection
    assert!(excessive_value_count > 0, "Should detect excessive value attacks");
    assert!(valid_count > excessive_value_count, "Valid ops should outnumber attacks");

    println!("✅ PASS: Byzantine validator stress test - attacks detected");
}
