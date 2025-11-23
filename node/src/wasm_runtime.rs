//! Sprint 13: Wasm Edge Functions Runtime
//!
//! This module provides WebAssembly runtime capabilities for:
//! 1. WAF execution in isolated Wasm sandbox (migrated from Sprint 8)
//! 2. Custom edge functions for request/response manipulation
//! 3. Resource governance (CPU, memory limits)
//! 4. Hot-reload capability for Wasm modules
//!
//! Sprint 14: Extended Host API for Data & External Access
//! - DragonflyDB cache operations (get/set)
//! - Controlled outbound HTTP requests
//! - Enhanced resource governance
//!
//! Architecture:
//! - wasmtime for Wasm execution
//! - Host API for request/response access
//! - Module caching and hot-reload
//! - IPFS CID resolution for deployment

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{debug, info, warn, error};
use wasmtime::*;

use crate::cache::CacheClient;

/// Custom error type for Wasm runtime operations
#[derive(Debug, Error)]
pub enum WasmRuntimeError {
    #[error("Failed to acquire lock (poisoned): {0}")]
    LockPoisoned(String),

    #[error("Module not found: {0}")]
    ModuleNotFound(String),

    #[error("Wasm execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Invalid module type: expected {expected}, got {actual}")]
    InvalidModuleType { expected: String, actual: String },

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl<T> From<std::sync::PoisonError<T>> for WasmRuntimeError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        WasmRuntimeError::LockPoisoned(err.to_string())
    }
}

/// Helper macro for host functions to safely acquire read locks
macro_rules! try_read_lock {
    ($lock:expr, $err_ret:expr) => {
        match $lock.read() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Lock poisoned (read): {}", e);
                return $err_ret;
            }
        }
    };
}

/// Helper macro for host functions to safely acquire write locks
macro_rules! try_write_lock {
    ($lock:expr, $err_ret:expr) => {
        match $lock.write() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Lock poisoned (write): {}", e);
                return $err_ret;
            }
        }
    };
}

/// Maximum execution time for Wasm modules (10ms for WAF, 50ms for edge functions)
const WAF_EXECUTION_TIMEOUT_MS: u64 = 10;
const EDGE_FUNCTION_TIMEOUT_MS: u64 = 50;

/// Maximum memory for Wasm modules (10MB for WAF, 50MB for edge functions)
/// Note: Currently unused, but reserved for future memory governance implementation
#[allow(dead_code)]
const WAF_MEMORY_LIMIT_BYTES: usize = 10 * 1024 * 1024;
#[allow(dead_code)]
const EDGE_FUNCTION_MEMORY_LIMIT_BYTES: usize = 50 * 1024 * 1024;

/// Sprint 14: HTTP request limits for edge functions
const MAX_HTTP_REQUEST_TIMEOUT_MS: u64 = 5000; // 5 seconds max for external calls
const MAX_HTTP_RESPONSE_SIZE: usize = 1024 * 1024; // 1MB max response size
const MAX_CACHE_KEY_SIZE: usize = 256; // Max cache key length
const MAX_CACHE_VALUE_SIZE: usize = 1024 * 1024; // 1MB max cache value

/// Security fix: Max body size for HTTP POST/PUT/DELETE (1MB)
const MAX_HTTP_REQUEST_BODY_SIZE: usize = 1024 * 1024;

/// Security fix: Validate header value for CRLF injection
/// Returns true if the header value is safe (no CR or LF characters)
fn is_header_value_safe(value: &str) -> bool {
    !value.contains('\r') && !value.contains('\n')
}

/// Wasm module type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmModuleType {
    /// WAF module (migrated from Sprint 8)
    Waf,
    /// Custom edge function
    EdgeFunction,
}

/// Wasm module metadata
#[derive(Debug, Clone)]
pub struct WasmModuleMetadata {
    pub module_type: WasmModuleType,
    pub name: String,
    pub version: String,
    pub ipfs_cid: Option<String>,
    pub loaded_at: Instant,
    /// Ed25519 signature of the Wasm module bytes (hex-encoded)
    pub signature: Option<String>,
    /// Ed25519 public key for signature verification (hex-encoded)
    pub public_key: Option<String>,
    /// Whether signature was verified
    pub signature_verified: bool,
}

/// Wasm execution context for request/response data
#[derive(Debug, Clone)]
pub struct WasmExecutionContext {
    pub request_method: String,
    pub request_uri: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Vec<u8>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Vec<u8>,
    /// Sprint 15: Flag to indicate if request should terminate early
    pub terminate_early: bool,
}

impl Default for WasmExecutionContext {
    fn default() -> Self {
        Self {
            request_method: String::new(),
            request_uri: String::new(),
            request_headers: Vec::new(),
            request_body: Vec::new(),
            response_status: None,
            response_headers: Vec::new(),
            response_body: Vec::new(),
            terminate_early: false,
        }
    }
}

/// Sprint 14: Edge function store data for host functions
/// This is passed to the Wasmtime store to enable host functions to access resources
pub struct EdgeFunctionStoreData {
    /// Cache client for DragonflyDB access
    pub cache: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
    /// HTTP client for outbound requests
    pub http_client: reqwest::Client,
    /// Shared memory buffer for data exchange between host and Wasm
    pub shared_buffer: Arc<RwLock<Vec<u8>>>,
    /// Sprint 15: Execution context for request/response manipulation
    pub execution_context: Arc<RwLock<WasmExecutionContext>>,
}

/// Sprint 15: Result from edge function execution with request/response context
#[derive(Debug, Clone)]
pub struct EdgeFunctionResult {
    /// Result data from shared buffer
    pub result_data: Vec<u8>,
    /// Updated execution context with response modifications
    pub context: WasmExecutionContext,
}

/// WAF analysis result returned from Wasm module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafResult {
    pub blocked: bool,
    pub matches: Vec<WafMatch>,
    pub execution_time_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafMatch {
    pub rule_id: u32,
    pub description: String,
    pub severity: u8,
    pub category: String,
    pub matched_value: String,
    pub location: String,
}

/// Wasm runtime manager
pub struct WasmRuntime {
    engine: Engine,
    modules: Arc<RwLock<HashMap<String, (Module, WasmModuleMetadata)>>>,
}

impl WasmRuntime {
    /// Safe read lock with proper error handling
    fn read_modules(&self) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, (Module, WasmModuleMetadata)>>, WasmRuntimeError> {
        self.modules.read()
            .map_err(|e| {
                error!("Failed to acquire read lock on modules: {}", e);
                WasmRuntimeError::LockPoisoned(format!("modules read lock: {}", e))
            })
    }

