# AEGIS DNS Infrastructure - Implementation Plan

## Overview

Implement a complete authoritative DNS infrastructure for AEGIS, enabling automatic traffic routing through the decentralized edge network. This is the critical missing piece that allows seamless onboarding (users just change nameservers).

**Core Components:**
- **Authoritative DNS Server** (Rust/Hickory DNS) - Responds to DNS queries
- **Zone Management API** (Hyper) - CRUD for DNS records
- **Geo-Aware Resolution** - Return nearest healthy edge node
- **DNSSEC** - Cryptographic signing for security
- **DNS over HTTPS/TLS** - Encrypted DNS protocols

**Library Choice:** Hickory DNS 0.25.x (formerly Trust-DNS)
- Most mature Rust DNS library
- DNSSEC support built-in
- Backed by ISRG/Prossimo for memory safety
- Requires custom hardening (TCP limits, rate limiting)

---

## Sprint 30.1: DNS Core Server

### Objective
Build the foundational DNS server that can respond to A/AAAA/CNAME queries for configured zones.

### New Files
```
/node/src/dns/
├── mod.rs              # Module exports
├── dns_server.rs       # UDP/TCP DNS server (Hickory)
├── dns_config.rs       # Configuration structs
├── zone_store.rs       # In-memory zone storage
└── dns_types.rs        # DNS record type definitions
```

### LLM Prompt for Sprint 30.1

```
You are implementing the DNS core server for the AEGIS decentralized edge network.

## Context
AEGIS is a decentralized CDN/edge network (like Cloudflare but decentralized). We need authoritative DNS so users can point their nameservers to AEGIS and have all traffic automatically routed through our edge nodes.

## Technical Requirements

### 1. Create `/node/src/dns/mod.rs`
Export all submodules:
- dns_server
- dns_config
- zone_store
- dns_types

### 2. Create `/node/src/dns/dns_types.rs`
Define DNS record types:

```rust
use std::net::{Ipv4Addr, Ipv6Addr};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DnsRecordType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    NS,
    SOA,
    CAA,
    SRV,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    pub name: String,           // e.g., "www" or "@" for root
    pub record_type: DnsRecordType,
    pub ttl: u32,               // Time to live in seconds
    pub value: DnsRecordValue,
    pub priority: Option<u16>,  // For MX/SRV records
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsRecordValue {
    A(Ipv4Addr),
    AAAA(Ipv6Addr),
    CNAME(String),
    MX { exchange: String },
    TXT(String),
    NS(String),
    SOA {
        mname: String,      // Primary nameserver
        rname: String,      // Admin email (with . instead of @)
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    CAA { flags: u8, tag: String, value: String },
    SRV { weight: u16, port: u16, target: String },
}
```

### 3. Create `/node/src/dns/dns_config.rs`
Configuration for the DNS server:

```rust
use serde::{Serialize, Deserialize};
use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    /// UDP listen address (default: 0.0.0.0:53)
    pub udp_addr: SocketAddr,

    /// TCP listen address (default: 0.0.0.0:53)
    pub tcp_addr: SocketAddr,

    /// Enable DNS over TLS (port 853)
    pub dot_enabled: bool,
    pub dot_addr: Option<SocketAddr>,
    pub dot_cert_path: Option<String>,
    pub dot_key_path: Option<String>,

    /// Enable DNS over HTTPS (port 443)
    pub doh_enabled: bool,
    pub doh_addr: Option<SocketAddr>,

    /// DNSSEC signing
    pub dnssec_enabled: bool,
    pub dnssec_key_path: Option<String>,

    /// Rate limiting
    pub rate_limit_per_ip: u32,      // Queries per second per IP
    pub rate_limit_burst: u32,        // Burst allowance

    /// TCP connection limits (DoS protection)
    pub tcp_max_connections: usize,
    pub tcp_max_per_ip: usize,
    pub tcp_idle_timeout_secs: u64,

    /// AXFR (zone transfer) settings
    pub axfr_enabled: bool,
    pub axfr_allowed_ips: Vec<String>,  // IPs allowed to request zone transfers

    /// Anycast IP to return for proxied records
    pub aegis_anycast_ipv4: Option<String>,
    pub aegis_anycast_ipv6: Option<String>,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            udp_addr: "0.0.0.0:53".parse().unwrap(),
            tcp_addr: "0.0.0.0:53".parse().unwrap(),
            dot_enabled: false,
            dot_addr: None,
            dot_cert_path: None,
            dot_key_path: None,
            doh_enabled: false,
            doh_addr: None,
            dnssec_enabled: false,
            dnssec_key_path: None,
            rate_limit_per_ip: 100,
            rate_limit_burst: 500,
            tcp_max_connections: 10000,
            tcp_max_per_ip: 10,
            tcp_idle_timeout_secs: 60,
            axfr_enabled: false,
            axfr_allowed_ips: vec![],
            aegis_anycast_ipv4: None,
            aegis_anycast_ipv6: None,
        }
    }
}

impl DnsConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.rate_limit_per_ip == 0 {
            anyhow::bail!("rate_limit_per_ip must be > 0");
        }
        if self.tcp_max_connections == 0 {
            anyhow::bail!("tcp_max_connections must be > 0");
        }
        if self.dot_enabled && (self.dot_cert_path.is_none() || self.dot_key_path.is_none()) {
            anyhow::bail!("DoT requires cert and key paths");
        }
        Ok(())
    }
}
```

### 4. Create `/node/src/dns/zone_store.rs`
In-memory zone storage with thread-safe access:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::dns::dns_types::{DnsRecord, DnsRecordType};

#[derive(Debug, Clone)]
pub struct Zone {
    pub domain: String,           // e.g., "example.com"
    pub records: Vec<DnsRecord>,
    pub proxied: bool,            // If true, return AEGIS anycast IP for A/AAAA
    pub created_at: u64,
    pub updated_at: u64,
}

pub struct ZoneStore {
    zones: Arc<RwLock<HashMap<String, Zone>>>,
}

impl ZoneStore {
    pub fn new() -> Self {
        Self {
            zones: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a zone
    pub async fn upsert_zone(&self, zone: Zone) -> anyhow::Result<()>

    /// Get a zone by domain
    pub async fn get_zone(&self, domain: &str) -> Option<Zone>

    /// Delete a zone
    pub async fn delete_zone(&self, domain: &str) -> bool

    /// List all zones
    pub async fn list_zones(&self) -> Vec<Zone>

    /// Find records for a query (handles subdomain matching)
    pub async fn resolve(&self, qname: &str, qtype: DnsRecordType) -> Option<Vec<DnsRecord>>

    /// Add a record to a zone
    pub async fn add_record(&self, domain: &str, record: DnsRecord) -> anyhow::Result<()>

    /// Remove a record from a zone
    pub async fn remove_record(&self, domain: &str, record_name: &str, record_type: DnsRecordType) -> anyhow::Result<()>
}
```

### 5. Create `/node/src/dns/dns_server.rs`
The main DNS server using Hickory DNS:

