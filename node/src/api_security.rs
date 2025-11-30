// Sprint 23: API Security Suite
//
// This module implements API-specific security features:
// 1. API endpoint discovery (learn endpoints from traffic)
// 2. OpenAPI/JSON Schema validation at edge
// 3. JWT/OAuth token validation (signature, expiry, claims)
// 4. Sequence detection (credential stuffing, enumeration, scraping)
// 5. Per-endpoint rate limiting with dynamic thresholds

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

// ============================================
// API Endpoint Discovery
// ============================================

/// Discovered API endpoint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredEndpoint {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Path pattern (e.g., /api/users/{id})
    pub path_pattern: String,
    /// Original paths that matched this pattern
    pub sample_paths: Vec<String>,
    /// Query parameter names seen
    pub query_params: Vec<String>,
    /// Header names commonly sent
    pub common_headers: Vec<String>,
    /// Request count
    pub request_count: u64,
    /// First seen timestamp
    pub first_seen: u64,
    /// Last seen timestamp
    pub last_seen: u64,
    /// Average response time (ms)
    pub avg_response_time_ms: f64,
    /// Error rate (4xx/5xx responses)
    pub error_rate: f64,
}

/// API Discovery engine that learns endpoints from traffic
#[derive(Debug)]
pub struct ApiDiscovery {
    /// Discovered endpoints keyed by (method, path_pattern)
    endpoints: Arc<RwLock<HashMap<(String, String), DiscoveredEndpoint>>>,
    /// Known path parameter patterns (e.g., UUIDs, numeric IDs)
    path_param_patterns: Vec<(Regex, String)>,
    /// Maximum sample paths to store per endpoint
    max_sample_paths: usize,
}

impl ApiDiscovery {
    pub fn new() -> Self {
        // Common patterns for path parameters
        let path_param_patterns = vec![
            // UUID pattern
            (
                Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                    .unwrap(),
                "{uuid}".to_string(),
            ),
            // Numeric ID
            (Regex::new(r"^\d+$").unwrap(), "{id}".to_string()),
            // Hex string (e.g., MongoDB ObjectId)
            (Regex::new(r"^[0-9a-f]{24}$").unwrap(), "{objectId}".to_string()),
            // Base64-like strings
            (
                Regex::new(r"^[A-Za-z0-9_-]{20,}$").unwrap(),
                "{token}".to_string(),
            ),
            // Email-like
            (
                Regex::new(r"^[^@]+@[^@]+\.[^@]+$").unwrap(),
                "{email}".to_string(),
            ),
        ];

        Self {
            endpoints: Arc::new(RwLock::new(HashMap::new())),
            path_param_patterns,
            max_sample_paths: 5,
        }
    }

    /// Normalize a path by replacing dynamic segments with placeholders
    pub fn normalize_path(&self, path: &str) -> String {
        let segments: Vec<&str> = path.split('/').collect();
        let normalized: Vec<String> = segments
            .iter()
            .map(|segment| {
                if segment.is_empty() {
                    return String::new();
                }
                // Check each pattern
                for (pattern, replacement) in &self.path_param_patterns {
                    if pattern.is_match(segment) {
                        return replacement.clone();
                    }
                }
                segment.to_string()
            })
            .collect();
        normalized.join("/")
    }

    /// Record an API request for discovery
    pub async fn record_request(
        &self,
        method: &str,
        path: &str,
        query_params: &[String],
        headers: &[String],
        response_time_ms: f64,
        is_error: bool,
    ) {
        let normalized_path = self.normalize_path(path);
        let key = (method.to_uppercase(), normalized_path.clone());

        let mut endpoints = self.endpoints.write().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let endpoint = endpoints.entry(key).or_insert_with(|| DiscoveredEndpoint {
            method: method.to_uppercase(),
            path_pattern: normalized_path,
            sample_paths: Vec::new(),
            query_params: Vec::new(),
            common_headers: Vec::new(),
            request_count: 0,
            first_seen: now,
            last_seen: now,
            avg_response_time_ms: 0.0,
            error_rate: 0.0,
        });

        // Update stats
        endpoint.request_count += 1;
        endpoint.last_seen = now;

        // Update running average for response time
        let count = endpoint.request_count as f64;
        endpoint.avg_response_time_ms =
            (endpoint.avg_response_time_ms * (count - 1.0) + response_time_ms) / count;

        // Update error rate
        let error_weight = if is_error { 1.0 } else { 0.0 };
        endpoint.error_rate = (endpoint.error_rate * (count - 1.0) + error_weight) / count;

        // Add sample path if not already present
        if endpoint.sample_paths.len() < self.max_sample_paths
            && !endpoint.sample_paths.contains(&path.to_string())
        {
            endpoint.sample_paths.push(path.to_string());
        }

        // Update query params
        for param in query_params {
            if !endpoint.query_params.contains(param) {
                endpoint.query_params.push(param.clone());
            }
        }

        // Update common headers
        for header in headers {
            if !endpoint.common_headers.contains(header) {
                endpoint.common_headers.push(header.clone());
            }
        }
    }

    /// Get all discovered endpoints
    pub async fn get_endpoints(&self) -> Vec<DiscoveredEndpoint> {
        let endpoints = self.endpoints.read().await;
        endpoints.values().cloned().collect()
    }

    /// Check if an endpoint is known (not a shadow API)
    pub async fn is_known_endpoint(&self, method: &str, path: &str) -> bool {
        let normalized_path = self.normalize_path(path);
        let key = (method.to_uppercase(), normalized_path);
        let endpoints = self.endpoints.read().await;
        endpoints.contains_key(&key)
    }

    /// Get statistics for a specific endpoint
    pub async fn get_endpoint_stats(
        &self,
        method: &str,
        path: &str,
    ) -> Option<DiscoveredEndpoint> {
        let normalized_path = self.normalize_path(path);
        let key = (method.to_uppercase(), normalized_path);
        let endpoints = self.endpoints.read().await;
        endpoints.get(&key).cloned()
    }

    /// Export inventory for storage
    pub async fn export_inventory(&self) -> String {
        let endpoints = self.endpoints.read().await;
        serde_json::to_string_pretty(&*endpoints).unwrap_or_default()
    }

    /// Import inventory from storage
    pub async fn import_inventory(&self, json: &str) -> Result<usize, String> {
        let imported: HashMap<(String, String), DiscoveredEndpoint> =
            serde_json::from_str(json).map_err(|e| e.to_string())?;
        let count = imported.len();
        let mut endpoints = self.endpoints.write().await;
        *endpoints = imported;
        Ok(count)
    }
}

impl Default for ApiDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// OpenAPI / JSON Schema Validation
// ============================================

/// OpenAPI path parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub required: bool,
    pub pattern: Option<String>,
}

/// OpenAPI request body schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBodySchema {
    pub content_type: String,
    pub schema: JsonValue,
    pub required: bool,
}

/// OpenAPI endpoint specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSpec {
    pub path: String,
    pub method: String,
    pub path_parameters: Vec<PathParameter>,
    pub query_parameters: Vec<PathParameter>,
    pub required_headers: Vec<String>,
    pub request_body: Option<RequestBodySchema>,
}

/// Schema validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

