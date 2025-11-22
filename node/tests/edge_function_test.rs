//! Sprint 14: Edge Function Host API Tests
//!
//! Tests for Wasm edge functions with cache and HTTP access

use aegis_node::wasm_runtime::{WasmRuntime, WasmModuleType};
use aegis_node::cache::CacheClient;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Helper to create a test Wasm module that uses the host API
fn create_test_wasm_module() -> Vec<u8> {
    // WAT (WebAssembly Text format) for a simple test module
    // This module imports and calls the host functions
    let wat = r#"
        (module
            ;; Import host functions
            (import "env" "log" (func $log (param i32 i32)))
            (import "env" "cache_get" (func $cache_get (param i32 i32) (result i32)))
            (import "env" "cache_set" (func $cache_set (param i32 i32 i32 i32 i32) (result i32)))
            (import "env" "http_get" (func $http_get (param i32 i32) (result i32)))
            (import "env" "get_shared_buffer" (func $get_shared_buffer (param i32 i32 i32) (result i32)))

            ;; Memory export
            (memory (export "memory") 1)

            ;; Test data
            (data (i32.const 0) "test-key")
            (data (i32.const 20) "test-value")
            (data (i32.const 50) "https://httpbin.org/get")
            (data (i32.const 100) "Test log message")

            ;; Test function: logging
            (func (export "test_log") (result i32)
                (call $log (i32.const 100) (i32.const 16))
                (i32.const 0)
            )

            ;; Test function: cache operations
            (func (export "test_cache_ops") (result i32)
                (local $result i32)

                ;; cache_set("test-key", "test-value", 60)
                (local.set $result
                    (call $cache_set
                        (i32.const 0)   ;; key ptr
                        (i32.const 8)   ;; key len
                        (i32.const 20)  ;; value ptr
                        (i32.const 10)  ;; value len
                        (i32.const 60)  ;; ttl
                    )
                )

                ;; Return result (0 = success, -1 = error)
                (local.get $result)
            )

            ;; Test function: cache get
            (func (export "test_cache_get") (result i32)
                ;; cache_get("test-key")
                (call $cache_get
                    (i32.const 0)   ;; key ptr
                    (i32.const 8)   ;; key len
                )
            )
        )
    "#;

    wat::parse_str(wat).expect("Failed to parse WAT")
}

#[tokio::test]
async fn test_edge_function_runtime_creation() {
    let runtime = WasmRuntime::new();
    assert!(runtime.is_ok());
}

#[tokio::test]
async fn test_load_edge_function_module() {
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();

    let result = runtime.load_module_from_bytes(
        "test-edge-function",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    );

    assert!(result.is_ok());

    // Verify module is loaded
    let metadata = runtime.get_module_metadata("test-edge-function").expect("get_module_metadata should succeed");
    assert!(metadata.is_some());

    let meta = metadata.unwrap();
    assert_eq!(meta.module_type, WasmModuleType::EdgeFunction);
    assert_eq!(meta.name, "test-edge-function");
}

