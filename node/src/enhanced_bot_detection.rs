// Sprint 19: Enhanced Bot Detection with TLS Fingerprinting
//
// This module combines User-Agent analysis (Sprint 9) with TLS fingerprinting
// to create a composite trust score for accurate bot detection.
//
// Scoring combines:
// - User-Agent analysis (Wasm module)
// - TLS fingerprint matching (JA3/JA4)
// - User-Agent/TLS mismatch detection
// - Behavioral signals (future: Sprint 21)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use crate::bot_management::{BotAction, BotManager, BotMetrics, BotPolicy, BotVerdict};
// SECURITY FIX (X2.6): Import lock recovery utilities
use crate::lock_utils::lock_or_recover;
use crate::tls_fingerprint::{
    ClientType, TlsAnalysisResult, TlsFingerprintAnalyzer, TlsFingerprint, TlsSuspicionLevel,
};

/// Enhanced bot detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedBotConfig {
    /// Base bot policy
    pub policy: BotPolicy,
    /// Enable TLS fingerprint analysis
    pub tls_fingerprinting_enabled: bool,
    /// Weight for User-Agent score (0.0 - 1.0)
    pub ua_weight: f64,
    /// Weight for TLS fingerprint score (0.0 - 1.0)
    pub tls_weight: f64,
    /// Score threshold for blocking (0-100)
    pub block_threshold: i32,
    /// Score threshold for challenging (0-100)
    pub challenge_threshold: i32,
    /// Score threshold for logging (0-100)
    pub log_threshold: i32,
}

impl Default for EnhancedBotConfig {
    fn default() -> Self {
        Self {
            policy: BotPolicy::default(),
            tls_fingerprinting_enabled: true,
            ua_weight: 0.4,  // 40% weight on User-Agent
            tls_weight: 0.6, // 60% weight on TLS fingerprint
            block_threshold: 30,
            challenge_threshold: 50,
            log_threshold: 70,
        }
    }
}

/// Composite trust score from all signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    /// Final composite score (0-100, higher = more trusted)
    pub score: i32,
    /// User-Agent analysis score component
    pub ua_score: i32,
    /// TLS fingerprint score component
    pub tls_score: i32,
    /// Mismatch penalty applied
    pub mismatch_penalty: i32,
    /// Rate limit penalty applied
    pub rate_limit_penalty: i32,
    /// Reasons affecting the score
    pub reasons: Vec<String>,
    /// Recommended action
    pub recommended_action: BotAction,
}

impl TrustScore {
    /// Create a new trust score with base value
    pub fn new() -> Self {
        Self {
            score: 50, // Start neutral
            ua_score: 50,
            tls_score: 50,
            mismatch_penalty: 0,
            rate_limit_penalty: 0,
            reasons: Vec::new(),
            recommended_action: BotAction::Allow,
        }
    }
}

impl Default for TrustScore {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced bot detection metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnhancedBotMetrics {
    /// Base bot metrics
    pub base: BotMetrics,
    /// Requests with TLS fingerprint analyzed
    pub tls_analyzed: u64,
    /// TLS/User-Agent mismatches detected
    pub mismatches_detected: u64,
    /// Browsers detected by TLS
    pub tls_browsers: u64,
    /// Automation tools detected by TLS
    pub tls_automation: u64,
    /// Scanners detected by TLS
    pub tls_scanners: u64,
    /// Headless browsers detected by TLS
    pub tls_headless: u64,
    /// Unknown TLS fingerprints
    pub tls_unknown: u64,
    /// Average trust score
    pub avg_trust_score: f64,
    /// Trust score sample count (for running average)
    score_samples: u64,
}

impl EnhancedBotMetrics {
    /// Update running average of trust scores
    pub fn record_score(&mut self, score: i32) {
        self.score_samples += 1;
        let delta = score as f64 - self.avg_trust_score;
        self.avg_trust_score += delta / self.score_samples as f64;
    }
}

/// Enhanced Bot Detection System
/// Combines User-Agent analysis with TLS fingerprinting
pub struct EnhancedBotDetector {
    /// Base bot manager (User-Agent Wasm analysis)
    bot_manager: BotManager,
    /// TLS fingerprint analyzer
    tls_analyzer: TlsFingerprintAnalyzer,
    /// Configuration
    config: EnhancedBotConfig,
    /// Enhanced metrics
    metrics: Arc<Mutex<EnhancedBotMetrics>>,
}

impl EnhancedBotDetector {
    /// Create new enhanced bot detector
    pub fn new(bot_manager: BotManager, config: EnhancedBotConfig) -> Self {
        Self {
            bot_manager,
            tls_analyzer: TlsFingerprintAnalyzer::new(),
            config,
            metrics: Arc::new(Mutex::new(EnhancedBotMetrics::default())),
        }
    }

