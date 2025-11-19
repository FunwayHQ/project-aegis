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

**Application-Level (WAF)**:
- **Coraza WAF**: OWASP CRS-compatible, compiled to WebAssembly
- Runs in isolated Wasm sandbox within River proxy (via wasmtime)
- Protects against SQLi, XSS, and Layer 7 attacks
- Isolation prevents WAF bugs from crashing entire proxy

**Bot Management**:
- Custom Wasm modules for bot detection (user-agent analysis, rate limiting)
- Configurable policies (challenge, block, allow)

#### 4. Distributed State Management

**Local State**:
- DragonflyDB for high-speed caching at each edge node

**Global State Synchronization**:
- **CRDTs (Conflict-Free Replicated Data Types)**: Using Loro or Automerge libraries
- **NATS JetStream**: Message transport for CRDT operations between regions
- Active-Active replication model (eventual consistency)
- Leaf nodes can operate autonomously if core connection severed

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
2. **eBPF/XDP** (Cilium) drops malicious packets at kernel level
3. **River Proxy** terminates TLS using BoringSSL
4. **Coraza WAF** (Wasm) inspects for Layer 7 attacks
5. **DragonflyDB** cache lookup (hit = immediate response, miss = proxy to origin)
6. **NATS JetStream** broadcasts state updates (rate limits, etc.) via CRDTs
7. **FluxCD** ensures config matches Git, validated by Flagger canaries

## Development Phases

### Phase 1: Foundation & Core Node (Sprints 1-6)
- Solana smart contract development ($AEGIS token, node registry, staking)
- Rust node with Pingora/River proxy, TLS termination
- DragonflyDB integration for caching
- Basic node operator CLI for registration and rewards claiming

### Phase 2: Security & Distributed State (Sprints 7-12)
- eBPF/XDP DDoS protection (SYN flood mitigation)
- Coraza WAF integration via Wasm
- Bot management Wasm modules
- CRDTs + NATS JetStream for global state sync
- Verifiable analytics framework with cryptographic signing

### Phase 3: Edge Compute & Governance (Sprints 13-18)
- Wasm edge functions runtime (custom logic at edge)
- DAO governance smart contracts (proposals, voting, treasury)
- Advanced P2P performance routing
- IPFS/Filecoin integration for decentralized storage

### Phase 4: Optimization & Launch (Sprints 19-24)
- Performance tuning and stress testing
- Smart contract security audits
- Mainnet deployment preparation
- Tokenomics finalization

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
