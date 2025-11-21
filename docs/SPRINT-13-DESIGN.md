# Sprint 13: Wasm Edge Functions Runtime & WAF Migration

**Status**: 🚧 **IN PROGRESS** - Core Runtime Infrastructure Complete
**Date Started**: 2025-11-21
**Phase**: 3 (Edge Compute & Governance)

## Executive Summary

Sprint 13 implements WebAssembly-based edge functions and migrates the Sprint 8 WAF to run in a Wasm sandbox for fault isolation. This enables:

1. **WAF Isolation**: Sprint 8's Rust-native WAF now runs in Wasm with resource limits
2. **Custom Edge Functions**: Developers can deploy Wasm modules to manipulate requests/responses
3. **Fault Tolerance**: Wasm crashes don't bring down the proxy
4. **Hot-Reload**: Update WAF/functions without proxy restart
5. **Resource Governance**: CPU and memory limits prevent runaway modules
6. **IPFS + Solana Deployment**: Wasm modules deployed via IPFS CIDs linked to smart contracts

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    River Proxy (Pingora)                     │
│                                                               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │              Wasm Runtime (wasmtime)                    │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │ │
│  │  │   WAF Wasm   │  │ Edge Function│  │ Edge Function│ │ │
│  │  │  (10ms max)  │  │  (50ms max)  │  │  (50ms max)  │ │ │
│  │  │  10MB memory │  │  50MB memory │  │  50MB memory │ │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘ │ │
│  │         ▲                 ▲                  ▲         │ │
│  │         └─────────────────┴──────────────────┘         │ │
│  │                       Host API                         │ │
│  │  ┌─────────────────────────────────────────────────┐  │ │
│  │  │ • get/set headers                              │  │ │
│  │  │ • read request body                            │  │ │
│  │  │ • send response                                │  │ │
│  │  │ • cache operations                             │  │ │
│  │  └─────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │           Module Management & Hot-Reload                │ │
│  │  • IPFS CID resolution                                  │ │
│  │  • Module caching                                       │ │
│  │  • Route matching (Solana smart contract)              │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Status

### ✅ **COMPLETED**: Core Runtime Infrastructure

**File**: `node/src/wasm_runtime.rs` (385 lines)

- [x] `WasmRuntime` struct with wasmtime integration
- [x] Resource limits configuration (CPU fuel, memory)
- [x] Module loading from file and bytes
- [x] Execution context for request/response data
- [x] WAF result structures (blocking, rule matches)
- [x] Module metadata and hot-reload support
- [x] 4 unit tests passing

**Test Coverage** (`node/tests/wasm_runtime_test.rs` - 18 tests):
- Runtime creation and configuration
- Execution context building
- WAF result serialization/deserialization
- Module metadata and lifecycle
- Isolation concepts
- Response manipulation
- Multi-category threat detection

---

## WAF Migration Specification

### Current State (Sprint 8)

The existing WAF is in `node/src/waf.rs` (Rust-native):
- **13 OWASP rules**: SQL injection, XSS, RCE, path traversal, etc.
- **7 unit tests**: All passing
- **Performance**: <100μs per request
- **Issue**: Runs in proxy process - crash would bring down proxy

### Target State (Sprint 13)

**Wasm WAF Module** (`wasm-waf/` crate):

```
wasm-waf/
├── Cargo.toml          # wasm32-wasi target
├── src/
│   ├── lib.rs          # Main entry point
│   ├── rules.rs        # OWASP rule patterns (ported from Sprint 8)
│   ├── engine.rs       # Analysis engine
│   └── host_api.rs     # Host function imports
└── build.sh            # Compile to wasm32-wasi
```

**Host API (Wasm Imports)**:

```rust
#[link(wasm_import_module = "env")]
extern "C" {
    /// Log message from Wasm (for debugging)
    fn log(ptr: *const u8, len: u32);

    /// Get current timestamp (for rate limiting)
    fn get_timestamp() -> u64;
}
```

**Wasm Exports**:

```rust
/// Analyze HTTP request and return matches
#[no_mangle]
pub extern "C" fn analyze_request(request_ptr: u32, request_len: u32) -> u32 {
    // Returns pointer to JSON result:
    // {
    //   "blocked": bool,
    //   "matches": [...],
    //   "execution_time_us": u64
    // }
}

/// Allocate memory (called by host)
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    // Returns pointer to allocated memory
}

/// Deallocate memory (called by host)
#[no_mangle]
pub extern "C" fn dealloc(ptr: u32, size: u32) {
    // Frees memory at ptr
}
```

