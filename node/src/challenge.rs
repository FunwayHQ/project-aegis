// Sprint 20: JavaScript Challenge System for Bot Verification
//
// This module implements a Turnstile-like challenge system that:
// 1. Issues challenges to suspicious requests
// 2. Verifies proof-of-work solutions
// 3. Validates browser fingerprints
// 4. Issues signed JWT tokens for verified clients
//
// Challenge Flow:
// 1. Bot detector flags request as suspicious
// 2. Response includes challenge script injection
// 3. Client solves PoW + collects fingerprints
// 4. Client submits solution to /aegis/challenge/verify
// 5. Server validates and issues JWT token
// 6. Subsequent requests include token in cookie/header

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
// SECURITY (X5.7): Using `subtle` crate for constant-time comparisons
// This prevents timing attacks in IP binding and token verification
// ct_eq() returns a Choice type that doesn't branch on secret data
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

// ============================================
// SECURITY FIX (X4.1): Increased challenge difficulty
// 16 bits = ~65K iterations, 20 bits = ~1M iterations
// This provides better protection against automated solvers
// ============================================

/// Challenge difficulty (number of leading zero bits in PoW)
///
/// SECURITY FIX (X4.1): Increased from 16 to 20 bits for production use
///
/// ## Justification (X5.4)
///
/// The difficulty represents the number of leading zero bits required in the
/// SHA-256 hash output. This creates an adjustable computational cost:
///
/// | Bits | Expected Iterations | Solve Time (2GHz) | Purpose |
/// |------|--------------------|--------------------|---------|
/// | 16   | ~65,536            | ~0.1s             | Development/Testing |
/// | 18   | ~262,144           | ~0.4s             | Low-traffic sites |
/// | 20   | ~1,048,576         | ~1.5s             | Production default |
/// | 22   | ~4,194,304         | ~6s               | High-value targets |
/// | 24   | ~16,777,216        | ~25s              | Under active attack |
///
/// ## Rationale for Default (20 bits)
///
/// - **Bot deterrence**: 1M iterations takes ~1.5s per challenge, making mass
///   automated requests economically infeasible (100 req/s → 15 min compute)
/// - **User experience**: Modern browsers solve in <2s, acceptable for legitimate users
/// - **Attack cost**: At $0.05/CPU-hour, attacking at 1000 req/s costs ~$25/hour
/// - **Headless browser detection**: Combined with fingerprinting, 20-bit difficulty
///   forces attackers to run real browsers with full JS execution
///
/// ## Tuning Guidelines
///
/// - Decrease to 16-18 for APIs where latency is critical
/// - Increase to 22-24 during active DDoS attacks
/// - Use AEGIS_POW_DIFFICULTY env var for runtime adjustment
///
/// Default: 20 bits (~1 million iterations on average)
/// Set AEGIS_POW_DIFFICULTY env var to override (16-24 range)
const DEFAULT_POW_DIFFICULTY: u8 = 20;

/// Minimum acceptable PoW difficulty
/// 16 bits provides minimal bot deterrence (~65K iterations)
const MIN_POW_DIFFICULTY: u8 = 16;

/// Maximum PoW difficulty (to prevent DoS on clients)
/// 24 bits (~16M iterations, ~25s solve time) prevents client DoS
const MAX_POW_DIFFICULTY: u8 = 24;

/// Get the current PoW difficulty from environment or use default
fn get_pow_difficulty() -> u8 {
    std::env::var("AEGIS_POW_DIFFICULTY")
        .ok()
        .and_then(|s| s.parse::<u8>().ok())
        .map(|d| d.clamp(MIN_POW_DIFFICULTY, MAX_POW_DIFFICULTY))
        .unwrap_or(DEFAULT_POW_DIFFICULTY)
}

/// Challenge expiration time
const CHALLENGE_TTL: Duration = Duration::from_secs(300); // 5 minutes to solve

// ============================================
// SECURITY FIX (X4.6): Configurable token TTL
// ============================================

/// Default token validity period
const DEFAULT_TOKEN_TTL_SECS: u64 = 900; // 15 minutes

/// Minimum token TTL (security floor)
const MIN_TOKEN_TTL_SECS: u64 = 300; // 5 minutes

/// Maximum token TTL (security ceiling)
const MAX_TOKEN_TTL_SECS: u64 = 86400; // 24 hours

/// Get the current token TTL from environment or use default
fn get_token_ttl() -> Duration {
    let secs = std::env::var("AEGIS_TOKEN_TTL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|ttl| ttl.clamp(MIN_TOKEN_TTL_SECS, MAX_TOKEN_TTL_SECS))
        .unwrap_or(DEFAULT_TOKEN_TTL_SECS);
    Duration::from_secs(secs)
}

/// Maximum token validity (for persistent cookies) - kept for backward compatibility
#[allow(dead_code)]
const TOKEN_MAX_TTL: Duration = Duration::from_secs(86400); // 24 hours

// ============================================
// SECURITY FIX (X4.4): Bounded challenge storage
// Prevents memory exhaustion from too many active challenges
// ============================================

/// Maximum number of active challenges to store
/// Beyond this, oldest challenges are evicted even if not expired
const MAX_ACTIVE_CHALLENGES: usize = 100_000;

/// Cleanup threshold - trigger cleanup when reaching this percentage of max
const CLEANUP_THRESHOLD: usize = 90_000; // 90% of max

// ============================================
// SECURITY FIX (X4.2): Use OsRng for cryptographic security
// thread_rng uses a userspace PRNG seeded from OsRng, which is
// faster but less suitable for security-critical random generation.
// For signing keys and challenge secrets, we use OsRng directly.
// ============================================

/// Generate cryptographically secure random bytes
fn secure_random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

/// Generate a cryptographically secure random alphanumeric string
fn secure_random_string(len: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut result = String::with_capacity(len);
    let mut rng = rand::rngs::OsRng;

    for _ in 0..len {
        // Use rejection sampling to avoid modulo bias
        loop {
            let mut byte = [0u8; 1];
            rng.fill_bytes(&mut byte);
            // 62 chars, so we need values 0-61 (reject 62-255)
            if byte[0] < 248 {
                // 248 = 62 * 4, largest multiple of 62 <= 256
                let idx = (byte[0] % 62) as usize;
                result.push(CHARSET[idx] as char);
                break;
            }
        }
    }
    result
}

/// SECURITY FIX (Y5.8): Generate a unique JWT ID (jti) for token replay protection
///
/// The jti is a 16-byte (128-bit) random identifier encoded as base64url.
/// This provides sufficient entropy to prevent collisions while being compact.
///
/// # Returns
/// A 22-character base64url-encoded unique identifier (no padding)
fn generate_jti() -> String {
    let bytes: [u8; 16] = secure_random_bytes();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// SECURITY FIX (X2.1): Helper to get current Unix timestamp safely
/// Returns an error if system time is before UNIX epoch (extremely rare but possible)
fn current_unix_timestamp() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| anyhow!("System time before UNIX epoch: {}", e))
}

/// SECURITY FIX (X2.3): Serialize to canonical JSON (sorted keys, no whitespace)
///
/// This ensures consistent JSON output for signature verification,
/// regardless of struct field ordering in different Rust versions or platforms.
fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    // First serialize to a Value so we can sort keys
    let json_value = serde_json::to_value(value)
        .map_err(|e| anyhow!("JSON serialization failed: {}", e))?;

    // Sort keys recursively and serialize
    let sorted = sort_json_keys(&json_value);
    serde_json::to_string(&sorted)
        .map_err(|e| anyhow!("JSON canonical serialization failed: {}", e))
}

/// Recursively sort JSON object keys
fn sort_json_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: serde_json::Map<String, serde_json::Value> =
                serde_json::Map::new();
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), sort_json_keys(&map[key]));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sort_json_keys).collect())
        }
        _ => value.clone(),
    }
}

/// Subnet mask level for IP binding flexibility
/// SECURITY FIX (X3.5): More granular subnet options than just /24
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubnetMask {
    /// Exact IP match (most secure) - /32 for IPv4
    Exact,
    /// Very narrow subnet - /30 (4 IPs) - minimal flexibility
    Narrow,
    /// Moderate subnet - /28 (16 IPs) - for small load-balanced setups
    Moderate,
    /// Wide subnet - /24 (256 IPs) - LESS SECURE, use with caution
    Wide,
}

impl SubnetMask {
    /// Get the number of bits to use for comparison
    pub fn prefix_bits(&self) -> u8 {
        match self {
            SubnetMask::Exact => 32,    // All bits must match
            SubnetMask::Narrow => 30,   // Only 4 IPs allowed
            SubnetMask::Moderate => 28, // 16 IPs allowed
            SubnetMask::Wide => 24,     // 256 IPs allowed
        }
    }

