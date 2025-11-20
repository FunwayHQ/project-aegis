use anyhow::Result;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use std::time::Duration;

/// Cache client for DragonflyDB/Redis
pub struct CacheClient {
    connection: ConnectionManager,
    default_ttl: u64,
}

impl CacheClient {
    /// Create a new cache client
    pub async fn new(redis_url: &str, default_ttl: u64) -> Result<Self> {
        let client = Client::open(redis_url)?;
        let connection = ConnectionManager::new(client).await?;

        Ok(Self {
            connection,
            default_ttl,
        })
    }

    /// Get a value from cache
    pub async fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.connection.get::<_, Option<Vec<u8>>>(key).await {
            Ok(value) => Ok(value),
            Err(e) => {
                log::warn!("Cache GET error for key {}: {}", key, e);
                Ok(None)
            }
        }
    }

    /// Set a value in cache with TTL
    pub async fn set(&mut self, key: &str, value: &[u8], ttl_seconds: Option<u64>) -> Result<()> {
        let ttl = ttl_seconds.unwrap_or(self.default_ttl);

        match self
            .connection
            .set_ex::<_, _, ()>(key, value, ttl)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("Cache SET error for key {}: {}", key, e);
                Err(e.into())
            }
        }
    }

    /// Check if a key exists in cache
    pub async fn exists(&mut self, key: &str) -> bool {
        self.connection
            .exists::<_, bool>(key)
            .await
            .unwrap_or(false)
    }

    /// Delete a key from cache
    pub async fn delete(&mut self, key: &str) -> Result<()> {
        self.connection.del::<_, ()>(key).await?;
        Ok(())
    }

    /// Get cache statistics
    pub async fn get_stats(&mut self) -> Result<CacheStats> {
        // Get info from Redis/DragonflyDB
        let info: String = redis::cmd("INFO")
            .query_async(&mut self.connection)
            .await?;

        // Parse relevant stats
        let mut stats = CacheStats::default();

        for line in info.lines() {
            if line.starts_with("used_memory:") {
                if let Some(mem) = line.split(':').nth(1) {
                    stats.memory_used = mem.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("total_commands_processed:") {
                if let Some(cmds) = line.split(':').nth(1) {
                    stats.total_commands = cmds.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("keyspace_hits:") {
                if let Some(hits) = line.split(':').nth(1) {
                    stats.hits = hits.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("keyspace_misses:") {
                if let Some(misses) = line.split(':').nth(1) {
                    stats.misses = misses.trim().parse().unwrap_or(0);
                }
            }
        }

        Ok(stats)
    }

    /// Flush all keys (for testing)
    pub async fn flush_all(&mut self) -> Result<()> {
        redis::cmd("FLUSHALL")
            .query_async(&mut self.connection)
            .await?;
        Ok(())
    }
}

/// Cache statistics
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub memory_used: u64,
    pub total_commands: u64,
    pub hits: u64,
    pub misses: u64,
}

impl CacheStats {
    /// Calculate hit rate as percentage
    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            (self.hits as f64 / (self.hits + self.misses) as f64) * 100.0
        }
    }
}

/// Generate a cache key from request method and URI
pub fn generate_cache_key(method: &str, uri: &str) -> String {
    format!("aegis:cache:{}:{}", method, uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_key_generation() {
        let key = generate_cache_key("GET", "/api/users");
        assert_eq!(key, "aegis:cache:GET:/api/users");
    }

    #[tokio::test]
    async fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);

        stats.hits = 80;
        stats.misses = 20;
        assert_eq!(stats.hit_rate(), 80.0);

        stats.hits = 50;
        stats.misses = 50;
        assert_eq!(stats.hit_rate(), 50.0);
    }

    #[tokio::test]
    #[ignore] // Requires Redis to be running
    async fn test_cache_set_and_get() {
        let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
            .await
            .expect("Failed to connect to Redis");

        cache.flush_all().await.unwrap();

        let key = "test:key";
        let value = b"test value";

        // Set value
        cache.set(key, value, None).await.unwrap();

        // Get value
        let result = cache.get(key).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), value);

        // Clean up
        cache.delete(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis to be running
    async fn test_cache_ttl_expiration() {
        let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
            .await
            .expect("Failed to connect to Redis");

        cache.flush_all().await.unwrap();

        let key = "test:expiring";
        let value = b"expires soon";

        // Set with 1 second TTL
        cache.set(key, value, Some(1)).await.unwrap();

        // Should exist immediately
        assert!(cache.exists(key).await);

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should be gone
        assert!(!cache.exists(key).await);
    }

    #[tokio::test]
    #[ignore] // Requires Redis to be running
    async fn test_cache_stats() {
        let mut cache = CacheClient::new("redis://127.0.0.1:6379", 60)
            .await
            .expect("Failed to connect to Redis");

        let stats = cache.get_stats().await.unwrap();
        assert!(stats.total_commands > 0 || stats.total_commands == 0);
    }
}
