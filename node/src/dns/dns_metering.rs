//! DNS Usage Metering
//!
//! Tracks DNS query statistics for analytics and billing purposes.
//! Supports per-zone and per-account metering with time-series data.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use rusqlite::{params, Connection};

use super::DnsRecordType;

/// Query result type for metering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueryResult {
    /// Successful response with records
    Success,
    /// NXDOMAIN - domain does not exist
    NxDomain,
    /// SERVFAIL - server error
    ServFail,
    /// REFUSED - query refused
    Refused,
    /// Rate limited
    RateLimited,
}

impl std::fmt::Display for QueryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryResult::Success => write!(f, "SUCCESS"),
            QueryResult::NxDomain => write!(f, "NXDOMAIN"),
            QueryResult::ServFail => write!(f, "SERVFAIL"),
            QueryResult::Refused => write!(f, "REFUSED"),
            QueryResult::RateLimited => write!(f, "RATELIMITED"),
        }
    }
}

/// A single query event for metering
#[derive(Debug, Clone)]
pub struct QueryEvent {
    /// Domain queried (zone)
    pub domain: String,
    /// Record type queried
    pub record_type: DnsRecordType,
    /// Query result
    pub result: QueryResult,
    /// Response latency in microseconds
    pub latency_us: u64,
    /// Whether response was served from cache
    pub cache_hit: bool,
    /// Client country code (ISO 3166-1 alpha-2)
    pub country: Option<String>,
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
}

/// Per-zone usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ZoneStats {
    /// Total queries
    pub total_queries: u64,
    /// Queries by record type
    pub queries_by_type: HashMap<String, u64>,
    /// Queries by result
    pub queries_by_result: HashMap<String, u64>,
    /// Queries by country
    pub queries_by_country: HashMap<String, u64>,
    /// Cache hits
    pub cache_hits: u64,
    /// Cache misses
    pub cache_misses: u64,
    /// Latency percentiles in microseconds
    pub latency_p50_us: u64,
    pub latency_p95_us: u64,
    pub latency_p99_us: u64,
    /// Period start (Unix seconds)
    pub period_start: u64,
    /// Period end (Unix seconds)
    pub period_end: u64,
}

impl ZoneStats {
    /// Calculate cache hit ratio
    pub fn cache_hit_ratio(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
}

/// Global usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalStats {
    /// Total queries across all zones
    pub total_queries: u64,
    /// Queries today
    pub queries_today: u64,
    /// Queries by type (all zones)
    pub queries_by_type: HashMap<String, u64>,
    /// Top queried zones
    pub top_zones: Vec<(String, u64)>,
    /// Cache hit ratio
    pub cache_hit_ratio: f64,
    /// Average latency in microseconds
    pub avg_latency_us: u64,
}

/// Time-series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    /// Timestamp (Unix seconds, start of bucket)
    pub timestamp: u64,
    /// Query count in this bucket
    pub queries: u64,
    /// Cache hits in this bucket
    pub cache_hits: u64,
    /// Average latency in this bucket (microseconds)
    pub avg_latency_us: u64,
}

/// Time series resolution
#[derive(Debug, Clone, Copy)]
pub enum TimeResolution {
    /// 1-minute buckets
    Minute,
    /// 5-minute buckets
    FiveMinutes,
    /// 1-hour buckets
    Hour,
    /// 1-day buckets
    Day,
}

impl TimeResolution {
    /// Get bucket duration in seconds
    pub fn bucket_seconds(&self) -> u64 {
        match self {
            TimeResolution::Minute => 60,
            TimeResolution::FiveMinutes => 300,
            TimeResolution::Hour => 3600,
            TimeResolution::Day => 86400,
        }
    }
}

