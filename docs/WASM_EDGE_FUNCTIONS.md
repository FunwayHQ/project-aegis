# AEGIS Wasm Edge Functions - Developer Guide

## Overview

AEGIS supports WebAssembly (Wasm) edge functions that run in isolated sandboxes at the edge. Edge functions can:

- **Access DragonflyDB Cache**: Read and write data to the local edge node cache
- **Make HTTP Requests**: Call external APIs with controlled timeouts and size limits
- **Process Requests**: Manipulate HTTP requests and responses
- **Execute Business Logic**: Run custom code at the edge with low latency

This guide explains how to build, test, and deploy Wasm edge functions to the AEGIS network.

## Architecture

### Host API

Edge functions interact with the AEGIS node through a Host API that provides controlled access to:

1. **Cache Operations** (DragonflyDB)
   - `cache_get(key)` - Retrieve cached data
   - `cache_set(key, value, ttl)` - Store data with TTL

2. **HTTP Client**
   - `http_get(url)` - Make GET requests to external APIs

3. **Logging**
   - `log(message)` - Write logs visible in node output

4. **Shared Buffer**
   - `get_shared_buffer(dest, offset, length)` - Read data from shared memory

### Resource Governance

Edge functions run with strict resource limits:

| Resource | Limit | Purpose |
|----------|-------|---------|
| Execution Time | 50ms | Prevent blocking the event loop |
| Memory | 50MB | Protect node resources |
| CPU Cycles | 5,000,000 fuel units | Prevent infinite loops |
| HTTP Timeout | 5 seconds | Prevent hanging requests |
| HTTP Response Size | 1MB | Prevent memory exhaustion |
| Cache Key Size | 256 bytes | Reasonable key length |
| Cache Value Size | 1MB | Prevent cache bloat |

## Building Edge Functions

### Prerequisites

1. **Rust toolchain** with `wasm32-unknown-unknown` target:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. **(Optional) wasm-opt** for optimization:
   ```bash
   cargo install wasm-opt
   ```

### Project Structure

Create a new Rust library project:

```bash
cargo new --lib my-edge-function
cd my-edge-function
```

Update `Cargo.toml`:

```toml
[package]
name = "my-edge-function"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Minimal dependencies to keep Wasm size small
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Enable link-time optimization
strip = true        # Strip symbols for smaller binary
```

### Host API Interface

Declare the host functions in your Rust code:

```rust
extern "C" {
    /// Log a message to the host
    fn log(ptr: *const u8, len: u32);

    /// Get a value from cache
    /// Returns the length of the value (stored in shared buffer), or -1 if not found
    fn cache_get(key_ptr: *const u8, key_len: u32) -> i32;

    /// Set a value in cache with TTL (seconds)
    /// Returns 0 on success, -1 on error
    fn cache_set(
        key_ptr: *const u8,
        key_len: u32,
        value_ptr: *const u8,
        value_len: u32,
        ttl: u32
    ) -> i32;

    /// Make an HTTP GET request
    /// Returns the length of the response (stored in shared buffer), or -1 on error
    fn http_get(url_ptr: *const u8, url_len: u32) -> i32;

    /// Get data from shared buffer
    /// Returns number of bytes copied, or -1 on error
    fn get_shared_buffer(dest_ptr: *mut u8, offset: u32, length: u32) -> i32;
}
```

### Helper Functions

Create safe Rust wrappers around the unsafe host functions:

```rust
fn log_message(msg: &str) {
    unsafe {
        log(msg.as_ptr(), msg.len() as u32);
    }
}

fn get_from_cache(key: &str) -> Option<Vec<u8>> {
    unsafe {
        let result_len = cache_get(key.as_ptr(), key.len() as u32);
        if result_len < 0 {
            return None;
        }

        let mut buffer = vec![0u8; result_len as usize];
        let copied = get_shared_buffer(buffer.as_mut_ptr(), 0, result_len as u32);
        if copied < 0 {
            return None;
        }

        Some(buffer)
    }
}

fn set_in_cache(key: &str, value: &[u8], ttl: u32) -> bool {
    unsafe {
        let result = cache_set(
            key.as_ptr(),
            key.len() as u32,
            value.as_ptr(),
            value.len() as u32,
            ttl
        );
        result == 0
    }
}

fn http_get_request(url: &str) -> Option<Vec<u8>> {
    unsafe {
        let result_len = http_get(url.as_ptr(), url.len() as u32);
        if result_len < 0 {
            return None;
        }

        let mut buffer = vec![0u8; result_len as usize];
        let copied = get_shared_buffer(buffer.as_mut_ptr(), 0, result_len as u32);
        if copied < 0 {
            return None;
        }

        Some(buffer)
    }
}
```