/// Schema validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub location: String,
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// OpenAPI Schema Validator
#[derive(Debug)]
pub struct SchemaValidator {
    /// Endpoint specifications keyed by (method, path_pattern)
    specs: HashMap<(String, String), EndpointSpec>,
    /// Path patterns with regex for matching
    path_patterns: Vec<(String, Regex, EndpointSpec)>,
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            path_patterns: Vec::new(),
        }
    }

    /// Load OpenAPI 3.0 specification from JSON
    pub fn load_openapi_spec(&mut self, spec_json: &str) -> Result<usize, String> {
        let spec: JsonValue = serde_json::from_str(spec_json).map_err(|e| e.to_string())?;

        let paths = spec
            .get("paths")
            .and_then(|p| p.as_object())
            .ok_or("Missing 'paths' in OpenAPI spec")?;

        let mut count = 0;
        for (path, methods) in paths {
            let methods_obj = methods.as_object().ok_or("Invalid path definition")?;

            for (method, operation) in methods_obj {
                if method.starts_with('x') {
                    continue; // Skip extensions
                }

                let endpoint_spec = self.parse_endpoint(path, method, operation)?;
                self.add_endpoint(endpoint_spec);
                count += 1;
            }
        }

        Ok(count)
    }

    fn parse_endpoint(
        &self,
        path: &str,
        method: &str,
        operation: &JsonValue,
    ) -> Result<EndpointSpec, String> {
        let mut path_parameters = Vec::new();
        let mut query_parameters = Vec::new();
        let mut required_headers = Vec::new();

        // Parse parameters
        if let Some(params) = operation.get("parameters").and_then(|p| p.as_array()) {
            for param in params {
                let name = param
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let location = param.get("in").and_then(|i| i.as_str()).unwrap_or("");
                let required = param
                    .get("required")
                    .and_then(|r| r.as_bool())
                    .unwrap_or(false);

                let schema = param.get("schema").cloned().unwrap_or(JsonValue::Null);
                let param_type = schema
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("string")
                    .to_string();
                let pattern = schema
                    .get("pattern")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string());

                let param_def = PathParameter {
                    name: name.clone(),
                    param_type,
                    required,
                    pattern,
                };

                match location {
                    "path" => path_parameters.push(param_def),
                    "query" => query_parameters.push(param_def),
                    "header" if required => required_headers.push(name),
                    _ => {}
                }
            }
        }

        // Parse request body
        let request_body = operation
            .get("requestBody")
            .and_then(|rb| {
                let required = rb.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
                let content = rb.get("content")?.as_object()?;

                // Prefer application/json
                let (content_type, schema_def) = content
                    .get("application/json")
                    .map(|s| ("application/json", s))
                    .or_else(|| content.iter().next().map(|(k, v)| (k.as_str(), v)))?;

                let schema = schema_def.get("schema")?.clone();

                Some(RequestBodySchema {
                    content_type: content_type.to_string(),
                    schema,
                    required,
                })
            });

        Ok(EndpointSpec {
            path: path.to_string(),
            method: method.to_uppercase(),
            path_parameters,
            query_parameters,
            required_headers,
            request_body,
        })
    }

    /// Add an endpoint specification
    pub fn add_endpoint(&mut self, spec: EndpointSpec) {
        // Convert OpenAPI path pattern to regex
        let mut regex_pattern = spec.path.clone();

        // Replace {param} with named capture groups
        let param_re = Regex::new(r"\{([^}]+)\}").unwrap();
        regex_pattern = param_re
            .replace_all(&regex_pattern, r"(?P<$1>[^/]+)")
            .to_string();

        // Anchor the pattern
        regex_pattern = format!("^{}$", regex_pattern);

        if let Ok(regex) = Regex::new(&regex_pattern) {
            self.path_patterns
                .push((spec.path.clone(), regex, spec.clone()));
        }

        let key = (spec.method.clone(), spec.path.clone());
        self.specs.insert(key, spec);
    }

    /// Find matching endpoint spec for a request
    pub fn find_spec(&self, method: &str, path: &str) -> Option<&EndpointSpec> {
        let method_upper = method.to_uppercase();

        for (_, regex, spec) in &self.path_patterns {
            if spec.method == method_upper && regex.is_match(path) {
                return Some(spec);
            }
        }

        None
    }

    /// Validate a request against the schema
    pub fn validate_request(
        &self,
        method: &str,
        path: &str,
        query_params: &HashMap<String, String>,
        headers: &HashMap<String, String>,
        body: Option<&str>,
    ) -> ValidationResult {
        let mut errors = Vec::new();

        // Find matching spec
        let spec = match self.find_spec(method, path) {
            Some(s) => s,
            None => {
                // Unknown endpoint - could be shadow API
                return ValidationResult {
                    valid: true, // Don't block unknown endpoints by default
                    errors: vec![ValidationError {
                        location: "path".to_string(),
                        message: "Unknown endpoint".to_string(),
                        expected: None,
                        actual: Some(format!("{} {}", method, path)),
                    }],
                };
            }
        };

        // Validate required query parameters
        for param in &spec.query_parameters {
            if param.required && !query_params.contains_key(&param.name) {
                errors.push(ValidationError {
                    location: format!("query.{}", param.name),
                    message: "Required query parameter missing".to_string(),
                    expected: Some(param.name.clone()),
                    actual: None,
                });
            }

            // Validate parameter type if present
            if let Some(value) = query_params.get(&param.name) {
                if let Some(err) = self.validate_type(value, &param.param_type, &param.pattern) {
                    errors.push(ValidationError {
                        location: format!("query.{}", param.name),
                        message: err,
                        expected: Some(param.param_type.clone()),
                        actual: Some(value.clone()),
                    });
                }
            }
        }

        // Validate required headers
        for header in &spec.required_headers {
            let header_lower = header.to_lowercase();
            if !headers.keys().any(|k| k.to_lowercase() == header_lower) {
                errors.push(ValidationError {
                    location: format!("header.{}", header),
                    message: "Required header missing".to_string(),
                    expected: Some(header.clone()),
                    actual: None,
                });
            }
        }

        // Validate request body
        if let Some(body_spec) = &spec.request_body {
            if body_spec.required && body.is_none() {
                errors.push(ValidationError {
                    location: "body".to_string(),
                    message: "Required request body missing".to_string(),
                    expected: Some("JSON body".to_string()),
                    actual: None,
                });
            }

            if let Some(body_str) = body {
                if let Err(e) = self.validate_json_body(body_str, &body_spec.schema) {
                    errors.push(e);
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
        }
    }

    fn validate_type(&self, value: &str, expected_type: &str, pattern: &Option<String>) -> Option<String> {
        match expected_type {
            "integer" => {
                if value.parse::<i64>().is_err() {
                    return Some("Expected integer".to_string());
                }
            }
            "number" => {
                if value.parse::<f64>().is_err() {
                    return Some("Expected number".to_string());
                }
            }
            "boolean" => {
                if value != "true" && value != "false" {
                    return Some("Expected boolean".to_string());
                }
            }
            _ => {}
        }

        // Check pattern if specified
        if let Some(pat) = pattern {
            if let Ok(re) = Regex::new(pat) {
                if !re.is_match(value) {
                    return Some(format!("Value does not match pattern: {}", pat));
                }
            }
        }

        None
    }

    fn validate_json_body(&self, body: &str, schema: &JsonValue) -> Result<(), ValidationError> {
        let parsed: JsonValue = serde_json::from_str(body).map_err(|e| ValidationError {
            location: "body".to_string(),
            message: format!("Invalid JSON: {}", e),
            expected: Some("valid JSON".to_string()),
            actual: Some(body.chars().take(100).collect()),
        })?;

        self.validate_json_value(&parsed, schema, "body")
    }

    fn validate_json_value(
        &self,
        value: &JsonValue,
        schema: &JsonValue,
        path: &str,
    ) -> Result<(), ValidationError> {
        let expected_type = schema.get("type").and_then(|t| t.as_str());

        match expected_type {
            Some("object") => {
                if !value.is_object() {
                    return Err(ValidationError {
                        location: path.to_string(),
                        message: "Expected object".to_string(),
                        expected: Some("object".to_string()),
                        actual: Some(format!("{:?}", value)),
                    });
                }

                // Check required properties
                if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                    for req in required {
                        if let Some(prop_name) = req.as_str() {
                            if value.get(prop_name).is_none() {
                                return Err(ValidationError {
                                    location: format!("{}.{}", path, prop_name),
                                    message: "Required property missing".to_string(),
                                    expected: Some(prop_name.to_string()),
                                    actual: None,
                                });
                            }
                        }
                    }
                }

                // Validate properties
                if let (Some(props), Some(obj)) = (
                    schema.get("properties").and_then(|p| p.as_object()),
                    value.as_object(),
                ) {
                    for (key, val) in obj {
                        if let Some(prop_schema) = props.get(key) {
                            self.validate_json_value(val, prop_schema, &format!("{}.{}", path, key))?;
                        }
                    }
                }
            }
            Some("array") => {
                if !value.is_array() {
                    return Err(ValidationError {
                        location: path.to_string(),
                        message: "Expected array".to_string(),
                        expected: Some("array".to_string()),
                        actual: Some(format!("{:?}", value)),
                    });
                }

                // Validate array items
                if let (Some(items_schema), Some(arr)) =
                    (schema.get("items"), value.as_array())
                {
                    for (i, item) in arr.iter().enumerate() {
                        self.validate_json_value(item, items_schema, &format!("{}[{}]", path, i))?;
                    }
                }
            }
            Some("string") => {
                if !value.is_string() {
                    return Err(ValidationError {
                        location: path.to_string(),
                        message: "Expected string".to_string(),
                        expected: Some("string".to_string()),
                        actual: Some(format!("{:?}", value)),
                    });
                }
            }
            Some("integer") => {
                if !value.is_i64() && !value.is_u64() {
                    return Err(ValidationError {
                        location: path.to_string(),
                        message: "Expected integer".to_string(),
                        expected: Some("integer".to_string()),
                        actual: Some(format!("{:?}", value)),
                    });
                }
            }
            Some("number") => {
                if !value.is_number() {
                    return Err(ValidationError {
                        location: path.to_string(),
                        message: "Expected number".to_string(),
                        expected: Some("number".to_string()),
                        actual: Some(format!("{:?}", value)),
                    });
                }
            }
            Some("boolean") => {
                if !value.is_boolean() {
                    return Err(ValidationError {
                        location: path.to_string(),
                        message: "Expected boolean".to_string(),
                        expected: Some("boolean".to_string()),
                        actual: Some(format!("{:?}", value)),
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// JWT/OAuth Token Validation
// ============================================

/// JWT algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JwtAlgorithm {
    HS256,
    HS384,
    HS512,
    RS256,
    RS384,
    RS512,
    ES256,
    ES384,
    ES512,
    EdDSA,
}

impl JwtAlgorithm {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "HS256" => Some(Self::HS256),
            "HS384" => Some(Self::HS384),
            "HS512" => Some(Self::HS512),
            "RS256" => Some(Self::RS256),
            "RS384" => Some(Self::RS384),
            "RS512" => Some(Self::RS512),
            "ES256" => Some(Self::ES256),
            "ES384" => Some(Self::ES384),
            "ES512" => Some(Self::ES512),
            "EdDSA" => Some(Self::EdDSA),
            _ => None,
        }
    }
}

/// JWT header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: Option<String>,
    pub kid: Option<String>,
}

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject
    pub sub: Option<String>,
    /// Issuer
    pub iss: Option<String>,
    /// Audience
    pub aud: Option<StringOrVec>,
    /// Expiration time (Unix timestamp)
    pub exp: Option<u64>,
    /// Not before (Unix timestamp)
    pub nbf: Option<u64>,
    /// Issued at (Unix timestamp)
    pub iat: Option<u64>,
    /// JWT ID
    pub jti: Option<String>,
    /// Additional claims
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

/// String or array of strings (for aud claim)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    String(String),
    Vec(Vec<String>),
}

impl StringOrVec {
    pub fn contains(&self, s: &str) -> bool {
        match self {
            StringOrVec::String(val) => val == s,
            StringOrVec::Vec(vals) => vals.iter().any(|v| v == s),
        }
    }
}

/// Decoded JWT token
#[derive(Debug, Clone)]
pub struct DecodedJwt {
    pub header: JwtHeader,
    pub claims: JwtClaims,
    pub signature: Vec<u8>,
    pub signed_payload: String,
}

/// JWT validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtValidationConfig {
    /// Required issuer
    pub required_issuer: Option<String>,
    /// Required audience
    pub required_audience: Option<String>,
    /// Clock skew tolerance in seconds
    pub clock_skew_seconds: u64,
    /// Validate expiration
    pub validate_exp: bool,
    /// Validate not before
    pub validate_nbf: bool,
}

impl Default for JwtValidationConfig {
    fn default() -> Self {
        Self {
            required_issuer: None,
            required_audience: None,
            clock_skew_seconds: 60,
            validate_exp: true,
            validate_nbf: true,
        }
    }
}

/// JWT validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JwtValidationError {
    InvalidFormat,
    InvalidBase64,
    InvalidJson,
    UnsupportedAlgorithm(String),
    TokenExpired,
    TokenNotYetValid,
    InvalidIssuer { expected: String, actual: Option<String> },
    InvalidAudience { expected: String, actual: Option<String> },
    InvalidSignature,
    MissingKey,
    /// SECURITY FIX: Added for mandatory exp claim validation
    MissingExpiration,
}