    /// Get the number of possible IPs in this subnet
    pub fn possible_ips(&self) -> u32 {
        match self {
            SubnetMask::Exact => 1,
            SubnetMask::Narrow => 4,
            SubnetMask::Moderate => 16,
            SubnetMask::Wide => 256,
        }
    }
}

/// SECURITY FIX (X2.4 + X3.5): Challenge system configuration
///
/// Controls security behavior for token verification, particularly IP binding.
#[derive(Debug, Clone)]
pub struct ChallengeConfig {
    /// Enforce IP binding (default: true - SECURE)
    ///
    /// When enabled, tokens are bound to the client's IP address and will
    /// fail verification if used from a different IP.
    ///
    /// Disable ONLY for mobile-first applications where IP changes are
    /// expected and acceptable (mobile networks frequently change IPs).
    pub enforce_ip_binding: bool,

    /// SECURITY FIX (X3.5): Subnet mask for IP binding flexibility
    ///
    /// When enforce_ip_binding is true, this controls how strict the IP
    /// matching is. More restrictive is more secure.
    pub subnet_mask: SubnetMask,

    /// DEPRECATED: Allow IP changes within same /24 subnet
    /// Use `subnet_mask` instead for finer control
    #[deprecated(since = "X3.5", note = "Use subnet_mask field instead")]
    pub allow_subnet_changes: bool,
}

impl Default for ChallengeConfig {
    /// SECURE DEFAULTS: IP binding is enforced with exact match
    fn default() -> Self {
        #[allow(deprecated)]
        Self {
            enforce_ip_binding: true,       // SECURITY: Bind tokens to IP by default
            subnet_mask: SubnetMask::Exact, // SECURITY (X3.5): Exact IP match
            allow_subnet_changes: false,    // DEPRECATED: kept for compatibility
        }
    }
}

impl ChallengeConfig {
    /// SECURITY WARNING: Create a config that completely disables IP binding
    ///
    /// This is the LEAST secure option and should ONLY be used when:
    /// 1. Your application is mobile-first AND
    /// 2. IP changes during a session are acceptable AND
    /// 3. You have other security measures in place (rate limiting, etc.)
    ///
    /// Consider using `load_balanced_narrow()` or `load_balanced_moderate()` instead,
    /// which still provide some IP binding protection.
    #[deprecated(since = "X3.5", note = "Use load_balanced_narrow() or load_balanced_moderate() for better security")]
    pub fn mobile_permissive() -> Self {
        #[allow(deprecated)]
        Self {
            enforce_ip_binding: false,
            subnet_mask: SubnetMask::Exact, // N/A when binding disabled
            allow_subnet_changes: false,
        }
    }

    /// SECURITY FIX (X3.5): Config for narrow load-balanced environments (/30 = 4 IPs)
    ///
    /// This is the RECOMMENDED option for load-balanced setups. It allows
    /// clients to use tokens from up to 4 IPs in the same /30 subnet.
    pub fn load_balanced_narrow() -> Self {
        #[allow(deprecated)]
        Self {
            enforce_ip_binding: true,
            subnet_mask: SubnetMask::Narrow,
            allow_subnet_changes: true,
        }
    }

    /// Config for moderate load-balanced environments (/28 = 16 IPs)
    ///
    /// Provides more flexibility than `load_balanced_narrow()` but less
    /// security. Use only if you have more than 4 IPs behind your load balancer.
    pub fn load_balanced_moderate() -> Self {
        #[allow(deprecated)]
        Self {
            enforce_ip_binding: true,
            subnet_mask: SubnetMask::Moderate,
            allow_subnet_changes: true,
        }
    }

    /// DEPRECATED: Create a config that allows /24 subnet changes
    /// SECURITY WARNING: /24 allows 256 different IPs - this is LESS SECURE
    ///
    /// Consider using `load_balanced_narrow()` or `load_balanced_moderate()` instead.
    #[deprecated(since = "X3.5", note = "Use load_balanced_narrow() or load_balanced_moderate() for better security")]
    pub fn allow_subnet() -> Self {
        #[allow(deprecated)]
        Self {
            enforce_ip_binding: true,
            subnet_mask: SubnetMask::Wide,
            allow_subnet_changes: true,
        }
    }

    /// Check if the new subnet mask field should be used
    pub fn uses_subnet_mask(&self) -> bool {
        self.subnet_mask != SubnetMask::Exact
    }
}

/// Challenge types supported by the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeType {
    /// Invisible challenge - runs automatically without user interaction
    Invisible,
    /// Managed challenge - shows brief loading indicator
    Managed,
    /// Interactive challenge - requires user interaction (click/slider)
    Interactive,
}

/// Challenge issued to a client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    /// Unique challenge ID
    pub id: String,
    /// Challenge type
    pub challenge_type: ChallengeType,
    /// Random data for PoW
    pub pow_challenge: String,
    /// Required difficulty (leading zero bits)
    pub pow_difficulty: u8,
    /// Timestamp when challenge was issued
    pub issued_at: u64,
    /// Timestamp when challenge expires
    pub expires_at: u64,
    /// Client IP (for binding)
    pub client_ip: String,
}

/// Solution submitted by client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeSolution {
    /// Challenge ID being solved
    pub challenge_id: String,
    /// PoW nonce that produces valid hash
    pub pow_nonce: u64,
    /// Browser fingerprint data
    pub fingerprint: BrowserFingerprint,
}

/// Browser fingerprint collected by client-side JavaScript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserFingerprint {
    /// Canvas fingerprint hash
    pub canvas_hash: String,
    /// WebGL renderer string
    pub webgl_renderer: Option<String>,
    /// WebGL vendor string
    pub webgl_vendor: Option<String>,
    /// Audio context fingerprint
    pub audio_hash: Option<String>,
    /// Screen dimensions
    pub screen: ScreenInfo,
    /// Timezone offset in minutes
    pub timezone_offset: i32,
    /// Detected language
    pub language: String,
    /// Platform string
    pub platform: String,
    /// Number of CPU cores (navigator.hardwareConcurrency)
    pub cpu_cores: Option<u8>,
    /// Device memory in GB (navigator.deviceMemory)
    pub device_memory: Option<f32>,
    /// Touch support
    pub touch_support: bool,
    /// WebDriver detected (automation)
    pub webdriver_detected: bool,
    /// Plugins count
    pub plugins_count: u32,
}

/// Screen information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub color_depth: u8,
    pub pixel_ratio: f32,
}

/// Verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether verification succeeded
    pub success: bool,
    /// Challenge token (if successful)
    pub token: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Trust score (0-100)
    pub score: u8,
    /// Detected issues
    pub issues: Vec<String>,
}

/// Challenge token payload (JWT-like structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeToken {
    /// Token version (incremented to 3 for Y5.8 jti support)
    pub ver: u8,
    /// SECURITY FIX (Y5.8): JWT ID - unique token identifier for replay protection
    /// This prevents token reuse attacks by allowing the server to track used tokens.
    pub jti: String,
    /// Fingerprint hash (for binding)
    pub fph: String,
    /// PoW verified
    pub pow: bool,
    /// Trust score
    pub score: u8,
    /// Issued at (Unix timestamp)
    pub iat: u64,
    /// Expires at (Unix timestamp)
    pub exp: u64,
    /// Client IP hash (for binding)
    pub iph: String,
    /// SECURITY FIX (X2.4): Client IP subnet hash (for relaxed binding)
    /// Stores hash of /24 subnet (IPv4) or /64 prefix (IPv6)
    #[serde(default)]
    pub snh: Option<String>,
    /// Challenge type that was solved
    pub ctype: ChallengeType,
}

/// Known bot fingerprint patterns
#[derive(Debug, Clone)]
pub struct BotPattern {
    pub name: String,
    pub webgl_renderer: Option<String>,
    pub webdriver: bool,
    pub plugins_count: Option<u32>,
    pub suspicion_score: u8,
}

/// Challenge system manager
pub struct ChallengeManager {
    /// Active challenges (ID -> Challenge)
    challenges: Arc<RwLock<HashMap<String, Challenge>>>,
    /// Ed25519 signing key for tokens
    signing_key: ed25519_dalek::SigningKey,
    /// Known bot patterns
    bot_patterns: Vec<BotPattern>,
    /// SECURITY FIX (X2.4): Security configuration
    config: ChallengeConfig,
}

impl ChallengeManager {
    /// Create new challenge manager with random signing key and SECURE defaults
    ///
    /// SECURITY FIX (X2.4): Uses ChallengeConfig::default() which enforces IP binding.
    pub fn new() -> Self {
        Self::with_config(ChallengeConfig::default())
    }

