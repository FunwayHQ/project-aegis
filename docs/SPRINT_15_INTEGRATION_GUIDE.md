# Sprint 15: Edge Function Integration with Pingora - Integration Guide

## Overview

Sprint 15 delivers complete request/response manipulation capabilities for Wasm edge functions, enabling them to:
- **Read request context**: method, URI, headers, body
- **Modify responses**: status code, headers, body
- **Terminate requests early**: return custom error responses (403, redirect, etc.)

This guide shows how to integrate these capabilities into the Pingora proxy pipeline.

## Architecture

### Host API Functions (Implemented in `wasm_runtime.rs`)

#### Request Context Access
- `request_get_method()` → Returns HTTP method (GET, POST, etc.)
- `request_get_uri()` → Returns request URI path
- `request_get_header(name_ptr, name_len)` → Returns specific header value
- `request_get_header_names()` → Returns JSON array of all header names
- `request_get_body()` → Returns request body bytes

#### Response Manipulation
- `response_set_status(status)` → Set HTTP status code (200, 404, etc.)
- `response_set_header(name_ptr, name_len, value_ptr, value_len)` → Set/replace header
- `response_add_header(name_ptr, name_len, value_ptr, value_len)` → Add header (allows duplicates)
- `response_remove_header(name_ptr, name_len)` → Remove all headers with given name
- `response_set_body(body_ptr, body_len)` → Set response body

#### Early Termination
- `request_terminate(status)` → Signal early request termination with status code

### Data Structures

#### WasmExecutionContext
```rust
pub struct WasmExecutionContext {
    // Request data (read-only from Wasm perspective)
    pub request_method: String,
    pub request_uri: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Vec<u8>,

    // Response data (modifiable from Wasm)
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Vec<u8>,

    // Control flags
    pub terminate_early: bool,  // Set by request_terminate()
}
```

#### EdgeFunctionResult
```rust
pub struct EdgeFunctionResult {
    pub result_data: Vec<u8>,        // Data from shared buffer
    pub context: WasmExecutionContext,  // Updated context with response modifications
}
```

## Integration with Pingora Proxy

### Step 1: Add WasmRuntime to AegisProxy

Modify `node/src/pingora_proxy.rs`:

```rust
use crate::wasm_runtime::{WasmRuntime, WasmExecutionContext, EdgeFunctionResult};
use std::sync::Arc;

pub struct AegisProxy {
    pub origin_addr: String,
    pub cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
    pub cache_ttl: u64,
    pub caching_enabled: bool,
    pub waf: Option<AegisWaf>,
    pub bot_manager: Option<Arc<BotManager>>,
    pub ip_extraction_config: IpExtractionConfig,

    // Sprint 15: Edge function runtime
    pub edge_runtime: Option<Arc<WasmRuntime>>,
    pub edge_function_config: Option<EdgeFunctionConfig>,
}

#[derive(Clone)]
pub struct EdgeFunctionConfig {
    pub module_id: String,
    pub function_name: String,
    pub enabled: bool,
}
```

### Step 2: Initialize WasmRuntime on Proxy Creation

```rust
impl AegisProxy {
    pub fn new_with_edge_functions(
        origin: String,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        edge_runtime: Option<Arc<WasmRuntime>>,
        edge_config: Option<EdgeFunctionConfig>,
    ) -> Self {
        Self {
            origin_addr: parse_origin(&origin),
            cache_client,
            cache_ttl: 60,
            caching_enabled: false,
            waf: None,
            bot_manager: None,
            ip_extraction_config: IpExtractionConfig::default(),
            edge_runtime,
            edge_function_config: edge_config,
        }
    }
}
```

### Step 3: Add Edge Function Phase to request_filter()

Insert this phase **AFTER** bot management and **BEFORE** WAF analysis:

