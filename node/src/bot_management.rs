use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Bot detection verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotVerdict {
    /// Confirmed human traffic
    Human,
    /// Known legitimate bot (search engines, etc.)
    KnownBot,
    /// Suspicious activity requiring challenge
    Suspicious,
    /// Malicious bot to be blocked
    Malicious,
}

/// Action to take based on bot verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotAction {
    /// Allow the request
    Allow,
    /// Issue a JavaScript challenge
    Challenge,
    /// Block the request (return 403)
    Block,
    /// Apply rate limiting
    RateLimit,
}

/// Bot detection rule based on User-Agent pattern
#[derive(Debug, Clone)]
pub struct BotRule {
    pub id: u32,
    pub description: String,
    pub pattern: Regex,
    pub verdict: BotVerdict,
    pub category: String,
}

/// Bot detection result
#[derive(Debug, Clone)]
pub struct BotDetection {
    pub verdict: BotVerdict,
    pub action: BotAction,
    pub rule_id: Option<u32>,
    pub rule_description: Option<String>,
    pub category: Option<String>,
    pub matched_value: Option<String>,
    pub reason: String,
}

/// Rate limiting entry per IP address
#[derive(Debug, Clone)]
struct RateLimitEntry {
    request_count: u64,
    window_start: Instant,
    last_request: Instant,
}

impl RateLimitEntry {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            request_count: 0,  // Start at 0, will be incremented immediately
            window_start: now,
            last_request: now,
        }
    }

    fn increment(&mut self, window_duration: Duration) {
        let now = Instant::now();

        // Reset window if expired
        if now.duration_since(self.window_start) > window_duration {
            self.request_count = 1;
            self.window_start = now;
        } else {
            self.request_count += 1;
        }

        self.last_request = now;
    }

    fn is_over_limit(&self, limit: u64, window_duration: Duration) -> bool {
        let now = Instant::now();

        // If window expired, rate is fine
        if now.duration_since(self.window_start) > window_duration {
            return false;
        }

        self.request_count > limit
    }

    fn requests_per_second(&self) -> f64 {
        let elapsed = self.last_request.duration_since(self.window_start).as_secs_f64();
        // Use a minimum elapsed time of 0.1 seconds to avoid false positives
        // from rapid-fire test requests
        let elapsed = elapsed.max(0.1);
        self.request_count as f64 / elapsed
    }
}

/// Bot management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// Enable/disable bot management
    pub enabled: bool,

    /// Action for known legitimate bots
    pub known_bot_action: BotAction,

    /// Action for suspicious bots
    pub suspicious_action: BotAction,

    /// Action for malicious bots
    pub malicious_action: BotAction,

    /// Rate limit: requests per window
    pub rate_limit_requests: u64,

    /// Rate limit window duration in seconds
    pub rate_limit_window_secs: u64,

    /// Threshold for suspicious rate (requests/sec)
    pub suspicious_rate_threshold: f64,

    /// Custom user-agent whitelist (bypass all checks)
    pub whitelist_user_agents: Vec<String>,

    /// Custom user-agent blacklist (immediate block)
    pub blacklist_user_agents: Vec<String>,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            known_bot_action: BotAction::Allow,
            suspicious_action: BotAction::Challenge,
            malicious_action: BotAction::Block,
            rate_limit_requests: 100,
            rate_limit_window_secs: 60,
            suspicious_rate_threshold: 10.0, // 10 req/sec
            whitelist_user_agents: Vec::new(),
            blacklist_user_agents: Vec::new(),
        }
    }
}

/// AEGIS Bot Management System
///
/// Implements bot detection using:
/// - User-Agent analysis (known bots, suspicious patterns)
/// - Per-IP rate limiting
/// - Configurable policies
///
/// Future: Sprint 13 will migrate this to Wasm for isolation
pub struct BotManager {
    config: BotConfig,
    rules: Vec<BotRule>,
    rate_limits: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
}

