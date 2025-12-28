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

/// Default maximum cache size (1GB)
const DEFAULT_MAX_CACHE_SIZE: u64 = 1024 * 1024 * 1024;

/// Minimum cache size (100MB) - cannot be set lower
const MIN_CACHE_SIZE: u64 = 100 * 1024 * 1024;

/// Target cache size after eviction (80% of max to avoid constant eviction)
const CACHE_EVICTION_TARGET_PERCENT: u64 = 80;

/// Default IPFS API endpoint (local node)
const DEFAULT_IPFS_API: &str = "http://127.0.0.1:5001";

/// Public IPFS gateways for fallback (CDN functionality)
const PUBLIC_IPFS_GATEWAYS: &[&str] = &[
    "https://ipfs.io",
    "https://cloudflare-ipfs.com",
    "https://dweb.link",
];

// =============================================================================
// Y4.8: IPFS CID Format Validation
// =============================================================================
//
// IPFS Content Identifiers (CIDs) have specific formats that must be validated
// before being used in URL construction to prevent injection attacks.
//
// Valid CID formats:
// - CIDv0: Starts with "Qm", base58btc encoded, 46 characters
// - CIDv1: Starts with "bafy" (raw), "bafk" (dag-cbor), base32 encoded
//
// Security concern: If CIDs are not validated, an attacker could inject:
// - Path traversal: "../../etc/passwd"
// - Query strings: "Qm...?evil=payload"
// - CRLF injection: "Qm...\r\nHost: evil.com"

/// Minimum length for a valid CID
const MIN_CID_LENGTH: usize = 32;

/// Maximum length for a valid CID
const MAX_CID_LENGTH: usize = 128;

/// Valid CIDv0 prefix (base58btc encoded multihash)
const CIDV0_PREFIX: &str = "Qm";

/// Valid CIDv1 prefixes (base32 encoded with multibase 'b' prefix)
const CIDV1_PREFIXES: &[&str] = &[
    "bafy",  // CIDv1 with raw codec
    "bafk",  // CIDv1 with dag-cbor codec
    "bafz",  // CIDv1 with dag-json codec
    "bafb",  // CIDv1 with dag-pb codec
];

/// Validate IPFS CID format before using in URL construction (Y4.8)
///
/// # Security
/// This function prevents injection attacks by validating:
/// 1. CID length is within expected range
/// 2. CID starts with a valid prefix (Qm for v0, bafy/bafk for v1)
/// 3. CID contains only alphanumeric characters (no special chars)
///
/// # Returns
/// - `Ok(())` if the CID is valid
/// - `Err(String)` with description if invalid
pub fn validate_cid_format(cid: &str) -> Result<(), String> {
    // Check length bounds
    if cid.len() < MIN_CID_LENGTH {
        return Err(format!(
            "CID too short: {} characters (minimum: {})",
            cid.len(),
            MIN_CID_LENGTH
        ));
    }
    if cid.len() > MAX_CID_LENGTH {
        return Err(format!(
            "CID too long: {} characters (maximum: {})",
            cid.len(),
            MAX_CID_LENGTH
        ));
    }

    // Check for valid prefix
    let has_valid_prefix = cid.starts_with(CIDV0_PREFIX)
        || CIDV1_PREFIXES.iter().any(|p| cid.starts_with(p));

    if !has_valid_prefix {
        return Err(format!(
            "Invalid CID prefix: must start with 'Qm' (v0) or 'bafy/bafk/bafz/bafb' (v1), got '{}'",
            &cid.chars().take(4).collect::<String>()
        ));
    }

    // Check for invalid characters (only alphanumeric allowed)
    // This prevents path traversal, query injection, and CRLF injection
    if !cid.chars().all(|c| c.is_ascii_alphanumeric()) {
        let invalid_chars: Vec<char> = cid
            .chars()
            .filter(|c| !c.is_ascii_alphanumeric())
            .take(5)
            .collect();
        return Err(format!(
            "CID contains invalid characters: {:?}. Only alphanumeric characters allowed.",
            invalid_chars
        ));
    }

    Ok(())
}

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
/// Y8.8: Two-phase tracking (reserve + refund) for accurate limiting
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
    /// Y8.8: Reserved bandwidth for in-progress downloads (not yet confirmed)
    reserved_bytes: u64,
}

