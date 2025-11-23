/// Sprint 16: Route-based Dispatch Configuration
///
/// This module provides configuration-driven routing for Wasm modules.
/// Routes map HTTP request patterns to sequences of Wasm modules (WAF, edge functions, etc.)
/// enabling flexible, GitOps-managed edge logic without code changes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Route matching pattern types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "pattern", rename_all = "lowercase")]
pub enum RoutePattern {
    /// Exact path match (e.g., "/api/users")
    Exact(String),
    /// Prefix match with wildcard (e.g., "/api/*")
    Prefix(String),
    /// Regex pattern match (e.g., "^/api/v[0-9]+/.*")
    Regex(String),
}

impl RoutePattern {
    /// Check if a given path matches this pattern
    pub fn matches(&self, path: &str) -> bool {
        match self {
            RoutePattern::Exact(pattern) => path == pattern,
            RoutePattern::Prefix(prefix) => {
                let normalized_prefix = prefix.trim_end_matches('*').trim_end_matches('/');
                path == normalized_prefix || path.starts_with(&format!("{}/", normalized_prefix))
            }
            RoutePattern::Regex(pattern) => {
                // For now, use simple glob-style matching
                // In production, compile and cache regex::Regex
                if let Ok(re) = regex::Regex::new(pattern) {
                    re.is_match(path)
                } else {
                    false
                }
            }
        }
    }
}

/// Wasm module reference in route configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmModuleRef {
    /// Module type (waf, edge_function, etc.)
    #[serde(rename = "type")]
    pub module_type: String,

    /// Module identifier (name or IPFS CID)
    pub module_id: String,

    /// Optional: IPFS CID for fetching module (if not pre-loaded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipfs_cid: Option<String>,

    /// Optional: Required Ed25519 public key for signature verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_public_key: Option<String>,
}

/// HTTP method matching
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MethodMatcher {
    /// Match all methods
    All(String), // "*"

    /// Match specific method
    Single(String), // "GET", "POST", etc.

    /// Match any of the listed methods
    Multiple(Vec<String>), // ["GET", "POST"]
}

impl MethodMatcher {
    /// Check if a given HTTP method matches this matcher
    pub fn matches(&self, method: &str) -> bool {
        match self {
            MethodMatcher::All(s) if s == "*" => true,
            MethodMatcher::Single(m) => m.eq_ignore_ascii_case(method),
            MethodMatcher::Multiple(methods) => {
                methods.iter().any(|m| m.eq_ignore_ascii_case(method))
            }
            _ => false,
        }
    }
}

impl Default for MethodMatcher {
    fn default() -> Self {
        MethodMatcher::All("*".to_string())
    }
}

/// Single route definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// Route identifier (for logging/debugging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Path pattern to match
    pub path: RoutePattern,

    /// HTTP methods to match (default: all methods)
    #[serde(default)]
    pub methods: MethodMatcher,

    /// Optional: Header matchers (key-value pairs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    /// Wasm modules to execute in order (pipeline)
    pub wasm_modules: Vec<WasmModuleRef>,

    /// Priority for route matching (higher = checked first, default: 0)
    #[serde(default)]
    pub priority: i32,

    /// Whether this route is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Route {
    /// Check if this route matches the given request
    pub fn matches_request(&self, method: &str, path: &str, headers: &[(String, String)]) -> bool {
        // Check if route is enabled
        if !self.enabled {
            return false;
        }

        // Check method match
        if !self.methods.matches(method) {
            return false;
        }

        // Check path match
        if !self.path.matches(path) {
            return false;
        }

        // Check header matches (if specified)
        if let Some(required_headers) = &self.headers {
            for (key, value) in required_headers {
                let header_match = headers.iter().any(|(h_key, h_val)| {
                    h_key.eq_ignore_ascii_case(key) && h_val == value
                });

                if !header_match {
                    return false;
                }
            }
        }

        true
    }
}

/// Top-level route configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteConfig {
    /// List of routes (evaluated in priority order)
    pub routes: Vec<Route>,

    /// Default Wasm modules to apply to all requests (executed first)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_modules: Option<Vec<WasmModuleRef>>,

    /// Global settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<RouteSettings>,
}

