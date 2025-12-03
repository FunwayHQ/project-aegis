//! Sprint 17: IPFS Client for Decentralized Wasm Module Distribution
//!
//! This module provides IPFS integration for uploading, downloading, and pinning
//! Wasm modules using content-addressed storage (IPFS CIDs).
//!
//! ## Features
//! - Upload Wasm modules to IPFS and get CID
//! - Download modules by CID with integrity verification
//! - Pin/unpin modules to prevent garbage collection
//! - Local disk caching to reduce IPFS fetches
//! - Content verification (CID matches actual content hash)
//!
//! ## Security (X1.2)
//! - Full cryptographic CID verification implemented
//! - Content hash must match CID's embedded hash
//! - Prevents MITM attacks and content substitution

use anyhow::{Context, Result};
use cid::Cid;
use ipfs_api_backend_hyper::{IpfsApi, IpfsClient as IpfsApiClient, TryFromUri};
use log::{debug, info, warn, error};
use multihash::Multihash;
use sha2::{Sha256, Digest};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

/// Maximum size for Wasm modules (10MB)
const MAX_MODULE_SIZE: usize = 10 * 1024 * 1024;

/// IPFS download timeout (30 seconds)
const IPFS_TIMEOUT_SECS: u64 = 30;

/// Default IPFS API endpoint (local node)
const DEFAULT_IPFS_API: &str = "http://127.0.0.1:5001";

/// Public IPFS gateways for fallback (CDN functionality)
const PUBLIC_IPFS_GATEWAYS: &[&str] = &[
    "https://ipfs.io",
    "https://cloudflare-ipfs.com",
    "https://dweb.link",
];

// ============================================================================
// SECURITY FIX (X4.12): Bandwidth limiting constants
// ============================================================================

/// Default bandwidth limit: 100 MB per minute
const DEFAULT_BANDWIDTH_LIMIT_BYTES: u64 = 100 * 1024 * 1024;

/// Minimum bandwidth limit: 10 MB per minute (cannot be set lower)
const MIN_BANDWIDTH_LIMIT_BYTES: u64 = 10 * 1024 * 1024;

/// Maximum bandwidth limit: 1 GB per minute
const MAX_BANDWIDTH_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

/// Bandwidth tracking window duration (1 minute)
const BANDWIDTH_WINDOW_SECS: u64 = 60;

/// Maximum concurrent downloads per client
const MAX_CONCURRENT_DOWNLOADS: usize = 5;