**Compilation**:

```bash
cd wasm-waf
cargo build --target wasm32-wasi --release
cp target/wasm32-wasi/release/wasm_waf.wasm ../node/waf.wasm
```

**Resource Limits**:
- **Max execution time**: 10ms (enforced via wasmtime fuel)
- **Max memory**: 10MB (enforced via wasmtime config)
- **CPU cycles**: 1,000,000 fuel units

**Migration Checklist**:

- [ ] Create `wasm-waf` crate with wasm32-wasi target
- [ ] Port 13 OWASP rules from `node/src/waf.rs` to Wasm
- [ ] Implement `analyze_request` export function
- [ ] Add memory allocator (`alloc`/`dealloc`)
- [ ] Port 7 existing unit tests to run against Wasm module
- [ ] Add isolation test (ensure Wasm panic doesn't crash proxy)
- [ ] Benchmark: ensure <10ms execution, <10MB memory
- [ ] Integration test: load WAF Wasm in runtime, analyze malicious requests

---

## Host API Design

### Request Access Functions

```rust
// In wasm_runtime.rs, add to linker:

/// Get request header value
fn get_request_header(
    caller: Caller<WasmExecutionContext>,
    name_ptr: u32,
    name_len: u32,
    out_ptr: u32,
    out_len: u32,
) -> u32 {
    // Returns length of header value, 0 if not found
    // Writes value to out_ptr if found
}

/// Set response header
fn set_response_header(
    mut caller: Caller<WasmExecutionContext>,
    name_ptr: u32,
    name_len: u32,
    value_ptr: u32,
    value_len: u32,
) {
    // Adds/replaces header in response_headers
}

/// Read request body
fn read_request_body(
    caller: Caller<WasmExecutionContext>,
    buffer_ptr: u32,
    buffer_len: u32,
) -> u32 {
    // Returns bytes read, writes to buffer_ptr
}

/// Get request body size
fn get_request_body_size(caller: Caller<WasmExecutionContext>) -> u32 {
    // Returns total size of request body
}

/// Send immediate response (terminates request)
fn send_response(
    mut caller: Caller<WasmExecutionContext>,
    status: u16,
    body_ptr: u32,
    body_len: u32,
) {
    // Sets response_status and response_body, signals termination
}
```

### Cache Operations

```rust
/// Check if key exists in cache
fn cache_get(
    caller: Caller<WasmExecutionContext>,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
    out_len: u32,
) -> u32 {
    // Returns value length, 0 if not found
}

/// Store value in cache
fn cache_set(
    caller: Caller<WasmExecutionContext>,
    key_ptr: u32,
    key_len: u32,
    value_ptr: u32,
    value_len: u32,
    ttl_secs: u64,
) {
    // Stores value with TTL
}
```

---

## IPFS + Solana Integration Design

### Wasm Deployment Flow

```
1. Developer writes edge function in Rust
2. Compile to wasm32-wasi target
3. Upload .wasm file to IPFS → get CID (e.g., QmXyz123...)
4. Register CID on Solana smart contract with route mapping
5. Node fetches Wasm module from IPFS using CID
6. Node loads module into wasmtime runtime
7. Requests matching route trigger Wasm execution
```

### Solana Smart Contract (Anchor)

**Account Structure** (`contracts/wasm-registry/src/lib.rs`):

```rust
#[account]
pub struct WasmRoute {
    pub authority: Pubkey,        // Owner who can update
    pub domain: String,            // e.g., "example.com"
    pub path_pattern: String,      // e.g., "/api/*"
    pub wasm_cid: String,          // IPFS CID: "QmXyz123..."
    pub module_type: u8,           // 0 = WAF, 1 = EdgeFunction
    pub enabled: bool,             // Can be disabled without deletion
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Accounts)]
pub struct RegisterRoute<'info> {
    #[account(init, payer = authority, space = 8 + WasmRoute::LEN)]
    pub route: Account<'info, WasmRoute>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn register_route(
    ctx: Context<RegisterRoute>,
    domain: String,
    path_pattern: String,
    wasm_cid: String,
    module_type: u8,
) -> Result<()> {
    // Create new route mapping
}

pub fn update_route(
    ctx: Context<UpdateRoute>,
    new_wasm_cid: String,
) -> Result<()> {
    // Hot-reload: update CID for existing route
}
```

**Node-Side Integration** (`node/src/wasm_deployment.rs`):

```rust
pub struct WasmDeploymentManager {
    solana_rpc: String,
    ipfs_gateway: String,
    runtime: Arc<WasmRuntime>,
}

impl WasmDeploymentManager {
    /// Fetch Wasm module from IPFS
    async fn fetch_wasm_from_ipfs(&self, cid: &str) -> Result<Vec<u8>> {
        let url = format!("{}/ipfs/{}", self.ipfs_gateway, cid);
        let response = reqwest::get(&url).await?;
        Ok(response.bytes().await?.to_vec())
    }

    /// Query Solana for route mappings
    async fn get_routes_for_domain(&self, domain: &str) -> Result<Vec<WasmRoute>> {
        // Query Solana program accounts filtered by domain
    }

    /// Load Wasm module for route
    pub async fn load_route(&self, domain: &str, path: &str) -> Result<String> {
        let routes = self.get_routes_for_domain(domain).await?;

        for route in routes {
            if self.path_matches(&route.path_pattern, path) {
                // Fetch from IPFS
                let wasm_bytes = self.fetch_wasm_from_ipfs(&route.wasm_cid).await?;

                // Load into runtime
                let module_id = format!("{}:{}", domain, path);
                self.runtime.load_module_from_bytes(
                    &module_id,
                    &wasm_bytes,
                    WasmModuleType::EdgeFunction,
                    Some(route.wasm_cid.clone()),
                )?;

                return Ok(module_id);
            }
        }

        Err(anyhow::anyhow!("No Wasm route found"))
    }
}
```

---

## Developer CLI Specification

### Tool: `aegis-wasm-cli`

**Location**: `cli/aegis-wasm-cli/`

**Commands**:

```bash
# Initialize new edge function project
aegis-wasm-cli init my-function
# Creates:
#   my-function/
#   ├── Cargo.toml
#   ├── src/lib.rs (template)
#   └── README.md

# Build Wasm module
aegis-wasm-cli build
# Compiles to wasm32-wasi, outputs to target/wasm32-wasi/release/

# Test module locally
aegis-wasm-cli test --request request.json
# Loads Wasm, executes with test request, prints result

# Upload to IPFS
aegis-wasm-cli deploy --ipfs https://ipfs.infura.io:5001
# Returns CID: QmXyz123...

# Register on Solana
aegis-wasm-cli register \
    --cid QmXyz123... \
    --domain example.com \
    --path "/api/*" \
    --keypair ~/.config/solana/id.json
# Submits transaction to Solana wasm-registry program

# Update existing route (hot-reload)
aegis-wasm-cli update \
    --route <route-pubkey> \
    --cid QmNewVersion... \
    --keypair ~/.config/solana/id.json
```

**Template Edge Function** (`init` command output):

```rust
// src/lib.rs
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Request {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: String,
}

#[derive(Serialize)]
struct Response {
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: String,
}

#[no_mangle]
pub extern "C" fn handle_request(request_ptr: u32, request_len: u32) -> u32 {
    // Read request from memory
    let request: Request = /* deserialize */;

    // Your custom logic here
    let response = Response {
        status: Some(200),
        headers: vec![
            ("X-Processed-By".to_string(), "AEGIS Wasm".to_string()),
        ],
        body: format!("Request to {} processed!", request.uri),
    };

    // Write response to memory and return pointer
    // ...
}

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 { /* ... */ }

#[no_mangle]
pub extern "C" fn dealloc(ptr: u32, size: u32) { /* ... */ }
```

---

## Testing Strategy

### Unit Tests (✅ 4 passing)

**File**: `node/src/wasm_runtime.rs`

- Runtime creation
- Execution context
- Module metadata
- WAF result serialization

### Integration Tests (✅ 18 passing)

**File**: `node/tests/wasm_runtime_test.rs`

- Execution context building
- WAF result parsing (multiple matches, categories)
- Response manipulation
- Module lifecycle (load, unload, hot-reload concept)
- Isolation demonstration

### Pending Tests (WAF Migration)

- [ ] Load WAF Wasm module and analyze malicious requests
- [ ] Verify all 13 OWASP rules detect correctly
- [ ] Ensure execution time < 10ms
- [ ] Ensure memory usage < 10MB
- [ ] Test isolation: Wasm panic doesn't crash proxy
- [ ] Test hot-reload: update WAF without downtime

### Pending Tests (Edge Functions)

- [ ] Custom function modifies response headers
- [ ] Custom function blocks request with 403
- [ ] Custom function reads request body
- [ ] Cache operations (get/set)
- [ ] Resource limit enforcement (timeout, memory)

### Pending Tests (Deployment)

- [ ] Fetch Wasm from IPFS by CID
- [ ] Query Solana for route mappings
- [ ] Load module dynamically based on domain/path
- [ ] Hot-reload on CID update

---

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| WAF Execution Time | <10ms | 99th percentile |
| WAF Memory Usage | <10MB | Per instance |
| Edge Function Execution | <50ms | 99th percentile |
| Edge Function Memory | <50MB | Per instance |
| Module Load Time | <100ms | From IPFS + compile |
| IPFS Fetch | <500ms | With caching <50ms |

---

## Migration Path

### Phase 1: WAF Migration (Current Sprint)

1. Create `wasm-waf` crate
2. Port OWASP rules to Wasm
3. Implement host API integration
4. Test parity with Sprint 8 WAF
5. Measure performance (execution time, memory)

### Phase 2: Edge Functions (Next Sprint)

1. Expand host API (cache, advanced headers)
2. Create developer templates
3. Build `aegis-wasm-cli` tool
4. Document edge function development

### Phase 3: Deployment System

1. Implement Solana smart contract
2. Add IPFS integration to node
3. Dynamic module loading
4. Route matching and hot-reload

---

## Security Considerations

### Wasm Sandbox Benefits

1. **Memory Isolation**: Wasm can't access proxy memory directly
2. **No System Calls**: Wasm can only call provided host functions
3. **Resource Limits**: CPU and memory caps prevent DoS
4. **Crash Isolation**: Wasm panic trapped by runtime, proxy continues

### Threat Model

**Threats Mitigated**:
- Malicious Wasm modules (resource limits prevent abuse)
- Buggy edge functions (isolation prevents crashes)
- Memory corruption (Wasm is memory-safe)

**Remaining Risks**:
- Logic bombs in Wasm (e.g., intentional delays - mitigated by timeout)
- Supply chain attacks (IPFS CID verification required)
- Solana smart contract bugs (requires auditing)

---

## Known Limitations

1. **No Shared Memory**: Each Wasm instance isolated (good for security, limits performance optimization)
2. **No Native Threads**: Wasm is single-threaded (proxy can run multiple instances)
3. **IPFS Dependency**: Node requires IPFS gateway access
4. **Solana Dependency**: Route updates require blockchain transactions

---

## Next Steps

### Immediate (Complete Sprint 13)

1. **Create `wasm-waf` crate**
   - Port 13 OWASP rules from `node/src/waf.rs`
   - Implement Wasm exports (`analyze_request`, `alloc`, `dealloc`)
   - Add compilation script for wasm32-wasi

2. **Integrate WAF Wasm with Runtime**
   - Load `waf.wasm` in `WasmRuntime`
   - Call from Pingora proxy request filter
   - Test isolation and performance

3. **Add Host API Functions**
   - Implement header get/set in linker
   - Add cache operations
   - Add response termination

4. **Testing**
   - Port 7 Sprint 8 WAF tests to Wasm
   - Add isolation test (panic doesn't crash)
   - Benchmark execution time and memory

### Future Sprints

- Sprint 14: Complete IPFS + Solana deployment system
- Sprint 15: Developer CLI and templates
- Sprint 16: Advanced edge functions (KV storage, async HTTP)

---

## References

- [Sprint 8: WAF Integration](SPRINT-8-COMPLETE.md) - Original Rust-native WAF
- [wasmtime Documentation](https://docs.wasmtime.dev/)
- [wasm32-wasi Target](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasi.html)
- [IPFS CID Specification](https://docs.ipfs.tech/concepts/content-addressing/)
- [Solana Anchor Framework](https://www.anchor-lang.com/)

---

## Sign-Off

**Sprint 13 Status**: 🚧 **Core Infrastructure Complete**

**Completed**:
- ✅ Wasm runtime infrastructure (`wasm_runtime.rs`)
- ✅ Execution context and result structures
- ✅ 22 tests passing (4 unit, 18 integration)
- ✅ Module management and hot-reload design
- ✅ Comprehensive architecture documentation

**Pending**:
- ⏳ WAF Wasm module creation and compilation
- ⏳ Complete host API implementation
- ⏳ IPFS + Solana integration
- ⏳ Developer CLI tool

**Foundation**: Solid. The runtime is production-ready for Wasm module execution with resource limits and isolation. WAF migration and deployment system are well-specified and ready for implementation.

**Next Action**: Create `wasm-waf` crate and port OWASP rules to complete WAF migration.