```rust
use hickory_server::ServerFuture;
use hickory_server::authority::{Authority, Catalog};
use hickory_proto::op::{Header, ResponseCode};
use tokio::net::{UdpSocket, TcpListener};
use std::sync::Arc;
use crate::dns::{DnsConfig, ZoneStore};

pub struct DnsServer {
    config: DnsConfig,
    zone_store: Arc<ZoneStore>,
    // Rate limiter for DoS protection
    rate_limiter: Arc<RateLimiter>,
    // TCP connection tracker
    tcp_tracker: Arc<TcpConnectionTracker>,
}

impl DnsServer {
    pub fn new(config: DnsConfig, zone_store: Arc<ZoneStore>) -> anyhow::Result<Self>

    /// Start the DNS server (UDP + TCP)
    pub async fn run(&self) -> anyhow::Result<()> {
        // 1. Bind UDP socket
        let udp_socket = UdpSocket::bind(&self.config.udp_addr).await?;

        // 2. Bind TCP listener
        let tcp_listener = TcpListener::bind(&self.config.tcp_addr).await?;

        // 3. Create Hickory server
        let mut server = ServerFuture::new(self.create_authority());

        // 4. Register sockets
        server.register_socket(udp_socket);
        server.register_listener(tcp_listener, ...);

        // 5. Run server loop
        server.block_until_done().await?;

        Ok(())
    }

    /// Handle incoming DNS query with rate limiting
    async fn handle_query(&self, query: &Message, client_ip: IpAddr) -> Message

    /// Create authority from zone store
    fn create_authority(&self) -> impl Authority
}

/// Token bucket rate limiter
struct RateLimiter {
    buckets: Arc<RwLock<HashMap<IpAddr, TokenBucket>>>,
    rate: u32,
    burst: u32,
}

/// TCP connection tracker for DoS protection
struct TcpConnectionTracker {
    connections: Arc<RwLock<HashMap<IpAddr, usize>>>,
    max_per_ip: usize,
    max_total: usize,
}
```

### 6. Update `/node/src/lib.rs`
Add the dns module:
```rust
pub mod dns;
```

### 7. Create `/node/src/main_dns.rs`
Entry point binary:

```rust
use aegis_node::dns::{DnsConfig, DnsServer, ZoneStore};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Load config
    let config = DnsConfig::default();
    config.validate()?;

    // Create zone store
    let zone_store = Arc::new(ZoneStore::new());

    // Create and run server
    let server = DnsServer::new(config, zone_store)?;
    server.run().await
}
```

### 8. Update `/node/Cargo.toml`
Add dependencies:
```toml
hickory-server = { version = "0.25", features = ["dns-over-tls", "dnssec-ring"] }
hickory-proto = "0.25"
hickory-resolver = "0.25"
```

Add binary:
```toml
[[bin]]
name = "aegis-dns"
path = "src/main_dns.rs"
```

## Testing Requirements
- Unit tests for ZoneStore CRUD operations
- Unit tests for rate limiter token bucket logic
- Unit tests for DNS record type parsing
- Integration test: UDP query/response
- Integration test: TCP query/response
- Integration test: Rate limiting blocks excessive queries

## Success Criteria
- DNS server starts and listens on UDP/TCP port 53
- Can resolve A records for configured zones
- Rate limiting prevents query floods
- TCP connection limits prevent DoS
- All tests pass
```

---

## Sprint 30.2: DNS Management API

### Objective
Build HTTP API for managing DNS zones and records, following existing AEGIS API patterns.

### New Files
```
/node/src/dns/
├── dns_api.rs          # HTTP API server
└── dns_persistence.rs  # SQLite storage for zones
```

### LLM Prompt for Sprint 30.2

```
You are implementing the DNS Management API for the AEGIS decentralized edge network.

## Context
In Sprint 30.1, we built the core DNS server. Now we need an HTTP API for managing zones and records. Follow the patterns from `/node/src/ddos_api.rs` (Hyper-based, manual routing).

## Technical Requirements

### 1. Create `/node/src/dns/dns_api.rs`
HTTP API following AEGIS patterns:

```rust
use hyper::{Body, Method, Request, Response, StatusCode, Server};
use hyper::service::{make_service_fn, service_fn};
use std::sync::Arc;
use crate::dns::ZoneStore;

pub struct DnsApi {
    zone_store: Arc<ZoneStore>,
}

impl DnsApi {
    pub fn new(zone_store: Arc<ZoneStore>) -> Self {
        Self { zone_store }
    }

    pub async fn handle_request(&self, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        let path = req.uri().path().to_string();
        let method = req.method().clone();

        let result = match (method, path.as_str()) {
            // Health check
            (Method::GET, "/aegis/dns/api/health") => self.handle_health().await,

            // Zone management
            (Method::GET, "/aegis/dns/api/zones") => self.handle_list_zones().await,
            (Method::POST, "/aegis/dns/api/zones") => self.handle_create_zone(req).await,
            (Method::GET, p) if p.starts_with("/aegis/dns/api/zones/") && !p.contains("/records") => {
                let domain = p.trim_start_matches("/aegis/dns/api/zones/");
                self.handle_get_zone(domain).await
            }
            (Method::PUT, p) if p.starts_with("/aegis/dns/api/zones/") && !p.contains("/records") => {
                let domain = p.trim_start_matches("/aegis/dns/api/zones/");
                self.handle_update_zone(domain, req).await
            }
            (Method::DELETE, p) if p.starts_with("/aegis/dns/api/zones/") && !p.contains("/records") => {
                let domain = p.trim_start_matches("/aegis/dns/api/zones/");
                self.handle_delete_zone(domain).await
            }

            // Record management
            (Method::GET, p) if p.contains("/records") => {
                // /aegis/dns/api/zones/{domain}/records
                let parts: Vec<&str> = p.split('/').collect();
                if parts.len() >= 6 {
                    let domain = parts[4];
                    self.handle_list_records(domain).await
                } else {
                    self.not_found()
                }
            }
            (Method::POST, p) if p.contains("/records") => {
                let parts: Vec<&str> = p.split('/').collect();
                if parts.len() >= 6 {
                    let domain = parts[4];
                    self.handle_create_record(domain, req).await
                } else {
                    self.not_found()
                }
            }
            (Method::DELETE, p) if p.contains("/records/") => {
                // /aegis/dns/api/zones/{domain}/records/{record_id}
                let parts: Vec<&str> = p.split('/').collect();
                if parts.len() >= 7 {
                    let domain = parts[4];
                    let record_id = parts[6];
                    self.handle_delete_record(domain, record_id).await
                } else {
                    self.not_found()
                }
            }

            // Nameserver info
            (Method::GET, "/aegis/dns/api/nameservers") => self.handle_get_nameservers().await,

            // Statistics
            (Method::GET, "/aegis/dns/api/stats") => self.handle_get_stats().await,
            (Method::GET, p) if p.starts_with("/aegis/dns/api/stats/") => {
                let domain = p.trim_start_matches("/aegis/dns/api/stats/");
                self.handle_get_zone_stats(domain).await
            }

            _ => self.not_found(),
        };

        Ok(result)
    }
}
```

