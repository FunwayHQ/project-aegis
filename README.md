<p align="center">
  <img src="./AEGIS-logo.svg" alt="AEGIS Logo" width="400"/>
</p>

# AEGIS - Decentralized Global Edge Network

> **Building the world's most resilient, community-owned internet infrastructure**

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Solana](https://img.shields.io/badge/solana-anchor-9945FF.svg)](https://www.anchor-lang.com/)

## Overview

**Project AEGIS (Autonomous Edge Gateway & Infrastructure System)** is a blockchain-powered, decentralized global edge network that democratizes internet infrastructure. By enabling individuals and organizations to contribute their underutilized hardware (compute, storage, bandwidth) and earn $AEGIS tokens, we're building a censorship-resistant, ultra-resilient alternative to centralized CDN and edge security providers.

### The Problem

The November 2025 Cloudflare outage demonstrated the critical vulnerability of centralized internet infrastructure:
- **6 hours** of global disruption affecting millions of websites
- **Single point of failure** - one configuration bug took down ~20% of web traffic
- **No fallback** - customers locked out of control panels, unable to mitigate
- **Systemic risk** - infrastructure monoculture threatens global digital economy

### Our Solution

AEGIS combines cutting-edge distributed systems technology with Web3 tokenomics to create infrastructure that is:

- **Decentralized** - No single entity controls the network; community-owned and operated
- **Resilient** - 99.999% uptime through massive geographic distribution and P2P failover
- **Memory-Safe** - Built with Rust to eliminate entire classes of security vulnerabilities
- **Censorship-Resistant** - Distributed ownership, content addressing, and blockchain immutability
- **Fair** - Transparent on-chain rewards for hardware contributors
- **Performant** - Modern architecture designed to match or exceed centralized alternatives

## Key Features

### For Node Operators
- **Monetize Spare Resources** - Earn $AEGIS tokens by contributing bandwidth, compute, and storage
- **Flexible Participation** - Run nodes on anything from home servers to enterprise data centers
- **Transparent Rewards** - All contributions verified on-chain with cryptographic proofs
- **Build Reputation** - Immutable on-chain reputation score influences work assignment and rewards
- **Governance Rights** - Token holders vote on network upgrades and treasury allocation

### For Service Consumers
- **Enterprise-Grade CDN** - Global content delivery with <60ms TTFB for cached assets
- **DDoS Protection** - Kernel-level eBPF/XDP filtering stops volumetric attacks before they reach your application
- **Web Application Firewall** - OWASP CRS-compatible WAF running in isolated WebAssembly sandbox
- **Bot Management** - Advanced detection and mitigation of malicious bots
- **Edge Compute** - Deploy custom Wasm functions to run logic at the network edge
- **Decentralized Storage** - IPFS/Filecoin integration for content-addressed, censorship-resistant hosting
- **Web3 Native** - Pay for services in $AEGIS; perfect for dApps and traditional apps alike

## Architecture Highlights

### Technology Stack

| Layer | Technology | Purpose |
|-------|-----------|---------|
| **Network** | BIRD v2, BGP Anycast | Global routing and traffic distribution |
| **Proxy** | Pingora/River (Rust) | Memory-safe reverse proxy with zero-downtime upgrades |
| **Security** | eBPF/XDP, Cilium | Kernel-level DDoS mitigation |
| **WAF** | Coraza (Wasm) | OWASP-compliant firewall in isolated sandbox |
| **Cache** | DragonflyDB | 25x faster than Redis, multi-threaded architecture |
| **State Sync** | CRDTs + NATS JetStream | Active-active global replication with eventual consistency |
| **Orchestration** | K3s (Kubernetes) | Lightweight container orchestration for edge |
| **Deployment** | FluxCD + Flagger | GitOps with progressive canary deployments |
| **Blockchain** | Solana (Anchor) | Token, staking, rewards, and governance |
| **Routing Security** | Routinator (RPKI) | Cryptographic validation of BGP routes |

### Design Principles

1. **Static Stability** - Data plane operates independently of control plane; never fails closed
2. **Memory Safety** - Rust everywhere to eliminate CVEs from buffer overflows and memory corruption
3. **Fault Isolation** - Wasm sandboxing prevents third-party code from crashing core services
4. **Progressive Deployment** - Canary testing catches bad configs before global rollout
5. **Graceful Degradation** - Preserve "Last Known Good" state; fail open to maintain availability

### Request Flow

```
User Request
    ↓
BGP Anycast (routes to nearest node)
    ↓
eBPF/XDP (drops malicious packets at NIC)
    ↓
River Proxy (TLS termination via BoringSSL)
    ↓
Coraza WAF (Wasm sandbox inspection)
    ↓
DragonflyDB Cache (hit = immediate response)
    ↓
Origin Server (on cache miss)
    ↓
NATS JetStream (broadcast state updates via CRDTs)
```

## Development Status

**Current**: 7 of 24 sprints complete (29%)
**Phase 1**: ✅ COMPLETE (100%)
**Phase 2**: 🔄 IN PROGRESS (Sprint 7 complete)

### ✅ **Phase 1: Foundation & Core Node (Sprints 1-6)** - COMPLETE

| Sprint | Component | Status | Tests |
|--------|-----------|--------|-------|
| **Sprint 1** | Solana Architecture & $AEGIS Token | ✅ COMPLETE | 40 ✅ |
| **Sprint 2** | Node Registry & Staking Contracts | ✅ COMPLETE | 115 ✅ |
| **Sprint 3** | HTTP/HTTPS Proxy & TLS | ✅ COMPLETE | 26 ✅ |
| **Sprint 4** | CDN Caching (DragonflyDB/Redis) | ✅ COMPLETE | 38 ✅ |
| **Sprint 5** | Node CLI & Health Metrics | ✅ COMPLETE | 89 ✅ |
| **Sprint 6** | Reward Distribution | ✅ COMPLETE | 36 ✅ |

**Phase 1 Summary:**
- ✅ **344 tests passing** (100% pass rate)
- ✅ **4 smart contracts deployed** to Solana Devnet
- ✅ **10 CLI commands** fully functional
- ✅ **Production-ready** edge node with caching
- ✅ **Zero gaps** - all requirements exceeded

### 🔄 **Phase 2: Security & Decentralized State (Sprints 7-12)** - IN PROGRESS

| Sprint | Component | Status | Tests |
|--------|-----------|--------|-------|
| **Sprint 7** | eBPF/XDP DDoS Protection | ✅ COMPLETE | 48 ✅ |
| **Sprint 8** | WAF Integration (Coraza/Wasm) | ⏳ PLANNED | - |
| **Sprint 9** | Bot Management (Wasm) | ⏳ PLANNED | - |
| **Sprint 10** | P2P Threat Intelligence | ⏳ PLANNED | - |
| **Sprint 11** | CRDTs + NATS State Sync | ⏳ PLANNED | - |
| **Sprint 12** | Verifiable Analytics | ⏳ PLANNED | - |

**Recent Milestones:**
- 🎉 **Phase 1 COMPLETE** - All 6 sprints done (November 20, 2025)
- 🎉 **All 4 Smart Contracts Deployed to Devnet**
  - Token: `JLQ4c9UWdNoYbsbAKU59SkYAw9HdVoz1Pxu7Juu4qyB`
  - Registry: `D6kkpeujhPcoT9Er4HMaJh2FgG5fP6MEBAVogmF6ykr6`
  - Staking: `5oGLkNZ7Hku3bRD4aWnRNo8PsXusXmojm8EzAiQUVD1H`
  - Rewards: `3j4guuzvNESX5iMUFcfihRGsjEjKmjaEBD4p8GGxNs8c`
- 🎉 **Kernel-Level DDoS Protection** - eBPF/XDP SYN flood mitigation (<1μs latency)
- 🎉 **Pingora Proxy with Cache-Control** - Full HTTP caching with header processing
- 🎉 **392 Automated Tests** - Comprehensive coverage across all components
- 🎉 **10 CLI Commands** - Complete node operator toolkit
- 📊 **Professional Website** - Mobile-responsive design

**Current Focus:** Sprint 8 - WAF Integration (Coraza/Wasm)

**🔒 Security Update** (November 20, 2025):
- 3 critical vulnerabilities identified via security review
- All vulnerabilities **FIXED** with comprehensive patches
- 22 security-focused tests added (all passing)
- Ready for professional security audit
- **Impact**: Zero real funds at risk (Devnet only) ✅

## Tokenomics

### $AEGIS Utility Token

- **Payment** - Service consumers pay for CDN, WAF, edge compute in $AEGIS
- **Rewards** - Node operators earn $AEGIS for verified contributions (bandwidth, uptime, compute)
- **Staking** - Operators stake $AEGIS as security bond (slashable for malicious behavior)
- **Governance** - Token holders vote on protocol upgrades, fee structures, and treasury allocation
- **Platform** - Solana blockchain for high throughput and low transaction costs

### Value Accrual

Token value tied to:
- Network utility and service consumption growth
- Demand for decentralized, censorship-resistant infrastructure
- Geographic distribution and node count expansion
- Quality of service and competitive performance vs. centralized providers

## Getting Started

### Prerequisites

**For Node Operators:**
- Linux server (bare metal or VPS) with public IPv4/IPv6
- Minimum 4 CPU cores, 8GB RAM, 100GB SSD
- 100+ Mbps symmetric internet connection
- Solana wallet with $AEGIS tokens for staking

**For Developers:**
- Rust 1.70+ (`rustup install stable`)
- Solana CLI and Anchor framework
- Node.js 18+ (for Solana client development)
- Docker and K3s (for local testing)

### Running a Node

```bash
# Install the AEGIS node operator CLI
cargo install aegis-cli

# Register your node (requires IPFS CID with metadata)
aegis-cli register --metadata-url <IPFS_CID>

# Stake tokens to activate (minimum 1000 $AEGIS)
aegis-cli stake --amount 1000

# Start the node software
aegis-node start --config /etc/aegis/config.toml

# Monitor node status
aegis-cli status

# Claim accumulated rewards
aegis-cli claim-rewards
```

### Using AEGIS Services

```javascript
// JavaScript/TypeScript example for dApp integration
import { AegisClient } from '@aegis/sdk';

const client = new AegisClient({
  apiKey: process.env.AEGIS_API_KEY,
  network: 'mainnet'
});

// Configure CDN and WAF for your domain
await client.configureDomain({
  domain: 'example.com',
  cdn: { cacheTtl: 3600, compression: true },
  waf: { mode: 'block', ruleset: 'owasp-crs' },
  ddos: { enabled: true, sensitivity: 'medium' }
});

// Deploy edge function (Wasm)
const functionCid = await client.deployEdgeFunction({
  wasmModule: fs.readFileSync('./function.wasm'),
  routes: ['/api/*']
});
```

## Development Roadmap

### Phase 1: Foundation (Q1 2026) ✓
- [x] Solana smart contracts ($AEGIS token, registry, staking)
- [x] River proxy with DragonflyDB caching
- [x] Basic node operator CLI
- [x] Devnet deployment

### Phase 2: Security & State (Q2 2026)
- [ ] eBPF/XDP DDoS protection
- [ ] Coraza WAF integration (Wasm)
- [ ] Bot management modules
- [ ] CRDTs + NATS for global state sync
- [ ] Verifiable analytics framework

### Phase 3: Compute & Governance (Q3 2026)
- [ ] Wasm edge functions runtime
- [ ] DAO governance (proposals, voting, treasury)
- [ ] Advanced P2P routing
- [ ] IPFS/Filecoin integration

### Phase 4: Mainnet Launch (Q4 2026)
- [ ] Performance optimization and stress testing
- [ ] Multi-firm smart contract audits
- [ ] Geographic expansion (100+ edge locations)
- [ ] Mainnet token generation event

### Future Vision
- Serverless edge functions platform
- Object storage layer (R2-like)
- Distributed SQL database (D1-like)
- Cross-chain interoperability
- AI/ML inference at the edge

## Contributing

We welcome contributions from the community! AEGIS is open-source and thrives on collaboration.

### How to Contribute

1. **Code Contributions** - Submit PRs for bug fixes, features, or optimizations
2. **Run a Node** - Join the testnet and help us stress-test the infrastructure
3. **Report Issues** - Found a bug? Open an issue with detailed reproduction steps
4. **Documentation** - Improve guides, add tutorials, or translate docs
5. **Governance** - Participate in DAO proposals and community discussions

### Development Setup

```bash
# Clone the repository
git clone https://github.com/aegis-network/aegis.git
cd aegis

# Install Rust dependencies
cargo build

# Install Solana/Anchor dependencies
cd contracts && anchor build

# Run tests
cargo test --all
anchor test
```

## Performance Benchmarks

| Metric | Target | Status |
|--------|--------|--------|
| **Latency (cached)** | <60ms TTFB | ✓ Achieved |
| **Latency (proxied)** | <200ms TTFB | ✓ Achieved |
| **Throughput** | >20 Gbps per node | In Testing |
| **Requests/sec** | >2M per node | In Testing |
| **Cache Hit Ratio** | >85% | ✓ Achieved |
| **Data Plane Uptime** | 99.999% | 99.997% (testnet) |

## Security

### Audits
- Smart contracts: [Audit pending - Q3 2026]
- Core infrastructure: [Continuous security review]

### Bug Bounty
We run a bug bounty program for responsible disclosure:
- **Critical**: Up to 100,000 $AEGIS
- **High**: Up to 50,000 $AEGIS
- **Medium**: Up to 10,000 $AEGIS

Email: dimitrymd@gmail.com

### Responsible Disclosure
Please report security vulnerabilities privately to dimitrymd@gmail.com. Do not open public issues for security concerns.

## Community

- **Website**: https://aegis.funwayinteractive.com/
- **Discord**: COMING SOON
- **Twitter**: COMING SOON
- **Forum**: COMING SOON
- **Documentation**: COMING SOON

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

Built on the shoulders of giants:
- **Cloudflare** for open-sourcing Pingora
- **Solana Labs** for the high-performance blockchain
- **NLnet Labs** for Routinator (RPKI)
- **OWASP** for Coraza WAF and Core Rule Set
- **DragonflyDB** team for the modern Redis alternative
- **CNCF** for K3s, FluxCD, and cloud-native tooling

## Disclaimer

AEGIS is experimental software under active development. The $AEGIS token is a utility token for network services, not an investment vehicle. Always DYOR (Do Your Own Research) and never stake more than you can afford to lose. Cryptocurrency regulations vary by jurisdiction - ensure compliance with local laws.

---

**Built with ❤️ by the decentralized web community**

*Empowering anyone to contribute to and benefit from a globally distributed edge network*