/// In-memory metering buffer
struct MeteringBuffer {
    /// Recent latencies for percentile calculation (circular buffer)
    latencies: Vec<u64>,
    latency_idx: usize,
    /// Query counts by domain
    queries_by_domain: HashMap<String, u64>,
    /// Cache hits by domain
    cache_hits_by_domain: HashMap<String, u64>,
    /// Queries by type
    queries_by_type: HashMap<String, u64>,
    /// Queries by result
    queries_by_result: HashMap<String, u64>,
    /// Queries by country
    queries_by_country: HashMap<String, u64>,
    /// Total queries since last flush
    total_queries: u64,
    /// Last flush timestamp
    last_flush: u64,
}

impl MeteringBuffer {
    const MAX_LATENCIES: usize = 10000;

    fn new() -> Self {
        Self {
            latencies: Vec::with_capacity(Self::MAX_LATENCIES),
            latency_idx: 0,
            queries_by_domain: HashMap::new(),
            cache_hits_by_domain: HashMap::new(),
            queries_by_type: HashMap::new(),
            queries_by_result: HashMap::new(),
            queries_by_country: HashMap::new(),
            total_queries: 0,
            last_flush: current_timestamp(),
        }
    }

    fn record(&mut self, event: &QueryEvent) {
        self.total_queries += 1;

        // Record latency
        if self.latencies.len() < Self::MAX_LATENCIES {
            self.latencies.push(event.latency_us);
        } else {
            self.latencies[self.latency_idx] = event.latency_us;
            self.latency_idx = (self.latency_idx + 1) % Self::MAX_LATENCIES;
        }

        // Record by domain
        *self.queries_by_domain.entry(event.domain.clone()).or_insert(0) += 1;
        if event.cache_hit {
            *self.cache_hits_by_domain.entry(event.domain.clone()).or_insert(0) += 1;
        }

        // Record by type
        *self.queries_by_type.entry(event.record_type.to_string()).or_insert(0) += 1;

        // Record by result
        *self.queries_by_result.entry(event.result.to_string()).or_insert(0) += 1;

        // Record by country
        if let Some(country) = &event.country {
            *self.queries_by_country.entry(country.clone()).or_insert(0) += 1;
        }
    }

    fn calculate_percentile(&self, percentile: f64) -> u64 {
        if self.latencies.is_empty() {
            return 0;
        }

        let mut sorted = self.latencies.clone();
        sorted.sort_unstable();

        let idx = ((percentile / 100.0) * (sorted.len() - 1) as f64) as usize;
        sorted[idx]
    }

    fn reset(&mut self) {
        self.latencies.clear();
        self.latency_idx = 0;
        self.queries_by_domain.clear();
        self.cache_hits_by_domain.clear();
        self.queries_by_type.clear();
        self.queries_by_result.clear();
        self.queries_by_country.clear();
        self.total_queries = 0;
        self.last_flush = current_timestamp();
    }
}

/// DNS Usage Metering System
pub struct DnsMetering {
    /// In-memory buffer for recent events
    buffer: Arc<RwLock<MeteringBuffer>>,
    /// SQLite connection for persistence
    conn: Option<Arc<tokio::sync::Mutex<Connection>>>,
    /// Flush interval (seconds)
    flush_interval: u64,
}