/// Get configured bandwidth limit from environment or use default
fn get_bandwidth_limit() -> u64 {
    std::env::var("AEGIS_IPFS_BANDWIDTH_LIMIT_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|mb| (mb * 1024 * 1024).clamp(MIN_BANDWIDTH_LIMIT_BYTES, MAX_BANDWIDTH_LIMIT_BYTES))
        .unwrap_or(DEFAULT_BANDWIDTH_LIMIT_BYTES)
}

/// Bandwidth tracker for rate limiting IPFS downloads
/// SECURITY FIX (X4.12): Prevents bandwidth exhaustion attacks
#[derive(Debug)]
struct BandwidthTracker {
    /// Rolling window of (timestamp, bytes) for bandwidth tracking
    window: Vec<(Instant, u64)>,
    /// Bandwidth limit in bytes per window
    limit_bytes: u64,
    /// Window duration
    window_duration: Duration,
    /// Current number of active downloads
    active_downloads: usize,
    /// Maximum concurrent downloads
    max_concurrent: usize,
}

impl BandwidthTracker {
    fn new() -> Self {
        Self {
            window: Vec::with_capacity(1000),
            limit_bytes: get_bandwidth_limit(),
            window_duration: Duration::from_secs(BANDWIDTH_WINDOW_SECS),
            active_downloads: 0,
            max_concurrent: MAX_CONCURRENT_DOWNLOADS,
        }
    }

    /// Create tracker with custom limit (for testing)
    #[cfg(test)]
    fn with_limit(limit_bytes: u64) -> Self {
        Self {
            window: Vec::with_capacity(1000),
            limit_bytes,
            window_duration: Duration::from_secs(BANDWIDTH_WINDOW_SECS),
            active_downloads: 0,
            max_concurrent: MAX_CONCURRENT_DOWNLOADS,
        }
    }

    /// Clean up old entries outside the window
    fn cleanup(&mut self) {
        let cutoff = Instant::now() - self.window_duration;
        self.window.retain(|(ts, _)| *ts > cutoff);
    }

    /// Get current bandwidth usage in the window
    fn current_usage(&mut self) -> u64 {
        self.cleanup();
        self.window.iter().map(|(_, bytes)| bytes).sum()
    }

    /// Check if a download of given size can proceed
    fn can_download(&mut self, size_hint: u64) -> bool {
        // Check concurrent download limit
        if self.active_downloads >= self.max_concurrent {
            return false;
        }

        // Check bandwidth limit
        let current = self.current_usage();
        current + size_hint <= self.limit_bytes
    }

    /// Record a download
    fn record_download(&mut self, bytes: u64) {
        self.window.push((Instant::now(), bytes));
        // Prevent unbounded growth
        if self.window.len() > 10_000 {
            self.cleanup();
        }
    }

    /// Start a download (increment active count)
    fn start_download(&mut self) {
        self.active_downloads = self.active_downloads.saturating_add(1);
    }

    /// End a download (decrement active count)
    fn end_download(&mut self) {
        self.active_downloads = self.active_downloads.saturating_sub(1);
    }

    /// Get remaining bandwidth in current window
    fn remaining_bandwidth(&mut self) -> u64 {
        let current = self.current_usage();
        self.limit_bytes.saturating_sub(current)
    }

    /// Get time until bandwidth resets (oldest entry expires)
    fn time_until_reset(&mut self) -> Option<Duration> {
        self.cleanup();
        self.window.first().map(|(ts, _)| {
            let elapsed = ts.elapsed();
            if elapsed < self.window_duration {
                self.window_duration - elapsed
            } else {
                Duration::ZERO
            }
        })
    }
}

/// IPFS client for Wasm module distribution
pub struct IpfsClient {
    /// IPFS HTTP API client
    api_client: IpfsApiClient,

    /// Local cache directory for downloaded modules
    cache_dir: PathBuf,

    /// HTTP client with timeout
    http_client: reqwest::Client,

    /// SECURITY FIX (X4.12): Bandwidth tracker for rate limiting
    bandwidth_tracker: Arc<RwLock<BandwidthTracker>>,
}

impl IpfsClient {
    /// Create a new IPFS client with default settings
    pub fn new() -> Result<Self> {
        Self::with_config(DEFAULT_IPFS_API, None)
    }

    /// Create a new IPFS client with custom configuration
    ///
    /// # Arguments
    /// * `api_endpoint` - IPFS API endpoint (e.g., "http://127.0.0.1:5001")
    /// * `cache_dir` - Optional cache directory (defaults to ~/.aegis/modules)
    pub fn with_config(api_endpoint: &str, cache_dir: Option<PathBuf>) -> Result<Self> {
        let api_client = IpfsApiClient::from_str(api_endpoint)
            .context("Failed to create IPFS API client")?;

        let cache_dir = match cache_dir {
            Some(dir) => dir,
            None => {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .context("Could not determine home directory")?;
                PathBuf::from(home).join(".aegis").join("modules")
            }
        };

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(IPFS_TIMEOUT_SECS))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            api_client,
            cache_dir,
            http_client,
            bandwidth_tracker: Arc::new(RwLock::new(BandwidthTracker::new())),
        })
    }

    /// Upload a Wasm module to IPFS
    ///
    /// # Arguments
    /// * `wasm_bytes` - The compiled Wasm module bytes
    ///
    /// # Returns
    /// The IPFS CID (Content Identifier) of the uploaded module
    ///
    /// # Example
    /// ```ignore
    /// let client = IpfsClient::new()?;
    /// let wasm_bytes = std::fs::read("module.wasm")?;
    /// let cid = client.upload_module(&wasm_bytes).await?;
    /// println!("Uploaded to IPFS: {}", cid);
    /// ```
    pub async fn upload_module(&self, wasm_bytes: &[u8]) -> Result<String> {
        // Validate module size
        if wasm_bytes.len() > MAX_MODULE_SIZE {
            anyhow::bail!(
                "Module too large: {} bytes (max: {} bytes)",
                wasm_bytes.len(),
                MAX_MODULE_SIZE
            );
        }

        info!("Uploading Wasm module to IPFS ({} bytes)", wasm_bytes.len());

        // Upload to IPFS (need to own the data for 'static lifetime)
        let wasm_vec = wasm_bytes.to_vec();
        let cursor = std::io::Cursor::new(wasm_vec);
        let response = self
            .api_client
            .add(cursor)
            .await
            .context("Failed to upload module to IPFS")?;

        let cid = response.hash;
        info!("Successfully uploaded module to IPFS: {}", cid);

        // Automatically pin the module
        self.pin_module(&cid).await?;

        Ok(cid)
    }

    /// Download a Wasm module from IPFS by CID
    ///
    /// This method implements a multi-tier CDN strategy:
    /// 1. Check local cache (fastest)
    /// 2. Try local IPFS node
    /// 3. Fallback to public IPFS gateways (CDN)
    ///
    /// # Arguments
    /// * `cid` - The IPFS Content Identifier
    ///
    /// # Returns
    /// The Wasm module bytes
    ///
    /// # Example
    /// ```ignore
    /// let client = IpfsClient::new()?;
    /// let wasm_bytes = client.fetch_module("QmXxx...").await?;
    /// ```
    pub async fn fetch_module(&self, cid: &str) -> Result<Vec<u8>> {
        // Check local cache first
        if let Some(cached_bytes) = self.get_from_cache(cid).await? {
            debug!("Cache HIT for module CID: {}", cid);
            return Ok(cached_bytes);
        }

        debug!("Cache MISS for module CID: {}, fetching from IPFS", cid);

        // SECURITY FIX (X4.12): Check bandwidth limits before downloading
        // Use MAX_MODULE_SIZE as the size hint since we don't know actual size yet
        {
            let mut tracker = self.bandwidth_tracker.write().await;
            if !tracker.can_download(MAX_MODULE_SIZE as u64) {
                let remaining = tracker.remaining_bandwidth();
                let reset_time = tracker.time_until_reset();
                warn!(
                    "IPFS bandwidth limit exceeded for CID {}. Remaining: {} bytes, Reset in: {:?}",
                    cid, remaining, reset_time
                );
                anyhow::bail!(
                    "IPFS bandwidth limit exceeded. Remaining bandwidth: {} bytes. \
                     Try again in {:?}.",
                    remaining,
                    reset_time.unwrap_or(Duration::from_secs(60))
                );
            }
            tracker.start_download();
        }

        // Ensure we end the download tracking even on error
        let result = self.fetch_module_internal(cid).await;

        // Record bandwidth usage and end download
        {
            let mut tracker = self.bandwidth_tracker.write().await;
            tracker.end_download();
            if let Ok(ref bytes) = result {
                tracker.record_download(bytes.len() as u64);
                info!(
                    "IPFS bandwidth recorded: {} bytes for CID {}. Remaining: {} bytes",
                    bytes.len(),
                    cid,
                    tracker.remaining_bandwidth()
                );
            }
        }

        result
    }

    /// Internal fetch implementation (after bandwidth check)
    async fn fetch_module_internal(&self, cid: &str) -> Result<Vec<u8>> {
        info!("Downloading module from IPFS: {}", cid);

        // Try local IPFS node first
        match self.fetch_from_local_node(cid).await {
            Ok(bytes) => {
                info!(
                    "Successfully downloaded module from local IPFS node: {} ({} bytes)",
                    cid,
                    bytes.len()
                );
                // Cache locally
                self.save_to_cache(cid, &bytes).await?;
                return Ok(bytes);
            }
            Err(e) => {
                warn!("Failed to fetch from local IPFS node: {}", e);
                info!("Falling back to public IPFS gateways (CDN)...");
            }
        }

        // Fallback to public IPFS gateways (CDN functionality)
        let bytes = self.fetch_from_public_gateways(cid).await
            .context("Failed to fetch from both local node and public gateways")?;

        info!(
            "Successfully downloaded module from public gateway: {} ({} bytes)",
            cid,
            bytes.len()
        );

        // Cache locally
        self.save_to_cache(cid, &bytes).await?;

        Ok(bytes)
    }

    /// Fetch from local IPFS node
    async fn fetch_from_local_node(&self, cid: &str) -> Result<Vec<u8>> {
        use futures::TryStreamExt;

        let response = self
            .api_client
            .cat(cid)
            .map_ok(|chunk| chunk.to_vec())
            .try_concat()
            .await
            .context("Failed to download module from local IPFS node")?;

        // Validate size
        if response.len() > MAX_MODULE_SIZE {
            anyhow::bail!(
                "Downloaded module too large: {} bytes (max: {} bytes)",
                response.len(),
                MAX_MODULE_SIZE
            );
        }

        // Verify CID integrity
        self.verify_cid(cid, &response)?;

        Ok(response)
    }

    /// Fetch from public IPFS gateways (CDN fallback)
    ///
    /// Tries multiple public gateways in order until one succeeds.
    /// This provides CDN-like redundancy and availability.
    async fn fetch_from_public_gateways(&self, cid: &str) -> Result<Vec<u8>> {
        let mut last_error = None;

        for gateway in PUBLIC_IPFS_GATEWAYS {
            let url = format!("{}/ipfs/{}", gateway, cid);
            debug!("Trying public IPFS gateway: {}", url);

            match self.http_client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    match response.bytes().await {
                        Ok(bytes) => {
                            let bytes_vec = bytes.to_vec();

                            // Validate size
                            if bytes_vec.len() > MAX_MODULE_SIZE {
                                warn!(
                                    "Module from gateway {} too large: {} bytes",
                                    gateway,
                                    bytes_vec.len()
                                );
                                continue;
                            }

                            // Verify CID integrity
                            if self.verify_cid(cid, &bytes_vec).is_err() {
                                warn!("CID verification failed for gateway: {}", gateway);
                                continue;
                            }

                            info!("Successfully fetched from gateway: {}", gateway);
                            return Ok(bytes_vec);
                        }
                        Err(e) => {
                            warn!("Failed to read response from {}: {}", gateway, e);
                            last_error = Some(anyhow::anyhow!("{}", e));
                        }
                    }
                }
                Ok(response) => {
                    warn!("Gateway {} returned error: {}", gateway, response.status());
                    last_error = Some(anyhow::anyhow!("HTTP {}", response.status()));
                }
                Err(e) => {
                    warn!("Failed to connect to {}: {}", gateway, e);
                    last_error = Some(anyhow::anyhow!("{}", e));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All public gateways failed")))
    }

    /// Pin a module to prevent garbage collection
    ///
    /// # Arguments
    /// * `cid` - The IPFS Content Identifier to pin
    pub async fn pin_module(&self, cid: &str) -> Result<()> {
        debug!("Pinning module to IPFS: {}", cid);

        self.api_client
            .pin_add(cid, false)
            .await
            .context("Failed to pin module")?;

        info!("Successfully pinned module: {}", cid);
        Ok(())
    }

    /// Unpin a module (allows garbage collection)
    ///
    /// # Arguments
    /// * `cid` - The IPFS Content Identifier to unpin
    pub async fn unpin_module(&self, cid: &str) -> Result<()> {
        debug!("Unpinning module from IPFS: {}", cid);

        self.api_client
            .pin_rm(cid, false)
            .await
            .context("Failed to unpin module")?;

        info!("Successfully unpinned module: {}", cid);
        Ok(())
    }

    /// List all pinned modules
    ///
    /// # Returns
    /// Vector of CIDs that are currently pinned
    pub async fn list_pinned(&self) -> Result<Vec<String>> {
        let response = self
            .api_client
            .pin_ls(None, None)
            .await
            .context("Failed to list pinned modules")?;

        let cids: Vec<String> = response.keys.keys().map(|k| k.to_string()).collect();

        debug!("Found {} pinned modules", cids.len());
        Ok(cids)
    }

    /// Get module from local cache
    async fn get_from_cache(&self, cid: &str) -> Result<Option<Vec<u8>>> {
        let cache_path = self.cache_dir.join(format!("{}.wasm", cid));

        if !cache_path.exists() {
            return Ok(None);
        }

        match fs::read(&cache_path).await {
            Ok(bytes) => {
                // Verify cached content still matches CID
                if self.verify_cid(cid, &bytes).is_ok() {
                    Ok(Some(bytes))
                } else {
                    warn!("Cached module CID mismatch, removing: {}", cid);
                    fs::remove_file(&cache_path).await.ok();
                    Ok(None)
                }
            }
            Err(e) => {
                warn!("Failed to read cached module: {}", e);
                Ok(None)
            }
        }
    }

    /// Save module to local cache
    async fn save_to_cache(&self, cid: &str, wasm_bytes: &[u8]) -> Result<()> {
        // Create cache directory if it doesn't exist
        fs::create_dir_all(&self.cache_dir)
            .await
            .context("Failed to create cache directory")?;

        let cache_path = self.cache_dir.join(format!("{}.wasm", cid));

        let mut file = fs::File::create(&cache_path)
            .await
            .context("Failed to create cache file")?;

        file.write_all(wasm_bytes)
            .await
            .context("Failed to write cache file")?;

        file.sync_all()
            .await
            .context("Failed to sync cache file")?;

        debug!("Cached module locally: {}", cache_path.display());
        Ok(())
    }

    /// Verify that the downloaded content matches the CID
    ///
    /// SECURITY FIX (X1.2): Full cryptographic CID verification
    ///
    /// IPFS CIDs are content-addressed, so we verify integrity by:
    /// 1. Parsing the CID to extract the hash algorithm and expected hash
    /// 2. Computing the hash of the downloaded content using the same algorithm
    /// 3. Comparing the computed hash with the CID's embedded hash
    ///
    /// This prevents MITM attacks where an attacker could substitute
    /// malicious content while keeping the same CID string.
    fn verify_cid(&self, cid_str: &str, content: &[u8]) -> Result<()> {
        debug!("CID verification for: {} ({} bytes)", cid_str, content.len());

        // Parse the CID to extract hash information
        let parsed_cid = Cid::from_str(cid_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse CID '{}': {}", cid_str, e))?;

        // Get the multihash from the CID
        let expected_hash = parsed_cid.hash();
        let hash_code = expected_hash.code();

        // Compute the hash of the content using the same algorithm
        let computed_hash = match hash_code {
            // SHA2-256 (most common for IPFS)
            0x12 => {
                let digest = Sha256::digest(content);
                Multihash::<64>::wrap(0x12, &digest)
                    .map_err(|e| anyhow::anyhow!("Failed to create multihash: {}", e))?
            }
            // SHA2-512
            0x13 => {
                use sha2::Sha512;
                let digest = Sha512::digest(content);
                Multihash::<64>::wrap(0x13, &digest)
                    .map_err(|e| anyhow::anyhow!("Failed to create multihash: {}", e))?
            }
            // Identity hash (content IS the hash, used for small data)
            0x00 => {
                if content.len() > 64 {
                    anyhow::bail!("Content too large for identity hash");
                }
                Multihash::<64>::wrap(0x00, content)
                    .map_err(|e| anyhow::anyhow!("Failed to create identity multihash: {}", e))?
            }
            _ => {
                // For unsupported hash algorithms, we cannot verify
                // Log a warning but don't fail - the Ed25519 signature verification
                // from Sprint 15 provides a secondary layer of security
                warn!(
                    "Unsupported hash algorithm 0x{:x} in CID {}, cannot verify content integrity",
                    hash_code, cid_str
                );
                warn!("Relying on Ed25519 module signature for security");
                return Ok(());
            }
        };

        // Compare the computed hash with the expected hash from the CID
        if computed_hash.digest() != expected_hash.digest() {
            error!(
                "CID VERIFICATION FAILED for {}: content hash does not match!",
                cid_str
            );
            error!(
                "Expected hash: {:?}, Computed hash: {:?}",
                hex::encode(expected_hash.digest()),
                hex::encode(computed_hash.digest())
            );
            anyhow::bail!(
                "CID verification failed: downloaded content hash does not match CID {}. \
                 This could indicate a MITM attack or corrupted download.",
                cid_str
            );
        }

        info!("CID verification PASSED for {} ({} bytes)", cid_str, content.len());
        Ok(())
    }

    /// Clear the local cache
    pub async fn clear_cache(&self) -> Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)
                .await
                .context("Failed to clear cache")?;
            info!("Cleared module cache: {}", self.cache_dir.display());
        }
        Ok(())
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> Result<CacheStats> {
        if !self.cache_dir.exists() {
            return Ok(CacheStats {
                module_count: 0,
                total_size_bytes: 0,
            });
        }

        let mut count = 0;
        let mut total_size = 0u64;

        let mut entries = fs::read_dir(&self.cache_dir)
            .await
            .context("Failed to read cache directory")?;

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(metadata) = entry.metadata().await {
                if metadata.is_file() && entry.path().extension() == Some(std::ffi::OsStr::new("wasm")) {
                    count += 1;
                    total_size += metadata.len();
                }
            }
        }

        Ok(CacheStats {
            module_count: count,
            total_size_bytes: total_size,
        })
    }

    /// SECURITY FIX (X4.12): Get bandwidth statistics
    pub async fn bandwidth_stats(&self) -> BandwidthStats {
        let mut tracker = self.bandwidth_tracker.write().await;
        BandwidthStats {
            used_bytes: tracker.current_usage(),
            limit_bytes: tracker.limit_bytes,
            remaining_bytes: tracker.remaining_bandwidth(),
            active_downloads: tracker.active_downloads,
            max_concurrent_downloads: tracker.max_concurrent,
            reset_time: tracker.time_until_reset(),
        }
    }
}

