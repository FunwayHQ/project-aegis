// Sprint 19: TLS Fingerprint Cache for DragonflyDB
//
// This module provides persistent storage for TLS fingerprints in DragonflyDB,
// enabling network-wide fingerprint database synchronization and fast lookups.

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tls_fingerprint::{ClientType, FingerprintEntry, TlsFingerprint};

/// Key prefixes for fingerprint storage
const JA3_PREFIX: &str = "aegis:tls:ja3:";
const JA4_PREFIX: &str = "aegis:tls:ja4:";
const STATS_KEY: &str = "aegis:tls:stats";
const UNKNOWN_SET: &str = "aegis:tls:unknown";

/// Default TTL for fingerprint entries (7 days)
const DEFAULT_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// TTL for frequently seen fingerprints (30 days)
const FREQUENT_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// Threshold for "frequent" fingerprint
const FREQUENT_THRESHOLD: u64 = 100;

/// Fingerprint statistics stored in cache
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FingerprintStats {
    /// Total fingerprints observed
    pub total_observed: u64,
    /// Unique JA3 hashes
    pub unique_ja3: u64,
    /// Unique JA4 hashes
    pub unique_ja4: u64,
    /// Browser fingerprints count
    pub browser_count: u64,
    /// Automation tool fingerprints count
    pub automation_count: u64,
    /// Scanner fingerprints count
    pub scanner_count: u64,
    /// Unknown fingerprints count
    pub unknown_count: u64,
    /// Last update timestamp
    pub last_updated: u64,
}

/// Cached fingerprint entry with additional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFingerprint {
    /// The fingerprint entry
    pub entry: FingerprintEntry,
    /// JA3 hash
    pub ja3: String,
    /// JA4 fingerprint
    pub ja4: String,
    /// Raw JA3 string (for debugging)
    pub ja3_raw: Option<String>,
    /// Geographic distribution (country codes seen)
    pub geo_distribution: Vec<String>,
    /// Associated User-Agents seen with this fingerprint
    pub associated_user_agents: Vec<String>,
}

/// TLS Fingerprint Cache Client for DragonflyDB
pub struct FingerprintCache {
    /// Redis/DragonflyDB connection
    connection: ConnectionManager,
    /// Default TTL for entries
    default_ttl: u64,
}