impl DnsMetering {
    /// Create a new metering system with in-memory storage only
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(RwLock::new(MeteringBuffer::new())),
            conn: None,
            flush_interval: 300, // 5 minutes default
        }
    }

    /// Create a new metering system with SQLite persistence
    pub fn with_persistence(db_path: &str) -> Result<Self, super::DnsError> {
        let conn = Connection::open(db_path)
            .map_err(|e| super::DnsError::ServerError(format!("Failed to open metering database: {}", e)))?;

        // Create tables
        Self::create_tables(&conn)?;

        Ok(Self {
            buffer: Arc::new(RwLock::new(MeteringBuffer::new())),
            conn: Some(Arc::new(tokio::sync::Mutex::new(conn))),
            flush_interval: 300,
        })
    }

    /// Create metering tables
    fn create_tables(conn: &Connection) -> Result<(), super::DnsError> {
        // Time-series metrics (5-minute buckets)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dns_metrics (
                domain TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                queries INTEGER NOT NULL DEFAULT 0,
                cache_hits INTEGER NOT NULL DEFAULT 0,
                total_latency_us INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (domain, timestamp)
            )",
            [],
        )
        .map_err(|e| super::DnsError::ServerError(format!("Failed to create dns_metrics table: {}", e)))?;

        // Daily rollups
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dns_daily_stats (
                domain TEXT NOT NULL,
                date TEXT NOT NULL,
                total_queries INTEGER NOT NULL DEFAULT 0,
                cache_hits INTEGER NOT NULL DEFAULT 0,
                latency_p50_us INTEGER,
                latency_p95_us INTEGER,
                latency_p99_us INTEGER,
                queries_by_type TEXT,
                queries_by_country TEXT,
                PRIMARY KEY (domain, date)
            )",
            [],
        )
        .map_err(|e| super::DnsError::ServerError(format!("Failed to create dns_daily_stats table: {}", e)))?;

        // Create index for time-series queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_dns_metrics_timestamp ON dns_metrics(timestamp)",
            [],
        )
        .map_err(|e| super::DnsError::ServerError(format!("Failed to create index: {}", e)))?;

        Ok(())
    }

    /// Record a query event
    pub async fn record(&self, event: QueryEvent) {
        let mut buffer = self.buffer.write().await;
        buffer.record(&event);

        // Check if we need to flush
        let now = current_timestamp();
        if now - buffer.last_flush >= self.flush_interval {
            // Flush in background
            if let Some(conn) = &self.conn {
                let stats = self.collect_buffer_stats(&buffer);
                buffer.reset();

                let conn = conn.clone();
                tokio::spawn(async move {
                    if let Err(e) = Self::persist_stats(&conn, stats).await {
                        tracing::warn!("Failed to persist metering stats: {}", e);
                    }
                });
            } else {
                buffer.reset();
            }
        }
    }

    /// Collect statistics from buffer
    fn collect_buffer_stats(&self, buffer: &MeteringBuffer) -> Vec<(String, u64, u64, u64, u64)> {
        let timestamp = (current_timestamp() / 300) * 300; // Round to 5-minute bucket

        buffer
            .queries_by_domain
            .iter()
            .map(|(domain, queries)| {
                let cache_hits = buffer.cache_hits_by_domain.get(domain).copied().unwrap_or(0);
                let avg_latency = if buffer.latencies.is_empty() {
                    0
                } else {
                    buffer.latencies.iter().sum::<u64>() / buffer.latencies.len() as u64
                };
                (domain.clone(), timestamp, *queries, cache_hits, avg_latency * queries)
            })
            .collect()
    }

    /// Persist stats to SQLite
    async fn persist_stats(
        conn: &tokio::sync::Mutex<Connection>,
        stats: Vec<(String, u64, u64, u64, u64)>,
    ) -> Result<(), super::DnsError> {
        let conn = conn.lock().await;

        for (domain, timestamp, queries, cache_hits, total_latency) in stats {
            conn.execute(
                "INSERT INTO dns_metrics (domain, timestamp, queries, cache_hits, total_latency_us)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(domain, timestamp) DO UPDATE SET
                    queries = queries + excluded.queries,
                    cache_hits = cache_hits + excluded.cache_hits,
                    total_latency_us = total_latency_us + excluded.total_latency_us",
                params![domain, timestamp as i64, queries as i64, cache_hits as i64, total_latency as i64],
            )
            .map_err(|e| super::DnsError::ServerError(format!("Failed to insert metrics: {}", e)))?;
        }

        Ok(())
    }

    /// Get statistics for a zone
    pub async fn get_zone_stats(&self, domain: &str, since: Option<u64>) -> ZoneStats {
        let buffer = self.buffer.read().await;

        let since = since.unwrap_or(current_timestamp() - 86400); // Default: last 24 hours
        let now = current_timestamp();

        // Get from buffer first
        let queries = buffer.queries_by_domain.get(domain).copied().unwrap_or(0);
        let cache_hits = buffer.cache_hits_by_domain.get(domain).copied().unwrap_or(0);
        let cache_misses = queries.saturating_sub(cache_hits);

        ZoneStats {
            total_queries: queries,
            queries_by_type: buffer.queries_by_type.clone(),
            queries_by_result: buffer.queries_by_result.clone(),
            queries_by_country: buffer.queries_by_country.clone(),
            cache_hits,
            cache_misses,
            latency_p50_us: buffer.calculate_percentile(50.0),
            latency_p95_us: buffer.calculate_percentile(95.0),
            latency_p99_us: buffer.calculate_percentile(99.0),
            period_start: since,
            period_end: now,
        }
    }

    /// Get global statistics
    pub async fn get_global_stats(&self) -> GlobalStats {
        let buffer = self.buffer.read().await;

        let total_queries = buffer.total_queries;
        let queries_today = total_queries; // In-memory only tracks recent

        let cache_hits: u64 = buffer.cache_hits_by_domain.values().sum();
        let cache_hit_ratio = if total_queries == 0 {
            0.0
        } else {
            cache_hits as f64 / total_queries as f64
        };

        let avg_latency_us = if buffer.latencies.is_empty() {
            0
        } else {
            buffer.latencies.iter().sum::<u64>() / buffer.latencies.len() as u64
        };

        // Top zones
        let mut top_zones: Vec<(String, u64)> = buffer
            .queries_by_domain
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        top_zones.sort_by(|a, b| b.1.cmp(&a.1));
        top_zones.truncate(10);

        GlobalStats {
            total_queries,
            queries_today,
            queries_by_type: buffer.queries_by_type.clone(),
            top_zones,
            cache_hit_ratio,
            avg_latency_us,
        }
    }

    /// Get time-series data for a zone
    pub async fn get_time_series(
        &self,
        domain: &str,
        start: u64,
        end: u64,
        resolution: TimeResolution,
    ) -> Result<Vec<TimeSeriesPoint>, super::DnsError> {
        let conn = match &self.conn {
            Some(c) => c,
            None => return Ok(vec![]), // No persistence, no historical data
        };

        let conn = conn.lock().await;
        let bucket_seconds = resolution.bucket_seconds() as i64;

        let mut stmt = conn
            .prepare(
                "SELECT (timestamp / ?1) * ?1 as bucket,
                        SUM(queries) as total_queries,
                        SUM(cache_hits) as total_cache_hits,
                        SUM(total_latency_us) / MAX(1, SUM(queries)) as avg_latency
                 FROM dns_metrics
                 WHERE domain = ?2 AND timestamp >= ?3 AND timestamp < ?4
                 GROUP BY bucket
                 ORDER BY bucket ASC",
            )
            .map_err(|e| super::DnsError::ServerError(format!("Failed to prepare query: {}", e)))?;

        let points: Vec<TimeSeriesPoint> = stmt
            .query_map(
                params![bucket_seconds, domain, start as i64, end as i64],
                |row| {
                    Ok(TimeSeriesPoint {
                        timestamp: row.get::<_, i64>(0)? as u64,
                        queries: row.get::<_, i64>(1)? as u64,
                        cache_hits: row.get::<_, i64>(2)? as u64,
                        avg_latency_us: row.get::<_, i64>(3)? as u64,
                    })
                },
            )
            .map_err(|e| super::DnsError::ServerError(format!("Failed to query time series: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(points)
    }

    /// Get usage for billing (queries in current period)
    pub async fn get_usage(&self, domain: &str, period_start: u64) -> Result<u64, super::DnsError> {
        let buffer = self.buffer.read().await;
        let buffer_queries = buffer.queries_by_domain.get(domain).copied().unwrap_or(0);

        // If we have persistence, also query historical data
        if let Some(conn) = &self.conn {
            let conn = conn.lock().await;
            let historical: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(queries), 0) FROM dns_metrics
                     WHERE domain = ?1 AND timestamp >= ?2",
                    params![domain, period_start as i64],
                    |row| row.get(0),
                )
                .map_err(|e| super::DnsError::ServerError(format!("Failed to query usage: {}", e)))?;

            return Ok(buffer_queries + historical as u64);
        }

        Ok(buffer_queries)
    }

    /// Force flush buffer to persistence
    pub async fn flush(&self) -> Result<(), super::DnsError> {
        if let Some(conn) = &self.conn {
            let mut buffer = self.buffer.write().await;
            let stats = self.collect_buffer_stats(&buffer);
            buffer.reset();
            Self::persist_stats(conn, stats).await?;
        }
        Ok(())
    }

    /// Clean up old metrics (retention policy)
    pub async fn cleanup(&self, retention_days: u32) -> Result<u64, super::DnsError> {
        let conn = match &self.conn {
            Some(c) => c,
            None => return Ok(0),
        };

        let conn = conn.lock().await;
        let cutoff = current_timestamp() - (retention_days as u64 * 86400);

        let deleted = conn
            .execute(
                "DELETE FROM dns_metrics WHERE timestamp < ?1",
                params![cutoff as i64],
            )
            .map_err(|e| super::DnsError::ServerError(format!("Failed to cleanup metrics: {}", e)))?;

        Ok(deleted as u64)
    }
}

