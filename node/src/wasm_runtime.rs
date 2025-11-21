//! Sprint 13: Wasm Edge Functions Runtime
//!
//! This module provides WebAssembly runtime capabilities for:
//! 1. WAF execution in isolated Wasm sandbox (migrated from Sprint 8)
//! 2. Custom edge functions for request/response manipulation
//! 3. Resource governance (CPU, memory limits)
//! 4. Hot-reload capability for Wasm modules
//!
//! Architecture:
//! - wasmtime for Wasm execution
//! - Host API for request/response access
//! - Module caching and hot-reload
//! - IPFS CID resolution for deployment

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use wasmtime::*;

/// Maximum execution time for Wasm modules (10ms for WAF, 50ms for edge functions)
const WAF_EXECUTION_TIMEOUT_MS: u64 = 10;
const EDGE_FUNCTION_TIMEOUT_MS: u64 = 50;

/// Maximum memory for Wasm modules (10MB for WAF, 50MB for edge functions)
const WAF_MEMORY_LIMIT_BYTES: usize = 10 * 1024 * 1024;
const EDGE_FUNCTION_MEMORY_LIMIT_BYTES: usize = 50 * 1024 * 1024;

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
        }
    }
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
        };

        self.modules.write().unwrap().insert(module_id.to_string(), (module, metadata));

        info!("Loaded Wasm module: {} (type: {:?})", module_id, module_type);
        Ok(())
    }

    /// Load Wasm module from bytes
    pub fn load_module_from_bytes(
        &self,
        module_id: &str,
        bytes: &[u8],
        module_type: WasmModuleType,
        ipfs_cid: Option<String>,
    ) -> Result<()> {
        let module = Module::new(&self.engine, bytes)
            .context("Failed to compile Wasm module from bytes")?;

        let metadata = WasmModuleMetadata {
            module_type,
            name: module_id.to_string(),
            version: "1.0.0".to_string(),
            ipfs_cid,
            loaded_at: Instant::now(),
        };

        self.modules.write().unwrap().insert(module_id.to_string(), (module, metadata));

        info!("Loaded Wasm module from bytes: {} (type: {:?})", module_id, module_type);
        Ok(())
    }

    /// Execute WAF analysis in Wasm sandbox
    pub fn execute_waf(
        &self,
        module_id: &str,
        context: &WasmExecutionContext,
    ) -> Result<WafResult> {
        let modules = self.modules.read().unwrap();
        let (module, metadata) = modules.get(module_id)
            .context("WAF module not found")?;

        if metadata.module_type != WasmModuleType::Waf {
            anyhow::bail!("Module is not a WAF module");
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

    /// Get module metadata
    pub fn get_module_metadata(&self, module_id: &str) -> Option<WasmModuleMetadata> {
        self.modules.read().unwrap()
            .get(module_id)
            .map(|(_, meta)| meta.clone())
    }

    /// List all loaded modules
    pub fn list_modules(&self) -> Vec<String> {
        self.modules.read().unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Unload module (for hot-reload)
    pub fn unload_module(&self, module_id: &str) -> Result<()> {
        self.modules.write().unwrap()
            .remove(module_id)
            .context("Module not found")?;

        info!("Unloaded Wasm module: {}", module_id);
        Ok(())
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create Wasm runtime")
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
        let runtime = WasmRuntime::new().unwrap();
        let modules = runtime.list_modules();
        assert_eq!(modules.len(), 0);
    }
}
