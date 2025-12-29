/// Sprint 15 Integration Tests: Edge Function Integration with Pingora & Request Manipulation
///
/// This test file demonstrates the complete Sprint 15 functionality:
/// 1. Request context access (method, URI, headers, body)
/// 2. Response manipulation (status, headers, body)
/// 3. Early termination
///
/// These tests use inline WAT (WebAssembly Text Format) to create test modules
/// that exercise all the new host functions.

use aegis_node::wasm_runtime::{WasmRuntime, WasmModuleType, WasmExecutionContext, EdgeFunctionResult};
use anyhow::Result;

/// Helper function to create a simple WAT module that tests request context access
fn create_request_context_test_wat() -> String {
    r#"
    (module
        (import "env" "request_get_method" (func $request_get_method (result i32)))
        (import "env" "request_get_uri" (func $request_get_uri (result i32)))
        (import "env" "request_get_header" (func $request_get_header (param i32 i32) (result i32)))
        (import "env" "request_get_header_names" (func $request_get_header_names (result i32)))
        (import "env" "request_get_body" (func $request_get_body (result i32)))
        (import "env" "get_shared_buffer" (func $get_shared_buffer (param i32 i32 i32) (result i32)))
        (import "env" "log" (func $log (param i32 i32)))

        (memory (export "memory") 1)

        (func (export "test_request_context") (result i32)
            ;; Test that request context functions return non-negative values
            ;; This indicates they successfully accessed the context

            ;; Test request_get_method
            (call $request_get_method)
            (i32.const 0)
            (i32.ge_s)
            (if (result i32)
                (then
                    ;; Success: method was retrieved
                    (i32.const 0)
                )
                (else
                    ;; Failure
                    (i32.const -1)
                )
            )
        )
    )
    "#.to_string()
}

/// Helper function to create a WAT module that tests response manipulation
fn create_response_manipulation_test_wat() -> String {
    r#"
    (module
        (import "env" "response_set_status" (func $response_set_status (param i32) (result i32)))
        (import "env" "response_set_header" (func $response_set_header (param i32 i32 i32 i32) (result i32)))
        (import "env" "response_add_header" (func $response_add_header (param i32 i32 i32 i32) (result i32)))
        (import "env" "response_remove_header" (func $response_remove_header (param i32 i32) (result i32)))
        (import "env" "response_set_body" (func $response_set_body (param i32 i32) (result i32)))

        (memory (export "memory") 1)

        ;; Data for header name and value
        (data (i32.const 0) "X-Custom-Header")
        (data (i32.const 20) "CustomValue")
        (data (i32.const 40) "Test Body")

        (func (export "test_response_manipulation") (result i32)
            ;; Set response status to 200
            (call $response_set_status (i32.const 200))
            drop

            ;; Set a custom header: X-Custom-Header: CustomValue
            (call $response_set_header
                (i32.const 0)   ;; header name ptr
                (i32.const 15)  ;; header name len
                (i32.const 20)  ;; header value ptr
                (i32.const 11)  ;; header value len
            )
            drop

            ;; Set response body: "Test Body"
            (call $response_set_body
                (i32.const 40)  ;; body ptr
                (i32.const 9)   ;; body len
            )
            drop

            ;; Return success
            (i32.const 0)
        )
    )
    "#.to_string()
}

/// Helper function to create a WAT module that tests early termination
fn create_early_termination_test_wat() -> String {
    r#"
    (module
        (import "env" "request_terminate" (func $request_terminate (param i32) (result i32)))
        (import "env" "response_set_body" (func $response_set_body (param i32 i32) (result i32)))

        (memory (export "memory") 1)

        (data (i32.const 0) "Access Denied")

        (func (export "test_early_termination") (result i32)
            ;; Set response body first
            (call $response_set_body
                (i32.const 0)   ;; body ptr
                (i32.const 13)  ;; body len ("Access Denied")
            )
            drop

            ;; Request early termination with 403 Forbidden
            (call $request_terminate (i32.const 403))
            drop

            ;; Return success
            (i32.const 0)
        )
    )
    "#.to_string()
}

