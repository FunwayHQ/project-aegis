# AEGIS Production Hardening Checklist

**Version:** 1.0
**Last Updated:** December 2, 2025
**Status:** Pre-Mainnet

---

## 1. Dependency Security

### Cargo Audit Results

**Last Run:** December 2, 2025

| Vulnerability | Severity | Status | Notes |
|---------------|----------|--------|-------|
| protobuf (RUSTSEC-2024-0437) | High | Transitive | Via prometheus → pingora-core. Waiting for Pingora update |
| atty (RUSTSEC-2021-0145) | Warning | Transitive | Unmaintained, low risk |

### Action Items

- [ ] Monitor Pingora releases for protobuf update
- [ ] Consider replacing prometheus with OpenTelemetry metrics
- [ ] Run `cargo audit` in CI pipeline

### Verification Commands

```bash
# Check for vulnerabilities
cargo audit

# Check for outdated dependencies
cargo outdated

# Check dependency licenses
cargo deny check licenses
```

---

## 2. Configuration Hardening

### Edge Node Configuration

| Setting | Production Value | Verified |
|---------|------------------|----------|
| TLS minimum version | 1.3 | [ ] |
| Debug logging | Disabled | [ ] |
| Verbose errors | Disabled | [ ] |
| Stack traces | Disabled | [ ] |
| Metrics endpoint | Authenticated | [ ] |
| Admin API | Disabled or mTLS | [ ] |

### eBPF Configuration

| Setting | Production Value | Verified |
|---------|------------------|----------|
| SYN threshold | 100/s per IP | [ ] |
| UDP threshold | 1000/s per IP | [ ] |
| Auto-blacklist duration | 30 seconds | [ ] |
| Blocklist size | 5000 entries | [ ] |
| Whitelist reviewed | Yes | [ ] |

### WAF Configuration

| Setting | Production Value | Verified |
|---------|------------------|----------|
| OWASP CRS rules | Enabled | [ ] |
| Paranoia level | 2 (recommended) | [ ] |
| Anomaly threshold | 5 | [ ] |
| Request body limit | 10MB | [ ] |
| Response body scanning | Disabled (perf) | [ ] |

### Challenge System

| Setting | Production Value | Verified |
|---------|------------------|----------|
| PoW difficulty | 16 bits | [ ] |
| Challenge TTL | 5 minutes | [ ] |
| Token TTL | 15 minutes | [ ] |
| Rate limit | 5/min per IP | [ ] |

---

## 3. Secret Management

### Secrets Audit

| Secret | Storage Location | Rotation Schedule | Verified |
|--------|------------------|-------------------|----------|
| Node identity key | Encrypted file | On compromise | [ ] |
| TLS certificates | ACME auto-renewed | 90 days | [ ] |
| Wallet keys | Hardware wallet | N/A | [ ] |
| Wasm signing keys | HSM/KMS | Annual | [ ] |
| API tokens | Environment vars | Quarterly | [ ] |

### Verification

- [ ] No secrets in source code (`git secrets --scan`)
- [ ] No secrets in logs (audit log output)
- [ ] No secrets in error messages (test error paths)
- [ ] Environment variables properly scoped
- [ ] Key files have correct permissions (600)

---

## 4. Network Hardening

### TLS Configuration

```toml
# Verified production TLS settings
[tls]
min_version = "TLS1.3"
prefer_server_ciphers = true
session_timeout = 86400

# Security headers
[headers]
strict_transport_security = "max-age=31536000; includeSubDomains; preload"
x_content_type_options = "nosniff"
x_frame_options = "DENY"
x_xss_protection = "1; mode=block"
content_security_policy = "default-src 'self'"
```

### Firewall Rules

| Port | Protocol | Source | Purpose | Verified |
|------|----------|--------|---------|----------|
| 443 | TCP | Any | HTTPS traffic | [ ] |
| 80 | TCP | Any | HTTP → HTTPS redirect | [ ] |
| 9001 | TCP | P2P peers | libp2p | [ ] |
| 22 | TCP | Admin IPs | SSH (if needed) | [ ] |
| * | * | Other | DENY | [ ] |

### Rate Limiting

