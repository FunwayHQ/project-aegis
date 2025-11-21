//! AEGIS WAF - Wasm Edition (Sprint 13)
//!
//! This is the Sprint 8 WAF refactored to run in WebAssembly sandbox.
//! Provides identical OWASP rule coverage with resource governance.
//!
//! Exports:
//! - analyze_request(ptr, len) -> ptr to JSON result
//! - alloc(size) -> ptr for host to write data
//! - dealloc(ptr, size) -> free memory
//!
//! Resource Limits (enforced by host):
//! - Max execution: 10ms
//! - Max memory: 10MB
//! - CPU cycles: ~1M fuel units

use serde::{Deserialize, Serialize};
use std::alloc::{alloc, dealloc, Layout};
use std::slice;

/// WAF analysis result (matches wasm_runtime.rs WafResult)
#[derive(Debug, Serialize, Deserialize)]
pub struct WafResult {
    pub blocked: bool,
    pub matches: Vec<WafMatch>,
    pub execution_time_us: u64, // Set by host after return
}

/// Individual rule match (matches wasm_runtime.rs WafMatch)
#[derive(Debug, Serialize, Deserialize)]
pub struct WafMatch {
    pub rule_id: u32,
    pub description: String,
    pub severity: u8,
    pub category: String,
    pub matched_value: String,
    pub location: String,
}

/// Request data passed from host
#[derive(Debug, Deserialize)]
struct RequestData {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: String,
}

/// WAF Rule (internal) - simplified pattern matching
struct WafRule {
    id: u32,
    description: &'static str,
    patterns: &'static [&'static str],
    severity: u8,
    category: &'static str,
    case_sensitive: bool,
}

impl WafRule {
    fn matches(&self, text: &str) -> Option<String> {
        let search_text = if self.case_sensitive {
            text.to_string()
        } else {
            text.to_lowercase()
        };

        for pattern in self.patterns {
            let search_pattern = if self.case_sensitive {
                pattern.to_string()
            } else {
                pattern.to_lowercase()
            };

            if search_text.contains(&search_pattern) {
                // Find the actual match in original text
                let start = search_text.find(&search_pattern).unwrap();
                let end = start + search_pattern.len();
                return Some(text[start..end].to_string());
            }
        }
        None
    }
}

/// Build the 13 OWASP rules from Sprint 8 (simplified patterns)
fn build_rules() -> Vec<WafRule> {
    vec![
        // ========================================
        // SQL Injection (OWASP #1) - 3 rules
        // ========================================
        WafRule {
            id: 942100,
            description: "SQL Injection Attack: Common DB names",
            patterns: &[
                "union select", "select from", "insert into",
                "delete from", "drop table", "exec xp_",
            ],
            severity: 5, // Critical
            category: "sqli",
            case_sensitive: false,
        },
        WafRule {
            id: 942110,
            description: "SQL Injection: Comment-based injection",
            patterns: &["' or '", "' and '", "'--", "' --"],
            severity: 5, // Critical
            category: "sqli",
            case_sensitive: false,
        },
        WafRule {
            id: 942120,
            description: "SQL Injection: MySQL comments and operators",
            patterns: &["/*!", "--", "xp_cmdshell", "sp_executesql"],
            severity: 4, // Error
            category: "sqli",
            case_sensitive: false,
        },

        // ========================================
        // Cross-Site Scripting (OWASP #3) - 4 rules
        // ========================================
        WafRule {
            id: 941100,
            description: "XSS Attack: Script tag injection",
            patterns: &["<script", "</script>"],
            severity: 5, // Critical
            category: "xss",
            case_sensitive: false,
        },
        WafRule {
            id: 941110,
            description: "XSS Attack: Event handler injection",
            patterns: &["onerror=", "onload=", "onclick=", "onmouseover="],
            severity: 5, // Critical
            category: "xss",
            case_sensitive: false,
        },
        WafRule {
            id: 941120,
            description: "XSS Attack: JavaScript protocol",
            patterns: &["javascript:"],
            severity: 4, // Error
            category: "xss",
            case_sensitive: false,
        },
        WafRule {
            id: 941130,
            description: "XSS Attack: Iframe injection",
            patterns: &["<iframe"],
            severity: 4, // Error
            category: "xss",
            case_sensitive: false,
        },

        // ========================================
        // Path Traversal / LFI (OWASP #1) - 2 rules
        // ========================================
        WafRule {
            id: 930100,
            description: "Path Traversal: ../ patterns",
            patterns: &["../", "..\\"],
            severity: 5, // Critical
            category: "path-traversal",
            case_sensitive: true,
        },
        WafRule {
            id: 930110,
            description: "Path Traversal: /etc/passwd access",
            patterns: &["/etc/passwd", "/etc/shadow", "../../etc"],
            severity: 5, // Critical
            category: "path-traversal",
            case_sensitive: false,
        },

        // ========================================
        // Remote Code Execution / Command Injection - 2 rules
        // ========================================
        WafRule {
            id: 932100,
            description: "RCE: Unix shell command injection",
            patterns: &["; ls", "; cat", "; wget", "; curl", "; bash", "; sh", "| cat", "| ls", "$(", "&& "],
            severity: 5, // Critical
            category: "rce",
            case_sensitive: false,
        },
        WafRule {
            id: 932110,
            description: "RCE: Windows commands",
            patterns: &["cmd.exe", "powershell", "net.exe", "wscript"],
            severity: 5, // Critical
            category: "rce",
            case_sensitive: false,
        },

        // ========================================
        // HTTP Protocol Violations - 1 rule
        // ========================================
        WafRule {
            id: 920100,
            description: "HTTP Protocol: Invalid method",
            patterns: &["TRACE", "TRACK", "DEBUG"],
            severity: 3, // Warning
            category: "protocol",
            case_sensitive: false,
        },

        // ========================================
        // Scanner/Bot Detection - 1 rule
        // ========================================
        WafRule {
            id: 913100,
            description: "Scanner Detection: Common scanner signatures",
            patterns: &["nikto", "nmap", "masscan", "sqlmap", "dirbuster", "acunetix"],
            severity: 4, // Error
            category: "scanner",
            case_sensitive: false,
        },
    ]
}

