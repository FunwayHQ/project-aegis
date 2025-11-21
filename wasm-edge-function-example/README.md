# AEGIS Edge Function Example

This is a proof-of-concept edge function that demonstrates:
- Accessing DragonflyDB cache from Wasm
- Making HTTP requests to external APIs
- Using the AEGIS Host API

## What it does

The `fetch_exchange_rates` function:
1. Checks the cache for exchange rate data
2. If cache miss, fetches from httpbin.org (demo API)
3. Stores the response in cache with 60s TTL
4. Returns the data

## Building

```bash
# Install Rust wasm32 target if you haven't already
rustup target add wasm32-unknown-unknown

# Build the Wasm module
cargo build --release --target wasm32-unknown-unknown

# The output will be at:
# target/wasm32-unknown-unknown/release/aegis_edge_function_example.wasm
```

## Testing

The example includes several test functions:

- `test_logging()` - Tests the logging host function
- `test_cache()` - Tests cache get/set operations
- `test_http()` - Tests HTTP GET requests
- `fetch_exchange_rates()` - Full example with caching

Run the tests from the node directory:

```bash
cd ../node
cargo test --test edge_function_test -- --ignored
```

Note: Tests require Redis/DragonflyDB to be running on `127.0.0.1:6379`

## Functions Exported

- `fetch_exchange_rates()` - Main demo function
- `test_logging()` - Test logging
- `test_cache()` - Test cache operations
- `test_http()` - Test HTTP requests
- `alloc(size)` - Memory allocator for host
- `dealloc(ptr, size)` - Memory deallocator

## Usage

Load and execute the function:

```rust
use aegis_node::wasm_runtime::{WasmRuntime, WasmModuleType};

let runtime = WasmRuntime::new()?;
let wasm_bytes = std::fs::read("aegis_edge_function_example.wasm")?;

runtime.load_module_from_bytes(
    "exchange-rates",
    &wasm_bytes,
    WasmModuleType::EdgeFunction,
    None,
)?;

let result = runtime.execute_edge_function(
    "exchange-rates",
    "fetch_exchange_rates",
    Some(cache_client_arc),
)?;

println!("Response: {}", String::from_utf8_lossy(&result));
```

## Binary Size

The compiled Wasm module should be small (< 100KB) for fast loading at edge nodes.

```bash
ls -lh target/wasm32-unknown-unknown/release/aegis_edge_function_example.wasm
```

## Further Optimization

For production, optimize with wasm-opt:

```bash
wasm-opt -Oz -o optimized.wasm target/wasm32-unknown-unknown/release/aegis_edge_function_example.wasm
```

## License

Part of the AEGIS project.