impl RouteConfig {
    /// Create an empty route configuration
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            default_modules: None,
            settings: None,
        }
    }

    /// Load from YAML string
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("Failed to parse route config: {}", e))
    }

    /// Load from TOML string
    pub fn from_toml(toml: &str) -> anyhow::Result<Self> {
        toml::from_str(toml).map_err(|e| anyhow::anyhow!("Failed to parse route config: {}", e))
    }

    /// Load from YAML file
    pub fn from_yaml_file(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read route config file '{}': {}", path, e))?;
        Self::from_yaml(&contents)
    }

    /// Load from TOML file
    pub fn from_toml_file(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read route config file '{}': {}", path, e))?;
        Self::from_toml(&contents)
    }

    /// Load from file (auto-detect format based on extension)
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        if path.ends_with(".yaml") || path.ends_with(".yml") {
            Self::from_yaml_file(path)
        } else if path.ends_with(".toml") {
            Self::from_toml_file(path)
        } else {
            Err(anyhow::anyhow!(
                "Unsupported route config format for '{}'. Use .yaml, .yml, or .toml",
                path
            ))
        }
    }

    /// Find the best matching route for a request
    /// Returns the first route that matches, prioritized by priority field
    pub fn find_matching_route(&self, method: &str, path: &str, headers: &[(String, String)]) -> Option<&Route> {
        // Sort by priority (highest first) and find first match
        let mut sorted_routes: Vec<&Route> = self.routes.iter().collect();
        sorted_routes.sort_by(|a, b| b.priority.cmp(&a.priority));

        sorted_routes.into_iter()
            .find(|route| route.matches_request(method, path, headers))
    }
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Global route configuration settings
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteSettings {
    /// Maximum number of modules to execute per request (safety limit)
    #[serde(default = "default_max_modules")]
    pub max_modules_per_request: usize,

    /// Whether to continue pipeline on module errors (default: false = stop on error)
    #[serde(default)]
    pub continue_on_error: bool,
}

fn default_max_modules() -> usize {
    10
}