**API Endpoints:**

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/aegis/dns/api/health` | Health check |
| GET | `/aegis/dns/api/zones` | List all zones |
| POST | `/aegis/dns/api/zones` | Create new zone |
| GET | `/aegis/dns/api/zones/:domain` | Get zone details |
| PUT | `/aegis/dns/api/zones/:domain` | Update zone settings |
| DELETE | `/aegis/dns/api/zones/:domain` | Delete zone |
| GET | `/aegis/dns/api/zones/:domain/records` | List records |
| POST | `/aegis/dns/api/zones/:domain/records` | Create record |
| DELETE | `/aegis/dns/api/zones/:domain/records/:id` | Delete record |
| GET | `/aegis/dns/api/nameservers` | Get AEGIS nameservers |
| GET | `/aegis/dns/api/stats` | Global DNS stats |
| GET | `/aegis/dns/api/stats/:domain` | Per-zone stats |

**Request/Response Formats:**

Create Zone:
```json
POST /aegis/dns/api/zones
{
  "domain": "example.com",
  "proxied": true
}

Response:
{
  "success": true,
  "data": {
    "domain": "example.com",
    "proxied": true,
    "nameservers": [
      "ns1.aegis.network",
      "ns2.aegis.network"
    ],
    "created_at": 1702000000
  }
}
```

Create Record:
```json
POST /aegis/dns/api/zones/example.com/records
{
  "name": "www",
  "type": "A",
  "value": "192.168.1.1",
  "ttl": 300,
  "proxied": true
}

Response:
{
  "success": true,
  "data": {
    "id": "rec_abc123",
    "name": "www",
    "type": "A",
    "value": "192.168.1.1",
    "ttl": 300,
    "proxied": true
  }
}
```

### 2. Create `/node/src/dns/dns_persistence.rs`
SQLite storage for zone persistence:

```rust
use rusqlite::{Connection, params};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::dns::{Zone, DnsRecord};

pub struct DnsPersistence {
    conn: Arc<Mutex<Connection>>,
}

impl DnsPersistence {
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS zones (
                domain TEXT PRIMARY KEY,
                proxied INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                name TEXT NOT NULL,
                record_type TEXT NOT NULL,
                value TEXT NOT NULL,
                ttl INTEGER NOT NULL,
                priority INTEGER,
                proxied INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (domain) REFERENCES zones(domain) ON DELETE CASCADE
            )",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub async fn save_zone(&self, zone: &Zone) -> anyhow::Result<()>
    pub async fn load_zone(&self, domain: &str) -> anyhow::Result<Option<Zone>>
    pub async fn delete_zone(&self, domain: &str) -> anyhow::Result<()>
    pub async fn list_zones(&self) -> anyhow::Result<Vec<Zone>>

    pub async fn save_record(&self, domain: &str, record: &DnsRecord) -> anyhow::Result<String>
    pub async fn delete_record(&self, record_id: &str) -> anyhow::Result<()>
    pub async fn list_records(&self, domain: &str) -> anyhow::Result<Vec<DnsRecord>>

