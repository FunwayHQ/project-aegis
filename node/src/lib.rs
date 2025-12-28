// Library interface for AEGIS node components
// Allows testing and reuse of proxy logic

// Security utilities
pub mod lock_utils;

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

// Sprint 17: IPFS/Filecoin Integration for Wasm Module Distribution
pub mod ipfs_client;

// Sprint 19: TLS Fingerprinting (JA3/JA4) for Advanced Bot Detection
pub mod tls_fingerprint;
pub mod fingerprint_cache;
pub mod enhanced_bot_detection;
pub mod tls_intercept;

// Sprint 20: JavaScript Challenge System (Turnstile-like)
pub mod challenge;
pub mod challenge_api;

// Sprint 21: Behavioral Analysis & Trust Scoring
pub mod behavioral_analysis;

// Sprint 22: Enhanced WAF with OWASP CRS & ML Anomaly Scoring
pub mod waf_enhanced;

// Sprint 23: API Security Suite
pub mod api_security;

// Sprint 24: Distributed Enforcement & Global Blocklist Sync
pub mod distributed_enforcement;

// DDoS Protection for Websites - Full Stack Implementation
pub mod ddos_policy;
pub mod ddos_stats;
pub mod ddos_manager;
pub mod ddos_api;

// Sprint 30: DNS Infrastructure
pub mod dns;

// Sprint 30.5: ACME/Let's Encrypt Certificate Automation
pub mod acme;
