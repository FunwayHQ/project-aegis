/// Sprint 16: Route-based Dispatch Configuration
///
/// This module provides configuration-driven routing for Wasm modules.
/// Routes map HTTP request patterns to sequences of Wasm modules (WAF, edge functions, etc.)
/// enabling flexible, GitOps-managed edge logic without code changes.
///
/// Sprint 25: Performance optimization - Added CompiledRouteConfig with pre-compiled regex patterns

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Y4.7: Route priority validation constants
// ============================================================================

/// Minimum allowed route priority
pub const MIN_ROUTE_PRIORITY: i32 = 0;

/// Maximum allowed route priority
pub const MAX_ROUTE_PRIORITY: i32 = 10_000;

// ============================================================================
// Security Fix: Input size validation to prevent DoS via large configs
// ============================================================================

/// Maximum allowed route config size (1MB)
/// Prevents memory exhaustion from maliciously large YAML/TOML configs
pub const MAX_ROUTE_CONFIG_SIZE: usize = 1024 * 1024;

/// Route configuration validation error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteConfigError {
    /// Priority is outside valid range
    InvalidPriority { priority: i32, min: i32, max: i32 },
    /// Invalid regex pattern
    InvalidRegexPattern { pattern: String, error: String },
    /// Empty route name
    EmptyRouteName,
    /// Duplicate route name
    DuplicateRouteName(String),
    /// Config size exceeds maximum allowed
    ConfigTooLarge { size: usize, max: usize },
}

impl std::fmt::Display for RouteConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteConfigError::InvalidPriority { priority, min, max } => {
                write!(
                    f,
                    "Route priority {} is outside valid range [{}, {}]",
                    priority, min, max
                )
            }
            RouteConfigError::InvalidRegexPattern { pattern, error } => {
                write!(f, "Invalid regex pattern '{}': {}", pattern, error)
            }
            RouteConfigError::EmptyRouteName => {
                write!(f, "Route name cannot be empty")
            }
            RouteConfigError::DuplicateRouteName(name) => {
                write!(f, "Duplicate route name: {}", name)
            }
            RouteConfigError::ConfigTooLarge { size, max } => {
                write!(
                    f,
                    "Route config size {} bytes exceeds maximum allowed {} bytes",
                    size, max
                )
            }
        }
    }
}

impl std::error::Error for RouteConfigError {}

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
    ///
    /// # Security Warning (Y4.1)
    ///
    /// This method recompiles regex patterns on each call, which can lead to:
    /// 1. Performance degradation
    /// 2. ReDoS (Regular Expression Denial of Service) attacks
    ///
    /// Use `CompiledRoutePattern::compile()` and `CompiledRoutePattern::matches()`
    /// instead, which pre-compiles and caches regex patterns.
    ///
    /// # Deprecation
    ///
    /// This method is deprecated and will be removed in a future release.
    /// Migrate to `CompiledRoutePattern` for all route matching operations.
    #[deprecated(
        since = "0.1.0",
        note = "SECURITY (Y4.1): Use CompiledRoutePattern::matches() instead to prevent ReDoS attacks"
    )]
    pub fn matches(&self, path: &str) -> bool {
        // Compile pattern on the fly - this is the security concern
        let compiled = CompiledRoutePattern::compile(self);
        compiled.matches(path)
    }

    /// Compile this pattern into a CompiledRoutePattern for efficient matching
    ///
    /// # Example
    /// ```
    /// use aegis_node::route_config::{RoutePattern, CompiledRoutePattern};
    ///
    /// let pattern = RoutePattern::Prefix("/api/*".to_string());
    /// let compiled = pattern.compile();
    /// assert!(compiled.matches("/api/users"));
    /// ```
    pub fn compile(&self) -> CompiledRoutePattern {
        CompiledRoutePattern::compile(self)
    }
}

// =============================================================================
// COMPILED ROUTE CONFIG (Sprint 25 Performance Optimization)
// =============================================================================

/// Pre-compiled route pattern for high-performance matching
#[derive(Debug, Clone)]
pub enum CompiledRoutePattern {
    /// Exact path match
    Exact(String),
    /// Prefix match (normalized without trailing * or /)
    Prefix(String),
    /// Pre-compiled regex pattern
    Regex(Arc<regex::Regex>),
    /// Invalid regex pattern (will never match)
    Invalid,
}

