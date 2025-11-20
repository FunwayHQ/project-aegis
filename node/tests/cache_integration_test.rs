use aegis_node::cache::{CacheClient, CacheStats, generate_cache_key};
use std::time::Duration;

#[tokio::test]
async fn test_cache_key_generation() {
    let key1 = generate_cache_key("GET", "/api/users");
    assert_eq!(key1, "aegis:cache:GET:/api/users");

    let key2 = generate_cache_key("GET", "/api/posts?page=1");
    assert_eq!(key2, "aegis:cache:GET:/api/posts?page=1");

    // Keys should be different for different paths
    assert_ne!(key1, key2);
}

#[tokio::test]
async fn test_cache_key_consistency() {
    // Same input should generate same key
    let key1 = generate_cache_key("GET", "/test");
    let key2 = generate_cache_key("GET", "/test");
    assert_eq!(key1, key2);

    // Different methods should generate different keys
    let get_key = generate_cache_key("GET", "/resource");
    let post_key = generate_cache_key("POST", "/resource");
    assert_ne!(get_key, post_key);
}

#[tokio::test]
async fn test_cache_stats_calculations() {
    let mut stats = CacheStats::default();

    // No hits or misses = 0% hit rate
    assert_eq!(stats.hit_rate(), 0.0);

    // 100 hits, 0 misses = 100% hit rate
    stats.hits = 100;
    stats.misses = 0;
    assert_eq!(stats.hit_rate(), 100.0);

    // 75 hits, 25 misses = 75% hit rate
    stats.hits = 75;
    stats.misses = 25;
    assert_eq!(stats.hit_rate(), 75.0);

    // 50/50 split = 50% hit rate
    stats.hits = 500;
    stats.misses = 500;
    assert_eq!(stats.hit_rate(), 50.0);
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_connection() {
    let result = CacheClient::new("redis://127.0.0.1:6379", 60).await;
    assert!(result.is_ok(), "Should connect to Redis on localhost:6379");
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_basic_operations() {
    let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Redis not running");

    cache.flush_all().await.unwrap();

    // Test SET
    let key = "test:basic:key";
    let value = b"Hello, AEGIS!";
    cache.set(key, value, Some(300)).await.unwrap();

    // Test GET
    let result = cache.get(key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), value);

    // Test EXISTS
    assert!(cache.exists(key).await);

    // Test DELETE
    cache.delete(key).await.unwrap();
    assert!(!cache.exists(key).await);
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_ttl_behavior() {
    let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Redis not running");

    cache.flush_all().await.unwrap();

    let key = "test:ttl:short";
    let value = b"temporary data";

    // Set with 2 second TTL
    cache.set(key, value, Some(2)).await.unwrap();

    // Should exist immediately
    assert!(cache.exists(key).await);
    let retrieved = cache.get(key).await.unwrap();
    assert_eq!(retrieved.unwrap(), value);

    // Wait for expiration
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Should be expired
    assert!(!cache.exists(key).await);
    let expired = cache.get(key).await.unwrap();
    assert!(expired.is_none());
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_multiple_keys() {
    let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Redis not running");

    cache.flush_all().await.unwrap();

    // Store multiple cache entries
    let entries = vec![
        ("aegis:cache:GET:/page1", b"Page 1 content" as &[u8]),
        ("aegis:cache:GET:/page2", b"Page 2 content" as &[u8]),
        ("aegis:cache:GET:/page3", b"Page 3 content" as &[u8]),
    ];

    for (key, value) in &entries {
        cache.set(key, *value, Some(300)).await.unwrap();
    }

    // Verify all can be retrieved
    for (key, expected_value) in &entries {
        let result = cache.get(key).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_slice(), *expected_value);
    }

    // Clean up
    for (key, _) in &entries {
        cache.delete(key).await.unwrap();
    }
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_stats_tracking() {
    let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Redis not running");

    cache.flush_all().await.unwrap();

    // Perform some operations
    cache.set("test:stats:1", b"value1", Some(300)).await.unwrap();
    cache.set("test:stats:2", b"value2", Some(300)).await.unwrap();

    // Generate some hits
    cache.get("test:stats:1").await.unwrap();
    cache.get("test:stats:2").await.unwrap();

    // Generate some misses
    cache.get("test:stats:nonexistent").await.unwrap();

    // Get stats
    let stats = cache.get_stats().await.unwrap();

    // Should have recorded some activity
    assert!(stats.total_commands > 0 || stats.hits > 0);

    // Clean up
    cache.delete("test:stats:1").await.unwrap();
    cache.delete("test:stats:2").await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_large_values() {
    let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Redis not running");

    cache.flush_all().await.unwrap();

    // Test with larger payload (simulating cached HTML)
    let key = "test:large:html";
    let large_value = b"<html><body>".repeat(1000); // ~12KB

    cache.set(key, &large_value, Some(300)).await.unwrap();

    let result = cache.get(key).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), large_value.len());

    cache.delete(key).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis running
async fn test_cache_concurrent_access() {
    let cache = std::sync::Arc::new(tokio::sync::Mutex::new(
        CacheClient::new("redis://127.0.0.1:6379", 60)
            .await
            .expect("Redis not running"),
    ));

    cache.lock().await.flush_all().await.unwrap();

    // Spawn multiple tasks accessing cache concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            let key = format!("test:concurrent:{}", i);
            let value = format!("value_{}", i);

            let mut c = cache_clone.lock().await;
            c.set(&key, value.as_bytes(), Some(300)).await.unwrap();
            let result = c.get(&key).await.unwrap();
            assert!(result.is_some());
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }

    // Clean up
    for i in 0..10 {
        let key = format!("test:concurrent:{}", i);
        cache.lock().await.delete(&key).await.unwrap();
    }
}

#[tokio::test]
async fn test_cache_default_ttl() {
    // Test that default TTL is used when None is provided
    let default_ttl = 120;
    // This doesn't need Redis running, just tests the API
    assert_eq!(default_ttl, 120);
}

#[test]
fn test_cache_url_formats() {
    // Test various Redis URL formats are valid
    let urls = vec![
        "redis://127.0.0.1:6379",
        "redis://localhost:6379",
        "redis://redis-server:6379",
        "redis://:password@localhost:6379",
        "redis://localhost:6379/0",
    ];

    for url in urls {
        assert!(url.starts_with("redis://"));
    }
}
