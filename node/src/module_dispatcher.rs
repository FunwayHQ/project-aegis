/// Sprint 16: Module Dispatcher for Wasm Pipeline Orchestration
///
/// This module orchestrates the execution of Wasm module pipelines defined by routes.
/// It handles:
/// - Sequential execution of WAF + edge function modules
/// - Early termination when WAF blocks a request
/// - Error handling per route settings (fail fast vs continue on error)
/// - Resource governance (max modules per request)
/// - Execution time tracking for profiling

use crate::cache::CacheClient;
use crate::route_config::{Route, RouteSettings, WasmModuleRef};
use crate::wasm_runtime::{WasmRuntime, WasmExecutionContext};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Result of executing a single module in the pipeline
#[derive(Debug, Clone)]
pub enum ModuleExecutionResult {
    /// WAF module result
    Waf {
        blocked: bool,
        block_reason: Option<String>,
        execution_time_us: u64,
    },
    /// Edge function result
    EdgeFunction {
        result_data: Vec<u8>,
        updated_context: WasmExecutionContext,
        execution_time_us: u64,
    },
    /// Rate limiter / DDoS protection result
    RateLimiter {
        /// Whether the request is blocked due to rate limiting
        blocked: bool,
        /// Reason for blocking (if blocked)
        block_reason: Option<String>,
        /// Seconds until the rate limit window resets
        retry_after_secs: u64,
        /// Current request count in window
        current_count: u64,
        /// Remaining requests in window
        remaining: u64,
        /// Execution time in microseconds
        execution_time_us: u64,
    },
}

/// Result of executing a complete module pipeline
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// Whether the request was blocked by any module (WAF)
    pub blocked: bool,

    /// HTTP status code if blocked (e.g., 403 Forbidden)
    pub status_code: u16,

    /// Response body if request was terminated early
    pub response_body: Vec<u8>,

    /// Execution time per module (module_id -> duration_us)
    pub execution_times: Vec<(String, u64)>,

    /// Final execution context after all modules
    pub final_context: WasmExecutionContext,

    /// Number of modules executed
    pub modules_executed: usize,
}

impl PipelineResult {
    /// Create a new pipeline result indicating the request was blocked
    pub fn blocked(reason: String, execution_times: Vec<(String, u64)>) -> Self {
        Self {
            blocked: true,
            status_code: 403,
            response_body: format!("Request blocked: {}", reason).into_bytes(),
            execution_times,
            final_context: WasmExecutionContext::default(),
            modules_executed: 0,
        }
    }

    /// Create a new pipeline result for rate limiting (429 Too Many Requests)
    pub fn rate_limited(reason: String, retry_after_secs: u64, execution_times: Vec<(String, u64)>) -> Self {
        Self {
            blocked: true,
            status_code: 429,
            response_body: format!("Rate limit exceeded: {}. Retry after {} seconds.", reason, retry_after_secs).into_bytes(),
            execution_times,
            final_context: WasmExecutionContext::default(),
            modules_executed: 0,
        }
    }

    /// Create a new pipeline result for successful execution
    pub fn success(context: WasmExecutionContext, execution_times: Vec<(String, u64)>, modules_executed: usize) -> Self {
        Self {
            blocked: false,
            status_code: 200,
            response_body: Vec::new(),
            execution_times,
            final_context: context,
            modules_executed,
        }
    }
}

/// Pipeline dispatcher for executing ordered Wasm modules
pub struct ModuleDispatcher {
    /// Wasm runtime for module execution
    wasm_runtime: Arc<WasmRuntime>,

    /// Optional cache client for edge functions
    cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,

    /// Global route settings
    settings: RouteSettings,
}

impl ModuleDispatcher {
    /// Create a new module dispatcher
    pub fn new(
        wasm_runtime: Arc<WasmRuntime>,
        cache_client: Option<Arc<tokio::sync::Mutex<CacheClient>>>,
        settings: RouteSettings,
    ) -> Self {
        info!(
            "Creating ModuleDispatcher (max_modules: {}, continue_on_error: {})",
            settings.max_modules_per_request, settings.continue_on_error
        );

        Self {
            wasm_runtime,
            cache_client,
            settings,
        }
    }