impl std::fmt::Display for JwtValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "Invalid JWT format"),
            Self::InvalidBase64 => write!(f, "Invalid base64 encoding"),
            Self::InvalidJson => write!(f, "Invalid JSON in JWT"),
            Self::UnsupportedAlgorithm(alg) => write!(f, "Unsupported algorithm: {}", alg),
            Self::TokenExpired => write!(f, "Token has expired"),
            Self::TokenNotYetValid => write!(f, "Token is not yet valid"),
            Self::InvalidIssuer { expected, actual } => {
                write!(f, "Invalid issuer: expected {}, got {:?}", expected, actual)
            }
            Self::InvalidAudience { expected, actual } => {
                write!(f, "Invalid audience: expected {}, got {:?}", expected, actual)
            }
            Self::InvalidSignature => write!(f, "Invalid signature"),
            Self::MissingKey => write!(f, "Missing key for signature validation"),
            Self::MissingExpiration => write!(f, "Token is missing required expiration claim"),
        }
    }
}

/// JWT Validator
#[derive(Debug)]
pub struct JwtValidator {
    /// HMAC secrets keyed by key ID
    hmac_secrets: HashMap<String, Vec<u8>>,
    /// RSA public keys keyed by key ID
    rsa_public_keys: HashMap<String, Vec<u8>>,
    /// Ed25519 public keys keyed by key ID
    ed25519_public_keys: HashMap<String, [u8; 32]>,
    /// Default key ID if none specified in token
    default_key_id: Option<String>,
    /// Validation configuration
    config: JwtValidationConfig,
}

impl JwtValidator {
    pub fn new(config: JwtValidationConfig) -> Self {
        Self {
            hmac_secrets: HashMap::new(),
            rsa_public_keys: HashMap::new(),
            ed25519_public_keys: HashMap::new(),
            default_key_id: None,
            config,
        }
    }

    /// Add HMAC secret
    pub fn add_hmac_secret(&mut self, key_id: &str, secret: &[u8]) {
        self.hmac_secrets.insert(key_id.to_string(), secret.to_vec());
        if self.default_key_id.is_none() {
            self.default_key_id = Some(key_id.to_string());
        }
    }

    /// Add Ed25519 public key
    pub fn add_ed25519_public_key(&mut self, key_id: &str, public_key: [u8; 32]) {
        self.ed25519_public_keys.insert(key_id.to_string(), public_key);
        if self.default_key_id.is_none() {
            self.default_key_id = Some(key_id.to_string());
        }
    }

    /// Extract token from Authorization header
    pub fn extract_token(auth_header: &str) -> Option<&str> {
        if auth_header.starts_with("Bearer ") {
            Some(&auth_header[7..])
        } else {
            None
        }
    }