impl CompiledRoutePattern {
    /// Compile a RoutePattern into a CompiledRoutePattern
    pub fn compile(pattern: &RoutePattern) -> Self {
        match pattern {
            RoutePattern::Exact(s) => CompiledRoutePattern::Exact(s.clone()),
            RoutePattern::Prefix(prefix) => {
                let normalized = prefix.trim_end_matches('*').trim_end_matches('/').to_string();
                CompiledRoutePattern::Prefix(normalized)
            }
            RoutePattern::Regex(pattern) => {
                match regex::Regex::new(pattern) {
                    Ok(re) => CompiledRoutePattern::Regex(Arc::new(re)),
                    Err(_) => CompiledRoutePattern::Invalid,
                }
            }
        }
    }

    /// Check if a given path matches this compiled pattern (fast path)
    #[inline]
    pub fn matches(&self, path: &str) -> bool {
        match self {
            CompiledRoutePattern::Exact(pattern) => path == pattern,
            CompiledRoutePattern::Prefix(normalized_prefix) => {
                path == normalized_prefix || path.starts_with(&format!("{}/", normalized_prefix))
            }
            CompiledRoutePattern::Regex(re) => re.is_match(path),
            CompiledRoutePattern::Invalid => false,
        }
    }
}

/// Pre-compiled route with cached regex patterns
#[derive(Debug, Clone)]
pub struct CompiledRoute {
    /// Route identifier (for logging/debugging)
    pub name: Option<String>,
    /// Compiled path pattern
    pub path: CompiledRoutePattern,
    /// HTTP methods to match
    pub methods: MethodMatcher,
    /// Optional: Header matchers
    pub headers: Option<HashMap<String, String>>,
    /// Wasm modules to execute
    pub wasm_modules: Vec<WasmModuleRef>,
    /// Priority for route matching
    pub priority: i32,
    /// Whether this route is enabled
    pub enabled: bool,
}

impl CompiledRoute {
    /// Compile a Route into a CompiledRoute
    pub fn compile(route: &Route) -> Self {
        Self {
            name: route.name.clone(),
            path: CompiledRoutePattern::compile(&route.path),
            methods: route.methods.clone(),
            headers: route.headers.clone(),
            wasm_modules: route.wasm_modules.clone(),
            priority: route.priority,
            enabled: route.enabled,
        }
    }