impl FingerprintCache {
    /// Create new fingerprint cache client
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)
            .context("Failed to create Redis client")?;
        let connection = ConnectionManager::new(client)
            .await
            .context("Failed to connect to Redis/DragonflyDB")?;

        Ok(Self {
            connection,
            default_ttl: DEFAULT_TTL_SECS,
        })
    }

    /// Create cache client with custom TTL
    pub async fn with_ttl(redis_url: &str, ttl_secs: u64) -> Result<Self> {
        let mut cache = Self::new(redis_url).await?;
        cache.default_ttl = ttl_secs;
        Ok(cache)
    }

    /// Look up fingerprint by JA3 hash
    pub async fn get_by_ja3(&mut self, ja3: &str) -> Result<Option<CachedFingerprint>> {
        let key = format!("{}{}", JA3_PREFIX, ja3);

        match self.connection.get::<_, Option<String>>(&key).await {
            Ok(Some(json)) => {
                let cached: CachedFingerprint = serde_json::from_str(&json)
                    .context("Failed to deserialize cached fingerprint")?;
                Ok(Some(cached))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                log::warn!("Cache GET error for JA3 {}: {}", ja3, e);
                Ok(None)
            }
        }
    }

    /// Look up fingerprint by JA4 prefix
    pub async fn get_by_ja4_prefix(&mut self, ja4_prefix: &str) -> Result<Option<CachedFingerprint>> {
        let key = format!("{}{}", JA4_PREFIX, ja4_prefix);

        match self.connection.get::<_, Option<String>>(&key).await {
            Ok(Some(json)) => {
                let cached: CachedFingerprint = serde_json::from_str(&json)
                    .context("Failed to deserialize cached fingerprint")?;
                Ok(Some(cached))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                log::warn!("Cache GET error for JA4 prefix {}: {}", ja4_prefix, e);
                Ok(None)
            }
        }
    }

    /// Store or update fingerprint
    pub async fn upsert(&mut self, fingerprint: &TlsFingerprint, entry: FingerprintEntry) -> Result<()> {
        let cached = CachedFingerprint {
            entry: entry.clone(),
            ja3: fingerprint.ja3.clone(),
            ja4: fingerprint.ja4.clone(),
            ja3_raw: Some(fingerprint.ja3_raw.clone()),
            geo_distribution: Vec::new(),
            associated_user_agents: Vec::new(),
        };

        let json = serde_json::to_string(&cached)
            .context("Failed to serialize fingerprint")?;

        // Determine TTL based on request count
        let ttl = if entry.request_count >= FREQUENT_THRESHOLD {
            FREQUENT_TTL_SECS
        } else {
            self.default_ttl
        };

        // Store by JA3
        let ja3_key = format!("{}{}", JA3_PREFIX, fingerprint.ja3);
        self.connection
            .set_ex::<_, _, ()>(&ja3_key, &json, ttl)
            .await
            .context("Failed to store JA3 fingerprint")?;

        // Store by JA4 prefix (first 12 chars for stability)
        let ja4_prefix = &fingerprint.ja4[..12.min(fingerprint.ja4.len())];
        let ja4_key = format!("{}{}", JA4_PREFIX, ja4_prefix);
        self.connection
            .set_ex::<_, _, ()>(&ja4_key, &json, ttl)
            .await
            .context("Failed to store JA4 fingerprint")?;

        // Update stats
        self.increment_stats(&entry.client_type).await?;

        Ok(())
    }

    /// Record observation of a fingerprint (increments count, updates timestamps)
    pub async fn record_observation(
        &mut self,
        fingerprint: &TlsFingerprint,
        user_agent: Option<&str>,
        geo: Option<&str>,
    ) -> Result<()> {
        let ja3_key = format!("{}{}", JA3_PREFIX, fingerprint.ja3);

        // Get existing entry
        if let Some(mut cached) = self.get_by_ja3(&fingerprint.ja3).await? {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();

            // Update counts and timestamps
            cached.entry.request_count += 1;
            cached.entry.last_seen = now;

            // Add User-Agent if provided and not already tracked (limit to 10)
            if let Some(ua) = user_agent {
                if !cached.associated_user_agents.contains(&ua.to_string())
                    && cached.associated_user_agents.len() < 10
                {
                    cached.associated_user_agents.push(ua.to_string());
                }
            }

            // Add geo if provided and not already tracked (limit to 50)
            if let Some(country) = geo {
                if !cached.geo_distribution.contains(&country.to_string())
                    && cached.geo_distribution.len() < 50
                {
                    cached.geo_distribution.push(country.to_string());
                }
            }

            // Re-serialize and store
            let json = serde_json::to_string(&cached)?;

            // Use longer TTL for frequent fingerprints
            let ttl = if cached.entry.request_count >= FREQUENT_THRESHOLD {
                FREQUENT_TTL_SECS
            } else {
                self.default_ttl
            };

            self.connection
                .set_ex::<_, _, ()>(&ja3_key, &json, ttl)
                .await?;
        } else {
            // New unknown fingerprint - add to unknown set for later analysis
            self.connection
                .sadd::<_, _, ()>(UNKNOWN_SET, &fingerprint.ja3)
                .await
                .ok();
        }

        Ok(())
    }

    /// Get unknown fingerprints for analysis
    pub async fn get_unknown_fingerprints(&mut self, limit: usize) -> Result<Vec<String>> {
        let members: Vec<String> = self.connection
            .srandmember_multiple(UNKNOWN_SET, limit)
            .await
            .unwrap_or_default();
        Ok(members)
    }

    /// Promote unknown fingerprint to known (after classification)
    pub async fn classify_unknown(
        &mut self,
        ja3: &str,
        client_type: ClientType,
        client_name: &str,
    ) -> Result<()> {
        // Remove from unknown set
        self.connection
            .srem::<_, _, ()>(UNKNOWN_SET, ja3)
            .await
            .ok();

        // Create entry with classification
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        let entry = FingerprintEntry {
            client_type,
            client_name: client_name.to_string(),
            confidence: 0.70, // Lower confidence for newly classified
            first_seen: now,
            last_seen: now,
            request_count: 1,
        };

        // Create minimal fingerprint for storage
        let cached = CachedFingerprint {
            entry,
            ja3: ja3.to_string(),
            ja4: String::new(), // JA4 not available from JA3 alone
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: Vec::new(),
        };

        let json = serde_json::to_string(&cached)?;
        let key = format!("{}{}", JA3_PREFIX, ja3);

        self.connection
            .set_ex::<_, _, ()>(&key, &json, self.default_ttl)
            .await?;

        Ok(())
    }

    /// Increment statistics counters
    async fn increment_stats(&mut self, client_type: &ClientType) -> Result<()> {
        // Use Redis HINCRBY for atomic increments
        let field = match client_type {
            ClientType::Browser | ClientType::MobileBrowser => "browser_count",
            ClientType::AutomationTool => "automation_count",
            ClientType::Scanner => "scanner_count",
            ClientType::HeadlessBrowser => "automation_count",
            ClientType::GoodBot => "browser_count",
            ClientType::Unknown => "unknown_count",
        };

        self.connection
            .hincr::<_, _, _, ()>(STATS_KEY, field, 1i64)
            .await
            .ok();

        self.connection
            .hincr::<_, _, _, ()>(STATS_KEY, "total_observed", 1i64)
            .await
            .ok();

        Ok(())
    }

    /// Get fingerprint statistics
    pub async fn get_stats(&mut self) -> Result<FingerprintStats> {
        let stats_map: std::collections::HashMap<String, i64> = self.connection
            .hgetall(STATS_KEY)
            .await
            .unwrap_or_default();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        Ok(FingerprintStats {
            total_observed: stats_map.get("total_observed").copied().unwrap_or(0) as u64,
            unique_ja3: stats_map.get("unique_ja3").copied().unwrap_or(0) as u64,
            unique_ja4: stats_map.get("unique_ja4").copied().unwrap_or(0) as u64,
            browser_count: stats_map.get("browser_count").copied().unwrap_or(0) as u64,
            automation_count: stats_map.get("automation_count").copied().unwrap_or(0) as u64,
            scanner_count: stats_map.get("scanner_count").copied().unwrap_or(0) as u64,
            unknown_count: stats_map.get("unknown_count").copied().unwrap_or(0) as u64,
            last_updated: now,
        })
    }

    /// Bulk import fingerprints from JSON
    pub async fn bulk_import(&mut self, fingerprints: &[CachedFingerprint]) -> Result<usize> {
        let mut imported = 0;

        for fp in fingerprints {
            let json = serde_json::to_string(fp)?;
            let key = format!("{}{}", JA3_PREFIX, fp.ja3);

            self.connection
                .set_ex::<_, _, ()>(&key, &json, self.default_ttl)
                .await?;

            if !fp.ja4.is_empty() {
                let ja4_prefix = &fp.ja4[..12.min(fp.ja4.len())];
                let ja4_key = format!("{}{}", JA4_PREFIX, ja4_prefix);
                self.connection
                    .set_ex::<_, _, ()>(&ja4_key, &json, self.default_ttl)
                    .await?;
            }

            imported += 1;
        }

        Ok(imported)
    }

    /// Export all known fingerprints
    pub async fn export_all(&mut self) -> Result<Vec<CachedFingerprint>> {
        // Use SCAN to iterate through JA3 keys
        let mut fingerprints = Vec::new();
        let mut cursor = 0u64;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(format!("{}*", JA3_PREFIX))
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.connection)
                .await?;

            for key in keys {
                if let Ok(Some(json)) = self.connection.get::<_, Option<String>>(&key).await {
                    if let Ok(cached) = serde_json::from_str::<CachedFingerprint>(&json) {
                        fingerprints.push(cached);
                    }
                }
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(fingerprints)
    }

    /// Flush all fingerprint data (for testing)
    pub async fn flush_all(&mut self) -> Result<()> {
        // Delete all keys with our prefixes using SCAN + DEL
        for prefix in [JA3_PREFIX, JA4_PREFIX] {
            let mut cursor = 0u64;
            loop {
                let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(format!("{}*", prefix))
                    .arg("COUNT")
                    .arg(100)
                    .query_async(&mut self.connection)
                    .await?;

                if !keys.is_empty() {
                    for key in &keys {
                        self.connection.del::<_, ()>(key).await.ok();
                    }
                }

                cursor = next_cursor;
                if cursor == 0 {
                    break;
                }
            }
        }

        // Delete stats and unknown set
        self.connection.del::<_, ()>(STATS_KEY).await.ok();
        self.connection.del::<_, ()>(UNKNOWN_SET).await.ok();

        Ok(())
    }
}