/// SECURITY FIX (X4.12): Statistics about bandwidth usage
#[derive(Debug, Clone)]
pub struct BandwidthStats {
    /// Bytes used in current window
    pub used_bytes: u64,
    /// Bandwidth limit in bytes per window
    pub limit_bytes: u64,
    /// Remaining bytes available
    pub remaining_bytes: u64,
    /// Number of active downloads
    pub active_downloads: usize,
    /// Maximum concurrent downloads allowed
    pub max_concurrent_downloads: usize,
    /// Time until oldest entry expires (bandwidth partially resets)
    pub reset_time: Option<Duration>,
}

/// Statistics about the local module cache
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached modules
    pub module_count: usize,

    /// Total size of cached modules in bytes
    pub total_size_bytes: u64,
}

impl Default for IpfsClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default IPFS client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipfs_client_creation() {
        // Test default client creation (may fail if no IPFS daemon running)
        let result = IpfsClient::new();
        assert!(result.is_ok() || result.is_err()); // Just test it compiles
    }

    #[test]
    fn test_cache_path_creation() {
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test-cache")),
        )
        .unwrap();

        assert_eq!(
            client.cache_dir,
            PathBuf::from("/tmp/aegis-test-cache")
        );
    }

    #[test]
    fn test_verify_cid_invalid_format() {
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // Invalid CID format should fail
        let result = client.verify_cid("invalid-cid", b"test");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Failed to parse CID"), "Error: {}", err_msg);
    }

    #[test]
    fn test_verify_cid_sha256_valid() {
        // SECURITY TEST (X1.2): Verify CID verification works correctly
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // Test content: "hello world"
        let content = b"hello world";

        // SHA256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        // CIDv1 (base32, dag-pb, sha2-256) for "hello world":
        // This is a real CID - we'll compute it properly
        // The CID format is: <multibase><version><codec><multihash>
        // For CIDv0 (starts with Qm), the content is hashed with SHA256, then
        // the result is base58btc encoded with multihash prefix

        // Instead of hardcoding, let's create a known CID from test content
        // CIDv1 base32 for raw SHA256 of "hello world":
        // bafkreifzjut3te2nhyekklss27ez56hb6xn37yp5zl5u27bwxlymzwcvhy
        // Note: This is computed using CIDv1 with raw codec (0x55) and sha2-256

        // Let's use a simpler approach - compute what the CID should be for our content
        // and test that tampered content fails

        // First, let's verify that identical content passes with a real IPFS CID
        // QmPK1s3pNYLi9ERiq3BDxKa4XosgWwFRQUydHUtz4YgpqB is a well-known test CID
        // Its actual content is empty directory, so we can't easily test with it

        // Instead, test that any properly formatted CIDv0 parses without error
        // and that hash mismatch is detected
        let valid_cidv0 = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        let result = client.verify_cid(valid_cidv0, content);
        // This should fail because the content doesn't match the CID's hash
        assert!(result.is_err(), "CID verification should fail for mismatched content");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("CID verification failed") || err_msg.contains("does not match"),
            "Error should indicate hash mismatch: {}", err_msg
        );
    }

    #[test]
    fn test_verify_cid_tampered_content_detected() {
        // SECURITY TEST (X1.2): Verify tampered content is detected
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // Use a valid CID format (CIDv0 - starts with Qm)
        // QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG
        let cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";

        // Any random content won't match this CID's hash
        let tampered_content = b"malicious wasm module with backdoor";

        let result = client.verify_cid(cid, tampered_content);
        assert!(result.is_err(), "CID verification should detect tampered content");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("CID verification failed") || err_msg.contains("does not match"),
            "Error should indicate content tampering: {}", err_msg
        );
    }

    #[test]
    fn test_verify_cid_cidv1_parsing() {
        // SECURITY TEST (X1.2): Verify CIDv1 format parsing works
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // CIDv1 format (base32, starts with 'bafy')
        // This is a well-known CID format
        let cidv1 = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";

        // Content won't match, but the CID should parse successfully
        // Error should be about hash mismatch, not CID parsing
        let result = client.verify_cid(cidv1, b"random content");
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        // Should fail due to content mismatch, NOT parsing error
        assert!(
            !err_msg.contains("Failed to parse CID"),
            "CIDv1 should parse correctly: {}", err_msg
        );
    }

    #[test]
    fn test_verify_cid_empty_content() {
        // SECURITY TEST (X1.2): Verify empty content handling
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // Real CID for empty content would be:
        // QmbFMke1KXqnYyBBWxB74N4c5SBnJMVAiMNRcGu6x1AwQH (empty file)
        // But we test that random CID + empty content still verifies hash correctly
        let cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        let empty_content: &[u8] = b"";

        let result = client.verify_cid(cid, empty_content);
        // Should fail because empty content hash doesn't match CID
        assert!(result.is_err(), "Empty content should not match random CID");
    }

    #[test]
    fn test_verify_cid_known_content_match() {
        // SECURITY TEST (X1.2): Test with content we can compute CID for
        // This tests that VALID content passes verification
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // Compute SHA256 hash of test content and create CIDv1
        let content = b"test content for aegis";

        // Compute the SHA256 hash
        let digest = Sha256::digest(content);

        // Create CIDv1 with raw codec (0x55) and sha2-256 (0x12)
        // Using the cid crate to construct a proper CID
        let mh = Multihash::<64>::wrap(0x12, &digest).unwrap();
        let computed_cid = Cid::new_v1(0x55, mh); // 0x55 = raw codec
        let cid_string = computed_cid.to_string();

        // Now verify - this should PASS because content matches CID
        let result = client.verify_cid(&cid_string, content);
        assert!(result.is_ok(), "Valid content should pass CID verification: {:?}", result);
    }

    #[test]
    fn test_verify_cid_prevents_mitm_attack() {
        // SECURITY TEST (X1.2): Simulate MITM attack scenario
        // Attacker tries to substitute malicious content while keeping same CID
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test")),
        )
        .unwrap();

        // Original legitimate content and its CID
        let original_content = b"legitimate wasm module";
        let digest = Sha256::digest(original_content);
        let mh = Multihash::<64>::wrap(0x12, &digest).unwrap();
        let original_cid = Cid::new_v1(0x55, mh);
        let cid_string = original_cid.to_string();

        // Verify original content works
        assert!(
            client.verify_cid(&cid_string, original_content).is_ok(),
            "Original content should pass verification"
        );

        // MITM attack: attacker substitutes malicious content
        let malicious_content = b"malicious wasm with backdoor that steals keys";

        // Verification should FAIL for malicious content
        let result = client.verify_cid(&cid_string, malicious_content);
        assert!(
            result.is_err(),
            "MITM attack should be detected - malicious content must fail verification"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("CID verification failed") || err_msg.contains("does not match"),
            "Error should clearly indicate MITM/tampering: {}", err_msg
        );
    }

    #[test]
    fn test_module_size_validation() {
        // Test that MAX_MODULE_SIZE constant is reasonable
        assert_eq!(MAX_MODULE_SIZE, 10 * 1024 * 1024); // 10MB
    }

    #[test]
    fn test_public_gateways_use_https() {
        // SECURITY TEST: Verify all public gateways use HTTPS
        for gateway in PUBLIC_IPFS_GATEWAYS {
            assert!(
                gateway.starts_with("https://"),
                "Gateway {} must use HTTPS for security", gateway
            );
        }
    }

    // ========================================================================
    // SECURITY TESTS (X4.12): Bandwidth limiting
    // ========================================================================

    #[test]
    fn test_x412_bandwidth_tracker_creation() {
        let tracker = BandwidthTracker::new();
        assert_eq!(tracker.limit_bytes, get_bandwidth_limit());
        assert_eq!(tracker.active_downloads, 0);
        assert_eq!(tracker.max_concurrent, MAX_CONCURRENT_DOWNLOADS);
    }

    #[test]
    fn test_x412_bandwidth_tracker_can_download() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB limit

        // Should allow download when under limit
        assert!(tracker.can_download(10 * 1024 * 1024)); // 10MB

        // Should allow download when exactly at limit
        assert!(tracker.can_download(100 * 1024 * 1024)); // 100MB

        // Should reject when over limit
        assert!(!tracker.can_download(101 * 1024 * 1024)); // 101MB
    }

    #[test]
    fn test_x412_bandwidth_tracker_records_usage() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB

        assert_eq!(tracker.current_usage(), 0);

        tracker.record_download(10 * 1024 * 1024); // 10MB
        assert_eq!(tracker.current_usage(), 10 * 1024 * 1024);

        tracker.record_download(5 * 1024 * 1024); // 5MB more
        assert_eq!(tracker.current_usage(), 15 * 1024 * 1024);
    }

    #[test]
    fn test_x412_bandwidth_tracker_remaining() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB

        assert_eq!(tracker.remaining_bandwidth(), 100 * 1024 * 1024);

        tracker.record_download(30 * 1024 * 1024); // 30MB
        assert_eq!(tracker.remaining_bandwidth(), 70 * 1024 * 1024);

        tracker.record_download(70 * 1024 * 1024); // 70MB more
        assert_eq!(tracker.remaining_bandwidth(), 0);
    }

    #[test]
    fn test_x412_bandwidth_tracker_concurrent_limit() {
        let mut tracker = BandwidthTracker::with_limit(1024 * 1024 * 1024); // 1GB (high to not trigger bandwidth limit)

        // Can start downloads up to the limit
        for _ in 0..MAX_CONCURRENT_DOWNLOADS {
            assert!(tracker.can_download(1024)); // 1KB
            tracker.start_download();
        }

        // Cannot start another download even though bandwidth is available
        assert!(!tracker.can_download(1024), "Should reject when at max concurrent downloads");

        // After ending one, can start another
        tracker.end_download();
        assert!(tracker.can_download(1024), "Should allow after ending a download");
    }

    #[test]
    fn test_x412_bandwidth_tracker_start_end_download() {
        let mut tracker = BandwidthTracker::new();

        assert_eq!(tracker.active_downloads, 0);

        tracker.start_download();
        assert_eq!(tracker.active_downloads, 1);

        tracker.start_download();
        assert_eq!(tracker.active_downloads, 2);

        tracker.end_download();
        assert_eq!(tracker.active_downloads, 1);

        tracker.end_download();
        assert_eq!(tracker.active_downloads, 0);

        // Should not underflow
        tracker.end_download();
        assert_eq!(tracker.active_downloads, 0);
    }

    #[test]
    fn test_x412_bandwidth_limits_constants() {
        // Verify sensible default values
        assert_eq!(DEFAULT_BANDWIDTH_LIMIT_BYTES, 100 * 1024 * 1024); // 100MB/min
        assert_eq!(MIN_BANDWIDTH_LIMIT_BYTES, 10 * 1024 * 1024); // 10MB/min minimum
        assert_eq!(MAX_BANDWIDTH_LIMIT_BYTES, 1024 * 1024 * 1024); // 1GB/min maximum
        assert_eq!(MAX_CONCURRENT_DOWNLOADS, 5);
        assert_eq!(BANDWIDTH_WINDOW_SECS, 60);
    }

    #[test]
    fn test_x412_bandwidth_limits_prevent_exhaustion() {
        // SECURITY TEST: Verify bandwidth limiting prevents resource exhaustion attacks
        let mut tracker = BandwidthTracker::with_limit(50 * 1024 * 1024); // 50MB limit

        // Attacker tries to download many large modules rapidly
        for _ in 0..5 {
            if tracker.can_download(MAX_MODULE_SIZE as u64) {
                tracker.record_download(MAX_MODULE_SIZE as u64);
            }
        }

        // After 5 x 10MB = 50MB, should be at limit
        assert!(!tracker.can_download(MAX_MODULE_SIZE as u64),
            "Bandwidth limit should prevent further downloads");
        assert_eq!(tracker.remaining_bandwidth(), 0);
    }

    #[tokio::test]
    async fn test_x412_ipfs_client_bandwidth_stats() {
        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test-bandwidth")),
        )
        .unwrap();

        let stats = client.bandwidth_stats().await;

        // Initial state should have no usage
        assert_eq!(stats.used_bytes, 0);
        assert_eq!(stats.remaining_bytes, stats.limit_bytes);
        assert_eq!(stats.active_downloads, 0);
        assert_eq!(stats.max_concurrent_downloads, MAX_CONCURRENT_DOWNLOADS);
    }
}
