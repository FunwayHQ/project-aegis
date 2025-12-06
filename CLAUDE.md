# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**AEGIS (Project Aegis DECENTRALIZED)** is a blockchain-powered global edge network designed as a decentralized alternative to centralized CDN and edge security providers like Cloudflare. The project combines cutting-edge infrastructure technologies with Web3 tokenomics to create a community-owned, censorship-resistant internet infrastructure platform.

### Core Vision
- Build a resilient, distributed edge network using memory-safe languages (Rust)
- Incentivize hardware contributors through $AEGIS utility token on Solana blockchain
- Achieve 99.999% uptime through massive geographic distribution
- Eliminate single points of failure through decentralized architecture

## High-Level Architecture

### Technology Stack Philosophy

The architecture is explicitly designed to avoid the failure modes that caused the November 2025 Cloudflare outage:

1. **Memory Safety**: Use Rust throughout to eliminate memory corruption bugs
2. **Static Stability**: Data plane must operate independently of control plane
3. **Graceful Degradation**: Fail open rather than fail closed; preserve Last Known Good state
4. **Fault Isolation**: Separate control plane from data plane completely

### Core Components

#### 1. Network Layer (BGP Anycast)
- **BIRD (v2)**: BGP routing daemon for global anycast network
- **Peering Manager**: Automates BGP session management and configuration generation
- **Routinator**: RPKI validation for route security (Rust-based)
- Users are routed to nearest edge node via anycast IP addressing

#### 2. Data Plane (Rust-based Proxy)
- **Pingora Framework**: Cloudflare's open-source Rust proxy library
- **River Proxy**: Reverse proxy application built on Pingora (replaces Nginx)
  - Multi-threaded architecture with work-stealing (vs Nginx process model)
  - Connection reuse across threads
  - Zero-downtime upgrades via graceful handoff
  - TLS 1.3 termination using BoringSSL
- **DragonflyDB**: Multi-threaded Redis replacement for local caching
  - 25x throughput of standard Redis
  - Dash-table indexing for memory efficiency

#### 3. Security Layer

**Kernel-Level (eBPF/XDP)**:
- **Cilium**: Orchestrates eBPF programs for DDoS mitigation
- XDP programs drop malicious packets at NIC driver level (nanoseconds latency)
- Handles volumetric attacks (SYN floods) before OS resource consumption
- Dynamic blocklist updated via P2P threat intelligence

**Application-Level (WAF)**:
- **Rust-Native WAF**: OWASP-compatible firewall (Sprint 8)
- Integrated into Pingora request filter
- Protects against SQLi, XSS, RCE, and Layer 7 attacks
- <100μs latency overhead per request
- Wasm migration planned for Sprint 13

**Bot Management**:
- **Wasm-based bot detector**: Isolated bot detection engine (Sprint 9)
- User-agent analysis and behavioral detection
- Configurable policies (allow, challenge, block, rate-limit)
- Runs in isolated Wasm sandbox for security

**Threat Intelligence (P2P)**:
- **libp2p-based network**: Decentralized threat intelligence sharing (Sprint 10)
- Automatic peer discovery (mDNS + Kademlia DHT)
- Real-time threat propagation via gossipsub
- Automatic eBPF blocklist updates on threat receipt
- <200ms from detection to network-wide protection

#### 4. Distributed State Management

**Local State**:
- DragonflyDB for high-speed caching at each edge node

**Global State Synchronization** (Sprint 11):
- **CRDTs (Conflict-Free Replicated Data Types)**: Using `crdts` crate (G-Counter for rate limiting)
- **NATS JetStream**: Message transport for CRDT operations between regions
- **Distributed Rate Limiter**: Multi-node rate limiting with <2s convergence
- Active-Active replication model (eventual consistency)
- Leaf nodes can operate autonomously if core connection severed
- 24 comprehensive tests covering CRDT properties and synchronization

#### 5. Control Plane & Orchestration

**Container Orchestration**:
- **K3s**: Lightweight Kubernetes for edge (single <100MB binary)
- Manages River, DragonflyDB, BIRD across global fleet

**GitOps & Safe Deployment**:
- **FluxCD**: Pull-based config sync from Git repository
- **Flagger**: Progressive delivery with canary deployments
  - New configs roll out to 1% of nodes first
  - Automatic rollback on error spike detection
  - Prevents global blackouts from bad configurations

**Certificate Management**:
- ACME protocol integration with Let's Encrypt
- Auto-renewal 30 days before expiration
- Certificates stored in DragonflyDB, replicated via NATS

