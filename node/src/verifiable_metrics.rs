use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::metrics::MetricsCollector;

/// Aggregated metrics for a specific time window
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregatedMetrics {
    /// Time window start (Unix timestamp)
    pub window_start: u64,
    /// Time window end (Unix timestamp)
    pub window_end: u64,
    /// Window duration in seconds
    pub window_duration_secs: u64,

    // Performance metrics
    /// Average request latency in milliseconds
    pub avg_latency_ms: f64,
    /// Median (P50) latency in milliseconds
    pub p50_latency_ms: f64,
    /// P95 latency in milliseconds
    pub p95_latency_ms: f64,
    /// P99 latency in milliseconds
    pub p99_latency_ms: f64,

    // Throughput metrics
    /// Total requests in window
    pub total_requests: u64,
    /// Requests per second
    pub requests_per_second: f64,

    // Cache metrics
    /// Cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: f64,
    /// Total cache hits
    pub cache_hits: u64,
    /// Total cache misses
    pub cache_misses: u64,

    // Security metrics
    /// WAF requests analyzed
    pub waf_requests_analyzed: u64,
    /// WAF requests blocked
    pub waf_requests_blocked: u64,
    /// Bot management challenges issued
    pub bot_challenges: u64,
    /// Bot blocks
    pub bot_blocks: u64,

    // System metrics
    /// Node uptime in seconds
    pub uptime_seconds: u64,
    /// Average CPU usage percentage
    pub avg_cpu_percent: f32,
    /// Average memory usage percentage
    pub avg_memory_percent: f32,
}

impl AggregatedMetrics {
    /// Create a new aggregated metrics snapshot
    pub fn new(window_start: u64, window_end: u64) -> Self {
        Self {
            window_start,
            window_end,
            window_duration_secs: window_end - window_start,
            avg_latency_ms: 0.0,
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            total_requests: 0,
            requests_per_second: 0.0,
            cache_hit_rate: 0.0,
            cache_hits: 0,
            cache_misses: 0,
            waf_requests_analyzed: 0,
            waf_requests_blocked: 0,
            bot_challenges: 0,
            bot_blocks: 0,
            uptime_seconds: 0,
            avg_cpu_percent: 0.0,
            avg_memory_percent: 0.0,
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize metrics to JSON")
    }

    /// Serialize to JSON (pretty printed)
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize metrics to JSON")
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize metrics from JSON")
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        format!(
            "Window: {}-{} ({}s) | Requests: {} ({:.2} req/s) | Latency: {:.2}ms (p50: {:.2}ms, p99: {:.2}ms) | Cache Hit: {:.2}% | WAF Blocks: {} | Bot Blocks: {}",
            self.window_start,
            self.window_end,
            self.window_duration_secs,
            self.total_requests,
            self.requests_per_second,
            self.avg_latency_ms,
            self.p50_latency_ms,
            self.p99_latency_ms,
            self.cache_hit_rate * 100.0,
            self.waf_requests_blocked,
            self.bot_blocks
        )
    }
}

/// Cryptographically signed metric report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMetricReport {
    /// The aggregated metrics
    pub metrics: AggregatedMetrics,
    /// Node operator's public key (hex encoded)
    pub public_key: String,
    /// Ed25519 signature (base64 encoded)
    pub signature: String,
    /// Report generation timestamp (Unix timestamp)
    pub signed_at: u64,
    /// Version of the report format
    pub version: String,
}

impl SignedMetricReport {
    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize signed report to JSON")
    }

    /// Serialize to JSON (pretty printed)
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize signed report to JSON")
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize signed report from JSON")
    }
}

/// Cryptographic key pair for signing metrics
pub struct MetricsSigner {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl MetricsSigner {
    /// Generate a new random key pair
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut rng = rand::rngs::OsRng;
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);

        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();

        info!("Generated new Ed25519 key pair for metrics signing");
        debug!("Public key: {}", hex::encode(verifying_key.to_bytes()));

        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Create from existing private key bytes
    pub fn from_bytes(private_key_bytes: &[u8; 32]) -> Result<Self> {
        let signing_key = SigningKey::from_bytes(private_key_bytes);
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Get the public key as hex string
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    /// Get the public key bytes
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Get the private key bytes
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Sign aggregated metrics
    pub fn sign_metrics(&self, metrics: &AggregatedMetrics) -> Result<SignedMetricReport> {
        // Serialize metrics to canonical JSON
        let metrics_json = metrics.to_json()?;
        let message_bytes = metrics_json.as_bytes();

        // Sign the message
        let signature: Signature = self.signing_key.sign(message_bytes);
        let signature_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signature.to_bytes(),
        );

        let signed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(SignedMetricReport {
            metrics: metrics.clone(),
            public_key: self.public_key_hex(),
            signature: signature_base64,
            signed_at,
            version: "1.0.0".to_string(),
        })
    }