    /// Safe write lock with proper error handling
    fn write_modules(&self) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<String, (Module, WasmModuleMetadata)>>, WasmRuntimeError> {
        self.modules.write()
            .map_err(|e| {
                error!("Failed to acquire write lock on modules: {}", e);
                WasmRuntimeError::LockPoisoned(format!("modules write lock: {}", e))
            })
    }

    /// Create new Wasm runtime with resource limits
    pub fn new() -> Result<Self> {
        let mut config = Config::new();

        // Enable Wasm features
        config.wasm_multi_memory(true);
        config.wasm_simd(true);

        // Set resource limits
        config.max_wasm_stack(1024 * 1024); // 1MB stack
        config.consume_fuel(true); // Enable fuel for CPU limiting

        // Enable async support for timeouts
        config.async_support(true);

        let engine = Engine::new(&config)?;

        info!("Wasm runtime initialized");

        Ok(Self {
            engine,
            modules: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Load Wasm module from file
    pub fn load_module(
        &self,
        module_id: &str,
        path: impl AsRef<Path>,
        module_type: WasmModuleType,
    ) -> Result<()> {
        let module = Module::from_file(&self.engine, path.as_ref())
            .context("Failed to load Wasm module")?;

        let metadata = WasmModuleMetadata {
            module_type,
            name: module_id.to_string(),
            version: "1.0.0".to_string(),
            ipfs_cid: None,
            loaded_at: Instant::now(),
            signature: None,
            public_key: None,
            signature_verified: false,
        };

        self.write_modules()
            .map_err(|e| anyhow::anyhow!("Failed to write modules: {}", e))?
            .insert(module_id.to_string(), (module, metadata));

        info!("Loaded Wasm module: {} (type: {:?})", module_id, module_type);
        Ok(())
    }

    /// Load Wasm module from bytes
    /// Verify Ed25519 signature of Wasm module bytes
    pub fn verify_module_signature(
        wasm_bytes: &[u8],
        signature_hex: &str,
        public_key_hex: &str,
    ) -> Result<(), WasmRuntimeError> {
        // Decode hex signature
        let signature_bytes = hex::decode(signature_hex)
            .map_err(|e| WasmRuntimeError::SignatureVerificationFailed(
                format!("Invalid signature hex: {}", e)
            ))?;

        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|e| WasmRuntimeError::SignatureVerificationFailed(
                format!("Invalid signature format: {}", e)
            ))?;

        // Decode hex public key
        let public_key_bytes = hex::decode(public_key_hex)
            .map_err(|e| WasmRuntimeError::SignatureVerificationFailed(
                format!("Invalid public key hex: {}", e)
            ))?;

        let public_key = VerifyingKey::from_bytes(
            public_key_bytes.as_slice().try_into()
                .map_err(|_| WasmRuntimeError::SignatureVerificationFailed(
                    "Public key must be 32 bytes".to_string()
                ))?
        ).map_err(|e| WasmRuntimeError::SignatureVerificationFailed(
            format!("Invalid public key: {}", e)
        ))?;

        // Verify signature
        public_key.verify(wasm_bytes, &signature)
            .map_err(|e| WasmRuntimeError::SignatureVerificationFailed(
                format!("Signature verification failed: {}", e)
            ))?;

        Ok(())
    }

    pub fn load_module_from_bytes(
        &self,
        module_id: &str,
        bytes: &[u8],
        module_type: WasmModuleType,
        ipfs_cid: Option<String>,
    ) -> Result<()> {
        self.load_module_from_bytes_with_signature(module_id, bytes, module_type, ipfs_cid, None, None)
    }

    /// Load Wasm module from bytes with optional signature verification
    pub fn load_module_from_bytes_with_signature(
        &self,
        module_id: &str,
        bytes: &[u8],
        module_type: WasmModuleType,
        ipfs_cid: Option<String>,
        signature: Option<String>,
        public_key: Option<String>,
    ) -> Result<()> {
        let mut signature_verified = false;

        // Verify signature if provided
        if let (Some(ref sig), Some(ref pk)) = (&signature, &public_key) {
            Self::verify_module_signature(bytes, sig, pk)?;
            signature_verified = true;
            info!("Wasm module signature verified for: {}", module_id);
        } else if signature.is_some() || public_key.is_some() {
            warn!("Partial signature info provided for {}, skipping verification", module_id);
        }

        let module = Module::new(&self.engine, bytes)
            .context("Failed to compile Wasm module from bytes")?;

        let metadata = WasmModuleMetadata {
            module_type,
            name: module_id.to_string(),
            version: "1.0.0".to_string(),
            ipfs_cid,
            loaded_at: Instant::now(),
            signature,
            public_key,
            signature_verified,
        };

        self.write_modules()
            .map_err(|e| anyhow::anyhow!("Failed to write modules: {}", e))?
            .insert(module_id.to_string(), (module, metadata));

        info!("Loaded Wasm module from bytes: {} (type: {:?}, signed: {})",
            module_id, module_type, signature_verified);
        Ok(())
    }

    /// Execute WAF analysis in Wasm sandbox
    pub fn execute_waf(
        &self,
        module_id: &str,
        context: &WasmExecutionContext,
    ) -> Result<WafResult> {
        let modules = self.read_modules()
            .map_err(|e| anyhow::anyhow!("Failed to read modules: {}", e))?;
        let (module, metadata) = modules.get(module_id)
            .ok_or_else(|| anyhow::anyhow!("WAF module '{}' not found", module_id))?;

        if metadata.module_type != WasmModuleType::Waf {
            anyhow::bail!("Module '{}' is not a WAF module (type: {:?})", module_id, metadata.module_type);
        }

        let start = Instant::now();

        // Create store with resource limits
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(1_000_000)?; // Limit CPU cycles
        store.set_epoch_deadline(1); // Enable epoch-based interruption

        // Create linker with host functions
        let mut linker = Linker::new(&self.engine);
        Self::add_waf_host_functions(&mut linker)?;

        // Instantiate module
        let instance = linker.instantiate(&mut store, module)
            .context("Failed to instantiate WAF module")?;

        // Get memory and allocator functions
        let memory = instance.get_memory(&mut store, "memory")
            .context("WAF module has no memory export")?;

        let alloc = instance.get_typed_func::<u32, u32>(&mut store, "alloc")
            .context("WAF module missing alloc function")?;

        // Serialize request data to JSON
        let request_json = serde_json::json!({
            "method": context.request_method,
            "uri": context.request_uri,
            "headers": context.request_headers,
            "body": String::from_utf8_lossy(&context.request_body),
        });
        let request_str = request_json.to_string();

        // Allocate memory in Wasm and copy request data
        let request_len = request_str.len() as u32;
        let request_ptr = alloc.call(&mut store, request_len)
            .context("Failed to allocate Wasm memory")?;

        memory.write(&mut store, request_ptr as usize, request_str.as_bytes())
            .context("Failed to write request to Wasm memory")?;

        // Call analyze_request function
        let analyze_request = instance.get_typed_func::<(u32, u32), u32>(&mut store, "analyze_request")
            .context("WAF module missing analyze_request function")?;

        let result_ptr = analyze_request.call(&mut store, (request_ptr, request_len))
            .context("Failed to call analyze_request")?;

        // Read result from Wasm memory
        // Result format: first 4 bytes = length, then JSON data
        let mut len_bytes = [0u8; 4];
        memory.read(&store, result_ptr as usize, &mut len_bytes)?;
        let result_len = u32::from_le_bytes(len_bytes) as usize;

        let mut result_bytes = vec![0u8; result_len];
        memory.read(&store, result_ptr as usize + 4, &mut result_bytes)?;

        // Parse result JSON
        let result_str = String::from_utf8(result_bytes)
            .context("Invalid UTF-8 in WAF result")?;
        let mut waf_result: WafResult = serde_json::from_str(&result_str)
            .context("Failed to parse WAF result")?;

        waf_result.execution_time_us = start.elapsed().as_micros() as u64;

        // Check execution time limit
        if waf_result.execution_time_us > WAF_EXECUTION_TIMEOUT_MS * 1000 {
            warn!("WAF execution exceeded {}ms limit: {}us",
                  WAF_EXECUTION_TIMEOUT_MS, waf_result.execution_time_us);
        }

        Ok(waf_result)
    }

    /// Add WAF-specific host functions to linker
    fn add_waf_host_functions(linker: &mut Linker<()>) -> Result<()> {
        // Host function for logging from Wasm
        linker.func_wrap("env", "log", |_caller: Caller<()>, ptr: u32, len: u32| {
            debug!("WAF log: ptr={}, len={}", ptr, len);
        })?;

        Ok(())
    }

    /// Sprint 14: Execute edge function with cache and HTTP access
    pub fn execute_edge_function(
        &self,
        module_id: &str,
        function_name: &str,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
    ) -> Result<Vec<u8>> {
        // Call with default execution context and return just the result data
        let result = self.execute_edge_function_with_context(
            module_id,
            function_name,
            cache_client,
            WasmExecutionContext::default(),
        )?;
        Ok(result.result_data)
    }

    /// Sprint 15: Execute edge function with request/response context
    pub fn execute_edge_function_with_context(
        &self,
        module_id: &str,
        function_name: &str,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        context: WasmExecutionContext,
    ) -> Result<EdgeFunctionResult> {
        let modules = self.read_modules()
            .map_err(|e| anyhow::anyhow!("Failed to read modules: {}", e))?;
        let (module, metadata) = modules.get(module_id)
            .ok_or_else(|| anyhow::anyhow!("Edge function module '{}' not found", module_id))?;

        if metadata.module_type != WasmModuleType::EdgeFunction {
            anyhow::bail!("Module '{}' is not an edge function module (type: {:?})", module_id, metadata.module_type);
        }

        let start = Instant::now();

        // Create store data with cache, HTTP client, and execution context
        let store_data = EdgeFunctionStoreData {
            cache: cache_client,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_millis(MAX_HTTP_REQUEST_TIMEOUT_MS))
                .build()?,
            shared_buffer: Arc::new(RwLock::new(Vec::new())),
            execution_context: Arc::new(RwLock::new(context)),
        };

        // Create store with resource limits
        let mut store = Store::new(&self.engine, store_data);
        store.set_fuel(5_000_000)?; // Higher fuel limit for edge functions
        store.set_epoch_deadline(1); // Enable epoch-based interruption

        // Create linker with edge function host functions
        let mut linker = Linker::new(&self.engine);
        Self::add_edge_function_host_functions(&mut linker)?;

        // Instantiate module
        let instance = linker.instantiate(&mut store, module)
            .context("Failed to instantiate edge function module")?;

        // Call the edge function
        let func = instance.get_typed_func::<(), i32>(&mut store, function_name)
            .context(format!("Edge function '{}' not found", function_name))?;

        let result = func.call(&mut store, ())
            .context("Failed to call edge function")?;

        let execution_time = start.elapsed();
        if execution_time.as_millis() > EDGE_FUNCTION_TIMEOUT_MS as u128 {
            warn!("Edge function execution exceeded {}ms limit: {:?}",
                  EDGE_FUNCTION_TIMEOUT_MS, execution_time);
        }

        // Get result from shared buffer and updated context if function returned success
        if result == 0 {
            let store_data = store.data();
            let buffer = store_data.shared_buffer.read()
                .map_err(|e| anyhow::anyhow!("Failed to read shared buffer: {}", e))?;
            let updated_context = store_data.execution_context.read()
                .map_err(|e| anyhow::anyhow!("Failed to read execution context: {}", e))?
                .clone();

            Ok(EdgeFunctionResult {
                result_data: buffer.clone(),
                context: updated_context,
            })
        } else {
            anyhow::bail!("Edge function returned error code: {}", result);
        }
    }

    /// Sprint 14: Add edge function host functions with cache and HTTP access
    fn add_edge_function_host_functions(linker: &mut Linker<EdgeFunctionStoreData>) -> Result<()> {
        // Host function for logging from Wasm
        linker.func_wrap("env", "log", |mut caller: Caller<EdgeFunctionStoreData>, ptr: u32, len: u32| {
            if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&caller, ptr as usize, &mut buffer).is_ok() {
                    if let Ok(msg) = String::from_utf8(buffer) {
                        info!("Edge function log: {}", msg);
                    }
                }
            }
        })?;