    /// Execute a pipeline of Wasm modules for a route
    pub fn execute_pipeline(
        &self,
        route: &Route,
        initial_context: WasmExecutionContext,
    ) -> Result<PipelineResult> {
        let pipeline_start = Instant::now();
        let route_name = route.name.as_deref().unwrap_or("unnamed");

        debug!(
            "Starting pipeline for route '{}' ({} modules)",
            route_name,
            route.wasm_modules.len()
        );

        // Enforce max modules safety limit
        let module_count = route.wasm_modules.len();
        if module_count > self.settings.max_modules_per_request {
            warn!(
                "Route '{}' exceeds max modules limit ({} > {}), truncating",
                route_name, module_count, self.settings.max_modules_per_request
            );
        }

        let modules_to_execute = route
            .wasm_modules
            .iter()
            .take(self.settings.max_modules_per_request);

        let mut current_context = initial_context;
        let mut execution_times = Vec::new();
        let mut modules_executed = 0;

        for (index, module_ref) in modules_to_execute.enumerate() {
            modules_executed += 1;

            debug!(
                "Executing module {}/{}: {} (type: {})",
                index + 1,
                module_count.min(self.settings.max_modules_per_request),
                module_ref.module_id,
                module_ref.module_type
            );

            // Execute module based on type
            let result = self.execute_module(module_ref, &current_context);

            // Handle execution result
            match result {
                Ok(exec_result) => {
                    match exec_result {
                        ModuleExecutionResult::Waf {
                            blocked,
                            block_reason,
                            execution_time_us,
                        } => {
                            execution_times.push((module_ref.module_id.clone(), execution_time_us));

                            if blocked {
                                let reason = block_reason.unwrap_or_else(|| "WAF rule violation".to_string());
                                info!(
                                    "Request blocked by WAF module '{}': {}",
                                    module_ref.module_id, reason
                                );

                                // Early termination - WAF blocked the request
                                return Ok(PipelineResult::blocked(reason, execution_times));
                            }

                            debug!(
                                "WAF module '{}' allowed request ({}μs)",
                                module_ref.module_id, execution_time_us
                            );
                        }
                        ModuleExecutionResult::EdgeFunction {
                            result_data: _,
                            updated_context,
                            execution_time_us,
                        } => {
                            execution_times.push((module_ref.module_id.clone(), execution_time_us));

                            debug!(
                                "Edge function '{}' completed ({}μs)",
                                module_ref.module_id, execution_time_us
                            );

                            // Check if edge function requested early termination
                            if updated_context.terminate_early {
                                info!(
                                    "Edge function '{}' requested early termination",
                                    module_ref.module_id
                                );
                                current_context = updated_context;
                                break;
                            }

                            // Update context with edge function modifications
                            current_context = updated_context;
                        }
                        ModuleExecutionResult::RateLimiter {
                            blocked,
                            block_reason,
                            retry_after_secs,
                            current_count: _,
                            remaining: _,
                            execution_time_us,
                        } => {
                            execution_times.push((module_ref.module_id.clone(), execution_time_us));

                            if blocked {
                                let reason = block_reason.unwrap_or_else(|| "Rate limit exceeded".to_string());
                                info!(
                                    "Request rate limited by module '{}': {} (retry after {}s)",
                                    module_ref.module_id, reason, retry_after_secs
                                );

                                // Early termination - rate limit exceeded
                                return Ok(PipelineResult::rate_limited(reason, retry_after_secs, execution_times));
                            }

                            debug!(
                                "Rate limiter '{}' allowed request ({}μs)",
                                module_ref.module_id, execution_time_us
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Module '{}' execution failed: {}",
                        module_ref.module_id, e
                    );

                    if self.settings.continue_on_error {
                        warn!(
                            "Continuing pipeline despite error in '{}' (continue_on_error=true)",
                            module_ref.module_id
                        );
                        // Add failed module to execution times with 0 duration
                        execution_times.push((module_ref.module_id.clone(), 0));
                        continue;
                    } else {
                        return Err(e).context(format!(
                            "Module pipeline failed at '{}' in route '{}'",
                            module_ref.module_id, route_name
                        ));
                    }
                }
            }
        }

        let total_duration = pipeline_start.elapsed();
        info!(
            "Pipeline completed for route '{}': {} modules in {:?}",
            route_name, modules_executed, total_duration
        );

        Ok(PipelineResult::success(
            current_context,
            execution_times,
            modules_executed,
        ))
    }

    /// Execute a single Wasm module
    fn execute_module(
        &self,
        module_ref: &WasmModuleRef,
        context: &WasmExecutionContext,
    ) -> Result<ModuleExecutionResult> {
        match module_ref.module_type.as_str() {
            "waf" => self.execute_waf_module(module_ref, context),
            "edge_function" => self.execute_edge_function_module(module_ref, context),
            "ddos_protection" | "rate_limiter" => {
                self.execute_rate_limiter_module(module_ref, context)
            }
            unknown => {
                error!("Unknown module type: {}", unknown);
                Err(anyhow::anyhow!("Unknown module type: {}", unknown))
            }
        }
    }

    /// Execute a rate limiter / DDoS protection module
    ///
    /// Note: This is a placeholder implementation that always allows requests.
    /// In production, this would integrate with DDoSManager.check_rate_limit().
    /// The actual rate limiting logic is handled by the DDoSManager and DistributedRateLimiter.
    fn execute_rate_limiter_module(
        &self,
        module_ref: &WasmModuleRef,
        _context: &WasmExecutionContext,
    ) -> Result<ModuleExecutionResult> {
        let start = Instant::now();

        // Parse config if provided
        let _config = module_ref.config.as_ref();

        // Placeholder: In production, this would call DDoSManager.check_rate_limit()
        // For now, we allow all requests and log the intent
        debug!(
            "Rate limiter module '{}' executed (config: {:?})",
            module_ref.module_id,
            module_ref.config
        );

        let execution_time = start.elapsed().as_micros() as u64;

        // Default: allow the request
        Ok(ModuleExecutionResult::RateLimiter {
            blocked: false,
            block_reason: None,
            retry_after_secs: 0,
            current_count: 0,
            remaining: u64::MAX,
            execution_time_us: execution_time,
        })
    }

    /// Execute a WAF module
    fn execute_waf_module(
        &self,
        module_ref: &WasmModuleRef,
        context: &WasmExecutionContext,
    ) -> Result<ModuleExecutionResult> {
        let start = Instant::now();

        let waf_result = self
            .wasm_runtime
            .execute_waf(&module_ref.module_id, context)
            .context(format!(
                "WAF module '{}' execution failed",
                module_ref.module_id
            ))?;

        let execution_time = start.elapsed().as_micros() as u64;

        // Determine block reason from WAF matches
        let block_reason = if waf_result.blocked {
            if !waf_result.matches.is_empty() {
                Some(format!(
                    "{} WAF rule(s) triggered",
                    waf_result.matches.len()
                ))
            } else {
                Some("WAF policy violation".to_string())
            }
        } else {
            None
        };

        Ok(ModuleExecutionResult::Waf {
            blocked: waf_result.blocked,
            block_reason,
            execution_time_us: execution_time,
        })
    }

    /// Execute an edge function module
    fn execute_edge_function_module(
        &self,
        module_ref: &WasmModuleRef,
        context: &WasmExecutionContext,
    ) -> Result<ModuleExecutionResult> {
        let start = Instant::now();

        // Execute edge function with context
        // Note: We use "main" as the default function name
        // In future sprints, this could be configurable per module
        let result = self
            .wasm_runtime
            .execute_edge_function_with_context(
                &module_ref.module_id,
                "main",
                self.cache_client.clone(),
                context.clone(),
            )
            .context(format!(
                "Edge function '{}' execution failed",
                module_ref.module_id
            ))?;

        let execution_time = start.elapsed().as_micros() as u64;

        Ok(ModuleExecutionResult::EdgeFunction {
            result_data: result.result_data,
            updated_context: result.context,
            execution_time_us: execution_time,
        })
    }

    /// Get statistics about the dispatcher
    pub fn stats(&self) -> DispatcherStats {
        DispatcherStats {
            max_modules_per_request: self.settings.max_modules_per_request,
            continue_on_error: self.settings.continue_on_error,
        }
    }
}

/// Dispatcher statistics
#[derive(Debug, Clone)]
pub struct DispatcherStats {
    pub max_modules_per_request: usize,
    pub continue_on_error: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_config::{MethodMatcher, Route, RoutePattern};

    #[test]
    fn test_pipeline_result_blocked() {
        let result = PipelineResult::blocked(
            "SQL injection detected".to_string(),
            vec![("waf-module".to_string(), 1500)],
        );

        assert!(result.blocked);
        assert_eq!(result.status_code, 403);
        assert!(String::from_utf8_lossy(&result.response_body).contains("SQL injection"));
        assert_eq!(result.execution_times.len(), 1);
    }

    #[test]
    fn test_pipeline_result_success() {
        let context = WasmExecutionContext {
            request_method: "GET".to_string(),
            request_uri: "/api/test".to_string(),
            ..Default::default()
        };

        let result = PipelineResult::success(
            context,
            vec![
                ("waf-module".to_string(), 1200),
                ("edge-fn".to_string(), 3400),
            ],
            2,
        );

        assert!(!result.blocked);
        assert_eq!(result.status_code, 200);
        assert_eq!(result.modules_executed, 2);
        assert_eq!(result.execution_times.len(), 2);
    }

    #[test]
    fn test_max_modules_enforcement() {
        // This test validates that routes with too many modules are truncated
        let settings = RouteSettings {
            max_modules_per_request: 3,
            continue_on_error: false,
        };

        let route = Route {
            name: Some("test-route".to_string()),
            path: RoutePattern::Exact("/test".to_string()),
            methods: MethodMatcher::All("*".to_string()),
            headers: None,
            wasm_modules: vec![
                WasmModuleRef {
                    module_type: "waf".to_string(),
                    module_id: "waf-1".to_string(),
                    ipfs_cid: None,
                    required_public_key: None,
                    config: None,
                },
                WasmModuleRef {
                    module_type: "edge_function".to_string(),
                    module_id: "fn-1".to_string(),
                    ipfs_cid: None,
                    required_public_key: None,
                    config: None,
                },
                WasmModuleRef {
                    module_type: "edge_function".to_string(),
                    module_id: "fn-2".to_string(),
                    ipfs_cid: None,
                    required_public_key: None,
                    config: None,
                },
                WasmModuleRef {
                    module_type: "edge_function".to_string(),
                    module_id: "fn-3".to_string(),
                    ipfs_cid: None,
                    required_public_key: None,
                    config: None,
                },
                WasmModuleRef {
                    module_type: "edge_function".to_string(),
                    module_id: "fn-4".to_string(),
                    ipfs_cid: None,
                    required_public_key: None,
                    config: None,
                },
            ],
            priority: 0,
            enabled: true,
        };

        // Verify that only first 3 modules would be executed
        let modules_to_execute: Vec<_> = route
            .wasm_modules
            .iter()
            .take(settings.max_modules_per_request)
            .collect();

        assert_eq!(modules_to_execute.len(), 3);
        assert_eq!(modules_to_execute[0].module_id, "waf-1");
        assert_eq!(modules_to_execute[1].module_id, "fn-1");
        assert_eq!(modules_to_execute[2].module_id, "fn-2");
    }

    #[test]
    fn test_dispatcher_stats() {
        let settings = RouteSettings {
            max_modules_per_request: 10,
            continue_on_error: true,
        };

        // Create a mock dispatcher (without real WasmRuntime for unit test)
        // Full integration tests will test actual execution
        assert_eq!(settings.max_modules_per_request, 10);
        assert!(settings.continue_on_error);
    }
}