    /// Create with custom TLS analyzer
    pub fn with_tls_analyzer(
        bot_manager: BotManager,
        tls_analyzer: TlsFingerprintAnalyzer,
        config: EnhancedBotConfig,
    ) -> Self {
        Self {
            bot_manager,
            tls_analyzer,
            config,
            metrics: Arc::new(Mutex::new(EnhancedBotMetrics::default())),
        }
    }

    /// Analyze request with composite scoring
    pub fn analyze(
        &self,
        user_agent: &str,
        ip: &str,
        tls_fingerprint: Option<&TlsFingerprint>,
    ) -> Result<(TrustScore, BotVerdict, BotAction)> {
        if !self.config.policy.enabled {
            return Ok((TrustScore::new(), BotVerdict::Human, BotAction::Allow));
        }

        let mut trust_score = TrustScore::new();

        // 1. Check rate limit first
        if self.bot_manager.check_rate_limit(ip) {
            trust_score.rate_limit_penalty = 50;
            trust_score.score = 0;
            trust_score.reasons.push(format!("Rate limit exceeded for IP {}", ip));
            trust_score.recommended_action = BotAction::Block;

            self.update_metrics(|m| {
                m.base.total_analyzed += 1;
                m.base.rate_limit_violations += 1;
                m.base.blocked_count += 1;
            });

            return Ok((trust_score, BotVerdict::Suspicious, BotAction::Block));
        }

        // 2. User-Agent analysis via Wasm module
        let ua_verdict = self.bot_manager.detect_bot(user_agent)?;
        let ua_score = match ua_verdict {
            BotVerdict::Human => 80,
            BotVerdict::Suspicious => 40,
            BotVerdict::KnownBot => 10,
        };
        trust_score.ua_score = ua_score;

        // 3. TLS fingerprint analysis (if available)
        let tls_result = if self.config.tls_fingerprinting_enabled {
            if let Some(fp) = tls_fingerprint {
                let result = self.tls_analyzer.analyze(fp, Some(user_agent));

                // Update TLS metrics
                self.update_metrics(|m| {
                    m.tls_analyzed += 1;
                    match result.client_type {
                        Some(ClientType::Browser) | Some(ClientType::MobileBrowser) => m.tls_browsers += 1,
                        Some(ClientType::AutomationTool) => m.tls_automation += 1,
                        Some(ClientType::Scanner) => m.tls_scanners += 1,
                        Some(ClientType::HeadlessBrowser) => m.tls_headless += 1,
                        Some(ClientType::GoodBot) => m.tls_browsers += 1,
                        Some(ClientType::Unknown) | None => m.tls_unknown += 1,
                    }
                });

                Some(result)
            } else {
                None
            }
        } else {
            None
        };

        // 4. Calculate TLS score
        let tls_score = if let Some(ref result) = tls_result {
            // Base score from suspicion level
            let base_tls_score = match result.suspicion_level {
                TlsSuspicionLevel::Low => 80,
                TlsSuspicionLevel::Medium => 50,
                TlsSuspicionLevel::High => 25,
                TlsSuspicionLevel::Critical => 5,
            };
            // Apply adjustment from analyzer
            (base_tls_score + result.score_adjustment).clamp(0, 100)
        } else {
            50 // Neutral if no TLS data
        };
        trust_score.tls_score = tls_score;

        // Add TLS reasons
        if let Some(ref result) = tls_result {
            for reason in &result.suspicion_reasons {
                trust_score.reasons.push(reason.clone());
            }
        }

        // 5. Detect User-Agent/TLS mismatches
        let mismatch_penalty = self.detect_mismatch(user_agent, ua_verdict, tls_result.as_ref());
        trust_score.mismatch_penalty = mismatch_penalty;

        if mismatch_penalty > 0 {
            trust_score.reasons.push(format!(
                "User-Agent/TLS mismatch detected (penalty: -{})",
                mismatch_penalty
            ));
            self.update_metrics(|m| m.mismatches_detected += 1);
        }

        // 6. Calculate composite score
        let weighted_score = if self.config.tls_fingerprinting_enabled && tls_fingerprint.is_some() {
            (trust_score.ua_score as f64 * self.config.ua_weight
                + trust_score.tls_score as f64 * self.config.tls_weight) as i32
        } else {
            trust_score.ua_score // Only UA score if no TLS
        };

        trust_score.score = (weighted_score - mismatch_penalty - trust_score.rate_limit_penalty)
            .clamp(0, 100);

        // 7. Determine action based on score thresholds
        let action = if trust_score.score < self.config.block_threshold {
            BotAction::Block
        } else if trust_score.score < self.config.challenge_threshold {
            BotAction::Challenge
        } else if trust_score.score < self.config.log_threshold {
            BotAction::Log
        } else {
            BotAction::Allow
        };
        trust_score.recommended_action = action;

        // 8. Map score to verdict
        let verdict = if trust_score.score >= 70 {
            BotVerdict::Human
        } else if trust_score.score >= 40 {
            BotVerdict::Suspicious
        } else {
            BotVerdict::KnownBot
        };

        // 9. Update metrics
        self.update_metrics(|m| {
            m.base.total_analyzed += 1;
            m.record_score(trust_score.score);

            match verdict {
                BotVerdict::Human => m.base.human_count += 1,
                BotVerdict::Suspicious => m.base.suspicious_count += 1,
                BotVerdict::KnownBot => m.base.known_bot_count += 1,
            }

            match action {
                BotAction::Allow => m.base.allowed_count += 1,
                BotAction::Block => m.base.blocked_count += 1,
                BotAction::Challenge => m.base.challenged_count += 1,
                BotAction::Log => m.base.logged_count += 1,
            }
        });

        Ok((trust_score, verdict, action))
    }