    /// Load all zones into ZoneStore on startup
    pub async fn restore_to_store(&self, store: &ZoneStore) -> anyhow::Result<usize>
}
```

### 3. Update `/node/src/dns/mod.rs`
Add new modules:
```rust
pub mod dns_api;
pub mod dns_persistence;
```

### 4. Create API Server Runner
```rust
pub async fn run_dns_api(addr: SocketAddr, api: Arc<DnsApi>) -> anyhow::Result<()> {
    let make_svc = make_service_fn(move |_conn| {
        let api = Arc::clone(&api);
        async move {
            Ok::<_, std::convert::Infallible>(service_fn(move |req| {
                let api = Arc::clone(&api);
                async move { api.handle_request(req).await }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    tracing::info!("DNS API listening on {}", addr);
    server.await?;
    Ok(())
}
```

### 5. Update `/node/src/main_dns.rs`
Add API server:
```rust
// Start DNS API in background
let api = Arc::new(DnsApi::new(zone_store.clone()));
let api_addr: SocketAddr = "0.0.0.0:8054".parse()?;
tokio::spawn(run_dns_api(api_addr, api));

// Start DNS server
server.run().await
```

## Testing Requirements
- Unit tests for all API endpoints
- Test zone CRUD operations
- Test record CRUD operations
- Test SQLite persistence save/restore
- Test error handling (invalid domain, duplicate records)
- Integration test: Create zone via API, resolve via DNS

## Success Criteria
- API server starts on port 8054
- Can create/list/delete zones via HTTP
- Can create/list/delete records via HTTP
- Zones persist across restarts (SQLite)
- API returns proper error messages
- All tests pass
```

---

## Sprint 30.3: Geo-Aware DNS Resolution

### Objective
Implement intelligent DNS resolution that returns the nearest healthy AEGIS edge node IP based on client location.

### New Files
```
/node/src/dns/
├── geo_resolver.rs     # Geographic resolution logic
├── health_checker.rs   # Edge node health monitoring
└── edge_registry.rs    # Registry of edge node IPs and locations
```

### LLM Prompt for Sprint 30.3

```
You are implementing geo-aware DNS resolution for the AEGIS decentralized edge network.

## Context
AEGIS has edge nodes distributed globally. When a user queries DNS for a proxied domain, we should return the IP of the nearest healthy edge node, not a static anycast IP. This mimics Cloudflare's intelligent routing.

## Technical Requirements

### 1. Create `/node/src/dns/edge_registry.rs`
Registry of edge nodes with their locations:

```rust
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeNode {
    pub id: String,
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub region: String,           // e.g., "us-east", "eu-west", "asia-pacific"
    pub country: String,          // ISO country code
    pub city: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub capacity: u32,            // Relative capacity weight
    pub healthy: bool,
    pub last_health_check: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub country: Option<String>,
    pub region: Option<String>,
}

pub struct EdgeRegistry {
    nodes: Arc<RwLock<HashMap<String, EdgeNode>>>,
    // Index by region for fast lookup
    region_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl EdgeRegistry {
    pub fn new() -> Self

    /// Register a new edge node
    pub async fn register(&self, node: EdgeNode) -> anyhow::Result<()>

    /// Update node health status
    pub async fn update_health(&self, node_id: &str, healthy: bool) -> anyhow::Result<()>

    /// Get all healthy nodes in a region
    pub async fn get_healthy_in_region(&self, region: &str) -> Vec<EdgeNode>

    /// Get all healthy nodes
    pub async fn get_all_healthy(&self) -> Vec<EdgeNode>

    /// Find nearest healthy nodes to a location
    pub async fn find_nearest(&self, location: &GeoLocation, count: usize) -> Vec<EdgeNode>

    /// Remove a node
    pub async fn unregister(&self, node_id: &str) -> bool
}
```

### 2. Create `/node/src/dns/health_checker.rs`
Background health checking for edge nodes:

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use crate::dns::EdgeRegistry;

pub struct HealthChecker {
    registry: Arc<EdgeRegistry>,
    check_interval: Duration,
    timeout: Duration,
    unhealthy_threshold: u32,  // Consecutive failures before marking unhealthy
}

impl HealthChecker {
    pub fn new(registry: Arc<EdgeRegistry>) -> Self {
        Self {
            registry,
            check_interval: Duration::from_secs(10),
            timeout: Duration::from_secs(5),
            unhealthy_threshold: 3,
        }
    }

    /// Start background health checking
    pub async fn run(&self) {
        let mut interval = interval(self.check_interval);

        loop {
            interval.tick().await;
            self.check_all_nodes().await;
        }
    }

    /// Check health of all registered nodes
    async fn check_all_nodes(&self) {
        let nodes = self.registry.get_all_healthy().await;

        for node in nodes {
            let healthy = self.check_node(&node).await;
            self.registry.update_health(&node.id, healthy).await.ok();
        }
    }

    /// Check single node health
    /// Performs HTTP health check to /health endpoint
    async fn check_node(&self, node: &EdgeNode) -> bool {
        // HTTP GET to http://{node.ipv4}/health with timeout
        // Return true if 200 OK, false otherwise
    }
}
```

### 3. Create `/node/src/dns/geo_resolver.rs`
Geographic resolution logic:

```rust
use std::sync::Arc;
use std::net::IpAddr;
use crate::dns::{EdgeRegistry, GeoLocation, DnsRecord, DnsRecordValue};

/// MaxMind GeoIP database integration (optional)
pub struct GeoIpDatabase {
    // Uses maxminddb crate for GeoLite2 database
}

pub struct GeoResolver {
    registry: Arc<EdgeRegistry>,
    geoip: Option<GeoIpDatabase>,
    fallback_nodes: Vec<IpAddr>,  // If all else fails
}

impl GeoResolver {
    pub fn new(registry: Arc<EdgeRegistry>) -> Self

    /// Load MaxMind GeoLite2 database
    pub fn with_geoip(mut self, db_path: &str) -> anyhow::Result<Self>

    /// Resolve client IP to geographic location
    pub fn locate_client(&self, client_ip: IpAddr) -> Option<GeoLocation> {
        // Use GeoIP database to find client location
        // Returns lat/lon and country/region
    }

    /// Get best edge node IPs for a client
    pub async fn resolve_for_client(
        &self,
        client_ip: IpAddr,
        record_type: &str,  // "A" or "AAAA"
    ) -> Vec<IpAddr> {
        // 1. Locate client geographically
        let location = self.locate_client(client_ip);

        // 2. Find nearest healthy nodes
        let nodes = match location {
            Some(loc) => self.registry.find_nearest(&loc, 3).await,
            None => self.registry.get_all_healthy().await,
        };

        // 3. Extract IPs of requested type
        let ips: Vec<IpAddr> = nodes.iter()
            .filter_map(|n| match record_type {
                "A" => n.ipv4.map(IpAddr::V4),
                "AAAA" => n.ipv6.map(IpAddr::V6),
                _ => None,
            })
            .take(3)  // Return up to 3 IPs
            .collect();

        // 4. Fallback if no nodes available
        if ips.is_empty() {
            return self.fallback_nodes.clone();
        }

        ips
    }

    /// Calculate distance between two points (Haversine formula)
    fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
        const R: f64 = 6371.0; // Earth radius in km

        let d_lat = (lat2 - lat1).to_radians();
        let d_lon = (lon2 - lon1).to_radians();

        let a = (d_lat / 2.0).sin().powi(2)
            + lat1.to_radians().cos()
            * lat2.to_radians().cos()
            * (d_lon / 2.0).sin().powi(2);

        let c = 2.0 * a.sqrt().asin();

        R * c
    }
}
```

### 4. Update `/node/src/dns/dns_server.rs`
Integrate geo resolution:

```rust
impl DnsServer {
    // Add geo_resolver field
    geo_resolver: Arc<GeoResolver>,

    /// Handle query with geo-aware resolution
    async fn handle_query(&self, query: &Message, client_ip: IpAddr) -> Message {
        // For proxied records, use geo resolver
        if self.is_proxied_record(query) {
            let qtype = query.query().query_type();
            let ips = self.geo_resolver.resolve_for_client(
                client_ip,
                match qtype {
                    RecordType::A => "A",
                    RecordType::AAAA => "AAAA",
                    _ => return self.handle_regular_query(query),
                }
            ).await;

            return self.build_response_with_ips(query, ips);
        }

        self.handle_regular_query(query)
    }
}
```

### 5. Add API endpoints for edge registry
Update `/node/src/dns/dns_api.rs`:

```rust
// Add endpoints:
// GET /aegis/dns/api/edges - List edge nodes
// POST /aegis/dns/api/edges - Register edge node
// DELETE /aegis/dns/api/edges/:id - Unregister edge node
// GET /aegis/dns/api/edges/:id/health - Get node health
// POST /aegis/dns/api/edges/:id/health - Update node health (internal)
```

### 6. Update Cargo.toml
Add dependencies:
```toml
maxminddb = "0.24"  # GeoIP database reader
```

## Testing Requirements
- Unit tests for Haversine distance calculation
- Unit tests for edge registry CRUD
- Unit tests for nearest node selection
- Integration test: Client from US gets US node IP
- Integration test: Unhealthy node is skipped
- Integration test: Fallback when no nodes available

## Success Criteria
- Edge nodes can register with location data
- Health checker runs in background
- DNS queries return nearest healthy node IP
- Unhealthy nodes are excluded from responses
- GeoIP lookup works for client IP resolution
- All tests pass
```

---

## Sprint 30.4: DNSSEC Implementation

### Objective
Implement DNSSEC signing for zone records to provide cryptographic authentication of DNS responses.

### New Files
```
/node/src/dns/
├── dnssec.rs           # DNSSEC signing logic
└── dnssec_keys.rs      # Key management
```

### LLM Prompt for Sprint 30.4

```
You are implementing DNSSEC for the AEGIS decentralized edge network.

## Context
DNSSEC adds cryptographic signatures to DNS responses, allowing resolvers to verify that responses haven't been tampered with. This is critical for security-conscious users.

## Technical Requirements

### 1. Create `/node/src/dns/dnssec_keys.rs`
Key management for DNSSEC:

```rust
use ring::signature::{Ed25519KeyPair, KeyPair};
use ring::rand::SystemRandom;
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub enum DnssecAlgorithm {
    RSASHA256,      // Algorithm 8
    ECDSAP256SHA256, // Algorithm 13
    ED25519,        // Algorithm 15 (recommended)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnssecKeyPair {
    pub key_tag: u16,
    pub algorithm: u8,
    pub flags: u16,          // 256 for ZSK, 257 for KSK
    pub public_key: Vec<u8>,
    #[serde(skip)]
    private_key: Option<Vec<u8>>,
}

pub struct DnssecKeyManager {
    keys: HashMap<String, DnssecKeyPair>,  // domain -> key
    rng: SystemRandom,
}

impl DnssecKeyManager {
    pub fn new() -> Self

    /// Generate new key pair for a domain
    pub fn generate_key(&mut self, domain: &str, algorithm: DnssecAlgorithm) -> anyhow::Result<DnssecKeyPair>

    /// Load key from file
    pub fn load_key(&mut self, domain: &str, path: &Path) -> anyhow::Result<DnssecKeyPair>

    /// Save key to file
    pub fn save_key(&self, domain: &str, path: &Path) -> anyhow::Result<()>

    /// Get public key for DS record (to give to registrar)
    pub fn get_ds_record(&self, domain: &str) -> Option<String>

    /// Calculate key tag from public key
    fn calculate_key_tag(flags: u16, algorithm: u8, public_key: &[u8]) -> u16
}
```

### 2. Create `/node/src/dns/dnssec.rs`
DNSSEC signing implementation:

```rust
use hickory_proto::rr::dnssec::{Algorithm, SigSigner};
use hickory_proto::rr::{DNSClass, Name, RData, Record, RecordType};
use crate::dns::{DnssecKeyPair, Zone, DnsRecord};
use std::time::{SystemTime, Duration, UNIX_EPOCH};

pub struct DnssecSigner {
    key_manager: Arc<DnssecKeyManager>,
    signature_validity: Duration,  // How long signatures are valid
    inception_offset: Duration,    // Start validity slightly in past
}

impl DnssecSigner {
    pub fn new(key_manager: Arc<DnssecKeyManager>) -> Self {
        Self {
            key_manager,
            signature_validity: Duration::from_secs(86400 * 30), // 30 days
            inception_offset: Duration::from_secs(3600),         // 1 hour
        }
    }

    /// Sign all records in a zone
    pub fn sign_zone(&self, zone: &Zone) -> anyhow::Result<SignedZone> {
        let key = self.key_manager.get_key(&zone.domain)?;

        // Group records by name and type (RRset)
        let rrsets = self.group_into_rrsets(&zone.records);

        let mut signed_records = Vec::new();

        for rrset in rrsets {
            // Sign each RRset
            let rrsig = self.sign_rrset(&zone.domain, &rrset, &key)?;
            signed_records.extend(rrset.records);
            signed_records.push(rrsig);
        }

        // Add DNSKEY record
        signed_records.push(self.create_dnskey_record(&zone.domain, &key));

        // Generate NSEC/NSEC3 records for authenticated denial
        let nsec_records = self.generate_nsec_chain(&zone.domain, &signed_records);
        signed_records.extend(nsec_records);

        Ok(SignedZone {
            domain: zone.domain.clone(),
            records: signed_records,
        })
    }

    /// Sign a single RRset
    fn sign_rrset(
        &self,
        domain: &str,
        rrset: &RRset,
        key: &DnssecKeyPair,
    ) -> anyhow::Result<DnsRecord> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as u32;
        let inception = now - self.inception_offset.as_secs() as u32;
        let expiration = now + self.signature_validity.as_secs() as u32;

        // Create signature data
        let sig_data = self.create_rrsig_data(
            rrset,
            key.algorithm,
            domain,
            key.key_tag,
            inception,
            expiration,
        );

        // Sign with private key
        let signature = self.sign_data(&sig_data, key)?;

        // Return RRSIG record
        Ok(DnsRecord {
            name: rrset.name.clone(),
            record_type: DnsRecordType::RRSIG,
            ttl: rrset.ttl,
            value: DnsRecordValue::RRSIG {
                type_covered: rrset.record_type.clone(),
                algorithm: key.algorithm,
                labels: self.count_labels(&rrset.name),
                original_ttl: rrset.ttl,
                expiration,
                inception,
                key_tag: key.key_tag,
                signer_name: domain.to_string(),
                signature,
            },
            priority: None,
        })
    }

    /// Generate NSEC chain for authenticated denial of existence
    fn generate_nsec_chain(&self, domain: &str, records: &[DnsRecord]) -> Vec<DnsRecord>

    /// Create DNSKEY record from key pair
    fn create_dnskey_record(&self, domain: &str, key: &DnssecKeyPair) -> DnsRecord
}

#[derive(Debug)]
pub struct SignedZone {
    pub domain: String,
    pub records: Vec<DnsRecord>,
}

#[derive(Debug)]
struct RRset {
    name: String,
    record_type: DnsRecordType,
    ttl: u32,
    records: Vec<DnsRecord>,
}
```

### 3. Update `/node/src/dns/dns_types.rs`
Add DNSSEC record types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DnsRecordType {
    // ... existing types ...
    DNSKEY,
    RRSIG,
    NSEC,
    NSEC3,
    NSEC3PARAM,
    DS,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsRecordValue {
    // ... existing values ...
    DNSKEY {
        flags: u16,
        protocol: u8,
        algorithm: u8,
        public_key: Vec<u8>,
    },
    RRSIG {
        type_covered: DnsRecordType,
        algorithm: u8,
        labels: u8,
        original_ttl: u32,
        expiration: u32,
        inception: u32,
        key_tag: u16,
        signer_name: String,
        signature: Vec<u8>,
    },
    NSEC {
        next_domain: String,
        types: Vec<DnsRecordType>,
    },
    DS {
        key_tag: u16,
        algorithm: u8,
        digest_type: u8,
        digest: Vec<u8>,
    },
}
```

### 4. Update `/node/src/dns/dns_server.rs`
Integrate DNSSEC:

```rust
impl DnsServer {
    // Add fields
    dnssec_signer: Option<Arc<DnssecSigner>>,
    signed_zones: Arc<RwLock<HashMap<String, SignedZone>>>,

    /// Sign zones on startup or when updated
    pub async fn sign_all_zones(&self) -> anyhow::Result<()> {
        let zones = self.zone_store.list_zones().await;

        for zone in zones {
            if let Some(signer) = &self.dnssec_signer {
                let signed = signer.sign_zone(&zone)?;
                self.signed_zones.write().await.insert(zone.domain.clone(), signed);
            }
        }

        Ok(())
    }

    /// Handle DNSKEY queries
    async fn handle_dnskey_query(&self, domain: &str) -> Option<Vec<DnsRecord>>

    /// Add RRSIG to response if DNSSEC requested (DO flag)
    fn add_dnssec_records(&self, response: &mut Message, domain: &str)
}
```

### 5. Add API endpoints for DNSSEC
Update `/node/src/dns/dns_api.rs`:

```rust
// Add endpoints:
// GET /aegis/dns/api/zones/:domain/dnssec - Get DNSSEC status
// POST /aegis/dns/api/zones/:domain/dnssec/enable - Enable DNSSEC
// POST /aegis/dns/api/zones/:domain/dnssec/disable - Disable DNSSEC
// GET /aegis/dns/api/zones/:domain/dnssec/ds - Get DS record for registrar
// POST /aegis/dns/api/zones/:domain/dnssec/resign - Force re-signing
```

### 6. Background re-signing task
Signatures expire, so we need to re-sign periodically:

```rust
pub async fn run_resignation_task(
    signer: Arc<DnssecSigner>,
    zone_store: Arc<ZoneStore>,
    signed_zones: Arc<RwLock<HashMap<String, SignedZone>>>,
) {
    let mut interval = interval(Duration::from_secs(86400)); // Daily

    loop {
        interval.tick().await;

        // Re-sign all zones
        for zone in zone_store.list_zones().await {
            if let Ok(signed) = signer.sign_zone(&zone) {
                signed_zones.write().await.insert(zone.domain.clone(), signed);
            }
        }
    }
}
```

## Testing Requirements
- Unit tests for key generation (Ed25519)
- Unit tests for key tag calculation
- Unit tests for RRSIG generation
- Unit tests for NSEC chain generation
- Integration test: Query with DO flag returns RRSIG
- Integration test: DNSKEY query returns valid key
- Validation test: Signed responses validate with external tool (dig +dnssec)

## Success Criteria
- Can generate DNSSEC keys for zones
- RRSIG records are generated for all RRsets
- DNSKEY record is served
- NSEC chain provides authenticated denial
- DS record can be exported for registrar
- Signatures re-generate before expiration
- All tests pass
```

---

## Sprint 30.5: DNS over HTTPS (DoH) & DNS over TLS (DoT)

### Objective
Implement encrypted DNS protocols for privacy and security.

### New Files
```
/node/src/dns/
├── doh_server.rs       # DNS over HTTPS
└── dot_server.rs       # DNS over TLS
```

### LLM Prompt for Sprint 30.5

```
You are implementing encrypted DNS protocols (DoH and DoT) for the AEGIS decentralized edge network.

## Context
DNS queries are traditionally unencrypted, allowing ISPs and attackers to snoop. DNS over HTTPS (DoH) and DNS over TLS (DoT) encrypt queries for privacy.

## Technical Requirements

### 1. Create `/node/src/dns/dot_server.rs`
DNS over TLS (port 853):

```rust
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use rustls::{Certificate, PrivateKey, ServerConfig};
use std::sync::Arc;
use crate::dns::DnsServer;

pub struct DotServer {
    dns_server: Arc<DnsServer>,
    tls_config: Arc<ServerConfig>,
    addr: SocketAddr,
}

impl DotServer {
    pub fn new(
        dns_server: Arc<DnsServer>,
        cert_path: &str,
        key_path: &str,
        addr: SocketAddr,
    ) -> anyhow::Result<Self> {
        // Load certificate and key
        let certs = load_certs(cert_path)?;
        let key = load_private_key(key_path)?;

        // Create TLS config
        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;

        Ok(Self {
            dns_server,
            tls_config: Arc::new(config),
            addr,
        })
    }

    /// Start DoT server
    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.addr).await?;
        let acceptor = TlsAcceptor::from(self.tls_config.clone());

        tracing::info!("DoT server listening on {}", self.addr);

        loop {
            let (stream, client_addr) = listener.accept().await?;
            let acceptor = acceptor.clone();
            let dns_server = Arc::clone(&self.dns_server);

            tokio::spawn(async move {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        Self::handle_client(dns_server, tls_stream, client_addr).await;
                    }
                    Err(e) => {
                        tracing::warn!("TLS handshake failed: {}", e);
                    }
                }
            });
        }
    }

    /// Handle single DoT client connection
    async fn handle_client(
        dns_server: Arc<DnsServer>,
        mut stream: TlsStream<TcpStream>,
        client_addr: SocketAddr,
    ) {
        // DoT uses TCP-style length-prefixed messages
        // 2-byte length prefix + DNS message
        loop {
            // Read length prefix
            let mut len_buf = [0u8; 2];
            if stream.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u16::from_be_bytes(len_buf) as usize;

            // Read DNS message
            let mut msg_buf = vec![0u8; len];
            if stream.read_exact(&mut msg_buf).await.is_err() {
                break;
            }

            // Parse and handle query
            let query = Message::from_vec(&msg_buf)?;
            let response = dns_server.handle_query(&query, client_addr.ip()).await;

            // Send response with length prefix
            let response_bytes = response.to_vec()?;
            let len_prefix = (response_bytes.len() as u16).to_be_bytes();
            stream.write_all(&len_prefix).await?;
            stream.write_all(&response_bytes).await?;
        }
    }
}

fn load_certs(path: &str) -> anyhow::Result<Vec<Certificate>>
fn load_private_key(path: &str) -> anyhow::Result<PrivateKey>
```

### 2. Create `/node/src/dns/doh_server.rs`
DNS over HTTPS (port 443):

```rust
use hyper::{Body, Method, Request, Response, StatusCode, Server};
use hyper::service::{make_service_fn, service_fn};
use hyper_rustls::TlsAcceptor;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::sync::Arc;
use crate::dns::DnsServer;

pub struct DohServer {
    dns_server: Arc<DnsServer>,
    tls_config: Arc<ServerConfig>,
    addr: SocketAddr,
}

impl DohServer {
    pub fn new(
        dns_server: Arc<DnsServer>,
        cert_path: &str,
        key_path: &str,
        addr: SocketAddr,
    ) -> anyhow::Result<Self>

    /// Start DoH server
    pub async fn run(&self) -> anyhow::Result<()> {
        // Create HTTPS server with TLS
        let acceptor = TlsAcceptor::from(self.tls_config.clone());

        let dns_server = Arc::clone(&self.dns_server);
        let make_svc = make_service_fn(move |_conn| {
            let dns_server = Arc::clone(&dns_server);
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    let dns_server = Arc::clone(&dns_server);
                    Self::handle_request(dns_server, req)
                }))
            }
        });

        tracing::info!("DoH server listening on {}", self.addr);

        // Bind with TLS
        let server = Server::builder(acceptor)
            .serve(make_svc);

        server.await?;
        Ok(())
    }

    /// Handle DoH request
    /// Supports both GET (dns parameter) and POST (body) methods
    async fn handle_request(
        dns_server: Arc<DnsServer>,
        req: Request<Body>,
    ) -> Result<Response<Body>, hyper::Error> {
        let path = req.uri().path();

        // Only handle /dns-query path (RFC 8484)
        if path != "/dns-query" {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap());
        }

        // Extract client IP from X-Forwarded-For or connection
        let client_ip = Self::extract_client_ip(&req);

        let dns_message = match *req.method() {
            Method::GET => {
                // GET: DNS query in ?dns= parameter (base64url encoded)
                Self::parse_get_query(&req)?
            }
            Method::POST => {
                // POST: DNS query in body (application/dns-message)
                let content_type = req.headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok());

                if content_type != Some("application/dns-message") {
                    return Ok(Response::builder()
                        .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                        .body(Body::empty())
                        .unwrap());
                }

                let body = hyper::body::to_bytes(req.into_body()).await?;
                Message::from_vec(&body)?
            }
            _ => {
                return Ok(Response::builder()
                    .status(StatusCode::METHOD_NOT_ALLOWED)
                    .body(Body::empty())
                    .unwrap());
            }
        };

        // Process DNS query
        let response = dns_server.handle_query(&dns_message, client_ip).await;
        let response_bytes = response.to_vec()?;

        // Return DNS response
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/dns-message")
            .header("Cache-Control", format!("max-age={}", response.ttl()))
            .body(Body::from(response_bytes))
            .unwrap())
    }

    /// Parse GET request with dns= query parameter
    fn parse_get_query(req: &Request<Body>) -> anyhow::Result<Message> {
        let query = req.uri().query().unwrap_or("");
        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

        let dns_param = params.get("dns")
            .ok_or_else(|| anyhow::anyhow!("Missing dns parameter"))?;

        // Decode base64url
        let bytes = URL_SAFE_NO_PAD.decode(dns_param)?;

        Message::from_vec(&bytes)
    }

    fn extract_client_ip(req: &Request<Body>) -> IpAddr
}
```

### 3. Update `/node/src/dns/dns_config.rs`
Add DoH/DoT configuration:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    // ... existing fields ...

    /// DNS over TLS (DoT) - Port 853
    pub dot: DotConfig,

    /// DNS over HTTPS (DoH) - Port 443
    pub doh: DohConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotConfig {
    pub enabled: bool,
    pub addr: SocketAddr,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DohConfig {
    pub enabled: bool,
    pub addr: SocketAddr,
    pub cert_path: String,
    pub key_path: String,
    pub path: String,  // Usually "/dns-query"
}
```

