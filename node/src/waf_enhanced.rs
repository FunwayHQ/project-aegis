// Sprint 22: Enhanced WAF with OWASP CRS Support & ML Anomaly Scoring
//
// This module extends the basic WAF from Sprint 8 with:
// 1. ModSecurity SecRule parser (subset of syntax)
// 2. OWASP CRS 4.0 rule import capability
// 3. Custom rule engine with YAML/JSON configuration
// 4. Rule priority, chaining, and skip logic
// 5. ML anomaly scoring for requests

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::waf::{Severity, WafAction, RuleMatch};

// ============================================
// Regex Security Configuration
// ============================================

/// SECURITY FIX: Maximum allowed regex pattern length to prevent ReDoS
const MAX_REGEX_PATTERN_LENGTH: usize = 2048;

/// SECURITY FIX: Regex compilation with size limits and timeout protection
/// This prevents ReDoS attacks via maliciously crafted regex patterns
fn compile_safe_regex(pattern: &str) -> Result<Regex, String> {
    // Check pattern length
    if pattern.len() > MAX_REGEX_PATTERN_LENGTH {
        return Err(format!(
            "Regex pattern exceeds maximum length of {} characters",
            MAX_REGEX_PATTERN_LENGTH
        ));
    }

    // Check for known dangerous patterns that could cause catastrophic backtracking
    // These patterns are known to cause exponential time complexity
    let dangerous_patterns = [
        r"(\w+)+",     // Nested quantifiers on character classes
        r"(.*)*",      // Nested unlimited quantifiers
        r"(.+)+",      // Nested unlimited quantifiers
        r"(.+)*",      // Nested unlimited quantifiers
        r"([a-z]+)*",  // Nested quantifiers on character classes
    ];

    for dangerous in &dangerous_patterns {
        if pattern.contains(dangerous) {
            return Err(format!(
                "Regex pattern contains potentially dangerous nested quantifier: {}",
                dangerous
            ));
        }
    }

    // Compile the regex with a timeout (regex crate doesn't support timeout natively,
    // but we've at least validated the pattern structure)
    regex::RegexBuilder::new(pattern)
        .size_limit(1024 * 1024) // 1MB compiled size limit
        .dfa_size_limit(1024 * 1024) // 1MB DFA size limit
        .build()
        .map_err(|e| format!("Failed to compile regex: {}", e))
}

// ============================================
// SecRule Parser Types
// ============================================

/// ModSecurity rule operator
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecRuleOperator {
    /// Regular expression match (@rx)
    Regex(String),
    /// Exact string match (@eq)
    Eq(String),
    /// Greater than (@gt)
    Gt(i64),
    /// Less than (@lt)
    Lt(i64),
    /// String contains (@contains)
    Contains(String),
    /// String begins with (@beginsWith)
    BeginsWith(String),
    /// String ends with (@endsWith)
    EndsWith(String),
    /// Value in list (@within)
    Within(Vec<String>),
    /// IP address match (@ipMatch)
    IpMatch(Vec<String>),
    /// Detect SQLi (@detectSQLi)
    DetectSQLi,
    /// Detect XSS (@detectXSS)
    DetectXSS,
    /// Unconditional match
    Unconditional,
}

/// ModSecurity rule target (variable)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SecRuleTarget {
    /// Request arguments (query string + body)
    Args,
    /// Query string arguments only
    ArgsGet,
    /// Body arguments only
    ArgsPost,
    /// Specific argument by name
    ArgsNames,
    /// Request headers
    RequestHeaders,
    /// Specific header by name
    RequestHeadersNames,
    /// Request body (raw)
    RequestBody,
    /// Request URI
    RequestUri,
    /// Request URI path only
    RequestUriPath,
    /// Request method
    RequestMethod,
    /// Request protocol
    RequestProtocol,
    /// Remote address
    RemoteAddr,
    /// Request filename
    RequestFilename,
    /// Request basename
    RequestBasename,
    /// Request cookies
    RequestCookies,
    /// Request cookies names
    RequestCookiesNames,
    /// Response status
    ResponseStatus,
    /// Response headers
    ResponseHeaders,
    /// Response body
    ResponseBody,
    /// Transaction variable
    Tx(String),
    /// IP collection
    Ip(String),
    /// Global collection
    Global(String),
}

/// SecRule action
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecRuleAction {
    /// Block request
    Deny,
    /// Allow request (skip remaining rules)
    Allow,
    /// Pass to next rule
    Pass,
    /// Log the match
    Log,
    /// No logging
    NoLog,
    /// Redirect to URL
    Redirect(String),
    /// Skip N rules
    Skip(u32),
    /// Skip after rule ID
    SkipAfter(String),
    /// Chain to next rule (AND logic)
    Chain,
    /// Set variable
    SetVar(String, String),
    /// Expire variable
    ExpireVar(String, u64),
    /// Set transaction variable
    SetTx(String, String),
    /// Execute external script
    Exec(String),
    /// Custom status code
    Status(u16),
    /// Custom message
    Msg(String),
    /// Severity
    Severity(Severity),
    /// Tag
    Tag(String),
    /// Log data
    LogData(String),
    /// Capture groups
    Capture,
    /// Transform (lowercase, htmlEntityDecode, etc.)
    Transform(TransformType),
}

/// Transformation types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransformType {
    Lowercase,
    UrlDecode,
    UrlDecodeUni,
    HtmlEntityDecode,
    JsDecode,
    CssDecode,
    Base64Decode,
    HexDecode,
    CompressWhitespace,
    RemoveWhitespace,
    ReplaceNulls,
    RemoveNulls,
    Length,
    Sha1,
    Md5,
    None,
}

/// Parsed SecRule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecRule {
    /// Rule ID
    pub id: u32,
    /// Rule phase (1-5)
    pub phase: u8,
    /// Targets to inspect
    pub targets: Vec<SecRuleTarget>,
    /// Operator to apply
    pub operator: SecRuleOperator,
    /// Negated operator (!)
    pub negated: bool,
    /// Actions to take
    pub actions: Vec<SecRuleAction>,
    /// Is this a chained rule?
    pub is_chained: bool,
    /// Chain parent ID
    pub chain_parent: Option<u32>,
    /// Severity level
    pub severity: Severity,
    /// Rule message/description
    pub msg: String,
    /// Tags
    pub tags: Vec<String>,
    /// File where rule was defined
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
}