    /// Detect mismatches between User-Agent claims and TLS fingerprint
    fn detect_mismatch(
        &self,
        user_agent: &str,
        ua_verdict: BotVerdict,
        tls_result: Option<&TlsAnalysisResult>,
    ) -> i32 {
        let Some(tls) = tls_result else {
            return 0;
        };

        let Some(tls_client_type) = tls.client_type else {
            return 0;
        };

        let ua_lower = user_agent.to_lowercase();
        let mut penalty = 0;

        // Check for browser claims with non-browser fingerprint
        let claims_chrome = ua_lower.contains("chrome") && !ua_lower.contains("headless");
        let claims_firefox = ua_lower.contains("firefox");
        let claims_safari = ua_lower.contains("safari") && !ua_lower.contains("chrome");
        let claims_edge = ua_lower.contains("edg/");
        let claims_browser = claims_chrome || claims_firefox || claims_safari || claims_edge;

        if claims_browser {
            match tls_client_type {
                ClientType::Scanner => {
                    // Critical mismatch: Browser UA with scanner fingerprint
                    penalty += 40;
                }
                ClientType::AutomationTool => {
                    // Significant mismatch: Browser UA with curl/python fingerprint
                    penalty += 30;
                }
                ClientType::HeadlessBrowser => {
                    // Moderate mismatch: Browser UA with headless fingerprint
                    penalty += 20;
                }
                _ => {}
            }
        }

        // Check for tool User-Agent with unexpected fingerprint
        let claims_curl = ua_lower.contains("curl/");
        let claims_python = ua_lower.contains("python-");
        let claims_wget = ua_lower.contains("wget/");

        if claims_curl || claims_python || claims_wget {
            if ua_verdict == BotVerdict::Human {
                // Tool claims to be human? Suspicious
                penalty += 10;
            }
        }

        // Check for Googlebot/Bingbot with non-expected fingerprint
        let claims_googlebot = ua_lower.contains("googlebot");
        let claims_bingbot = ua_lower.contains("bingbot");

        if claims_googlebot || claims_bingbot {
            match tls_client_type {
                ClientType::GoodBot => {
                    // Expected - no penalty
                }
                ClientType::Scanner | ClientType::AutomationTool => {
                    // Fake search engine bot
                    penalty += 35;
                }
                _ => {
                    // Unexpected fingerprint for search bot
                    penalty += 15;
                }
            }
        }

        penalty
    }

    /// Update metrics with a closure
    fn update_metrics<F>(&self, f: F)
    where
        F: FnOnce(&mut EnhancedBotMetrics),
    {
        if let Ok(mut metrics) = self.metrics.lock() {
            f(&mut metrics);
        }
    }

    /// Get current metrics
    pub fn get_metrics(&self) -> EnhancedBotMetrics {
        // SECURITY FIX (X2.6): Use lock recovery to prevent panics
        lock_or_recover(&self.metrics, "enhanced bot metrics").clone()
    }

