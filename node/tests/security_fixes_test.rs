/// Security Fixes Tests
///
/// Tests for the recommended security improvements:
/// 1. CRLF injection prevention in header values
/// 2. HTTP POST/PUT/DELETE support with body size and content-type validation
///
/// Note: These tests require the `dev_unsigned_modules` feature to load unsigned Wasm modules.

use aegis_node::wasm_runtime::{WasmRuntime, WasmModuleType, WasmExecutionContext};
use anyhow::Result;

/// Test CRLF injection prevention in response_set_header
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_crlf_injection_prevention_set_header() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    // Create WAT module that attempts CRLF injection via response_set_header
    let wat = r#"
    (module
        (import "env" "response_set_header" (func $response_set_header (param i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        ;; Malicious header value with CRLF injection attempt
        ;; Value: "CustomValue\r\nX-Injected: malicious"
        (data (i32.const 0) "X-Custom-Header")
        (data (i32.const 20) "CustomValue\r\nX-Injected: malicious")

        (func (export "test_crlf_injection") (result i32)
            ;; Attempt to set header with CRLF characters
            (call $response_set_header
                (i32.const 0)   ;; header name ptr
                (i32.const 15)  ;; header name len
                (i32.const 20)  ;; header value ptr (contains CRLF)
                (i32.const 36)  ;; header value len
            )
            ;; This should return -1 (error) due to CRLF validation
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_crlf_set_header.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_crlf", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    // Execute edge function
    let result = runtime.execute_edge_function_with_context(
        "test_crlf",
        "test_crlf_injection",
        None,
        context,
    );

    // The function should fail because the CRLF injection attempt is blocked
    assert!(result.is_err(), "CRLF injection should be blocked");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test CRLF injection prevention in response_add_header
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_crlf_injection_prevention_add_header() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    // Create WAT module that attempts CRLF injection via response_add_header
    let wat = r#"
    (module
        (import "env" "response_add_header" (func $response_add_header (param i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        ;; Malicious header value with newline character
        (data (i32.const 0) "Set-Cookie")
        (data (i32.const 20) "session=abc\nmalicious=true")

        (func (export "test_crlf_injection_add") (result i32)
            ;; Attempt to add header with LF character
            (call $response_add_header
                (i32.const 0)   ;; header name ptr
                (i32.const 10)  ;; header name len
                (i32.const 20)  ;; header value ptr (contains \n)
                (i32.const 27)  ;; header value len
            )
            ;; This should return -1 (error)
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_crlf_add_header.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_crlf_add", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_crlf_add",
        "test_crlf_injection_add",
        None,
        context,
    );

    assert!(result.is_err(), "CRLF injection in add_header should be blocked");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test that valid headers (without CRLF) are still accepted
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_valid_headers_accepted() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    let wat = r#"
    (module
        (import "env" "response_set_header" (func $response_set_header (param i32 i32 i32 i32) (result i32)))
        (import "env" "response_add_header" (func $response_add_header (param i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        (data (i32.const 0) "X-Custom-Header")
        (data (i32.const 20) "ValidValue123")
        (data (i32.const 40) "Set-Cookie")
        (data (i32.const 60) "session=xyz789; HttpOnly; Secure")

        (func (export "test_valid_headers") (result i32)
            ;; Set a valid header
            (call $response_set_header
                (i32.const 0)   ;; X-Custom-Header
                (i32.const 15)
                (i32.const 20)  ;; ValidValue123
                (i32.const 13)
            )
            (i32.const 0)
            (i32.ne)
            (if
                (then (i32.const -1) (return))
            )

            ;; Add a valid header
            (call $response_add_header
                (i32.const 40)  ;; Set-Cookie
                (i32.const 10)
                (i32.const 60)  ;; session=xyz789; HttpOnly; Secure
                (i32.const 33)
            )
            (i32.const 0)
            (i32.ne)
            (if
                (then (i32.const -1) (return))
            )

            (i32.const 0)
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_valid_headers.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_valid", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_valid",
        "test_valid_headers",
        None,
        context,
    )?;

    // Verify headers were set correctly
    assert_eq!(result.context.response_headers.len(), 2);

    let custom_header = result.context.response_headers.iter()
        .find(|(name, _)| name == "X-Custom-Header");
    assert!(custom_header.is_some());
    assert_eq!(custom_header.unwrap().1, "ValidValue123");

    let cookie_header = result.context.response_headers.iter()
        .find(|(name, _)| name == "Set-Cookie");
    assert!(cookie_header.is_some());
    assert_eq!(cookie_header.unwrap().1, "session=xyz789; HttpOnly; Secure");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test HTTP POST with body size limit enforcement
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_http_post_body_size_limit() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    // Create WAT module that attempts to send oversized POST body
    let wat = r#"
    (module
        (import "env" "http_post" (func $http_post (param i32 i32 i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 2)

        (data (i32.const 0) "https://httpbin.org/post")
        (data (i32.const 40) "application/json")

        (func (export "test_oversized_post") (result i32)
            ;; Attempt to POST with body size > 1MB (1048577 bytes)
            ;; This should be rejected
            (call $http_post
                (i32.const 0)       ;; URL ptr
                (i32.const 24)      ;; URL len
                (i32.const 1000)    ;; body ptr (doesn't matter, will fail size check first)
                (i32.const 1048577) ;; body len > 1MB limit
                (i32.const 40)      ;; content-type ptr
                (i32.const 16)      ;; content-type len
            )
            ;; Should return -1 (error)
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_post_size.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_post_size", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_post_size",
        "test_oversized_post",
        None,
        context,
    );

    // Should fail due to oversized body
    assert!(result.is_err(), "Oversized POST body should be rejected");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test HTTP PUT with body size limit enforcement
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_http_put_body_size_limit() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    let wat = r#"
    (module
        (import "env" "http_put" (func $http_put (param i32 i32 i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 2)

        (data (i32.const 0) "https://httpbin.org/put")
        (data (i32.const 40) "application/json")

        (func (export "test_oversized_put") (result i32)
            ;; Attempt PUT with oversized body
            (call $http_put
                (i32.const 0)       ;; URL ptr
                (i32.const 23)      ;; URL len
                (i32.const 1000)    ;; body ptr
                (i32.const 2097152) ;; body len = 2MB (exceeds 1MB limit)
                (i32.const 40)      ;; content-type ptr
                (i32.const 16)      ;; content-type len
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_put_size.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_put_size", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_put_size",
        "test_oversized_put",
        None,
        context,
    );

    assert!(result.is_err(), "Oversized PUT body should be rejected");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test HTTP POST with missing Content-Type
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_http_post_missing_content_type() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    let wat = r#"
    (module
        (import "env" "http_post" (func $http_post (param i32 i32 i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        (data (i32.const 0) "https://httpbin.org/post")
        (data (i32.const 40) "{\"key\":\"value\"}")
        ;; Empty content-type at offset 100
        (data (i32.const 100) "")

        (func (export "test_missing_content_type") (result i32)
            ;; Attempt POST with empty Content-Type
            (call $http_post
                (i32.const 0)   ;; URL ptr
                (i32.const 24)  ;; URL len
                (i32.const 40)  ;; body ptr
                (i32.const 15)  ;; body len
                (i32.const 100) ;; content-type ptr (empty string)
                (i32.const 0)   ;; content-type len = 0 (should fail)
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_post_no_ct.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_post_no_ct", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_post_no_ct",
        "test_missing_content_type",
        None,
        context,
    );

    assert!(result.is_err(), "POST without Content-Type should be rejected");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test HTTP PUT with missing Content-Type
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_http_put_missing_content_type() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    let wat = r#"
    (module
        (import "env" "http_put" (func $http_put (param i32 i32 i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        (data (i32.const 0) "https://httpbin.org/put")
        (data (i32.const 40) "data")

        (func (export "test_put_no_ct") (result i32)
            ;; PUT with empty Content-Type
            (call $http_put
                (i32.const 0)   ;; URL ptr
                (i32.const 23)  ;; URL len
                (i32.const 40)  ;; body ptr
                (i32.const 4)   ;; body len
                (i32.const 100) ;; content-type ptr (undefined/empty)
                (i32.const 0)   ;; content-type len = 0
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_put_no_ct.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_put_no_ct", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_put_no_ct",
        "test_put_no_ct",
        None,
        context,
    );

    assert!(result.is_err(), "PUT without Content-Type should be rejected");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test HTTP DELETE (should work without body or Content-Type)
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_http_delete_url_validation() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    let wat = r#"
    (module
        (import "env" "http_delete" (func $http_delete (param i32 i32) (result i32)))

        (memory (export "memory") 1)

        ;; Invalid URL scheme (ftp://)
        (data (i32.const 0) "ftp://example.com/resource")

        (func (export "test_delete_invalid_url") (result i32)
            ;; DELETE with invalid URL scheme
            (call $http_delete
                (i32.const 0)   ;; URL ptr
                (i32.const 26)  ;; URL len
            )
            ;; Should return -1 due to invalid scheme
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_delete_url.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_delete", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_delete",
        "test_delete_invalid_url",
        None,
        context,
    );

    assert!(result.is_err(), "DELETE with invalid URL scheme should be rejected");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}

/// Test that carriage return (\r) alone is also blocked
#[tokio::test]
#[cfg_attr(not(feature = "dev_unsigned_modules"), ignore = "Requires dev_unsigned_modules feature")]
async fn test_cr_only_injection_blocked() -> Result<()> {
    let runtime = WasmRuntime::new()?;

    let wat = r#"
    (module
        (import "env" "response_set_header" (func $response_set_header (param i32 i32 i32 i32) (result i32)))

        (memory (export "memory") 1)

        (data (i32.const 0) "Location")
        ;; Value with only \r (carriage return)
        (data (i32.const 20) "https://example.com\rX-Evil: true")

        (func (export "test_cr_only") (result i32)
            (call $response_set_header
                (i32.const 0)
                (i32.const 8)
                (i32.const 20)
                (i32.const 33)
            )
        )
    )
    "#;

    let wasm_bytes = wat::parse_str(wat)?;
    let temp_path = std::env::temp_dir().join("test_cr_only.wasm");
    std::fs::write(&temp_path, &wasm_bytes)?;

    runtime.load_module("test_cr", &temp_path, WasmModuleType::EdgeFunction)?;

    let context = WasmExecutionContext::default();

    let result = runtime.execute_edge_function_with_context(
        "test_cr",
        "test_cr_only",
        None,
        context,
    );

    assert!(result.is_err(), "CR character alone should be blocked");

    std::fs::remove_file(&temp_path)?;
    Ok(())
}