    /// Check if this route matches the given request (fast path)
    #[inline]
    pub fn matches_request(&self, method: &str, path: &str, headers: &[(String, String)]) -> bool {
        // Check if route is enabled
        if !self.enabled {
            return false;
        }

        // Check method match
        if !self.methods.matches(method) {
            return false;
        }

        // Check path match (using compiled pattern)
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

/// Pre-compiled route configuration for high-performance matching
/// Routes are sorted by priority at compile time for O(n) matching
#[derive(Debug, Clone)]
pub struct CompiledRouteConfig {
    /// Compiled routes (pre-sorted by priority, highest first)
    routes: Vec<CompiledRoute>,
    /// Default Wasm modules
    pub default_modules: Option<Vec<WasmModuleRef>>,
    /// Global settings
    pub settings: Option<RouteSettings>,
}

impl CompiledRouteConfig {
    /// Compile a RouteConfig into a CompiledRouteConfig
    pub fn compile(config: &RouteConfig) -> Self {
        let mut routes: Vec<CompiledRoute> = config.routes.iter()
            .map(CompiledRoute::compile)
            .collect();

        // Pre-sort by priority (highest first) at compile time
        routes.sort_by(|a, b| b.priority.cmp(&a.priority));

        Self {
            routes,
            default_modules: config.default_modules.clone(),
            settings: config.settings.clone(),
        }
    }

    /// Find the best matching route for a request (fast path)
    /// Routes are pre-sorted, so we just find the first match
    #[inline]
    pub fn find_matching_route(&self, method: &str, path: &str, headers: &[(String, String)]) -> Option<&CompiledRoute> {
        self.routes.iter()
            .find(|route| route.matches_request(method, path, headers))
    }

    /// Get the number of compiled routes
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

/// Wasm module reference in route configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WasmModuleRef {
    /// Module type (waf, edge_function, ddos_protection, rate_limiter, etc.)
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

    /// Optional: Inline configuration for the module (e.g., rate limit settings)
    /// Used by ddos_protection and rate_limiter module types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
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
    ///
    /// # Security Note (Y4.2)
    ///
    /// This method compiles the route pattern on each call.
    /// For high-performance scenarios, use `RouteConfig::compile()` to get a
    /// `CompiledRouteConfig`, which pre-compiles all patterns.
    pub fn matches_request(&self, method: &str, path: &str, headers: &[(String, String)]) -> bool {
        // Check if route is enabled
        if !self.enabled {
            return false;
        }

        // Check method match
        if !self.methods.matches(method) {
            return false;
        }

        // SECURITY FIX (Y4.2): Use CompiledRoutePattern instead of legacy matches()
        // This compiles the pattern, which is still not ideal for hot paths.
        // For production, use CompiledRouteConfig for pre-compiled patterns.
        let compiled_path = self.path.compile();
        if !compiled_path.matches(path) {
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

    /// Validate this route configuration
    ///
    /// # Returns
    /// `Ok(())` if the route is valid, `Err(RouteConfigError)` if validation fails.
    ///
    /// # Validation Rules (Y4.7)
    /// - Route name must not be empty if provided
    /// - Priority must be within valid range (0-10000)
    /// - Regex patterns must be valid
    pub fn validate(&self) -> Result<(), RouteConfigError> {
        // Y4.7: Validate route priority range
        if self.priority < MIN_ROUTE_PRIORITY || self.priority > MAX_ROUTE_PRIORITY {
            return Err(RouteConfigError::InvalidPriority {
                priority: self.priority,
                min: MIN_ROUTE_PRIORITY,
                max: MAX_ROUTE_PRIORITY,
            });
        }

        // Validate route name if provided
        if let Some(ref name) = self.name {
            if name.is_empty() {
                return Err(RouteConfigError::EmptyRouteName);
            }
        }

        // Validate regex pattern compiles successfully
        if let RoutePattern::Regex(ref pattern) = self.path {
            match regex::Regex::new(pattern) {
                Ok(_) => {}
                Err(e) => {
                    return Err(RouteConfigError::InvalidRegexPattern {
                        pattern: pattern.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(())
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
    ///
    /// # Security
    /// Validates input size to prevent DoS attacks via large configs.
    /// Maximum size is `MAX_ROUTE_CONFIG_SIZE` (1MB).
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        // Security: Validate input size before parsing
        if yaml.len() > MAX_ROUTE_CONFIG_SIZE {
            return Err(anyhow::anyhow!(
                "{}",
                RouteConfigError::ConfigTooLarge {
                    size: yaml.len(),
                    max: MAX_ROUTE_CONFIG_SIZE
                }
            ));
        }
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("Failed to parse route config: {}", e))
    }

    /// Load from TOML string
    ///
    /// # Security
    /// Validates input size to prevent DoS attacks via large configs.
    /// Maximum size is `MAX_ROUTE_CONFIG_SIZE` (1MB).
    pub fn from_toml(toml: &str) -> anyhow::Result<Self> {
        // Security: Validate input size before parsing
        if toml.len() > MAX_ROUTE_CONFIG_SIZE {
            return Err(anyhow::anyhow!(
                "{}",
                RouteConfigError::ConfigTooLarge {
                    size: toml.len(),
                    max: MAX_ROUTE_CONFIG_SIZE
                }
            ));
        }
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
    /// Note: This sorts routes on each call. For high-performance matching,
    /// use `compile()` to get a CompiledRouteConfig.
    pub fn find_matching_route(&self, method: &str, path: &str, headers: &[(String, String)]) -> Option<&Route> {
        // Sort by priority (highest first) and find first match
        let mut sorted_routes: Vec<&Route> = self.routes.iter().collect();
        sorted_routes.sort_by(|a, b| b.priority.cmp(&a.priority));

        sorted_routes.into_iter()
            .find(|route| route.matches_request(method, path, headers))
    }

    /// Compile this configuration for high-performance matching
    /// Pre-compiles all regex patterns and pre-sorts routes by priority
    pub fn compile(&self) -> CompiledRouteConfig {
        CompiledRouteConfig::compile(self)
    }

    /// Validate this route configuration
    ///
    /// # Returns
    /// `Ok(())` if all routes are valid, `Err(RouteConfigError)` if validation fails.
    ///
    /// # Validation Rules (Y4.7)
    /// - All route priorities must be within valid range (0-10000)
    /// - Route names must not be empty if provided
    /// - Route names must be unique
    /// - Regex patterns must be valid
    ///
    /// # Example
    /// ```
    /// use aegis_node::route_config::{RouteConfig, Route, RoutePattern, MethodMatcher};
    ///
    /// let mut config = RouteConfig::new();
    /// config.routes.push(Route {
    ///     name: Some("api".to_string()),
    ///     path: RoutePattern::Prefix("/api/*".to_string()),
    ///     methods: MethodMatcher::default(),
    ///     headers: None,
    ///     wasm_modules: vec![],
    ///     priority: 100,
    ///     enabled: true,
    /// });
    /// assert!(config.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), RouteConfigError> {
        use std::collections::HashSet;
        let mut seen_names: HashSet<&str> = HashSet::new();

        for route in &self.routes {
            // Validate each route
            route.validate()?;

            // Check for duplicate route names
            if let Some(ref name) = route.name {
                if !seen_names.insert(name.as_str()) {
                    return Err(RouteConfigError::DuplicateRouteName(name.clone()));
                }
            }
        }

        Ok(())
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

    // ========================================================================
    // Sprint Y4: Wasm Runtime Security Tests
    // ========================================================================

    /// Y4.1: Test that CompiledRoutePattern is used for matching
    #[test]
    fn test_y41_compiled_route_pattern_matches() {
        // Test exact pattern
        let exact = RoutePattern::Exact("/api/users".to_string());
        let compiled = exact.compile();
        assert!(compiled.matches("/api/users"));
        assert!(!compiled.matches("/api/users/123"));

        // Test prefix pattern
        let prefix = RoutePattern::Prefix("/api/*".to_string());
        let compiled = prefix.compile();
        assert!(compiled.matches("/api"));
        assert!(compiled.matches("/api/users"));
        assert!(compiled.matches("/api/users/123"));
        assert!(!compiled.matches("/other"));

        // Test regex pattern
        let regex = RoutePattern::Regex(r"^/api/v[0-9]+/.*".to_string());
        let compiled = regex.compile();
        assert!(compiled.matches("/api/v1/users"));
        assert!(compiled.matches("/api/v2/products"));
        assert!(!compiled.matches("/api/users"));
    }

    /// Y4.1: Test that invalid regex produces CompiledRoutePattern::Invalid
    #[test]
    fn test_y41_compiled_route_pattern_invalid_regex() {
        let invalid_regex = RoutePattern::Regex(r"[invalid(".to_string());
        let compiled = invalid_regex.compile();
        assert!(matches!(compiled, CompiledRoutePattern::Invalid));
        assert!(!compiled.matches("/anything")); // Invalid never matches
    }

    /// Y4.2: Test CompiledRoute for high-performance matching
    #[test]
    fn test_y42_compiled_route_matches_request() {
        let route = Route {
            name: Some("api_route".to_string()),
            path: RoutePattern::Prefix("/api/*".to_string()),
            methods: MethodMatcher::Multiple(vec!["GET".to_string(), "POST".to_string()]),
            headers: None,
            wasm_modules: vec![],
            priority: 100,
            enabled: true,
        };

        let compiled = CompiledRoute::compile(&route);

        assert!(compiled.matches_request("GET", "/api/users", &[]));
        assert!(compiled.matches_request("POST", "/api/products", &[]));
        assert!(!compiled.matches_request("DELETE", "/api/users", &[]));
        assert!(!compiled.matches_request("GET", "/other", &[]));
    }

    /// Y4.2: Test CompiledRouteConfig for pre-compiled route matching
    #[test]
    fn test_y42_compiled_route_config_priority_sorting() {
        let config = RouteConfig {
            routes: vec![
                Route {
                    name: Some("low".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 10,
                    enabled: true,
                },
                Route {
                    name: Some("high".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
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

        let compiled = config.compile();
        assert_eq!(compiled.route_count(), 2);

        // High priority route should match first
        let matched = compiled.find_matching_route("GET", "/api/test", &[]);
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().name, Some("high".to_string()));
    }

    /// Y4.7: Test route priority validation - valid range
    #[test]
    fn test_y47_route_priority_valid_range() {
        // Test minimum valid priority
        let route_min = Route {
            name: Some("min_priority".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: MIN_ROUTE_PRIORITY,
            enabled: true,
        };
        assert!(route_min.validate().is_ok());

        // Test maximum valid priority
        let route_max = Route {
            name: Some("max_priority".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: MAX_ROUTE_PRIORITY,
            enabled: true,
        };
        assert!(route_max.validate().is_ok());

        // Test mid-range priority
        let route_mid = Route {
            name: Some("mid_priority".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: 5000,
            enabled: true,
        };
        assert!(route_mid.validate().is_ok());
    }

    /// Y4.7: Test route priority validation - invalid range (too high)
    #[test]
    fn test_y47_route_priority_too_high() {
        let route = Route {
            name: Some("too_high".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: MAX_ROUTE_PRIORITY + 1,
            enabled: true,
        };

        let result = route.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            RouteConfigError::InvalidPriority { priority, min, max } => {
                assert_eq!(priority, MAX_ROUTE_PRIORITY + 1);
                assert_eq!(min, MIN_ROUTE_PRIORITY);
                assert_eq!(max, MAX_ROUTE_PRIORITY);
            }
            _ => panic!("Expected InvalidPriority error"),
        }
    }

    /// Y4.7: Test route priority validation - invalid range (negative)
    #[test]
    fn test_y47_route_priority_negative() {
        let route = Route {
            name: Some("negative".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: -1,
            enabled: true,
        };

        let result = route.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RouteConfigError::InvalidPriority { .. }));
    }

    /// Y4.7: Test route validation - empty name
    #[test]
    fn test_y47_route_empty_name() {
        let route = Route {
            name: Some("".to_string()), // Empty name
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: 0,
            enabled: true,
        };

        let result = route.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RouteConfigError::EmptyRouteName));
    }

    /// Y4.7: Test route validation - invalid regex pattern
    #[test]
    fn test_y47_route_invalid_regex() {
        let route = Route {
            name: Some("bad_regex".to_string()),
            path: RoutePattern::Regex(r"[unclosed".to_string()), // Invalid regex
            methods: MethodMatcher::default(),
            headers: None,
            wasm_modules: vec![],
            priority: 0,
            enabled: true,
        };

        let result = route.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            RouteConfigError::InvalidRegexPattern { pattern, error: _ } => {
                assert_eq!(pattern, "[unclosed");
            }
            _ => panic!("Expected InvalidRegexPattern error"),
        }
    }

    /// Y4.7: Test RouteConfig validation - duplicate route names
    #[test]
    fn test_y47_config_duplicate_route_names() {
        let config = RouteConfig {
            routes: vec![
                Route {
                    name: Some("duplicate".to_string()),
                    path: RoutePattern::Exact("/first".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 0,
                    enabled: true,
                },
                Route {
                    name: Some("duplicate".to_string()), // Same name
                    path: RoutePattern::Exact("/second".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 0,
                    enabled: true,
                },
            ],
            default_modules: None,
            settings: None,
        };

        let result = config.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            RouteConfigError::DuplicateRouteName(name) => {
                assert_eq!(name, "duplicate");
            }
            _ => panic!("Expected DuplicateRouteName error"),
        }
    }

    /// Y4.7: Test RouteConfig validation - valid config
    #[test]
    fn test_y47_config_valid() {
        let config = RouteConfig {
            routes: vec![
                Route {
                    name: Some("route1".to_string()),
                    path: RoutePattern::Exact("/first".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 100,
                    enabled: true,
                },
                Route {
                    name: Some("route2".to_string()),
                    path: RoutePattern::Prefix("/api/*".to_string()),
                    methods: MethodMatcher::default(),
                    headers: None,
                    wasm_modules: vec![],
                    priority: 50,
                    enabled: true,
                },
                Route {
                    name: None, // Anonymous routes are allowed
                    path: RoutePattern::Regex(r"^/v[0-9]+/.*".to_string()),
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

        assert!(config.validate().is_ok());
    }

    /// Y4.7: Test RouteConfigError Display implementation
    #[test]
    fn test_y47_route_config_error_display() {
        let err1 = RouteConfigError::InvalidPriority {
            priority: 99999,
            min: 0,
            max: 10000,
        };
        assert!(err1.to_string().contains("99999"));
        assert!(err1.to_string().contains("10000"));

        let err2 = RouteConfigError::InvalidRegexPattern {
            pattern: "[bad".to_string(),
            error: "unclosed bracket".to_string(),
        };
        assert!(err2.to_string().contains("[bad"));
        assert!(err2.to_string().contains("unclosed bracket"));

        let err3 = RouteConfigError::EmptyRouteName;
        assert!(err3.to_string().contains("empty"));

        let err4 = RouteConfigError::DuplicateRouteName("test".to_string());
        assert!(err4.to_string().contains("test"));
    }
}