    /// Reset metrics
    pub fn reset_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            *metrics = EnhancedBotMetrics::default();
        }
    }

    /// Get configuration
    pub fn config(&self) -> &EnhancedBotConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: EnhancedBotConfig) {
        self.config = config;
    }

    /// Get reference to TLS analyzer
    pub fn tls_analyzer(&self) -> &TlsFingerprintAnalyzer {
        &self.tls_analyzer
    }

    /// Get reference to base bot manager
    pub fn bot_manager(&self) -> &BotManager {
        &self.bot_manager
    }
}

/// Quick analysis result for high-throughput scenarios
#[derive(Debug, Clone)]
pub struct QuickAnalysisResult {
    pub action: BotAction,
    pub verdict: BotVerdict,
    pub score: i32,
    pub is_mismatch: bool,
}

impl EnhancedBotDetector {
    /// Quick analysis without detailed scoring (faster)
    pub fn quick_analyze(
        &self,
        user_agent: &str,
        ip: &str,
        tls_fingerprint: Option<&TlsFingerprint>,
    ) -> Result<QuickAnalysisResult> {
        let (trust_score, verdict, action) = self.analyze(user_agent, ip, tls_fingerprint)?;

        Ok(QuickAnalysisResult {
            action,
            verdict,
            score: trust_score.score,
            is_mismatch: trust_score.mismatch_penalty > 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls_fingerprint::{ClientHello, TlsVersion};
    use std::path::Path;

    const WASM_PATH: &str = "bot-detector.wasm";

    fn get_test_detector() -> Option<EnhancedBotDetector> {
        if !Path::new(WASM_PATH).exists() {
            return None;
        }

        let bot_manager = BotManager::new(WASM_PATH, BotPolicy::default()).ok()?;
        Some(EnhancedBotDetector::new(bot_manager, EnhancedBotConfig::default()))
    }

    fn create_browser_fingerprint() -> TlsFingerprint {
        let ch = ClientHello {
            handshake_version: TlsVersion::Tls13,
            cipher_suites: vec![0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f, 0xc02c, 0xc030],
            extensions: vec![0x0000, 0x000a, 0x000b, 0x000d, 0x0010, 0x002b, 0x0033],
            sni: Some("example.com".to_string()),
            alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
            supported_versions: vec![0x0304, 0x0303],
            ..Default::default()
        };
        TlsFingerprint::from_client_hello(&ch)
    }

    fn create_curl_fingerprint() -> TlsFingerprint {
        let ch = ClientHello {
            handshake_version: TlsVersion::Tls12,
            cipher_suites: vec![0x0035, 0x002f, 0x000a],
            extensions: vec![0x000a, 0x000b],
            sni: None,
            alpn_protocols: Vec::new(),
            ..Default::default()
        };
        TlsFingerprint::from_client_hello(&ch)
    }

    #[test]
    fn test_trust_score_defaults() {
        let score = TrustScore::new();
        assert_eq!(score.score, 50);
        assert_eq!(score.ua_score, 50);
        assert_eq!(score.tls_score, 50);
        assert_eq!(score.mismatch_penalty, 0);
    }

    #[test]
    fn test_config_defaults() {
        let config = EnhancedBotConfig::default();
        assert!(config.tls_fingerprinting_enabled);
        assert_eq!(config.ua_weight + config.tls_weight, 1.0);
        assert!(config.block_threshold < config.challenge_threshold);
        assert!(config.challenge_threshold < config.log_threshold);
    }

    #[test]
    fn test_enhanced_metrics() {
        let mut metrics = EnhancedBotMetrics::default();

        metrics.record_score(80);
        assert_eq!(metrics.avg_trust_score, 80.0);

        metrics.record_score(60);
        assert_eq!(metrics.avg_trust_score, 70.0);

        metrics.record_score(70);
        assert!((metrics.avg_trust_score - 70.0).abs() < 0.1);
    }

    #[test]
    fn test_browser_with_browser_fingerprint() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let browser_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let browser_fp = create_browser_fingerprint();

        let (score, _verdict, action) = detector
            .analyze(browser_ua, "192.168.1.1", Some(&browser_fp))
            .unwrap();

        // Browser UA + browser-like fingerprint should score reasonably (>= 40)
        // and should not be blocked. The exact score depends on Wasm module behavior.
        assert!(score.score >= 40, "Score {} should be >= 40", score.score);
        assert_eq!(score.mismatch_penalty, 0, "Should not have mismatch penalty");
        assert!(matches!(action, BotAction::Allow | BotAction::Log | BotAction::Challenge));
    }

    #[test]
    fn test_browser_ua_with_curl_fingerprint() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let browser_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0";
        let curl_fp = create_curl_fingerprint();

        let (score, _verdict, _action) = detector
            .analyze(browser_ua, "192.168.1.2", Some(&curl_fp))
            .unwrap();

        // Browser UA + curl-like fingerprint should be penalized
        // But may not trigger mismatch if curl isn't in the known database
        assert!(score.score < 80, "Mismatched request should score lower");
    }

    #[test]
    fn test_curl_with_matching_fingerprint() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let curl_ua = "curl/8.4.0";
        let curl_fp = create_curl_fingerprint();

        let (score, verdict, _action) = detector
            .analyze(curl_ua, "192.168.1.3", Some(&curl_fp))
            .unwrap();

        // curl UA + curl fingerprint = honest bot, should be detected but not heavily penalized
        assert_eq!(score.mismatch_penalty, 0, "Matching should not have mismatch penalty");
        assert_eq!(verdict, BotVerdict::KnownBot);
    }