    /// Decode JWT without verification
    pub fn decode(&self, token: &str) -> Result<DecodedJwt, JwtValidationError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(JwtValidationError::InvalidFormat);
        }

        // Decode header
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .map_err(|_| JwtValidationError::InvalidBase64)?;
        let header: JwtHeader =
            serde_json::from_slice(&header_bytes).map_err(|_| JwtValidationError::InvalidJson)?;

        // Decode claims
        let claims_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| JwtValidationError::InvalidBase64)?;
        let claims: JwtClaims =
            serde_json::from_slice(&claims_bytes).map_err(|_| JwtValidationError::InvalidJson)?;

        // Decode signature
        let signature = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|_| JwtValidationError::InvalidBase64)?;

        let signed_payload = format!("{}.{}", parts[0], parts[1]);

        Ok(DecodedJwt {
            header,
            claims,
            signature,
            signed_payload,
        })
    }

    /// Validate JWT claims (without signature verification)
    /// SECURITY FIX: exp claim is now required when validate_exp is enabled
    pub fn validate_claims(&self, claims: &JwtClaims) -> Result<(), JwtValidationError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Validate expiration
        // SECURITY FIX: exp claim is now REQUIRED when validate_exp is enabled
        // This prevents tokens without expiration from being accepted indefinitely
        if self.config.validate_exp {
            match claims.exp {
                Some(exp) => {
                    if now > exp + self.config.clock_skew_seconds {
                        return Err(JwtValidationError::TokenExpired);
                    }
                }
                None => {
                    // SECURITY FIX: Reject tokens without exp claim
                    return Err(JwtValidationError::MissingExpiration);
                }
            }
        }

        // Validate not before
        if self.config.validate_nbf {
            if let Some(nbf) = claims.nbf {
                if now + self.config.clock_skew_seconds < nbf {
                    return Err(JwtValidationError::TokenNotYetValid);
                }
            }
        }

        // Validate issuer
        if let Some(required_iss) = &self.config.required_issuer {
            match &claims.iss {
                Some(iss) if iss == required_iss => {}
                _ => {
                    return Err(JwtValidationError::InvalidIssuer {
                        expected: required_iss.clone(),
                        actual: claims.iss.clone(),
                    });
                }
            }
        }

        // Validate audience
        if let Some(required_aud) = &self.config.required_audience {
            match &claims.aud {
                Some(aud) if aud.contains(required_aud) => {}
                _ => {
                    return Err(JwtValidationError::InvalidAudience {
                        expected: required_aud.clone(),
                        actual: claims.aud.as_ref().map(|a| format!("{:?}", a)),
                    });
                }
            }
        }

        Ok(())
    }

    /// Verify HMAC signature using ring crate
    pub fn verify_hmac_signature(
        &self,
        jwt: &DecodedJwt,
    ) -> Result<(), JwtValidationError> {
        use ring::hmac;

        let key_id = jwt
            .header
            .kid
            .as_ref()
            .or(self.default_key_id.as_ref())
            .ok_or(JwtValidationError::MissingKey)?;

        let secret = self
            .hmac_secrets
            .get(key_id)
            .ok_or(JwtValidationError::MissingKey)?;

        let algorithm = match jwt.header.alg.as_str() {
            "HS256" => hmac::HMAC_SHA256,
            "HS384" => hmac::HMAC_SHA384,
            "HS512" => hmac::HMAC_SHA512,
            alg => return Err(JwtValidationError::UnsupportedAlgorithm(alg.to_string())),
        };

        let key = hmac::Key::new(algorithm, secret);
        hmac::verify(&key, jwt.signed_payload.as_bytes(), &jwt.signature)
            .map_err(|_| JwtValidationError::InvalidSignature)
    }

    /// Verify Ed25519 signature
    pub fn verify_ed25519_signature(
        &self,
        jwt: &DecodedJwt,
    ) -> Result<(), JwtValidationError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        if jwt.header.alg != "EdDSA" {
            return Err(JwtValidationError::UnsupportedAlgorithm(jwt.header.alg.clone()));
        }

        let key_id = jwt
            .header
            .kid
            .as_ref()
            .or(self.default_key_id.as_ref())
            .ok_or(JwtValidationError::MissingKey)?;

        let public_key_bytes = self
            .ed25519_public_keys
            .get(key_id)
            .ok_or(JwtValidationError::MissingKey)?;

        let public_key = VerifyingKey::from_bytes(public_key_bytes)
            .map_err(|_| JwtValidationError::MissingKey)?;

        let signature_bytes: [u8; 64] = jwt
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| JwtValidationError::InvalidSignature)?;

        let signature = Signature::from_bytes(&signature_bytes);

        public_key
            .verify(jwt.signed_payload.as_bytes(), &signature)
            .map_err(|_| JwtValidationError::InvalidSignature)
    }

    /// Full validation (decode + verify signature + validate claims)
    pub fn validate(&self, token: &str) -> Result<DecodedJwt, JwtValidationError> {
        let jwt = self.decode(token)?;

        // Verify signature based on algorithm
        match jwt.header.alg.as_str() {
            "HS256" | "HS384" | "HS512" => {
                self.verify_hmac_signature(&jwt)?;
            }
            "EdDSA" => {
                self.verify_ed25519_signature(&jwt)?;
            }
            alg => {
                return Err(JwtValidationError::UnsupportedAlgorithm(alg.to_string()));
            }
        }

        // Validate claims
        self.validate_claims(&jwt.claims)?;

        Ok(jwt)
    }
}

impl Default for JwtValidator {
    fn default() -> Self {
        Self::new(JwtValidationConfig::default())
    }
}

// ============================================
// Sequence Detection (Abuse Detection)
// ============================================

/// Login attempt record
#[derive(Debug, Clone)]
struct LoginAttempt {
    timestamp: Instant,
    success: bool,
    username_hash: u64,
}

/// Request sequence record for enumeration detection
#[derive(Debug, Clone)]
struct RequestSequence {
    timestamp: Instant,
    path: String,
    response_code: u16,
}

/// Abuse detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbuseDetectionResult {
    pub is_abuse: bool,
    pub abuse_type: Option<AbuseType>,
    pub confidence: f64,
    pub details: String,
}

/// Type of detected abuse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AbuseType {
    CredentialStuffing,
    AccountEnumeration,
    ApiScraping,
    BruteForce,
    SequentialProbing,
}

/// Sequence detector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDetectorConfig {
    /// Window size for tracking (seconds)
    pub window_seconds: u64,
    /// Max failed logins before flagging credential stuffing
    pub max_failed_logins: u32,
    /// Max sequential IDs before flagging enumeration
    pub max_sequential_ids: u32,
    /// Min requests to detect scraping pattern
    pub min_scraping_requests: u32,
    /// Scraping detection threshold (pagination pattern score)
    pub scraping_threshold: f64,
}

impl Default for SequenceDetectorConfig {
    fn default() -> Self {
        Self {
            window_seconds: 300, // 5 minutes
            max_failed_logins: 10,
            max_sequential_ids: 20,
            min_scraping_requests: 50,
            scraping_threshold: 0.7,
        }
    }
}

/// Sequence detector for abuse patterns
#[derive(Debug)]
pub struct SequenceDetector {
    /// Login attempts per IP
    login_attempts: Arc<RwLock<HashMap<String, Vec<LoginAttempt>>>>,
    /// Request sequences per IP
    request_sequences: Arc<RwLock<HashMap<String, Vec<RequestSequence>>>>,
    /// Detected numeric IDs per IP (for enumeration detection)
    detected_ids: Arc<RwLock<HashMap<String, Vec<i64>>>>,
    /// Configuration
    config: SequenceDetectorConfig,
}