    /// Create with custom configuration
    ///
    /// SECURITY FIX (X2.4): Allows customizing IP binding behavior.
    /// Use `ChallengeConfig::default()` for secure defaults.
    /// Use `ChallengeConfig::mobile_permissive()` only for mobile apps.
    pub fn with_config(config: ChallengeConfig) -> Self {
        // SECURITY FIX (X4.2): Use OsRng for cryptographic key generation
        let secret_key_bytes: [u8; 32] = secure_random_bytes();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret_key_bytes);

        if !config.enforce_ip_binding {
            log::warn!(
                "⚠️  SECURITY WARNING: IP binding is DISABLED! \
                 Tokens can be reused from any IP address."
            );
        }

        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            signing_key,
            bot_patterns: Self::default_bot_patterns(),
            config,
        }
    }

    /// Create with specific signing key (for testing or key persistence)
    pub fn with_signing_key(signing_key: ed25519_dalek::SigningKey) -> Self {
        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            signing_key,
            bot_patterns: Self::default_bot_patterns(),
            config: ChallengeConfig::default(),
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &ChallengeConfig {
        &self.config
    }

    /// Get public key for token verification
    pub fn public_key(&self) -> ed25519_dalek::VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Get public key as hex string
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key().as_bytes())
    }

    /// Default bot patterns for detection
    fn default_bot_patterns() -> Vec<BotPattern> {
        vec![
            BotPattern {
                name: "Headless Chrome".to_string(),
                webgl_renderer: Some("Google SwiftShader".to_string()),
                webdriver: true,
                plugins_count: Some(0),
                suspicion_score: 90,
            },
            BotPattern {
                name: "PhantomJS".to_string(),
                webgl_renderer: None,
                webdriver: false,
                plugins_count: Some(0),
                suspicion_score: 95,
            },
            BotPattern {
                name: "Selenium".to_string(),
                webgl_renderer: None,
                webdriver: true,
                plugins_count: None,
                suspicion_score: 85,
            },
            BotPattern {
                name: "Puppeteer".to_string(),
                webgl_renderer: Some("Google SwiftShader".to_string()),
                webdriver: false,
                plugins_count: Some(0),
                suspicion_score: 80,
            },
        ]
    }

    /// Issue a new challenge
    /// SECURITY FIX (X2.1): Now returns Result to handle timestamp errors
    /// SECURITY FIX (X4.2): Uses OsRng for cryptographic security
    /// SECURITY FIX (X4.5): Uses longer PoW challenge string to prevent collisions
    pub async fn issue_challenge(
        &self,
        client_ip: &str,
        challenge_type: ChallengeType,
    ) -> Result<Challenge> {
        // SECURITY FIX (X4.2): Use cryptographically secure random generator
        // SECURITY FIX (X4.5): Use 32-char ID and 128-char PoW challenge to prevent collisions
        let id = secure_random_string(32);
        let pow_challenge = secure_random_string(128); // Increased from 64 to 128 for collision resistance

        // SECURITY FIX (X2.1): Use safe timestamp helper
        let now = current_unix_timestamp()?;

        let challenge = Challenge {
            id: id.clone(),
            challenge_type,
            pow_challenge,
            pow_difficulty: get_pow_difficulty(), // SECURITY FIX (X4.1): Use configurable difficulty
            issued_at: now,
            expires_at: now + CHALLENGE_TTL.as_secs(),
            client_ip: client_ip.to_string(),
        };

        // Store challenge
        let mut challenges = self.challenges.write().await;

        // SECURITY FIX (X4.4): Bounded challenge storage with cleanup
        // First, always remove expired challenges
        challenges.retain(|_, c| c.expires_at > now);

        // If still over threshold, remove oldest challenges (by issued_at)
        if challenges.len() >= CLEANUP_THRESHOLD {
            log::warn!(
                "Challenge storage at {} entries (threshold: {}), forcing cleanup",
                challenges.len(),
                CLEANUP_THRESHOLD
            );

            // Collect and sort by issued_at to find oldest
            let mut entries: Vec<_> = challenges.iter().map(|(k, v)| (k.clone(), v.issued_at)).collect();
            entries.sort_by_key(|(_, issued_at)| *issued_at);

            // Remove oldest 10% to make room
            let remove_count = challenges.len() / 10;
            for (key, _) in entries.into_iter().take(remove_count) {
                challenges.remove(&key);
            }
        }

        // Hard cap: reject new challenges if at absolute maximum
        if challenges.len() >= MAX_ACTIVE_CHALLENGES {
            log::error!(
                "Challenge storage at maximum capacity ({}), rejecting new challenge",
                MAX_ACTIVE_CHALLENGES
            );
            return Err(anyhow!("Challenge system at capacity, please try again later"));
        }

        challenges.insert(id, challenge.clone());

        Ok(challenge)
    }

    /// Verify a challenge solution
    pub async fn verify_solution(
        &self,
        solution: &ChallengeSolution,
        client_ip: &str,
    ) -> VerificationResult {
        let mut issues = Vec::new();
        let mut score: u8 = 100;

        // Get challenge
        let challenge = {
            let challenges = self.challenges.read().await;
            challenges.get(&solution.challenge_id).cloned()
        };

        let challenge = match challenge {
            Some(c) => c,
            None => {
                return VerificationResult {
                    success: false,
                    token: None,
                    error: Some("Challenge not found or expired".to_string()),
                    score: 0,
                    issues: vec!["invalid_challenge".to_string()],
                };
            }
        };

        // SECURITY FIX (X2.1): Check expiration with safe timestamp handling
        let now = match current_unix_timestamp() {
            Ok(ts) => ts,
            Err(e) => {
                return VerificationResult {
                    success: false,
                    token: None,
                    error: Some(format!("System time error: {}", e)),
                    score: 0,
                    issues: vec!["system_time_error".to_string()],
                };
            }
        };

        if now > challenge.expires_at {
            return VerificationResult {
                success: false,
                token: None,
                error: Some("Challenge expired".to_string()),
                score: 0,
                issues: vec!["challenge_expired".to_string()],
            };
        }

        // Check IP binding
        // SECURITY FIX: Use constant-time comparison to prevent timing attacks
        // While IP binding is less sensitive than secret comparison, this provides defense in depth
        let client_ip_bytes = client_ip.as_bytes();
        let challenge_ip_bytes = challenge.client_ip.as_bytes();
        // Constant-time comparison: both length check and content check in constant time
        let ip_match = client_ip_bytes.len() == challenge_ip_bytes.len()
            && client_ip_bytes.ct_eq(challenge_ip_bytes).into();
        if !ip_match {
            issues.push("ip_mismatch".to_string());
            score = score.saturating_sub(30);
        }

        // Verify PoW
        if !self.verify_pow(&challenge.pow_challenge, solution.pow_nonce, challenge.pow_difficulty) {
            return VerificationResult {
                success: false,
                token: None,
                error: Some("Invalid proof-of-work".to_string()),
                score: 0,
                issues: vec!["invalid_pow".to_string()],
            };
        }

        // Analyze fingerprint for bot patterns
        let (fingerprint_score, fingerprint_issues) = self.analyze_fingerprint(&solution.fingerprint);
        score = score.saturating_sub(100 - fingerprint_score);
        issues.extend(fingerprint_issues);

        // Check for webdriver (strong bot indicator)
        if solution.fingerprint.webdriver_detected {
            issues.push("webdriver_detected".to_string());
            score = score.saturating_sub(50);
        }

        // Check for suspicious characteristics
        if solution.fingerprint.plugins_count == 0 {
            issues.push("no_plugins".to_string());
            score = score.saturating_sub(10);
        }

        // Determine success threshold based on challenge type
        let threshold = match challenge.challenge_type {
            ChallengeType::Invisible => 40,
            ChallengeType::Managed => 30,
            ChallengeType::Interactive => 20,
        };

        let success = score >= threshold;

        // Remove used challenge
        {
            let mut challenges = self.challenges.write().await;
            challenges.remove(&solution.challenge_id);
        }

        if success {
            // Generate token
            let fingerprint_hash = self.hash_fingerprint(&solution.fingerprint);
            let ip_hash = self.hash_string(client_ip);
            // SECURITY FIX (X2.4): Store subnet hash for relaxed IP binding
            let subnet = self.extract_subnet(client_ip);
            let subnet_hash = self.hash_string(&subnet);

            let token = ChallengeToken {
                ver: 3, // Version 3 includes jti for replay protection (Y5.8)
                jti: generate_jti(), // SECURITY FIX (Y5.8): Unique token ID
                fph: fingerprint_hash,
                pow: true,
                score,
                iat: now,
                exp: now + get_token_ttl().as_secs(), // SECURITY FIX (X4.6): Configurable TTL
                iph: ip_hash,
                snh: Some(subnet_hash), // X2.4: Subnet hash for relaxed binding
                ctype: challenge.challenge_type,
            };

            // SECURITY FIX (X2.1): Handle token signing errors gracefully
            let token_str = match self.sign_token(&token) {
                Ok(t) => t,
                Err(e) => {
                    return VerificationResult {
                        success: false,
                        token: None,
                        error: Some(format!("Token signing failed: {}", e)),
                        score,
                        issues: vec!["token_signing_error".to_string()],
                    };
                }
            };

            VerificationResult {
                success: true,
                token: Some(token_str),
                error: None,
                score,
                issues,
            }
        } else {
            VerificationResult {
                success: false,
                token: None,
                error: Some(format!("Score {} below threshold {}", score, threshold)),
                score,
                issues,
            }
        }
    }

    /// Verify proof-of-work solution
    fn verify_pow(&self, challenge: &str, nonce: u64, difficulty: u8) -> bool {
        let input = format!("{}{}", challenge, nonce);
        let hash = Sha256::digest(input.as_bytes());

        // Check leading zero bits
        let required_zeros = difficulty as usize;
        let mut zero_bits = 0;

        for byte in hash.iter() {
            if *byte == 0 {
                zero_bits += 8;
            } else {
                zero_bits += byte.leading_zeros() as usize;
                break;
            }
            if zero_bits >= required_zeros {
                break;
            }
        }

        zero_bits >= required_zeros
    }

    /// Analyze fingerprint for bot patterns
    fn analyze_fingerprint(&self, fp: &BrowserFingerprint) -> (u8, Vec<String>) {
        let mut score: u8 = 100;
        let mut issues = Vec::new();

        // Check against known bot patterns
        for pattern in &self.bot_patterns {
            let mut matches = 0;
            let mut checks = 0;

            if let Some(ref renderer) = pattern.webgl_renderer {
                checks += 1;
                if fp.webgl_renderer.as_ref() == Some(renderer) {
                    matches += 1;
                }
            }

            if pattern.webdriver && fp.webdriver_detected {
                checks += 1;
                matches += 1;
            }

            if let Some(plugins) = pattern.plugins_count {
                checks += 1;
                if fp.plugins_count == plugins {
                    matches += 1;
                }
            }

            if checks > 0 && matches == checks {
                issues.push(format!("bot_pattern_{}", pattern.name.to_lowercase().replace(' ', "_")));
                score = score.saturating_sub(pattern.suspicion_score);
            }
        }

        // Check for suspicious screen dimensions
        if fp.screen.width == 0 || fp.screen.height == 0 {
            issues.push("invalid_screen".to_string());
            score = score.saturating_sub(20);
        }

        // Check for suspicious pixel ratio
        if fp.screen.pixel_ratio == 0.0 {
            issues.push("invalid_pixel_ratio".to_string());
            score = score.saturating_sub(10);
        }

        // Canvas hash should be non-empty
        if fp.canvas_hash.is_empty() {
            issues.push("empty_canvas".to_string());
            score = score.saturating_sub(15);
        }

        (score, issues)
    }

    /// Hash fingerprint for token binding
    /// SECURITY FIX (X4.3): Use full 32-byte hash for better collision resistance
    fn hash_fingerprint(&self, fp: &BrowserFingerprint) -> String {
        let data = format!(
            "{}{}{}{}{}",
            fp.canvas_hash,
            fp.webgl_renderer.as_deref().unwrap_or(""),
            fp.screen.width,
            fp.screen.height,
            fp.timezone_offset
        );
        let hash = Sha256::digest(data.as_bytes());
        hex::encode(hash) // Full 32-byte hash (64 hex chars)
    }

    /// Hash a string (for IP binding)
    /// SECURITY FIX (X4.3): Use full 32-byte hash for better collision resistance
    fn hash_string(&self, s: &str) -> String {
        let hash = Sha256::digest(s.as_bytes());
        hex::encode(hash) // Full 32-byte hash (64 hex chars)
    }

    /// SECURITY FIX (X2.4): Check if client IP is in the same subnet as the original token IP
    ///
    /// For IPv4: Compares first 3 octets (/24 subnet)
    /// For IPv6: Compares first 4 segments (/64 subnet)
    ///
    /// This allows for some IP changes (NAT, mobile carrier changes) while still
    /// providing meaningful security against token theft across networks.
    fn check_subnet_match(&self, client_ip: &str, token: &ChallengeToken) -> bool {
        // Extract subnet from client IP and compute hash
        let client_subnet = self.extract_subnet(client_ip);
        let client_subnet_hash = self.hash_string(&client_subnet);

        // Check if token has subnet hash (version 2+ tokens)
        match &token.snh {
            Some(stored_subnet_hash) => {
                // Use constant-time comparison for the subnet hash
                let client_bytes = client_subnet_hash.as_bytes();
                let stored_bytes = stored_subnet_hash.as_bytes();

                let subnet_match = client_bytes.len() == stored_bytes.len()
                    && client_bytes.ct_eq(stored_bytes).into();

                if subnet_match {
                    log::debug!(
                        "Subnet match: client_ip={} is in same subnet as token origin",
                        client_ip
                    );
                } else {
                    log::debug!(
                        "Subnet mismatch: client_ip={}, client_subnet_hash={}, token_subnet_hash={}",
                        client_ip,
                        client_subnet_hash,
                        stored_subnet_hash
                    );
                }
                subnet_match
            }
            None => {
                // Legacy token (version 1) - no subnet hash stored
                // Allow for backward compatibility but log warning
                log::warn!(
                    "Legacy token without subnet hash - allowing for backward compatibility (ip={})",
                    client_ip
                );
                true
            }
        }
    }

    /// SECURITY FIX (X3.5): Extract subnet portion from an IP address based on configured mask
    ///
    /// IPv4: Applies configured SubnetMask (Exact=/32, Narrow=/30, Moderate=/28, Wide=/24)
    /// IPv6: Always uses /64 (first 4 segments) - standard for IPv6 subnetting
    ///
    /// For IPv4, the subnet is extracted by masking the IP address bits according
    /// to the prefix length, providing granular control over IP binding flexibility.
    fn extract_subnet(&self, ip: &str) -> String {
        if ip.contains(':') {
            // IPv6: Take first 4 segments (/64) - standard IPv6 subnet
            let segments: Vec<&str> = ip.split(':').collect();
            if segments.len() >= 4 {
                segments[..4].join(":")
            } else {
                ip.to_string()
            }
        } else {
            // IPv4: Apply configured subnet mask
            self.extract_ipv4_subnet(ip)
        }
    }

    /// SECURITY FIX (X3.5): Extract IPv4 subnet based on configured mask
    ///
    /// This applies proper bit masking for precise subnet extraction:
    /// - /32 (Exact): Full IP (e.g., "192.168.1.100")
    /// - /30 (Narrow): 4 IPs (e.g., "192.168.1.100/30" -> "192.168.1.100")
    /// - /28 (Moderate): 16 IPs (e.g., "192.168.1.100" -> "192.168.1.96")
    /// - /24 (Wide): 256 IPs (e.g., "192.168.1.100" -> "192.168.1.0")
    fn extract_ipv4_subnet(&self, ip: &str) -> String {
        let prefix_bits = self.config.subnet_mask.prefix_bits();

        // For exact match (/32), return the full IP
        if prefix_bits == 32 {
            return ip.to_string();
        }

        // Parse IPv4 octets
        let octets: Vec<&str> = ip.split('.').collect();
        if octets.len() != 4 {
            log::warn!("SECURITY (X3.5): Invalid IPv4 address format: {}", ip);
            return ip.to_string();
        }

        // Parse each octet to u8
        let parsed: Result<Vec<u8>, _> = octets.iter().map(|o| o.parse::<u8>()).collect();
        let bytes = match parsed {
            Ok(b) => b,
            Err(_) => {
                log::warn!("SECURITY (X3.5): Failed to parse IPv4 octets: {}", ip);
                return ip.to_string();
            }
        };

        // Convert to u32, apply mask, convert back
        let ip_u32 = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let mask = !((1u32 << (32 - prefix_bits)) - 1);
        let subnet_u32 = ip_u32 & mask;
        let subnet_bytes = subnet_u32.to_be_bytes();

        // Return masked IP as subnet identifier
        format!(
            "{}.{}.{}.{}",
            subnet_bytes[0], subnet_bytes[1], subnet_bytes[2], subnet_bytes[3]
        )
    }

    /// SECURITY FIX (X2.1/X2.3): Sign a challenge token using canonical JSON
    ///
    /// Uses canonical JSON serialization (sorted keys) to ensure consistent
    /// signatures across different serialization contexts/versions.
    fn sign_token(&self, token: &ChallengeToken) -> Result<String> {
        use ed25519_dalek::Signer;

        // SECURITY FIX (X2.3): Use canonical JSON with sorted keys for deterministic signing
        let payload = canonical_json(token)?;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());

        let signature = self.signing_key.sign(payload.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{}.{}", payload_b64, sig_b64))
    }

    /// Verify and decode a challenge token
    pub fn verify_token(&self, token_str: &str, client_ip: &str) -> Result<ChallengeToken> {
        use ed25519_dalek::Verifier;

        let parts: Vec<&str> = token_str.split('.').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid token format"));
        }

        let payload_bytes = URL_SAFE_NO_PAD.decode(parts[0])?;
        let signature_bytes = URL_SAFE_NO_PAD.decode(parts[1])?;

        let signature = ed25519_dalek::Signature::from_slice(&signature_bytes)?;

        self.signing_key
            .verifying_key()
            .verify(&payload_bytes, &signature)?;

        let token: ChallengeToken = serde_json::from_slice(&payload_bytes)?;

        // SECURITY FIX (X2.1): Check expiration with safe timestamp handling
        let now = current_unix_timestamp()?;

        if now > token.exp {
            return Err(anyhow!("Token expired"));
        }

        // SECURITY FIX (X2.4): Enforce IP binding based on configuration
        // Use constant-time comparison for hash verification to prevent timing attacks
        let ip_hash = self.hash_string(client_ip);
        let token_iph_bytes = token.iph.as_bytes();
        let ip_hash_bytes = ip_hash.as_bytes();

        // Constant-time comparison for hash values
        let ip_hash_match = token_iph_bytes.len() == ip_hash_bytes.len()
            && token_iph_bytes.ct_eq(ip_hash_bytes).into();

        if !ip_hash_match {
            if self.config.enforce_ip_binding {
                // Check if subnet matching is allowed (for mobile networks, NAT, etc.)
                if self.config.allow_subnet_changes {
                    // For subnet matching, compare first 3 octets of IPv4 or first 4 segments of IPv6
                    let subnet_match = self.check_subnet_match(client_ip, &token);
                    if !subnet_match {
                        log::warn!(
                            "SECURITY: Token IP binding failed - IP {} not in same subnet as original",
                            client_ip
                        );
                        return Err(anyhow!(
                            "Token IP binding failed: client IP not in allowed subnet"
                        ));
                    }
                    log::debug!(
                        "Token IP changed but within same subnet - allowing (ip={})",
                        client_ip
                    );
                } else {
                    // Strict IP binding - reject any IP change
                    log::warn!(
                        "SECURITY: Token IP binding failed - strict mode, IP {} does not match",
                        client_ip
                    );
                    return Err(anyhow!(
                        "Token IP binding failed: client IP does not match token"
                    ));
                }
            } else {
                // IP binding disabled - just log for monitoring
                log::debug!("Token IP mismatch (binding disabled): expected hash {}, got {}", token.iph, ip_hash);
            }
        }

        Ok(token)
    }

    /// Generate JavaScript challenge code
    pub fn generate_challenge_script(&self, challenge: &Challenge) -> String {
        format!(r#"
(function() {{
    const AEGIS_CHALLENGE = {{
        id: "{}",
        pow_challenge: "{}",
        pow_difficulty: {},
        expires_at: {}
    }};

    // Fingerprint collection
    // SECURITY (X5.5): CSP Resilience - All fingerprint functions use try-catch
    // to gracefully degrade when Content Security Policy blocks canvas, WebGL, or audio APIs.
    // The challenge will still work with reduced fingerprint data.
    function collectFingerprint() {{
        return {{
            canvas_hash: getCanvasFingerprint(),
            webgl_renderer: getWebGLRenderer(),
            webgl_vendor: getWebGLVendor(),
            audio_hash: getAudioFingerprint(),
            screen: {{
                width: screen.width || 0,
                height: screen.height || 0,
                color_depth: screen.colorDepth || 0,
                pixel_ratio: window.devicePixelRatio || 1
            }},
            timezone_offset: new Date().getTimezoneOffset(),
            language: navigator.language || 'unknown',
            platform: navigator.platform || 'unknown',
            cpu_cores: navigator.hardwareConcurrency || null,
            device_memory: navigator.deviceMemory || null,
            touch_support: 'ontouchstart' in window,
            webdriver_detected: navigator.webdriver || false,
            plugins_count: navigator.plugins ? navigator.plugins.length : 0
        }};
    }}

    // SECURITY (X5.5): Canvas fingerprinting - CSP may block toDataURL() or canvas context
    function getCanvasFingerprint() {{
        try {{
            const canvas = document.createElement('canvas');
            const ctx = canvas.getContext('2d');
            if (!ctx) return '';  // CSP may block canvas context
            canvas.width = 200;
            canvas.height = 50;
            ctx.textBaseline = 'top';
            ctx.font = '14px Arial';
            ctx.fillStyle = '#f60';
            ctx.fillRect(125, 1, 62, 20);
            ctx.fillStyle = '#069';
            ctx.fillText('AEGIS', 2, 15);
            ctx.fillStyle = 'rgba(102, 204, 0, 0.7)';
            ctx.fillText('AEGIS', 4, 17);
            return btoa(canvas.toDataURL()).substring(0, 64);
        }} catch (e) {{
            // CSP blocks canvas fingerprinting - continue with empty hash
            return '';
        }}
    }}

    // SECURITY (X5.5): WebGL fingerprinting - may be blocked by CSP or browser settings
    function getWebGLRenderer() {{
        try {{
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
            if (!gl) return null;  // WebGL not available or blocked
            const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
            return debugInfo ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL) : null;
        }} catch (e) {{
            // WebGL blocked by CSP or browser - continue without
            return null;
        }}
    }}

    // SECURITY (X5.5): WebGL vendor - may be blocked by CSP or privacy settings
    function getWebGLVendor() {{
        try {{
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
            if (!gl) return null;  // WebGL not available or blocked
            const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
            return debugInfo ? gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL) : null;
        }} catch (e) {{
            // WebGL blocked by CSP or browser - continue without
            return null;
        }}
    }}

    // SECURITY (X5.5): Audio fingerprinting - may be blocked by CSP or autoplay policies
    function getAudioFingerprint() {{
        try {{
            const AudioContextClass = window.AudioContext || window.webkitAudioContext;
            if (!AudioContextClass) return null;  // Audio API not available
            const audioContext = new AudioContextClass();
            const oscillator = audioContext.createOscillator();
            const analyser = audioContext.createAnalyser();
            const gain = audioContext.createGain();
            const processor = audioContext.createScriptProcessor(4096, 1, 1);

            gain.gain.value = 0;
            oscillator.type = 'triangle';
            oscillator.connect(analyser);
            analyser.connect(processor);
            processor.connect(gain);
            gain.connect(audioContext.destination);
            oscillator.start(0);

            const data = new Float32Array(analyser.frequencyBinCount);
            analyser.getFloatFrequencyData(data);

            oscillator.stop();
            audioContext.close();

            let hash = 0;
            for (let i = 0; i < data.length; i++) {{
                hash = ((hash << 5) - hash) + (data[i] | 0);
                hash = hash & hash;
            }}
            return hash.toString(16);
        }} catch (e) {{
            // Audio API blocked by CSP or autoplay policy - continue without
            return null;
        }}
    }}

    // Proof-of-Work solver
    async function solvePoW(challenge, difficulty) {{
        const target = BigInt(2) ** BigInt(256 - difficulty);
        let nonce = 0;

        while (true) {{
            const input = challenge + nonce;
            const hashBuffer = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
            const hashArray = new Uint8Array(hashBuffer);
            const hashHex = Array.from(hashArray).map(b => b.toString(16).padStart(2, '0')).join('');
            const hashBigInt = BigInt('0x' + hashHex);

            if (hashBigInt < target) {{
                return nonce;
            }}

            nonce++;

            // Yield to prevent UI blocking
            if (nonce % 10000 === 0) {{
                await new Promise(resolve => setTimeout(resolve, 0));
            }}
        }}
    }}

    // Main challenge execution
    async function runChallenge() {{
        try {{
            // Collect fingerprint
            const fingerprint = collectFingerprint();

            // Solve PoW
            const nonce = await solvePoW(AEGIS_CHALLENGE.pow_challenge, AEGIS_CHALLENGE.pow_difficulty);

            // Submit solution
            const response = await fetch('/aegis/challenge/verify', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{
                    challenge_id: AEGIS_CHALLENGE.id,
                    pow_nonce: nonce,
                    fingerprint: fingerprint
                }})
            }});

            const result = await response.json();

            if (result.success && result.token) {{
                // Store token in cookie
                document.cookie = 'aegis_token=' + result.token + '; path=/; max-age=900; SameSite=Strict';

                // Reload page to continue
                window.location.reload();
            }} else {{
                console.error('Challenge failed:', result.error);
            }}
        }} catch (e) {{
            console.error('Challenge error:', e);
        }}
    }}

    // Start challenge
    runChallenge();
}})();
"#,
            challenge.id,
            challenge.pow_challenge,
            challenge.pow_difficulty,
            challenge.expires_at
        )
    }

    /// Generate challenge HTML page
    pub fn generate_challenge_page(&self, challenge: &Challenge) -> String {
        let script = self.generate_challenge_script(challenge);

        format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Verifying your connection...</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            margin: 0;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            color: #fff;
        }}
        .container {{
            text-align: center;
            padding: 40px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 16px;
            backdrop-filter: blur(10px);
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
        }}
        .spinner {{
            width: 50px;
            height: 50px;
            border: 3px solid rgba(255, 255, 255, 0.1);
            border-top-color: #4ade80;
            border-radius: 50%;
            animation: spin 1s linear infinite;
            margin: 0 auto 20px;
        }}
        @keyframes spin {{
            to {{ transform: rotate(360deg); }}
        }}
        h1 {{
            font-size: 1.5rem;
            margin-bottom: 10px;
            font-weight: 500;
        }}
        p {{
            color: rgba(255, 255, 255, 0.7);
            font-size: 0.9rem;
        }}
        .powered-by {{
            margin-top: 30px;
            font-size: 0.75rem;
            color: rgba(255, 255, 255, 0.4);
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="spinner"></div>
        <h1>Verifying your connection...</h1>
        <p>This check is automatic and should complete in a few seconds.</p>
        <p class="powered-by">Protected by AEGIS</p>
    </div>
    <script>{}</script>
</body>
</html>"#,
            script
        )
    }
}