    /// Verify a signed metric report
    pub fn verify_report(&self, report: &SignedMetricReport) -> Result<bool> {
        self.verify_report_with_key(report, &self.verifying_key)
    }

    /// Verify a signed metric report with a specific public key
    pub fn verify_report_with_key(
        &self,
        report: &SignedMetricReport,
        public_key: &VerifyingKey,
    ) -> Result<bool> {
        // Decode signature from base64
        let signature_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &report.signature,
        )
        .context("Failed to decode signature from base64")?;

        let signature = Signature::from_bytes(
            &signature_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
        );

        // Serialize metrics to canonical JSON
        let metrics_json = report.metrics.to_json()?;
        let message_bytes = metrics_json.as_bytes();

        // Verify signature
        match public_key.verify(message_bytes, &signature) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Verify a signed metric report against a public key (standalone function)
pub fn verify_signed_report(report: &SignedMetricReport, public_key_hex: &str) -> Result<bool> {
    // Decode public key from hex
    let public_key_bytes = hex::decode(public_key_hex)
        .context("Failed to decode public key from hex")?;

    let public_key_array: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid public key length"))?;

    let verifying_key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

    // Decode signature from base64
    let signature_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &report.signature,
    )
    .context("Failed to decode signature from base64")?;

    let signature = Signature::from_bytes(
        &signature_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
    );

    // Serialize metrics to canonical JSON
    let metrics_json = report.metrics.to_json()?;
    let message_bytes = metrics_json.as_bytes();

    // Verify signature
    match verifying_key.verify(message_bytes, &signature) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// SQLite storage for signed metric reports
pub struct MetricsStorage {
    db_path: String,
}

impl MetricsStorage {
    /// Create a new metrics storage
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let path_str = db_path.as_ref().to_string_lossy().to_string();
        let storage = Self { db_path: path_str };

        // Initialize database
        storage.init_db()?;

        info!("Initialized metrics storage at: {}", storage.db_path);
        Ok(storage)
    }

