use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use wasmtime::*;

/// Bot detection verdict returned by Wasm module
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum BotVerdict {
    Human = 0,
    KnownBot = 1,
    Suspicious = 2,
}

impl From<u32> for BotVerdict {
    fn from(value: u32) -> Self {
        match value {
            0 => BotVerdict::Human,
            1 => BotVerdict::KnownBot,
            2 => BotVerdict::Suspicious,
            _ => BotVerdict::Suspicious, // Default to suspicious for unknown values
        }
    }
}

/// Bot policy action to take based on verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotAction {
    /// Allow the request
    Allow,
    /// Block with 403
    Block,
    /// Issue JavaScript challenge (for PoC, just log)
    Challenge,
    /// Log but allow
    Log,
}

/// Bot management policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotPolicy {
    /// Enabled/disabled
    pub enabled: bool,
    /// Action for known bots
    pub known_bot_action: BotAction,
    /// Action for suspicious requests
    pub suspicious_action: BotAction,
    /// Action for humans
    pub human_action: BotAction,
    /// Enable rate limiting
    pub rate_limiting_enabled: bool,
    /// Requests per minute threshold for rate limiting
    pub rate_limit_threshold: u32,
    /// Rate limit window in seconds
    pub rate_limit_window_secs: u64,
}

impl Default for BotPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            known_bot_action: BotAction::Block,
            suspicious_action: BotAction::Challenge,
            human_action: BotAction::Allow,
            rate_limiting_enabled: true,
            rate_limit_threshold: 100, // 100 requests per window
            rate_limit_window_secs: 60, // 1 minute window
        }
    }
}

/// Rate limiter entry for tracking request rates per IP
#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: SystemTime,
}

/// Bot Management System with Wasm-based detection
pub struct BotManager {
    /// Wasmtime engine
    engine: Engine,
    /// Compiled Wasm module
    module: Module,
    /// Bot policy configuration
    policy: BotPolicy,
    /// Rate limiter (IP -> RateLimitEntry)
    rate_limiter: Arc<std::sync::Mutex<HashMap<String, RateLimitEntry>>>,
}

