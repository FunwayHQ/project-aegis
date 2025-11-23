// Library interface for AEGIS node components
// Allows testing and reuse of proxy logic

pub mod bot_management;
pub mod cache;
pub mod config;
pub mod metrics;
pub mod pingora_proxy;
pub mod proxy;
pub mod server;
pub mod waf;

#[cfg(target_os = "linux")]
pub mod ebpf_loader;

// Sprint 10: P2P Threat Intelligence
pub mod threat_intel_p2p;

#[cfg(target_os = "linux")]
pub mod threat_intel_service;

// Sprint 11: Global State Sync (CRDTs + NATS)
pub mod distributed_counter;
pub mod distributed_rate_limiter;
pub mod nats_sync;

// Sprint 12: Verifiable Analytics
pub mod verifiable_metrics;
pub mod verifiable_metrics_api;

// Sprint 12.5: Security Polish & Resilience
pub mod ip_extraction;
pub mod blocklist_persistence;

// Sprint 13: Wasm Edge Functions Runtime & WAF Migration
pub mod wasm_runtime;

// Sprint 16: Route-based Dispatch for Wasm Modules
pub mod route_config;
pub mod module_dispatcher;