impl SequenceDetector {
    pub fn new(config: SequenceDetectorConfig) -> Self {
        Self {
            login_attempts: Arc::new(RwLock::new(HashMap::new())),
            request_sequences: Arc::new(RwLock::new(HashMap::new())),
            detected_ids: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Record a login attempt
    pub async fn record_login(
        &self,
        ip: &str,
        username: &str,
        success: bool,
    ) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        username.hash(&mut hasher);
        let username_hash = hasher.finish();

        let attempt = LoginAttempt {
            timestamp: Instant::now(),
            success,
            username_hash,
        };

        let mut attempts = self.login_attempts.write().await;
        let ip_attempts = attempts.entry(ip.to_string()).or_insert_with(Vec::new);
        ip_attempts.push(attempt);

        // Cleanup old attempts
        let cutoff = Instant::now() - Duration::from_secs(self.config.window_seconds);
        ip_attempts.retain(|a| a.timestamp > cutoff);
    }

    /// Record a request for sequence analysis
    pub async fn record_request(
        &self,
        ip: &str,
        path: &str,
        response_code: u16,
    ) {
        let request = RequestSequence {
            timestamp: Instant::now(),
            path: path.to_string(),
            response_code,
        };

        // Record request
        {
            let mut sequences = self.request_sequences.write().await;
            let ip_sequences = sequences.entry(ip.to_string()).or_insert_with(Vec::new);
            ip_sequences.push(request);

            // Cleanup old requests
            let cutoff = Instant::now() - Duration::from_secs(self.config.window_seconds);
            ip_sequences.retain(|r| r.timestamp > cutoff);
        }

        // Extract numeric IDs from path for enumeration detection
        if let Some(id) = self.extract_numeric_id(path) {
            let mut ids = self.detected_ids.write().await;
            let ip_ids = ids.entry(ip.to_string()).or_insert_with(Vec::new);
            ip_ids.push(id);

            // Keep only recent IDs
            if ip_ids.len() > 1000 {
                ip_ids.drain(0..500);
            }
        }
    }

    fn extract_numeric_id(&self, path: &str) -> Option<i64> {
        // Extract numeric segments from path
        let re = Regex::new(r"/(\d+)(?:/|$|\?)").ok()?;
        re.captures(path)?
            .get(1)?
            .as_str()
            .parse()
            .ok()
    }

    /// Detect credential stuffing
    pub async fn detect_credential_stuffing(&self, ip: &str) -> AbuseDetectionResult {
        let attempts = self.login_attempts.read().await;

        if let Some(ip_attempts) = attempts.get(ip) {
            let cutoff = Instant::now() - Duration::from_secs(self.config.window_seconds);
            let recent: Vec<_> = ip_attempts
                .iter()
                .filter(|a| a.timestamp > cutoff)
                .collect();

            let failed_count = recent.iter().filter(|a| !a.success).count() as u32;
            let unique_usernames: std::collections::HashSet<_> =
                recent.iter().map(|a| a.username_hash).collect();

            // Credential stuffing: many failures with many different usernames
            if failed_count >= self.config.max_failed_logins {
                let confidence = if unique_usernames.len() as u32 >= failed_count / 2 {
                    0.9 // Many unique usernames = high confidence stuffing
                } else {
                    0.6 // Same username = might be brute force
                };

                return AbuseDetectionResult {
                    is_abuse: true,
                    abuse_type: Some(if unique_usernames.len() as u32 >= failed_count / 2 {
                        AbuseType::CredentialStuffing
                    } else {
                        AbuseType::BruteForce
                    }),
                    confidence,
                    details: format!(
                        "{} failed logins with {} unique usernames in {}s",
                        failed_count,
                        unique_usernames.len(),
                        self.config.window_seconds
                    ),
                };
            }
        }

        AbuseDetectionResult {
            is_abuse: false,
            abuse_type: None,
            confidence: 0.0,
            details: "No credential stuffing detected".to_string(),
        }
    }

    /// Detect account enumeration (sequential ID probing)
    pub async fn detect_enumeration(&self, ip: &str) -> AbuseDetectionResult {
        let ids = self.detected_ids.read().await;

        if let Some(ip_ids) = ids.get(ip) {
            if ip_ids.len() < 10 {
                return AbuseDetectionResult {
                    is_abuse: false,
                    abuse_type: None,
                    confidence: 0.0,
                    details: "Not enough data".to_string(),
                };
            }

            // Count sequential pairs
            let mut sequential_count = 0;
            let mut sorted_ids = ip_ids.clone();
            sorted_ids.sort();
            sorted_ids.dedup();

            for i in 1..sorted_ids.len() {
                if sorted_ids[i] - sorted_ids[i - 1] == 1 {
                    sequential_count += 1;
                }
            }

            // High proportion of sequential IDs indicates enumeration
            let sequential_ratio = sequential_count as f64 / (sorted_ids.len() - 1) as f64;

            if sequential_count >= self.config.max_sequential_ids as usize || sequential_ratio > 0.5
            {
                return AbuseDetectionResult {
                    is_abuse: true,
                    abuse_type: Some(AbuseType::AccountEnumeration),
                    confidence: sequential_ratio.min(1.0),
                    details: format!(
                        "{} sequential IDs detected ({:.1}% of requests)",
                        sequential_count,
                        sequential_ratio * 100.0
                    ),
                };
            }
        }

        AbuseDetectionResult {
            is_abuse: false,
            abuse_type: None,
            confidence: 0.0,
            details: "No enumeration pattern detected".to_string(),
        }
    }

    /// Detect API scraping (systematic pagination)
    pub async fn detect_scraping(&self, ip: &str) -> AbuseDetectionResult {
        let sequences = self.request_sequences.read().await;

        if let Some(ip_sequences) = sequences.get(ip) {
            if (ip_sequences.len() as u32) < self.config.min_scraping_requests {
                return AbuseDetectionResult {
                    is_abuse: false,
                    abuse_type: None,
                    confidence: 0.0,
                    details: "Not enough requests".to_string(),
                };
            }

            // Look for pagination patterns
            let page_pattern = Regex::new(r"[?&](page|offset|skip|cursor)=").unwrap();
            let pagination_requests: Vec<_> = ip_sequences
                .iter()
                .filter(|r| page_pattern.is_match(&r.path))
                .collect();

            let pagination_ratio =
                pagination_requests.len() as f64 / ip_sequences.len() as f64;

            // Look for systematic endpoint coverage
            let unique_base_paths: std::collections::HashSet<_> = ip_sequences
                .iter()
                .map(|r| {
                    r.path
                        .split('?')
                        .next()
                        .unwrap_or(&r.path)
                        .split('/')
                        .take(3)
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .collect();

            let coverage_score =
                unique_base_paths.len() as f64 / ip_sequences.len().min(100) as f64;

            // High pagination + high coverage = scraping
            let scraping_score = (pagination_ratio * 0.6) + (coverage_score * 0.4);

            if scraping_score >= self.config.scraping_threshold {
                return AbuseDetectionResult {
                    is_abuse: true,
                    abuse_type: Some(AbuseType::ApiScraping),
                    confidence: scraping_score.min(1.0),
                    details: format!(
                        "{:.1}% pagination requests, {} unique endpoints ({:.1}% coverage)",
                        pagination_ratio * 100.0,
                        unique_base_paths.len(),
                        coverage_score * 100.0
                    ),
                };
            }
        }

        AbuseDetectionResult {
            is_abuse: false,
            abuse_type: None,
            confidence: 0.0,
            details: "No scraping pattern detected".to_string(),
        }
    }

    /// Run all detection checks
    pub async fn analyze(&self, ip: &str) -> Vec<AbuseDetectionResult> {
        let mut results = Vec::new();

        let stuffing = self.detect_credential_stuffing(ip).await;
        if stuffing.is_abuse {
            results.push(stuffing);
        }

        let enumeration = self.detect_enumeration(ip).await;
        if enumeration.is_abuse {
            results.push(enumeration);
        }

        let scraping = self.detect_scraping(ip).await;
        if scraping.is_abuse {
            results.push(scraping);
        }

        results
    }

    /// Cleanup old data
    pub async fn cleanup(&self) {
        let cutoff = Instant::now() - Duration::from_secs(self.config.window_seconds);

        {
            let mut attempts = self.login_attempts.write().await;
            for (_, ip_attempts) in attempts.iter_mut() {
                ip_attempts.retain(|a| a.timestamp > cutoff);
            }
            attempts.retain(|_, v| !v.is_empty());
        }

        {
            let mut sequences = self.request_sequences.write().await;
            for (_, ip_sequences) in sequences.iter_mut() {
                ip_sequences.retain(|r| r.timestamp > cutoff);
            }
            sequences.retain(|_, v| !v.is_empty());
        }
    }
}

impl Default for SequenceDetector {
    fn default() -> Self {
        Self::new(SequenceDetectorConfig::default())
    }
}

// ============================================
// Per-Endpoint Rate Limiting
// ============================================

/// Rate limit rule for an endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRateLimit {
    /// Path pattern (supports glob: /api/*)
    pub path_pattern: String,
    /// HTTP methods this applies to (empty = all)
    pub methods: Vec<String>,
    /// Requests per window
    pub requests_per_window: u32,
    /// Window duration in seconds
    pub window_seconds: u32,
    /// Key type for rate limiting
    pub key_type: RateLimitKeyType,
    /// Adaptive threshold adjustment enabled
    pub adaptive: bool,
}

/// What to use as the rate limit key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitKeyType {
    /// Rate limit by IP address
    Ip,
    /// Rate limit by JWT subject (sub claim)
    JwtSubject,
    /// Rate limit by API key header
    ApiKey,
    /// Rate limit by custom header
    Header(String),
    /// Global rate limit (all requests)
    Global,
}

/// Rate limit check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub reset_seconds: u32,
    pub limit: u32,
}

