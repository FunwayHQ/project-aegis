# Security Policy

## Reporting Security Issues

**Do NOT report security vulnerabilities through public GitHub issues.**

Please report security vulnerabilities by emailing: security@aegis.network

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested mitigations

We will acknowledge receipt within 24 hours and provide a detailed response within 72 hours.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Security Measures

### Memory Safety
- **Rust-Only Data Plane**: All critical path code written in Rust to eliminate memory corruption vulnerabilities
- **Wasm Sandboxing**: Edge functions run in isolated Wasm sandboxes with resource limits
- **No Unsafe Code** (except eBPF): Minimal unsafe blocks, all audited and documented

### Cryptography
- **Ed25519 Signatures**: All Wasm modules require cryptographic signatures
- **SHA-256 Hashing**: Used for integrity verification (JA3/JA4 fingerprints, cache keys)
- **TLS 1.3**: BoringSSL for TLS termination

### Input Validation
- **CRLF Injection Prevention**: Header value sanitization
- **Path Traversal Protection**: WAF rules and input validation
- **SQL Injection Protection**: WAF rules (OWASP CRS compatible)
- **XSS Protection**: WAF rules and output encoding
- **ReDoS Protection**: Safe regex compilation with timeouts

### Rate Limiting & DoS Protection
- **Distributed Rate Limiter**: CRDT-based rate limiting across nodes
- **eBPF/XDP**: Kernel-level packet filtering (Linux only)
- **Byzantine Tolerance**: Validators detect and block malicious actors

### Blockchain Security (Solana)
- **Vote Escrow Pattern**: Prevents flash loan attacks on DAO voting
- **Timelock**: 48-hour delay on configuration changes
- **Nonce Tracking**: Replay attack protection
- **Ownership Validation**: Token account and mint verification

## Security Testing

### Automated Testing
- **Unit Tests**: 500+ tests covering security-critical paths
- **Property-Based Tests**: CRDT properties (commutativity, idempotence, associativity)
- **Stress Tests**: DoS resistance verification
- **Fuzzing**: cargo-fuzz targets for TLS parser, WAF, route config, Wasm loader, cache keys

### Manual Review
- **Code Review**: All PRs require security-focused review
- **Dependency Audit**: Regular `cargo audit` runs (see CARGO_AUDIT_REPORT.md)
- **Penetration Testing**: Planned for mainnet preparation

## Known Vulnerabilities

### Transitive Dependencies
See `node/CARGO_AUDIT_REPORT.md` for current cargo audit findings.

Current issues are in upstream dependencies:
- `protobuf` (via prometheus/pingora)
- `ring` (via ipfs-api/libp2p)
- `rustls` (via ipfs-api)

These are monitored and will be addressed when upstream updates are available.

## Security Architecture

### Defense in Depth
```
Internet → eBPF/XDP (kernel) → WAF (application) → Bot Management → Rate Limiter → Wasm Functions → Origin
```

### Fail-Open Design
- Data plane continues operating if control plane is unavailable
- Last Known Good configuration retained on parse errors
- Pipeline errors don't crash proxy

### Isolation
- Control plane separate from data plane
- Each Wasm module isolated
- Per-resource rate limiting

## Incident Response

See `INCIDENT_RESPONSE_PLAYBOOK.md` for detailed procedures.

### Severity Levels
- **P0 (Critical)**: Active exploitation, immediate response
- **P1 (High)**: Vulnerability discovered, 24-hour response
- **P2 (Medium)**: Security improvement needed, 7-day response
- **P3 (Low)**: Minor issue, scheduled maintenance

## Compliance

### Standards
- OWASP Top 10 (Web Application Security)
- OWASP API Security Top 10
- CWE/SANS Top 25

### Audits
- Internal security review: Sprints Y1-Y10
- External audit: Planned for mainnet

## Security Contacts

- Security Team: security@aegis.network
- Bug Bounty: TBD
- PGP Key: TBD