impl BotManager {
    /// Create new bot manager with default rules
    pub fn new(config: BotConfig) -> Self {
        let rules = Self::build_default_rules();
        Self {
            config,
            rules,
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Analyze request and determine bot verdict
    pub fn analyze_request(
        &self,
        user_agent: &str,
        client_ip: &str,
    ) -> BotDetection {
        if !self.config.enabled {
            return BotDetection {
                verdict: BotVerdict::Human,
                action: BotAction::Allow,
                rule_id: None,
                rule_description: None,
                category: None,
                matched_value: None,
                reason: "Bot management disabled".to_string(),
            };
        }

        // Check whitelist first
        if self.is_whitelisted(user_agent) {
            return BotDetection {
                verdict: BotVerdict::Human,
                action: BotAction::Allow,
                rule_id: None,
                rule_description: Some("Whitelisted User-Agent".to_string()),
                category: Some("whitelist".to_string()),
                matched_value: Some(user_agent.to_string()),
                reason: "User-Agent is whitelisted".to_string(),
            };
        }

        // Check blacklist
        if self.is_blacklisted(user_agent) {
            return BotDetection {
                verdict: BotVerdict::Malicious,
                action: self.config.malicious_action,
                rule_id: None,
                rule_description: Some("Blacklisted User-Agent".to_string()),
                category: Some("blacklist".to_string()),
                matched_value: Some(user_agent.to_string()),
                reason: "User-Agent is blacklisted".to_string(),
            };
        }

        // Check rate limiting
        if let Some(rate_detection) = self.check_rate_limit(client_ip) {
            return rate_detection;
        }

        // Check against bot detection rules
        for rule in &self.rules {
            if rule.pattern.is_match(user_agent) {
                let action = match rule.verdict {
                    BotVerdict::Human => BotAction::Allow,
                    BotVerdict::KnownBot => self.config.known_bot_action,
                    BotVerdict::Suspicious => self.config.suspicious_action,
                    BotVerdict::Malicious => self.config.malicious_action,
                };

                return BotDetection {
                    verdict: rule.verdict,
                    action,
                    rule_id: Some(rule.id),
                    rule_description: Some(rule.description.clone()),
                    category: Some(rule.category.clone()),
                    matched_value: Some(user_agent.to_string()),
                    reason: format!("Matched rule: {}", rule.description),
                };
            }
        }

        // Default: assume human
        BotDetection {
            verdict: BotVerdict::Human,
            action: BotAction::Allow,
            rule_id: None,
            rule_description: None,
            category: None,
            matched_value: None,
            reason: "No bot patterns detected".to_string(),
        }
    }

    /// Check if user-agent is whitelisted
    fn is_whitelisted(&self, user_agent: &str) -> bool {
        self.config.whitelist_user_agents
            .iter()
            .any(|pattern| user_agent.contains(pattern))
    }

    /// Check if user-agent is blacklisted
    fn is_blacklisted(&self, user_agent: &str) -> bool {
        self.config.blacklist_user_agents
            .iter()
            .any(|pattern| user_agent.contains(pattern))
    }

    /// Check rate limiting for IP address
    fn check_rate_limit(&self, client_ip: &str) -> Option<BotDetection> {
        let window_duration = Duration::from_secs(self.config.rate_limit_window_secs);

        let mut rate_limits = self.rate_limits.write().ok()?;

        let entry = rate_limits
            .entry(client_ip.to_string())
            .or_insert_with(RateLimitEntry::new);

        entry.increment(window_duration);

        // Check if over limit
        if entry.is_over_limit(self.config.rate_limit_requests, window_duration) {
            return Some(BotDetection {
                verdict: BotVerdict::Malicious,
                action: self.config.malicious_action,
                rule_id: None,
                rule_description: Some("Rate limit exceeded".to_string()),
                category: Some("rate_limit".to_string()),
                matched_value: Some(client_ip.to_string()),
                reason: format!(
                    "Exceeded {} requests per {} seconds",
                    self.config.rate_limit_requests, self.config.rate_limit_window_secs
                ),
            });
        }

        // Check if suspicious rate
        let req_per_sec = entry.requests_per_second();
        if req_per_sec > self.config.suspicious_rate_threshold {
            return Some(BotDetection {
                verdict: BotVerdict::Suspicious,
                action: self.config.suspicious_action,
                rule_id: None,
                rule_description: Some("Suspicious request rate".to_string()),
                category: Some("rate_limit".to_string()),
                matched_value: Some(client_ip.to_string()),
                reason: format!(
                    "Suspicious rate: {:.2} req/sec (threshold: {})",
                    req_per_sec, self.config.suspicious_rate_threshold
                ),
            });
        }

        None
    }

    /// Build default bot detection rules
    fn build_default_rules() -> Vec<BotRule> {
        vec![
            // Known legitimate bots (search engines)
            BotRule {
                id: 1001,
                description: "Googlebot".to_string(),
                pattern: Regex::new(r"(?i)googlebot").unwrap(),
                verdict: BotVerdict::KnownBot,
                category: "search_engine".to_string(),
            },
            BotRule {
                id: 1002,
                description: "Bingbot".to_string(),
                pattern: Regex::new(r"(?i)bingbot").unwrap(),
                verdict: BotVerdict::KnownBot,
                category: "search_engine".to_string(),
            },
            BotRule {
                id: 1003,
                description: "Yahoo Slurp".to_string(),
                pattern: Regex::new(r"(?i)slurp").unwrap(),
                verdict: BotVerdict::KnownBot,
                category: "search_engine".to_string(),
            },
            BotRule {
                id: 1004,
                description: "DuckDuckBot".to_string(),
                pattern: Regex::new(r"(?i)duckduckbot").unwrap(),
                verdict: BotVerdict::KnownBot,
                category: "search_engine".to_string(),
            },
            BotRule {
                id: 1005,
                description: "Baiduspider".to_string(),
                pattern: Regex::new(r"(?i)baiduspider").unwrap(),
                verdict: BotVerdict::KnownBot,
                category: "search_engine".to_string(),
            },

            // Suspicious patterns
            BotRule {
                id: 2001,
                description: "Python requests library".to_string(),
                pattern: Regex::new(r"(?i)python-requests").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "scripted".to_string(),
            },
            BotRule {
                id: 2002,
                description: "cURL command-line tool".to_string(),
                pattern: Regex::new(r"(?i)^curl/").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "scripted".to_string(),
            },
            BotRule {
                id: 2003,
                description: "Wget command-line tool".to_string(),
                pattern: Regex::new(r"(?i)wget").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "scripted".to_string(),
            },
            BotRule {
                id: 2004,
                description: "Go HTTP client".to_string(),
                pattern: Regex::new(r"(?i)go-http-client").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "scripted".to_string(),
            },
            BotRule {
                id: 2005,
                description: "Java HTTP client".to_string(),
                pattern: Regex::new(r"(?i)java/[0-9]").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "scripted".to_string(),
            },

            // Malicious bots
            BotRule {
                id: 3001,
                description: "Scrapy framework".to_string(),
                pattern: Regex::new(r"(?i)scrapy").unwrap(),
                verdict: BotVerdict::Malicious,
                category: "scraper".to_string(),
            },
            BotRule {
                id: 3003,
                description: "Crawler pattern".to_string(),
                pattern: Regex::new(r"(?i)crawler").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "crawler".to_string(),
            },
            BotRule {
                id: 3004,
                description: "Spider pattern".to_string(),
                pattern: Regex::new(r"(?i)spider").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "crawler".to_string(),
            },
            BotRule {
                id: 3005,
                description: "Headless browser (PhantomJS)".to_string(),
                pattern: Regex::new(r"(?i)phantomjs").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "headless".to_string(),
            },
            BotRule {
                id: 3006,
                description: "Headless browser (Puppeteer)".to_string(),
                pattern: Regex::new(r"(?i)headlesschrome").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "headless".to_string(),
            },
            BotRule {
                id: 3007,
                description: "Empty user-agent".to_string(),
                pattern: Regex::new(r"^$").unwrap(),
                verdict: BotVerdict::Suspicious,
                category: "empty".to_string(),
            },
        ]
    }

    /// Get current rate limit stats for an IP
    pub fn get_rate_limit_stats(&self, client_ip: &str) -> Option<(u64, f64)> {
        let rate_limits = self.rate_limits.read().ok()?;
        let entry = rate_limits.get(client_ip)?;
        Some((entry.request_count, entry.requests_per_second()))
    }

    /// Reset rate limit for an IP (for testing)
    pub fn reset_rate_limit(&self, client_ip: &str) {
        if let Ok(mut rate_limits) = self.rate_limits.write() {
            rate_limits.remove(client_ip);
        }
    }

    /// Clear all rate limit entries
    pub fn clear_rate_limits(&self) {
        if let Ok(mut rate_limits) = self.rate_limits.write() {
            rate_limits.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bot_config_default() {
        let config = BotConfig::default();
        assert!(config.enabled);
        assert_eq!(config.known_bot_action, BotAction::Allow);
        assert_eq!(config.suspicious_action, BotAction::Challenge);
        assert_eq!(config.malicious_action, BotAction::Block);
        assert_eq!(config.rate_limit_requests, 100);
        assert_eq!(config.rate_limit_window_secs, 60);
    }

    #[test]
    fn test_known_bot_detection_googlebot() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0; // High threshold to avoid rate limit interference
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request(
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "66.249.66.1",
        );

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
        assert_eq!(detection.action, BotAction::Allow);
        assert_eq!(detection.rule_id, Some(1001));
        assert!(detection.rule_description.unwrap().contains("Googlebot"));
    }

    #[test]
    fn test_known_bot_detection_bingbot() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request(
            "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
            "157.55.39.1",
        );

        assert_eq!(detection.verdict, BotVerdict::KnownBot);
        assert_eq!(detection.action, BotAction::Allow);
        assert_eq!(detection.category, Some("search_engine".to_string()));
    }

    #[test]
    fn test_suspicious_bot_python_requests() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("python-requests/2.25.1", "192.168.1.100");

        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Challenge);
        assert_eq!(detection.rule_id, Some(2001));
    }

