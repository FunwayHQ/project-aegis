# AEGIS Security Architecture

**Version:** 1.0
**Last Updated:** December 2, 2025
**Classification:** Public

## 1. System Overview

AEGIS is a decentralized CDN and edge security network with the following major components:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           INTERNET                                      │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        BGP ANYCAST LAYER                                │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐                    │
│  │ Edge 1  │  │ Edge 2  │  │ Edge 3  │  │ Edge N  │  (Global)          │
│  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘                    │
└───────┼────────────┼────────────┼────────────┼──────────────────────────┘
        │            │            │            │
        ▼            ▼            ▼            ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        EDGE NODE (per node)                             │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                    eBPF/XDP (Kernel)                              │  │
│  │  • SYN flood detection    • UDP flood detection                  │  │
│  │  • IPv4/IPv6 blocklist    • Auto-blacklisting                    │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                │                                        │
│                                ▼                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                    Pingora Proxy (Rust)                          │  │
│  │  • TLS 1.3 termination    • Route-based dispatch                 │  │
│  │  • Wasm module pipeline   • Cache integration                    │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                │                                        │
│         ┌──────────────────────┼──────────────────────┐                │
│         ▼                      ▼                      ▼                │
│  ┌────────────┐  ┌────────────────────────┐  ┌────────────────┐        │
│  │    WAF     │  │ Challenge System       │  │ API Security   │        │
│  │  (Wasm)    │  │ (PoW, Fingerprint)     │  │ (JWT, Schema)  │        │
│  └────────────┘  └────────────────────────┘  └────────────────┘        │
│                                │                                        │
│                                ▼                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                    DragonflyDB (Cache)                           │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
        │                        │
        ▼                        ▼
┌─────────────────┐    ┌─────────────────────────────────────────────────┐
│  P2P Network    │    │              SOLANA BLOCKCHAIN                  │
│  (libp2p)       │    │  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐   │
│  • Threat Intel │    │  │ Token  │ │Registry│ │Staking │ │  DAO   │   │
│  • Trust Tokens │    │  └────────┘ └────────┘ └────────┘ └────────┘   │
└─────────────────┘    └─────────────────────────────────────────────────┘
```

---

## 2. Trust Boundaries

### Boundary 1: Internet ↔ Edge Node
- **Trust Level:** Untrusted → Partially Trusted
- **Controls:** eBPF/XDP filtering, TLS termination, WAF

### Boundary 2: Edge Node ↔ Origin Server
- **Trust Level:** Partially Trusted → Trusted
- **Controls:** mTLS (optional), request validation

### Boundary 3: Edge Node ↔ P2P Network
- **Trust Level:** Partially Trusted ↔ Partially Trusted
- **Controls:** Ed25519 message signatures, trusted node registry

### Boundary 4: Edge Node ↔ Blockchain
- **Trust Level:** Partially Trusted → Trusted (consensus)
- **Controls:** Cryptographic verification, RPC authentication

### Boundary 5: User ↔ DAO dApp
- **Trust Level:** Untrusted → Trusted
- **Controls:** Wallet signatures, frontend validation

---

## 3. Data Flow with Encryption States

### HTTP Request Flow

```
Client                Edge Node                Origin
  │                      │                       │
  │──── HTTPS (TLS 1.3) ─┤                       │
  │                      │                       │
  │                      │ [Decrypt, Inspect]    │
  │                      │                       │
  │                      │ WAF ✓ Challenge ✓     │
  │                      │                       │
  │                      │──── HTTPS/mTLS ──────▶│
  │                      │                       │
  │◀─── HTTPS (TLS 1.3) ─┤◀──── Response ────────│
  │                      │                       │
```

### Threat Intelligence Flow

```
Node A                P2P Network              Node B
  │                      │                       │
  │ Detect threat        │                       │
  │ Sign(Ed25519)        │                       │
  │                      │                       │
  │──── Gossipsub ───────┤                       │
  │     (Noise enc)      │                       │
  │                      │─── Gossipsub ────────▶│
  │                      │                       │
  │                      │      Verify(Ed25519)  │
  │                      │      Add to blocklist │
  │                      │                       │
```

### Blockchain Transaction Flow

```
Node Operator         CLI               Solana RPC         Blockchain
     │                 │                    │                   │
     │ stake()         │                    │                   │
     │ Sign(wallet)    │                    │                   │
     │────────────────▶│                    │                   │
     │                 │ Serialize + Sign   │                   │
     │                 │───── HTTPS ───────▶│                   │
     │                 │                    │── Submit tx ─────▶│
     │                 │                    │                   │
     │                 │◀── Confirmation ───│◀── Confirm ───────│
     │◀────────────────│                    │                   │