        // Host function: cache_get(key_ptr, key_len) -> i32
        // Returns the length of the value (stored in shared buffer), or -1 if not found
        linker.func_wrap(
            "env",
            "cache_get",
            |mut caller: Caller<EdgeFunctionStoreData>, key_ptr: u32, key_len: u32| -> i32 {
                // Validate key length
                if key_len as usize > MAX_CACHE_KEY_SIZE {
                    warn!("Cache key too large: {} bytes", key_len);
                    return -1;
                }

                // Read key from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut key_bytes = vec![0u8; key_len as usize];
                if memory.read(&caller, key_ptr as usize, &mut key_bytes).is_err() {
                    error!("Failed to read key from Wasm memory");
                    return -1;
                }

                let key = match String::from_utf8(key_bytes) {
                    Ok(k) => k,
                    Err(_) => {
                        error!("Invalid UTF-8 in cache key");
                        return -1;
                    }
                };

                debug!("cache_get called for key: {}", key);

                // Access cache client from store data
                let data = caller.data_mut();
                let cache_arc = match &data.cache {
                    Some(c) => c.clone(),
                    None => {
                        error!("Cache client not available");
                        return -1;
                    }
                };

                // Perform cache get (blocking operation in async context)
                // Use tokio::task::block_in_place to avoid nested block_on issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut cache = cache_arc.lock().await;
                        cache.get(&key).await
                    })
                });

                match result {
                    Ok(Some(value)) => {
                        if value.len() > MAX_CACHE_VALUE_SIZE {
                            warn!("Cache value too large: {} bytes", value.len());
                            return -1;
                        }

                        let data = caller.data_mut();
                        *try_write_lock!(data.shared_buffer, -1) = value.clone();
                        value.len() as i32
                    }
                    Ok(None) => {
                        debug!("Cache miss for key: {}", key);
                        -1
                    }
                    Err(e) => {
                        error!("Cache get error: {}", e);
                        -1
                    }
                }
            },
        )?;

        // Host function: cache_set(key_ptr, key_len, value_ptr, value_len, ttl) -> i32
        // Returns 0 on success, -1 on error
        linker.func_wrap(
            "env",
            "cache_set",
            |mut caller: Caller<EdgeFunctionStoreData>,
             key_ptr: u32,
             key_len: u32,
             value_ptr: u32,
             value_len: u32,
             ttl: u32| -> i32 {
                // Validate sizes
                if key_len as usize > MAX_CACHE_KEY_SIZE {
                    warn!("Cache key too large: {} bytes", key_len);
                    return -1;
                }
                if value_len as usize > MAX_CACHE_VALUE_SIZE {
                    warn!("Cache value too large: {} bytes", value_len);
                    return -1;
                }

                // Read key and value from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut key_bytes = vec![0u8; key_len as usize];
                if memory.read(&caller, key_ptr as usize, &mut key_bytes).is_err() {
                    error!("Failed to read key from Wasm memory");
                    return -1;
                }

                let mut value_bytes = vec![0u8; value_len as usize];
                if memory.read(&caller, value_ptr as usize, &mut value_bytes).is_err() {
                    error!("Failed to read value from Wasm memory");
                    return -1;
                }

                let key = match String::from_utf8(key_bytes) {
                    Ok(k) => k,
                    Err(_) => {
                        error!("Invalid UTF-8 in cache key");
                        return -1;
                    }
                };

                debug!("cache_set called for key: {} (ttl: {}s)", key, ttl);

                // Access cache client from store data
                let data = caller.data_mut();
                let cache_arc = match &data.cache {
                    Some(c) => c.clone(),
                    None => {
                        error!("Cache client not available");
                        return -1;
                    }
                };

                // Perform cache set
                // Use tokio::task::block_in_place to avoid nested block_on issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut cache = cache_arc.lock().await;
                        cache.set(&key, &value_bytes, Some(ttl as u64)).await
                    })
                });

                match result {
                    Ok(_) => {
                        debug!("Successfully cached key: {}", key);
                        0
                    }
                    Err(e) => {
                        error!("Cache set error: {}", e);
                        -1
                    }
                }
            },
        )?;

        // Host function: http_get(url_ptr, url_len) -> i32
        // Returns the length of the response (stored in shared buffer), or -1 on error
        linker.func_wrap(
            "env",
            "http_get",
            |mut caller: Caller<EdgeFunctionStoreData>, url_ptr: u32, url_len: u32| -> i32 {
                // Read URL from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut url_bytes = vec![0u8; url_len as usize];
                if memory.read(&caller, url_ptr as usize, &mut url_bytes).is_err() {
                    error!("Failed to read URL from Wasm memory");
                    return -1;
                }

                let url = match String::from_utf8(url_bytes) {
                    Ok(u) => u,
                    Err(_) => {
                        error!("Invalid UTF-8 in URL");
                        return -1;
                    }
                };

                debug!("http_get called for URL: {}", url);

                // Security fix: HTTPS-only validation
                if !url.starts_with("https://") {
                    error!("Invalid URL scheme (HTTPS required): {}", url);
                    return -1;
                }

                // Access HTTP client from store data
                let data = caller.data_mut();
                let http_client = data.http_client.clone();

                // Perform HTTP GET request
                // Use tokio::task::block_in_place to avoid nested block_on issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        http_client
                            .get(&url)
                            .timeout(Duration::from_millis(MAX_HTTP_REQUEST_TIMEOUT_MS))
                            .send()
                            .await
                    })
                });

                match result {
                    Ok(response) => {
                        let status = response.status();
                        debug!("HTTP GET response status: {}", status);

                        let body_result = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                response.bytes().await
                            })
                        });

                        match body_result {
                            Ok(body) => {
                                if body.len() > MAX_HTTP_RESPONSE_SIZE {
                                    warn!("HTTP response too large: {} bytes", body.len());
                                    return -1;
                                }

                                let data = caller.data_mut();
                                *try_write_lock!(data.shared_buffer, -1) = body.to_vec();
                                body.len() as i32
                            }
                            Err(e) => {
                                error!("Failed to read HTTP response body: {}", e);
                                -1
                            }
                        }
                    }
                    Err(e) => {
                        error!("HTTP GET error: {}", e);
                        -1
                    }
                }
            },
        )?;

        // ============================================
        // Security fix: HTTP POST/PUT/DELETE support with body size limits
        // ============================================

        // Host function: http_post(url_ptr, url_len, body_ptr, body_len, content_type_ptr, content_type_len) -> i32
        // Returns the length of the response (stored in shared buffer), or -1 on error
        linker.func_wrap(
            "env",
            "http_post",
            |mut caller: Caller<EdgeFunctionStoreData>,
             url_ptr: u32, url_len: u32,
             body_ptr: u32, body_len: u32,
             content_type_ptr: u32, content_type_len: u32| -> i32 {
                // Validate body size
                if body_len as usize > MAX_HTTP_REQUEST_BODY_SIZE {
                    error!("HTTP POST body too large: {} bytes (max: {})", body_len, MAX_HTTP_REQUEST_BODY_SIZE);
                    return -1;
                }

                // Read URL, body, and content-type from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut url_bytes = vec![0u8; url_len as usize];
                if memory.read(&caller, url_ptr as usize, &mut url_bytes).is_err() {
                    error!("Failed to read URL from Wasm memory");
                    return -1;
                }

                let mut body_bytes = vec![0u8; body_len as usize];
                if memory.read(&caller, body_ptr as usize, &mut body_bytes).is_err() {
                    error!("Failed to read body from Wasm memory");
                    return -1;
                }

                let mut content_type_bytes = vec![0u8; content_type_len as usize];
                if memory.read(&caller, content_type_ptr as usize, &mut content_type_bytes).is_err() {
                    error!("Failed to read content-type from Wasm memory");
                    return -1;
                }

                let url = match String::from_utf8(url_bytes) {
                    Ok(u) => u,
                    Err(_) => {
                        error!("Invalid UTF-8 in URL");
                        return -1;
                    }
                };

                let content_type = match String::from_utf8(content_type_bytes) {
                    Ok(ct) => ct,
                    Err(_) => {
                        error!("Invalid UTF-8 in content-type");
                        return -1;
                    }
                };

                debug!("http_post called for URL: {} (body size: {}, content-type: {})", url, body_len, content_type);

                // Security fix: HTTPS-only validation
                if !url.starts_with("https://") {
                    error!("Invalid URL scheme (HTTPS required): {}", url);
                    return -1;
                }

                // Validate content-type (basic check)
                if content_type.is_empty() {
                    error!("Content-Type is required for POST requests");
                    return -1;
                }

                // Access HTTP client from store data
                let data = caller.data_mut();
                let http_client = data.http_client.clone();

                // Perform HTTP POST request
                // Use tokio::task::block_in_place to avoid nested block_on issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        http_client
                            .post(&url)
                            .header("Content-Type", content_type)
                            .body(body_bytes)
                            .timeout(Duration::from_millis(MAX_HTTP_REQUEST_TIMEOUT_MS))
                            .send()
                            .await
                    })
                });

                match result {
                    Ok(response) => {
                        let status = response.status();
                        debug!("HTTP POST response status: {}", status);

                        let body_result = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                response.bytes().await
                            })
                        });

                        match body_result {
                            Ok(body) => {
                                if body.len() > MAX_HTTP_RESPONSE_SIZE {
                                    warn!("HTTP response too large: {} bytes", body.len());
                                    return -1;
                                }

                                let data = caller.data_mut();
                                *try_write_lock!(data.shared_buffer, -1) = body.to_vec();
                                body.len() as i32
                            }
                            Err(e) => {
                                error!("Failed to read HTTP response body: {}", e);
                                -1
                            }
                        }
                    }
                    Err(e) => {
                        error!("HTTP POST error: {}", e);
                        -1
                    }
                }
            },
        )?;

        // Host function: http_put(url_ptr, url_len, body_ptr, body_len, content_type_ptr, content_type_len) -> i32
        // Returns the length of the response (stored in shared buffer), or -1 on error
        linker.func_wrap(
            "env",
            "http_put",
            |mut caller: Caller<EdgeFunctionStoreData>,
             url_ptr: u32, url_len: u32,
             body_ptr: u32, body_len: u32,
             content_type_ptr: u32, content_type_len: u32| -> i32 {
                // Validate body size
                if body_len as usize > MAX_HTTP_REQUEST_BODY_SIZE {
                    error!("HTTP PUT body too large: {} bytes (max: {})", body_len, MAX_HTTP_REQUEST_BODY_SIZE);
                    return -1;
                }

                // Read URL, body, and content-type from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut url_bytes = vec![0u8; url_len as usize];
                if memory.read(&caller, url_ptr as usize, &mut url_bytes).is_err() {
                    error!("Failed to read URL from Wasm memory");
                    return -1;
                }

                let mut body_bytes = vec![0u8; body_len as usize];
                if memory.read(&caller, body_ptr as usize, &mut body_bytes).is_err() {
                    error!("Failed to read body from Wasm memory");
                    return -1;
                }

                let mut content_type_bytes = vec![0u8; content_type_len as usize];
                if memory.read(&caller, content_type_ptr as usize, &mut content_type_bytes).is_err() {
                    error!("Failed to read content-type from Wasm memory");
                    return -1;
                }

                let url = match String::from_utf8(url_bytes) {
                    Ok(u) => u,
                    Err(_) => {
                        error!("Invalid UTF-8 in URL");
                        return -1;
                    }
                };

                let content_type = match String::from_utf8(content_type_bytes) {
                    Ok(ct) => ct,
                    Err(_) => {
                        error!("Invalid UTF-8 in content-type");
                        return -1;
                    }
                };

                debug!("http_put called for URL: {} (body size: {}, content-type: {})", url, body_len, content_type);

                // Security fix: HTTPS-only validation
                if !url.starts_with("https://") {
                    error!("Invalid URL scheme (HTTPS required): {}", url);
                    return -1;
                }

                // Validate content-type
                if content_type.is_empty() {
                    error!("Content-Type is required for PUT requests");
                    return -1;
                }

                // Access HTTP client from store data
                let data = caller.data_mut();
                let http_client = data.http_client.clone();

                // Perform HTTP PUT request
                // Use tokio::task::block_in_place to avoid nested block_on issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        http_client
                            .put(&url)
                            .header("Content-Type", content_type)
                            .body(body_bytes)
                            .timeout(Duration::from_millis(MAX_HTTP_REQUEST_TIMEOUT_MS))
                            .send()
                            .await
                    })
                });

                match result {
                    Ok(response) => {
                        let status = response.status();
                        debug!("HTTP PUT response status: {}", status);

                        let body_result = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                response.bytes().await
                            })
                        });

                        match body_result {
                            Ok(body) => {
                                if body.len() > MAX_HTTP_RESPONSE_SIZE {
                                    warn!("HTTP response too large: {} bytes", body.len());
                                    return -1;
                                }

                                let data = caller.data_mut();
                                *try_write_lock!(data.shared_buffer, -1) = body.to_vec();
                                body.len() as i32
                            }
                            Err(e) => {
                                error!("Failed to read HTTP response body: {}", e);
                                -1
                            }
                        }
                    }
                    Err(e) => {
                        error!("HTTP PUT error: {}", e);
                        -1
                    }
                }
            },
        )?;

        // Host function: http_delete(url_ptr, url_len) -> i32
        // Returns the length of the response (stored in shared buffer), or -1 on error
        linker.func_wrap(
            "env",
            "http_delete",
            |mut caller: Caller<EdgeFunctionStoreData>, url_ptr: u32, url_len: u32| -> i32 {
                // Read URL from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut url_bytes = vec![0u8; url_len as usize];
                if memory.read(&caller, url_ptr as usize, &mut url_bytes).is_err() {
                    error!("Failed to read URL from Wasm memory");
                    return -1;
                }

                let url = match String::from_utf8(url_bytes) {
                    Ok(u) => u,
                    Err(_) => {
                        error!("Invalid UTF-8 in URL");
                        return -1;
                    }
                };

                debug!("http_delete called for URL: {}", url);

                // Security fix: HTTPS-only validation
                if !url.starts_with("https://") {
                    error!("Invalid URL scheme (HTTPS required): {}", url);
                    return -1;
                }

                // Access HTTP client from store data
                let data = caller.data_mut();
                let http_client = data.http_client.clone();

                // Perform HTTP DELETE request
                // Use tokio::task::block_in_place to avoid nested block_on issues
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        http_client
                            .delete(&url)
                            .timeout(Duration::from_millis(MAX_HTTP_REQUEST_TIMEOUT_MS))
                            .send()
                            .await
                    })
                });

                match result {
                    Ok(response) => {
                        let status = response.status();
                        debug!("HTTP DELETE response status: {}", status);

                        let body_result = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                response.bytes().await
                            })
                        });

                        match body_result {
                            Ok(body) => {
                                if body.len() > MAX_HTTP_RESPONSE_SIZE {
                                    warn!("HTTP response too large: {} bytes", body.len());
                                    return -1;
                                }

                                let data = caller.data_mut();
                                *try_write_lock!(data.shared_buffer, -1) = body.to_vec();
                                body.len() as i32
                            }
                            Err(e) => {
                                error!("Failed to read HTTP response body: {}", e);
                                -1
                            }
                        }
                    }
                    Err(e) => {
                        error!("HTTP DELETE error: {}", e);
                        -1
                    }
                }
            },
        )?;

        // Host function: get_shared_buffer(dest_ptr, offset, length) -> i32
        // Copies data from shared buffer to Wasm memory
        // Returns number of bytes copied, or -1 on error
        linker.func_wrap(
            "env",
            "get_shared_buffer",
            |mut caller: Caller<EdgeFunctionStoreData>, dest_ptr: u32, offset: u32, length: u32| -> i32 {
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let offset = offset as usize;
                let length = length as usize;

                // Read from shared buffer and copy to a local Vec
                // This allows us to drop the lock before writing to Wasm memory
                let data_to_write = {
                    let data = caller.data_mut();
                    let buffer = try_read_lock!(data.shared_buffer, -1);

                    if offset + length > buffer.len() {
                        error!("Invalid buffer read: offset={}, length={}, buffer_len={}", offset, length, buffer.len());
                        return -1;
                    }

                    buffer[offset..offset + length].to_vec()
                };

                // Now write to Wasm memory (caller is not borrowed anymore)
                if memory.write(&mut caller, dest_ptr as usize, &data_to_write).is_err() {
                    error!("Failed to write to Wasm memory");
                    return -1;
                }

                length as i32
            },
        )?;

        // ============================================
        // Sprint 15: Request Context Access Functions
        // ============================================

        // Host function: request_get_method() -> i32
        // Returns the length of the request method (stored in shared buffer), or -1 on error
        linker.func_wrap(
            "env",
            "request_get_method",
            |mut caller: Caller<EdgeFunctionStoreData>| -> i32 {
                let data = caller.data_mut();
                let context = try_read_lock!(data.execution_context, -1);
                let method_bytes = context.request_method.as_bytes();

                *try_write_lock!(data.shared_buffer, -1) = method_bytes.to_vec();
                method_bytes.len() as i32
            },
        )?;

        // Host function: request_get_uri() -> i32
        // Returns the length of the request URI (stored in shared buffer), or -1 on error
        linker.func_wrap(
            "env",
            "request_get_uri",
            |mut caller: Caller<EdgeFunctionStoreData>| -> i32 {
                let data = caller.data_mut();
                let context = try_read_lock!(data.execution_context, -1);
                let uri_bytes = context.request_uri.as_bytes();

                *try_write_lock!(data.shared_buffer, -1) = uri_bytes.to_vec();
                uri_bytes.len() as i32
            },
        )?;

        // Host function: request_get_header(name_ptr, name_len) -> i32
        // Returns the length of the header value (stored in shared buffer), or -1 if not found
        linker.func_wrap(
            "env",
            "request_get_header",
            |mut caller: Caller<EdgeFunctionStoreData>, name_ptr: u32, name_len: u32| -> i32 {
                // Read header name from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut name_bytes = vec![0u8; name_len as usize];
                if memory.read(&caller, name_ptr as usize, &mut name_bytes).is_err() {
                    error!("Failed to read header name from Wasm memory");
                    return -1;
                }

                let header_name = match String::from_utf8(name_bytes) {
                    Ok(n) => n,
                    Err(_) => {
                        error!("Invalid UTF-8 in header name");
                        return -1;
                    }
                };

                // Look up header in execution context
                let data = caller.data_mut();
                let context = try_read_lock!(data.execution_context, -1);

                // Case-insensitive header lookup
                let header_name_lower = header_name.to_lowercase();
                for (name, value) in &context.request_headers {
                    if name.to_lowercase() == header_name_lower {
                        let value_bytes = value.as_bytes();
                        *try_write_lock!(data.shared_buffer, -1) = value_bytes.to_vec();
                        return value_bytes.len() as i32;
                    }
                }

                debug!("Header not found: {}", header_name);
                -1
            },
        )?;

        // Host function: request_get_header_names() -> i32
        // Returns the length of JSON array of all header names (stored in shared buffer)
        linker.func_wrap(
            "env",
            "request_get_header_names",
            |mut caller: Caller<EdgeFunctionStoreData>| -> i32 {
                let data = caller.data_mut();
                let context = try_read_lock!(data.execution_context, -1);

                // Collect all header names into a Vec
                let header_names: Vec<String> = context.request_headers
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect();

                // Serialize to JSON
                match serde_json::to_vec(&header_names) {
                    Ok(json_bytes) => {
                        let len = json_bytes.len() as i32;
                        *try_write_lock!(data.shared_buffer, -1) = json_bytes;
                        len
                    }
                    Err(e) => {
                        error!("Failed to serialize header names: {}", e);
                        -1
                    }
                }
            },
        )?;

        // Host function: request_get_body() -> i32
        // Returns the length of the request body (stored in shared buffer)
        linker.func_wrap(
            "env",
            "request_get_body",
            |mut caller: Caller<EdgeFunctionStoreData>| -> i32 {
                let data = caller.data_mut();
                let context = try_read_lock!(data.execution_context, -1);

                *try_write_lock!(data.shared_buffer, -1) = context.request_body.clone();
                context.request_body.len() as i32
            },
        )?;

        // ============================================
        // Sprint 15: Response Manipulation Functions
        // ============================================

        // Host function: response_set_status(status: u32) -> i32
        // Sets the response status code, returns 0 on success, -1 on error
        linker.func_wrap(
            "env",
            "response_set_status",
            |mut caller: Caller<EdgeFunctionStoreData>, status: u32| -> i32 {
                if status < 100 || status > 599 {
                    error!("Invalid HTTP status code: {}", status);
                    return -1;
                }

                let data = caller.data_mut();
                let mut context = try_write_lock!(data.execution_context, -1);
                context.response_status = Some(status as u16);

                debug!("Response status set to: {}", status);
                0
            },
        )?;

        // Host function: response_set_header(name_ptr, name_len, value_ptr, value_len) -> i32
        // Sets (replaces) a response header, returns 0 on success, -1 on error
        linker.func_wrap(
            "env",
            "response_set_header",
            |mut caller: Caller<EdgeFunctionStoreData>,
             name_ptr: u32, name_len: u32,
             value_ptr: u32, value_len: u32| -> i32 {
                // Read header name and value from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut name_bytes = vec![0u8; name_len as usize];
                if memory.read(&caller, name_ptr as usize, &mut name_bytes).is_err() {
                    error!("Failed to read header name from Wasm memory");
                    return -1;
                }

                let mut value_bytes = vec![0u8; value_len as usize];
                if memory.read(&caller, value_ptr as usize, &mut value_bytes).is_err() {
                    error!("Failed to read header value from Wasm memory");
                    return -1;
                }

                let header_name = match String::from_utf8(name_bytes) {
                    Ok(n) => n,
                    Err(_) => {
                        error!("Invalid UTF-8 in header name");
                        return -1;
                    }
                };

                let header_value = match String::from_utf8(value_bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        error!("Invalid UTF-8 in header value");
                        return -1;
                    }
                };

                // Security fix: Validate header value for CRLF injection
                if !is_header_value_safe(&header_value) {
                    error!("Header value contains CRLF characters (injection attempt): {}", header_name);
                    return -1;
                }

                // Update response headers in execution context
                let data = caller.data_mut();
                let mut context = try_write_lock!(data.execution_context, -1);

                // Remove existing header with same name (case-insensitive)
                let header_name_lower = header_name.to_lowercase();
                context.response_headers.retain(|(name, _)| name.to_lowercase() != header_name_lower);

                // Add new header
                context.response_headers.push((header_name.clone(), header_value.clone()));

                debug!("Response header set: {} = {}", header_name, header_value);
                0
            },
        )?;

        // Host function: response_add_header(name_ptr, name_len, value_ptr, value_len) -> i32
        // Adds a response header (allows duplicates), returns 0 on success, -1 on error
        linker.func_wrap(
            "env",
            "response_add_header",
            |mut caller: Caller<EdgeFunctionStoreData>,
             name_ptr: u32, name_len: u32,
             value_ptr: u32, value_len: u32| -> i32 {
                // Read header name and value from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut name_bytes = vec![0u8; name_len as usize];
                if memory.read(&caller, name_ptr as usize, &mut name_bytes).is_err() {
                    error!("Failed to read header name from Wasm memory");
                    return -1;
                }

                let mut value_bytes = vec![0u8; value_len as usize];
                if memory.read(&caller, value_ptr as usize, &mut value_bytes).is_err() {
                    error!("Failed to read header value from Wasm memory");
                    return -1;
                }

                let header_name = match String::from_utf8(name_bytes) {
                    Ok(n) => n,
                    Err(_) => {
                        error!("Invalid UTF-8 in header name");
                        return -1;
                    }
                };

                let header_value = match String::from_utf8(value_bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        error!("Invalid UTF-8 in header value");
                        return -1;
                    }
                };

                // Security fix: Validate header value for CRLF injection
                if !is_header_value_safe(&header_value) {
                    error!("Header value contains CRLF characters (injection attempt): {}", header_name);
                    return -1;
                }

                // Add header to execution context (allows duplicates)
                let data = caller.data_mut();
                let mut context = try_write_lock!(data.execution_context, -1);
                context.response_headers.push((header_name.clone(), header_value.clone()));

                debug!("Response header added: {} = {}", header_name, header_value);
                0
            },
        )?;

        // Host function: response_remove_header(name_ptr, name_len) -> i32
        // Removes all response headers with the given name, returns count removed
        linker.func_wrap(
            "env",
            "response_remove_header",
            |mut caller: Caller<EdgeFunctionStoreData>, name_ptr: u32, name_len: u32| -> i32 {
                // Read header name from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut name_bytes = vec![0u8; name_len as usize];
                if memory.read(&caller, name_ptr as usize, &mut name_bytes).is_err() {
                    error!("Failed to read header name from Wasm memory");
                    return -1;
                }

                let header_name = match String::from_utf8(name_bytes) {
                    Ok(n) => n,
                    Err(_) => {
                        error!("Invalid UTF-8 in header name");
                        return -1;
                    }
                };

                // Remove headers from execution context
                let data = caller.data_mut();
                let mut context = try_write_lock!(data.execution_context, -1);
                let header_name_lower = header_name.to_lowercase();
                let original_len = context.response_headers.len();

                context.response_headers.retain(|(name, _)| name.to_lowercase() != header_name_lower);

                let removed_count = original_len - context.response_headers.len();
                debug!("Removed {} headers with name: {}", removed_count, header_name);
                removed_count as i32
            },
        )?;

        // Host function: response_set_body(body_ptr, body_len) -> i32
        // Sets the response body, returns 0 on success, -1 on error
        linker.func_wrap(
            "env",
            "response_set_body",
            |mut caller: Caller<EdgeFunctionStoreData>, body_ptr: u32, body_len: u32| -> i32 {
                // Read body from Wasm memory
                let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => {
                        error!("Failed to get Wasm memory");
                        return -1;
                    }
                };

                let mut body_bytes = vec![0u8; body_len as usize];
                if memory.read(&caller, body_ptr as usize, &mut body_bytes).is_err() {
                    error!("Failed to read body from Wasm memory");
                    return -1;
                }

                // Update response body in execution context
                let data = caller.data_mut();
                let mut context = try_write_lock!(data.execution_context, -1);
                context.response_body = body_bytes;

                debug!("Response body set ({} bytes)", body_len);
                0
            },
        )?;

        // ============================================
        // Sprint 15: Early Termination Function
        // ============================================

        // Host function: request_terminate(status: u32) -> i32
        // Signals that the request should terminate early with the given status
        // Returns 0 on success, -1 on error
        linker.func_wrap(
            "env",
            "request_terminate",
            |mut caller: Caller<EdgeFunctionStoreData>, status: u32| -> i32 {
                if status < 100 || status > 599 {
                    error!("Invalid HTTP status code for termination: {}", status);
                    return -1;
                }

                let data = caller.data_mut();
                let mut context = try_write_lock!(data.execution_context, -1);
                context.terminate_early = true;
                context.response_status = Some(status as u16);

                info!("Request early termination requested with status: {}", status);
                0
            },
        )?;

        Ok(())
    }

    /// Get module metadata
    pub fn get_module_metadata(&self, module_id: &str) -> Result<Option<WasmModuleMetadata>> {
        Ok(self.read_modules()
            .map_err(|e| anyhow::anyhow!("Failed to read modules: {}", e))?
            .get(module_id)
            .map(|(_, meta)| meta.clone()))
    }

    /// List all loaded modules
    pub fn list_modules(&self) -> Result<Vec<String>> {
        Ok(self.read_modules()
            .map_err(|e| anyhow::anyhow!("Failed to read modules: {}", e))?
            .keys()
            .cloned()
            .collect())
    }

    /// Unload module (for hot-reload)
    pub fn unload_module(&self, module_id: &str) -> Result<()> {
        self.write_modules()
            .map_err(|e| anyhow::anyhow!("Failed to write modules: {}", e))?
            .remove(module_id)
            .ok_or_else(|| anyhow::anyhow!("Module '{}' not found", module_id))?;

        info!("Unloaded Wasm module: {}", module_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let runtime = WasmRuntime::new();
        assert!(runtime.is_ok());
    }

    #[test]
    fn test_execution_context_default() {
        let ctx = WasmExecutionContext::default();
        assert_eq!(ctx.request_method, "");
        assert_eq!(ctx.request_uri, "");
        assert!(ctx.request_headers.is_empty());
    }

    #[test]
    fn test_waf_result_serialization() {
        let result = WafResult {
            blocked: true,
            matches: vec![WafMatch {
                rule_id: 1,
                description: "SQL Injection".to_string(),
                severity: 5,
                category: "sqli".to_string(),
                matched_value: "' OR '1'='1".to_string(),
                location: "URI".to_string(),
            }],
            execution_time_us: 1500,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("SQL Injection"));

        let deserialized: WafResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.blocked, true);
        assert_eq!(deserialized.matches.len(), 1);
    }

    #[test]
    fn test_module_listing() {
        let runtime = WasmRuntime::new().expect("Failed to create runtime");
        let modules = runtime.list_modules().expect("Failed to list modules");
        assert_eq!(modules.len(), 0);
    }

    #[test]
    fn test_signature_verification() {
        use ed25519_dalek::{SigningKey, Signer};

        // Generate a test keypair
        let signing_key = SigningKey::from_bytes(&[
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60,
            0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
            0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19,
            0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
        ]);
        let verifying_key = signing_key.verifying_key();

        // Create test Wasm module bytes
        let test_wasm = b"fake wasm module bytes for testing";

        // Sign the bytes
        let signature = signing_key.sign(test_wasm);

        // Convert to hex
        let signature_hex = hex::encode(signature.to_bytes());
        let public_key_hex = hex::encode(verifying_key.to_bytes());

        // Test successful verification
        let result = WasmRuntime::verify_module_signature(
            test_wasm,
            &signature_hex,
            &public_key_hex,
        );
        assert!(result.is_ok(), "Valid signature should verify");

        // Test with wrong signature
        let wrong_signature = hex::encode([0u8; 64]);
        let result = WasmRuntime::verify_module_signature(
            test_wasm,
            &wrong_signature,
            &public_key_hex,
        );
        assert!(result.is_err(), "Invalid signature should fail");

        // Test with wrong public key
        let wrong_public_key = hex::encode([1u8; 32]);
        let result = WasmRuntime::verify_module_signature(
            test_wasm,
            &signature_hex,
            &wrong_public_key,
        );
        assert!(result.is_err(), "Wrong public key should fail");

        // Test with wrong data
        let wrong_data = b"different data";
        let result = WasmRuntime::verify_module_signature(
            wrong_data,
            &signature_hex,
            &public_key_hex,
        );
        assert!(result.is_err(), "Signature of different data should fail");
    }

    #[test]
    fn test_load_module_with_signature() {
        use ed25519_dalek::{SigningKey, Signer};

        let runtime = WasmRuntime::new().expect("Failed to create runtime");

        // Generate a test keypair
        let signing_key = SigningKey::from_bytes(&[
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60,
            0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
            0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19,
            0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
        ]);
        let verifying_key = signing_key.verifying_key();

        // Create minimal valid Wasm module
        let test_wasm = wat::parse_str(
            r#"
            (module
                (func (export "test") (result i32)
                    i32.const 42
                )
            )
            "#
        ).expect("Failed to parse WAT");

        // Sign the module
        let signature = signing_key.sign(&test_wasm);
        let signature_hex = hex::encode(signature.to_bytes());
        let public_key_hex = hex::encode(verifying_key.to_bytes());

        // Load module with valid signature
        let result = runtime.load_module_from_bytes_with_signature(
            "test-module",
            &test_wasm,
            WasmModuleType::Waf,
            None,
            Some(signature_hex.clone()),
            Some(public_key_hex.clone()),
        );
        assert!(result.is_ok(), "Should load module with valid signature");

        // Verify metadata
        let metadata = runtime.get_module_metadata("test-module")
            .expect("Should get metadata")
            .expect("Module should exist");
        assert_eq!(metadata.signature, Some(signature_hex));
        assert_eq!(metadata.public_key, Some(public_key_hex.clone()));
        assert!(metadata.signature_verified, "Signature should be verified");

        // Test loading with invalid signature
        let wrong_signature = hex::encode([0u8; 64]);
        let result = runtime.load_module_from_bytes_with_signature(
            "test-module-invalid",
            &test_wasm,
            WasmModuleType::Waf,
            None,
            Some(wrong_signature),
            Some(public_key_hex),
        );
        assert!(result.is_err(), "Should fail with invalid signature");
    }

    // ============================================
    // Security Fix Tests
    // ============================================

    #[test]
    fn test_header_value_safety_check() {
        // Valid header values
        assert!(is_header_value_safe("Normal-Value"));
        assert!(is_header_value_safe("Value with spaces"));
        assert!(is_header_value_safe("session=abc123; HttpOnly; Secure"));
        assert!(is_header_value_safe(""));

        // Invalid header values with CRLF characters
        assert!(!is_header_value_safe("Value\r\nX-Injected: malicious"));
        assert!(!is_header_value_safe("Value\n"));
        assert!(!is_header_value_safe("Value\r"));
        assert!(!is_header_value_safe("\r\nEvil-Header: true"));
        assert!(!is_header_value_safe("normal\nvalue"));
    }
}