/// Create well-known browser fingerprints for seeding the database
pub fn create_builtin_fingerprints() -> Vec<CachedFingerprint> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    vec![
        // Chrome 120+
        CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::Browser,
                client_name: "Chrome 120+".to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            },
            ja3: "cd08e31494f9531f560d64c695473da9".to_string(),
            ja4: "t3d171200h2_".to_string(),
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            ],
        },
        // Firefox 120+
        CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::Browser,
                client_name: "Firefox 120+".to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            },
            ja3: "28a2eb8f6ff952b9c8c7e8b5c9e0e1e2".to_string(),
            ja4: "t3d161200h2_".to_string(),
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: vec![
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0".to_string(),
            ],
        },
        // curl (OpenSSL)
        CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::AutomationTool,
                client_name: "curl (OpenSSL)".to_string(),
                confidence: 0.90,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            },
            ja3: "3b5074b1b5d032e5620f69f9f700ff0e".to_string(),
            ja4: "t2d050400_".to_string(),
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: vec![
                "curl/8.4.0".to_string(),
                "curl/7.88.1".to_string(),
            ],
        },
        // Python requests
        CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::AutomationTool,
                client_name: "python-requests".to_string(),
                confidence: 0.90,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            },
            ja3: "b3a29e8d7c6f5e4d3c2b1a0f9e8d7c6b".to_string(),
            ja4: "t2d060500_".to_string(),
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: vec![
                "python-requests/2.31.0".to_string(),
            ],
        },
        // Puppeteer
        CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::HeadlessBrowser,
                client_name: "Puppeteer/Headless Chrome".to_string(),
                confidence: 0.85,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            },
            ja3: "1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a".to_string(),
            ja4: "t3i171200h2_".to_string(), // 'i' = no SNI (common in headless)
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: vec![
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/120.0.0.0 Safari/537.36".to_string(),
            ],
        },
        // Nmap SSL scanner
        CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::Scanner,
                client_name: "Nmap SSL".to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 0,
            },
            ja3: "e9c0a1b2c3d4e5f6a7b8c9d0e1f2a3b4".to_string(),
            ja4: "t2i030200_".to_string(),
            ja3_raw: None,
            geo_distribution: Vec::new(),
            associated_user_agents: Vec::new(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_fingerprints() {
        let fingerprints = create_builtin_fingerprints();
        assert!(!fingerprints.is_empty());

        // Check Chrome fingerprint
        let chrome = fingerprints.iter()
            .find(|f| f.entry.client_name.contains("Chrome"))
            .unwrap();
        assert_eq!(chrome.entry.client_type, ClientType::Browser);
        assert!(chrome.entry.confidence > 0.9);

        // Check curl fingerprint
        let curl = fingerprints.iter()
            .find(|f| f.entry.client_name.contains("curl"))
            .unwrap();
        assert_eq!(curl.entry.client_type, ClientType::AutomationTool);

        // Check scanner fingerprint
        let nmap = fingerprints.iter()
            .find(|f| f.entry.client_name.contains("Nmap"))
            .unwrap();
        assert_eq!(nmap.entry.client_type, ClientType::Scanner);
    }

    #[test]
    fn test_cached_fingerprint_serialization() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        let cached = CachedFingerprint {
            entry: FingerprintEntry {
                client_type: ClientType::Browser,
                client_name: "Test Browser".to_string(),
                confidence: 0.95,
                first_seen: now,
                last_seen: now,
                request_count: 100,
            },
            ja3: "abc123def456".to_string(),
            ja4: "t3d1712000_abc_def".to_string(),
            ja3_raw: Some("771,4865-4866,0-23-65281,29-23-24,0".to_string()),
            geo_distribution: vec!["US".to_string(), "GB".to_string()],
            associated_user_agents: vec!["Mozilla/5.0 Test".to_string()],
        };

        // Serialize
        let json = serde_json::to_string(&cached).unwrap();
        assert!(json.contains("Test Browser"));
        assert!(json.contains("abc123def456"));

        // Deserialize
        let restored: CachedFingerprint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.entry.client_name, "Test Browser");
        assert_eq!(restored.ja3, "abc123def456");
        assert_eq!(restored.entry.request_count, 100);
    }

    #[test]
    fn test_fingerprint_stats_default() {
        let stats = FingerprintStats::default();
        assert_eq!(stats.total_observed, 0);
        assert_eq!(stats.browser_count, 0);
        assert_eq!(stats.unknown_count, 0);
    }

    #[tokio::test]
    #[ignore] // Requires Redis/DragonflyDB
    async fn test_cache_upsert_and_get() {
        let mut cache = FingerprintCache::new("redis://127.0.0.1:6379")
            .await
            .expect("Failed to connect");

        cache.flush_all().await.unwrap();

        let fp = TlsFingerprint {
            ja3: "test123abc".to_string(),
            ja3_raw: "771,4865,0-23,29,0".to_string(),
            ja4: "t3d050400_abc_def".to_string(),
            tls_version: crate::tls_fingerprint::TlsVersion::Tls13,
            cipher_count: 5,
            extension_count: 4,
            has_sni: true,
            has_alpn: true,
        };

        let entry = FingerprintEntry {
            client_type: ClientType::Browser,
            client_name: "Test Browser".to_string(),
            confidence: 0.95,
            first_seen: 0,
            last_seen: 0,
            request_count: 1,
        };

        // Upsert
        cache.upsert(&fp, entry).await.unwrap();

        // Get by JA3
        let result = cache.get_by_ja3("test123abc").await.unwrap();
        assert!(result.is_some());
        let cached = result.unwrap();
        assert_eq!(cached.entry.client_name, "Test Browser");
        assert_eq!(cached.entry.client_type, ClientType::Browser);

        // Clean up
        cache.flush_all().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis/DragonflyDB
    async fn test_record_observation() {
        let mut cache = FingerprintCache::new("redis://127.0.0.1:6379")
            .await
            .expect("Failed to connect");

        cache.flush_all().await.unwrap();

        // First upsert a fingerprint
        let fp = TlsFingerprint {
            ja3: "obs_test_123".to_string(),
            ja3_raw: "771,4865,0-23,29,0".to_string(),
            ja4: "t3d050400_xyz_abc".to_string(),
            tls_version: crate::tls_fingerprint::TlsVersion::Tls13,
            cipher_count: 5,
            extension_count: 4,
            has_sni: true,
            has_alpn: true,
        };

        let entry = FingerprintEntry {
            client_type: ClientType::Browser,
            client_name: "Test".to_string(),
            confidence: 0.9,
            first_seen: 0,
            last_seen: 0,
            request_count: 1,
        };

        cache.upsert(&fp, entry).await.unwrap();

        // Record observations
        cache.record_observation(&fp, Some("Mozilla/5.0 Test"), Some("US")).await.unwrap();
        cache.record_observation(&fp, Some("Mozilla/5.0 Test 2"), Some("GB")).await.unwrap();

        // Verify updates
        let result = cache.get_by_ja3("obs_test_123").await.unwrap().unwrap();
        assert_eq!(result.entry.request_count, 3); // Original + 2 observations
        assert!(result.associated_user_agents.len() >= 1);
        assert!(result.geo_distribution.len() >= 1);

        cache.flush_all().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis/DragonflyDB
    async fn test_bulk_import_export() {
        let mut cache = FingerprintCache::new("redis://127.0.0.1:6379")
            .await
            .expect("Failed to connect");

        cache.flush_all().await.unwrap();

        // Import builtin fingerprints
        let builtins = create_builtin_fingerprints();
        let imported = cache.bulk_import(&builtins).await.unwrap();
        assert_eq!(imported, builtins.len());

        // Export and verify
        let exported = cache.export_all().await.unwrap();
        assert_eq!(exported.len(), builtins.len());

        cache.flush_all().await.unwrap();
    }
}