#### 6. Blockchain Layer (Solana)

**Smart Contracts (Anchor Framework)**:
- $AEGIS token program (SPL token standard)
- Node Registry: On-chain registration of operator hardware
- Staking mechanism: Node operators stake $AEGIS as security bond
- Reward Distribution: Based on verified uptime and performance
- DAO Governance: Token-weighted voting on proposals

**Verification**:
- Oracles bring off-chain performance metrics on-chain
- Proof-of-Contribution/Uptime consensus
- Cryptographically signed performance reports from nodes

### Request Lifecycle

1. **BGP Anycast** routes user to nearest edge node
2. **eBPF/XDP** drops malicious/blocklisted packets at kernel level (uses P2P threat intel)
3. **River Proxy** terminates TLS using BoringSSL
4. **Sprint 16: Route-based Dispatch** matches request to configured route, executes Wasm module pipeline
5. **WAF + Bot Management** inspects for Layer 7 attacks (SQLi, XSS, bots) - via routes or legacy
6. **DragonflyDB** cache lookup (hit = immediate response, miss = proxy to origin)
7. **P2P Threat Intelligence** shares detected threats with network (libp2p gossipsub)
8. **NATS JetStream** broadcasts state updates (rate limits, cache invalidation) via CRDTs
9. **FluxCD** ensures config matches Git, validated by Flagger canaries

### Sprint 16: Route-based Dispatch Architecture

**Configuration-Driven Routing for Wasm Modules**

Sprint 16 introduces a flexible routing system that maps HTTP request patterns to sequences of Wasm modules, enabling GitOps-managed edge logic without code changes.

**Core Components:**

1. **RouteConfig** (`route_config.rs`):
   - RoutePattern: Exact (`/api/users`), Prefix (`/api/*`), or Regex (`^/api/v[0-9]+/.*`) matching
   - MethodMatcher: Single method, multiple methods, or wildcard (`*`)
   - Priority-based route selection (higher priority = checked first)
   - Header matching for fine-grained control
   - YAML/TOML configuration support

2. **ModuleDispatcher** (`module_dispatcher.rs`):
   - Sequential execution of Wasm module pipelines
   - Early termination when WAF blocks request (403 Forbidden)
   - Error handling: fail fast or continue on error (per route settings)
   - Resource governance: max_modules_per_request safety limit (default: 10)
   - Execution time tracking (microsecond precision) for profiling

3. **Pingora Integration** (`pingora_proxy.rs`):
   - Route matching in request_filter phase (before cache lookup)
   - Falls back to legacy WAF if no route matches (backward compatibility)
   - Fail-open behavior: pipeline errors don't crash proxy

**Request Flow with Routes:**

```
HTTP Request → Route Matching → Pipeline Execution → Response
                     ↓                    ↓
              Find route by:      Execute modules:
              - Path pattern      1. WAF (security)
              - HTTP method       2. Auth (validation)
              - Headers           3. Rate limit
                                 4. Transform
                                 5. Custom logic
                     ↓                    ↓
              If no match:        If blocked:
              → Legacy WAF        → Return 403
              → Continue          → Log & skip upstream
```

**Example Route Configuration (YAML):**

```yaml
settings:
  max_modules_per_request: 10
  continue_on_error: false

routes:
  - name: api_endpoints
    priority: 100
    path:
      type: prefix
      pattern: "/api/*"
    methods: ["GET", "POST", "PUT", "DELETE"]
    wasm_modules:
      - type: waf
        module_id: api-waf
        ipfs_cid: QmWafCID
      - type: edge_function
        module_id: api-auth
      - type: edge_function
        module_id: api-rate-limiter
```

**Key Advantages:**

- **GitOps-Friendly**: Routes stored in YAML, version controlled, FluxCD synced
- **Zero-Downtime**: Hot-reload capability (future sprint)
- **Progressive Deployment**: Flagger canary testing for route changes
- **Fail-Safe**: Pipeline errors don't crash proxy (fail open)
- **Observable**: Per-module execution times logged
- **Flexible**: Combine multiple modules (WAF → auth → rate limit → transform)

**Module Pipeline Example:**

```
Request: POST /api/v1/users
         ↓
Route Match: "api_endpoints" (priority: 100)
         ↓
Execute Pipeline:
  1. WAF Module (1.2ms) ✅ PASS
  2. Auth Module (0.8ms) ✅ PASS
  3. Rate Limiter (0.5ms) ✅ PASS
         ↓
Total: 2.5ms → Continue to cache/upstream
```

**Module Pipeline with Block:**

