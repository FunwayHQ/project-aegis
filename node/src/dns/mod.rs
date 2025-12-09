//! AEGIS DNS Infrastructure
//!
//! Sprint 30.1: DNS Core Server
//! Sprint 30.2: DNS Management API & Usage Metering
//!
//! Provides authoritative DNS services for the AEGIS decentralized edge network,
//! enabling automatic traffic routing through edge nodes via nameserver delegation.
//!
//! ## Architecture
//!
//! ```text
//! User → DNS Query → AEGIS DNS Server → Returns Edge Node IPs
//!                         ↓
//!                   Zone Store (in-memory)
//!                         ↓
//!                   Rate Limiter (DoS protection)
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
//! - `dns_types`: DNS record type definitions (A, AAAA, CNAME, MX, TXT, etc.)
//! - `dns_config`: Server configuration (ports, rate limits, DNSSEC settings)
//! - `zone_store`: In-memory zone storage with thread-safe access
//! - `dns_server`: UDP/TCP DNS server using Hickory DNS
//! - `rate_limiter`: Token bucket rate limiting for DoS protection
//! - `dns_api`: HTTP API for zone and record management
//! - `dns_persistence`: SQLite storage for zone durability
//! - `dns_metering`: Usage analytics and query statistics
//! - `dns_account`: Account tier management and feature gates

pub mod dns_types;
pub mod dns_config;
pub mod zone_store;
pub mod dns_server;
pub mod rate_limiter;
pub mod dns_api;
pub mod dns_persistence;
pub mod dns_metering;
pub mod dns_account;

pub use dns_types::*;
pub use dns_config::*;
pub use zone_store::*;
pub use dns_server::*;
pub use rate_limiter::*;
pub use dns_api::*;
pub use dns_persistence::*;
pub use dns_metering::*;
pub use dns_account::*;