impl Default for DnsMetering {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event(domain: &str) -> QueryEvent {
        QueryEvent {
            domain: domain.to_string(),
            record_type: DnsRecordType::A,
            result: QueryResult::Success,
            latency_us: 500,
            cache_hit: false,
            country: Some("US".to_string()),
            timestamp: current_timestamp(),
        }
    }

    #[tokio::test]
    async fn test_record_and_get_stats() {
        let metering = DnsMetering::new();

        // Record some events
        for _ in 0..10 {
            metering.record(create_test_event("example.com")).await;
        }

        // Add some cache hits
        let mut cache_event = create_test_event("example.com");
        cache_event.cache_hit = true;
        for _ in 0..5 {
            metering.record(cache_event.clone()).await;
        }

        let stats = metering.get_zone_stats("example.com", None).await;
        assert_eq!(stats.total_queries, 15);
        assert_eq!(stats.cache_hits, 5);
        assert_eq!(stats.cache_misses, 10);
    }

    #[tokio::test]
    async fn test_global_stats() {
        let metering = DnsMetering::new();

        // Record events for multiple domains
        for _ in 0..10 {
            metering.record(create_test_event("example.com")).await;
        }
        for _ in 0..5 {
            metering.record(create_test_event("test.com")).await;
        }

        let stats = metering.get_global_stats().await;
        assert_eq!(stats.total_queries, 15);
        assert_eq!(stats.top_zones.len(), 2);
        assert_eq!(stats.top_zones[0].0, "example.com");
        assert_eq!(stats.top_zones[0].1, 10);
    }

