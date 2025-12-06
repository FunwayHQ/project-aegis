// AEGIS Bot Detection Wasm Module
// Sprint 9: Advanced Bot Management
// Sprint Y8.7: Security Hardening - Max UA length and exact matching
//
// This module implements bot detection heuristics that run in a WebAssembly sandbox.
// It analyzes User-Agent strings and provides bot classification.

use core::slice;
use core::str;

// =============================================================================
// Y8.7: User-Agent Length Limits
// =============================================================================
//
// RFC 7231 doesn't specify a max User-Agent length, but excessively long
// User-Agent strings are suspicious and may be:
// 1. DoS attacks trying to exhaust memory
// 2. Payload injection attempts
// 3. Header injection attacks

/// Y8.7: Maximum User-Agent length (bytes)
/// Beyond this length, the request is automatically marked suspicious.
/// Legitimate browsers typically have User-Agents between 100-300 bytes.
pub const MAX_USER_AGENT_LENGTH: usize = 1024;

/// Y8.7: Minimum User-Agent length for legitimate browsers
/// Extremely short User-Agents are suspicious as browsers include
/// detailed version and platform information.
pub const MIN_USER_AGENT_LENGTH: usize = 10;

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

/// Y8.7: Exact match patterns for known tools
///
/// These patterns require exact matches to prevent false positives.
/// For example, "curl/7.68.0" should match but "curly-hair" should not.
static KNOWN_BOT_EXACT_PREFIXES: &[&str] = &[
    "curl/",           // curl/7.68.0
    "wget/",           // wget/1.21
    "Wget/",           // Wget/1.21
    "python-requests/", // python-requests/2.25.1
    "python-urllib/",   // python-urllib3/1.26.6
    "python/",         // python/3.9
    "Java/",           // Java/1.8.0_281
    "Go-http-client/", // Go-http-client/2.0
    "okhttp/",         // okhttp/4.9.0
    "axios/",          // axios/0.21.1
    "node-fetch/",     // node-fetch/2.6.1
    "httpx/",          // httpx/0.18.1
    "aiohttp/",        // aiohttp/3.7.4
    "libwww-perl/",    // libwww-perl/6.52
    "Ruby/",           // Ruby/3.0.0
    "Faraday/",        // Faraday/1.3.0
    "RestSharp/",      // RestSharp/106.11.7
    "nikto/",          // nikto/2.1.6
    "nmap ",           // nmap 7.80
    "masscan/",        // masscan/1.3.2
    "sqlmap/",         // sqlmap/1.5.2
    "Scrapy/",         // Scrapy/2.5.0
    "Apache-HttpClient/", // Apache-HttpClient/4.5.13
    "PostmanRuntime/", // PostmanRuntime/7.28.0
];

/// Y8.7: Exact match for complete User-Agent strings
///
/// Some minimal User-Agents are suspicious and should be caught exactly.
static SUSPICIOUS_EXACT_MATCH: &[&str] = &[
    "-",
    "Mozilla",
    "Mozilla/",
    "Mozilla/4",
    "Mozilla/5",
    "Mozilla/5.0",
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
///
/// # Y8.7 Security Enhancements
/// - Enforces MAX_USER_AGENT_LENGTH to prevent DoS
/// - Uses exact prefix matching for known tools
/// - Catches minimal/suspicious exact User-Agents
#[no_mangle]
pub extern "C" fn detect_bot(user_agent_ptr: *const u8, user_agent_len: usize) -> u32 {
    // Safety: Host must provide valid UTF-8 string
    let user_agent_bytes = unsafe {
        if user_agent_ptr.is_null() || user_agent_len == 0 {
            return BotVerdict::Suspicious as u32;
        }

        // Y8.7: Early length check before creating slice
        // Prevents allocation of huge buffers
        if user_agent_len > MAX_USER_AGENT_LENGTH {
            return BotVerdict::Suspicious as u32;
        }

        slice::from_raw_parts(user_agent_ptr, user_agent_len)
    };

    // Convert to string (return Suspicious if invalid UTF-8)
    let user_agent = match str::from_utf8(user_agent_bytes) {
        Ok(s) => s,
        Err(_) => return BotVerdict::Suspicious as u32,
    };

    // Y8.7: Check for exact suspicious matches first
    // These are User-Agents that are too minimal to be legitimate browsers
    for &exact in SUSPICIOUS_EXACT_MATCH {
        if user_agent == exact {
            return BotVerdict::Suspicious as u32;
        }
    }

    // Y8.7: Check for known bot exact prefixes (more precise matching)
    // This prevents false positives like "curly-hair" matching "curl"
    for &prefix in KNOWN_BOT_EXACT_PREFIXES {
        if user_agent.starts_with(prefix) {
            return BotVerdict::KnownBot as u32;
        }
    }

    // Check for known bot signatures (case-insensitive substring matching)
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
    // Y8.7: Use constants for length limits
    // Too short (legitimate browsers have detailed User-Agents)
    if user_agent.len() < MIN_USER_AGENT_LENGTH {
        return true;
    }

    // Y8.7: Use MAX_USER_AGENT_LENGTH constant
    // Note: This check is also done earlier in detect_bot() for early rejection
    if user_agent.len() > MAX_USER_AGENT_LENGTH {
        return true;
    }

    // Missing "Mozilla/" prefix (most browsers include this)
    if !user_agent.starts_with("Mozilla/") {
        // Exception: known tools that are detected by exact prefix matching
        // If we reach here, the UA wasn't caught by KNOWN_BOT_EXACT_PREFIXES,
        // so it's suspicious
        let exceptions = ["Pingdom", "UptimeRobot", "StatusCake", "Site24x7"];
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

    // Contains null bytes (potential injection)
    if user_agent.contains('\0') {
        return true;
    }

    // Contains newlines (potential header injection)
    if user_agent.contains('\n') || user_agent.contains('\r') {
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