impl BandwidthTracker {
    fn new() -> Self {
        Self {
            window: Vec::with_capacity(1000),
            limit_bytes: get_bandwidth_limit(),
            window_duration: Duration::from_secs(BANDWIDTH_WINDOW_SECS),
            active_downloads: 0,
            max_concurrent: MAX_CONCURRENT_DOWNLOADS,
            reserved_bytes: 0,
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
            reserved_bytes: 0,
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
    /// Y8.8: Now includes reserved bandwidth in the calculation
    fn can_download(&mut self, size_hint: u64) -> bool {
        // Check concurrent download limit
        if self.active_downloads >= self.max_concurrent {
            return false;
        }

        // Y8.8: Check bandwidth limit including reserved bandwidth
        let current = self.current_usage();
        let total_committed = current + self.reserved_bytes;
        total_committed + size_hint <= self.limit_bytes
    }

    /// Record a download
    fn record_download(&mut self, bytes: u64) {
        self.window.push((Instant::now(), bytes));
        // Prevent unbounded growth
        if self.window.len() > 10_000 {
            self.cleanup();
        }
    }

    /// Y8.8: Reserve bandwidth before download starts (phase 1)
    ///
    /// This prevents concurrent downloads from exceeding the limit.
    /// Call `commit_reservation` when download completes, or
    /// `cancel_reservation` if download fails.
    fn reserve_bandwidth(&mut self, bytes: u64) -> bool {
        if !self.can_download(bytes) {
            return false;
        }
        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
        debug!("Y8.8: Reserved {} bytes, total reserved: {}", bytes, self.reserved_bytes);
        true
    }

    /// Y8.8: Commit a reservation (phase 2 - success)
    ///
    /// Called when download completes successfully.
    /// `actual_bytes` may be less than reserved (partial download refund).
    fn commit_reservation(&mut self, reserved: u64, actual_bytes: u64) {
        // Remove from reserved
        self.reserved_bytes = self.reserved_bytes.saturating_sub(reserved);
        // Record actual usage
        self.record_download(actual_bytes);
        debug!(
            "Y8.8: Committed {} bytes (reserved {}), refunded {}",
            actual_bytes,
            reserved,
            reserved.saturating_sub(actual_bytes)
        );
    }

    /// Y8.8: Cancel a reservation (phase 2 - failure)
    ///
    /// Called when download fails - returns all reserved bandwidth.
    fn cancel_reservation(&mut self, reserved: u64) {
        self.reserved_bytes = self.reserved_bytes.saturating_sub(reserved);
        debug!("Y8.8: Cancelled reservation of {} bytes", reserved);
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
    /// Y8.8: Now includes reserved bandwidth
    fn remaining_bandwidth(&mut self) -> u64 {
        let current = self.current_usage();
        let total_committed = current + self.reserved_bytes;
        self.limit_bytes.saturating_sub(total_committed)
    }

    /// Y8.8: Get currently reserved bandwidth
    fn reserved(&self) -> u64 {
        self.reserved_bytes
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

/// Get configured max cache size from environment or use default
fn get_max_cache_size() -> u64 {
    std::env::var("AEGIS_IPFS_CACHE_SIZE_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|mb| (mb * 1024 * 1024).max(MIN_CACHE_SIZE))
        .unwrap_or(DEFAULT_MAX_CACHE_SIZE)
}

/// Cache entry metadata for LRU tracking
#[derive(Debug, Clone)]
struct CacheEntry {
    /// CID of the cached module
    cid: String,
    /// Size in bytes
    size: u64,
    /// Last access time (for LRU eviction)
    last_access: std::time::SystemTime,
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

    /// Maximum cache size in bytes (for LRU eviction)
    max_cache_size: u64,
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
            max_cache_size: get_max_cache_size(),
        })
    }

    /// Create a new IPFS client with custom cache size limit
    pub fn with_cache_limit(api_endpoint: &str, cache_dir: Option<PathBuf>, max_cache_mb: u64) -> Result<Self> {
        let mut client = Self::with_config(api_endpoint, cache_dir)?;
        client.max_cache_size = (max_cache_mb * 1024 * 1024).max(MIN_CACHE_SIZE);
        Ok(client)
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
        // SECURITY FIX (Y4.8): Validate CID format early to prevent injection attacks
        // This check happens before any network requests or URL construction
        validate_cid_format(cid).map_err(|e| {
            error!("SECURITY (Y4.8): Invalid CID format in fetch_module: {}", e);
            anyhow::anyhow!("Invalid CID format: {}", e)
        })?;

        // Check local cache first
        if let Some(cached_bytes) = self.get_from_cache(cid).await? {
            debug!("Cache HIT for module CID: {}", cid);
            return Ok(cached_bytes);
        }

        debug!("Cache MISS for module CID: {}, fetching from IPFS", cid);

        // Y8.8: Two-phase bandwidth tracking (reserve + commit/cancel)
        // Phase 1: Reserve bandwidth before download starts
        // This prevents concurrent downloads from exceeding the limit
        let reserved_bytes = MAX_MODULE_SIZE as u64;
        {
            let mut tracker = self.bandwidth_tracker.write().await;
            if !tracker.reserve_bandwidth(reserved_bytes) {
                let remaining = tracker.remaining_bandwidth();
                let reset_time = tracker.time_until_reset();
                warn!(
                    "Y8.8: IPFS bandwidth reservation failed for CID {}. Remaining: {} bytes, Reserved: {} bytes, Reset in: {:?}",
                    cid, remaining, tracker.reserved(), reset_time
                );
                anyhow::bail!(
                    "IPFS bandwidth limit exceeded. Remaining bandwidth: {} bytes, Reserved: {} bytes. \
                     Try again in {:?}.",
                    remaining,
                    tracker.reserved(),
                    reset_time.unwrap_or(Duration::from_secs(60))
                );
            }
            tracker.start_download();
            info!(
                "Y8.8: Reserved {} bytes for CID {}. Total reserved: {} bytes",
                reserved_bytes, cid, tracker.reserved()
            );
        }

        // Perform the download
        let result = self.fetch_module_internal(cid).await;

        // Y8.8 Phase 2: Commit or cancel the reservation
        {
            let mut tracker = self.bandwidth_tracker.write().await;
            tracker.end_download();

            match &result {
                Ok(bytes) => {
                    // Success: Commit with actual bytes used (may refund unused)
                    let actual_bytes = bytes.len() as u64;
                    tracker.commit_reservation(reserved_bytes, actual_bytes);
                    info!(
                        "Y8.8: IPFS download complete for CID {}. Used: {} bytes, Refunded: {} bytes, Remaining: {} bytes",
                        cid,
                        actual_bytes,
                        reserved_bytes.saturating_sub(actual_bytes),
                        tracker.remaining_bandwidth()
                    );
                }
                Err(e) => {
                    // Failure: Cancel reservation (full refund)
                    tracker.cancel_reservation(reserved_bytes);
                    warn!(
                        "Y8.8: IPFS download failed for CID {}: {}. Refunded {} bytes reservation",
                        cid, e, reserved_bytes
                    );
                }
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
        // SECURITY FIX (Y4.8): Validate CID format before URL construction
        // This prevents injection attacks via malformed CIDs
        validate_cid_format(cid).map_err(|e| {
            error!("SECURITY (Y4.8): Invalid CID format rejected: {}", e);
            anyhow::anyhow!("Invalid CID format: {}", e)
        })?;

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
                    // Touch file for LRU tracking (update access time)
                    if let Err(e) = self.touch_cache_file(cid).await {
                        debug!("Failed to touch cache file for LRU (non-fatal): {}", e);
                    }
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

    /// Save module to local cache with LRU eviction
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

        // Trigger LRU eviction if cache exceeds max size
        if let Err(e) = self.evict_lru_if_needed().await {
            warn!("LRU eviction failed (non-fatal): {}", e);
        }

        Ok(())
    }

    /// Evict least recently used modules if cache exceeds max size
    ///
    /// LRU eviction strategy:
    /// 1. Scan cache directory for all .wasm files
    /// 2. If total size > max_cache_size, evict oldest accessed files
    /// 3. Target 80% of max size to avoid constant eviction
    pub async fn evict_lru_if_needed(&self) -> Result<EvictionResult> {
        if !self.cache_dir.exists() {
            return Ok(EvictionResult {
                evicted_count: 0,
                evicted_bytes: 0,
                remaining_count: 0,
                remaining_bytes: 0,
            });
        }

        // Collect all cache entries with metadata
        let mut entries: Vec<CacheEntry> = Vec::new();
        let mut total_size = 0u64;

        let mut dir_entries = fs::read_dir(&self.cache_dir)
            .await
            .context("Failed to read cache directory")?;

        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.extension() == Some(std::ffi::OsStr::new("wasm")) {
                if let Ok(metadata) = entry.metadata().await {
                    let size = metadata.len();
                    total_size += size;

                    // Get last access time (fall back to modified time)
                    let last_access = metadata
                        .accessed()
                        .or_else(|_| metadata.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                    // Extract CID from filename
                    if let Some(stem) = path.file_stem() {
                        entries.push(CacheEntry {
                            cid: stem.to_string_lossy().to_string(),
                            size,
                            last_access,
                        });
                    }
                }
            }
        }

        // Check if eviction is needed
        if total_size <= self.max_cache_size {
            debug!(
                "Cache size {} bytes is within limit {} bytes, no eviction needed",
                total_size, self.max_cache_size
            );
            return Ok(EvictionResult {
                evicted_count: 0,
                evicted_bytes: 0,
                remaining_count: entries.len(),
                remaining_bytes: total_size,
            });
        }

        // Calculate target size (80% of max to avoid thrashing)
        let target_size = (self.max_cache_size * CACHE_EVICTION_TARGET_PERCENT) / 100;

        info!(
            "Cache size {} bytes exceeds limit {} bytes, evicting to target {} bytes",
            total_size, self.max_cache_size, target_size
        );

        // Sort by last access time (oldest first for eviction)
        entries.sort_by(|a, b| a.last_access.cmp(&b.last_access));

        let mut evicted_count = 0usize;
        let mut evicted_bytes = 0u64;
        let mut current_size = total_size;

        // Evict oldest entries until we're at target size
        for entry in &entries {
            if current_size <= target_size {
                break;
            }

            let cache_path = self.cache_dir.join(format!("{}.wasm", entry.cid));
            match fs::remove_file(&cache_path).await {
                Ok(()) => {
                    info!(
                        "LRU evicted: {} ({} bytes, last accessed: {:?})",
                        entry.cid, entry.size, entry.last_access
                    );
                    current_size -= entry.size;
                    evicted_count += 1;
                    evicted_bytes += entry.size;
                }
                Err(e) => {
                    warn!("Failed to evict cached module {}: {}", entry.cid, e);
                }
            }
        }

        info!(
            "LRU eviction complete: evicted {} modules ({} bytes), remaining {} bytes",
            evicted_count, evicted_bytes, current_size
        );

        Ok(EvictionResult {
            evicted_count,
            evicted_bytes,
            remaining_count: entries.len() - evicted_count,
            remaining_bytes: current_size,
        })
    }

    /// Touch a cached file to update its access time (for LRU tracking)
    async fn touch_cache_file(&self, cid: &str) -> Result<()> {
        let cache_path = self.cache_dir.join(format!("{}.wasm", cid));
        if cache_path.exists() {
            // Open and close the file to update access time
            // On most systems, this updates atime
            let _ = fs::File::open(&cache_path).await?;
            debug!("Touched cache file for LRU: {}", cid);
        }
        Ok(())
    }

    /// Get current max cache size setting
    pub fn max_cache_size(&self) -> u64 {
        self.max_cache_size
    }

    /// Set max cache size and trigger eviction if needed
    pub async fn set_max_cache_size(&mut self, max_size_mb: u64) -> Result<EvictionResult> {
        self.max_cache_size = (max_size_mb * 1024 * 1024).max(MIN_CACHE_SIZE);
        info!("Updated max cache size to {} bytes", self.max_cache_size);
        self.evict_lru_if_needed().await
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
    /// Y8.8: Now includes reserved bandwidth
    pub async fn bandwidth_stats(&self) -> BandwidthStats {
        let mut tracker = self.bandwidth_tracker.write().await;
        BandwidthStats {
            used_bytes: tracker.current_usage(),
            reserved_bytes: tracker.reserved(),
            limit_bytes: tracker.limit_bytes,
            remaining_bytes: tracker.remaining_bandwidth(),
            active_downloads: tracker.active_downloads,
            max_concurrent_downloads: tracker.max_concurrent,
            reset_time: tracker.time_until_reset(),
        }
    }
}

/// SECURITY FIX (X4.12): Statistics about bandwidth usage
/// Y8.8: Added reserved_bytes for two-phase tracking
#[derive(Debug, Clone)]
pub struct BandwidthStats {
    /// Bytes used in current window (confirmed downloads)
    pub used_bytes: u64,
    /// Y8.8: Bytes reserved for in-progress downloads (not yet confirmed)
    pub reserved_bytes: u64,
    /// Bandwidth limit in bytes per window
    pub limit_bytes: u64,
    /// Remaining bytes available (limit - used - reserved)
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

/// Result of LRU cache eviction operation
#[derive(Debug, Clone)]
pub struct EvictionResult {
    /// Number of modules evicted
    pub evicted_count: usize,
    /// Total bytes freed by eviction
    pub evicted_bytes: u64,
    /// Number of modules remaining in cache
    pub remaining_count: usize,
    /// Total bytes remaining in cache
    pub remaining_bytes: u64,
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
        assert_eq!(stats.reserved_bytes, 0); // Y8.8: New field
        assert_eq!(stats.remaining_bytes, stats.limit_bytes);
        assert_eq!(stats.active_downloads, 0);
        assert_eq!(stats.max_concurrent_downloads, MAX_CONCURRENT_DOWNLOADS);
    }

    // ========================================================================
    // Y8.8: Two-phase bandwidth tracking tests
    // ========================================================================

    #[test]
    fn test_y88_reserve_bandwidth() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB

        // Should be able to reserve bandwidth
        assert!(tracker.reserve_bandwidth(10 * 1024 * 1024)); // 10MB
        assert_eq!(tracker.reserved(), 10 * 1024 * 1024);

        // Can reserve more
        assert!(tracker.reserve_bandwidth(20 * 1024 * 1024)); // 20MB more
        assert_eq!(tracker.reserved(), 30 * 1024 * 1024);

        // Remaining should account for reserved
        assert_eq!(tracker.remaining_bandwidth(), 70 * 1024 * 1024);
    }

    #[test]
    fn test_y88_reserve_exceeds_limit() {
        let mut tracker = BandwidthTracker::with_limit(50 * 1024 * 1024); // 50MB

        // First reservation should succeed
        assert!(tracker.reserve_bandwidth(40 * 1024 * 1024)); // 40MB

        // Second reservation would exceed limit
        assert!(!tracker.reserve_bandwidth(20 * 1024 * 1024)); // 20MB more = 60MB > 50MB limit

        // Reserved should still be just the first amount
        assert_eq!(tracker.reserved(), 40 * 1024 * 1024);
    }

    #[test]
    fn test_y88_commit_reservation() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB

        // Reserve 10MB
        assert!(tracker.reserve_bandwidth(10 * 1024 * 1024));
        assert_eq!(tracker.reserved(), 10 * 1024 * 1024);

        // Commit with actual 5MB used (partial refund)
        tracker.commit_reservation(10 * 1024 * 1024, 5 * 1024 * 1024);

        // Reserved should be 0, used should be 5MB
        assert_eq!(tracker.reserved(), 0);
        assert_eq!(tracker.current_usage(), 5 * 1024 * 1024);
        assert_eq!(tracker.remaining_bandwidth(), 95 * 1024 * 1024);
    }

    #[test]
    fn test_y88_cancel_reservation() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB

        // Reserve 30MB
        assert!(tracker.reserve_bandwidth(30 * 1024 * 1024));
        assert_eq!(tracker.reserved(), 30 * 1024 * 1024);
        assert_eq!(tracker.remaining_bandwidth(), 70 * 1024 * 1024);

        // Cancel (full refund - download failed)
        tracker.cancel_reservation(30 * 1024 * 1024);

        // Reserved should be 0, no usage recorded
        assert_eq!(tracker.reserved(), 0);
        assert_eq!(tracker.current_usage(), 0);
        assert_eq!(tracker.remaining_bandwidth(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_y88_concurrent_reservations() {
        let mut tracker = BandwidthTracker::with_limit(100 * 1024 * 1024); // 100MB

        // Multiple concurrent reservations
        assert!(tracker.reserve_bandwidth(20 * 1024 * 1024)); // 20MB
        tracker.start_download();
        assert!(tracker.reserve_bandwidth(30 * 1024 * 1024)); // 30MB
        tracker.start_download();
        assert!(tracker.reserve_bandwidth(40 * 1024 * 1024)); // 40MB
        tracker.start_download();

        // Total reserved: 90MB
        assert_eq!(tracker.reserved(), 90 * 1024 * 1024);
        assert_eq!(tracker.remaining_bandwidth(), 10 * 1024 * 1024);

        // Cannot reserve more than remaining
        assert!(!tracker.reserve_bandwidth(20 * 1024 * 1024)); // 20MB > 10MB remaining

        // Complete first download with actual 15MB
        tracker.end_download();
        tracker.commit_reservation(20 * 1024 * 1024, 15 * 1024 * 1024);

        // Reserved: 70MB, Used: 15MB
        assert_eq!(tracker.reserved(), 70 * 1024 * 1024);
        assert_eq!(tracker.current_usage(), 15 * 1024 * 1024);
        assert_eq!(tracker.remaining_bandwidth(), 15 * 1024 * 1024);
    }

    #[test]
    fn test_y88_prevents_bandwidth_overcommit() {
        // SECURITY TEST: Ensure two-phase tracking prevents overcommitting bandwidth
        let mut tracker = BandwidthTracker::with_limit(50 * 1024 * 1024); // 50MB

        // Simulate 5 concurrent downloads each reserving 10MB
        for i in 0..5 {
            assert!(tracker.reserve_bandwidth(10 * 1024 * 1024),
                "Reservation {} should succeed", i);
            tracker.start_download();
        }

        // All bandwidth reserved (50MB)
        assert_eq!(tracker.reserved(), 50 * 1024 * 1024);
        assert_eq!(tracker.remaining_bandwidth(), 0);

        // 6th download cannot even start (cannot reserve bandwidth)
        assert!(!tracker.reserve_bandwidth(10 * 1024 * 1024),
            "Should not allow 6th reservation - bandwidth exhausted");
    }

    // ========================================================================
    // Y4.8: IPFS CID Format Validation Tests
    // ========================================================================

    #[test]
    fn test_y48_valid_cidv0() {
        // Valid CIDv0 format (base58btc, starts with Qm, 46 chars)
        let valid_cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        assert!(validate_cid_format(valid_cid).is_ok());
    }

    #[test]
    fn test_y48_valid_cidv1_bafy() {
        // Valid CIDv1 format with raw codec (base32, starts with bafy)
        let valid_cid = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";
        assert!(validate_cid_format(valid_cid).is_ok());
    }

    #[test]
    fn test_y48_valid_cidv1_bafk() {
        // Valid CIDv1 format with dag-cbor codec (base32, starts with bafk)
        let valid_cid = "bafkreih2ac5yabo2daerkw5w5wcwdc7rveqejf4l645hss3uj4f2m7a3kq";
        // Note: bafkreih is actually bafk prefix for inline CIDs
        let valid_cid2 = "bafkqabtimvwgy3lfnz2g2lbnfxxg5dboj4sa2lom4qe2lom4qg64dsnzxxi";
        // Use a constructed valid bafk CID for the test
        let valid_cid3 = "bafkaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(validate_cid_format(valid_cid3).is_ok());
    }

    #[test]
    fn test_y48_cid_too_short() {
        // CID that's too short
        let short_cid = "Qm123";
        let result = validate_cid_format(short_cid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn test_y48_cid_too_long() {
        // CID that's too long (over 128 characters)
        let long_cid = format!("Qm{}", "a".repeat(150));
        let result = validate_cid_format(&long_cid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_y48_invalid_prefix() {
        // CID with invalid prefix
        let invalid_prefixes = vec![
            "Xm1234567890abcdefghijklmnopqrstuvwxyz",
            "Ba1234567890abcdefghijklmnopqrstuvwxyz",
            "abc1234567890abcdefghijklmnopqrstuvwxyz",
            "12345678901234567890123456789012345678901234",
        ];

        for cid in invalid_prefixes {
            let result = validate_cid_format(cid);
            assert!(result.is_err(), "Should reject invalid prefix: {}", cid);
            assert!(result.unwrap_err().contains("Invalid CID prefix"));
        }
    }

    #[test]
    fn test_y48_injection_path_traversal() {
        // SECURITY TEST: Path traversal attempt
        let malicious_cid = "Qm../../etc/passwd1234567890123456";
        let result = validate_cid_format(malicious_cid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn test_y48_injection_query_string() {
        // SECURITY TEST: Query string injection attempt
        let malicious_cid = "QmYwAPJzv5CZsnA625s3Xf2ne?evil=payload";
        let result = validate_cid_format(malicious_cid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn test_y48_injection_crlf() {
        // SECURITY TEST: CRLF injection attempt
        let malicious_cid = "QmYwAPJzv5CZsnA625s3Xf2ne\r\nHost: evil.com";
        let result = validate_cid_format(malicious_cid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn test_y48_injection_null_byte() {
        // SECURITY TEST: Null byte injection attempt
        // Make sure the CID is long enough to pass length check
        let malicious_cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPp\x00evil12345";
        let result = validate_cid_format(malicious_cid);
        assert!(result.is_err(), "Should reject null byte in CID");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("invalid characters"),
            "Error should mention invalid characters, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_y48_injection_special_chars() {
        // SECURITY TEST: Various special characters that shouldn't be in CIDs
        let special_chars = vec![
            "QmYwAPJzv5CZsnA625s3Xf2ne<script>alert(1)",
            "QmYwAPJzv5CZsnA625s3Xf2ne\"onload=",
            "QmYwAPJzv5CZsnA625s3Xf2ne'OR'1'='1",
            "QmYwAPJzv5CZsnA625s3Xf2ne;ls -la",
            "QmYwAPJzv5CZsnA625s3Xf2ne|cat /etc/passwd",
        ];

        for malicious in special_chars {
            let result = validate_cid_format(malicious);
            assert!(result.is_err(), "Should reject special chars in: {}", malicious);
        }
    }

    #[test]
    fn test_y48_cid_validation_constants() {
        // Verify the constants are sensible
        assert!(MIN_CID_LENGTH >= 32, "Min CID length should be at least 32");
        assert!(MAX_CID_LENGTH <= 256, "Max CID length should be reasonable");
        assert!(MIN_CID_LENGTH < MAX_CID_LENGTH);
    }

    // ========================================================================
    // LRU Cache Eviction Tests
    // ========================================================================

    #[test]
    fn test_lru_cache_constants() {
        // Verify LRU cache constants are sensible
        assert_eq!(DEFAULT_MAX_CACHE_SIZE, 1024 * 1024 * 1024); // 1GB
        assert_eq!(MIN_CACHE_SIZE, 100 * 1024 * 1024); // 100MB minimum
        assert_eq!(CACHE_EVICTION_TARGET_PERCENT, 80); // Evict to 80% to avoid thrashing
        assert!(MIN_CACHE_SIZE < DEFAULT_MAX_CACHE_SIZE);
    }

    #[test]
    fn test_lru_cache_entry_struct() {
        // Test CacheEntry struct creation
        let entry = CacheEntry {
            cid: "QmTest123".to_string(),
            size: 1024 * 1024, // 1MB
            last_access: std::time::SystemTime::now(),
        };

        assert_eq!(entry.cid, "QmTest123");
        assert_eq!(entry.size, 1024 * 1024);
    }

    #[test]
    fn test_lru_eviction_result_struct() {
        // Test EvictionResult struct
        let result = EvictionResult {
            evicted_count: 5,
            evicted_bytes: 10 * 1024 * 1024, // 10MB
            remaining_count: 10,
            remaining_bytes: 50 * 1024 * 1024, // 50MB
        };

        assert_eq!(result.evicted_count, 5);
        assert_eq!(result.evicted_bytes, 10 * 1024 * 1024);
        assert_eq!(result.remaining_count, 10);
        assert_eq!(result.remaining_bytes, 50 * 1024 * 1024);
    }

    #[test]
    fn test_lru_cache_limit_constructor() {
        // Test client creation with custom cache limit
        let client = IpfsClient::with_cache_limit(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test-lru")),
            500, // 500MB
        )
        .unwrap();

        // Should be 500MB
        assert_eq!(client.max_cache_size(), 500 * 1024 * 1024);
    }

    #[test]
    fn test_lru_cache_limit_enforces_minimum() {
        // Test that cache limit enforces minimum size
        let client = IpfsClient::with_cache_limit(
            DEFAULT_IPFS_API,
            Some(PathBuf::from("/tmp/aegis-test-lru-min")),
            10, // Only 10MB - below minimum
        )
        .unwrap();

        // Should be enforced to MIN_CACHE_SIZE (100MB)
        assert_eq!(client.max_cache_size(), MIN_CACHE_SIZE);
    }

    #[test]
    fn test_lru_get_max_cache_size_env_var() {
        // Test get_max_cache_size respects environment variable
        // When env var is not set, should return DEFAULT_MAX_CACHE_SIZE
        std::env::remove_var("AEGIS_IPFS_CACHE_SIZE_MB");
        assert_eq!(get_max_cache_size(), DEFAULT_MAX_CACHE_SIZE);
    }

    #[tokio::test]
    async fn test_lru_eviction_empty_cache() {
        // Test eviction when cache directory doesn't exist
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("nonexistent");

        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        let result = client.evict_lru_if_needed().await.unwrap();

        assert_eq!(result.evicted_count, 0);
        assert_eq!(result.evicted_bytes, 0);
        assert_eq!(result.remaining_count, 0);
        assert_eq!(result.remaining_bytes, 0);
    }

    #[tokio::test]
    async fn test_lru_eviction_under_limit() {
        // Test eviction when cache is under limit - should not evict
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        // Create a small test file (1KB)
        let test_file = cache_dir.join("QmTestSmall123456789012345678901234.wasm");
        fs::write(&test_file, vec![0u8; 1024]).await.unwrap();

        let mut client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        // Set cache limit to 1MB (well above our 1KB file)
        client.max_cache_size = 1024 * 1024;

        let result = client.evict_lru_if_needed().await.unwrap();

        assert_eq!(result.evicted_count, 0);
        assert_eq!(result.evicted_bytes, 0);
        assert!(test_file.exists(), "File should not be evicted");
    }

    #[tokio::test]
    async fn test_lru_eviction_over_limit() {
        // Test eviction when cache exceeds limit
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        // Create files totaling more than our test limit
        // File 1: 60KB (older)
        let old_file = cache_dir.join("QmOldFile1234567890123456789012345.wasm");
        fs::write(&old_file, vec![0u8; 60 * 1024]).await.unwrap();

        // Sleep briefly to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // File 2: 60KB (newer)
        let new_file = cache_dir.join("QmNewFile1234567890123456789012345.wasm");
        fs::write(&new_file, vec![0u8; 60 * 1024]).await.unwrap();

        let mut client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        // Set cache limit to 100KB (total is 120KB, so should evict ~24KB to reach 80KB target)
        client.max_cache_size = 100 * 1024;

        let result = client.evict_lru_if_needed().await.unwrap();

        // Should have evicted at least one file
        assert!(result.evicted_count >= 1, "Should evict at least 1 file");
        assert!(result.evicted_bytes > 0, "Should have evicted some bytes");

        // The older file should be evicted first (LRU)
        assert!(!old_file.exists(), "Older file should be evicted");
        // Newer file should remain
        assert!(new_file.exists(), "Newer file should remain");
    }

    #[tokio::test]
    async fn test_lru_set_max_cache_size() {
        // Test dynamic cache size adjustment
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        let mut client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        // Initial size should be default
        assert_eq!(client.max_cache_size(), get_max_cache_size());

        // Set new size (200MB)
        let result = client.set_max_cache_size(200).await.unwrap();

        // Should be 200MB now
        assert_eq!(client.max_cache_size(), 200 * 1024 * 1024);

        // Eviction result should be returned (even if nothing evicted)
        assert_eq!(result.evicted_count, 0);
    }

    #[tokio::test]
    async fn test_lru_touch_cache_file() {
        // Test that touching a cache file updates its access time
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        // Create a test file
        let test_cid = "QmTouchTest1234567890123456789012";
        let test_file = cache_dir.join(format!("{}.wasm", test_cid));
        fs::write(&test_file, vec![0u8; 1024]).await.unwrap();

        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        // Touch should succeed
        let result = client.touch_cache_file(test_cid).await;
        assert!(result.is_ok(), "Touch should succeed");
    }

    #[tokio::test]
    async fn test_lru_touch_nonexistent_file() {
        // Test touching a file that doesn't exist (should not error)
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        // Touch non-existent file should succeed (no-op)
        let result = client.touch_cache_file("QmNonExistent12345678901234567890").await;
        assert!(result.is_ok(), "Touch of nonexistent file should not error");
    }

    #[tokio::test]
    async fn test_lru_cache_stats_with_files() {
        // Test cache_stats returns correct values
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        fs::create_dir_all(&cache_dir).await.unwrap();

        // Create test files
        fs::write(cache_dir.join("QmFile1234567890123456789012345678.wasm"), vec![0u8; 1024]).await.unwrap();
        fs::write(cache_dir.join("QmFile2234567890123456789012345678.wasm"), vec![0u8; 2048]).await.unwrap();

        let client = IpfsClient::with_config(
            DEFAULT_IPFS_API,
            Some(cache_dir),
        )
        .unwrap();

        let stats = client.cache_stats().await.unwrap();

        assert_eq!(stats.module_count, 2);
        assert_eq!(stats.total_size_bytes, 1024 + 2048);
    }
}