    #[tokio::test]
    async fn test_latency_percentiles() {
        let metering = DnsMetering::new();

        // Record events with varying latencies
        for i in 0..100 {
            let mut event = create_test_event("example.com");
            event.latency_us = (i + 1) * 100; // 100, 200, ..., 10000
            metering.record(event).await;
        }

        let stats = metering.get_zone_stats("example.com", None).await;

        // p50 should be around 5000 (50th value = 50 * 100 = 5000)
        assert!(stats.latency_p50_us >= 4500 && stats.latency_p50_us <= 5500);

        // p95 should be around 9500 (95th value = 95 * 100 = 9500)
        assert!(stats.latency_p95_us >= 9000 && stats.latency_p95_us <= 10000);
    }

    #[tokio::test]
    async fn test_queries_by_type() {
        let metering = DnsMetering::new();

        // Record A queries
        for _ in 0..10 {
            metering.record(create_test_event("example.com")).await;
        }

        // Record AAAA queries
        let mut aaaa_event = create_test_event("example.com");
        aaaa_event.record_type = DnsRecordType::AAAA;
        for _ in 0..5 {
            metering.record(aaaa_event.clone()).await;
        }

        let stats = metering.get_zone_stats("example.com", None).await;
        assert_eq!(stats.queries_by_type.get("A"), Some(&10));
        assert_eq!(stats.queries_by_type.get("AAAA"), Some(&5));
    }