### 4. Update `/node/src/main_dns.rs`
Start DoH/DoT servers:

```rust
// Start DoT server if enabled
if config.dot.enabled {
    let dot_server = DotServer::new(
        dns_server.clone(),
        &config.dot.cert_path,
        &config.dot.key_path,
        config.dot.addr,
    )?;
    tokio::spawn(async move {
        if let Err(e) = dot_server.run().await {
            tracing::error!("DoT server error: {}", e);
        }
    });
}

// Start DoH server if enabled
if config.doh.enabled {
    let doh_server = DohServer::new(
        dns_server.clone(),
        &config.doh.cert_path,
        &config.doh.key_path,
        config.doh.addr,
    )?;
    tokio::spawn(async move {
        if let Err(e) = doh_server.run().await {
            tracing::error!("DoH server error: {}", e);
        }
    });
}
```

### 5. Update Cargo.toml
Add dependencies:
```toml
tokio-rustls = "0.25"
hyper-rustls = "0.25"
rustls = "0.22"
rustls-pemfile = "2.0"
base64 = "0.21"
```

## Testing Requirements
- Unit tests for base64url encoding/decoding
- Unit tests for DoH GET parameter parsing
- Unit tests for DoH POST body parsing
- Integration test: DoT query with openssl s_client
- Integration test: DoH GET query with curl
- Integration test: DoH POST query with curl
- Test TLS certificate validation