/// Endpoint rate limiter with dynamic thresholds
#[derive(Debug)]
pub struct EndpointRateLimiter {
    /// Rate limit rules
    rules: Vec<EndpointRateLimit>,
    /// Request counts: (endpoint_pattern, key) -> (count, window_start)
    counts: Arc<RwLock<HashMap<(String, String), (u32, Instant)>>>,
    /// Compiled path patterns
    compiled_patterns: Vec<(String, Regex)>,
    /// Traffic statistics for adaptive thresholds
    traffic_stats: Arc<RwLock<HashMap<String, TrafficStats>>>,
}

/// Traffic statistics for adaptive rate limiting
#[derive(Debug, Clone, Default)]
struct TrafficStats {
    /// Total requests in current period
    total_requests: u64,
    /// Request timestamps for rate calculation
    recent_requests: Vec<Instant>,
    /// Error responses
    error_count: u64,
}

impl EndpointRateLimiter {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            counts: Arc::new(RwLock::new(HashMap::new())),
            compiled_patterns: Vec::new(),
            traffic_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a rate limit rule
    pub fn add_rule(&mut self, rule: EndpointRateLimit) {
        // Compile path pattern to regex
        let mut regex_pattern = rule.path_pattern.replace("*", ".*");
        regex_pattern = format!("^{}$", regex_pattern);

        if let Ok(regex) = Regex::new(&regex_pattern) {
            self.compiled_patterns
                .push((rule.path_pattern.clone(), regex));
        }

        self.rules.push(rule);
    }

    /// Load rules from YAML configuration
    pub fn load_config(&mut self, yaml: &str) -> Result<usize, String> {
        let rules: Vec<EndpointRateLimit> =
            serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
        let count = rules.len();

        for rule in rules {
            self.add_rule(rule);
        }

        Ok(count)
    }

    /// Find matching rule for a request
    fn find_rule(&self, method: &str, path: &str) -> Option<&EndpointRateLimit> {
        for (i, (_, regex)) in self.compiled_patterns.iter().enumerate() {
            let rule = &self.rules[i];

            // Check method
            if !rule.methods.is_empty()
                && !rule
                    .methods
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(method))
            {
                continue;
            }

            // Check path
            if regex.is_match(path) {
                return Some(rule);
            }
        }

        None
    }

    /// Extract rate limit key from request
    fn extract_key(
        &self,
        key_type: &RateLimitKeyType,
        ip: &str,
        headers: &HashMap<String, String>,
        jwt_subject: Option<&str>,
    ) -> String {
        match key_type {
            RateLimitKeyType::Ip => ip.to_string(),
            RateLimitKeyType::JwtSubject => jwt_subject.unwrap_or(ip).to_string(),
            RateLimitKeyType::ApiKey => headers
                .get("x-api-key")
                .or_else(|| headers.get("X-API-Key"))
                .cloned()
                .unwrap_or_else(|| ip.to_string()),
            RateLimitKeyType::Header(name) => headers
                .get(name)
                .or_else(|| headers.get(&name.to_lowercase()))
                .cloned()
                .unwrap_or_else(|| ip.to_string()),
            RateLimitKeyType::Global => "global".to_string(),
        }
    }

    /// Check rate limit for a request
    pub async fn check_rate_limit(
        &self,
        method: &str,
        path: &str,
        ip: &str,
        headers: &HashMap<String, String>,
        jwt_subject: Option<&str>,
    ) -> RateLimitResult {
        // Find matching rule
        let rule = match self.find_rule(method, path) {
            Some(r) => r,
            None => {
                return RateLimitResult {
                    allowed: true,
                    remaining: u32::MAX,
                    reset_seconds: 0,
                    limit: u32::MAX,
                };
            }
        };

        let key = self.extract_key(&rule.key_type, ip, headers, jwt_subject);
        let cache_key = (rule.path_pattern.clone(), key);
        let window_duration = Duration::from_secs(rule.window_seconds as u64);

        let mut counts = self.counts.write().await;
        let now = Instant::now();

        let (count, window_start) = counts
            .entry(cache_key)
            .or_insert((0, now));

        // Check if window expired
        if now.duration_since(*window_start) > window_duration {
            *count = 0;
            *window_start = now;
        }

        // Calculate effective limit (adaptive if enabled)
        let effective_limit = if rule.adaptive {
            self.calculate_adaptive_limit(rule, &rule.path_pattern).await
        } else {
            rule.requests_per_window
        };

        // Check limit
        if *count >= effective_limit {
            let reset = window_duration
                .checked_sub(now.duration_since(*window_start))
                .unwrap_or_default()
                .as_secs() as u32;

            return RateLimitResult {
                allowed: false,
                remaining: 0,
                reset_seconds: reset,
                limit: effective_limit,
            };
        }

        // Increment count
        *count += 1;

        let reset = window_duration
            .checked_sub(now.duration_since(*window_start))
            .unwrap_or_default()
            .as_secs() as u32;

        RateLimitResult {
            allowed: true,
            remaining: effective_limit.saturating_sub(*count),
            reset_seconds: reset,
            limit: effective_limit,
        }
    }

    /// Calculate adaptive limit based on traffic patterns
    async fn calculate_adaptive_limit(
        &self,
        rule: &EndpointRateLimit,
        endpoint: &str,
    ) -> u32 {
        let stats = self.traffic_stats.read().await;

        if let Some(traffic) = stats.get(endpoint) {
            // If high error rate, reduce limit
            let error_rate = if traffic.total_requests > 0 {
                traffic.error_count as f64 / traffic.total_requests as f64
            } else {
                0.0
            };

            if error_rate > 0.5 {
                // High error rate - reduce limit by 50%
                return rule.requests_per_window / 2;
            } else if error_rate > 0.2 {
                // Medium error rate - reduce limit by 25%
                return (rule.requests_per_window as f64 * 0.75) as u32;
            }

            // Calculate current request rate
            let recent_count = traffic.recent_requests.len();
            if recent_count > 100 {
                // High traffic - might want to be more restrictive
                let base = rule.requests_per_window;
                return (base as f64 * 0.9) as u32;
            }
        }

        rule.requests_per_window
    }

    /// Record traffic for adaptive rate limiting
    pub async fn record_traffic(&self, endpoint: &str, is_error: bool) {
        let mut stats = self.traffic_stats.write().await;
        let traffic = stats.entry(endpoint.to_string()).or_default();

        traffic.total_requests += 1;
        traffic.recent_requests.push(Instant::now());

        if is_error {
            traffic.error_count += 1;
        }

        // Keep only recent requests (last 60 seconds)
        let cutoff = Instant::now() - Duration::from_secs(60);
        traffic.recent_requests.retain(|t| *t > cutoff);
    }

    /// Cleanup old entries
    pub async fn cleanup(&self) {
        let mut counts = self.counts.write().await;
        let cutoff = Instant::now() - Duration::from_secs(3600); // 1 hour

        counts.retain(|_, (_, start)| *start > cutoff);
    }

    /// Get default rate limit rules for common API endpoints
    pub fn default_rules() -> Vec<EndpointRateLimit> {
        vec![
            // Login endpoints - strict limits
            EndpointRateLimit {
                path_pattern: "/login".to_string(),
                methods: vec!["POST".to_string()],
                requests_per_window: 5,
                window_seconds: 60,
                key_type: RateLimitKeyType::Ip,
                adaptive: true,
            },
            EndpointRateLimit {
                path_pattern: "/api/*/login".to_string(),
                methods: vec!["POST".to_string()],
                requests_per_window: 5,
                window_seconds: 60,
                key_type: RateLimitKeyType::Ip,
                adaptive: true,
            },
            // Password reset
            EndpointRateLimit {
                path_pattern: "/api/*/password-reset".to_string(),
                methods: vec!["POST".to_string()],
                requests_per_window: 3,
                window_seconds: 300,
                key_type: RateLimitKeyType::Ip,
                adaptive: false,
            },
            // User endpoints - moderate limits per token
            EndpointRateLimit {
                path_pattern: "/api/*/users*".to_string(),
                methods: vec![],
                requests_per_window: 100,
                window_seconds: 60,
                key_type: RateLimitKeyType::JwtSubject,
                adaptive: true,
            },
            // Generic API - standard limits
            EndpointRateLimit {
                path_pattern: "/api/*".to_string(),
                methods: vec![],
                requests_per_window: 1000,
                window_seconds: 60,
                key_type: RateLimitKeyType::JwtSubject,
                adaptive: true,
            },
        ]
    }
}