impl SecRule {
    /// Check if this rule matches the given input
    pub fn matches(&self, input: &str, transforms: &[TransformType]) -> Option<String> {
        // Apply transformations
        let transformed = self.apply_transforms(input, transforms);

        // Apply operator
        match &self.operator {
            SecRuleOperator::Regex(pattern) => {
                // SECURITY FIX: Use safe regex compilation with limits
                if let Ok(re) = compile_safe_regex(pattern) {
                    re.find(&transformed).map(|m| m.as_str().to_string())
                } else {
                    // Log and skip if pattern is unsafe
                    log::warn!("Skipping unsafe regex pattern: {}", pattern);
                    None
                }
            }
            SecRuleOperator::Eq(value) => {
                if (self.negated && transformed != *value) || (!self.negated && transformed == *value) {
                    Some(transformed.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::Contains(value) => {
                let contains = transformed.contains(value);
                if (self.negated && !contains) || (!self.negated && contains) {
                    Some(value.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::BeginsWith(value) => {
                let starts = transformed.starts_with(value);
                if (self.negated && !starts) || (!self.negated && starts) {
                    Some(value.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::EndsWith(value) => {
                let ends = transformed.ends_with(value);
                if (self.negated && !ends) || (!self.negated && ends) {
                    Some(value.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::Within(values) => {
                let found = values.iter().find(|v| transformed.contains(*v));
                if let Some(v) = found {
                    if !self.negated {
                        Some(v.clone())
                    } else {
                        None
                    }
                } else if self.negated {
                    Some(transformed.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::Gt(value) => {
                if let Ok(num) = transformed.parse::<i64>() {
                    if (self.negated && num <= *value) || (!self.negated && num > *value) {
                        Some(num.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            SecRuleOperator::Lt(value) => {
                if let Ok(num) = transformed.parse::<i64>() {
                    if (self.negated && num >= *value) || (!self.negated && num < *value) {
                        Some(num.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            SecRuleOperator::DetectSQLi => {
                if detect_sqli(&transformed) {
                    Some(transformed.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::DetectXSS => {
                if detect_xss(&transformed) {
                    Some(transformed.clone())
                } else {
                    None
                }
            }
            SecRuleOperator::IpMatch(_ips) => {
                // IP matching would need proper CIDR parsing
                None
            }
            SecRuleOperator::Unconditional => {
                if !self.negated {
                    Some(String::new())
                } else {
                    None
                }
            }
        }
    }

    /// Apply transformations to input
    fn apply_transforms(&self, input: &str, transforms: &[TransformType]) -> String {
        let mut result = input.to_string();

        for transform in transforms {
            result = match transform {
                TransformType::Lowercase => result.to_lowercase(),
                TransformType::UrlDecode => urlencoding::decode(&result)
                    .map(|s| s.into_owned())
                    .unwrap_or(result),
                TransformType::HtmlEntityDecode => html_entity_decode(&result),
                TransformType::CompressWhitespace => compress_whitespace(&result),
                TransformType::RemoveWhitespace => result.chars().filter(|c| !c.is_whitespace()).collect(),
                TransformType::RemoveNulls => result.replace('\0', ""),
                TransformType::ReplaceNulls => result.replace('\0', " "),
                TransformType::Length => result.len().to_string(),
                TransformType::Sha1 | TransformType::Md5 => result, // Not commonly used in detection
                _ => result,
            };
        }

        result
    }
}

// ============================================
// SecRule Parser
// ============================================

/// SecRule parser for ModSecurity rule format
pub struct SecRuleParser;

impl SecRuleParser {
    /// Parse a SecRule string into a SecRule struct
    pub fn parse(rule_text: &str) -> Result<SecRule, String> {
        let rule_text = rule_text.trim();

        // Check for SecRule prefix
        if !rule_text.starts_with("SecRule") {
            return Err("Rule must start with 'SecRule'".to_string());
        }

        // Extract parts: SecRule TARGETS "OPERATOR" "ACTIONS"
        let after_secrule = &rule_text[7..].trim_start();

        // Parse targets (space-separated before the first ")
        let targets_end = after_secrule.find('"')
            .ok_or("Missing operator quotes")?;
        let targets_str = &after_secrule[..targets_end].trim();
        let targets = Self::parse_targets(targets_str)?;

        // Parse operator (between first pair of quotes)
        let remaining = &after_secrule[targets_end + 1..];
        let operator_end = remaining.find('"')
            .ok_or("Missing closing operator quote")?;
        let operator_str = &remaining[..operator_end];
        let (operator, negated) = Self::parse_operator(operator_str)?;

        // Parse actions (between second pair of quotes)
        let remaining = &remaining[operator_end + 1..].trim_start();
        if !remaining.starts_with('"') {
            return Err("Missing actions quotes".to_string());
        }
        let remaining = &remaining[1..];
        let actions_end = remaining.rfind('"')
            .ok_or("Missing closing actions quote")?;
        let actions_str = &remaining[..actions_end];
        let (actions, id, phase, severity, msg, tags) = Self::parse_actions(actions_str)?;

        let is_chained = actions.iter().any(|a| matches!(a, SecRuleAction::Chain));

        Ok(SecRule {
            id,
            phase,
            targets,
            operator,
            negated,
            actions,
            is_chained,
            chain_parent: None,
            severity,
            msg,
            tags,
            file: None,
            line: None,
        })
    }

    /// Parse targets string
    fn parse_targets(targets_str: &str) -> Result<Vec<SecRuleTarget>, String> {
        let mut targets = Vec::new();

        for target in targets_str.split('|') {
            let target = target.trim();
            if target.is_empty() {
                continue;
            }

            let parsed = match target.to_uppercase().as_str() {
                "ARGS" => SecRuleTarget::Args,
                "ARGS_GET" => SecRuleTarget::ArgsGet,
                "ARGS_POST" => SecRuleTarget::ArgsPost,
                "ARGS_NAMES" => SecRuleTarget::ArgsNames,
                "REQUEST_HEADERS" => SecRuleTarget::RequestHeaders,
                "REQUEST_HEADERS_NAMES" => SecRuleTarget::RequestHeadersNames,
                "REQUEST_BODY" => SecRuleTarget::RequestBody,
                "REQUEST_URI" => SecRuleTarget::RequestUri,
                "REQUEST_URI_RAW" => SecRuleTarget::RequestUri,
                "REQUEST_FILENAME" => SecRuleTarget::RequestFilename,
                "REQUEST_BASENAME" => SecRuleTarget::RequestBasename,
                "REQUEST_METHOD" => SecRuleTarget::RequestMethod,
                "REQUEST_PROTOCOL" => SecRuleTarget::RequestProtocol,
                "REMOTE_ADDR" => SecRuleTarget::RemoteAddr,
                "REQUEST_COOKIES" => SecRuleTarget::RequestCookies,
                "REQUEST_COOKIES_NAMES" => SecRuleTarget::RequestCookiesNames,
                "RESPONSE_STATUS" => SecRuleTarget::ResponseStatus,
                "RESPONSE_HEADERS" => SecRuleTarget::ResponseHeaders,
                "RESPONSE_BODY" => SecRuleTarget::ResponseBody,
                _ if target.starts_with("TX:") => {
                    SecRuleTarget::Tx(target[3..].to_string())
                }
                _ if target.starts_with("IP:") => {
                    SecRuleTarget::Ip(target[3..].to_string())
                }
                _ if target.starts_with("&") => {
                    // Count operator - not fully implemented
                    continue;
                }
                _ => {
                    // Unknown target - skip but don't fail
                    continue;
                }
            };
            targets.push(parsed);
        }

        if targets.is_empty() {
            return Err("No valid targets found".to_string());
        }

        Ok(targets)
    }

    /// Parse operator string (e.g., "@rx pattern" or "!@contains value")
    fn parse_operator(op_str: &str) -> Result<(SecRuleOperator, bool), String> {
        let op_str = op_str.trim();
        let (negated, op_str) = if op_str.starts_with('!') {
            (true, op_str[1..].trim_start())
        } else {
            (false, op_str)
        };

        let operator = if op_str.starts_with("@rx ") || op_str.starts_with("@rx\t") {
            SecRuleOperator::Regex(op_str[4..].trim().to_string())
        } else if op_str.starts_with("@eq ") {
            SecRuleOperator::Eq(op_str[4..].trim().to_string())
        } else if op_str.starts_with("@gt ") {
            let val = op_str[4..].trim().parse::<i64>()
                .map_err(|_| "Invalid @gt value")?;
            SecRuleOperator::Gt(val)
        } else if op_str.starts_with("@lt ") {
            let val = op_str[4..].trim().parse::<i64>()
                .map_err(|_| "Invalid @lt value")?;
            SecRuleOperator::Lt(val)
        } else if op_str.starts_with("@contains ") {
            SecRuleOperator::Contains(op_str[10..].trim().to_string())
        } else if op_str.starts_with("@beginsWith ") {
            SecRuleOperator::BeginsWith(op_str[12..].trim().to_string())
        } else if op_str.starts_with("@endsWith ") {
            SecRuleOperator::EndsWith(op_str[10..].trim().to_string())
        } else if op_str.starts_with("@within ") {
            let values: Vec<String> = op_str[8..].split(',')
                .map(|s| s.trim().to_string())
                .collect();
            SecRuleOperator::Within(values)
        } else if op_str.starts_with("@ipMatch ") {
            let ips: Vec<String> = op_str[9..].split(',')
                .map(|s| s.trim().to_string())
                .collect();
            SecRuleOperator::IpMatch(ips)
        } else if op_str == "@detectSQLi" {
            SecRuleOperator::DetectSQLi
        } else if op_str == "@detectXSS" {
            SecRuleOperator::DetectXSS
        } else if op_str == "@unconditionalMatch" || op_str.is_empty() {
            SecRuleOperator::Unconditional
        } else {
            // Default to regex for bare patterns
            SecRuleOperator::Regex(op_str.to_string())
        };

        Ok((operator, negated))
    }

    /// Parse actions string
    fn parse_actions(actions_str: &str) -> Result<(Vec<SecRuleAction>, u32, u8, Severity, String, Vec<String>), String> {
        let mut actions = Vec::new();
        let mut id = 0u32;
        let mut phase = 2u8;
        let mut severity = Severity::Warning;
        let mut msg = String::new();
        let mut tags = Vec::new();

        for action in actions_str.split(',') {
            let action = action.trim();
            if action.is_empty() {
                continue;
            }

            if action.starts_with("id:") {
                id = action[3..].trim().parse().unwrap_or(0);
            } else if action.starts_with("phase:") {
                phase = action[6..].trim().parse().unwrap_or(2);
            } else if action.starts_with("severity:") {
                severity = match action[9..].trim().to_uppercase().as_str() {
                    "CRITICAL" | "0" | "2" => Severity::Critical,
                    "ERROR" | "1" | "3" => Severity::Error,
                    "WARNING" | "4" => Severity::Warning,
                    "NOTICE" | "5" => Severity::Notice,
                    _ => Severity::Info,
                };
            } else if action.starts_with("msg:") {
                // Extract message (may be quoted)
                let msg_part = &action[4..];
                msg = if msg_part.starts_with('\'') && msg_part.ends_with('\'') {
                    msg_part[1..msg_part.len()-1].to_string()
                } else {
                    msg_part.to_string()
                };
            } else if action.starts_with("tag:") {
                let tag = action[4..].trim().trim_matches('\'').to_string();
                tags.push(tag);
            } else if action == "deny" || action == "block" {
                actions.push(SecRuleAction::Deny);
            } else if action == "allow" {
                actions.push(SecRuleAction::Allow);
            } else if action == "pass" {
                actions.push(SecRuleAction::Pass);
            } else if action == "log" {
                actions.push(SecRuleAction::Log);
            } else if action == "nolog" {
                actions.push(SecRuleAction::NoLog);
            } else if action == "chain" {
                actions.push(SecRuleAction::Chain);
            } else if action == "capture" {
                actions.push(SecRuleAction::Capture);
            } else if action.starts_with("status:") {
                let status = action[7..].trim().parse().unwrap_or(403);
                actions.push(SecRuleAction::Status(status));
            } else if action.starts_with("skip:") {
                let skip = action[5..].trim().parse().unwrap_or(0);
                actions.push(SecRuleAction::Skip(skip));
            } else if action.starts_with("skipAfter:") {
                actions.push(SecRuleAction::SkipAfter(action[10..].trim().to_string()));
            } else if action.starts_with("setvar:") {
                let var_expr = &action[7..];
                if let Some((name, value)) = var_expr.split_once('=') {
                    actions.push(SecRuleAction::SetVar(name.to_string(), value.to_string()));
                }
            } else if action.starts_with("t:") {
                let transform = match action[2..].to_lowercase().as_str() {
                    "lowercase" => TransformType::Lowercase,
                    "urldecode" | "urldecodeuni" => TransformType::UrlDecode,
                    "htmlentitydecode" => TransformType::HtmlEntityDecode,
                    "jsdecode" => TransformType::JsDecode,
                    "cssdecode" => TransformType::CssDecode,
                    "base64decode" => TransformType::Base64Decode,
                    "hexdecode" => TransformType::HexDecode,
                    "compresswhitespace" => TransformType::CompressWhitespace,
                    "removewhitespace" => TransformType::RemoveWhitespace,
                    "replacenulls" => TransformType::ReplaceNulls,
                    "removenulls" => TransformType::RemoveNulls,
                    "length" => TransformType::Length,
                    "none" => TransformType::None,
                    _ => continue,
                };
                actions.push(SecRuleAction::Transform(transform));
            }
        }

        Ok((actions, id, phase, severity, msg, tags))
    }
}

// ============================================
// Custom Rule Configuration (YAML/JSON)
// ============================================

/// Custom rule configuration file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRuleConfig {
    /// Configuration metadata
    #[serde(default)]
    pub meta: RuleConfigMeta,
    /// List of custom rules
    pub rules: Vec<CustomRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleConfigMeta {
    /// Configuration version
    #[serde(default)]
    pub version: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Author
    #[serde(default)]
    pub author: String,
}

/// Custom rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    /// Unique rule ID
    pub id: u32,
    /// Rule description
    pub description: String,
    /// Enabled status
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Severity level
    pub severity: String,
    /// Target to inspect
    pub target: String,
    /// Operator type
    pub operator: String,
    /// Pattern or value to match
    pub pattern: String,
    /// Action to take
    pub action: String,
    /// Rule category
    #[serde(default)]
    pub category: String,
    /// Tags for grouping
    #[serde(default)]
    pub tags: Vec<String>,
    /// Priority (higher = checked first)
    #[serde(default)]
    pub priority: i32,
    /// Chain to another rule (AND logic)
    #[serde(default)]
    pub chain: Option<u32>,
}

fn default_true() -> bool {
    true
}

impl CustomRule {
    /// Convert to SecRule
    pub fn to_secrule(&self) -> Result<SecRule, String> {
        let targets = vec![match self.target.to_uppercase().as_str() {
            "URI" | "REQUEST_URI" => SecRuleTarget::RequestUri,
            "ARGS" => SecRuleTarget::Args,
            "BODY" | "REQUEST_BODY" => SecRuleTarget::RequestBody,
            "HEADERS" | "REQUEST_HEADERS" => SecRuleTarget::RequestHeaders,
            "COOKIES" | "REQUEST_COOKIES" => SecRuleTarget::RequestCookies,
            "METHOD" | "REQUEST_METHOD" => SecRuleTarget::RequestMethod,
            "REMOTE_ADDR" => SecRuleTarget::RemoteAddr,
            _ => SecRuleTarget::Args,
        }];

        let operator = match self.operator.to_lowercase().as_str() {
            "regex" | "rx" => SecRuleOperator::Regex(self.pattern.clone()),
            "eq" | "equals" => SecRuleOperator::Eq(self.pattern.clone()),
            "contains" => SecRuleOperator::Contains(self.pattern.clone()),
            "beginswith" | "startswith" => SecRuleOperator::BeginsWith(self.pattern.clone()),
            "endswith" => SecRuleOperator::EndsWith(self.pattern.clone()),
            "detectsqli" | "sqli" => SecRuleOperator::DetectSQLi,
            "detectxss" | "xss" => SecRuleOperator::DetectXSS,
            _ => SecRuleOperator::Regex(self.pattern.clone()),
        };

        let severity = match self.severity.to_lowercase().as_str() {
            "critical" => Severity::Critical,
            "error" => Severity::Error,
            "warning" => Severity::Warning,
            "notice" => Severity::Notice,
            _ => Severity::Info,
        };

        let action = match self.action.to_lowercase().as_str() {
            "deny" | "block" => SecRuleAction::Deny,
            "allow" => SecRuleAction::Allow,
            "log" => SecRuleAction::Log,
            _ => SecRuleAction::Deny,
        };

        Ok(SecRule {
            id: self.id,
            phase: 2,
            targets,
            operator,
            negated: false,
            actions: vec![action],
            is_chained: self.chain.is_some(),
            chain_parent: self.chain,
            severity,
            msg: self.description.clone(),
            tags: self.tags.clone(),
            file: None,
            line: None,
        })
    }
}

// ============================================
// ML Anomaly Scoring
// ============================================

/// Request features for anomaly detection
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestFeatures {
    /// Total request size (headers + body)
    pub request_size: usize,
    /// Number of parameters (query + body)
    pub param_count: usize,
    /// Average parameter value length
    pub avg_param_length: f64,
    /// Maximum parameter value length
    pub max_param_length: usize,
    /// Character entropy of body
    pub body_entropy: f64,
    /// Number of special characters
    pub special_char_count: usize,
    /// Ratio of special chars to total
    pub special_char_ratio: f64,
    /// SQL keyword density
    pub sql_keyword_density: f64,
    /// XSS keyword density
    pub xss_keyword_density: f64,
    /// Path depth (number of /)
    pub path_depth: usize,
    /// Query string length
    pub query_length: usize,
    /// Content-Type present
    pub has_content_type: bool,
    /// Content-Length present
    pub has_content_length: bool,
    /// Number of cookies
    pub cookie_count: usize,
    /// Number of headers
    pub header_count: usize,
}

/// Anomaly scoring result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyScore {
    /// Total anomaly score (0-100)
    pub score: u8,
    /// Individual component scores
    pub components: HashMap<String, f64>,
    /// Anomaly reasons
    pub anomalies: Vec<String>,
    /// Is this request anomalous?
    pub is_anomalous: bool,
}

/// ML-based anomaly detector for requests
pub struct AnomalyDetector {
    /// Baseline statistics for normal requests
    baseline: RequestBaseline,
    /// Anomaly threshold (0-100)
    threshold: u8,
}

/// Baseline statistics from training data
#[derive(Debug, Clone, Default)]
pub struct RequestBaseline {
    pub avg_request_size: f64,
    pub std_request_size: f64,
    pub avg_param_count: f64,
    pub std_param_count: f64,
    pub avg_body_entropy: f64,
    pub std_body_entropy: f64,
    pub avg_special_char_ratio: f64,
    pub std_special_char_ratio: f64,
    pub avg_path_depth: f64,
    pub std_path_depth: f64,
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl AnomalyDetector {
    /// Create new anomaly detector with default baseline
    pub fn new() -> Self {
        Self {
            baseline: RequestBaseline {
                avg_request_size: 2000.0,
                std_request_size: 1500.0,
                avg_param_count: 5.0,
                std_param_count: 3.0,
                avg_body_entropy: 4.5,
                std_body_entropy: 0.5,
                avg_special_char_ratio: 0.05,
                std_special_char_ratio: 0.03,
                avg_path_depth: 3.0,
                std_path_depth: 1.5,
            },
            threshold: 70,
        }
    }

    /// Extract features from an HTTP request
    pub fn extract_features(
        &self,
        method: &str,
        uri: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> RequestFeatures {
        let mut features = RequestFeatures::default();

        // Parse query parameters
        let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
        features.path_depth = path.matches('/').count();
        features.query_length = query.len();

        // Count query parameters
        let query_params: Vec<&str> = query.split('&').filter(|s| !s.is_empty()).collect();
        features.param_count = query_params.len();

        // Header features
        features.header_count = headers.len();
        features.has_content_type = headers.iter().any(|(n, _)| n.to_lowercase() == "content-type");
        features.has_content_length = headers.iter().any(|(n, _)| n.to_lowercase() == "content-length");
        features.cookie_count = headers.iter()
            .filter(|(n, _)| n.to_lowercase() == "cookie")
            .map(|(_, v)| v.split(';').count())
            .sum();

        // Request size
        let header_size: usize = headers.iter()
            .map(|(n, v)| n.len() + v.len() + 4) // ": " and "\r\n"
            .sum();
        let body_size = body.map(|b| b.len()).unwrap_or(0);
        features.request_size = method.len() + uri.len() + header_size + body_size;

        // Body analysis
        if let Some(body_bytes) = body {
            if let Ok(body_str) = std::str::from_utf8(body_bytes) {
                // Parse body parameters
                let body_params: Vec<&str> = body_str.split('&').filter(|s| !s.is_empty()).collect();
                features.param_count += body_params.len();

                // Parameter lengths
                let mut param_lengths: Vec<usize> = Vec::new();
                for param in query_params.iter().chain(body_params.iter()) {
                    if let Some((_, value)) = param.split_once('=') {
                        param_lengths.push(value.len());
                    }
                }
                if !param_lengths.is_empty() {
                    features.avg_param_length = param_lengths.iter().sum::<usize>() as f64 / param_lengths.len() as f64;
                    features.max_param_length = *param_lengths.iter().max().unwrap_or(&0);
                }

                // Entropy
                features.body_entropy = calculate_entropy(body_str);

                // Special characters
                let special_chars = body_str.chars()
                    .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
                    .count();
                features.special_char_count = special_chars;
                features.special_char_ratio = if body_str.is_empty() {
                    0.0
                } else {
                    special_chars as f64 / body_str.len() as f64
                };

                // SQL keyword density
                features.sql_keyword_density = calculate_keyword_density(body_str, &SQL_KEYWORDS);

                // XSS keyword density
                features.xss_keyword_density = calculate_keyword_density(body_str, &XSS_KEYWORDS);
            }
        }

        // Also check URI for keywords
        let uri_lower = uri.to_lowercase();
        features.sql_keyword_density = features.sql_keyword_density.max(
            calculate_keyword_density(&uri_lower, &SQL_KEYWORDS)
        );
        features.xss_keyword_density = features.xss_keyword_density.max(
            calculate_keyword_density(&uri_lower, &XSS_KEYWORDS)
        );

        features
    }

    /// Calculate anomaly score for request features
    pub fn score(&self, features: &RequestFeatures) -> AnomalyScore {
        let mut components = HashMap::new();
        let mut anomalies = Vec::new();
        let mut total_score: f64 = 0.0;

        // Request size anomaly
        let size_z = z_score(features.request_size as f64, self.baseline.avg_request_size, self.baseline.std_request_size);
        let size_score = sigmoid_scale(size_z.abs());
        components.insert("request_size".to_string(), size_score);
        if size_z > 3.0 {
            anomalies.push("unusually_large_request".to_string());
            total_score += size_score * 15.0;
        }

        // Parameter count anomaly
        let param_z = z_score(features.param_count as f64, self.baseline.avg_param_count, self.baseline.std_param_count);
        let param_score = sigmoid_scale(param_z.abs());
        components.insert("param_count".to_string(), param_score);
        if param_z > 3.0 {
            anomalies.push("excessive_parameters".to_string());
            total_score += param_score * 10.0;
        }

        // Entropy anomaly (low entropy can indicate encoded payload)
        if features.body_entropy > 0.0 {
            let entropy_z = z_score(features.body_entropy, self.baseline.avg_body_entropy, self.baseline.std_body_entropy);
            let entropy_score = sigmoid_scale(entropy_z.abs());
            components.insert("body_entropy".to_string(), entropy_score);
            if entropy_z.abs() > 2.5 {
                if features.body_entropy < 3.0 {
                    anomalies.push("low_entropy_body".to_string());
                } else if features.body_entropy > 6.0 {
                    anomalies.push("high_entropy_body".to_string());
                }
                total_score += entropy_score * 10.0;
            }
        }

        // Special character ratio
        let special_z = z_score(features.special_char_ratio, self.baseline.avg_special_char_ratio, self.baseline.std_special_char_ratio);
        let special_score = sigmoid_scale(special_z.abs());
        components.insert("special_char_ratio".to_string(), special_score);
        if special_z > 3.0 {
            anomalies.push("excessive_special_chars".to_string());
            total_score += special_score * 15.0;
        }

        // SQL keyword density
        if features.sql_keyword_density > 0.1 {
            let sql_score = (features.sql_keyword_density * 100.0).min(100.0);
            components.insert("sql_keywords".to_string(), sql_score);
            anomalies.push("high_sql_keyword_density".to_string());
            total_score += sql_score * 0.2;
        }

        // XSS keyword density
        if features.xss_keyword_density > 0.05 {
            let xss_score = (features.xss_keyword_density * 200.0).min(100.0);
            components.insert("xss_keywords".to_string(), xss_score);
            anomalies.push("high_xss_keyword_density".to_string());
            total_score += xss_score * 0.2;
        }

        // Max parameter length
        if features.max_param_length > 1000 {
            let length_score = ((features.max_param_length as f64 - 1000.0) / 100.0).min(100.0);
            components.insert("max_param_length".to_string(), length_score);
            anomalies.push("extremely_long_parameter".to_string());
            total_score += length_score * 0.1;
        }

        // Missing standard headers
        if !features.has_content_type && features.request_size > 100 {
            components.insert("missing_content_type".to_string(), 20.0);
            total_score += 5.0;
        }

        let final_score = (total_score.min(100.0)) as u8;

        AnomalyScore {
            score: final_score,
            components,
            anomalies,
            is_anomalous: final_score >= self.threshold,
        }
    }
}

// ============================================
// Enhanced WAF Engine
// ============================================

/// Enhanced WAF with CRS support and anomaly scoring
pub struct EnhancedWaf {
    /// Parsed SecRules
    rules: Vec<SecRule>,
    /// Custom rules from YAML
    custom_rules: Vec<SecRule>,
    /// Anomaly detector
    anomaly_detector: AnomalyDetector,
    /// Rule skip markers
    skip_after: HashMap<String, bool>,
    /// Transaction variables
    tx_vars: HashMap<String, String>,
    /// Anomaly score threshold for blocking
    anomaly_threshold: u8,
    /// Enable anomaly scoring
    anomaly_enabled: bool,
}

impl Default for EnhancedWaf {
    fn default() -> Self {
        Self::new()
    }
}

impl EnhancedWaf {
    /// Create new enhanced WAF
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            custom_rules: Vec::new(),
            anomaly_detector: AnomalyDetector::new(),
            skip_after: HashMap::new(),
            tx_vars: HashMap::new(),
            anomaly_threshold: 70,
            anomaly_enabled: true,
        }
    }

    /// Load rules from SecRule text
    pub fn load_secrules(&mut self, rules_text: &str) -> Result<usize, String> {
        let mut loaded = 0;
        let mut current_rule = String::new();

        for line in rules_text.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handle multi-line rules (ending with \)
            if line.ends_with('\\') {
                current_rule.push_str(&line[..line.len()-1]);
                current_rule.push(' ');
                continue;
            }

            current_rule.push_str(line);

            if current_rule.starts_with("SecRule") {
                match SecRuleParser::parse(&current_rule) {
                    Ok(rule) => {
                        self.rules.push(rule);
                        loaded += 1;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse rule: {} - {}", e, &current_rule[..50.min(current_rule.len())]);
                    }
                }
            }

            current_rule.clear();
        }

        Ok(loaded)
    }

    /// Load custom rules from YAML
    pub fn load_custom_rules(&mut self, yaml_content: &str) -> Result<usize, String> {
        let config: CustomRuleConfig = serde_yaml::from_str(yaml_content)
            .map_err(|e| format!("YAML parse error: {}", e))?;

        let mut loaded = 0;
        for rule in config.rules {
            if rule.enabled {
                match rule.to_secrule() {
                    Ok(secrule) => {
                        self.custom_rules.push(secrule);
                        loaded += 1;
                    }
                    Err(e) => {
                        log::warn!("Failed to convert custom rule {}: {}", rule.id, e);
                    }
                }
            }
        }

        Ok(loaded)
    }

    /// Add OWASP CRS base rules (simplified subset)
    pub fn add_crs_base_rules(&mut self) -> usize {
        let crs_rules = get_crs_base_rules();
        let count = crs_rules.len();
        self.rules.extend(crs_rules);
        count
    }

    /// Analyze request with enhanced WAF
    pub fn analyze(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
        remote_addr: &str,
    ) -> EnhancedWafResult {
        let mut matches = Vec::new();
        let mut blocked = false;
        let mut block_rule_id = None;

        // Reset transaction state
        self.tx_vars.clear();
        self.skip_after.clear();

        // Get request data
        let body_str = body.and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("");
        let (path, query) = uri.split_once('?').unwrap_or((uri, ""));

        // Collect all targets
        let targets_data: HashMap<SecRuleTarget, Vec<String>> = self.collect_targets(
            method, uri, path, query, headers, body_str, remote_addr
        );

        // Check custom rules first (higher priority)
        for rule in &self.custom_rules {
            if let Some(rule_match) = self.check_rule(rule, &targets_data) {
                matches.push(rule_match.clone());
                if rule.actions.iter().any(|a| matches!(a, SecRuleAction::Deny)) {
                    blocked = true;
                    block_rule_id = Some(rule.id);
                }
            }
        }

        // Check SecRules
        for rule in &self.rules {
            if let Some(rule_match) = self.check_rule(rule, &targets_data) {
                matches.push(rule_match.clone());
                if rule.actions.iter().any(|a| matches!(a, SecRuleAction::Deny)) {
                    blocked = true;
                    block_rule_id = Some(rule.id);
                }
            }
        }

        // Calculate anomaly score
        let anomaly_score = if self.anomaly_enabled {
            let features = self.anomaly_detector.extract_features(method, uri, headers, body);
            Some(self.anomaly_detector.score(&features))
        } else {
            None
        };

        // Check if anomaly score triggers block
        if let Some(ref score) = anomaly_score {
            if score.is_anomalous && !blocked {
                blocked = true;
            }
        }

        EnhancedWafResult {
            blocked,
            block_rule_id,
            matches,
            anomaly_score,
            action: if blocked { WafAction::Block } else { WafAction::Allow },
        }
    }

    /// Collect all target data for rule matching
    fn collect_targets(
        &self,
        method: &str,
        uri: &str,
        path: &str,
        query: &str,
        headers: &[(String, String)],
        body: &str,
        remote_addr: &str,
    ) -> HashMap<SecRuleTarget, Vec<String>> {
        let mut targets: HashMap<SecRuleTarget, Vec<String>> = HashMap::new();

        // Method
        targets.insert(SecRuleTarget::RequestMethod, vec![method.to_string()]);

        // URI
        targets.insert(SecRuleTarget::RequestUri, vec![uri.to_string()]);
        targets.insert(SecRuleTarget::RequestUriPath, vec![path.to_string()]);
        targets.insert(SecRuleTarget::RequestFilename, vec![path.to_string()]);

        // Query parameters
        let mut args = Vec::new();
        for param in query.split('&').filter(|s| !s.is_empty()) {
            if let Some((_, value)) = param.split_once('=') {
                args.push(value.to_string());
            }
        }
        targets.insert(SecRuleTarget::ArgsGet, args.clone());

        // Body parameters
        for param in body.split('&').filter(|s| !s.is_empty()) {
            if let Some((_, value)) = param.split_once('=') {
                args.push(value.to_string());
            }
        }
        targets.insert(SecRuleTarget::Args, args);
        targets.insert(SecRuleTarget::RequestBody, vec![body.to_string()]);

        // Headers
        let header_values: Vec<String> = headers.iter().map(|(_, v)| v.clone()).collect();
        targets.insert(SecRuleTarget::RequestHeaders, header_values);

        // Cookies
        let cookies: Vec<String> = headers.iter()
            .filter(|(n, _)| n.to_lowercase() == "cookie")
            .flat_map(|(_, v)| v.split(';').map(|s| s.trim().to_string()))
            .collect();
        targets.insert(SecRuleTarget::RequestCookies, cookies);

        // Remote address
        targets.insert(SecRuleTarget::RemoteAddr, vec![remote_addr.to_string()]);

        targets
    }

    /// Check if a rule matches the request
    fn check_rule(
        &self,
        rule: &SecRule,
        targets_data: &HashMap<SecRuleTarget, Vec<String>>,
    ) -> Option<RuleMatch> {
        // Get transforms from rule actions
        let transforms: Vec<TransformType> = rule.actions.iter()
            .filter_map(|a| {
                if let SecRuleAction::Transform(t) = a {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect();

        // Check each target
        for target in &rule.targets {
            if let Some(values) = targets_data.get(target) {
                for value in values {
                    if let Some(matched) = rule.matches(value, &transforms) {
                        return Some(RuleMatch {
                            rule_id: rule.id,
                            rule_description: rule.msg.clone(),
                            severity: rule.severity,
                            category: rule.tags.first().cloned().unwrap_or_else(|| "unknown".to_string()),
                            matched_value: matched,
                            location: format!("{:?}", target),
                        });
                    }
                }
            }
        }

        None
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.rules.len() + self.custom_rules.len()
    }

    /// Enable/disable anomaly detection
    pub fn set_anomaly_enabled(&mut self, enabled: bool) {
        self.anomaly_enabled = enabled;
    }

    /// Set anomaly threshold
    pub fn set_anomaly_threshold(&mut self, threshold: u8) {
        self.anomaly_threshold = threshold;
    }
}

/// Enhanced WAF analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedWafResult {
    /// Was the request blocked?
    pub blocked: bool,
    /// Which rule caused the block (if any)
    pub block_rule_id: Option<u32>,
    /// All rule matches
    pub matches: Vec<RuleMatch>,
    /// Anomaly score (if enabled)
    pub anomaly_score: Option<AnomalyScore>,
    /// Final action
    pub action: WafAction,
}

// ============================================
// Helper Functions
// ============================================

/// Calculate Shannon entropy of a string
fn calculate_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }

    let mut freq: HashMap<char, usize> = HashMap::new();
    for c in s.chars() {
        *freq.entry(c).or_insert(0) += 1;
    }

    let len = s.len() as f64;
    let mut entropy = 0.0;
    for &count in freq.values() {
        let p = count as f64 / len;
        entropy -= p * p.log2();
    }

    entropy
}

/// Calculate keyword density
fn calculate_keyword_density(text: &str, keywords: &[&str]) -> f64 {
    if text.is_empty() {
        return 0.0;
    }

    let text_lower = text.to_lowercase();
    let mut matches = 0;

    for keyword in keywords {
        matches += text_lower.matches(*keyword).count();
    }

    matches as f64 / text.len() as f64
}

/// Z-score calculation
fn z_score(value: f64, mean: f64, std: f64) -> f64 {
    if std == 0.0 {
        return 0.0;
    }
    (value - mean) / std
}

/// Sigmoid scaling for anomaly scores
fn sigmoid_scale(z: f64) -> f64 {
    100.0 / (1.0 + (-z + 2.0).exp())
}

/// Simple SQLi detection
fn detect_sqli(input: &str) -> bool {
    let input_lower = input.to_lowercase();
    let patterns = [
        r"union\s+select",
        r"select\s+.*\s+from",
        r"insert\s+into",
        r"delete\s+from",
        r"drop\s+table",
        r"'\s*(or|and)\s*'",
        r"--\s*$",
        r"/\*.*\*/",
        r"exec\s*\(",
        r"xp_cmdshell",
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(&input_lower) {
                return true;
            }
        }
    }

    false
}

/// Simple XSS detection
fn detect_xss(input: &str) -> bool {
    let input_lower = input.to_lowercase();
    let patterns = [
        r"<script[^>]*>",
        r"javascript:",
        r"on\w+\s*=",
        r"<iframe",
        r"<object",
        r"<embed",
        r"expression\s*\(",
        r"document\.",
        r"window\.",
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(&input_lower) {
                return true;
            }
        }
    }

    false
}

/// HTML entity decode (basic)
fn html_entity_decode(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&#x3c;", "<")
        .replace("&#x3e;", ">")
}

/// Compress whitespace
fn compress_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_ws = false;

    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                result.push(' ');
                prev_ws = true;
            }
        } else {
            result.push(c);
            prev_ws = false;
        }
    }

    result
}

/// SQL keywords for density calculation
static SQL_KEYWORDS: &[&str] = &[
    "select", "union", "insert", "update", "delete", "drop", "table",
    "from", "where", "and", "or", "exec", "execute", "xp_", "sp_",
    "declare", "cast", "convert", "char(", "nchar(", "varchar",
];

/// XSS keywords for density calculation
static XSS_KEYWORDS: &[&str] = &[
    "script", "javascript", "onerror", "onload", "onclick", "onmouseover",
    "onfocus", "onblur", "document.", "window.", "alert(", "eval(",
    "settimeout", "setinterval", "innerhtml", "outerhtml",
];

// ============================================
// OWASP CRS Base Rules (Simplified Subset)
// ============================================

/// Get OWASP CRS base rules (simplified subset for performance)
fn get_crs_base_rules() -> Vec<SecRule> {
    vec![
        // SQL Injection rules
        SecRule {
            id: 942100,
            phase: 2,
            targets: vec![SecRuleTarget::Args, SecRuleTarget::RequestBody],
            operator: SecRuleOperator::DetectSQLi,
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "SQL Injection Attack Detected".to_string(),
            tags: vec!["sqli".to_string(), "owasp-crs".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 942110,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)(union\s+(all\s+)?select|select\s+.*\s+from|insert\s+into|delete\s+from|update\s+.*\s+set)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "SQL Injection: Common SQL Keywords".to_string(),
            tags: vec!["sqli".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 942120,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)('(\s*)(or|and)(\s*)'|'\s*--|;\s*--|\s+--\s*$)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "SQL Injection: Comment-based".to_string(),
            tags: vec!["sqli".to_string()],
            file: None,
            line: None,
        },

        // XSS rules
        SecRule {
            id: 941100,
            phase: 2,
            targets: vec![SecRuleTarget::Args, SecRuleTarget::RequestBody],
            operator: SecRuleOperator::DetectXSS,
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "XSS Attack Detected".to_string(),
            tags: vec!["xss".to_string(), "owasp-crs".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 941110,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)<script[^>]*>".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "XSS: Script Tag Injection".to_string(),
            tags: vec!["xss".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 941120,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)on(error|load|click|mouseover|focus|blur)\s*=".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "XSS: Event Handler Injection".to_string(),
            tags: vec!["xss".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 941130,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)javascript:".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Error,
            msg: "XSS: JavaScript Protocol".to_string(),
            tags: vec!["xss".to_string()],
            file: None,
            line: None,
        },

        // Path Traversal
        SecRule {
            id: 930100,
            phase: 2,
            targets: vec![SecRuleTarget::RequestUri, SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(\.\.\/|\.\.\\|%2e%2e%2f|%2e%2e\/|\.\.%2f|%2e%2e%5c)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Path Traversal Attack".to_string(),
            tags: vec!["path-traversal".to_string(), "lfi".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 930110,
            phase: 2,
            targets: vec![SecRuleTarget::RequestUri, SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)(\/etc\/passwd|\/etc\/shadow|\/windows\/system32|boot\.ini)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Path Traversal: Sensitive File Access".to_string(),
            tags: vec!["path-traversal".to_string()],
            file: None,
            line: None,
        },

        // Remote Code Execution
        SecRule {
            id: 932100,
            phase: 2,
            targets: vec![SecRuleTarget::Args, SecRuleTarget::RequestBody],
            operator: SecRuleOperator::Regex(r"(?i)(;\s*(ls|cat|wget|curl|bash|sh|python|perl|ruby)|&&\s*(ls|cat)|`[^`]+`|\$\([^)]+\)|\|[^|])".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Remote Code Execution: Command Injection".to_string(),
            tags: vec!["rce".to_string()],
            file: None,
            line: None,
        },
        SecRule {
            id: 932110,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)(cmd\.exe|powershell|wscript|cscript)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Remote Code Execution: Windows Commands".to_string(),
            tags: vec!["rce".to_string()],
            file: None,
            line: None,
        },

        // PHP Injection
        SecRule {
            id: 933100,
            phase: 2,
            targets: vec![SecRuleTarget::Args, SecRuleTarget::RequestBody],
            operator: SecRuleOperator::Regex(r"(?i)(<\?php|<\?=|eval\s*\(|base64_decode\s*\(|assert\s*\(|system\s*\(|passthru\s*\(|shell_exec\s*\()".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "PHP Code Injection".to_string(),
            tags: vec!["php".to_string(), "rce".to_string()],
            file: None,
            line: None,
        },

        // Java Injection
        SecRule {
            id: 944100,
            phase: 2,
            targets: vec![SecRuleTarget::Args, SecRuleTarget::RequestBody],
            operator: SecRuleOperator::Regex(r"(?i)(java\.lang\.|Runtime\.getRuntime|ProcessBuilder|ScriptEngine|javax\.script)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Java Code Injection".to_string(),
            tags: vec!["java".to_string(), "rce".to_string()],
            file: None,
            line: None,
        },

        // Protocol Attack
        SecRule {
            id: 920100,
            phase: 1,
            targets: vec![SecRuleTarget::RequestMethod],
            operator: SecRuleOperator::Regex(r"(?i)^(TRACE|TRACK|DEBUG|CONNECT)$".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Warning,
            msg: "HTTP Protocol: Disallowed Method".to_string(),
            tags: vec!["protocol".to_string()],
            file: None,
            line: None,
        },

        // Scanner Detection
        SecRule {
            id: 913100,
            phase: 1,
            targets: vec![SecRuleTarget::RequestHeaders],
            operator: SecRuleOperator::Regex(r"(?i)(nikto|nmap|masscan|sqlmap|dirbuster|gobuster|wfuzz|acunetix|nessus|openvas)".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Error,
            msg: "Security Scanner Detected".to_string(),
            tags: vec!["scanner".to_string()],
            file: None,
            line: None,
        },

        // Session Fixation
        SecRule {
            id: 943100,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Regex(r"(?i)(PHPSESSID|JSESSIONID|ASPSESSIONID|session_id)\s*=".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny, SecRuleAction::Log],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Warning,
            msg: "Possible Session Fixation Attack".to_string(),
            tags: vec!["session-fixation".to_string()],
            file: None,
            line: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secrule_parser_basic() {
        let rule_text = r#"SecRule ARGS "@rx union.*select" "id:1001,phase:2,deny,status:403,msg:'SQL Injection Detected',tag:'sqli',severity:CRITICAL""#;

        let rule = SecRuleParser::parse(rule_text).unwrap();

        assert_eq!(rule.id, 1001);
        assert_eq!(rule.phase, 2);
        assert_eq!(rule.severity, Severity::Critical);
        assert!(matches!(&rule.operator, SecRuleOperator::Regex(p) if p.contains("union")));
        assert!(rule.actions.iter().any(|a| matches!(a, SecRuleAction::Deny)));
    }

    #[test]
    fn test_secrule_parser_multiple_targets() {
        let rule_text = r#"SecRule ARGS|REQUEST_BODY "@contains password" "id:1002,phase:2,log,pass""#;

        let rule = SecRuleParser::parse(rule_text).unwrap();

        assert_eq!(rule.targets.len(), 2);
        assert!(rule.targets.contains(&SecRuleTarget::Args));
        assert!(rule.targets.contains(&SecRuleTarget::RequestBody));
    }

    #[test]
    fn test_secrule_matching() {
        let rule = SecRule {
            id: 1,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Contains("union".to_string()),
            negated: false,
            actions: vec![SecRuleAction::Deny],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Test".to_string(),
            tags: vec![],
            file: None,
            line: None,
        };

        assert!(rule.matches("SELECT * union ALL", &[]).is_some());
        assert!(rule.matches("normal query", &[]).is_none());
    }

    #[test]
    fn test_secrule_transforms() {
        let rule = SecRule {
            id: 1,
            phase: 2,
            targets: vec![SecRuleTarget::Args],
            operator: SecRuleOperator::Contains("script".to_string()),
            negated: false,
            actions: vec![],
            is_chained: false,
            chain_parent: None,
            severity: Severity::Critical,
            msg: "Test".to_string(),
            tags: vec![],
            file: None,
            line: None,
        };

        // With lowercase transform
        assert!(rule.matches("SCRIPT TAG", &[TransformType::Lowercase]).is_some());
    }

    #[test]
    fn test_custom_rule_config_yaml() {
        let yaml = r#"
meta:
  version: "1.0"
  description: "Test rules"
rules:
  - id: 100001
    description: "Block API key in URL"
    severity: critical
    target: REQUEST_URI
    operator: contains
    pattern: "api_key="
    action: deny
    category: security
    tags:
      - api
      - sensitive
"#;

        let config: CustomRuleConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].id, 100001);
        assert_eq!(config.rules[0].pattern, "api_key=");
    }

    #[test]
    fn test_custom_rule_to_secrule() {
        let custom = CustomRule {
            id: 100001,
            description: "Test rule".to_string(),
            enabled: true,
            severity: "critical".to_string(),
            target: "ARGS".to_string(),
            operator: "regex".to_string(),
            pattern: "test.*pattern".to_string(),
            action: "deny".to_string(),
            category: "test".to_string(),
            tags: vec!["test".to_string()],
            priority: 0,
            chain: None,
        };

        let secrule = custom.to_secrule().unwrap();

        assert_eq!(secrule.id, 100001);
        assert_eq!(secrule.severity, Severity::Critical);
        assert!(matches!(&secrule.operator, SecRuleOperator::Regex(_)));
    }

    #[test]
    fn test_anomaly_detector_features() {
        let detector = AnomalyDetector::new();

        let features = detector.extract_features(
            "POST",
            "/api/users?id=1&name=test",
            &[
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Content-Length".to_string(), "100".to_string()),
            ],
            Some(b"username=admin&password=secret"),
        );

        assert!(features.param_count >= 2);
        assert!(features.has_content_type);
        assert!(features.has_content_length);
        assert!(features.path_depth >= 1);
    }

    #[test]
    fn test_anomaly_detector_score() {
        let detector = AnomalyDetector::new();

        // Normal request
        let normal_features = RequestFeatures {
            request_size: 500,
            param_count: 3,
            body_entropy: 4.5,
            special_char_ratio: 0.05,
            ..Default::default()
        };

        let normal_score = detector.score(&normal_features);
        assert!(normal_score.score < 50, "Normal request should have low anomaly score");

        // Suspicious request (high SQL keyword density)
        let suspicious_features = RequestFeatures {
            request_size: 500,
            param_count: 3,
            sql_keyword_density: 0.3,
            special_char_ratio: 0.15,
            ..Default::default()
        };

        let suspicious_score = detector.score(&suspicious_features);
        assert!(suspicious_score.score > normal_score.score, "Suspicious request should have higher score");
    }

    #[test]
    fn test_enhanced_waf_basic() {
        let mut waf = EnhancedWaf::new();

        // Add CRS rules
        let count = waf.add_crs_base_rules();
        assert!(count > 10, "Should have at least 10 CRS base rules");

        // Test SQLi detection
        let result = waf.analyze(
            "GET",
            "/search?q=' OR '1'='1",
            &[],
            None,
            "127.0.0.1",
        );

        assert!(result.blocked || !result.matches.is_empty());
    }

    #[test]
    fn test_enhanced_waf_xss() {
        let mut waf = EnhancedWaf::new();
        waf.add_crs_base_rules();

        let result = waf.analyze(
            "GET",
            "/page?content=<script>alert('xss')</script>",
            &[],
            None,
            "127.0.0.1",
        );

        assert!(result.blocked || !result.matches.is_empty());
        assert!(result.matches.iter().any(|m| m.category.contains("xss")));
    }

    #[test]
    fn test_enhanced_waf_path_traversal() {
        let mut waf = EnhancedWaf::new();
        waf.add_crs_base_rules();

        let result = waf.analyze(
            "GET",
            "/files?path=../../etc/passwd",
            &[],
            None,
            "127.0.0.1",
        );

        assert!(result.blocked || !result.matches.is_empty());
    }

    #[test]
    fn test_enhanced_waf_rce() {
        let mut waf = EnhancedWaf::new();
        waf.add_crs_base_rules();

        let result = waf.analyze(
            "POST",
            "/exec",
            &[("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string())],
            Some(b"cmd=; cat /etc/passwd"),
            "127.0.0.1",
        );

        assert!(result.blocked || !result.matches.is_empty());
    }

    #[test]
    fn test_enhanced_waf_clean_request() {
        let mut waf = EnhancedWaf::new();
        waf.add_crs_base_rules();
        waf.set_anomaly_enabled(false); // Disable anomaly to test rules only

        let result = waf.analyze(
            "GET",
            "/api/users/123",
            &[("User-Agent".to_string(), "Mozilla/5.0".to_string())],
            None,
            "127.0.0.1",
        );

        assert!(!result.blocked, "Clean request should not be blocked");
        assert!(result.matches.is_empty(), "Clean request should have no matches");
    }

    #[test]
    fn test_entropy_calculation() {
        // Low entropy (repeated chars)
        let low_entropy = calculate_entropy("aaaaaaaaaa");
        assert!(low_entropy < 1.0);

        // High entropy (varied chars)
        let high_entropy = calculate_entropy("abcdefghij");
        assert!(high_entropy > low_entropy);
    }

    #[test]
    fn test_detect_sqli() {
        assert!(detect_sqli("' OR '1'='1"));
        assert!(detect_sqli("UNION SELECT password FROM users"));
        assert!(detect_sqli("1; DROP TABLE users--"));
        assert!(!detect_sqli("normal search query"));
    }

    #[test]
    fn test_detect_xss() {
        assert!(detect_xss("<script>alert(1)</script>"));
        assert!(detect_xss("javascript:alert(1)"));
        assert!(detect_xss("<img onerror=alert(1)>"));
        assert!(!detect_xss("normal text content"));
    }

    #[test]
    fn test_html_entity_decode() {
        assert_eq!(html_entity_decode("&lt;script&gt;"), "<script>");
        assert_eq!(html_entity_decode("&amp;&amp;"), "&&");
    }

    // SECURITY FIX: Tests for safe regex compilation
    #[test]
    fn test_safe_regex_valid_pattern() {
        // Normal patterns should compile successfully
        assert!(compile_safe_regex(r"union\s+select").is_ok());
        assert!(compile_safe_regex(r"<script[^>]*>").is_ok());
        assert!(compile_safe_regex(r"[a-z]+").is_ok());
    }

    #[test]
    fn test_safe_regex_rejects_oversized_pattern() {
        // Pattern exceeding max length should be rejected
        let oversized = "a".repeat(MAX_REGEX_PATTERN_LENGTH + 1);
        let result = compile_safe_regex(&oversized);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds maximum length"));
    }

    #[test]
    fn test_safe_regex_rejects_dangerous_patterns() {
        // Nested quantifiers that cause catastrophic backtracking
        assert!(compile_safe_regex(r"(\w+)+").is_err());
        assert!(compile_safe_regex(r"(.+)*").is_err());
        assert!(compile_safe_regex(r"(.*)*").is_err());
    }
}