impl BotManager {
    /// Create new bot manager with Wasm module from file
    pub fn new(wasm_path: impl AsRef<Path>, policy: BotPolicy) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, wasm_path)
            .context("Failed to load Wasm module")?;

        Ok(Self {
            engine,
            module,
            policy,
            rate_limiter: Arc::new(std::sync::Mutex::new(HashMap::new())),
        })
    }

    /// Create bot manager from Wasm bytes
    pub fn from_bytes(wasm_bytes: &[u8], policy: BotPolicy) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes)
            .context("Failed to load Wasm module from bytes")?;

        Ok(Self {
            engine,
            module,
            policy,
            rate_limiter: Arc::new(std::sync::Mutex::new(HashMap::new())),
        })
    }

    /// Detect bot from User-Agent string using Wasm module
    pub fn detect_bot(&self, user_agent: &str) -> Result<BotVerdict> {
        // Create Wasm instance
        let mut store = Store::new(&self.engine, ());

        // Create linker (no imports needed for this simple module)
        let linker = Linker::new(&self.engine);

        // Instantiate module
        let instance = linker
            .instantiate(&mut store, &self.module)
            .context("Failed to instantiate Wasm module")?;

        // Get memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("Failed to get Wasm memory")?;

        // Allocate memory in Wasm for user-agent string
        let alloc = instance
            .get_typed_func::<u32, u32>(&mut store, "alloc")
            .context("Failed to get alloc function")?;

        let user_agent_len = user_agent.len() as u32;
        let user_agent_ptr = alloc
            .call(&mut store, user_agent_len)
            .context("Failed to allocate Wasm memory")?;

        // Write user-agent to Wasm memory
        memory
            .write(&mut store, user_agent_ptr as usize, user_agent.as_bytes())
            .context("Failed to write to Wasm memory")?;

        // Call detect_bot function
        let detect_bot = instance
            .get_typed_func::<(u32, u32), u32>(&mut store, "detect_bot")
            .context("Failed to get detect_bot function")?;

        let verdict_u32 = detect_bot
            .call(&mut store, (user_agent_ptr, user_agent_len))
            .context("Failed to call detect_bot")?;

        // Deallocate memory
        let dealloc = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "dealloc")
            .ok();
        if let Some(dealloc) = dealloc {
            let _ = dealloc.call(&mut store, (user_agent_ptr, user_agent_len));
        }

        Ok(BotVerdict::from(verdict_u32))
    }

    /// Check rate limit for IP address
    /// Returns true if rate limit exceeded
    pub fn check_rate_limit(&self, ip: &str) -> bool {
        if !self.policy.rate_limiting_enabled {
            return false;
        }

        let mut limiter = self.rate_limiter.lock().unwrap();
        let now = SystemTime::now();
        let window_duration = Duration::from_secs(self.policy.rate_limit_window_secs);

        if let Some(entry) = limiter.get_mut(ip) {
            // Check if window expired
            if now
                .duration_since(entry.window_start)
                .unwrap_or(Duration::ZERO)
                > window_duration
            {
                // Reset window
                entry.count = 1;
                entry.window_start = now;
                false
            } else {
                // Increment count
                entry.count += 1;
                entry.count > self.policy.rate_limit_threshold
            }
        } else {
            // New IP, create entry
            limiter.insert(
                ip.to_string(),
                RateLimitEntry {
                    count: 1,
                    window_start: now,
                },
            );
            false
        }
    }

    /// Analyze request and determine action
    pub fn analyze_request(&self, user_agent: &str, ip: &str) -> Result<(BotVerdict, BotAction)> {
        if !self.policy.enabled {
            return Ok((BotVerdict::Human, BotAction::Allow));
        }

        // Check rate limit first
        if self.check_rate_limit(ip) {
            log::warn!("Rate limit exceeded for IP: {}", ip);
            return Ok((BotVerdict::Suspicious, BotAction::Block));
        }

        // Detect bot using Wasm module
        let verdict = self.detect_bot(user_agent)?;

        // Determine action based on verdict and policy
        let action = match verdict {
            BotVerdict::Human => self.policy.human_action,
            BotVerdict::KnownBot => self.policy.known_bot_action,
            BotVerdict::Suspicious => self.policy.suspicious_action,
        };

        Ok((verdict, action))
    }

    /// Get policy configuration
    pub fn policy(&self) -> &BotPolicy {
        &self.policy
    }

    /// Update policy configuration
    pub fn set_policy(&mut self, policy: BotPolicy) {
        self.policy = policy;
    }

    /// Clear rate limiter state
    pub fn clear_rate_limiter(&self) {
        self.rate_limiter.lock().unwrap().clear();
    }

    /// Get rate limiter statistics
    pub fn get_rate_limiter_stats(&self) -> (usize, u32) {
        let limiter = self.rate_limiter.lock().unwrap();
        let total_tracked = limiter.len();
        let max_count = limiter.values().map(|e| e.count).max().unwrap_or(0);
        (total_tracked, max_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WASM_PATH: &str = "bot-detector.wasm";

    fn get_test_manager() -> BotManager {
        let policy = BotPolicy::default();
        BotManager::new(WASM_PATH, policy).expect("Failed to create bot manager")
    }

    #[test]
    fn test_detect_known_bots() {
        let manager = get_test_manager();

        let bot_user_agents = vec![
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "curl/7.68.0",
            "python-requests/2.25.1",
            "nikto/2.1.6",
        ];

        for ua in bot_user_agents {
            let verdict = manager.detect_bot(ua).expect("Failed to detect");
            assert_eq!(
                verdict,
                BotVerdict::KnownBot,
                "Should detect as bot: {}",
                ua
            );
        }
    }

    #[test]
    fn test_detect_humans() {
        let manager = get_test_manager();

        let human_user_agents = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        ];

        for ua in human_user_agents {
            let verdict = manager.detect_bot(ua).expect("Failed to detect");
            assert_eq!(verdict, BotVerdict::Human, "Should detect as human: {}", ua);
        }
    }

    #[test]
    fn test_detect_suspicious() {
        let manager = get_test_manager();

        let suspicious_user_agents = vec![
            "",                          // Empty
            "X",                         // Too short
            "<script>alert(1)</script>", // XSS
        ];

        for ua in suspicious_user_agents {
            let verdict = manager.detect_bot(ua).expect("Failed to detect");
            assert_eq!(
                verdict,
                BotVerdict::Suspicious,
                "Should detect as suspicious: {}",
                ua
            );
        }
    }

    #[test]
    fn test_policy_actions() {
        let manager = get_test_manager();

        // Test known bot -> block
        let (verdict, action) = manager
            .analyze_request("Googlebot", "192.168.1.1")
            .expect("Failed");
        assert_eq!(verdict, BotVerdict::KnownBot);
        assert_eq!(action, BotAction::Block);

        // Test human -> allow
        let (verdict, action) = manager
            .analyze_request(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
                "192.168.1.2",
            )
            .expect("Failed");
        assert_eq!(verdict, BotVerdict::Human);
        assert_eq!(action, BotAction::Allow);
    }

    #[test]
    fn test_rate_limiting() {
        let policy = BotPolicy {
            rate_limiting_enabled: true,
            rate_limit_threshold: 5,
            rate_limit_window_secs: 60,
            ..Default::default()
        };
        let manager = BotManager::new(WASM_PATH, policy).expect("Failed to create");

        let ip = "192.168.1.100";

        // First 5 requests should pass
        for _ in 0..5 {
            assert!(!manager.check_rate_limit(ip), "Should not be rate limited");
        }

        // 6th request should be rate limited
        assert!(manager.check_rate_limit(ip), "Should be rate limited");

        // Clear and verify
        manager.clear_rate_limiter();
        assert!(
            !manager.check_rate_limit(ip),
            "Should not be rate limited after clear"
        );
    }

    #[test]
    fn test_policy_disabled() {
        let policy = BotPolicy {
            enabled: false,
            ..Default::default()
        };
        let manager = BotManager::new(WASM_PATH, policy).expect("Failed to create");

        // Even with bot user-agent, should return Human/Allow when disabled
        let (verdict, action) = manager
            .analyze_request("Googlebot", "192.168.1.1")
            .expect("Failed");
        assert_eq!(verdict, BotVerdict::Human);
        assert_eq!(action, BotAction::Allow);
    }
}