impl Default for EndpointRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// Combined API Security Engine
// ============================================

/// API Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSecurityConfig {
    /// Enable API discovery
    pub enable_discovery: bool,
    /// Enable schema validation
    pub enable_schema_validation: bool,
    /// Enable JWT validation
    pub enable_jwt_validation: bool,
    /// Enable abuse detection
    pub enable_abuse_detection: bool,
    /// Enable rate limiting
    pub enable_rate_limiting: bool,
    /// Block unknown endpoints (shadow API protection)
    pub block_unknown_endpoints: bool,
    /// JWT validation config
    pub jwt_config: JwtValidationConfig,
    /// Sequence detector config
    pub sequence_config: SequenceDetectorConfig,
}

impl Default for ApiSecurityConfig {
    fn default() -> Self {
        Self {
            enable_discovery: true,
            enable_schema_validation: true,
            enable_jwt_validation: true,
            enable_abuse_detection: true,
            enable_rate_limiting: true,
            block_unknown_endpoints: false, // Don't block by default
            jwt_config: JwtValidationConfig::default(),
            sequence_config: SequenceDetectorConfig::default(),
        }
    }
}

/// API Security check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSecurityResult {
    pub allowed: bool,
    pub action: ApiSecurityAction,
    pub schema_errors: Vec<ValidationError>,
    pub jwt_error: Option<String>,
    pub abuse_detected: Vec<AbuseDetectionResult>,
    pub rate_limit: Option<RateLimitResult>,
}

/// Action to take based on security check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiSecurityAction {
    Allow,
    Block { reason: String },
    RateLimit { retry_after: u32 },
    Challenge,
}

/// Combined API Security Engine
pub struct ApiSecurityEngine {
    pub config: ApiSecurityConfig,
    pub discovery: ApiDiscovery,
    pub schema_validator: SchemaValidator,
    pub jwt_validator: JwtValidator,
    pub sequence_detector: SequenceDetector,
    pub rate_limiter: EndpointRateLimiter,
}

impl ApiSecurityEngine {
    pub fn new(config: ApiSecurityConfig) -> Self {
        let jwt_validator = JwtValidator::new(config.jwt_config.clone());
        let sequence_detector = SequenceDetector::new(config.sequence_config.clone());

        Self {
            config,
            discovery: ApiDiscovery::new(),
            schema_validator: SchemaValidator::new(),
            jwt_validator,
            sequence_detector,
            rate_limiter: EndpointRateLimiter::new(),
        }
    }

    /// Process an API request through all security checks
    pub async fn check_request(
        &self,
        method: &str,
        path: &str,
        ip: &str,
        headers: &HashMap<String, String>,
        query_params: &HashMap<String, String>,
        body: Option<&str>,
    ) -> ApiSecurityResult {
        let mut result = ApiSecurityResult {
            allowed: true,
            action: ApiSecurityAction::Allow,
            schema_errors: Vec::new(),
            jwt_error: None,
            abuse_detected: Vec::new(),
            rate_limit: None,
        };

        let mut jwt_subject: Option<String> = None;

        // 1. JWT Validation
        if self.config.enable_jwt_validation {
            if let Some(auth_header) = headers.get("authorization").or(headers.get("Authorization"))
            {
                if let Some(token) = JwtValidator::extract_token(auth_header) {
                    match self.jwt_validator.validate(token) {
                        Ok(jwt) => {
                            jwt_subject = jwt.claims.sub.clone();
                        }
                        Err(e) => {
                            result.jwt_error = Some(e.to_string());
                            result.allowed = false;
                            result.action = ApiSecurityAction::Block {
                                reason: format!("JWT validation failed: {}", e),
                            };
                            return result;
                        }
                    }
                }
            }
        }

        // 2. Rate Limiting
        if self.config.enable_rate_limiting {
            let rate_result = self
                .rate_limiter
                .check_rate_limit(
                    method,
                    path,
                    ip,
                    headers,
                    jwt_subject.as_deref(),
                )
                .await;

            result.rate_limit = Some(rate_result.clone());

            if !rate_result.allowed {
                result.allowed = false;
                result.action = ApiSecurityAction::RateLimit {
                    retry_after: rate_result.reset_seconds,
                };
                return result;
            }
        }

        // 3. Schema Validation
        if self.config.enable_schema_validation {
            let validation = self.schema_validator.validate_request(
                method,
                path,
                query_params,
                headers,
                body,
            );

            if !validation.valid {
                result.schema_errors = validation.errors.clone();

                // Check if unknown endpoint should be blocked
                let is_unknown = validation.errors.iter().any(|e| e.message == "Unknown endpoint");
                if !is_unknown || self.config.block_unknown_endpoints {
                    result.allowed = false;
                    result.action = ApiSecurityAction::Block {
                        reason: "Schema validation failed".to_string(),
                    };
                    return result;
                }
            }
        }

        // 4. Abuse Detection
        if self.config.enable_abuse_detection {
            let abuse_results = self.sequence_detector.analyze(ip).await;

            if !abuse_results.is_empty() {
                result.abuse_detected = abuse_results.clone();

                // High confidence abuse should be blocked
                let high_confidence = abuse_results
                    .iter()
                    .any(|a| a.is_abuse && a.confidence > 0.8);

                if high_confidence {
                    result.allowed = false;
                    result.action = ApiSecurityAction::Block {
                        reason: format!(
                            "Abuse detected: {:?}",
                            abuse_results
                                .iter()
                                .filter_map(|a| a.abuse_type.clone())
                                .collect::<Vec<_>>()
                        ),
                    };
                    return result;
                }

                // Medium confidence - challenge
                let medium_confidence = abuse_results
                    .iter()
                    .any(|a| a.is_abuse && a.confidence > 0.5);

                if medium_confidence {
                    result.action = ApiSecurityAction::Challenge;
                }
            }
        }

        // 5. Record for API Discovery
        if self.config.enable_discovery {
            let query_param_names: Vec<String> = query_params.keys().cloned().collect();
            let header_names: Vec<String> = headers.keys().cloned().collect();

            self.discovery
                .record_request(
                    method,
                    path,
                    &query_param_names,
                    &header_names,
                    0.0, // Would be filled with actual response time
                    false,
                )
                .await;
        }

        result
    }

    /// Record login attempt for abuse detection
    pub async fn record_login(&self, ip: &str, username: &str, success: bool) {
        self.sequence_detector.record_login(ip, username, success).await;
    }

    /// Record request for sequence analysis
    pub async fn record_request(&self, ip: &str, path: &str, response_code: u16) {
        self.sequence_detector.record_request(ip, path, response_code).await;
    }
}

impl Default for ApiSecurityEngine {
    fn default() -> Self {
        Self::new(ApiSecurityConfig::default())
    }
}