    #[tokio::test]
    async fn test_queries_by_country() {
        let metering = DnsMetering::new();

        // US queries
        for _ in 0..10 {
            metering.record(create_test_event("example.com")).await;
        }

        // EU queries
        let mut eu_event = create_test_event("example.com");
        eu_event.country = Some("DE".to_string());
        for _ in 0..5 {
            metering.record(eu_event.clone()).await;
        }

        let stats = metering.get_zone_stats("example.com", None).await;
        assert_eq!(stats.queries_by_country.get("US"), Some(&10));
        assert_eq!(stats.queries_by_country.get("DE"), Some(&5));
    }

    #[tokio::test]
    async fn test_query_results() {
        let metering = DnsMetering::new();

        // Successful queries
        for _ in 0..10 {
            metering.record(create_test_event("example.com")).await;
        }

        // NXDOMAIN queries
        let mut nxdomain_event = create_test_event("example.com");
        nxdomain_event.result = QueryResult::NxDomain;
        for _ in 0..3 {
            metering.record(nxdomain_event.clone()).await;
        }

        let stats = metering.get_zone_stats("example.com", None).await;
        assert_eq!(stats.queries_by_result.get("SUCCESS"), Some(&10));
        assert_eq!(stats.queries_by_result.get("NXDOMAIN"), Some(&3));
    }

    #[tokio::test]
    async fn test_cache_hit_ratio() {
        let metering = DnsMetering::new();

        // Record 60 cache misses
        for _ in 0..60 {
            metering.record(create_test_event("example.com")).await;
        }

        // Record 40 cache hits
        let mut cache_event = create_test_event("example.com");
        cache_event.cache_hit = true;
        for _ in 0..40 {
            metering.record(cache_event.clone()).await;
        }

        let stats = metering.get_zone_stats("example.com", None).await;
        let ratio = stats.cache_hit_ratio();
        assert!((ratio - 0.4).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_get_usage() {
        let metering = DnsMetering::new();

        for _ in 0..100 {
            metering.record(create_test_event("example.com")).await;
        }

        let usage = metering.get_usage("example.com", current_timestamp() - 3600).await.unwrap();
        assert_eq!(usage, 100);
    }

    #[test]
    fn test_query_result_display() {
        assert_eq!(QueryResult::Success.to_string(), "SUCCESS");
        assert_eq!(QueryResult::NxDomain.to_string(), "NXDOMAIN");
        assert_eq!(QueryResult::ServFail.to_string(), "SERVFAIL");
        assert_eq!(QueryResult::RateLimited.to_string(), "RATELIMITED");
    }

    #[test]
    fn test_time_resolution_bucket_seconds() {
        assert_eq!(TimeResolution::Minute.bucket_seconds(), 60);
        assert_eq!(TimeResolution::FiveMinutes.bucket_seconds(), 300);
        assert_eq!(TimeResolution::Hour.bucket_seconds(), 3600);
        assert_eq!(TimeResolution::Day.bucket_seconds(), 86400);
    }

    #[tokio::test]
    async fn test_buffer_circular_behavior() {
        let metering = DnsMetering::new();

        // Record more events than buffer size
        for i in 0..15000 {
            let mut event = create_test_event("example.com");
            event.latency_us = (i % 1000) as u64;
            metering.record(event).await;
        }

        // Should still work and not panic
        let stats = metering.get_zone_stats("example.com", None).await;
        assert!(stats.latency_p50_us > 0);
    }

    #[tokio::test]
    async fn test_empty_stats() {
        let metering = DnsMetering::new();

        let stats = metering.get_zone_stats("nonexistent.com", None).await;
        assert_eq!(stats.total_queries, 0);
        assert_eq!(stats.cache_hit_ratio(), 0.0);
        assert_eq!(stats.latency_p50_us, 0);
    }
}