/// Analyze request and return matches
fn analyze(request: RequestData) -> WafResult {
    let rules = build_rules();
    let mut matches = Vec::new();
    let min_severity = 3; // Warning and above

    // Check URI
    for rule in &rules {
        if let Some(matched_value) = rule.matches(&request.uri) {
            if rule.severity >= min_severity {
                matches.push(WafMatch {
                    rule_id: rule.id,
                    description: rule.description.to_string(),
                    severity: rule.severity,
                    category: rule.category.to_string(),
                    matched_value,
                    location: "URI".to_string(),
                });
            }
        }
    }

    // Check headers
    for (name, value) in &request.headers {
        for rule in &rules {
            if let Some(matched_value) = rule.matches(value) {
                if rule.severity >= min_severity {
                    matches.push(WafMatch {
                        rule_id: rule.id,
                        description: rule.description.to_string(),
                        severity: rule.severity,
                        category: rule.category.to_string(),
                        matched_value,
                        location: format!("Header:{}", name),
                    });
                }
            }
        }
    }

    // Check method (for protocol violations)
    for rule in &rules {
        if rule.category == "protocol" {
            if let Some(matched_value) = rule.matches(&request.method) {
                if rule.severity >= min_severity {
                    matches.push(WafMatch {
                        rule_id: rule.id,
                        description: rule.description.to_string(),
                        severity: rule.severity,
                        category: rule.category.to_string(),
                        matched_value,
                        location: "Method".to_string(),
                    });
                }
            }
        }
    }

    // Check body
    if !request.body.is_empty() {
        for rule in &rules {
            if let Some(matched_value) = rule.matches(&request.body) {
                if rule.severity >= min_severity {
                    matches.push(WafMatch {
                        rule_id: rule.id,
                        description: rule.description.to_string(),
                        severity: rule.severity,
                        category: rule.category.to_string(),
                        matched_value,
                        location: "Body".to_string(),
                    });
                }
            }
        }
    }

    // Determine if request should be blocked (any Critical match)
    let blocked = matches.iter().any(|m| m.severity >= 5);

    WafResult {
        blocked,
        matches,
        execution_time_us: 0, // Host will set this
    }
}

