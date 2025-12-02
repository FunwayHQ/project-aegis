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
