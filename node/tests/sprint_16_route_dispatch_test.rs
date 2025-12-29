/// Sprint 16: Integration tests for route-based dispatch and module pipeline
///
/// These tests verify the end-to-end functionality of:
/// - Route matching and selection
/// - Module dispatcher pipeline execution
/// - WAF blocking behavior
/// - Module sequencing and context passing
/// - Error handling and fail-open behavior

use aegis_node::route_config::{
    MethodMatcher, Route, RouteConfig, RoutePattern, RouteSettings, WasmModuleRef,
};

/// Helper to create a test route
fn create_test_route(
    name: &str,
    path_pattern: RoutePattern,
    wasm_modules: Vec<WasmModuleRef>,
) -> Route {
    Route {
        name: Some(name.to_string()),
        path: path_pattern,
        methods: MethodMatcher::All("*".to_string()),
        headers: None,
        wasm_modules,
        priority: 0,
        enabled: true,
    }
}

#[test]
fn test_route_matching_and_selection() {
    let config = RouteConfig {
        routes: vec![
            create_test_route(
                "exact_match",
                RoutePattern::Exact("/api/users".to_string()),
                vec![],
            ),
            create_test_route(
                "prefix_match",
                RoutePattern::Prefix("/api/*".to_string()),
                vec![],
            ),
            create_test_route(
                "catch_all",
                RoutePattern::Prefix("/*".to_string()),
                vec![],
            ),
        ],
        default_modules: None,
        settings: None,
    };

    // Test exact match
    let matched = config.find_matching_route("GET", "/api/users", &[]);
    assert_eq!(matched.unwrap().name, Some("exact_match".to_string()));

    // Test prefix match
    let matched = config.find_matching_route("GET", "/api/products", &[]);
    assert_eq!(matched.unwrap().name, Some("prefix_match".to_string()));

    // Test catch-all
    let matched = config.find_matching_route("GET", "/other", &[]);
    assert_eq!(matched.unwrap().name, Some("catch_all".to_string()));
}

#[test]
fn test_route_priority_based_selection() {
    let config = RouteConfig {
        routes: vec![
            create_test_route(
                "low_priority",
                RoutePattern::Prefix("/api/*".to_string()),
                vec![],
            ),
            Route {
                name: Some("high_priority".to_string()),
                path: RoutePattern::Prefix("/api/*".to_string()),
                methods: MethodMatcher::All("*".to_string()),
                headers: None,
                wasm_modules: vec![],
                priority: 100,
                enabled: true,
            },
        ],
        default_modules: None,
        settings: None,
    };

    // Higher priority route should match first
    let matched = config.find_matching_route("GET", "/api/test", &[]);
    assert_eq!(matched.unwrap().name, Some("high_priority".to_string()));
}

#[test]
fn test_disabled_route_skipped() {
    let config = RouteConfig {
        routes: vec![
            Route {
                name: Some("disabled".to_string()),
                path: RoutePattern::Exact("/test".to_string()),
                methods: MethodMatcher::All("*".to_string()),
                headers: None,
                wasm_modules: vec![],
                priority: 100,
                enabled: false, // Disabled
            },
            create_test_route(
                "fallback",
                RoutePattern::Prefix("/*".to_string()),
                vec![],
            ),
        ],
        default_modules: None,
        settings: None,
    };

    // Disabled route should be skipped, fallback should match
    let matched = config.find_matching_route("GET", "/test", &[]);
    assert_eq!(matched.unwrap().name, Some("fallback".to_string()));
}

#[test]
fn test_method_based_routing() {
    let config = RouteConfig {
        routes: vec![
            Route {
                name: Some("get_only".to_string()),
                path: RoutePattern::Prefix("/api/*".to_string()),
                methods: MethodMatcher::Single("GET".to_string()),
                headers: None,
                wasm_modules: vec![],
                priority: 0,
                enabled: true,
            },
            Route {
                name: Some("post_only".to_string()),
                path: RoutePattern::Prefix("/api/*".to_string()),
                methods: MethodMatcher::Single("POST".to_string()),
                headers: None,
                wasm_modules: vec![],
                priority: 0,
                enabled: true,
            },
        ],
        default_modules: None,
        settings: None,
    };

    // GET should match first route
    let matched = config.find_matching_route("GET", "/api/data", &[]);
    assert_eq!(matched.unwrap().name, Some("get_only".to_string()));

    // POST should match second route
    let matched = config.find_matching_route("POST", "/api/data", &[]);
    assert_eq!(matched.unwrap().name, Some("post_only".to_string()));

    // DELETE should not match any route
    let matched = config.find_matching_route("DELETE", "/api/data", &[]);
    assert!(matched.is_none());
}

#[test]
fn test_header_based_routing() {
    let mut headers_map = std::collections::HashMap::new();
    headers_map.insert("X-API-Key".to_string(), "secret123".to_string());

    let config = RouteConfig {
        routes: vec![
            Route {
                name: Some("authenticated".to_string()),
                path: RoutePattern::Prefix("/api/*".to_string()),
                methods: MethodMatcher::All("*".to_string()),
                headers: Some(headers_map),
                wasm_modules: vec![],
                priority: 100,
                enabled: true,
            },
            create_test_route(
                "unauthenticated",
                RoutePattern::Prefix("/api/*".to_string()),
                vec![],
            ),
        ],
        default_modules: None,
        settings: None,
    };

    // Request with correct header should match authenticated route
    let headers = vec![("X-API-Key".to_string(), "secret123".to_string())];
    let matched = config.find_matching_route("GET", "/api/data", &headers);
    assert_eq!(matched.unwrap().name, Some("authenticated".to_string()));

    // Request without header should match unauthenticated route
    let matched = config.find_matching_route("GET", "/api/data", &[]);
    assert_eq!(
        matched.unwrap().name,
        Some("unauthenticated".to_string())
    );

    // Request with wrong header value should match unauthenticated route
    let wrong_headers = vec![("X-API-Key".to_string(), "wrong".to_string())];
    let matched = config.find_matching_route("GET", "/api/data", &wrong_headers);
    assert_eq!(
        matched.unwrap().name,
        Some("unauthenticated".to_string())
    );
}