```

---

## 4. Authentication & Authorization Matrix

| Component | Auth Method | Authorization Model |
|-----------|-------------|---------------------|
| **Edge Node API** | API Key / mTLS | Role-based (operator, admin) |
| **P2P Network** | Ed25519 signatures | Trusted node registry |
| **DAO Proposals** | Wallet signature | Token-weighted voting |
| **Staking** | Wallet signature | Self-service (own stake) |
| **Rewards Claim** | Wallet signature | Performance-based |
| **Node Registration** | Wallet signature | Stake requirement |
| **Wasm Modules** | Ed25519 signatures | IPFS CID verification |
| **Admin Operations** | Multi-sig | Timelock (48 hours) |

---

## 5. Key Management

### Key Types

| Key Type | Algorithm | Storage | Rotation |
|----------|-----------|---------|----------|
| **Node Identity** | Ed25519 | Local encrypted file | On compromise |
| **TLS Certificates** | ECDSA P-256 | Memory (auto-renewed) | 90 days (ACME) |
| **Wallet Keys** | Ed25519 (Solana) | User's wallet | User controlled |
| **Wasm Signing Keys** | Ed25519 | HSM / secure enclave | Annual |
| **P2P Identity** | Ed25519 (libp2p) | Local encrypted file | On compromise |

### Key Storage Recommendations

**Node Operators:**
- Store wallet keys in hardware wallet (Ledger/Trezor)
- Use dedicated machine for key operations
- Backup seed phrases offline (metal plate)

**AEGIS Infrastructure:**
- Wasm signing keys in HSM or cloud KMS
- Automated TLS via ACME (no manual key handling)
- P2P keys encrypted at rest with node-specific password

---

## 6. Secret Handling

### Environment Variables

```bash
# Required secrets (must be set externally)
AEGIS_NODE_KEY_PASSWORD=         # Decrypts node identity key
AEGIS_SOLANA_RPC_URL=            # Solana RPC endpoint
AEGIS_WALLET_PATH=               # Path to wallet keypair

# Optional secrets
AEGIS_SENTRY_DSN=                # Error reporting
AEGIS_METRICS_TOKEN=             # Metrics export auth
```

### Secret Injection Methods

| Environment | Method |
|-------------|--------|
| Development | `.env` file (gitignored) |
| Docker | Docker secrets / env files |
| Kubernetes | K8s Secrets (encrypted etcd) |
| Cloud | AWS Secrets Manager / Vault |

### What MUST NOT Be in Code

- Private keys (any type)
- API tokens
- Database credentials
- Encryption keys
- JWT secrets

---

## 7. Cryptographic Standards

### Algorithms in Use

| Purpose | Algorithm | Key Size | Library |
|---------|-----------|----------|---------|
| Message signing | Ed25519 | 256-bit | ed25519-dalek |
| TLS | TLS 1.3 | N/A | BoringSSL |
| Key exchange | X25519 | 256-bit | BoringSSL |
| Hashing | SHA-256 | 256-bit | sha2 crate |
| Password hashing | Argon2id | 256-bit | argon2 crate |
| Challenge PoW | SHA-256 | 16-bit prefix | sha2 crate |
| P2P encryption | Noise XX | 256-bit | libp2p-noise |

### Deprecated/Prohibited

- MD5, SHA-1 (any use)
- RSA < 2048 bits
- TLS 1.0, 1.1, 1.2 (prefer 1.3)
- RC4, DES, 3DES
- ECB mode for any cipher

---

## 8. Network Security

### TLS Configuration

```toml
# Minimum TLS version
min_version = "TLS1.3"

# Cipher suites (TLS 1.3 only)
ciphers = [
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256",
    "TLS_AES_128_GCM_SHA256"
]

# HSTS header
strict_transport_security = "max-age=31536000; includeSubDomains; preload"
```

### Rate Limiting

| Endpoint | Limit | Window |
|----------|-------|--------|
| General API | 100 req/s | Per IP |
| Auth endpoints | 10 req/min | Per IP |
| Challenge verification | 5 req/min | Per IP |
| P2P messages | 1000/s | Per peer |

### DDoS Thresholds (eBPF)

| Attack Type | Threshold | Action |
|-------------|-----------|--------|
| SYN flood | 100/s per IP | Drop + temporary block |
| UDP flood | 1000/s per IP | Drop + temporary block |
| Severe (2x threshold) | Auto | 30-second blocklist |

---

## 9. Logging & Monitoring

### Security Events Logged

| Event | Log Level | Retention |
|-------|-----------|-----------|
| Authentication failures | WARN | 90 days |
| WAF blocks | INFO | 30 days |
| Challenge failures | INFO | 7 days |
| P2P signature failures | WARN | 90 days |
| Rate limit triggers | INFO | 7 days |
| Admin operations | AUDIT | 1 year |
| Contract interactions | AUDIT | 1 year |

### What is NOT Logged

- Request/response bodies (privacy)
- User passwords/tokens
- Full credit card numbers
- Personal identifying information

### Monitoring Alerts

| Condition | Severity | Response |
|-----------|----------|----------|
| WAF blocks > 1000/min | High | Investigate attack |
| Auth failures > 100/min | High | Possible brute force |
| P2P signature failures > 10/min | Critical | Possible network attack |
| Node offline > 5 min | High | Check infrastructure |
| CPU > 90% sustained | Medium | Scale or optimize |

---

## 10. Compliance Considerations

### Data Privacy

- No PII stored on edge nodes
- IP addresses treated as personal data
- Logs anonymized after 7 days (IP → hash)
- GDPR data deletion supported

### Security Standards Alignment

| Standard | Status | Notes |
|----------|--------|-------|
| OWASP Top 10 | Compliant | WAF rules aligned |
| CIS Benchmarks | Partial | K8s hardening applied |
| SOC 2 Type II | Planned | Post-mainnet |

---

## 11. Security Contacts

| Role | Contact |
|------|---------|
| Security Team | security@aegis.network |
| Bug Bounty | See BUG-BOUNTY-PROGRAM.md |
| Emergency | security+urgent@aegis.network |

---

## Appendix: Security Checklist for Deployment

- [ ] All secrets externalized (no hardcoded values)
- [ ] TLS 1.3 enforced, older versions disabled
- [ ] WAF rules enabled and tested
- [ ] Rate limiting configured
- [ ] eBPF programs loaded
- [ ] P2P signatures enabled
- [ ] Logging to secure storage
- [ ] Monitoring alerts configured
- [ ] Backup keys stored securely
- [ ] Incident response contacts verified