### Edge Function Implementation

Export your edge function with `#[no_mangle]` and `extern "C"`:

```rust
#[no_mangle]
pub extern "C" fn my_edge_function() -> i32 {
    log_message("Edge function started");

    // Your logic here
    // Return 0 for success, -1 for error

    0
}
```

### Example: API Proxy with Caching

```rust
#[no_mangle]
pub extern "C" fn fetch_weather_data() -> i32 {
    log_message("Fetching weather data");

    let cache_key = "weather:san_francisco";

    // Try cache first
    if let Some(cached_data) = get_from_cache(cache_key) {
        log_message("Cache HIT!");
        return 0; // Data in shared buffer
    }

    log_message("Cache MISS - fetching from API");

    // Call external API
    let api_url = "https://api.weather.gov/stations/KSFO/observations/latest";

    match http_get_request(api_url) {
        Some(response) => {
            log_message("API call successful");

            // Cache for 5 minutes (300 seconds)
            if set_in_cache(cache_key, &response, 300) {
                log_message("Cached successfully");
            }

            0 // Success
        }
        None => {
            log_message("API call failed");
            -1 // Error
        }
    }
}
```

### Memory Management

Export allocator functions for the host:

```rust
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let mut buffer = Vec::with_capacity(size as usize);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, size as usize, size as usize);
    }
}
```

## Building

Build your edge function:

```bash
cargo build --release --target wasm32-unknown-unknown
```

The compiled Wasm module will be at:
```
target/wasm32-unknown-unknown/release/my_edge_function.wasm
```

### Optimization (Optional)

Optimize the Wasm binary for smaller size:

```bash
wasm-opt -Oz -o optimized.wasm target/wasm32-unknown-unknown/release/my_edge_function.wasm
```

### Inspect Binary Size

```bash
ls -lh target/wasm32-unknown-unknown/release/*.wasm
```

Good edge functions should be < 100KB for fast loading.

## Testing Locally

### Unit Tests

Test your Rust logic with standard `#[test]` functions:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json() {
        // Test your business logic
    }
}
```

Run tests:
```bash
cargo test
```

### Integration Tests

Test the Wasm module with the AEGIS runtime:

```rust
use aegis_node::wasm_runtime::{WasmRuntime, WasmModuleType};

#[tokio::test]
async fn test_my_edge_function() {
    let runtime = WasmRuntime::new().unwrap();
    let wasm_bytes = std::fs::read("my_edge_function.wasm").unwrap();

    runtime.load_module_from_bytes(
        "my-function",
        &wasm_bytes,
        WasmModuleType::EdgeFunction,
        None,
    ).unwrap();

    let result = runtime.execute_edge_function(
        "my-function",
        "my_edge_function",
        None,
    );

    assert!(result.is_ok());
}
```

## Deployment

### 1. Upload to IPFS

Edge functions are deployed via IPFS for decentralized distribution:

```bash
# Using IPFS CLI
ipfs add my_edge_function.wasm
# Returns: QmXXXXXXXXXXXXXXXXXXXXXXXXX (your CID)
```

### 2. Register on Solana

Register your edge function in the AEGIS smart contract:

```bash
aegis-cli deploy-edge-function \
    --name "My Edge Function" \
    --ipfs-cid QmXXXXXXXXXXXXXXXXXXXXXXXXX \
    --description "Proxies weather API with caching"
```

### 3. Propagate to Network

Edge nodes will automatically:
1. Fetch the Wasm module from IPFS
2. Validate and compile it
3. Make it available for invocation

## Invoking Edge Functions

### Via HTTP API

```bash
curl https://edge.aegis.network/invoke/QmXXXXXXXXXXXXXXXXXXXXXXXXX/my_edge_function
```

### Programmatically

```rust
let result = runtime.execute_edge_function(
    "my-function",
    "my_edge_function",
    Some(cache_client_arc),
)?;

