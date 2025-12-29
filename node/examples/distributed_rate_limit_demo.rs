/// Multi-node simulation demonstrating distributed rate limiting with CRDTs and NATS
///
/// This demo shows how multiple nodes can share rate limiting state through NATS JetStream,
/// with eventual consistency guaranteed by CRDTs.
///
/// Prerequisites:
/// 1. NATS server with JetStream enabled running on localhost:4222
///    Start with: `nats-server -js`
///
/// Usage:
///   cargo run --example distributed_rate_limit_demo

use anyhow::Result;
use aegis_node::{
    distributed_rate_limiter::{DistributedRateLimiter, RateLimitDecision, RateLimiterConfig},
    nats_sync::NatsConfig,
};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn, Level};
use tracing_subscriber;

const RESOURCE_ID: &str = "shared-api-endpoint";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("=== Distributed Rate Limiter Demo ===");
    info!("This demo simulates 3 nodes sharing rate limit state via NATS");
    info!("");

    // Check if NATS is available
    info!("Checking NATS server connection...");
    if let Err(e) = check_nats_connection().await {
        error!("Failed to connect to NATS: {}", e);
        error!("");
        error!("Please start NATS server with JetStream:");
        error!("  nats-server -js");
        error!("");
        error!("Or use Docker:");
        error!("  docker run -p 4222:4222 nats:latest -js");
        return Err(e);
    }

    info!("NATS server is running!");
    info!("");

    // Configuration: 10 requests per 30 second window
    let config = RateLimiterConfig {
        actor_id: 0, // Will be overridden per node
        nats_config: NatsConfig {
            server_url: "nats://localhost:4222".to_string(),
            stream_name: "DEMO_STATE".to_string(),
            counter_subject: "demo.rate.counter".to_string(),
            consumer_name: "demo-consumer".to_string(),
            ..Default::default()
        },
        window_duration_secs: 30,
        max_requests: 10,
        auto_sync: true,
    };

    info!("Configuration:");
    info!("  Window: {} seconds", config.window_duration_secs);
    info!("  Max requests per window: {}", config.max_requests);
    info!("");

    // Create 3 nodes
    info!("Creating 3 nodes...");

    let mut node1 = create_node(1, config.clone()).await?;
    let mut node2 = create_node(2, config.clone()).await?;
    let mut node3 = create_node(3, config.clone()).await?;

    info!("All nodes created!");
    info!("");

    // Start subscriptions
    info!("Starting NATS subscriptions for all nodes...");
    node1.start_subscription(RESOURCE_ID).await?;
    node2.start_subscription(RESOURCE_ID).await?;
    node3.start_subscription(RESOURCE_ID).await?;

    info!("All subscriptions active!");
    info!("");

    // Wait for subscriptions to be fully initialized
    sleep(Duration::from_millis(500)).await;

    info!("=== Demo Scenario ===");
    info!("We'll make requests from different nodes and observe");
    info!("how the distributed counter converges across all nodes.");
    info!("");

    // Scenario 1: Node 1 makes 3 requests
    info!(">>> Scenario 1: Node 1 makes 3 requests");
    for i in 1..=3 {
        make_request(&node1, RESOURCE_ID, 1, i).await?;
        sleep(Duration::from_millis(200)).await;
    }

    info!("");
    sleep(Duration::from_secs(1)).await;
    show_all_counts(&node1, &node2, &node3, RESOURCE_ID).await?;
    info!("");

    // Scenario 2: Node 2 makes 3 requests
    info!(">>> Scenario 2: Node 2 makes 3 requests");
    for i in 1..=3 {
        make_request(&node2, RESOURCE_ID, 2, i).await?;
        sleep(Duration::from_millis(200)).await;
    }

    info!("");
    sleep(Duration::from_secs(1)).await;
    show_all_counts(&node1, &node2, &node3, RESOURCE_ID).await?;
    info!("");

    // Scenario 3: Node 3 makes 3 requests
    info!(">>> Scenario 3: Node 3 makes 3 requests");
    for i in 1..=3 {
        make_request(&node3, RESOURCE_ID, 3, i).await?;
        sleep(Duration::from_millis(200)).await;
    }

    info!("");
    sleep(Duration::from_secs(1)).await;
    show_all_counts(&node1, &node2, &node3, RESOURCE_ID).await?;
    info!("");

    // Scenario 4: Try to exceed limit
    info!(">>> Scenario 4: Node 1 tries to make one more request (should exceed limit)");
    make_request(&node1, RESOURCE_ID, 1, 4).await?;

    info!("");
    sleep(Duration::from_secs(1)).await;
    show_all_counts(&node1, &node2, &node3, RESOURCE_ID).await?;
    info!("");

    // Scenario 5: Burst from multiple nodes
    info!(">>> Scenario 5: Concurrent requests from all nodes");
    info!("Node 1, 2, and 3 each try to make a request simultaneously");

    let n1 = node1.check_rate_limit(RESOURCE_ID);
    let n2 = node2.check_rate_limit(RESOURCE_ID);
    let n3 = node3.check_rate_limit(RESOURCE_ID);

    let (d1, d2, d3) = tokio::join!(n1, n2, n3);

    info!("  Node 1 decision: {:?}", d1?);
    info!("  Node 2 decision: {:?}", d2?);
    info!("  Node 3 decision: {:?}", d3?);

    info!("");
    sleep(Duration::from_secs(2)).await;
    show_all_counts(&node1, &node2, &node3, RESOURCE_ID).await?;
    info!("");

    info!("=== Demo Complete ===");
    info!("");
    info!("Key Observations:");
    info!("1. Each node maintains its own CRDT counter");
    info!("2. Increments are published to NATS and merged by other nodes");
    info!("3. All nodes eventually converge to the same total count");
    info!("4. Rate limiting works across the entire distributed system");
    info!("5. No central coordinator needed - fully decentralized!");
    info!("");

    Ok(())
}