#[tokio::test]
#[ignore] // Requires Redis/DragonflyDB to be running
async fn test_edge_function_cache_operations() {
    // Create cache client
    let cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Failed to connect to Redis");
    let cache_arc = Arc::new(Mutex::new(cache));

    // Create runtime and load module
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();
    runtime.load_module_from_bytes(
        "test-edge-function",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    ).unwrap();

    // Execute edge function that sets a cache value
    let result = runtime.execute_edge_function(
        "test-edge-function",
        "test_cache_ops",
        Some(cache_arc.clone()),
    );

    assert!(result.is_ok());

    // Verify the value was actually set in cache
    let mut cache_guard = cache_arc.lock().await;
    let cached_value = cache_guard.get("test-key").await.unwrap();
    assert!(cached_value.is_some());
    assert_eq!(cached_value.unwrap(), b"test-value");

    // Clean up
    cache_guard.delete("test-key").await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis/DragonflyDB to be running
async fn test_edge_function_cache_get() {
    // Create cache client and pre-populate with data
    let cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Failed to connect to Redis");
    let cache_arc = Arc::new(Mutex::new(cache));

    {
        let mut cache_guard = cache_arc.lock().await;
        cache_guard.set("test-key", b"cached-data", Some(60)).await.unwrap();
    }

    // Create runtime and load module
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();
    runtime.load_module_from_bytes(
        "test-edge-function",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    ).unwrap();

    // Execute edge function that gets from cache
    let result = runtime.execute_edge_function(
        "test-edge-function",
        "test_cache_get",
        Some(cache_arc.clone()),
    );

    assert!(result.is_ok());

    // Clean up
    let mut cache_guard = cache_arc.lock().await;
    cache_guard.delete("test-key").await.unwrap();
}

#[tokio::test]
async fn test_edge_function_without_cache() {
    // Create runtime and load module
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();
    runtime.load_module_from_bytes(
        "test-edge-function",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    ).unwrap();

    // Execute edge function without cache client (should fail gracefully)
    let result = runtime.execute_edge_function(
        "test-edge-function",
        "test_log",
        None,
    );

    // Logging should work even without cache
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_module_hot_reload() {
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();

    // Load initial module
    runtime.load_module_from_bytes(
        "reloadable",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        Some("QmTest123".to_string()),
    ).unwrap();

    // Verify loaded
    assert!(runtime.get_module_metadata("reloadable").expect("get_module_metadata should succeed").is_some());

    // Unload
    runtime.unload_module("reloadable").unwrap();

    // Verify unloaded
    assert!(runtime.get_module_metadata("reloadable").expect("get_module_metadata should succeed").is_none());

    // Reload
    runtime.load_module_from_bytes(
        "reloadable",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        Some("QmTest456".to_string()),
    ).unwrap();

    // Verify reloaded with new CID
    let metadata = runtime.get_module_metadata("reloadable").expect("get_module_metadata should succeed").unwrap();
    assert_eq!(metadata.ipfs_cid, Some("QmTest456".to_string()));
}

#[tokio::test]
async fn test_edge_function_list_modules() {
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();

    // Initially empty
    assert_eq!(runtime.list_modules().expect("list_modules should succeed").len(), 0);

    // Load multiple modules
    runtime.load_module_from_bytes("module1", &wasm_bytes, WasmModuleType::EdgeFunction, None).unwrap();
    runtime.load_module_from_bytes("module2", &wasm_bytes, WasmModuleType::EdgeFunction, None).unwrap();
    runtime.load_module_from_bytes("module3", &wasm_bytes, WasmModuleType::Waf, None).unwrap();

    // Verify count
    let modules = runtime.list_modules().expect("list_modules should succeed");
    assert_eq!(modules.len(), 3);
    assert!(modules.contains(&"module1".to_string()));
    assert!(modules.contains(&"module2".to_string()));
    assert!(modules.contains(&"module3".to_string()));
}

#[tokio::test]
async fn test_edge_function_module_type_validation() {
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();

    // Load as WAF module
    runtime.load_module_from_bytes(
        "waf-module",
        &wasm_bytes,
        WasmModuleType::Waf,
        None,
    ).unwrap();

    // Try to execute as edge function (should fail)
    let result = runtime.execute_edge_function(
        "waf-module",
        "test_log",
        None,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not an edge function"));
}

#[tokio::test]
async fn test_nonexistent_module() {
    let runtime = WasmRuntime::new().unwrap();

    let result = runtime.execute_edge_function(
        "nonexistent",
        "test_log",
        None,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_nonexistent_function() {
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = create_test_wasm_module();

    runtime.load_module_from_bytes(
        "test-module",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    ).unwrap();

    let result = runtime.execute_edge_function(
        "test-module",
        "nonexistent_function",
        None,
    );

    assert!(result.is_err());
}

// Integration test: Build and test the real example edge function
#[tokio::test]
#[ignore] // Requires building the Wasm module first
async fn test_real_edge_function_example() {
    use std::fs;

    // Load the compiled example Wasm module
    let wasm_path = "../wasm-edge-function-example/target/wasm32-unknown-unknown/release/aegis_edge_function_example.wasm";

    if !std::path::Path::new(wasm_path).exists() {
        eprintln!("Wasm module not found. Build it first with:");
        eprintln!("cd ../wasm-edge-function-example && cargo build --release --target wasm32-unknown-unknown");
        return;
    }

    let wasm_bytes = fs::read(wasm_path).expect("Failed to read Wasm file");

    // Create cache client
    let cache = CacheClient::new("redis://127.0.0.1:6379", 60)
        .await
        .expect("Failed to connect to Redis");
    let cache_arc = Arc::new(Mutex::new(cache));

    // Create runtime and load module
    let runtime = WasmRuntime::new().unwrap();
    runtime.load_module_from_bytes(
        "exchange-rates",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    ).unwrap();

    // Test logging function
    let result = runtime.execute_edge_function(
        "exchange-rates",
        "test_logging",
        Some(cache_arc.clone()),
    );
    assert!(result.is_ok());

    // Test cache operations
    let result = runtime.execute_edge_function(
        "exchange-rates",
        "test_cache",
        Some(cache_arc.clone()),
    );
    assert!(result.is_ok());

    // Clean up cache
    let mut cache_guard = cache_arc.lock().await;
    cache_guard.delete("test:key").await.ok();
}
