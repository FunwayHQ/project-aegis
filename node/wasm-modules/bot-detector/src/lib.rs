// AEGIS Bot Detection Wasm Module
// Sprint 9: Advanced Bot Management
//
// This module implements bot detection heuristics that run in a WebAssembly sandbox.
// It analyzes User-Agent strings and provides bot classification.

use core::slice;
use core::str;

/// Bot detection verdict
#[repr(u8)]
pub enum BotVerdict {
    Human = 0,
    KnownBot = 1,
    Suspicious = 2,
}

/// Known bot user-agent patterns (substring matching)
static KNOWN_BOT_SIGNATURES: &[&str] = &[
    // Search Engine Crawlers
    "Googlebot",
    "Bingbot",
    "Slurp",          // Yahoo
    "DuckDuckBot",
    "Baiduspider",
    "YandexBot",
    "Sogou",

    // Social Media Crawlers
    "facebookexternalhit",
    "Twitterbot",
    "LinkedInBot",
    "Pinterestbot",
    "Slackbot",
    "Discordbot",
    "TelegramBot",
    "WhatsApp",

    // Security/Monitoring Bots (legitimate)
    "Pingdom",
    "UptimeRobot",
    "StatusCake",
    "Site24x7",

    // Development Tools
    "curl",
    "wget",
    "python-requests",
    "Java/",
    "Go-http-client",
    "Postman",
    "HTTPie",

    // Scrapers (potentially malicious)
    "Scrapy",
    "BeautifulSoup",
    "Selenium",
    "PhantomJS",
    "HeadlessChrome",

    // Vulnerability Scanners (malicious)
    "nikto",
    "nmap",
    "masscan",
    "sqlmap",
    "dirbuster",
    "acunetix",
    "nessus",
    "OpenVAS",
    "w3af",
    "ZAP",              // OWASP ZAP

    // Other bots
    "bot",
    "crawler",
    "spider",
    "scraper",
];

/// Suspicious patterns that indicate potential bots
static SUSPICIOUS_PATTERNS: &[&str] = &[
    // Missing common fields
    "Mozilla/4.0",      // Outdated browser version
    "Mozilla/3.0",

    // Suspicious characteristics
    "compatible;",      // Often used by bots
    "http://",          // URL in User-Agent (unusual)
    "https://",

    // Empty or minimal User-Agents
    "",
    "-",
];

/// Analyze User-Agent string and return bot verdict
///
/// Memory layout from host:
/// - user_agent_ptr: pointer to UTF-8 string
/// - user_agent_len: length of string in bytes
///
/// Returns: BotVerdict as u32
#[no_mangle]
pub extern "C" fn detect_bot(user_agent_ptr: *const u8, user_agent_len: usize) -> u32 {
    // Safety: Host must provide valid UTF-8 string
    let user_agent_bytes = unsafe {
        if user_agent_ptr.is_null() || user_agent_len == 0 {
            return BotVerdict::Suspicious as u32;
        }
        slice::from_raw_parts(user_agent_ptr, user_agent_len)
    };

    // Convert to string (return Suspicious if invalid UTF-8)
    let user_agent = match str::from_utf8(user_agent_bytes) {
        Ok(s) => s,
        Err(_) => return BotVerdict::Suspicious as u32,
    };

    // Check for known bot signatures (case-insensitive)
    let user_agent_lower = user_agent.to_lowercase();

    for &pattern in KNOWN_BOT_SIGNATURES {
        if user_agent_lower.contains(&pattern.to_lowercase()) {
            return BotVerdict::KnownBot as u32;
        }
    }

    // Check for suspicious patterns
    for &pattern in SUSPICIOUS_PATTERNS {
        if user_agent.contains(pattern) {
            return BotVerdict::Suspicious as u32;
        }
    }

    // Check heuristics
    if is_suspicious_heuristic(user_agent) {
        return BotVerdict::Suspicious as u32;
    }

    // Likely human
    BotVerdict::Human as u32
}

/// Heuristic-based suspicion detection
fn is_suspicious_heuristic(user_agent: &str) -> bool {
    // Too short (legitimate browsers have detailed User-Agents)
    if user_agent.len() < 10 {
        return true;
    }

    // Too long (may be injection attempt)
    if user_agent.len() > 500 {
        return true;
    }

    // Missing "Mozilla/" prefix (most browsers include this)
    if !user_agent.starts_with("Mozilla/") {
        // Exception: known tools
        let exceptions = ["curl", "wget", "Pingdom", "UptimeRobot"];
        if !exceptions.iter().any(|e| user_agent.contains(e)) {
            return true;
        }
    }

    // Contains script tags (potential XSS)
    if user_agent.contains("<script") || user_agent.contains("</script>") {
        return true;
    }

    // Contains SQL patterns (potential SQLi)
    if user_agent.contains("' OR ") || user_agent.contains("UNION SELECT") {
        return true;
    }

    false
}

/// Get the version of this Wasm module
#[no_mangle]
pub extern "C" fn get_version() -> u32 {
    100 // Version 1.0.0
}

/// Memory allocator for Wasm
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// Memory deallocator for Wasm
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_bots() {
        let test_cases = vec![
            ("Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)", BotVerdict::KnownBot),
            ("curl/7.68.0", BotVerdict::KnownBot),
            ("python-requests/2.25.1", BotVerdict::KnownBot),
            ("nikto/2.1.6", BotVerdict::KnownBot),
        ];

        for (ua, expected) in test_cases {
            let result = detect_bot(ua.as_ptr(), ua.len());
            assert_eq!(result, expected as u32, "Failed for UA: {}", ua);
        }
    }

    #[test]
    fn test_human_browsers() {
        let test_cases = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
        ];

        for ua in test_cases {
            let result = detect_bot(ua.as_ptr(), ua.len());
            assert_eq!(result, BotVerdict::Human as u32, "False positive for UA: {}", ua);
        }
    }

    #[test]
    fn test_suspicious() {
        let test_cases = vec![
            "",                          // Empty
            "X",                         // Too short
            "Mozilla/3.0",               // Outdated
            "<script>alert(1)</script>", // XSS attempt
            "' OR '1'='1",              // SQLi attempt
        ];

        for ua in test_cases {
            let result = detect_bot(ua.as_ptr(), ua.len());
            assert_eq!(result, BotVerdict::Suspicious as u32, "Should be suspicious: {}", ua);
        }
    }
}