// ============================================
// Tests
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    // API Discovery Tests

    #[test]
    fn test_path_normalization() {
        let discovery = ApiDiscovery::new();

        assert_eq!(
            discovery.normalize_path("/api/users/123"),
            "/api/users/{id}"
        );
        assert_eq!(
            discovery.normalize_path("/api/users/550e8400-e29b-41d4-a716-446655440000"),
            "/api/users/{uuid}"
        );
        assert_eq!(
            discovery.normalize_path("/api/products"),
            "/api/products"
        );
    }

    #[tokio::test]
    async fn test_api_discovery_recording() {
        let discovery = ApiDiscovery::new();

        discovery
            .record_request(
                "GET",
                "/api/users/123",
                &["page".to_string()],
                &["Authorization".to_string()],
                50.0,
                false,
            )
            .await;

        discovery
            .record_request(
                "GET",
                "/api/users/456",
                &["page".to_string()],
                &["Authorization".to_string()],
                60.0,
                false,
            )
            .await;

        let endpoints = discovery.get_endpoints().await;
        assert_eq!(endpoints.len(), 1);

        let endpoint = &endpoints[0];
        assert_eq!(endpoint.method, "GET");
        assert_eq!(endpoint.path_pattern, "/api/users/{id}");
        assert_eq!(endpoint.request_count, 2);
        assert_eq!(endpoint.avg_response_time_ms, 55.0);
    }

    #[tokio::test]
    async fn test_unknown_endpoint_detection() {
        let discovery = ApiDiscovery::new();

        // Record known endpoint
        discovery
            .record_request("GET", "/api/users/123", &[], &[], 50.0, false)
            .await;

        assert!(discovery.is_known_endpoint("GET", "/api/users/456").await);
        assert!(!discovery.is_known_endpoint("POST", "/api/users/456").await);
        assert!(!discovery.is_known_endpoint("GET", "/api/products").await);
    }

    // Schema Validation Tests

    #[test]
    fn test_openapi_spec_loading() {
        let mut validator = SchemaValidator::new();

        let spec = r#"{
            "openapi": "3.0.0",
            "paths": {
                "/users/{id}": {
                    "get": {
                        "parameters": [
                            {
                                "name": "id",
                                "in": "path",
                                "required": true,
                                "schema": {"type": "integer"}
                            }
                        ]
                    }
                }
            }
        }"#;

        let count = validator.load_openapi_spec(spec).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_validation() {
        let mut validator = SchemaValidator::new();

        let spec = r#"{
            "openapi": "3.0.0",
            "paths": {
                "/users": {
                    "post": {
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["name", "email"],
                                        "properties": {
                                            "name": {"type": "string"},
                                            "email": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }"#;

        validator.load_openapi_spec(spec).unwrap();

        // Valid request
        let result = validator.validate_request(
            "POST",
            "/users",
            &HashMap::new(),
            &HashMap::new(),
            Some(r#"{"name": "John", "email": "john@example.com"}"#),
        );
        assert!(result.valid);

        // Missing required field
        let result = validator.validate_request(
            "POST",
            "/users",
            &HashMap::new(),
            &HashMap::new(),
            Some(r#"{"name": "John"}"#),
        );
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("Required property")));
    }

    // JWT Validation Tests

    #[test]
    fn test_jwt_decode() {
        let validator = JwtValidator::new(JwtValidationConfig::default());

        // Valid JWT structure (not signature-verified)
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

        let result = validator.decode(token);
        assert!(result.is_ok());

        let jwt = result.unwrap();
        assert_eq!(jwt.header.alg, "HS256");
        assert_eq!(jwt.claims.sub, Some("1234567890".to_string()));
    }

    #[test]
    fn test_jwt_claims_validation() {
        let config = JwtValidationConfig {
            required_issuer: Some("https://auth.example.com".to_string()),
            required_audience: Some("api".to_string()),
            clock_skew_seconds: 60,
            validate_exp: true,
            validate_nbf: true,
        };
        let validator = JwtValidator::new(config);

        // Valid claims
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = JwtClaims {
            sub: Some("user123".to_string()),
            iss: Some("https://auth.example.com".to_string()),
            aud: Some(StringOrVec::String("api".to_string())),
            exp: Some(now + 3600),
            nbf: Some(now - 60),
            iat: Some(now),
            jti: None,
            extra: HashMap::new(),
        };

        assert!(validator.validate_claims(&claims).is_ok());

        // Expired token
        let expired_claims = JwtClaims {
            exp: Some(now - 120),
            ..claims.clone()
        };
        assert!(matches!(
            validator.validate_claims(&expired_claims),
            Err(JwtValidationError::TokenExpired)
        ));

        // Wrong issuer
        let wrong_iss = JwtClaims {
            iss: Some("https://wrong.com".to_string()),
            ..claims.clone()
        };
        assert!(matches!(
            validator.validate_claims(&wrong_iss),
            Err(JwtValidationError::InvalidIssuer { .. })
        ));
    }

    #[test]
    fn test_hmac_signature_verification() {
        let mut validator = JwtValidator::new(JwtValidationConfig::default());
        validator.add_hmac_secret("default", b"your-256-bit-secret");

        // This token is signed with "your-256-bit-secret"
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

        let jwt = validator.decode(token).unwrap();
        assert!(validator.verify_hmac_signature(&jwt).is_ok());
    }

    // Sequence Detection Tests

    #[tokio::test]
    async fn test_credential_stuffing_detection() {
        let detector = SequenceDetector::new(SequenceDetectorConfig {
            max_failed_logins: 5,
            ..Default::default()
        });

        // Simulate credential stuffing
        for i in 0..10 {
            detector
                .record_login("192.168.1.1", &format!("user{}", i), false)
                .await;
        }

        let result = detector.detect_credential_stuffing("192.168.1.1").await;
        assert!(result.is_abuse);
        assert!(matches!(result.abuse_type, Some(AbuseType::CredentialStuffing)));
    }

    #[tokio::test]
    async fn test_enumeration_detection() {
        let detector = SequenceDetector::new(SequenceDetectorConfig {
            max_sequential_ids: 10,
            ..Default::default()
        });

        // Simulate sequential ID probing
        for i in 1..=30 {
            detector
                .record_request("192.168.1.1", &format!("/api/users/{}", i), 200)
                .await;
        }

        let result = detector.detect_enumeration("192.168.1.1").await;
        assert!(result.is_abuse);
        assert!(matches!(result.abuse_type, Some(AbuseType::AccountEnumeration)));
    }

    // Rate Limiting Tests

    #[tokio::test]
    async fn test_endpoint_rate_limiting() {
        let mut limiter = EndpointRateLimiter::new();
        limiter.add_rule(EndpointRateLimit {
            path_pattern: "/api/test".to_string(),
            methods: vec![],
            requests_per_window: 5,
            window_seconds: 60,
            key_type: RateLimitKeyType::Ip,
            adaptive: false,
        });

        let headers = HashMap::new();

        // First 5 requests should pass
        for i in 0..5 {
            let result = limiter
                .check_rate_limit("GET", "/api/test", "192.168.1.1", &headers, None)
                .await;
            assert!(result.allowed, "Request {} should be allowed", i);
            assert_eq!(result.remaining, 4 - i as u32);
        }

        // 6th request should be blocked
        let result = limiter
            .check_rate_limit("GET", "/api/test", "192.168.1.1", &headers, None)
            .await;
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    #[tokio::test]
    async fn test_rate_limit_different_keys() {
        let mut limiter = EndpointRateLimiter::new();
        limiter.add_rule(EndpointRateLimit {
            path_pattern: "/api/test".to_string(),
            methods: vec![],
            requests_per_window: 2,
            window_seconds: 60,
            key_type: RateLimitKeyType::Ip,
            adaptive: false,
        });

        let headers = HashMap::new();

        // Different IPs should have separate limits
        for _ in 0..2 {
            let result = limiter
                .check_rate_limit("GET", "/api/test", "192.168.1.1", &headers, None)
                .await;
            assert!(result.allowed);
        }

        // IP 1 should be rate limited
        let result = limiter
            .check_rate_limit("GET", "/api/test", "192.168.1.1", &headers, None)
            .await;
        assert!(!result.allowed);

        // IP 2 should still be allowed
        let result = limiter
            .check_rate_limit("GET", "/api/test", "192.168.1.2", &headers, None)
            .await;
        assert!(result.allowed);
    }

    // Combined API Security Engine Tests

    #[tokio::test]
    async fn test_api_security_engine() {
        let config = ApiSecurityConfig {
            enable_jwt_validation: false, // Disable for this test
            ..Default::default()
        };
        let engine = ApiSecurityEngine::new(config);

        let headers = HashMap::new();
        let query_params = HashMap::new();

        let result = engine
            .check_request("GET", "/api/users", "192.168.1.1", &headers, &query_params, None)
            .await;

        assert!(result.allowed);
        assert!(matches!(result.action, ApiSecurityAction::Allow));
    }

    #[test]
    fn test_default_rate_limit_rules() {
        let rules = EndpointRateLimiter::default_rules();
        assert!(!rules.is_empty());

        // Should have login rule
        assert!(rules.iter().any(|r| r.path_pattern.contains("login")));
    }
}
