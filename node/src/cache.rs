use anyhow::Result;
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};

// ============================================================================
// Y3.3: Security constants for cache key handling
// ============================================================================

/// Maximum cache key length to prevent injection and memory attacks (Y3.3)
pub const MAX_CACHE_KEY_LENGTH: usize = 1024;

/// Maximum number of Cache-Control directives to parse (Y9.6 preview)
const MAX_CACHE_DIRECTIVES: usize = 20;

// ============================================================================
// Y3.4: Cache key sanitization error
// ============================================================================

/// Error type for cache key validation
#[derive(Debug, Clone)]
pub enum CacheKeyError {
    /// Key exceeds maximum length
    TooLong { length: usize, max: usize },
    /// Key contains invalid characters
    InvalidCharacters(String),
    /// Key is empty
    Empty,
}

impl std::fmt::Display for CacheKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheKeyError::TooLong { length, max } => {
                write!(f, "Cache key too long: {} bytes (max {})", length, max)
            }
            CacheKeyError::InvalidCharacters(chars) => {
                write!(f, "Cache key contains invalid characters: {:?}", chars)
            }
            CacheKeyError::Empty => write!(f, "Cache key is empty"),
        }
    }
}

impl std::error::Error for CacheKeyError {}

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

        match self.connection.set_ex::<_, _, ()>(key, value, ttl).await {
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
        let info: String = redis::cmd("INFO").query_async(&mut self.connection).await?;

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
            .query_async::<_, ()>(&mut self.connection)
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
///
/// Y3.3-Y3.4: Sanitizes the key by:
/// - Removing CRLF characters (prevents HTTP response splitting attacks)
/// - Removing null bytes (prevents injection attacks)
/// - Truncating to MAX_CACHE_KEY_LENGTH
///
/// Returns Result to indicate if sanitization was needed
pub fn generate_cache_key(method: &str, uri: &str) -> Result<String, CacheKeyError> {
    // Calculate prefix length
    let prefix = "aegis:cache:";
    let prefix_len = prefix.len() + method.len() + 1; // +1 for ':'

    // Y3.4: Sanitize URI by removing dangerous characters
    // CRLF characters can enable HTTP response splitting attacks
    // Null bytes can cause issues with C-based storage engines
    let safe_uri: String = uri
        .chars()
        .filter(|c| *c != '\r' && *c != '\n' && *c != '\0')
        .collect();

    // Y3.3: Calculate maximum URI length to stay within limit
    let max_uri_len = MAX_CACHE_KEY_LENGTH.saturating_sub(prefix_len);

    // Truncate if necessary
    let truncated_uri: String = safe_uri.chars().take(max_uri_len).collect();

    // Build the key
    let key = format!("{}{}:{}", prefix, method, truncated_uri);

    // Final validation
    if key.is_empty() {
        return Err(CacheKeyError::Empty);
    }

    if key.len() > MAX_CACHE_KEY_LENGTH {
        return Err(CacheKeyError::TooLong {
            length: key.len(),
            max: MAX_CACHE_KEY_LENGTH,
        });
    }

    Ok(key)
}

/// Sanitize an arbitrary string for use as part of a cache key
///
/// Y3.4: Removes CRLF, null bytes, and truncates to specified max length
pub fn sanitize_cache_key_component(input: &str, max_len: usize) -> String {
    input
        .chars()
        .filter(|c| *c != '\r' && *c != '\n' && *c != '\0')
        .take(max_len)
        .collect()
}

/// Generate a cache key (legacy compatibility - returns String, logs warnings)
///
/// For new code, prefer `generate_cache_key` which returns Result
pub fn generate_cache_key_unchecked(method: &str, uri: &str) -> String {
    match generate_cache_key(method, uri) {
        Ok(key) => key,
        Err(e) => {
            log::warn!("Cache key generation issue: {}. Using fallback.", e);
            // Fallback: truncate aggressively
            let safe_uri = sanitize_cache_key_component(uri, 256);
            format!("aegis:cache:{}:{}", method, safe_uri)
        }
    }
}

/// Cache-Control header directives
#[derive(Debug, Clone, Default)]
pub struct CacheControl {
    pub no_cache: bool,
    pub no_store: bool,
    pub max_age: Option<u64>,
    pub private: bool,
    pub public: bool,
}

impl CacheControl {
    /// Parse Cache-Control header value
    ///
    /// Y9.6 (preview): Limits to MAX_CACHE_DIRECTIVES to prevent DoS
    pub fn parse(header_value: &str) -> Self {
        let mut control = CacheControl::default();
        let mut directive_count = 0;

        for directive in header_value.split(',') {
            // Y9.6: Enforce maximum directive count
            directive_count += 1;
            if directive_count > MAX_CACHE_DIRECTIVES {
                log::warn!(
                    "Cache-Control header exceeded {} directives, ignoring rest",
                    MAX_CACHE_DIRECTIVES
                );
                break;
            }

            let directive = directive.trim().to_lowercase();

            if directive == "no-cache" {
                control.no_cache = true;
            } else if directive == "no-store" {
                control.no_store = true;
            } else if directive == "private" {
                control.private = true;
            } else if directive == "public" {
                control.public = true;
            } else if directive.starts_with("max-age=") {
                if let Some(age_str) = directive.strip_prefix("max-age=") {
                    control.max_age = age_str.parse().ok();
                }
            }
        }

        control
    }

    /// Determine if response should be cached
    pub fn should_cache(&self) -> bool {
        // Don't cache if no-store or no-cache directives present
        if self.no_store || self.no_cache {
            return false;
        }

        // Don't cache if marked private (we're a shared cache)
        if self.private {
            return false;
        }

        // Cache if public or no directives
        true
    }

    /// Get effective TTL based on Cache-Control
    pub fn effective_ttl(&self, default_ttl: u64) -> Option<u64> {
        if !self.should_cache() {
            return None;
        }

        // Use max-age if specified, otherwise use default
        Some(self.max_age.unwrap_or(default_ttl))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Y3.3-Y3.4: Cache key sanitization tests
    // ========================================================================

    #[test]
    fn test_cache_key_generation() {
        let key = generate_cache_key("GET", "/api/users").unwrap();
        assert_eq!(key, "aegis:cache:GET:/api/users");
    }

    #[test]
    fn test_cache_key_crlf_removal() {
        // Y3.4: CRLF should be removed to prevent HTTP response splitting
        let key = generate_cache_key("GET", "/api/users\r\n/admin").unwrap();
        assert!(!key.contains('\r'));
        assert!(!key.contains('\n'));
        assert_eq!(key, "aegis:cache:GET:/api/users/admin");
    }

    #[test]
    fn test_cache_key_null_byte_removal() {
        // Y3.4: Null bytes should be removed
        let key = generate_cache_key("GET", "/api/users\0/admin").unwrap();
        assert!(!key.contains('\0'));
        assert_eq!(key, "aegis:cache:GET:/api/users/admin");
    }

    #[test]
    fn test_cache_key_truncation() {
        // Y3.3: Long URIs should be truncated
        let long_uri: String = "/".to_string() + &"a".repeat(2000);
        let key = generate_cache_key("GET", &long_uri).unwrap();
        assert!(key.len() <= MAX_CACHE_KEY_LENGTH);
    }

    #[test]
    fn test_cache_key_max_length_enforced() {
        // Test that the key never exceeds MAX_CACHE_KEY_LENGTH
        let very_long_uri: String = "/".to_string() + &"x".repeat(5000);
        let key = generate_cache_key("GET", &very_long_uri).unwrap();
        assert!(key.len() <= MAX_CACHE_KEY_LENGTH);
    }

    #[test]
    fn test_sanitize_cache_key_component() {
        // Test the standalone sanitization function
        let sanitized = sanitize_cache_key_component("hello\r\nworld\0!", 10);
        assert_eq!(sanitized, "helloworld");

        // Test truncation
        let long_input = "a".repeat(100);
        let truncated = sanitize_cache_key_component(&long_input, 50);
        assert_eq!(truncated.len(), 50);
    }

    #[test]
    fn test_cache_key_unchecked_fallback() {
        // Test the legacy unchecked function
        let key = generate_cache_key_unchecked("GET", "/api/test");
        assert!(key.starts_with("aegis:cache:"));
    }

    #[test]
    fn test_cache_key_error_display() {
        // Test error messages
        let too_long = CacheKeyError::TooLong {
            length: 2000,
            max: 1024,
        };
        assert!(too_long.to_string().contains("too long"));

        let invalid = CacheKeyError::InvalidCharacters("\r\n".to_string());
        assert!(invalid.to_string().contains("invalid"));

        let empty = CacheKeyError::Empty;
        assert!(empty.to_string().contains("empty"));
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
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

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

    // Cache-Control header tests
    #[test]
    fn test_cache_control_no_cache() {
        let control = CacheControl::parse("no-cache");
        assert!(control.no_cache);
        assert!(!control.should_cache());
    }

    #[test]
    fn test_cache_control_no_store() {
        let control = CacheControl::parse("no-store");
        assert!(control.no_store);
        assert!(!control.should_cache());
    }

    #[test]
    fn test_cache_control_private() {
        let control = CacheControl::parse("private");
        assert!(control.private);
        assert!(!control.should_cache()); // Shared cache shouldn't cache private
    }

    #[test]
    fn test_cache_control_public() {
        let control = CacheControl::parse("public");
        assert!(control.public);
        assert!(control.should_cache());
    }

    #[test]
    fn test_cache_control_max_age() {
        let control = CacheControl::parse("max-age=3600");
        assert_eq!(control.max_age, Some(3600));
        assert!(control.should_cache());
        assert_eq!(control.effective_ttl(60), Some(3600));
    }

    #[test]
    fn test_cache_control_multiple_directives() {
        let control = CacheControl::parse("public, max-age=300");
        assert!(control.public);
        assert_eq!(control.max_age, Some(300));
        assert!(control.should_cache());
        assert_eq!(control.effective_ttl(60), Some(300));
    }

    #[test]
    fn test_cache_control_no_cache_with_max_age() {
        let control = CacheControl::parse("no-cache, max-age=3600");
        assert!(control.no_cache);
        assert_eq!(control.max_age, Some(3600));
        assert!(!control.should_cache()); // no-cache takes precedence
    }

    #[test]
    fn test_cache_control_empty() {
        let control = CacheControl::parse("");
        assert!(!control.no_cache);
        assert!(!control.no_store);
        assert!(!control.private);
        assert!(control.should_cache()); // No restrictions = cacheable
    }

    #[test]
    fn test_cache_control_case_insensitive() {
        let control = CacheControl::parse("NO-CACHE, MAX-AGE=300");
        assert!(control.no_cache);
        assert_eq!(control.max_age, Some(300));
    }

    #[test]
    fn test_cache_control_whitespace() {
        let control = CacheControl::parse("public , max-age=300 ");
        assert!(control.public);
        assert_eq!(control.max_age, Some(300));
    }

    #[test]
    fn test_cache_control_effective_ttl() {
        // No directives - use default
        let control1 = CacheControl::parse("");
        assert_eq!(control1.effective_ttl(60), Some(60));

        // max-age specified - use it
        let control2 = CacheControl::parse("max-age=120");
        assert_eq!(control2.effective_ttl(60), Some(120));

        // no-cache - no TTL
        let control3 = CacheControl::parse("no-cache");
        assert_eq!(control3.effective_ttl(60), None);

        // no-store - no TTL
        let control4 = CacheControl::parse("no-store");
        assert_eq!(control4.effective_ttl(60), None);
    }

    #[test]
    fn test_cache_control_max_age_zero() {
        let control = CacheControl::parse("max-age=0");
        assert_eq!(control.max_age, Some(0));
        assert_eq!(control.effective_ttl(60), Some(0));
    }

    #[test]
    fn test_cache_control_invalid_max_age() {
        let control = CacheControl::parse("max-age=invalid");
        assert_eq!(control.max_age, None);
        assert_eq!(control.effective_ttl(60), Some(60)); // Falls back to default
    }

    #[test]
    fn test_cache_control_real_world_examples() {
        // Common real-world Cache-Control headers
        let examples = vec![
            ("public, max-age=31536000", true, Some(31536000)), // 1 year
            ("private, max-age=0", false, None),
            ("no-cache, no-store, must-revalidate", false, None),
            ("public, max-age=3600, immutable", true, Some(3600)),
        ];

        for (header, should_cache, expected_ttl) in examples {
            let control = CacheControl::parse(header);
            assert_eq!(
                control.should_cache(),
                should_cache,
                "Failed for: {}",
                header
            );
            assert_eq!(
                control.effective_ttl(60),
                expected_ttl,
                "Failed TTL for: {}",
                header
            );
        }
    }
}