let response_data = result; // Vec<u8> from shared buffer
```

## Best Practices

### Performance

1. **Minimize Dependencies**: Each dependency increases Wasm size and compilation time
2. **Optimize for Size**: Use `opt-level = "z"` and LTO
3. **Cache Aggressively**: Use cache_set with appropriate TTLs
4. **Avoid Allocations**: Reuse buffers where possible
5. **Keep Functions Small**: < 100KB for fast loading

### Security

1. **Validate Inputs**: Always validate data from external APIs
2. **Handle Errors**: Return -1 on errors, never panic
3. **Limit Recursion**: Be aware of stack limits
4. **Use HTTPS**: Only call HTTPS URLs, never HTTP
5. **Sanitize Cache Keys**: Validate key format to prevent injection

### Reliability

1. **Set Reasonable TTLs**: Balance freshness vs. API load
2. **Handle API Failures**: Always have fallback logic
3. **Log Important Events**: Use log() for debugging
4. **Test Thoroughly**: Test with real cache and HTTP calls
5. **Version Your Functions**: Include version in IPFS CID metadata

## Debugging

### View Logs

Edge function logs appear in the node's output:

```
[INFO] Edge function log: Fetching weather data
[INFO] Edge function log: Cache HIT!
```

### Common Issues

**"Failed to instantiate module"**
- Check that you exported the function with `#[no_mangle]` and `extern "C"`
- Verify the function signature matches: `() -> i32`

**"Cache client not available"**
- Ensure the edge node has a configured cache client
- Check Redis/DragonflyDB is running and accessible

**"HTTP GET error"**
- Verify the URL is valid and uses HTTPS
- Check the external API is reachable
- Ensure response size is under 1MB

**"Execution time exceeded limit"**
- Optimize your function logic
- Reduce HTTP call latency
- Consider pre-caching data

## Example Project

See `wasm-edge-function-example/` for a complete working example:

```bash
cd wasm-edge-function-example
cargo build --release --target wasm32-unknown-unknown

# Test it
cd ../node
cargo test --test edge_function_test -- --ignored
```

## API Reference

### Host Functions

#### `log(ptr: *const u8, len: u32)`
Logs a UTF-8 string message to the node output.

#### `cache_get(key_ptr: *const u8, key_len: u32) -> i32`
Retrieves a value from cache. Returns length of value (stored in shared buffer), or -1 if not found.

**Limits:**
- Key max size: 256 bytes
- Value max size: 1MB

#### `cache_set(key_ptr: *const u8, key_len: u32, value_ptr: *const u8, value_len: u32, ttl: u32) -> i32`
Stores a value in cache with TTL (seconds). Returns 0 on success, -1 on error.

**Limits:**
- Key max size: 256 bytes
- Value max size: 1MB

#### `http_get(url_ptr: *const u8, url_len: u32) -> i32`
Makes an HTTP GET request. Returns length of response (stored in shared buffer), or -1 on error.

**Limits:**
- Timeout: 5 seconds
- Response max size: 1MB
- Only HTTPS URLs allowed

#### `get_shared_buffer(dest_ptr: *mut u8, offset: u32, length: u32) -> i32`
Copies data from the shared buffer to Wasm memory. Returns number of bytes copied, or -1 on error.

## Support

- **Documentation**: `docs/WASM_EDGE_FUNCTIONS.md`
- **Examples**: `wasm-edge-function-example/`
- **Tests**: `node/tests/edge_function_test.rs`
- **Issues**: Report on GitHub

## Roadmap

Future enhancements planned:

- **POST/PUT/DELETE** HTTP methods
- **Request/Response Manipulation**: Modify headers and body
- **Key-Value Store**: Persistent edge storage beyond cache
- **Metrics Collection**: Track invocations and performance
- **A/B Testing**: Route requests based on edge function logic
- **Geolocation API**: Access to request origin location
- **Crypto APIs**: Sign/verify data at edge

---

**Sprint 14: Wasm Edge Functions - Data & External Access**

This implementation provides the foundation for powerful edge computing capabilities in the AEGIS network, enabling developers to deploy custom logic at the edge with access to caching and external APIs.
