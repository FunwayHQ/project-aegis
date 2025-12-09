//! AEGIS DNS Infrastructure
//!
//! Sprint 30.1: DNS Core Server
//! Sprint 30.2: DNS Management API & Usage Metering
//! Sprint 30.3: Geo-Aware DNS Resolution
//! Sprint 30.4: DNSSEC Implementation
//! Sprint 30.5: DNS over HTTPS (DoH) & DNS over TLS (DoT)
//!
//! Provides authoritative DNS services for the AEGIS decentralized edge network,
//! enabling automatic traffic routing through edge nodes via nameserver delegation.
//!
//! ## Architecture
//!
//! ```text
//! User → DNS Query → AEGIS DNS Server → Geo Resolver → Returns Nearest Edge Node IPs
//!                         ↓                   ↓
//!                   Zone Store          Edge Registry
//!                   (in-memory)         (healthy nodes)
//!                         ↓                   ↓
//!                   Rate Limiter        Health Checker
//!                   (DoS protection)    (background monitoring)
//!                         ↓
//!                   DNSSEC Signer → Signed Zones Cache
//!
//! Admin → HTTP API → Zone/Record CRUD → Persistence (SQLite)
//!                         ↓
//!                   Metering → Usage Analytics
//!                         ↓
//!                   Account Manager → Tier Enforcement
//! ```
//!
//! ## Components
//!
//! - `dns_types`: DNS record type definitions (A, AAAA, CNAME, MX, TXT, RRSIG, DNSKEY, etc.)
//! - `dns_config`: Server configuration (ports, rate limits, DNSSEC settings)
//! - `zone_store`: In-memory zone storage with thread-safe access
//! - `dns_server`: UDP/TCP DNS server using Hickory DNS
//! - `rate_limiter`: Token bucket rate limiting for DoS protection
//! - `dns_api`: HTTP API for zone and record management
//! - `dns_persistence`: SQLite storage for zone durability
//! - `dns_metering`: Usage analytics and query statistics
//! - `dns_account`: Account tier management and feature gates
//! - `edge_registry`: Registry of edge nodes with geographic data
//! - `health_checker`: Background health monitoring for edge nodes
//! - `geo_resolver`: Geographic resolution using client location
//! - `dnssec_keys`: DNSSEC key management (ZSK/KSK, Ed25519)
//! - `dnssec`: DNSSEC zone signing, RRSIG/NSEC generation, re-signing
//! - `dot_server`: DNS over TLS server (port 853, RFC 7858)
//! - `doh_server`: DNS over HTTPS server (RFC 8484)

pub mod dns_types;
pub mod dns_config;
pub mod zone_store;
pub mod dns_server;
pub mod rate_limiter;
pub mod dns_api;
pub mod dns_persistence;
pub mod dns_metering;
pub mod dns_account;
pub mod edge_registry;
pub mod health_checker;
pub mod geo_resolver;
pub mod dnssec_keys;
pub mod dnssec;
pub mod dot_server;
pub mod doh_server;

pub use dns_types::*;
pub use dns_config::*;
pub use zone_store::*;
pub use dns_server::*;
pub use rate_limiter::*;
pub use dns_api::*;
pub use dns_persistence::*;
pub use dns_metering::*;
pub use dns_account::*;
pub use edge_registry::*;
pub use health_checker::*;
pub use geo_resolver::*;
pub use dnssec_keys::*;
pub use dnssec::*;
pub use dot_server::*;
pub use doh_server::*;