async fn create_node(actor_id: u64, mut config: RateLimiterConfig) -> Result<DistributedRateLimiter> {
    config.actor_id = actor_id;
    config.nats_config.consumer_name = format!("demo-consumer-{}", actor_id);

    let mut limiter = DistributedRateLimiter::new(config);
    limiter.connect_and_sync().await?;

    info!("  Node {} created (actor_id: {})", actor_id, actor_id);
    Ok(limiter)
}

async fn make_request(
    node: &DistributedRateLimiter,
    resource_id: &str,
    node_num: u64,
    req_num: u64,
) -> Result<()> {
    let decision = node.check_rate_limit(resource_id).await?;

    match decision {
        RateLimitDecision::Allowed {
            current_count,
            remaining,
        } => {
            info!(
                "  Node {} Request #{}: ✓ ALLOWED (count: {}, remaining: {})",
                node_num, req_num, current_count, remaining
            );
        }
        RateLimitDecision::Denied {
            current_count,
            retry_after_secs,
        } => {
            warn!(
                "  Node {} Request #{}: ✗ DENIED (count: {}, retry after: {}s)",
                node_num, req_num, current_count, retry_after_secs
            );
        }
    }

    Ok(())
}

async fn show_all_counts(
    node1: &DistributedRateLimiter,
    node2: &DistributedRateLimiter,
    node3: &DistributedRateLimiter,
    resource_id: &str,
) -> Result<()> {
    let count1 = node1.get_count(resource_id)?;
    let count2 = node2.get_count(resource_id)?;
    let count3 = node3.get_count(resource_id)?;

    info!("Current counts across all nodes:");
    info!("  Node 1 sees: {}", count1);
    info!("  Node 2 sees: {}", count2);
    info!("  Node 3 sees: {}", count3);

    if count1 == count2 && count2 == count3 {
        info!("  ✓ All nodes converged to same value!");
    } else {
        warn!("  ⚠ Nodes have different values (eventual consistency in progress)");
    }

    Ok(())
}

async fn check_nats_connection() -> Result<()> {
    let client = async_nats::connect("nats://localhost:4222").await?;

    // Try to access JetStream by getting or creating a test stream
    let jetstream = async_nats::jetstream::new(client);

    // Verify JetStream is enabled by attempting to get/create a stream
    let _ = jetstream
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: "DEMO_CONNECTION_TEST".to_string(),
            subjects: vec!["demo.test.>".to_string()],
            ..Default::default()
        })
        .await?;

    Ok(())
}