```rust
async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
    // ============================================
    // PHASE 0: Bot Management (Sprint 9)
    // ============================================
    // ... existing bot management code ...

    // ============================================
    // PHASE 0.5: Edge Function Execution (Sprint 15)
    // ============================================
    if let (Some(edge_runtime), Some(edge_config)) = (&self.edge_runtime, &self.edge_function_config) {
        if edge_config.enabled {
            // Build execution context from request
            let request_headers: Vec<(String, String)> = session
                .req_header()
                .headers
                .iter()
                .filter_map(|(name, value)| {
                    value.to_str().ok().map(|v| (name.to_string(), v.to_string()))
                })
                .collect();

            let mut execution_context = WasmExecutionContext {
                request_method: session.req_header().method.as_str().to_string(),
                request_uri: session.req_header().uri.path().to_string(),
                request_headers,
                request_body: ctx.request_body.clone(),
                response_status: None,
                response_headers: Vec::new(),
                response_body: Vec::new(),
                terminate_early: false,
            };

            // Execute edge function
            match edge_runtime.execute_edge_function_with_context(
                &edge_config.module_id,
                &edge_config.function_name,
                self.cache_client.clone(),
                execution_context,
            ) {
                Ok(result) => {
                    // Check if request should terminate early
                    if result.context.terminate_early {
                        log::info!("Edge function requested early termination with status: {:?}",
                            result.context.response_status);

                        // Send custom response
                        let status = result.context.response_status.unwrap_or(403);
                        let mut response = ResponseHeader::build(status, None)?;

                        // Apply response headers from edge function
                        for (name, value) in result.context.response_headers {
                            response.insert_header(name, value)?;
                        }

                        // Send response with body
                        session.write_response_header(Box::new(response), false).await?;
                        if !result.context.response_body.is_empty() {
                            session.write_response_body(
                                Some(Bytes::from(result.context.response_body)),
                                true
                            ).await?;
                        }

                        return Ok(true);  // Request handled, don't proxy to origin
                    }

                    // Store modified response headers/body in ctx for later use
                    // (could be applied in response_filter phase)
                    ctx.edge_response_status = result.context.response_status;
                    ctx.edge_response_headers = result.context.response_headers;
                    ctx.edge_response_body = result.context.response_body;
                }
                Err(e) => {
                    log::error!("Edge function execution failed: {}", e);
                    // Continue to next phase - don't block request on edge function failure
                }
            }
        }
    }

    // ============================================
    // PHASE 1: WAF Analysis (Sprint 8)
    // ============================================
    // ... existing WAF code ...

    Ok(false)
}
```

### Step 4: Update ProxyContext to Store Edge Function Results

```rust
pub struct ProxyContext {
    pub start_time: Instant,
    pub cache_hit: bool,
    pub cache_key: Option<String>,
    pub cache_ttl: Option<u64>,
    pub waf_blocked: bool,
    pub bot_blocked: bool,
    pub request_body: Vec<u8>,

    // Sprint 15: Edge function response modifications
    pub edge_response_status: Option<u16>,
    pub edge_response_headers: Vec<(String, String)>,
    pub edge_response_body: Vec<u8>,
}
```

### Step 5: Apply Edge Function Modifications in response_filter()

```rust
async fn response_filter(
    &self,
    _session: &mut Session,
    upstream_response: &mut pingora::http::ResponseHeader,
    ctx: &mut Self::CTX,
) -> Result<()> {
    // Apply edge function status override
    if let Some(status) = ctx.edge_response_status {
        upstream_response.set_status(status)?;
    }

    // Apply edge function headers
    for (name, value) in &ctx.edge_response_headers {
        upstream_response.insert_header(name.clone(), value.clone())?;
    }

    // ... existing cache control logic ...

    Ok(())
}
```

## Example: Security Edge Function

Here's a complete example of a security edge function that blocks requests with suspicious patterns:

```wat
(module
    ;; Import host functions
    (import "env" "request_get_uri" (func $request_get_uri (result i32)))
    (import "env" "request_get_header" (func $request_get_header (param i32 i32) (result i32)))
    (import "env" "get_shared_buffer" (func $get_shared_buffer (param i32 i32 i32) (result i32)))
    (import "env" "request_terminate" (func $request_terminate (param i32) (result i32)))
    (import "env" "response_set_body" (func $response_set_body (param i32 i32) (result i32)))
    (import "env" "log" (func $log (param i32 i32)))

    (memory (export "memory") 2)

    (data (i32.const 0) "Access Denied: Suspicious Request")
    (data (i32.const 100) "User-Agent")
    (data (i32.const 200) "curl")

    (func (export "security_check") (result i32)
        (local $uri_len i32)
        (local $ua_len i32)
        (local $i i32)

        ;; Get request URI
        (local.set $uri_len (call $request_get_uri))

        ;; Check if URI contains "../" (path traversal attempt)
        ;; Copy URI to memory at offset 1000
        (call $get_shared_buffer (i32.const 1000) (i32.const 0) (local.get $uri_len))
        drop

        ;; Simple check: if URI length > 500, suspicious
        (if (i32.gt_u (local.get $uri_len) (i32.const 500))
            (then
                ;; Block request with 403
                (call $response_set_body (i32.const 0) (i32.const 33))
                drop
                (call $request_terminate (i32.const 403))
                drop
                (return (i32.const 0))
            )
        )

        ;; Get User-Agent header
        (local.set $ua_len (call $request_get_header (i32.const 100) (i32.const 10)))

        ;; If User-Agent is missing, block
        (if (i32.lt_s (local.get $ua_len) (i32.const 0))
            (then
                (call $response_set_body (i32.const 0) (i32.const 33))
                drop
                (call $request_terminate (i32.const 403))
                drop
                (return (i32.const 0))
            )
        )

        ;; Allow request
        (i32.const 0)
    )
)
```

## Example: Header Injection Edge Function

Add custom tracking headers to responses:

```wat
(module
    (import "env" "response_add_header" (func $response_add_header (param i32 i32 i32 i32) (result i32)))

    (memory (export "memory") 1)

    (data (i32.const 0) "X-Edge-Node")
    (data (i32.const 20) "node-123")
    (data (i32.const 40) "X-Processing-Time")
    (data (i32.const 60) "42ms")

    (func (export "add_tracking_headers") (result i32)
        ;; Add X-Edge-Node header
        (call $response_add_header
            (i32.const 0)   ;; "X-Edge-Node"
            (i32.const 11)
            (i32.const 20)  ;; "node-123"
            (i32.const 8)
        )
        drop

        ;; Add X-Processing-Time header
        (call $response_add_header
            (i32.const 40)  ;; "X-Processing-Time"
            (i32.const 17)
            (i32.const 60)  ;; "42ms"
            (i32.const 4)
        )
        drop

        (i32.const 0)
    )
)
```

## Testing

Run the comprehensive Sprint 15 integration tests:

```bash
cd node
cargo test sprint_15 -- --nocapture
```

Tests include:
- `test_request_context_access` - Verify request data can be read
- `test_response_manipulation` - Verify response can be modified
- `test_early_termination` - Verify early termination works
- `test_header_reading` - Verify specific headers can be read
- `test_multiple_response_headers` - Verify multiple headers can be set

## Performance Considerations

1. **Resource Limits**: Each edge function execution is limited to:
   - 50ms execution time
   - 50MB memory
   - Fuel-based CPU limiting

2. **Error Handling**: Edge function failures do NOT block requests - they continue to WAF/origin

3. **Caching**: Edge functions can use `cache_get`/`cache_set` for shared state

4. **Async Operations**: Host functions use `tokio::runtime::Handle::block_on()` for async operations

## Security

1. **Sandbox Isolation**: All edge functions run in Wasm sandbox
2. **Memory Safety**: No direct memory access outside Wasm linear memory
3. **Network Restrictions**: HTTP requests limited to 5 seconds, 1MB response
4. **Header Validation**: HTTP status codes validated (100-599 range)

## Next Steps

- **Sprint 16**: Route-based edge function dispatch
- **Sprint 17**: IPFS/Solana integration for module loading
- **Sprint 18**: Hot-reload and module versioning

## Wasm Compilation Issues (Sprint 13 Carry-over)

The WAF Wasm module (`wasm-waf`) has compilation issues with native dependencies. Resolution options:

1. **Remove native dependencies** from wasm-waf (use pure Rust regex)
2. **Use wasm32-wasip1 target** with compatible dependencies
3. **Compile with wasm-bindgen** for browser-compatible Wasm

See `docs/WASM_COMPILATION_GUIDE.md` for detailed resolution steps.