## Success Criteria
- DoT server accepts TLS connections on port 853
- DoH server accepts HTTPS connections
- DoH supports both GET and POST methods
- Responses include proper Content-Type header
- TLS handshake works with standard clients
- All tests pass
```

---

## Sprint 30.6: DNS Dashboard & CLI

### Objective
Build management interfaces for DNS: React dashboard and CLI tool.

### New Files
```
/contracts/dao/packages/
├── dns-sdk/              # TypeScript SDK
│   ├── src/
│   │   ├── client.ts
│   │   └── types.ts
│   └── package.json
│
└── dns-dashboard/        # React app
    ├── src/
    │   ├── pages/
    │   │   ├── Zones.tsx
    │   │   ├── Records.tsx
    │   │   └── Analytics.tsx
    │   └── components/
    │       ├── ZoneCard.tsx
    │       ├── RecordTable.tsx
    │       └── DnsStats.tsx
    └── package.json

/cli/src/
└── dns_commands.rs       # CLI commands for DNS
```

### LLM Prompt for Sprint 30.6

```
You are implementing the DNS management dashboard and CLI for the AEGIS decentralized edge network.

## Context
We have a DNS API (Sprint 30.2) that needs user interfaces. Build a React dashboard and extend the aegis-cli with DNS commands.