    #[test]
    fn test_suspicious_bot_curl() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("curl/7.68.0", "192.168.1.101");

        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Challenge);
    }

    #[test]
    fn test_malicious_bot_scrapy() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("Scrapy/2.5.0", "192.168.1.102");

        assert_eq!(detection.verdict, BotVerdict::Malicious);
        assert_eq!(detection.action, BotAction::Block);
        assert_eq!(detection.rule_id, Some(3001));
    }

    #[test]
    fn test_human_user_agent() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
            "192.168.1.103",
        );

        assert_eq!(detection.verdict, BotVerdict::Human);
        assert_eq!(detection.action, BotAction::Allow);
        assert_eq!(detection.rule_id, None);
    }

    #[test]
    fn test_whitelist_bypass() {
        let mut config = BotConfig::default();
        config.whitelist_user_agents.push("MyTrustedBot".to_string());
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("MyTrustedBot/1.0 Scrapy/2.0", "192.168.1.104");

        assert_eq!(detection.verdict, BotVerdict::Human);
        assert_eq!(detection.action, BotAction::Allow);
        assert!(detection.reason.contains("whitelisted"));
    }

    #[test]
    fn test_blacklist_immediate_block() {
        let mut config = BotConfig::default();
        config.blacklist_user_agents.push("EvilBot".to_string());
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("EvilBot/1.0", "192.168.1.105");

        assert_eq!(detection.verdict, BotVerdict::Malicious);
        assert_eq!(detection.action, BotAction::Block);
        assert!(detection.reason.contains("blacklisted"));
    }

    #[test]
    fn test_bot_disabled() {
        let mut config = BotConfig::default();
        config.enabled = false;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("Scrapy/2.5.0", "192.168.1.106");

        assert_eq!(detection.verdict, BotVerdict::Human);
        assert_eq!(detection.action, BotAction::Allow);
    }

    #[test]
    fn test_rate_limit_tracking() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        // Make several requests
        for _ in 0..5 {
            bot_mgr.analyze_request(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
                "192.168.1.107",
            );
        }

        let stats = bot_mgr.get_rate_limit_stats("192.168.1.107");
        assert!(stats.is_some());
        let (count, _rate) = stats.unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_rate_limit_reset() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        bot_mgr.analyze_request("Mozilla/5.0", "192.168.1.108");
        assert!(bot_mgr.get_rate_limit_stats("192.168.1.108").is_some());

        bot_mgr.reset_rate_limit("192.168.1.108");
        assert!(bot_mgr.get_rate_limit_stats("192.168.1.108").is_none());
    }

    #[test]
    fn test_empty_user_agent() {
        let config = BotConfig::default();
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("", "192.168.1.109");

        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Challenge);
    }

    #[test]
    fn test_headless_browser_detection() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("HeadlessChrome/91.0.4472.124", "192.168.1.110");

        assert_eq!(detection.verdict, BotVerdict::Suspicious);
        assert_eq!(detection.action, BotAction::Challenge);
        assert_eq!(detection.category, Some("headless".to_string()));
    }

    #[test]
    fn test_custom_action_per_verdict() {
        let mut config = BotConfig::default();
        config.known_bot_action = BotAction::RateLimit;
        config.suspicious_action = BotAction::Block;
        config.suspicious_rate_threshold = 1000.0;

        let bot_mgr = BotManager::new(config);

        let detection = bot_mgr.analyze_request("Googlebot/2.1", "66.249.66.1");
        assert_eq!(detection.action, BotAction::RateLimit);

        let detection2 = bot_mgr.analyze_request("curl/7.68.0", "192.168.1.111");
        assert_eq!(detection2.action, BotAction::Block);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let detection1 = bot_mgr.analyze_request("GOOGLEBOT/2.1", "66.249.66.1");
        assert_eq!(detection1.verdict, BotVerdict::KnownBot);

        let detection2 = bot_mgr.analyze_request("googlebot/2.1", "66.249.66.2");
        assert_eq!(detection2.verdict, BotVerdict::KnownBot);

        let detection3 = bot_mgr.analyze_request("GoogleBot/2.1", "66.249.66.3");
        assert_eq!(detection3.verdict, BotVerdict::KnownBot);
    }

    #[test]
    fn test_multiple_bot_rules_priority() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        // User-agent contains both "bot" and specific bot name
        let detection = bot_mgr.analyze_request(
            "Mozilla/5.0 (compatible; Googlebot/2.1)",
            "66.249.66.1",
        );

        // Should match Googlebot first (higher priority)
        assert_eq!(detection.verdict, BotVerdict::KnownBot);
        assert_eq!(detection.rule_id, Some(1001));
    }

    #[test]
    fn test_all_search_engines() {
        let mut config = BotConfig::default();
        config.suspicious_rate_threshold = 1000.0;
        let bot_mgr = BotManager::new(config);

        let test_cases = vec![
            ("Googlebot/2.1", BotVerdict::KnownBot),
            ("bingbot/2.0", BotVerdict::KnownBot),
            ("Yahoo! Slurp", BotVerdict::KnownBot),
            ("DuckDuckBot/1.0", BotVerdict::KnownBot),
            ("Baiduspider/2.0", BotVerdict::KnownBot),
        ];

        for (ua, expected_verdict) in test_cases {
            let detection = bot_mgr.analyze_request(ua, "1.2.3.4");
            assert_eq!(
                detection.verdict, expected_verdict,
                "Failed for user-agent: {}",
                ua
            );
            assert_eq!(detection.action, BotAction::Allow);
        }
    }
}
