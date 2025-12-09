//! AEGIS DNS Infrastructure
//!
//! Sprint 30.1: DNS Core Server
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
//! ```
//!
//! ## Components
//!
//! - `dns_types`: DNS record type definitions (A, AAAA, CNAME, MX, TXT, etc.)
//! - `dns_config`: Server configuration (ports, rate limits, DNSSEC settings)
//! - `zone_store`: In-memory zone storage with thread-safe access
//! - `dns_server`: UDP/TCP DNS server using Hickory DNS
//! - `rate_limiter`: Token bucket rate limiting for DoS protection

pub mod dns_types;
pub mod dns_config;
pub mod zone_store;
pub mod dns_server;
pub mod rate_limiter;

pub use dns_types::*;
pub use dns_config::*;
pub use zone_store::*;
pub use dns_server::*;
pub use rate_limiter::*;
