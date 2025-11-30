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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;

/// Challenge difficulty (number of leading zero bits in PoW)
const POW_DIFFICULTY: u8 = 16; // ~65536 iterations on average

/// Challenge expiration time
const CHALLENGE_TTL: Duration = Duration::from_secs(300); // 5 minutes to solve

/// Token validity period
const TOKEN_TTL: Duration = Duration::from_secs(900); // 15 minutes

/// Maximum token validity (for persistent cookies)
#[allow(dead_code)]
const TOKEN_MAX_TTL: Duration = Duration::from_secs(86400); // 24 hours

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
    /// Token version
    pub ver: u8,
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
}

impl ChallengeManager {
    /// Create new challenge manager with random signing key
    pub fn new() -> Self {
        use rand::RngCore;
        let mut secret_key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key_bytes);
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret_key_bytes);

        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            signing_key,
            bot_patterns: Self::default_bot_patterns(),
        }
    }

    /// Create with specific signing key (for testing or key persistence)
    pub fn with_signing_key(signing_key: ed25519_dalek::SigningKey) -> Self {
        Self {
            challenges: Arc::new(RwLock::new(HashMap::new())),
            signing_key,
            bot_patterns: Self::default_bot_patterns(),
        }
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
    pub async fn issue_challenge(
        &self,
        client_ip: &str,
        challenge_type: ChallengeType,
    ) -> Challenge {
        use rand::Rng;

        // Generate random data before any await points (ThreadRng is not Send)
        let (id, pow_challenge) = {
            let mut rng = rand::thread_rng();
            let id: String = (0..32)
                .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
                .collect();

            let pow_challenge: String = (0..64)
                .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
                .collect();

            (id, pow_challenge)
        }; // rng dropped here, before await

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let challenge = Challenge {
            id: id.clone(),
            challenge_type,
            pow_challenge,
            pow_difficulty: POW_DIFFICULTY,
            issued_at: now,
            expires_at: now + CHALLENGE_TTL.as_secs(),
            client_ip: client_ip.to_string(),
        };

        // Store challenge
        let mut challenges = self.challenges.write().await;
        challenges.insert(id, challenge.clone());

        // Cleanup expired challenges
        challenges.retain(|_, c| c.expires_at > now);

        challenge
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

        // Check expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

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

            let token = ChallengeToken {
                ver: 1,
                fph: fingerprint_hash,
                pow: true,
                score,
                iat: now,
                exp: now + TOKEN_TTL.as_secs(),
                iph: ip_hash,
                ctype: challenge.challenge_type,
            };

            let token_str = self.sign_token(&token);

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
        hex::encode(&hash[..16]) // First 16 bytes
    }

    /// Hash a string (for IP binding)
    fn hash_string(&self, s: &str) -> String {
        let hash = Sha256::digest(s.as_bytes());
        hex::encode(&hash[..8]) // First 8 bytes
    }

    /// Sign a challenge token
    fn sign_token(&self, token: &ChallengeToken) -> String {
        use ed25519_dalek::Signer;

        let payload = serde_json::to_string(token).unwrap();
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());

        let signature = self.signing_key.sign(payload.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        format!("{}.{}", payload_b64, sig_b64)
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

        // Check expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > token.exp {
            return Err(anyhow!("Token expired"));
        }

        // Check IP binding (optional, can be disabled)
        // SECURITY FIX: Use constant-time comparison for hash verification
        let ip_hash = self.hash_string(client_ip);
        let token_iph_bytes = token.iph.as_bytes();
        let ip_hash_bytes = ip_hash.as_bytes();
        // Constant-time comparison for hash values
        let ip_hash_match = token_iph_bytes.len() == ip_hash_bytes.len()
            && token_iph_bytes.ct_eq(ip_hash_bytes).into();
        if !ip_hash_match {
            // Log but don't fail - IPs can change (NAT, mobile networks, etc.)
            log::debug!("Token IP mismatch: expected {}, got {}", token.iph, ip_hash);
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
    function collectFingerprint() {{
        return {{
            canvas_hash: getCanvasFingerprint(),
            webgl_renderer: getWebGLRenderer(),
            webgl_vendor: getWebGLVendor(),
            audio_hash: getAudioFingerprint(),
            screen: {{
                width: screen.width,
                height: screen.height,
                color_depth: screen.colorDepth,
                pixel_ratio: window.devicePixelRatio || 1
            }},
            timezone_offset: new Date().getTimezoneOffset(),
            language: navigator.language,
            platform: navigator.platform,
            cpu_cores: navigator.hardwareConcurrency || null,
            device_memory: navigator.deviceMemory || null,
            touch_support: 'ontouchstart' in window,
            webdriver_detected: navigator.webdriver || false,
            plugins_count: navigator.plugins ? navigator.plugins.length : 0
        }};
    }}

    function getCanvasFingerprint() {{
        try {{
            const canvas = document.createElement('canvas');
            const ctx = canvas.getContext('2d');
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
            return '';
        }}
    }}

    function getWebGLRenderer() {{
        try {{
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
            if (!gl) return null;
            const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
            return debugInfo ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL) : null;
        }} catch (e) {{
            return null;
        }}
    }}

    function getWebGLVendor() {{
        try {{
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
            if (!gl) return null;
            const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
            return debugInfo ? gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL) : null;
        }} catch (e) {{
            return null;
        }}
    }}

    function getAudioFingerprint() {{
        try {{
            const audioContext = new (window.AudioContext || window.webkitAudioContext)();
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
        let challenge = manager.issue_challenge("192.168.1.1", ChallengeType::Invisible).await;

        assert!(!challenge.id.is_empty());
        assert!(!challenge.pow_challenge.is_empty());
        assert_eq!(challenge.pow_difficulty, POW_DIFFICULTY);
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
        let challenge = manager.issue_challenge(client_ip, ChallengeType::Managed).await;

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

        let token = ChallengeToken {
            ver: 1,
            fph: "fingerprint_hash".to_string(),
            pow: true,
            score: 85,
            iat: 1700000000,
            exp: 1700001000,
            iph: manager.hash_string(client_ip),
            ctype: ChallengeType::Invisible,
        };

        let token_str = manager.sign_token(&token);

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
}