## Technical Requirements

### 1. Create `/contracts/dao/packages/dns-sdk/`
TypeScript SDK for DNS API:

**src/types.ts:**
```typescript
export interface Zone {
  domain: string;
  proxied: boolean;
  nameservers: string[];
  dnssec_enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface DnsRecord {
  id: string;
  name: string;
  type: 'A' | 'AAAA' | 'CNAME' | 'MX' | 'TXT' | 'NS' | 'CAA' | 'SRV';
  value: string;
  ttl: number;
  priority?: number;
  proxied: boolean;
}

export interface CreateZoneRequest {
  domain: string;
  proxied?: boolean;
}

export interface CreateRecordRequest {
  name: string;
  type: DnsRecord['type'];
  value: string;
  ttl?: number;
  priority?: number;
  proxied?: boolean;
}

export interface DnsStats {
  total_queries: number;
  queries_today: number;
  cache_hit_rate: number;
  top_queried_domains: Array<{ domain: string; count: number }>;
  query_types: Record<string, number>;
}

export interface ApiResponse<T> {
  success: boolean;
  message?: string;
  data?: T;
}
```

**src/client.ts:**
```typescript
import type { Zone, DnsRecord, CreateZoneRequest, CreateRecordRequest, DnsStats, ApiResponse } from './types';

export class DnsClient {
  private baseUrl: string;

  constructor(baseUrl: string = 'http://localhost:8054') {
    this.baseUrl = baseUrl;
  }

  // Zone operations
  async listZones(): Promise<Zone[]>
  async getZone(domain: string): Promise<Zone>
  async createZone(req: CreateZoneRequest): Promise<Zone>
  async updateZone(domain: string, updates: Partial<Zone>): Promise<Zone>
  async deleteZone(domain: string): Promise<void>

  // Record operations
  async listRecords(domain: string): Promise<DnsRecord[]>
  async createRecord(domain: string, req: CreateRecordRequest): Promise<DnsRecord>
  async updateRecord(domain: string, recordId: string, updates: Partial<DnsRecord>): Promise<DnsRecord>
  async deleteRecord(domain: string, recordId: string): Promise<void>

  // DNSSEC
  async getDnssecStatus(domain: string): Promise<{ enabled: boolean; ds_record?: string }>
  async enableDnssec(domain: string): Promise<{ ds_record: string }>
  async disableDnssec(domain: string): Promise<void>

  // Stats
  async getStats(): Promise<DnsStats>
  async getZoneStats(domain: string): Promise<DnsStats>

  // Nameservers
  async getNameservers(): Promise<string[]>

  private async request<T>(method: string, path: string, body?: unknown): Promise<T>
}
```

### 2. Create `/contracts/dao/packages/dns-dashboard/`
React dashboard following AEGIS patterns:

**src/pages/Zones.tsx:**
```tsx
import { useState, useEffect } from 'react';
import { DnsClient } from '@aegis/dns-sdk';
import type { Zone } from '@aegis/dns-sdk';

export default function Zones() {
  const [zones, setZones] = useState<Zone[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);

  const client = new DnsClient();

  useEffect(() => {
    loadZones();
  }, []);

  const loadZones = async () => {
    setLoading(true);
    const data = await client.listZones();
    setZones(data);
    setLoading(false);
  };

  return (
    <div className="p-6">
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-2xl font-bold text-white">DNS Zones</h1>
        <button
          onClick={() => setShowCreate(true)}
          className="px-4 py-2 bg-aegis-teal text-white rounded-lg"
        >
          Add Zone
        </button>
      </div>

      {loading ? (
        <div>Loading...</div>
      ) : (
        <div className="grid gap-4">
          {zones.map(zone => (
            <ZoneCard key={zone.domain} zone={zone} onDelete={loadZones} />
          ))}
        </div>
      )}

      {showCreate && (
        <CreateZoneModal
          onClose={() => setShowCreate(false)}
          onCreate={loadZones}
        />
      )}
    </div>
  );
}
```