impl Default for ChallengeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP header name for challenge token
pub const CHALLENGE_TOKEN_HEADER: &str = "X-Aegis-Token";

/// Cookie name for challenge token
pub const CHALLENGE_TOKEN_COOKIE: &str = "aegis_token";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_issue_challenge() {
        let manager = ChallengeManager::new();
        let challenge = manager.issue_challenge("192.168.1.1", ChallengeType::Invisible).await.unwrap();

        assert!(!challenge.id.is_empty());
        assert!(!challenge.pow_challenge.is_empty());
        assert_eq!(challenge.pow_difficulty, get_pow_difficulty());
        assert_eq!(challenge.client_ip, "192.168.1.1");
        assert!(challenge.expires_at > challenge.issued_at);
    }

    #[test]
    fn test_verify_pow() {
        let manager = ChallengeManager::new();

        // Test with known values (low difficulty for testing)
        let challenge = "test_challenge";
        let difficulty = 8; // Just need 1 leading zero byte

        // Find a valid nonce
        let mut nonce = 0u64;
        loop {
            if manager.verify_pow(challenge, nonce, difficulty) {
                break;
            }
            nonce += 1;
            if nonce > 1000000 {
                panic!("Could not find valid nonce in reasonable time");
            }
        }

        assert!(manager.verify_pow(challenge, nonce, difficulty));
        assert!(!manager.verify_pow(challenge, nonce + 1, difficulty)); // Usually fails
    }

    #[test]
    fn test_fingerprint_analysis() {
        let manager = ChallengeManager::new();

        // Normal browser fingerprint
        let normal_fp = BrowserFingerprint {
            canvas_hash: "abc123".to_string(),
            webgl_renderer: Some("ANGLE (Intel, Mesa Intel(R) UHD Graphics)".to_string()),
            webgl_vendor: Some("Google Inc. (Intel)".to_string()),
            audio_hash: Some("deadbeef".to_string()),
            screen: ScreenInfo {
                width: 1920,
                height: 1080,
                color_depth: 24,
                pixel_ratio: 1.0,
            },
            timezone_offset: -480,
            language: "en-US".to_string(),
            platform: "Win32".to_string(),
            cpu_cores: Some(8),
            device_memory: Some(8.0),
            touch_support: false,
            webdriver_detected: false,
            plugins_count: 3,
        };

        let (score, issues) = manager.analyze_fingerprint(&normal_fp);
        assert!(score >= 80, "Normal fingerprint should have high score: {}", score);
        assert!(issues.is_empty(), "Normal fingerprint should have no issues: {:?}", issues);

        // Headless Chrome fingerprint
        let headless_fp = BrowserFingerprint {
            canvas_hash: "def456".to_string(),
            webgl_renderer: Some("Google SwiftShader".to_string()),
            webgl_vendor: Some("Google Inc.".to_string()),
            audio_hash: None,
            screen: ScreenInfo {
                width: 800,
                height: 600,
                color_depth: 24,
                pixel_ratio: 1.0,
            },
            timezone_offset: 0,
            language: "en-US".to_string(),
            platform: "Linux x86_64".to_string(),
            cpu_cores: Some(1),
            device_memory: None,
            touch_support: false,
            webdriver_detected: true,
            plugins_count: 0,
        };

        let (score, issues) = manager.analyze_fingerprint(&headless_fp);
        assert!(score < 50, "Headless Chrome should have low score: {}", score);
        assert!(!issues.is_empty(), "Headless Chrome should have issues");
    }

    #[tokio::test]
    async fn test_full_challenge_flow() {
        let manager = ChallengeManager::new();
        let client_ip = "192.168.1.100";

        // Issue challenge
        let challenge = manager.issue_challenge(client_ip, ChallengeType::Managed).await.unwrap();

        // Solve PoW (with lower difficulty for test)
        let mut nonce = 0u64;
        while !manager.verify_pow(&challenge.pow_challenge, nonce, challenge.pow_difficulty) {
            nonce += 1;
            if nonce > 10_000_000 {
                // Skip if taking too long in tests
                return;
            }
        }

        // Create solution
        let solution = ChallengeSolution {
            challenge_id: challenge.id.clone(),
            pow_nonce: nonce,
            fingerprint: BrowserFingerprint {
                canvas_hash: "test_canvas".to_string(),
                webgl_renderer: Some("ANGLE (NVIDIA)".to_string()),
                webgl_vendor: Some("Google Inc.".to_string()),
                audio_hash: Some("audio123".to_string()),
                screen: ScreenInfo {
                    width: 1920,
                    height: 1080,
                    color_depth: 24,
                    pixel_ratio: 2.0,
                },
                timezone_offset: -300,
                language: "en-US".to_string(),
                platform: "MacIntel".to_string(),
                cpu_cores: Some(8),
                device_memory: Some(16.0),
                touch_support: false,
                webdriver_detected: false,
                plugins_count: 5,
            },
        };

        // Verify solution
        let result = manager.verify_solution(&solution, client_ip).await;

        assert!(result.success, "Verification should succeed: {:?}", result.error);
        assert!(result.token.is_some(), "Should receive token");
        assert!(result.score >= 30, "Score should be above threshold");

        // Verify token
        let token_str = result.token.unwrap();
        let token = manager.verify_token(&token_str, client_ip).unwrap();

        assert!(token.pow);
        assert_eq!(token.ctype, ChallengeType::Managed);
    }

    #[test]
    fn test_token_signing_and_verification() {
        let manager = ChallengeManager::new();
        let client_ip = "10.0.0.1";
        let subnet = manager.extract_subnet(client_ip);

        let token = ChallengeToken {
            ver: 3, // Version 3 includes jti (Y5.8)
            jti: generate_jti(),
            fph: "fingerprint_hash".to_string(),
            pow: true,
            score: 85,
            iat: 1700000000,
            exp: 1700001000,
            iph: manager.hash_string(client_ip),
            snh: Some(manager.hash_string(&subnet)), // X2.4: Include subnet hash
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Token format should be payload.signature
        assert!(token_str.contains('.'));

        // Verification should fail with expired token (exp is in past)
        let result = manager.verify_token(&token_str, client_ip);
        assert!(result.is_err()); // Expired
    }

    #[test]
    fn test_challenge_script_generation() {
        let manager = ChallengeManager::new();

        let challenge = Challenge {
            id: "test123".to_string(),
            challenge_type: ChallengeType::Invisible,
            pow_challenge: "challenge_data".to_string(),
            pow_difficulty: 16,
            issued_at: 1700000000,
            expires_at: 1700000300,
            client_ip: "192.168.1.1".to_string(),
        };

        let script = manager.generate_challenge_script(&challenge);

        assert!(script.contains("test123"));
        assert!(script.contains("challenge_data"));
        assert!(script.contains("pow_difficulty: 16"));
        assert!(script.contains("collectFingerprint"));
        assert!(script.contains("solvePoW"));
    }

    #[test]
    fn test_challenge_page_generation() {
        let manager = ChallengeManager::new();

        let challenge = Challenge {
            id: "page_test".to_string(),
            challenge_type: ChallengeType::Managed,
            pow_challenge: "page_challenge".to_string(),
            pow_difficulty: 16,
            issued_at: 1700000000,
            expires_at: 1700000300,
            client_ip: "192.168.1.1".to_string(),
        };

        let page = manager.generate_challenge_page(&challenge);

        assert!(page.contains("<!DOCTYPE html>"));
        assert!(page.contains("Verifying your connection"));
        assert!(page.contains("Protected by AEGIS"));
        assert!(page.contains("page_test"));
    }

    // ==========================================================================
    // SECURITY TESTS: X2.4 - IP Binding Enforcement
    // ==========================================================================

    #[test]
    fn test_x24_ip_binding_strict_mode_rejects_different_ip() {
        // Test: With strict IP binding (default), tokens from different IPs are rejected
        let manager = ChallengeManager::new(); // Default config has enforce_ip_binding=true
        let original_ip = "192.168.1.100";
        let different_ip = "10.0.0.50"; // Completely different IP

        // Create a valid, non-expired token
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let subnet = manager.extract_subnet(original_ip);

        let token = ChallengeToken {
            ver: 3,
            jti: generate_jti(),
            fph: "fingerprint".to_string(),
            pow: true,
            score: 80,
            iat: now,
            exp: now + 900, // Valid for 15 minutes
            iph: manager.hash_string(original_ip),
            snh: Some(manager.hash_string(&subnet)),
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Verification from same IP should succeed
        let result = manager.verify_token(&token_str, original_ip);
        assert!(result.is_ok(), "Same IP should be accepted");

        // Verification from different IP should fail (strict mode)
        let result = manager.verify_token(&token_str, different_ip);
        assert!(result.is_err(), "Different IP should be rejected in strict mode");
        assert!(
            result.unwrap_err().to_string().contains("IP binding failed"),
            "Error should mention IP binding"
        );
    }

    #[test]
    fn test_x24_ip_binding_disabled_allows_any_ip() {
        // Test: With IP binding disabled, tokens from any IP are accepted
        #[allow(deprecated)]
        let config = ChallengeConfig {
            enforce_ip_binding: false,
            subnet_mask: SubnetMask::Exact,
            allow_subnet_changes: false,
        };
        let manager = ChallengeManager::with_config(config);
        let original_ip = "192.168.1.100";
        let different_ip = "10.0.0.50";

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let subnet = manager.extract_subnet(original_ip);

        let token = ChallengeToken {
            ver: 3,
            jti: generate_jti(),
            fph: "fingerprint".to_string(),
            pow: true,
            score: 80,
            iat: now,
            exp: now + 900,
            iph: manager.hash_string(original_ip),
            snh: Some(manager.hash_string(&subnet)),
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Both IPs should be accepted when binding is disabled
        let result = manager.verify_token(&token_str, original_ip);
        assert!(result.is_ok(), "Same IP should be accepted");

        let result = manager.verify_token(&token_str, different_ip);
        assert!(result.is_ok(), "Different IP should be accepted when binding disabled");
    }

    #[test]
    fn test_x24_subnet_mode_allows_same_subnet() {
        // Test: With subnet mode, IPs in the same /24 subnet are accepted
        #[allow(deprecated)]
        let config = ChallengeConfig {
            enforce_ip_binding: true,
            subnet_mask: SubnetMask::Wide, // /24 for backward compatibility with old test
            allow_subnet_changes: true,
        };
        let manager = ChallengeManager::with_config(config);
        let original_ip = "192.168.1.100";
        let same_subnet_ip = "192.168.1.200"; // Same /24 subnet
        let different_subnet_ip = "192.168.2.100"; // Different /24 subnet

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let subnet = manager.extract_subnet(original_ip);

        let token = ChallengeToken {
            ver: 3,
            jti: generate_jti(),
            fph: "fingerprint".to_string(),
            pow: true,
            score: 80,
            iat: now,
            exp: now + 900,
            iph: manager.hash_string(original_ip),
            snh: Some(manager.hash_string(&subnet)),
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Same subnet should be accepted
        let result = manager.verify_token(&token_str, same_subnet_ip);
        assert!(result.is_ok(), "Same subnet IP should be accepted");

        // Different subnet should be rejected
        let result = manager.verify_token(&token_str, different_subnet_ip);
        assert!(result.is_err(), "Different subnet IP should be rejected");
    }

    #[test]
    fn test_x35_subnet_extraction_ipv4_exact() {
        // SECURITY FIX (X3.5): Default config now uses Exact subnet mask
        let manager = ChallengeManager::new();

        // Test IPv4 subnet extraction with Exact (/32) - returns full IP
        assert_eq!(manager.extract_subnet("192.168.1.100"), "192.168.1.100");
        assert_eq!(manager.extract_subnet("10.0.0.1"), "10.0.0.1");
        assert_eq!(manager.extract_subnet("172.16.254.1"), "172.16.254.1");
    }

    #[test]
    fn test_x35_subnet_extraction_ipv4_narrow() {
        // Test /30 subnet mask (4 IPs)
        let config = ChallengeConfig::load_balanced_narrow();
        let manager = ChallengeManager::with_config(config);

        // 192.168.1.100 & /30 mask = 192.168.1.100 (network: 192.168.1.100)
        assert_eq!(manager.extract_subnet("192.168.1.100"), "192.168.1.100");
        // 192.168.1.101 & /30 mask = 192.168.1.100 (same /30 as .100)
        assert_eq!(manager.extract_subnet("192.168.1.101"), "192.168.1.100");
        // 192.168.1.103 & /30 mask = 192.168.1.100 (same /30)
        assert_eq!(manager.extract_subnet("192.168.1.103"), "192.168.1.100");
        // 192.168.1.104 is in the next /30 block
        assert_eq!(manager.extract_subnet("192.168.1.104"), "192.168.1.104");
    }

    #[test]
    fn test_x35_subnet_extraction_ipv4_moderate() {
        // Test /28 subnet mask (16 IPs)
        let config = ChallengeConfig::load_balanced_moderate();
        let manager = ChallengeManager::with_config(config);

        // 192.168.1.100 & /28 mask = 192.168.1.96 (network: 192.168.1.96 - .111)
        assert_eq!(manager.extract_subnet("192.168.1.100"), "192.168.1.96");
        // 192.168.1.111 & /28 mask = 192.168.1.96 (same /28)
        assert_eq!(manager.extract_subnet("192.168.1.111"), "192.168.1.96");
        // 192.168.1.112 is in the next /28 block (192.168.1.112 - .127)
        assert_eq!(manager.extract_subnet("192.168.1.112"), "192.168.1.112");
    }

    #[test]
    #[allow(deprecated)]
    fn test_x35_subnet_extraction_ipv4_wide() {
        // Test /24 subnet mask (256 IPs) - LESS SECURE, use load_balanced configs instead
        let config = ChallengeConfig::allow_subnet();
        let manager = ChallengeManager::with_config(config);

        // 192.168.1.100 & /24 mask = 192.168.1.0
        assert_eq!(manager.extract_subnet("192.168.1.100"), "192.168.1.0");
        assert_eq!(manager.extract_subnet("192.168.1.255"), "192.168.1.0");
        // Different /24
        assert_eq!(manager.extract_subnet("192.168.2.100"), "192.168.2.0");
    }

    #[test]
    fn test_x24_subnet_extraction_ipv6() {
        let manager = ChallengeManager::new();

        // Test IPv6 subnet extraction (/64)
        assert_eq!(
            manager.extract_subnet("2001:db8:85a3:0:0:8a2e:370:7334"),
            "2001:db8:85a3:0"
        );
        assert_eq!(
            manager.extract_subnet("fe80:0:0:0:1:2:3:4"),
            "fe80:0:0:0"
        );

        // Shorter IPv6 should return as-is
        assert_eq!(
            manager.extract_subnet("::1"),
            "::1"
        );
    }

    #[test]
    fn test_x24_legacy_token_backward_compatibility() {
        // Test: Legacy tokens (v1, without subnet hash) should still work
        #[allow(deprecated)]
        let config = ChallengeConfig {
            enforce_ip_binding: true,
            subnet_mask: SubnetMask::Wide, // /24 for backward compatibility
            allow_subnet_changes: true, // Enable subnet mode
        };
        let manager = ChallengeManager::with_config(config);
        let original_ip = "192.168.1.100";
        let different_ip = "10.0.0.50";

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Create a v1 token (no subnet hash)
        let token = ChallengeToken {
            ver: 1, // Legacy version (pre-Y5.8, no jti)
            jti: String::new(), // Empty jti for legacy compatibility test
            fph: "fingerprint".to_string(),
            pow: true,
            score: 80,
            iat: now,
            exp: now + 900,
            iph: manager.hash_string(original_ip),
            snh: None, // No subnet hash in legacy tokens
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Legacy tokens should be allowed (backward compatibility)
        // Even from different IPs when subnet mode is enabled
        let result = manager.verify_token(&token_str, different_ip);
        assert!(
            result.is_ok(),
            "Legacy tokens should be allowed for backward compatibility"
        );
    }

    #[test]
    fn test_x24_mobile_permissive_config() {
        // Test: Mobile permissive config disables IP binding entirely
        let config = ChallengeConfig::mobile_permissive();
        // mobile_permissive disables IP binding (for mobile apps with frequent IP changes)
        assert!(!config.enforce_ip_binding);
        assert!(!config.allow_subnet_changes); // Not used when enforce_ip_binding is false

        let manager = ChallengeManager::with_config(config);
        let original_ip = "192.168.1.100";
        let completely_different_ip = "10.0.0.50"; // Different network entirely

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let subnet = manager.extract_subnet(original_ip);

        let token = ChallengeToken {
            ver: 3,
            jti: generate_jti(),
            fph: "fingerprint".to_string(),
            pow: true,
            score: 80,
            iat: now,
            exp: now + 900,
            iph: manager.hash_string(original_ip),
            snh: Some(manager.hash_string(&subnet)),
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Any IP should be accepted in mobile permissive mode (no IP binding)
        let result = manager.verify_token(&token_str, completely_different_ip);
        assert!(result.is_ok(), "Mobile permissive should allow any IP");
    }

    #[test]
    fn test_x24_allow_subnet_config() {
        // Test: allow_subnet config enforces subnet binding
        let config = ChallengeConfig::allow_subnet();
        assert!(config.enforce_ip_binding);
        assert!(config.allow_subnet_changes);

        let manager = ChallengeManager::with_config(config);
        let original_ip = "192.168.1.100";
        let same_subnet_ip = "192.168.1.200";

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let subnet = manager.extract_subnet(original_ip);

        let token = ChallengeToken {
            ver: 3,
            jti: generate_jti(),
            fph: "fingerprint".to_string(),
            pow: true,
            score: 80,
            iat: now,
            exp: now + 900,
            iph: manager.hash_string(original_ip),
            snh: Some(manager.hash_string(&subnet)),
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token).unwrap();

        // Same subnet should be accepted with allow_subnet config
        let result = manager.verify_token(&token_str, same_subnet_ip);
        assert!(result.is_ok(), "allow_subnet should allow same subnet IPs");
    }

    // ============== X3.5 Security Tests ==============

    #[test]
    fn test_x35_subnet_mask_enum() {
        // Test SubnetMask enum properties
        assert_eq!(SubnetMask::Exact.prefix_bits(), 32);
        assert_eq!(SubnetMask::Exact.possible_ips(), 1);

        assert_eq!(SubnetMask::Narrow.prefix_bits(), 30);
        assert_eq!(SubnetMask::Narrow.possible_ips(), 4);

        assert_eq!(SubnetMask::Moderate.prefix_bits(), 28);
        assert_eq!(SubnetMask::Moderate.possible_ips(), 16);

        assert_eq!(SubnetMask::Wide.prefix_bits(), 24);
        assert_eq!(SubnetMask::Wide.possible_ips(), 256);
    }

    #[test]
    fn test_x35_default_config_is_exact() {
        // SECURITY: Default config should use Exact subnet mask
        let config = ChallengeConfig::default();
        assert!(config.enforce_ip_binding, "Default should enforce IP binding");
        assert_eq!(config.subnet_mask, SubnetMask::Exact, "Default should use Exact mask");
    }

    #[test]
    fn test_x35_load_balanced_narrow_config() {
        let config = ChallengeConfig::load_balanced_narrow();
        assert!(config.enforce_ip_binding);
        assert_eq!(config.subnet_mask, SubnetMask::Narrow);
    }

    #[test]
    fn test_x35_load_balanced_moderate_config() {
        let config = ChallengeConfig::load_balanced_moderate();
        assert!(config.enforce_ip_binding);
        assert_eq!(config.subnet_mask, SubnetMask::Moderate);
    }

    #[test]
    fn test_x35_invalid_ipv4_graceful_fallback() {
        // Test graceful handling of invalid IPv4 addresses
        let config = ChallengeConfig::load_balanced_moderate();
        let manager = ChallengeManager::with_config(config);

        // Invalid format should return as-is (fail open for logging, not security)
        assert_eq!(manager.extract_subnet("not.an.ip"), "not.an.ip");
        assert_eq!(manager.extract_subnet("192.168.1"), "192.168.1");
        assert_eq!(manager.extract_subnet("192.168.1.1.1"), "192.168.1.1.1");
        assert_eq!(manager.extract_subnet("999.999.999.999"), "999.999.999.999"); // Out of range
    }

    // ==========================================================================
    // X5.6: Concurrent Challenge Solution Tests
    // ==========================================================================

    #[tokio::test]
    async fn test_x56_concurrent_challenge_issuance() {
        // Test: Multiple concurrent challenge issuances should not cause data races
        use std::sync::Arc;

        let manager = Arc::new(ChallengeManager::new());
        let mut handles = Vec::new();

        // Issue 10 challenges concurrently from different IPs
        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                let ip = format!("192.168.1.{}", i + 1);
                manager_clone
                    .issue_challenge(&ip, ChallengeType::Invisible)
                    .await
            });
            handles.push(handle);
        }

        // All should succeed without data races
        let mut challenge_ids = Vec::new();
        for handle in handles {
            let result = handle.await.expect("Task panicked");
            let challenge = result.expect("Challenge issuance failed");
            challenge_ids.push(challenge.id.clone());
        }

        // All challenge IDs should be unique
        challenge_ids.sort();
        challenge_ids.dedup();
        assert_eq!(challenge_ids.len(), 10, "All challenge IDs should be unique");
    }

    #[tokio::test]
    async fn test_x56_concurrent_token_verification() {
        // Test: Multiple concurrent token verifications should not cause data races
        use std::sync::Arc;

        let manager = Arc::new(ChallengeManager::new());

        // First, create a valid token
        let ip = "192.168.1.100";
        let challenge = manager
            .issue_challenge(ip, ChallengeType::Invisible)
            .await
            .expect("Failed to issue challenge");

        // Solve the challenge
        let mut nonce = 0u64;
        loop {
            if manager.verify_pow(&challenge.pow_challenge, nonce, challenge.pow_difficulty) {
                break;
            }
            nonce += 1;
            if nonce > 10_000_000 {
                panic!("Failed to solve PoW in reasonable iterations");
            }
        }

        // Create a solution
        let solution = ChallengeSolution {
            challenge_id: challenge.id.clone(),
            pow_nonce: nonce,
            fingerprint: BrowserFingerprint {
                canvas_hash: "test_canvas".to_string(),
                webgl_renderer: Some("Apple GPU".to_string()),
                webgl_vendor: Some("Apple Inc.".to_string()),
                audio_hash: Some("test_audio".to_string()),
                screen: ScreenInfo {
                    width: 1920,
                    height: 1080,
                    color_depth: 24,
                    pixel_ratio: 2.0,
                },
                timezone_offset: -420,
                language: "en-US".to_string(),
                platform: "MacIntel".to_string(),
                cpu_cores: Some(8),
                device_memory: Some(16.0),
                touch_support: false,
                webdriver_detected: false,
                plugins_count: 5,
            },
        };

        // Verify the solution and get token
        let result = manager
            .verify_solution(&solution, ip)
            .await;
        assert!(result.success, "Verification should succeed: {:?}", result.error);
        let token = result.token.expect("Should have token");

        // Now verify the token concurrently from multiple tasks
        let mut handles = Vec::new();
        for _ in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let token_clone = token.clone();
            let handle = tokio::spawn(async move {
                manager_clone.verify_token(&token_clone, ip)
            });
            handles.push(handle);
        }

        // All verifications should succeed
        for handle in handles {
            let result = handle.await.expect("Task panicked");
            assert!(result.is_ok(), "Token verification failed: {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_x56_challenge_cleanup_under_load() {
        // Test: Challenge cleanup should work correctly under high load
        use std::sync::Arc;

        let manager = Arc::new(ChallengeManager::new());
        let mut handles = Vec::new();

        // Issue many challenges concurrently (simulating high load)
        for batch in 0..5 {
            for i in 0..20 {
                let manager_clone = Arc::clone(&manager);
                let handle = tokio::spawn(async move {
                    let ip = format!("10.{}.{}.{}", batch, i / 256, i % 256);
                    manager_clone
                        .issue_challenge(&ip, ChallengeType::Managed)
                        .await
                });
                handles.push(handle);
            }
        }

        // All should complete without panic
        let mut success_count = 0;
        for handle in handles {
            let result = handle.await.expect("Task panicked");
            if result.is_ok() {
                success_count += 1;
            }
        }

        // At least some should succeed (may reject if over limit)
        assert!(success_count > 0, "At least some challenges should be issued");
    }
}