    #[test]
    fn test_no_tls_fingerprint() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let browser_ua = "Mozilla/5.0 Chrome/120.0.0.0";

        let (score, _verdict, _action) = detector
            .analyze(browser_ua, "192.168.1.4", None)
            .unwrap();

        // Without TLS, should fall back to UA-only analysis
        assert_eq!(score.tls_score, 50, "TLS score should be neutral without fingerprint");
    }

    #[test]
    fn test_tls_disabled() {
        let Some(bot_manager) = BotManager::new(WASM_PATH, BotPolicy::default()).ok() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let config = EnhancedBotConfig {
            tls_fingerprinting_enabled: false,
            ..Default::default()
        };

        let detector = EnhancedBotDetector::new(bot_manager, config);
        let browser_fp = create_browser_fingerprint();

        let (score, _verdict, _action) = detector
            .analyze("Mozilla/5.0 Chrome/120", "192.168.1.5", Some(&browser_fp))
            .unwrap();

        // With TLS disabled, should only use UA score
        assert_eq!(score.tls_score, 50, "TLS should be neutral when disabled");
    }

    #[test]
    fn test_rate_limiting() {
        let Some(bot_manager) = BotManager::new(WASM_PATH, BotPolicy {
            rate_limiting_enabled: true,
            rate_limit_threshold: 2,
            rate_limit_window_secs: 60,
            ..Default::default()
        }).ok() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let detector = EnhancedBotDetector::new(bot_manager, EnhancedBotConfig::default());
        let ip = "192.168.1.100";

        // First two requests should pass
        for _ in 0..2 {
            let (score, _, action) = detector
                .analyze("Mozilla/5.0 Chrome/120", ip, None)
                .unwrap();
            assert_ne!(action, BotAction::Block, "Should not block before limit");
            assert_eq!(score.rate_limit_penalty, 0);
        }

        // Third request should be rate limited
        let (score, _, action) = detector
            .analyze("Mozilla/5.0 Chrome/120", ip, None)
            .unwrap();
        assert_eq!(action, BotAction::Block);
        assert!(score.rate_limit_penalty > 0);
    }

    #[test]
    fn test_metrics_update() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        // Reset and verify initial state
        detector.reset_metrics();
        let metrics = detector.get_metrics();
        assert_eq!(metrics.base.total_analyzed, 0);

        // Analyze some requests
        let _ = detector.analyze("Mozilla/5.0 Chrome/120", "192.168.1.1", None);
        let _ = detector.analyze("curl/8.0", "192.168.1.2", None);
        let _ = detector.analyze("Googlebot/2.1", "192.168.1.3", None);

        let metrics = detector.get_metrics();
        assert_eq!(metrics.base.total_analyzed, 3);
        assert!(metrics.base.known_bot_count > 0 || metrics.base.suspicious_count > 0);
    }

    #[test]
    fn test_quick_analyze() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        let result = detector
            .quick_analyze("curl/8.0", "192.168.1.1", None)
            .unwrap();

        assert_eq!(result.verdict, BotVerdict::KnownBot);
        assert!(result.score < 50);
    }

    #[test]
    fn test_googlebot_spoofing_detection() {
        let Some(detector) = get_test_detector() else {
            println!("Skipping test - Wasm module not available");
            return;
        };

        // Fake Googlebot with curl-like fingerprint
        let fake_googlebot = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)";
        let curl_fp = create_curl_fingerprint();

        let (score, _verdict, _action) = detector
            .analyze(fake_googlebot, "192.168.1.1", Some(&curl_fp))
            .unwrap();

        // Fake Googlebot should be flagged
        assert!(score.score < 50, "Fake Googlebot should have low score: {}", score.score);
    }
}