/// WASM Export: Analyze request
///
/// Host allocates memory via alloc(), writes JSON request data, calls this.
/// Returns pointer to result (format: 4 bytes length + JSON data)
#[no_mangle]
pub extern "C" fn analyze_request(ptr: u32, len: u32) -> u32 {
    // Read request JSON from Wasm memory
    let request_bytes = unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) };

    let request: RequestData = match serde_json::from_slice(request_bytes) {
        Ok(req) => req,
        Err(_) => {
            // Return error result
            let error_result = WafResult {
                blocked: false,
                matches: Vec::new(),
                execution_time_us: 0,
            };
            return write_result(&error_result);
        }
    };

    // Analyze request
    let result = analyze(request);

    // Write result to Wasm memory
    write_result(&result)
}

/// Write result to Wasm memory (format: 4-byte length + JSON)
fn write_result(result: &WafResult) -> u32 {
    let json = serde_json::to_string(result).unwrap_or_else(|_| "{}".to_string());
    let json_bytes = json.as_bytes();
    let json_len = json_bytes.len() as u32;

    // Allocate: 4 bytes for length + JSON data
    let total_size = 4 + json_bytes.len();
    let layout = Layout::from_size_align(total_size, 4).unwrap();
    let result_ptr = unsafe { alloc(layout) };

    // Write length (first 4 bytes)
    unsafe {
        *(result_ptr as *mut u32) = json_len;
    }

    // Write JSON data
    unsafe {
        let data_ptr = result_ptr.add(4);
        std::ptr::copy_nonoverlapping(json_bytes.as_ptr(), data_ptr, json_bytes.len());
    }

    result_ptr as u32
}

/// WASM Export: Allocate memory for host to write data
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    let layout = Layout::from_size_align(size as usize, 4).unwrap();
    unsafe { alloc(layout) as u32 }
}

/// WASM Export: Deallocate memory (not currently used, but exported for completeness)
#[no_mangle]
pub extern "C" fn dealloc(ptr: u32, size: u32) {
    let layout = Layout::from_size_align(size as usize, 4).unwrap();
    unsafe { dealloc(ptr as *mut u8, layout) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_injection_detection() {
        let request = RequestData {
            method: "GET".to_string(),
            uri: "SELECT * FROM users".to_string(),
            headers: Vec::new(),
            body: String::new(),
        };

        let result = analyze(request);
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].category, "sqli");
        assert!(result.blocked);
    }

    #[test]
    fn test_xss_detection() {
        let request = RequestData {
            method: "GET".to_string(),
            uri: "<script>alert('XSS')</script>".to_string(),
            headers: Vec::new(),
            body: String::new(),
        };

        let result = analyze(request);
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].category, "xss");
        assert!(result.blocked);
    }

    #[test]
    fn test_path_traversal_detection() {
        let request = RequestData {
            method: "GET".to_string(),
            uri: "../../../etc/passwd".to_string(),
            headers: Vec::new(),
            body: String::new(),
        };

        let result = analyze(request);
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].category, "path-traversal");
        assert!(result.blocked);
    }

    #[test]
    fn test_rce_detection() {
        let request = RequestData {
            method: "GET".to_string(),
            uri: "; ls -la".to_string(),
            headers: Vec::new(),
            body: String::new(),
        };

        let result = analyze(request);
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].category, "rce");
        assert!(result.blocked);
    }

    #[test]
    fn test_clean_request() {
        let request = RequestData {
            method: "GET".to_string(),
            uri: "/api/users".to_string(),
            headers: Vec::new(),
            body: String::new(),
        };

        let result = analyze(request);
        assert!(result.matches.is_empty());
        assert!(!result.blocked);
    }

    #[test]
    fn test_header_analysis() {
        let request = RequestData {
            method: "GET".to_string(),
            uri: "/".to_string(),
            headers: vec![("User-Agent".to_string(), "nikto scanner".to_string())],
            body: String::new(),
        };

        let result = analyze(request);
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].category, "scanner");
        assert!(result.matches[0].location.starts_with("Header:"));
    }

    #[test]
    fn test_body_analysis() {
        let request = RequestData {
            method: "POST".to_string(),
            uri: "/api/data".to_string(),
            headers: Vec::new(),
            body: "INSERT INTO users VALUES ('admin', 'pass')".to_string(),
        };

        let result = analyze(request);
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].category, "sqli");
        assert_eq!(result.matches[0].location, "Body");
        assert!(result.blocked);
    }
}
