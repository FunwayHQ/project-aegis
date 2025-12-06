# Cargo Audit Report

**Date:** 2025-12-06
**Sprint:** Y10.8

## Summary

Running `cargo audit` identified **3 vulnerabilities** and **11 warnings** (unmaintained crates).

## Vulnerabilities

### 1. RUSTSEC-2024-0437 - protobuf (HIGH)
- **Crate:** protobuf 2.28.0
- **Issue:** Crash due to uncontrolled recursion
- **Solution:** Upgrade to >=3.7.2
- **Status:** ⚠️ Transitive dependency (pingora → prometheus → protobuf)
- **Mitigation:** Wait for pingora/prometheus upstream update

**Dependency Chain:**
```
protobuf 2.28.0
└── prometheus 0.13.4
    └── pingora-core 0.6.0
        └── aegis-node
```

### 2. RUSTSEC-2025-0009 - ring (HIGH)
- **Crate:** ring 0.16.20
- **Issue:** Some AES functions may panic when overflow checking is enabled
- **Solution:** Upgrade to >=0.17.12
- **Status:** ⚠️ Transitive dependency (ipfs-api → hyper-rustls → rustls → ring)
- **Mitigation:** Wait for ipfs-api-backend-hyper upstream update

**Dependency Chain:**
```
ring 0.16.20
├── rustls 0.20.9
│   └── hyper-rustls 0.23.2
│       └── ipfs-api-backend-hyper 0.6.0
│           └── aegis-node
└── rcgen 0.11.3
    └── libp2p-tls 0.5.0
        └── libp2p 0.54.1
            └── aegis-node
```

### 3. RUSTSEC-2024-0336 - rustls (HIGH, CVSS 7.5)
- **Crate:** rustls 0.20.9
- **Issue:** `rustls::ConnectionCommon::complete_io` could fall into an infinite loop based on network input
- **Solution:** Upgrade to >=0.23.5
- **Status:** ⚠️ Transitive dependency (ipfs-api → hyper-rustls → rustls)
- **Mitigation:** Wait for ipfs-api-backend-hyper upstream update

**Dependency Chain:**
```
rustls 0.20.9
├── tokio-rustls 0.23.4
│   └── hyper-rustls 0.23.2
│       └── ipfs-api-backend-hyper 0.6.0
│           └── aegis-node
└── hyper-rustls 0.23.2
```

## Unmaintained Crate Warnings

| Crate | Version | Advisory | Root Cause |
|-------|---------|----------|------------|
| atty | 0.2.14 | RUSTSEC-2024-0375 | pingora → clap 3.x |
| daemonize | 0.5.0 | RUSTSEC-2025-0069 | pingora-core |
| derivative | 2.2.0 | RUSTSEC-2024-0388 | pingora-core |
| instant | 0.1.13 | RUSTSEC-2024-0384 | libp2p-gossipsub |
| paste | 1.0.15 | RUSTSEC-2024-0436 | pingora-cache, libp2p |
| proc-macro-error | 1.0.4 | RUSTSEC-2024-0370 | clap 3.x, multihash |
| ring | 0.16.20 | RUSTSEC-2025-0010 | See above |
| rustls-pemfile | 1.0.4 & 2.2.0 | RUSTSEC-2025-0134 | ipfs-api, async-nats |
| yaml-rust | 0.4.5 | RUSTSEC-2024-0320 | pingora-core → serde_yaml |

## Risk Assessment

### Current Risk: MEDIUM

1. **protobuf recursion (RUSTSEC-2024-0437)**
   - Risk: DoS via crafted protobuf messages
   - Impact: Prometheus metrics endpoint only
   - Likelihood: Low (requires malformed metrics data)

2. **ring AES panic (RUSTSEC-2025-0009)**
   - Risk: Panic in AES operations with overflow checks
   - Impact: Cryptographic operations
   - Likelihood: Very Low (requires specific conditions)

3. **rustls infinite loop (RUSTSEC-2024-0336)**
   - Risk: DoS via crafted TLS handshake
   - Impact: IPFS client connections
   - Likelihood: Low (IPFS is optional feature)

## Recommended Actions

### Short-term (Immediate)
1. ✅ Document findings (this report)
2. Monitor upstream for pingora 0.7.x release
3. Consider disabling IPFS feature for production until ipfs-api updates

### Medium-term (Next Sprint)
1. Track issues:
   - https://github.com/cloudflare/pingora/issues (prometheus update)
   - https://github.com/ferristseng/rust-ipfs-api/issues (rustls update)
2. Consider alternative IPFS clients with updated dependencies
3. Evaluate prometheus-client as alternative to prometheus crate

### Long-term
1. Contribute upstream PRs to update dependencies
2. Consider vendoring/forking critical dependencies if upstream unresponsive
3. Regular cargo audit in CI pipeline

## Notes

- All vulnerabilities are in **transitive dependencies** from upstream crates
- Direct dependencies (aegis-node) use latest secure versions
- The ipfs-api-backend-hyper crate appears to be less actively maintained
- Pingora is actively developed by Cloudflare; updates likely coming

## CI Integration

Added to pre-commit hooks recommendation (Y10.12):
```bash
cargo audit --deny warnings
```