    /// Initialize the database schema
    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open SQLite database")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS metric_reports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                window_start INTEGER NOT NULL,
                window_end INTEGER NOT NULL,
                public_key TEXT NOT NULL,
                signature TEXT NOT NULL,
                signed_at INTEGER NOT NULL,
                metrics_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(window_start, window_end, public_key)
            )",
            [],
        )
        .context("Failed to create metric_reports table")?;

        // Create index on window_start for fast queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_window_start ON metric_reports(window_start DESC)",
            [],
        )
        .context("Failed to create index")?;

        Ok(())
    }

    /// Store a signed metric report
    pub fn store_report(&self, report: &SignedMetricReport) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        let metrics_json = report.metrics.to_json()?;
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        conn.execute(
            "INSERT OR REPLACE INTO metric_reports
             (window_start, window_end, public_key, signature, signed_at, metrics_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                report.metrics.window_start,
                report.metrics.window_end,
                report.public_key,
                report.signature,
                report.signed_at,
                metrics_json,
                created_at,
            ],
        )
        .context("Failed to insert metric report")?;

        debug!(
            "Stored metric report for window {}-{}",
            report.metrics.window_start, report.metrics.window_end
        );

        Ok(())
    }

    /// Get the most recent N reports
    pub fn get_recent_reports(&self, limit: usize) -> Result<Vec<SignedMetricReport>> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT window_start, window_end, public_key, signature, signed_at, metrics_json
             FROM metric_reports
             ORDER BY window_start DESC
             LIMIT ?1",
        )?;

        let reports = stmt
            .query_map([limit], |row| {
                let window_start: u64 = row.get(0)?;
                let window_end: u64 = row.get(1)?;
                let public_key: String = row.get(2)?;
                let signature: String = row.get(3)?;
                let signed_at: u64 = row.get(4)?;
                let metrics_json: String = row.get(5)?;

                let metrics = AggregatedMetrics::from_json(&metrics_json)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;

                Ok(SignedMetricReport {
                    metrics,
                    public_key,
                    signature,
                    signed_at,
                    version: "1.0.0".to_string(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(reports)
    }

    /// Get reports for a specific time range
    pub fn get_reports_in_range(&self, start: u64, end: u64) -> Result<Vec<SignedMetricReport>> {
        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT window_start, window_end, public_key, signature, signed_at, metrics_json
             FROM metric_reports
             WHERE window_start >= ?1 AND window_end <= ?2
             ORDER BY window_start DESC",
        )?;

        let reports = stmt
            .query_map([start, end], |row| {
                let window_start: u64 = row.get(0)?;
                let window_end: u64 = row.get(1)?;
                let public_key: String = row.get(2)?;
                let signature: String = row.get(3)?;
                let signed_at: u64 = row.get(4)?;
                let metrics_json: String = row.get(5)?;

                let metrics = AggregatedMetrics::from_json(&metrics_json)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;

                Ok(SignedMetricReport {
                    metrics,
                    public_key,
                    signature,
                    signed_at,
                    version: "1.0.0".to_string(),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(reports)
    }

    /// Delete reports older than the specified timestamp
    pub fn cleanup_old_reports(&self, older_than: u64) -> Result<usize> {
        let conn = Connection::open(&self.db_path)?;

        let deleted = conn.execute(
            "DELETE FROM metric_reports WHERE window_end < ?1",
            params![older_than],
        )?;

        if deleted > 0 {
            info!("Cleaned up {} old metric reports", deleted);
        }

        Ok(deleted)
    }

    /// Get total count of stored reports
    pub fn count_reports(&self) -> Result<usize> {
        let conn = Connection::open(&self.db_path)?;
        let count: usize = conn.query_row("SELECT COUNT(*) FROM metric_reports", [], |row| {
            row.get(0)
        })?;
        Ok(count)
    }
}

/// Verifiable metrics aggregator
pub struct VerifiableMetricsAggregator {
    collector: Arc<MetricsCollector>,
    signer: Arc<MetricsSigner>,
    storage: Arc<MetricsStorage>,
    window_duration_secs: u64,
    last_aggregation: Arc<RwLock<Option<AggregatedMetrics>>>,
}

impl VerifiableMetricsAggregator {
    /// Create a new verifiable metrics aggregator
    pub fn new(
        collector: Arc<MetricsCollector>,
        signer: MetricsSigner,
        storage_path: &str,
        window_duration_secs: u64,
    ) -> Result<Self> {
        let storage = MetricsStorage::new(storage_path)?;

        info!(
            "Created verifiable metrics aggregator (window: {}s)",
            window_duration_secs
        );

        Ok(Self {
            collector,
            signer: Arc::new(signer),
            storage: Arc::new(storage),
            window_duration_secs,
            last_aggregation: Arc::new(RwLock::new(None)),
        })
    }

    /// Aggregate current metrics and create a signed report
    pub async fn aggregate_and_sign(&self) -> Result<SignedMetricReport> {
        // Get current timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let window_end = now;
        let window_start = window_end - self.window_duration_secs;

        // Get current metrics snapshot
        let current_metrics = self.collector.get_metrics().await;

        // Create aggregated metrics
        let mut aggregated = AggregatedMetrics::new(window_start, window_end);

        // Copy metrics from collector
        aggregated.avg_latency_ms = current_metrics.avg_latency_ms;
        aggregated.p50_latency_ms = current_metrics.p50_latency_ms;
        aggregated.p95_latency_ms = current_metrics.p95_latency_ms;
        aggregated.p99_latency_ms = current_metrics.p99_latency_ms;
        aggregated.total_requests = current_metrics.requests_total;
        aggregated.requests_per_second = current_metrics.requests_per_second;
        aggregated.cache_hit_rate = current_metrics.cache_hit_rate;
        aggregated.cache_hits = current_metrics.cache_hits;
        aggregated.cache_misses = current_metrics.cache_misses;
        aggregated.waf_requests_analyzed = current_metrics.waf_requests_analyzed;
        aggregated.waf_requests_blocked = current_metrics.waf_requests_blocked;
        aggregated.bot_challenges = 0; // TODO: Track from bot management
        aggregated.bot_blocks = 0; // TODO: Track from bot management
        aggregated.uptime_seconds = current_metrics.uptime_seconds;
        aggregated.avg_cpu_percent = current_metrics.cpu_usage_percent;
        aggregated.avg_memory_percent = current_metrics.memory_percent;

        // Store last aggregation
        *self.last_aggregation.write().await = Some(aggregated.clone());

        // Sign the metrics
        let signed_report = self.signer.sign_metrics(&aggregated)?;

        // Store in database
        self.storage.store_report(&signed_report)?;

        info!(
            "Aggregated and signed metrics: {}",
            aggregated.summary()
        );

        Ok(signed_report)
    }

    /// Get the last aggregated metrics (without signature)
    pub async fn get_last_aggregation(&self) -> Option<AggregatedMetrics> {
        self.last_aggregation.read().await.clone()
    }

    /// Get recent signed reports from storage
    pub fn get_recent_reports(&self, limit: usize) -> Result<Vec<SignedMetricReport>> {
        self.storage.get_recent_reports(limit)
    }

    /// Get reports in a time range
    pub fn get_reports_in_range(&self, start: u64, end: u64) -> Result<Vec<SignedMetricReport>> {
        self.storage.get_reports_in_range(start, end)
    }

    /// Clean up old reports
    pub fn cleanup_old_reports(&self, older_than: u64) -> Result<usize> {
        self.storage.cleanup_old_reports(older_than)
    }

    /// Get the public key as hex string
    pub fn public_key_hex(&self) -> String {
        self.signer.public_key_hex()
    }

    /// Verify a report
    pub fn verify_report(&self, report: &SignedMetricReport) -> Result<bool> {
        self.signer.verify_report(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_aggregated_metrics_serialization() {
        let metrics = AggregatedMetrics {
            window_start: 1000,
            window_end: 1300,
            window_duration_secs: 300,
            avg_latency_ms: 15.5,
            p50_latency_ms: 12.0,
            p95_latency_ms: 45.0,
            p99_latency_ms: 120.0,
            total_requests: 5000,
            requests_per_second: 16.67,
            cache_hit_rate: 0.85,
            cache_hits: 4250,
            cache_misses: 750,
            waf_requests_analyzed: 5000,
            waf_requests_blocked: 25,
            bot_challenges: 10,
            bot_blocks: 5,
            uptime_seconds: 86400,
            avg_cpu_percent: 45.2,
            avg_memory_percent: 62.5,
        };

        let json = metrics.to_json().unwrap();
        let deserialized = AggregatedMetrics::from_json(&json).unwrap();

        assert_eq!(metrics, deserialized);
    }

    #[test]
    fn test_metrics_signer_generation() {
        let signer = MetricsSigner::generate();
        let public_key = signer.public_key_hex();

        assert_eq!(public_key.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_sign_and_verify() {
        let signer = MetricsSigner::generate();

        let metrics = AggregatedMetrics::new(1000, 1300);

        let signed_report = signer.sign_metrics(&metrics).unwrap();

        // Verify with same signer
        assert!(signer.verify_report(&signed_report).unwrap());

        // Verify with public key function
        assert!(verify_signed_report(&signed_report, &signer.public_key_hex()).unwrap());
    }

    #[test]
    fn test_tampered_metrics_fail_verification() {
        let signer = MetricsSigner::generate();

        let metrics = AggregatedMetrics::new(1000, 1300);

        let mut signed_report = signer.sign_metrics(&metrics).unwrap();

        // Tamper with metrics
        signed_report.metrics.total_requests = 99999;

        // Verification should fail
        assert!(!signer.verify_report(&signed_report).unwrap());
    }

    #[test]
    fn test_different_key_fails_verification() {
        let signer1 = MetricsSigner::generate();
        let signer2 = MetricsSigner::generate();

        let metrics = AggregatedMetrics::new(1000, 1300);

        let signed_report = signer1.sign_metrics(&metrics).unwrap();

        // Verify with different signer should fail
        assert!(!signer2.verify_report(&signed_report).unwrap());
    }

    #[test]
    fn test_storage_create_and_retrieve() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = MetricsStorage::new(temp_file.path()).unwrap();

        let signer = MetricsSigner::generate();
        let metrics = AggregatedMetrics::new(1000, 1300);
        let signed_report = signer.sign_metrics(&metrics).unwrap();

        // Store report
        storage.store_report(&signed_report).unwrap();

        // Retrieve reports
        let reports = storage.get_recent_reports(10).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].metrics, signed_report.metrics);
    }

    #[test]
    fn test_storage_multiple_reports() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = MetricsStorage::new(temp_file.path()).unwrap();

        let signer = MetricsSigner::generate();

        // Store 5 reports with different time windows
        for i in 0..5 {
            let start = 1000 + (i * 300);
            let end = start + 300;
            let metrics = AggregatedMetrics::new(start, end);
            let signed_report = signer.sign_metrics(&metrics).unwrap();
            storage.store_report(&signed_report).unwrap();
        }

        // Retrieve all reports
        let reports = storage.get_recent_reports(10).unwrap();
        assert_eq!(reports.len(), 5);

        // Verify they're in descending order
        assert!(reports[0].metrics.window_start > reports[1].metrics.window_start);
    }

    #[test]
    fn test_storage_time_range_query() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = MetricsStorage::new(temp_file.path()).unwrap();

        let signer = MetricsSigner::generate();

        // Store reports for different time windows
        for i in 0..10 {
            let start = 1000 + (i * 300);
            let end = start + 300;
            let metrics = AggregatedMetrics::new(start, end);
            let signed_report = signer.sign_metrics(&metrics).unwrap();
            storage.store_report(&signed_report).unwrap();
        }

        // Query for specific range (windows 2-5)
        let reports = storage.get_reports_in_range(1600, 2200).unwrap();
        assert_eq!(reports.len(), 2); // Windows starting at 1600 and 1900
    }

    #[test]
    fn test_storage_cleanup() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = MetricsStorage::new(temp_file.path()).unwrap();

        let signer = MetricsSigner::generate();

        // Store 5 reports
        for i in 0..5 {
            let start = 1000 + (i * 300);
            let end = start + 300;
            let metrics = AggregatedMetrics::new(start, end);
            let signed_report = signer.sign_metrics(&metrics).unwrap();
            storage.store_report(&signed_report).unwrap();
        }

        assert_eq!(storage.count_reports().unwrap(), 5);

        // Delete reports older than timestamp 1900 (window_end < 1900)
        let deleted = storage.cleanup_old_reports(1900).unwrap();
        assert_eq!(deleted, 2); // Windows ending at 1300, 1600

        assert_eq!(storage.count_reports().unwrap(), 3);
    }

    #[test]
    fn test_signer_from_bytes() {
        let signer1 = MetricsSigner::generate();
        let private_key = signer1.private_key_bytes();

        // Create new signer from same private key
        let signer2 = MetricsSigner::from_bytes(&private_key).unwrap();

        // Should have same public key
        assert_eq!(signer1.public_key_hex(), signer2.public_key_hex());

        // Should produce verifiable signatures
        let metrics = AggregatedMetrics::new(1000, 1300);
        let report1 = signer1.sign_metrics(&metrics).unwrap();
        let report2 = signer2.sign_metrics(&metrics).unwrap();

        // Both signers should verify each other's signatures
        assert!(signer1.verify_report(&report2).unwrap());
        assert!(signer2.verify_report(&report1).unwrap());
    }

    #[test]
    fn test_metrics_summary() {
        let metrics = AggregatedMetrics {
            window_start: 1000,
            window_end: 1300,
            window_duration_secs: 300,
            avg_latency_ms: 15.5,
            p50_latency_ms: 12.0,
            p95_latency_ms: 45.0,
            p99_latency_ms: 120.0,
            total_requests: 5000,
            requests_per_second: 16.67,
            cache_hit_rate: 0.85,
            cache_hits: 4250,
            cache_misses: 750,
            waf_requests_analyzed: 5000,
            waf_requests_blocked: 25,
            bot_challenges: 10,
            bot_blocks: 5,
            uptime_seconds: 86400,
            avg_cpu_percent: 45.2,
            avg_memory_percent: 62.5,
        };

        let summary = metrics.summary();
        assert!(summary.contains("5000"));
        assert!(summary.contains("16.67"));
        assert!(summary.contains("85.00%"));
    }

    #[tokio::test]
    async fn test_aggregator_creation() {
        let collector = Arc::new(MetricsCollector::new());
        let signer = MetricsSigner::generate();
        let temp_file = NamedTempFile::new().unwrap();

        let aggregator = VerifiableMetricsAggregator::new(
            collector,
            signer,
            temp_file.path().to_str().unwrap(),
            300,
        )
        .unwrap();

        assert_eq!(aggregator.public_key_hex().len(), 64);
    }

    #[tokio::test]
    async fn test_aggregator_aggregate_and_sign() {
        let collector = Arc::new(MetricsCollector::new());
        let signer = MetricsSigner::generate();
        let temp_file = NamedTempFile::new().unwrap();

        let aggregator = VerifiableMetricsAggregator::new(
            collector.clone(),
            signer,
            temp_file.path().to_str().unwrap(),
            300,
        )
        .unwrap();

        // Record some metrics
        collector.record_request(15.5).await;
        collector.record_cache_hit().await;

        // Aggregate and sign
        let signed_report = aggregator.aggregate_and_sign().await.unwrap();

        // Verify the report
        assert!(aggregator.verify_report(&signed_report).unwrap());

        // Check that it was stored
        let reports = aggregator.get_recent_reports(10).unwrap();
        assert_eq!(reports.len(), 1);
    }
}