```
Request: POST /api/v1/users?id=1' OR '1'='1
         ↓
Route Match: "api_endpoints"
         ↓
Execute Pipeline:
  1. WAF Module (1.5ms) ❌ BLOCKED (SQL injection detected)
         ↓
Return 403 Forbidden → Skip remaining modules → Skip upstream
```

### Sprint 17: IPFS/Filecoin Integration for Decentralized Module Distribution

**Censorship-Resistant Content Addressing for Wasm Modules**

Sprint 17 enables Wasm modules to be distributed via IPFS (InterPlanetary File System), eliminating single points of failure and enabling censorship-resistant edge logic deployment.

**Core Components:**

1. **IpfsClient** (`ipfs_client.rs`):
   - Upload modules to IPFS and get Content ID (CID)
   - Download modules by CID with integrity verification
   - Pin/unpin modules to control garbage collection
   - Local disk caching (~/.aegis/modules/) for performance
   - Multi-tier CDN strategy (local cache → IPFS node → public gateways)

2. **Public IPFS Gateway Fallback** (CDN functionality):
   - Primary: Local IPFS daemon (http://127.0.0.1:5001)
   - Fallback 1: Cloudflare IPFS (https://cloudflare-ipfs.com)
   - Fallback 2: ipfs.io (https://ipfs.io)
   - Fallback 3: dweb.link (https://dweb.link)
   - Automatic failover ensures high availability

3. **WasmRuntime Integration**:
   - `load_module_from_ipfs(cid, module_type, ipfs_client)`
   - Combines IPFS fetching with Ed25519 signature verification
   - Seamless integration with existing module loading

4. **Route Config Support**:
   - Routes reference modules by IPFS CID instead of file paths
   - GitOps-friendly: CIDs in YAML config files
   - Version control via content addressing

**Example Usage:**

```yaml
# Route configuration with IPFS CIDs
routes:
  - name: api_waf
    wasm_modules:
      - type: waf
        module_id: waf-v1
        ipfs_cid: QmWafModuleCID123abc
        required_public_key: ed25519_pubkey_hex
```

**Module Distribution Flow:**

```
Developer → Build Wasm → Upload to IPFS → Get CID (QmXxx...)
                              ↓
                    Update route config with CID
                              ↓
Edge Nodes → Load by CID → Multi-tier fetch:
                              1. Check local cache (~/.aegis/modules/)
                              2. Try local IPFS node
                              3. Fallback to public gateways (CDN)
                              ↓
                    Verify CID integrity + Ed25519 signature
                              ↓
                    Cache locally → Load into WasmRuntime → Execute
```

**Key Advantages:**

- **Censorship Resistance**: No central server can block module distribution
- **Content Verification**: CID guarantees integrity (hash of content)
- **High Availability**: Multi-tier CDN strategy with public gateway fallback
- **Bandwidth Efficiency**: Local caching reduces IPFS fetches
- **Decentralization**: Aligns with project's core mission
- **Version Control**: CIDs provide immutable versioning

**Security Features:**

1. **Size Validation**: Max 10MB per module
2. **CID Verification**: Downloaded content must match CID
3. **Signature Verification**: Ed25519 signatures (from Sprint 15)
4. **HTTPS-Only Gateways**: All public gateway requests use HTTPS
5. **Timeout Protection**: 30s max download time

**Resource Management:**

- Local cache directory: `~/.aegis/modules/<cid>.wasm`
- Cache statistics API: `cache_stats()` returns count and size
- Cache clearing: `clear_cache()` for maintenance
- LRU eviction (planned): Auto-remove old modules when cache > 1GB

**Testing:**

- 11 comprehensive tests covering all functionality
- 7 tests pass without external dependencies
- 4 tests require IPFS daemon (marked `#[ignore]`)
- End-to-end workflow test validates complete integration

### Content Publisher CLI (`aegis-cdn`)

**GitOps-Friendly Website Deployment Tool**

The `aegis-cdn` CLI enables developers and content creators to publish static websites and web applications to the AEGIS decentralized CDN with zero blockchain transaction costs for regular content.

**Commands:**

1. **`init <project-name>`** - Initialize new CDN project
   - Creates project directory structure
   - Generates `aegis-cdn.yaml` configuration
   - Creates sample `index.html` with AEGIS branding
   - Outputs README with quick start guide

2. **`upload <source>`** - Upload content to IPFS
   - Supports single files or directories
   - Automatic pinning to prevent garbage collection
   - Returns IPFS CID and public gateway URLs
   - Saves deployment record locally

3. **`deploy <source>`** - Full deployment with routing
   - Uploads content to IPFS
   - Generates route configuration (YAML)
   - Applies WAF and bot management rules
   - Creates GitOps-ready config for FluxCD sync

4. **`status <project>`** - Check deployment metrics
   - Shows IPFS CID and deployment timestamp
   - Displays edge node distribution (~150 nodes)
   - Cache hit ratio and latency metrics
   - WAF blocks and bot challenge counts

5. **`config show/set <key> <value>`** - Manage configuration
   - View current project settings
   - Update IPFS, routing, or cache settings
   - Generate default configuration templates

6. **`list [--active]`** - List all deployments
   - Shows project names and CIDs
   - Deployment timestamps
   - Filter by active status

7. **`remove <project> [--force]`** - Remove deployment
   - Deletes deployment record
   - Content remains in IPFS (not deleted)
   - Requires --force flag for safety

**Project Structure:**

```
my-website/
├── aegis-cdn.yaml       # Project configuration
├── public/              # Content directory
│   ├── index.html
│   ├── style.css
│   └── app.js
├── README.md
└── .aegis/
    └── deployments/     # Local deployment records
        └── my-website.json
```

**Configuration Format (`aegis-cdn.yaml`):**

```yaml
name: my-website
description: AEGIS CDN Project
source_dir: ./public

ipfs:
  api_endpoint: http://127.0.0.1:5001
  pin: true
  use_filecoin: false

routing:
  enable_waf: true
  enable_bot_management: true
  custom_routes: []

cache:
  ttl: 3600
  cache_control: public, max-age=3600
```

**Deployment Workflow:**

```
Developer → aegis-cdn init my-site
         → Edit public/index.html
         → aegis-cdn deploy public/
              ↓
         Upload to IPFS → Get CID (QmXxx...)
              ↓
         Generate routes-production.yaml
              ↓
         Commit to Git → FluxCD pulls config
              ↓
         Edge nodes fetch via IPFS → Serve traffic
```

**Generated Route Configuration Example:**

```yaml
routes:
  - name: my-website_waf
    priority: 100
    enabled: true
    path:
      type: prefix
      pattern: "/*"
    methods: "*"
    wasm_modules:
      - type: waf
        module_id: default-waf

  - name: my-website_bot_mgmt
    priority: 90
    enabled: true
    path:
      type: prefix
      pattern: "/*"
    methods: "*"
    wasm_modules:
      - type: edge_function
        module_id: bot-detector
```

**Cost Model:**

- ✅ **FREE** for static content (HTML, CSS, JS, images)
- No $AEGIS tokens required for publishing
- IPFS uploads use local daemon (no transaction fees)
- Route configs deployed via Git (no blockchain interaction)
- Optional: Filecoin pinning for guaranteed long-term storage

**Example Usage:**

```bash
# Initialize new project
aegis-cdn init my-website
cd my-website

# Edit your content
vim public/index.html

# Deploy to decentralized CDN
aegis-cdn deploy public/

# Output:
# 📦 IPFS CID: QmXxx...
# 🌐 Public Gateway URLs:
#    • https://ipfs.io/ipfs/QmXxx...
#    • https://cloudflare-ipfs.com/ipfs/QmXxx...
# 📝 Route config saved to: routes-production.yaml
# ✅ Deployment complete!

# Check deployment status
aegis-cdn status my-website

# List all deployments
aegis-cdn list
```

**Integration with AEGIS Edge Network:**

1. Developer deploys via `aegis-cdn deploy`
2. Content uploaded to IPFS (CID: QmXxx...)
3. Route config committed to Git repository
4. FluxCD syncs config to all edge nodes
5. Edge nodes fetch content from IPFS (with CDN fallback)
6. Content cached locally on each node
7. Requests served with WAF protection and bot management

**Key Advantages:**

- **Zero Token Cost**: No $AEGIS required for static content
- **Decentralized**: Content distributed via IPFS
- **GitOps-Ready**: Configuration in Git, auto-synced
- **Security Built-in**: WAF and bot management by default
- **High Availability**: Multi-tier CDN strategy
- **Simple Workflow**: 3 commands to go live (init, edit, deploy)

## Development Phases

### Phase 1: Foundation & Core Node (Sprints 1-6) ✅ COMPLETE
- ✅ Solana smart contract development ($AEGIS token, node registry, staking, rewards)
- ✅ Rust node with Pingora/River proxy, TLS termination
- ✅ DragonflyDB integration for caching
- ✅ Node operator CLI for registration and rewards claiming (10 commands)
- ✅ 344 tests passing, all 4 contracts deployed to Devnet

### Phase 2: Security & Distributed State (Sprints 7-12) - ✅ COMPLETE (100%)
- ✅ **Sprint 7:** eBPF/XDP DDoS protection (SYN flood mitigation) - 48 tests
- ✅ **Sprint 8:** WAF integration (Rust-native, OWASP rules) - 7 tests + 17 integration
- ✅ **Sprint 9:** Bot management (Wasm-based detection) - 6 tests
- ✅ **Sprint 10:** P2P Threat Intelligence (libp2p, real-time sharing) - 30 tests
- ✅ **Sprint 11:** CRDTs + NATS JetStream (G-Counter, distributed rate limiter) - 24 tests
- ✅ **Sprint 12:** Verifiable Analytics (Ed25519 signatures, SQLite, HTTP API) - 17 tests

### Phase 3: Edge Compute & Governance (Sprints 13-18) - ✅ COMPLETE (100%)
- ✅ **Sprint 13:** Wasm Edge Functions Runtime (custom logic at edge, host API for cache/HTTP)
- ✅ **Sprint 14:** Extended Host API (DragonflyDB cache ops, controlled HTTP requests)
- ✅ **Sprint 15:** WAF Migration to Wasm + Ed25519 Module Signatures
- ✅ **Sprint 15.5:** Architectural Cleanup (PN-Counter migration, HTTPS-only enforcement)
- ✅ **Sprint 16:** Route-based Dispatch (YAML/TOML config, module pipelines) - 156 tests
- ✅ **Sprint 17:** IPFS/Filecoin Integration (CDN fallback, local caching) - 11 tests
- ✅ **Sprint 18:** DAO Governance Smart Contracts (Security Hardened) - 14 tests
  - Snapshot-based voting (flash loan protection)
  - 48-hour timelock for config changes
  - Token account ownership/mint validation
  - Recipient validation in treasury execution
  - 13 instructions including `register_vote_snapshot`, `queue_config_update`, `cancel_proposal`
- ✅ **Sprint 18.5:** Critical Security Hardening (3 High-Severity Fixes)
  - **DAO Vote Escrow Pattern**: Replaced vulnerable snapshot voting with token locking
    - `deposit_vote_tokens`: Transfers tokens to PDA-owned vault (prevents double voting)
    - `cast_vote`: Uses escrowed token amount as vote weight (prevents flash loans)
    - `retract_vote`: Allows vote removal and unlocks tokens
    - `withdraw_vote_tokens`: Returns tokens after vote_end or if not voted
  - **Rewards Access Control**: Added `has_one = authority` constraint to `RecordPerformance`
  - **Staking-Registry CPI**: Implemented cross-program invocation to sync stake amounts
    - `stake()`, `execute_unstake()`, `slash_stake()` now call `registry::update_stake()`
    - Added `staking_authority` PDA for signing CPI calls
    - Added `registry_program_id` to `GlobalConfig`
- ✅ **Sprint 19:** DAO SDK, CLI, and dApp - 118 tests
  - **@aegis/dao-sdk**: TypeScript SDK with DaoClient (16 instructions), PDA helpers, types
  - **@aegis/dao-cli**: Commander.js CLI for config, proposals, voting, treasury, admin
  - **@aegis/dao-app**: React + Vite + Tailwind dApp with Solana wallet adapters
  - pnpm monorepo with Turborepo orchestration
  - Devnet verified: fetches config, lists proposals, displays voting data

### Phase 4: Advanced Security & Mainnet (Sprints 19-30)

**Cloudflare Parity Features (Sprints 19-24):**
- ✅ **Sprint 19:** TLS Fingerprinting (JA3/JA4) - Advanced bot detection via ClientHello analysis - 31 tests
  - JA3/JA4 fingerprint computation from ClientHello
  - Fingerprint database with built-in browser/tool signatures
  - Enhanced bot detection with composite scoring (UA + TLS)
  - Mismatch detection (Browser UA with curl fingerprint)
  - DragonflyDB cache integration for fingerprint storage
- ✅ **Sprint 20:** JavaScript Challenge System - Turnstile-like invisible/interactive challenges - 14 tests
  - ChallengeManager with PoW (SHA-256 leading zeros) + browser fingerprinting
  - Three challenge types: Invisible, Managed, Interactive
  - Ed25519 signed challenge tokens (JWT-like) with 15-minute TTL
  - Browser fingerprint collection (canvas, WebGL, audio, screen, timezone)
  - Bot pattern detection (Headless Chrome, PhantomJS, Selenium, Puppeteer)
  - HTTP API for challenge issuance and verification
  - Pingora proxy integration with BotAction::Challenge support
- ✅ **Sprint 21:** Behavioral Analysis & Trust Scoring - 9 tests
  - Mouse movement analysis (velocity, acceleration, entropy, direction changes)
  - Keystroke dynamics (inter-key timing, hold duration, typing speed)
  - Scroll behavior analysis (speed, reversals, depth)
  - Touch event analysis for mobile (pressure, radius)
  - BehavioralFeatures extraction with 25+ metrics
  - BehavioralAnalyzer with human/bot classification
  - TrustScoreCalculator: composite score (TLS 20pts + Challenge 30pts + Behavior 50pts)
  - JavaScript collection library generation for client-side tracking
  - Trust actions: Allow (60+), Challenge (30-60), Block (<30)
- ✅ **Sprint 22:** Enhanced WAF with OWASP CRS & ML Anomaly Scoring - 17 tests
  - ModSecurity SecRule parser (subset syntax: @rx, @eq, @contains, @detectSQLi, @detectXSS, etc.)
  - OWASP CRS 4.0 base rules (15+ rules covering SQLi, XSS, path traversal, RCE, PHP, Java)
  - Custom rule engine with YAML/JSON configuration
  - Rule priority, chaining (chain action), and skip logic
  - ML anomaly scoring: entropy, keyword density, z-score normalization, sigmoid scaling
  - Request baseline tracking: body size, parameter count, header count
  - Transform support: lowercase, urlDecode, htmlEntityDecode, compressWhitespace, etc.
  - EnhancedWafResult with rule matches, anomaly score, and recommendations
- ✅ **Sprint 23:** API Security Suite - 14 tests
  - API endpoint discovery: automatic learning from traffic, path normalization, shadow API detection
  - OpenAPI 3.0 schema validation: path/query params, headers, JSON body validation
  - JWT/OAuth validation: HS256/384/512, EdDSA support, claims validation (exp, nbf, iss, aud)
  - Sequence detection: credential stuffing, account enumeration, API scraping
  - Per-endpoint rate limiting with adaptive thresholds based on traffic patterns
  - Combined ApiSecurityEngine with configurable security checks
- ✅ **Sprint 24:** Distributed Enforcement & Global Blocklist Sync - 17 tests
  - IPv6 support for threat intelligence: ThreatIpAddress enum (V4/V6), parsing, byte conversion
  - EnhancedThreatIntel: typed ThreatType enum, Ed25519 signatures, validation, expiration
  - GlobalBlocklist: separate IPv4/IPv6 maps, automatic expiration, eBPF callback interface
  - TrustToken: signed tokens with TTL, distributed trust score sharing
  - TrustScoreCache: store/retrieve tokens, skip-challenge threshold logic
  - CoordinatedChallengeManager: track in-progress challenges, prevent re-challenges
  - EbpfBlocklistUpdater trait: interface for real-time eBPF map updates
  - EnforcementMessage: P2P message types (ThreatIntel, TrustToken, ChallengeComplete, BlocklistSync)
  - DistributedEnforcementEngine: unified API combining all components

**Mainnet Preparation (Sprints 25-30):**
- ✅ **Sprint 25:** Performance Benchmarking & Optimization
- ✅ **Sprint 26:** Performance Stress Testing & Profiling
- ✅ **Sprint 27:** Game Day Distributed Stress Testing
- ✅ **Sprint 28:** Infrastructure Security Audit - 284 tests
  - Static analysis (cargo audit, clippy) - zero vulnerabilities
  - Unsafe code audit - minimal, justified eBPF usage only
  - Input validation review across all components
  - Wasm sandbox, eBPF, P2P networking security review
  - Authentication/authorization (JWT, Challenge) review
  - Penetration testing plan created
  - OWASP Top 10 compliance checklist
- ✅ **Sprint 29:** Security Hardening (High-Priority Fixes) - 31 new tests
  - **P2P Threat Signatures:** Ed25519 signatures on `SignedThreatIntelligence`
  - **TrustedNodeRegistry:** Manage trusted node public keys for P2P
  - **Verified Blocklist:** `add_verified()` requires signature verification
  - **IPv6 Blocklist (eBPF):** `BLOCKLIST_V6` map with auto-blacklisting
  - **External Audit Prep:** Created `SOLANA-AUDIT-REQUEST.md` for auditors
- 🔲 **Sprint 30:** Mainnet Launch (TGE, 100+ nodes, geographic expansion)

**Security Remediation (Sprints Y1-Y10):**

Based on comprehensive security audit findings (85 total: 9 Critical, 16 High, 31 Medium, 21 Low, 8 Informational), implementing targeted fixes:

- ✅ **Sprint Y1:** NATS & Distributed State Authentication - 23 tests
  - Ed25519 signatures on CrdtMessage (sign, verify, is_trusted methods)
  - NatsAuth enum: None, UserPassword, Token, NKey authentication
  - TLS enforcement for production NATS connections
  - Trusted node registry for signature verification
  - Replay protection with message timestamps

- ✅ **Sprint Y2:** Solana Contract Hardening - All 3 contracts compile
  - **Rewards Contract:**
    - Epoch validation in record_performance (prevents stale data)
    - Nonce parameter for replay protection
    - NonceTracker account with sliding window (100 nonces max)
  - **Staking Contract:**
    - MIN_COOLDOWN_PERIOD constant (1 day minimum)
    - Slash nonce in GlobalConfig (deterministic PDA seeds)
    - PDA seeds use nonce instead of timestamp
  - **Registry Contract:**
    - Config-based min_stake_for_registration validation

- 🔲 **Sprint Y3:** Input Validation & Bounds Checking
- 🔲 **Sprint Y4:** WAF & Bot Management Hardening
- 🔲 **Sprint Y5:** P2P Network Security
- 🔲 **Sprint Y6:** Wasm Runtime Isolation
- 🔲 **Sprint Y7:** API Security & Rate Limiting
- 🔲 **Sprint Y8:** Cryptographic Operations Audit
- 🔲 **Sprint Y9:** Logging, Monitoring & Incident Response
- 🔲 **Sprint Y10:** Final Security Review & Documentation

## Key Architectural Principles

### 1. Static Stability
- Edge nodes must boot and serve traffic with zero control plane connectivity
- Last Known Good configuration retained on parse errors
- Never fail closed if configuration is corrupt (fail open to preserve availability)

### 2. Memory Safety
- All core infrastructure in Rust (River, Routinator)
- Eliminates CVEs from buffer overflows, use-after-free, null pointer dereferences
- Wasm sandboxing for third-party code (WAF, bot management, edge functions)

### 3. Fault Isolation
- Control plane (dashboard, API) on independent infrastructure from data plane
- Wasm modules isolated from proxy crashes
- Each layer can fail independently without cascading

### 4. Decentralization
- No single point of control or failure
- Blockchain-based service registry and discovery
- Token incentives for distributed hardware contribution
- Community governance via DAO

### 5. Progressive Deployment
- Flagger canary deployments (1% traffic first)
- Hard limits on configuration file sizes
- Input validation at every boundary
- Automated rollback on error rate increases

## Technology Choices Rationale

| Component | Technology | Why Not Alternatives? |
|-----------|-----------|---------------------|
| Proxy | Pingora/River (Rust) | Nginx: C-based (memory unsafe), process model doesn't share connections, blocking operations stall workers |
| Cache | DragonflyDB | Redis: Single-threaded bottleneck on multi-core, lower memory efficiency |
| Routing | BIRD v2 | Quagga/FRR: Less flexible filter language, BIRD standard for IXPs |
| RPKI | Routinator | C-based validators: Memory safety concerns |
| WAF | Coraza + Wasm | ModSecurity: Slow, difficult integration, not memory safe |
| Orchestration | K3s | Full K8s: Too heavy for edge nodes (resource overhead) |
| State Sync | CRDTs + NATS | Strong consistency (Raft/Paxos): High latency for global distribution |
| Blockchain | Solana | Ethereum: Higher transaction costs, lower throughput for rewards |

## Critical Security Considerations

1. **Smart Contract Auditing**: All Solana programs must undergo multiple independent audits before mainnet
2. **Slashing Conditions**: Node operators can lose staked $AEGIS for malicious behavior or extended downtime
3. **Sybil Resistance**: Staking requirements prevent low-cost node spam
4. **Configuration Validation**: Hard limits on file sizes, schema validation, canary testing
5. **Wasm Sandboxing**: CPU cycle limits and memory caps for edge functions to prevent DoS

## Edge Compute & Extensibility

### Wasm Edge Functions
- Developers can deploy custom logic to edge nodes
- Host API provides access to:
  - HTTP request/response manipulation
  - DragonflyDB cache operations
  - Controlled outbound HTTP requests
- Resource governance (CPU, memory limits) prevents abuse
- Modules referenced by IPFS CID on Solana smart contract
- Developer CLI for build, IPFS upload, and Solana registration

### Content Addressing
- Deep IPFS/Filecoin integration for censorship resistance
- Content served by CID, not just URLs
- Critical content (Wasm modules, DAO proposals) pinned to Filecoin

## Testing & Validation

### Performance Targets
- Latency: <60ms TTFB for cached assets, <200ms for proxied requests
- Throughput: >20 Gbps and >2M req/sec per node
- Cache Hit Ratio: >85%
- Data Plane Uptime: 99.999% (five nines)

### "Game Day" Exercises
- Simulate DDoS attacks to verify eBPF/XDP drops without CPU spikes
- Test DAO governance flows (proposal, voting, treasury withdrawal)
- Verify failover when control plane is completely offline
- Canary rollout with intentionally bad config to confirm Flagger auto-rollback

## Common Development Workflows

### Working with Rust Components
- River proxy configurations in TOML/YAML
- Build with `cargo build --release` for production binaries
- Integrate new Pingora middleware as Rust modules

### Solana Smart Contract Development
- Use Anchor framework for all programs
- Deploy to Devnet first: `anchor deploy --provider.cluster devnet`
- Generate IDL for client integration: `anchor build`
- Test with `anchor test`

### Node Operator Onboarding
- CLI tool: `aegis-cli register --metadata-url <IPFS_CID>`
- Stake tokens: `aegis-cli stake --amount <AMOUNT>`
- Monitor status: `aegis-cli status`
- Claim rewards: `aegis-cli claim-rewards`

### Configuration Management
- All configs stored in Git repository
- FluxCD pulls and applies automatically
- Flagger monitors error rates during rollout
- Manual override only in emergencies (violates GitOps)

## Infrastructure Automation

### Network (BGP)
- Peering Manager generates BIRD configs from templates (Jinja2)
- RPKI validation via Routinator RTR protocol to BIRD
- Route withdrawal automated on local health check failure

### Kubernetes (K3s)
- Single binary deployment to edge nodes
- Stripped cloud provider integrations (bare metal focus)
- Manages River, DragonflyDB, BIRD, monitoring agents

### Observability
- Metrics agent collects KPIs: latency, throughput, cache hit rate, WAF blocks
- Cryptographically signed metric reports (node operator private key)
- Local HTTP API `/verifiable-metrics` for oracle consumption
- On-chain submission via oracles for reward calculations

## Repository Structure (Expected)

```
/contracts/          # Solana Anchor programs
  /token/           # $AEGIS SPL token
  /registry/        # Node registration
  /staking/         # Staking mechanism
  /rewards/         # Reward distribution
  /dao/             # Governance

/node/              # Rust edge node software
  /river-config/   # River proxy configurations
  /ebpf/           # eBPF/XDP programs
  /wasm/           # WAF and bot management Wasm modules

/cli/              # Node operator CLI tool (Rust)

/ops/              # Infrastructure as Code
  /k3s/           # Kubernetes manifests
  /flux/          # FluxCD configurations
  /peering/       # Peering Manager configs

/docs/             # Architecture documentation
```

## Dependency Management

### Rust Dependencies
- `pingora`: Core proxy framework
- `wasmtime`: Wasm runtime for WAF/functions
- `tokio`: Async runtime
- `boringsssl-sys`: TLS library bindings
- Specific CRDT library: `loro` or `automerge`

### Blockchain Dependencies
- `@coral-xyz/anchor`: Solana Anchor framework
- `@solana/web3.js`: Solana client library
- `@solana/spl-token`: Token program interaction

### Infrastructure Dependencies
- BIRD v2 routing daemon
- Cilium for eBPF orchestration
- NATS server with JetStream enabled
- DragonflyDB binary

## Non-Functional Requirements

### Scalability
- Horizontal scaling to millions of nodes
- Solana Layer 2 or custom chain for high transaction volume

### Reliability
- RTO <30 seconds for node failure, <5 minutes for regional outage
- RPO near-zero for blockchain state, eventual consistency for edge cache

### Decentralization Metrics
- Geographic distribution of nodes (Nakamoto coefficient)
- Token distribution (avoid whale concentration)
- Governance participation rate

## Future Roadmap (Post-MVP)

- Serverless edge functions platform (expanded Wasm capabilities)
- Object storage layer (R2-like)
- Serverless database (D1-like, eventually consistent SQL at edge)
- Cross-chain interoperability for payments
- Decentralized Identity (DID) for node authentication
- AI/ML inference at edge using distributed compute