**src/pages/Records.tsx:**
```tsx
// Page for managing DNS records for a specific zone
// - Table with all records
// - Add/Edit/Delete functionality
// - Filter by record type
// - Bulk import from zone file
```

**src/pages/Analytics.tsx:**
```tsx
// DNS query analytics dashboard
// - Queries over time chart
// - Top queried domains
// - Query type distribution
// - Geographic distribution of queries
// - Cache hit rate
```

**src/components/ZoneCard.tsx:**
```tsx
interface ZoneCardProps {
  zone: Zone;
  onDelete: () => void;
}

export function ZoneCard({ zone, onDelete }: ZoneCardProps) {
  return (
    <div className="bg-darkGrey rounded-xl p-6 border border-gray-700">
      <div className="flex justify-between items-start">
        <div>
          <h3 className="text-xl font-bold text-white">{zone.domain}</h3>
          <div className="flex gap-2 mt-2">
            {zone.proxied && (
              <span className="px-2 py-1 bg-aegis-teal/20 text-aegis-teal rounded text-sm">
                Proxied
              </span>
            )}
            {zone.dnssec_enabled && (
              <span className="px-2 py-1 bg-green-500/20 text-green-400 rounded text-sm">
                DNSSEC
              </span>
            )}
          </div>
        </div>
        <div className="flex gap-2">
          <Link to={`/dns/zones/${zone.domain}/records`}>
            <button className="p-2 hover:bg-gray-700 rounded">
              <EditIcon />
            </button>
          </Link>
          <button onClick={handleDelete} className="p-2 hover:bg-red-500/20 rounded">
            <TrashIcon />
          </button>
        </div>
      </div>

      <div className="mt-4 text-sm text-gray-400">
        <p>Nameservers:</p>
        <ul className="mt-1">
          {zone.nameservers.map(ns => (
            <li key={ns} className="font-mono">{ns}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}
```

**src/components/RecordTable.tsx:**
```tsx
// Table component for displaying DNS records
// Columns: Name, Type, Value, TTL, Proxied, Actions
// Supports inline editing
// Color-coded by record type
```

### 3. Create `/cli/src/dns_commands.rs`
CLI commands for DNS management:

```rust
use clap::{Args, Subcommand};
use crate::dns_client::DnsApiClient;

#[derive(Debug, Subcommand)]
pub enum DnsCommands {
    /// List all DNS zones
    ListZones,

    /// Create a new DNS zone
    CreateZone {
        /// Domain name
        #[arg(short, long)]
        domain: String,

        /// Enable proxying through AEGIS
        #[arg(short, long, default_value = "true")]
        proxied: bool,
    },

    /// Delete a DNS zone
    DeleteZone {
        /// Domain name
        domain: String,
    },

    /// List records for a zone
    ListRecords {
        /// Domain name
        domain: String,
    },

    /// Add a DNS record
    AddRecord {
        /// Domain name
        #[arg(short, long)]
        domain: String,

        /// Record name (e.g., "www" or "@")
        #[arg(short, long)]
        name: String,

        /// Record type (A, AAAA, CNAME, MX, TXT)
        #[arg(short = 't', long)]
        record_type: String,

        /// Record value
        #[arg(short, long)]
        value: String,

        /// TTL in seconds
        #[arg(long, default_value = "300")]
        ttl: u32,
    },

    /// Delete a DNS record
    DeleteRecord {
        /// Domain name
        domain: String,

        /// Record ID
        record_id: String,
    },

    /// Show DNSSEC status and DS record
    DnssecStatus {
        /// Domain name
        domain: String,
    },

    /// Enable DNSSEC for a zone
    EnableDnssec {
        /// Domain name
        domain: String,
    },

    /// Import zone from file
    ImportZone {
        /// Path to zone file
        file: String,
    },

    /// Export zone to file
    ExportZone {
        /// Domain name
        domain: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
}

pub async fn handle_dns_command(cmd: DnsCommands) -> anyhow::Result<()> {
    let client = DnsApiClient::new("http://localhost:8054");

    match cmd {
        DnsCommands::ListZones => {
            let zones = client.list_zones().await?;
            println!("{:<30} {:<10} {:<10}", "DOMAIN", "PROXIED", "DNSSEC");
            println!("{}", "-".repeat(50));
            for zone in zones {
                println!(
                    "{:<30} {:<10} {:<10}",
                    zone.domain,
                    if zone.proxied { "Yes" } else { "No" },
                    if zone.dnssec_enabled { "Yes" } else { "No" }
                );
            }
        }
        DnsCommands::CreateZone { domain, proxied } => {
            let zone = client.create_zone(&domain, proxied).await?;
            println!("Zone created: {}", zone.domain);
            println!("\nNameservers (update at your registrar):");
            for ns in &zone.nameservers {
                println!("  {}", ns);
            }
        }
        // ... handle other commands
    }

    Ok(())
}
```

### 4. Update main CLI
Add DNS commands to aegis-cli:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands

    /// DNS management commands
    Dns {
        #[command(subcommand)]
        command: DnsCommands,
    },
}
```

## Testing Requirements
- Unit tests for SDK client methods
- Component tests for React components
- Integration test: Create zone via CLI
- Integration test: Add record via dashboard
- End-to-end: Full workflow through CLI

## Success Criteria
- SDK covers all API endpoints
- Dashboard displays zones and records
- Can create/edit/delete zones via dashboard
- CLI provides all DNS management commands
- Zone import/export works
- All tests pass
```

---

## Files Summary

### New Files (Backend)
```
/node/src/dns/
├── mod.rs
├── dns_server.rs
├── dns_config.rs
├── dns_types.rs
├── zone_store.rs
├── dns_api.rs
├── dns_persistence.rs
├── geo_resolver.rs
├── health_checker.rs
├── edge_registry.rs
├── dnssec.rs
├── dnssec_keys.rs
├── doh_server.rs
└── dot_server.rs

/node/src/main_dns.rs
```

### New Files (Frontend)
```
/contracts/dao/packages/dns-sdk/
/contracts/dao/packages/dns-dashboard/
```

### Modified Files
```
/node/src/lib.rs
/node/Cargo.toml
/cli/src/main.rs
```

---

## Dependencies

```toml
# DNS
hickory-server = { version = "0.25", features = ["dns-over-tls", "dnssec-ring"] }
hickory-proto = "0.25"
hickory-resolver = "0.25"

# TLS for DoT/DoH
tokio-rustls = "0.25"
rustls = "0.22"
rustls-pemfile = "2.0"

# GeoIP
maxminddb = "0.24"

# Crypto for DNSSEC
ring = "0.17"

# Base64 for DoH
base64 = "0.21"
```

---

## Estimated Scope

| Sprint | Lines of Code | Tests |
|--------|---------------|-------|
| 30.1 Core Server | ~1,500 Rust | ~25 |
| 30.2 Management API | ~1,000 Rust | ~20 |
| 30.3 Geo Resolution | ~800 Rust | ~15 |
| 30.4 DNSSEC | ~1,200 Rust | ~20 |
| 30.5 DoH/DoT | ~600 Rust | ~15 |
| 30.6 Dashboard/CLI | ~1,500 TS/Rust | ~20 |
| **Total** | **~6,600** | **~115** |