| Endpoint | Limit | Verified |
|----------|-------|----------|
| General traffic | 100 req/s per IP | [ ] |
| Authentication | 10 req/min per IP | [ ] |
| API writes | 20 req/min per IP | [ ] |
| P2P messages | 1000/s per peer | [ ] |

---

## 5. Logging & Monitoring

### Security Logging

| Event Type | Logged | Retention | Alert | Verified |
|------------|--------|-----------|-------|----------|
| Auth failures | Yes | 90 days | >100/min | [ ] |
| WAF blocks | Yes | 30 days | >1000/min | [ ] |
| Rate limits | Yes | 7 days | >500/min | [ ] |
| Admin actions | Yes | 1 year | All | [ ] |
| P2P sig failures | Yes | 90 days | >10/min | [ ] |

### Metrics Collection

- [ ] CPU, memory, disk, network metrics
- [ ] Request latency percentiles
- [ ] Error rates by type
- [ ] Cache hit/miss ratio
- [ ] eBPF drop statistics

### Alerting

- [ ] PagerDuty/OpsGenie configured
- [ ] On-call rotation set
- [ ] Runbook links in alerts
- [ ] Escalation paths defined

---

## 6. Smart Contract Hardening

### Pre-Deployment Checks

| Check | Status | Verified |
|-------|--------|----------|
| External audit complete | ✅ Sprint 28 | [ ] |
| All high/critical findings fixed | ✅ Sprint 29 | [ ] |
| Upgrade mechanism tested | Pending | [ ] |
| Pause mechanism tested | Pending | [ ] |
| Devnet testing complete | ✅ | [ ] |
| Testnet testing complete | Pending | [ ] |

### Contract Settings

| Contract | Admin Key | Timelock | Verified |
|----------|-----------|----------|----------|
| Token | Multi-sig | N/A | [ ] |
| Staking | Multi-sig | 48 hours | [ ] |
| Registry | Multi-sig | 48 hours | [ ] |
| Rewards | Multi-sig | 48 hours | [ ] |
| DAO | Multi-sig | 48 hours | [ ] |

---

## 7. Operational Security

### Access Control

| System | Auth Method | MFA | Access Review | Verified |
|--------|-------------|-----|---------------|----------|
| Production nodes | SSH keys | Yes | Quarterly | [ ] |
| Cloud console | SSO | Yes | Quarterly | [ ] |
| GitHub | SSO + SAML | Yes | Quarterly | [ ] |
| Solana wallets | Hardware | N/A | Per-use | [ ] |

### Backup & Recovery

| Data | Backup Frequency | Retention | Tested | Verified |
|------|------------------|-----------|--------|----------|
| Node configs | Daily | 30 days | [ ] | [ ] |
| TLS certs | On renewal | 1 year | [ ] | [ ] |
| Wallet backups | On creation | Forever | [ ] | [ ] |
| Logs | Real-time | 90 days | [ ] | [ ] |

---

## 8. Pre-Launch Verification

### Final Checks

- [ ] All checklist items above verified
- [ ] Bug bounty program launched
- [ ] Incident response team on standby
- [ ] Communication channels ready
- [ ] Rollback procedure documented
- [ ] External audit sign-off received
- [ ] Legal review complete
- [ ] Insurance coverage confirmed

### Launch Day Monitoring

- [ ] Extra staff on call
- [ ] War room channel open
- [ ] Real-time dashboards visible
- [ ] Automated alerts active
- [ ] Social media monitored
- [ ] Support channels staffed

---

## Sign-Off

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Security Lead | | | |
| Infrastructure Lead | | | |
| Smart Contract Lead | | | |
| CEO/CTO | | | |

---

## Appendix: Verification Commands

```bash
# Dependency audit
cargo audit
cargo outdated
cargo deny check

# Secret scanning
git secrets --scan
trufflehog git file://.

# TLS verification
openssl s_client -connect aegis.network:443 -tls1_3

# Security headers check
curl -I https://aegis.network | grep -i security

# eBPF program loaded
bpftool prog list | grep syn_flood

# Log verification
grep -i "password\|secret\|key" /var/log/aegis/*.log
```

---

*This checklist must be completed and signed before mainnet launch.*