impl Default for RouteSettings {
    fn default() -> Self {
        Self {
            max_modules_per_request: default_max_modules(),
            continue_on_error: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_pattern_match() {
        let pattern = RoutePattern::Exact("/api/users".to_string());
        assert!(pattern.matches("/api/users"));
        assert!(!pattern.matches("/api/users/123"));
        assert!(!pattern.matches("/api"));
    }

    #[test]
    fn test_prefix_pattern_match() {
        let pattern = RoutePattern::Prefix("/api/*".to_string());
        assert!(pattern.matches("/api"));
        assert!(pattern.matches("/api/users"));
        assert!(pattern.matches("/api/users/123"));
        assert!(!pattern.matches("/other"));
    }

    #[test]
    fn test_regex_pattern_match() {
        let pattern = RoutePattern::Regex(r"^/api/v[0-9]+/.*".to_string());
        assert!(pattern.matches("/api/v1/users"));
        assert!(pattern.matches("/api/v2/products"));
        assert!(!pattern.matches("/api/users"));
        assert!(!pattern.matches("/v1/users"));
    }

    #[test]
    fn test_method_matcher_all() {
        let matcher = MethodMatcher::All("*".to_string());
        assert!(matcher.matches("GET"));
        assert!(matcher.matches("POST"));
        assert!(matcher.matches("DELETE"));
    }

    #[test]
    fn test_method_matcher_single() {
        let matcher = MethodMatcher::Single("GET".to_string());
        assert!(matcher.matches("GET"));
        assert!(matcher.matches("get")); // Case insensitive
        assert!(!matcher.matches("POST"));
    }

    #[test]
    fn test_method_matcher_multiple() {
        let matcher = MethodMatcher::Multiple(vec!["GET".to_string(), "POST".to_string()]);
        assert!(matcher.matches("GET"));
        assert!(matcher.matches("POST"));
        assert!(!matcher.matches("DELETE"));
    }

    #[test]
    fn test_route_matches_request() {
        let route = Route {
            name: Some("api_route".to_string()),
            path: RoutePattern::Prefix("/api/*".to_string()),
            methods: MethodMatcher::Multiple(vec!["GET".to_string(), "POST".to_string()]),
            headers: None,
            wasm_modules: vec![],
            priority: 0,
            enabled: true,
        };

        assert!(route.matches_request("GET", "/api/users", &[]));
        assert!(route.matches_request("POST", "/api/users", &[]));
        assert!(!route.matches_request("DELETE", "/api/users", &[]));
        assert!(!route.matches_request("GET", "/other", &[]));
    }

    #[test]
    fn test_route_priority_ordering() {
        let config = RouteConfig {
            routes: vec![
                Route {
                    name: Some("low_priority".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 1,
                    enabled: true,
                },
                Route {
                    name: Some("high_priority".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 10,
                    enabled: true,
                },
            ],
            default_modules: None,
            settings: None,
        };

        let matched = config.find_matching_route("GET", "/api/test", &[]);
        assert_eq!(matched.unwrap().name, Some("high_priority".to_string()));
    }

    #[test]
    fn test_route_config_from_yaml() {
        let yaml = r#"
routes:
  - name: api_route
    path:
      type: prefix
      pattern: "/api/*"
    methods: ["GET", "POST"]
    wasm_modules:
      - type: waf
        module_id: security-waf-v1
      - type: edge_function
        module_id: api-transform
    priority: 10
    enabled: true
"#;

        match RouteConfig::from_yaml(yaml) {
            Ok(config) => {
                assert_eq!(config.routes.len(), 1);
                assert_eq!(config.routes[0].name, Some("api_route".to_string()));
                assert_eq!(config.routes[0].wasm_modules.len(), 2);
                assert_eq!(config.routes[0].path, RoutePattern::Prefix("/api/*".to_string()));
            }
            Err(e) => {
                eprintln!("YAML parse error: {}", e);
                panic!("Failed to parse YAML: {}", e);
            }
        }
    }

    #[test]
    fn test_disabled_route_does_not_match() {
        let route = Route {
            name: Some("disabled_route".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: 0,
            enabled: false,
        };

        assert!(!route.matches_request("GET", "/test", &[]));
    }

    #[test]
    fn test_route_with_header_matching() {
        let mut headers_map = HashMap::new();
        headers_map.insert("X-API-Key".to_string(), "secret123".to_string());

        let route = Route {
            name: Some("authenticated_route".to_string()),
            path: RoutePattern::Exact("/admin".to_string()),
            methods: MethodMatcher::Single("GET".to_string()),
            headers: Some(headers_map),
            wasm_modules: vec![],
            priority: 0,
            enabled: true,
        };

        // Match with correct header
        let request_headers = vec![
            ("X-API-Key".to_string(), "secret123".to_string()),
            ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
        ];
        assert!(route.matches_request("GET", "/admin", &request_headers));

        // No match without header
        assert!(!route.matches_request("GET", "/admin", &[]));

        // No match with wrong header value
        let wrong_headers = vec![("X-API-Key".to_string(), "wrong".to_string())];
        assert!(!route.matches_request("GET", "/admin", &wrong_headers));
    }

    #[test]
    fn test_route_case_insensitive_headers() {
        let mut headers_map = HashMap::new();
        headers_map.insert("Content-Type".to_string(), "application/json".to_string());

        let route = Route {
            name: Some("json_route".to_string()),
            path: RoutePattern::Exact("/api/data".to_string()),
            methods: MethodMatcher::default(),
            headers: Some(headers_map),
            wasm_modules: vec![],
            priority: 0,
            enabled: true,
        };

        // Header names should be case-insensitive
        let headers_lowercase = vec![("content-type".to_string(), "application/json".to_string())];
        assert!(route.matches_request("POST", "/api/data", &headers_lowercase));

        let headers_uppercase = vec![("CONTENT-TYPE".to_string(), "application/json".to_string())];
        assert!(route.matches_request("POST", "/api/data", &headers_uppercase));
    }

    #[test]
    fn test_prefix_pattern_edge_cases() {
        let pattern = RoutePattern::Prefix("/api/*".to_string());

        // Should match exact prefix without trailing slash
        assert!(pattern.matches("/api"));

        // Should match with trailing slash
        assert!(pattern.matches("/api/"));

        // Should match nested paths
        assert!(pattern.matches("/api/v1/users"));

        // Should not match similar but different paths
        assert!(!pattern.matches("/apis"));
        assert!(!pattern.matches("/api_v2"));
    }

    #[test]
    fn test_regex_pattern_validation() {
        // Valid regex patterns
        let pattern1 = RoutePattern::Regex(r"^/api/v[0-9]+/.*".to_string());
        assert!(pattern1.matches("/api/v1/users"));
        assert!(pattern1.matches("/api/v2/products"));
        assert!(pattern1.matches("/api/v999/items"));
        assert!(!pattern1.matches("/api/users"));

        // Complex regex with groups
        let pattern2 = RoutePattern::Regex(r"^/files/[a-z]+\.(jpg|png|gif)$".to_string());
        assert!(pattern2.matches("/files/image.jpg"));
        assert!(pattern2.matches("/files/photo.png"));
        assert!(!pattern2.matches("/files/document.pdf"));
    }

    #[test]
    fn test_find_matching_route_no_match() {
        let config = RouteConfig {
            routes: vec![
                Route {
                    name: Some("api_route".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
                    methods: MethodMatcher::Single("GET".to_string()),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 0,
                    enabled: true,
                },
            ],
            default_modules: None,
            settings: None,
        };

        // No match - wrong method
        assert!(config.find_matching_route("POST", "/api/users", &[]).is_none());

        // No match - wrong path
        assert!(config.find_matching_route("GET", "/other", &[]).is_none());
    }

    #[test]
    fn test_route_config_default_values() {
        let config = RouteConfig::new();
        assert_eq!(config.routes.len(), 0);
        assert!(config.default_modules.is_none());
        assert!(config.settings.is_none());
    }

    #[test]
    fn test_route_settings_defaults() {
        let settings = RouteSettings::default();
        assert_eq!(settings.max_modules_per_request, 10);
        assert_eq!(settings.continue_on_error, false);
    }

    #[test]
    fn test_multiple_routes_priority_ordering() {
        let config = RouteConfig {
            routes: vec![
                Route {
                    name: Some("catch_all".to_string()),
                    path: RoutePattern::Prefix("/*".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 1,
                    enabled: true,
                },
                Route {
                    name: Some("specific_api".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 50,
                    enabled: true,
                },
                Route {
                    name: Some("very_specific".to_string()),
                    path: RoutePattern::Exact("/api/users".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 100,
                    enabled: true,
                },
            ],
            default_modules: None,
            settings: None,
        };

        // Most specific route should match first (highest priority)
        let matched = config.find_matching_route("GET", "/api/users", &[]);
        assert_eq!(matched.unwrap().name, Some("very_specific".to_string()));

        // Second most specific for other API paths
        let matched = config.find_matching_route("GET", "/api/products", &[]);
        assert_eq!(matched.unwrap().name, Some("specific_api".to_string()));

        // Catch-all for everything else
        let matched = config.find_matching_route("GET", "/other", &[]);
        assert_eq!(matched.unwrap().name, Some("catch_all".to_string()));
    }

    #[test]
    fn test_route_config_from_toml() {
        let toml = r#"
[[routes]]
name = "api_route"
priority = 10
enabled = true

[routes.path]
type = "prefix"
pattern = "/api/*"

routes.methods = ["GET", "POST"]

[[routes.wasm_modules]]
type = "waf"
module_id = "security-waf"
"#;

        let config = RouteConfig::from_toml(toml).unwrap();
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0].name, Some("api_route".to_string()));
    }

    #[test]
    fn test_method_matcher_edge_cases() {
        // Empty string should not match anything
        let matcher = MethodMatcher::Single("GET".to_string());
        assert!(!matcher.matches(""));

        // Case insensitivity
        let matcher = MethodMatcher::Single("post".to_string());
        assert!(matcher.matches("POST"));
        assert!(matcher.matches("Post"));
        assert!(matcher.matches("post"));

        // Multiple methods
        let matcher = MethodMatcher::Multiple(vec![
            "GET".to_string(),
            "HEAD".to_string(),
            "OPTIONS".to_string(),
        ]);
        assert!(matcher.matches("GET"));
        assert!(matcher.matches("head"));
        assert!(matcher.matches("Options"));
        assert!(!matcher.matches("POST"));
    }

    #[test]
    fn test_from_file_auto_detect() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Test YAML file
        let yaml_content = r#"
routes:
  - name: test_route
    path:
      type: exact
      pattern: "/test"
    methods: ["GET"]
    wasm_modules: []
    priority: 0
    enabled: true
"#;
        let mut yaml_file = NamedTempFile::new().unwrap();
        let yaml_path = format!("{}.yaml", yaml_file.path().to_str().unwrap());
        std::fs::write(&yaml_path, yaml_content).unwrap();

        let config = RouteConfig::from_file(&yaml_path).unwrap();
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0].name, Some("test_route".to_string()));

        std::fs::remove_file(&yaml_path).ok();

        // Test .yml extension
        let yml_path = format!("{}.yml", yaml_file.path().to_str().unwrap());
        std::fs::write(&yml_path, yaml_content).unwrap();

        let config = RouteConfig::from_file(&yml_path).unwrap();
        assert_eq!(config.routes.len(), 1);

        std::fs::remove_file(&yml_path).ok();
    }

    #[test]
    fn test_from_file_unsupported_extension() {
        let result = RouteConfig::from_file("config.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported route config format"));
    }

    #[test]
    fn test_from_file_not_found() {
        let result = RouteConfig::from_yaml_file("/nonexistent/path/routes.yaml");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read"));
    }
}