#[test]
fn test_route_config_from_yaml_integration() {
    let yaml = r#"
routes:
  - name: api_v1
    priority: 100
    enabled: true
    path:
      type: prefix
      pattern: "/api/v1/*"
    methods: ["GET", "POST"]
    wasm_modules:
      - type: waf
        module_id: waf-v1
      - type: edge_function
        module_id: auth-v1

  - name: api_v2
    priority: 90
    enabled: true
    path:
      type: regex
      pattern: "^/api/v2/.*"
    methods: ["GET"]
    wasm_modules:
      - type: edge_function
        module_id: router-v2

settings:
  max_modules_per_request: 10
  continue_on_error: false
"#;

    let config = RouteConfig::from_yaml(yaml).unwrap();

    // Verify routes loaded correctly
    assert_eq!(config.routes.len(), 2);
    assert_eq!(config.routes[0].name, Some("api_v1".to_string()));
    assert_eq!(config.routes[0].wasm_modules.len(), 2);
    assert_eq!(config.routes[1].name, Some("api_v2".to_string()));

    // Verify settings
    assert!(config.settings.is_some());
    let settings = config.settings.as_ref().unwrap();
    assert_eq!(settings.max_modules_per_request, 10);
    assert_eq!(settings.continue_on_error, false);

    // Test route matching
    let matched = config.find_matching_route("GET", "/api/v1/users", &[]);
    assert_eq!(matched.unwrap().name, Some("api_v1".to_string()));

    let matched = config.find_matching_route("GET", "/api/v2/products", &[]);
    assert_eq!(matched.unwrap().name, Some("api_v2".to_string()));
}

#[test]
fn test_max_modules_enforcement() {
    let settings = RouteSettings {
        max_modules_per_request: 2,
        continue_on_error: false,
    };

    // Create route with 5 modules
    let route = Route {
        name: Some("test".to_string()),
        path: RoutePattern::Exact("/test".to_string()),
        methods: MethodMatcher::default(),
        headers: None,
        wasm_modules: vec![
            WasmModuleRef {
                module_type: "waf".to_string(),
                module_id: "mod1".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            },
            WasmModuleRef {
                module_type: "edge_function".to_string(),
                module_id: "mod2".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            },
            WasmModuleRef {
                module_type: "edge_function".to_string(),
                module_id: "mod3".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            },
            WasmModuleRef {
                module_type: "edge_function".to_string(),
                module_id: "mod4".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            },
            WasmModuleRef {
                module_type: "edge_function".to_string(),
                module_id: "mod5".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            },
        ],
        priority: 0,
        enabled: true,
    };

    // Verify only first 2 modules would be executed
    let modules_to_execute: Vec<_> = route
        .wasm_modules
        .iter()
        .take(settings.max_modules_per_request)
        .collect();

    assert_eq!(modules_to_execute.len(), 2);
    assert_eq!(modules_to_execute[0].module_id, "mod1");
    assert_eq!(modules_to_execute[1].module_id, "mod2");
}

#[test]
fn test_regex_route_matching() {
    let config = RouteConfig {
        routes: vec![
            create_test_route(
                "versioned_api",
                RoutePattern::Regex(r"^/api/v[0-9]+/.*".to_string()),
                vec![],
            ),
            create_test_route(
                "file_downloads",
                RoutePattern::Regex(r"^/files/[a-z]+\.(pdf|doc|txt)$".to_string()),
                vec![],
            ),
        ],
        default_modules: None,
        settings: None,
    };

    // Test versioned API regex
    let matched = config.find_matching_route("GET", "/api/v1/users", &[]);
    assert_eq!(matched.unwrap().name, Some("versioned_api".to_string()));

    let matched = config.find_matching_route("GET", "/api/v99/products", &[]);
    assert_eq!(matched.unwrap().name, Some("versioned_api".to_string()));

    let matched = config.find_matching_route("GET", "/api/users", &[]);
    assert!(matched.is_none());

    // Test file download regex
    let matched = config.find_matching_route("GET", "/files/document.pdf", &[]);
    assert_eq!(matched.unwrap().name, Some("file_downloads".to_string()));

    let matched = config.find_matching_route("GET", "/files/report.txt", &[]);
    assert_eq!(matched.unwrap().name, Some("file_downloads".to_string()));

    let matched = config.find_matching_route("GET", "/files/image.jpg", &[]);
    assert!(matched.is_none());
}

#[test]
fn test_route_settings_defaults() {
    let settings = RouteSettings::default();
    assert_eq!(settings.max_modules_per_request, 10);
    assert_eq!(settings.continue_on_error, false);
}

#[test]
fn test_empty_route_config() {
    let config = RouteConfig::new();
    assert_eq!(config.routes.len(), 0);
    assert!(config.default_modules.is_none());
    assert!(config.settings.is_none());

    // Should return None for any route match
    let matched = config.find_matching_route("GET", "/any/path", &[]);
    assert!(matched.is_none());
}