/// Helper function to create a WAT module that reads headers
fn create_header_reader_test_wat() -> String {
    r#"
    (module
        (import "env" "request_get_header" (func $request_get_header (param i32 i32) (result i32)))
        (import "env" "get_shared_buffer" (func $get_shared_buffer (param i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        ;; Data for header name "User-Agent"
        (data (i32.const 0) "User-Agent")

        (func (export "test_header_reading") (result i32)
            ;; Get User-Agent header
            (call $request_get_header
                (i32.const 0)   ;; header name ptr
                (i32.const 10)  ;; header name len
            )

            ;; Return the length (should be > 0 if header exists)
            ;; If -1 (not found), return error
            ;; Otherwise return success
            (i32.const 0)
            (i32.ge_s)
            (if (result i32)
                (then (i32.const 0))
                (else (i32.const -1))
            )
        )
    )
    "#.to_string()
}

#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_request_context_access() -> Result<()> {
    // Create Wasm runtime
    let runtime = WasmRuntime::new()?;

    // Create and load test module
    let wat = create_request_context_test_wat();
    let wasm_bytes = wat::parse_str(&wat)?;

    // Write to temp file
    let temp_path = std::env::temp_dir().join("test_request_context.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    // Load module
    runtime.load_module("test_request_context", &temp_path, WasmModuleType::EdgeFunction)?;

    // Create execution context with request data
    let context = WasmExecutionContext {
        request_method: "GET".to_string(),
        request_uri: "/api/test".to_string(),
        request_headers: vec![
            ("User-Agent".to_string(), "Test/1.0".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ],
        request_body: b"test request body".to_vec(),
        response_status: None,
        response_headers: Vec::new(),
        response_body: Vec::new(),
        terminate_early: false,
    };

    // Execute edge function with context
    let result = runtime.execute_edge_function_with_context(
        "test_request_context",
        "test_request_context",
        None,
        context,
    )?;

    // Verify execution succeeded
    assert_eq!(result.result_data, Vec::<u8>::new());

    // Verify context was passed through (we can't modify it in this simple test)
    assert_eq!(result.context.request_method, "GET");
    assert_eq!(result.context.request_uri, "/api/test");

    // Cleanup
    std::fs::remove_file(&temp_path)?;

    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_response_manipulation() -> Result<()> {
    // Create Wasm runtime
    let runtime = WasmRuntime::new()?;

    // Create and load test module
    let wat = create_response_manipulation_test_wat();
    let wasm_bytes = wat::parse_str(&wat)?;

    let temp_path = std::env::temp_dir().join("test_response_manipulation.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_response", &temp_path, WasmModuleType::EdgeFunction)?;

    // Create execution context
    let context = WasmExecutionContext {
        request_method: "POST".to_string(),
        request_uri: "/api/data".to_string(),
        request_headers: vec![],
        request_body: Vec::new(),
        response_status: None,
        response_headers: Vec::new(),
        response_body: Vec::new(),
        terminate_early: false,
    };

    // Execute edge function
    let result = runtime.execute_edge_function_with_context(
        "test_response",
        "test_response_manipulation",
        None,
        context,
    )?;

    // Verify response was manipulated
    assert_eq!(result.context.response_status, Some(200));

    // Check that custom header was added
    let custom_header = result.context.response_headers.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("X-Custom-Header"));
    assert!(custom_header.is_some(), "Custom header should be present");
    assert_eq!(custom_header.unwrap().1, "CustomValue");

    // Check response body
    assert_eq!(result.context.response_body, b"Test Body");
    assert!(!result.context.terminate_early);

    // Cleanup
    std::fs::remove_file(&temp_path)?;

    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_early_termination() -> Result<()> {
    // Create Wasm runtime
    let runtime = WasmRuntime::new()?;

    // Create and load test module
    let wat = create_early_termination_test_wat();
    let wasm_bytes = wat::parse_str(&wat)?;

    let temp_path = std::env::temp_dir().join("test_early_termination.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_termination", &temp_path, WasmModuleType::EdgeFunction)?;

    // Create execution context
    let context = WasmExecutionContext::default();

    // Execute edge function
    let result = runtime.execute_edge_function_with_context(
        "test_termination",
        "test_early_termination",
        None,
        context,
    )?;

    // Verify early termination was triggered
    assert!(result.context.terminate_early, "terminate_early flag should be set");
    assert_eq!(result.context.response_status, Some(403));
    assert_eq!(result.context.response_body, b"Access Denied");

    // Cleanup
    std::fs::remove_file(&temp_path)?;

    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_header_reading() -> Result<()> {
    // Create Wasm runtime
    let runtime = WasmRuntime::new()?;

    // Create and load test module
    let wat = create_header_reader_test_wat();
    let wasm_bytes = wat::parse_str(&wat)?;

    let temp_path = std::env::temp_dir().join("test_header_reader.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_header_reader", &temp_path, WasmModuleType::EdgeFunction)?;

    // Create execution context with User-Agent header
    let context = WasmExecutionContext {
        request_method: "GET".to_string(),
        request_uri: "/".to_string(),
        request_headers: vec![
            ("User-Agent".to_string(), "Mozilla/5.0".to_string()),
            ("Host".to_string(), "example.com".to_string()),
        ],
        request_body: Vec::new(),
        response_status: None,
        response_headers: Vec::new(),
        response_body: Vec::new(),
        terminate_early: false,
    };

    // Execute edge function
    let result = runtime.execute_edge_function_with_context(
        "test_header_reader",
        "test_header_reading",
        None,
        context,
    )?;

    // Verify execution succeeded (returned 0)
    assert_eq!(result.result_data, Vec::<u8>::new());

    // Cleanup
    std::fs::remove_file(&temp_path)?;

    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_multiple_response_headers() -> Result<()> {
    // Create Wasm runtime
    let runtime = WasmRuntime::new()?;

    // Create a WAT module that adds multiple headers
    let wat = r#"
    (module
        (import "env" "response_add_header" (func $response_add_header (param i32 i32 i32 i32) (result i32)))
        (import "env" "response_set_header" (func $response_set_header (param i32 i32 i32 i32) (result i32)))
        (import "env" "response_remove_header" (func $response_remove_header (param i32 i32) (result i32)))

        (memory (export "memory") 1)

        (data (i32.const 0) "Set-Cookie")
        (data (i32.const 20) "session=abc123")
        (data (i32.const 40) "session=xyz789")
        (data (i32.const 60) "X-Custom")
        (data (i32.const 80) "Value1")

        (func (export "test_headers") (result i32)
            ;; Add first Set-Cookie header
            (call $response_add_header
                (i32.const 0)   ;; "Set-Cookie"
                (i32.const 10)
                (i32.const 20)  ;; "session=abc123"
                (i32.const 14)
            )
            drop

            ;; Add second Set-Cookie header
            (call $response_add_header
                (i32.const 0)   ;; "Set-Cookie"
                (i32.const 10)
                (i32.const 40)  ;; "session=xyz789"
                (i32.const 14)
            )
            drop

            ;; Set X-Custom header
            (call $response_set_header
                (i32.const 60)  ;; "X-Custom"
                (i32.const 8)
                (i32.const 80)  ;; "Value1"
                (i32.const 6)
            )
            drop

            (i32.const 0)
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_multiple_headers.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_headers", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_headers",
        "test_headers",
        None,
        context,
    )?;

    // Verify multiple Set-Cookie headers were added
    let set_cookie_headers: Vec<_> = result.context.response_headers.iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("Set-Cookie"))
        .collect();

    assert_eq!(set_cookie_headers.len(), 2, "Should have 2 Set-Cookie headers");

    // Verify custom header was set
    let custom_header = result.context.response_headers.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("X-Custom"));
    assert!(custom_header.is_some());
    assert_eq!(custom_header.unwrap().1, "Value1");

    // Cleanup
    std::fs::remove_file(&temp_path)?;

    Ok(())
}
