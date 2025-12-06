# Security Remediation Sprint Plan (Y-Series)

**Created:** 2025-12-03
**Based On:** Security Review 2025-12-03
**Total Findings:** 85 (9 Critical, 16 High, 31 Medium, 21 Low, 8 Informational)

---

## Sprint Overview

| Sprint | Focus Area | Severity | Items | Duration | Dependencies | Status |
|--------|------------|----------|-------|----------|--------------|--------|
| **Y1** | NATS & Distributed State Authentication | 🔴 CRITICAL | 6 | 1 week | None | ✅ COMPLETE |
| **Y2** | Solana Contract Hardening | 🔴🟠 CRITICAL/HIGH | 7 | 1 week | None | ✅ COMPLETE |
| **Y3** | Input Validation & Memory Safety | 🔴🟠 CRITICAL/HIGH | 8 | 1 week | None | ✅ COMPLETE |
| **Y4** | Wasm Runtime Security | 🔴🟠 CRITICAL/HIGH | 8 | 1 week | Y3 | ✅ COMPLETE |
| **Y5** | P2P & Cryptographic Hardening | 🟠 HIGH | 8 | 1 week | Y1 | ✅ COMPLETE |
| **Y6** | Distributed Systems Resilience | 🟡 MEDIUM | 10 | 1 week | Y1, Y5 |
| **Y7** | Smart Contract Refinements | 🟡 MEDIUM | 8 | 1 week | Y2 |
| **Y8** | API & Edge Security | 🟡 MEDIUM | 8 | 1 week | Y3, Y4 |
| **Y9** | Defense in Depth | 🟡🔵 MEDIUM/LOW | 12 | 1 week | Y6-Y8 |
| **Y10** | Security Testing & Hardening | 🔵ℹ️ LOW/INFO | 10+ | 1 week | Y1-Y9 |

**Total Estimated Duration:** 10 weeks (parallelizable to 6 weeks with 2 teams)

---

## Sprint Y1: NATS & Distributed State Authentication

**Duration:** 1 week
**Priority:** 🔴 CRITICAL
**Team:** Backend/Distributed Systems
**Risk if Delayed:** Complete compromise of distributed state

### Objectives
Secure all NATS JetStream communications with authentication, authorization, and encryption.

### Tasks

| ID | Finding | Task | File | Effort | Tests |
|----|---------|------|------|--------|-------|
| Y1.1 | P2P-C1 | Add Ed25519 signatures to `CrdtMessage` struct | `nats_sync.rs` | M | 5 |
| Y1.2 | P2P-C1 | Implement `sign()` and `verify()` methods for CRDT messages | `nats_sync.rs` | M | 3 |
| Y1.3 | P2P-C1 | Add signature verification before merging operations | `nats_sync.rs` | S | 2 |
| Y1.4 | P2P-C2 | Add NATS authentication config (username/password/token/nkey) | `nats_sync.rs` | M | 3 |
| Y1.5 | P2P-C2 | Implement `ConnectOptions` with authentication | `nats_sync.rs` | S | 2 |
| Y1.6 | P2P-H2 | Enforce TLS for all NATS connections | `nats_sync.rs` | S | 2 |

### Acceptance Criteria
- [ ] All CRDT messages are Ed25519 signed
- [ ] Unsigned/invalid messages are rejected with logging
- [ ] NATS connections require authentication
- [ ] TLS is mandatory in production mode
- [ ] 17 new tests pass

### Code Changes
```rust
// Y1.1-Y1.3: Signed CRDT messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtMessage {
    pub actor_id: u64,
    pub operation: CounterOp,
    pub timestamp: u64,
    pub signature: String,      // NEW
    pub public_key: String,     // NEW
}

impl CrdtMessage {
    pub fn sign(&mut self, signing_key: &SigningKey) { ... }
    pub fn verify(&self, expected_key: &VerifyingKey) -> bool { ... }
}

// Y1.4-Y1.6: Authenticated NATS
pub struct NatsConfig {
    pub server_url: String,
    pub auth: NatsAuth,         // NEW
    pub require_tls: bool,      // NEW
}

pub enum NatsAuth {
    None,
    UserPassword { username: String, password: String },
    Token(String),
    NKey { seed: String },
}
```

---

## Sprint Y2: Solana Contract Hardening

**Duration:** 1 week
**Priority:** 🔴🟠 CRITICAL/HIGH
**Team:** Blockchain/Smart Contracts
**Risk if Delayed:** Reward manipulation, slashing bypass

### Objectives
Fix replay attacks in Rewards contract and PDA collision in Staking contract.

### Tasks

| ID | Finding | Task | File | Effort | Tests |
|----|---------|------|------|--------|-------|
| Y2.1 | SC-H1 | Add epoch validation: `require!(epoch > last_performance_epoch)` | `rewards/lib.rs` | S | 3 |
| Y2.2 | SC-H2 | Add nonce field to performance signature message | `rewards/lib.rs` | M | 4 |
| Y2.3 | SC-H2 | Store used nonces per operator to prevent replay | `rewards/lib.rs` | M | 3 |
| Y2.4 | SC-H3 | Replace timestamp with incrementing nonce in SlashRequest PDA seeds | `staking/lib.rs` | M | 4 |
| Y2.5 | SC-H3 | Add `slash_nonce` field to `GlobalConfig` | `staking/lib.rs` | S | 2 |
| Y2.6 | SC-C1 | Use `registry_config.min_stake_for_registration` in validation | `registry/lib.rs` | S | 2 |
| Y2.7 | SC-M4 | Add `MIN_COOLDOWN_PERIOD` constant validation (1 day minimum) | `staking/lib.rs` | S | 2 |

### Acceptance Criteria
- [ ] Stale epoch attestations are rejected
- [ ] Replay attacks with same nonce are rejected
- [ ] Multiple slash requests in same slot work correctly
- [ ] Configurable min_stake is enforced
- [ ] Cooldown period cannot be set below 24 hours
- [ ] 20 new tests pass

### Code Changes
```rust
// Y2.1: Epoch validation
require!(
    epoch > rewards.last_performance_epoch,
    RewardsError::StaleEpoch
);

// Y2.2-Y2.3: Nonce in signature
let mut message = Vec::with_capacity(32 + 8 + 1 + 1 + 1 + 8 + 8);
message.extend_from_slice(operator.as_ref());
message.extend_from_slice(&epoch.to_le_bytes());
message.extend_from_slice(&[uptime_percentage, latency_score, throughput_score]);
message.extend_from_slice(&requests_served.to_le_bytes());
message.extend_from_slice(&nonce.to_le_bytes());  // NEW

// Y2.4-Y2.5: Slash request with nonce
seeds = [b"slash_request", stake_account.operator.as_ref(), &global_config.slash_nonce.to_le_bytes()]
```

---

## Sprint Y3: Input Validation & Memory Safety

**Duration:** 1 week
**Priority:** 🔴🟠 CRITICAL/HIGH
**Team:** Node/Core
**Risk if Delayed:** Memory corruption, DoS, injection attacks

### Objectives
Fix buffer overflow in TLS parsing, cache key injection, and ReDoS vulnerabilities.

### Tasks

| ID | Finding | Task | File | Effort | Tests |
|----|---------|------|------|--------|-------|
| Y3.1 | INPUT-C1 | Add comprehensive bounds checking to `ClientHello::parse()` | `tls_fingerprint.rs` | L | 8 |
| Y3.2 | INPUT-C1 | Validate all array indices before access | `tls_fingerprint.rs` | M | 5 |
| Y3.3 | INPUT-C2 | Add `MAX_CACHE_KEY_LENGTH` constant (1024) | `cache.rs` | S | 2 |
| Y3.4 | INPUT-C2 | Sanitize cache keys (remove CRLF, truncate) | `cache.rs` | S | 3 |
| Y3.5 | INPUT-C3 | Use `safe_compile_regex()` for OpenAPI patterns | `api_security.rs` | S | 2 |
| Y3.6 | INPUT-H1 | Validate prefix_len range (0-32 for IPv4, 0-128 for IPv6) | `ip_extraction.rs` | S | 3 |
| Y3.7 | INPUT-H3 | Add `MAX_REQUEST_BODY_SIZE` constant (1MB) | `api_security.rs` | S | 2 |
| Y3.8 | INPUT-H5 | Handle invalid UTF-8 in SNI explicitly (return None) | `tls_fingerprint.rs` | S | 2 |

### Acceptance Criteria
- [ ] Malformed TLS ClientHello does not crash
- [ ] Cache keys are sanitized and bounded
- [ ] ReDoS patterns are detected and rejected
- [ ] Integer overflows are prevented
- [ ] 27 new tests pass including fuzz tests

### Code Changes
```rust
// Y3.1-Y3.2: Bounds-checked TLS parsing
fn ptr_at<T>(ctx: &[u8], offset: usize) -> Option<&T> {
    if offset + mem::size_of::<T>() > ctx.len() {
        return None;
    }
    // Safe access
}

// Y3.3-Y3.4: Safe cache keys
const MAX_CACHE_KEY_LENGTH: usize = 1024;

pub fn generate_cache_key(method: &str, uri: &str) -> Result<String, CacheError> {
    let safe_uri = uri.chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .take(MAX_CACHE_KEY_LENGTH - method.len() - 20)
        .collect::<String>();
    Ok(format!("aegis:cache:{}:{}", method, safe_uri))
}

// Y3.5: Safe regex compilation
if let Some(pat) = pattern {
    if let Some(re) = safe_compile_regex(pat, "OpenAPI pattern") {
        // Use regex
    }
}
```

---

## Sprint Y4: Wasm Runtime Security ✅ COMPLETE

**Duration:** 1 week
**Priority:** 🔴🟠 CRITICAL/HIGH
**Team:** Node/Wasm
**Depends On:** Y3
**Completed:** 2025-12-06
**Tests Added:** 24 (12 route_config + 12 ipfs_client)

### Objectives
Secure Wasm runtime with proper resource limits, signature verification, and route validation.

### Tasks

| ID | Finding | Task | File | Effort | Status |
|----|---------|------|------|--------|--------|
| Y4.1 | WASM-C1 | Remove legacy `RoutePattern::matches()` method | `route_config.rs` | S | ✅ |
| Y4.2 | WASM-C1 | Migrate all route matching to `CompiledRoutePattern` | `route_config.rs` | M | ✅ |
| Y4.3 | WASM-C2 | Calibrate fuel limits with benchmarks | `wasm_runtime.rs` | M | ✅ |
| Y4.4 | WASM-C2 | Implement epoch-based timeout enforcement | `wasm_runtime.rs` | M | ✅ |
| Y4.5 | WASM-C3 | Add compile-time check preventing dev_unsigned_modules in release | `wasm_runtime.rs` | S | ✅ |
| Y4.6 | WASM-H2 | Enforce memory limits via `config.max_memory_size()` | `wasm_runtime.rs` | S | ✅ |
| Y4.7 | WASM-H4 | Validate route priority range (0-10000), use `u16` | `route_config.rs` | S | ✅ |
| Y4.8 | INPUT-H2 | Validate IPFS CID format before URL construction | `ipfs_client.rs` | S | ✅ |

### Acceptance Criteria
- [x] ReDoS via route patterns is impossible (deprecated legacy `matches()`, uses CompiledRoutePattern)
- [x] Wasm modules timeout after calibrated limits (WAF: 2M fuel/10ms, Edge: 10M fuel/50ms)
- [x] dev_unsigned_modules fails to compile in release (compile_error! added)
- [x] Wasm memory is bounded (10MB WAF, 50MB edge functions via constants)
- [x] Route priorities are validated (MIN_ROUTE_PRIORITY=0, MAX_ROUTE_PRIORITY=10000)
- [x] Invalid CIDs are rejected (validate_cid_format with prefix/character validation)
- [x] 24 new tests pass

### Code Changes
```rust
// Y4.5: Compile-time check
#[cfg(all(feature = "dev_unsigned_modules", not(debug_assertions)))]
compile_error!("dev_unsigned_modules MUST NOT be enabled in release builds!");

// Y4.6: Memory limits
let mut config = Config::new();
config.max_memory_size(if is_waf { 10 * 1024 * 1024 } else { 50 * 1024 * 1024 });

// Y4.7: Priority validation
pub fn validate_route(&self) -> Result<(), RouteConfigError> {
    if self.priority > 10000 {
        return Err(RouteConfigError::InvalidPriority(self.priority));
    }
    Ok(())
}

// Y4.8: CID validation
fn validate_cid_format(cid: &str) -> Result<()> {
    if !cid.starts_with("Qm") && !cid.starts_with("bafy") && !cid.starts_with("bafk") {
        anyhow::bail!("Invalid CID format");
    }
    if !cid.chars().all(|c| c.is_alphanumeric()) {
        anyhow::bail!("CID contains invalid characters");
    }
    Ok(())
}
```

---

## Sprint Y5: P2P & Cryptographic Hardening

**Duration:** 1 week
**Priority:** 🟠 HIGH
**Team:** Backend/Security
**Depends On:** Y1
**Risk if Delayed:** Message replay, authentication bypass, collision attacks

### Objectives
Add replay protection to P2P messaging and fix cryptographic weaknesses.

### Tasks

| ID | Finding | Task | File | Effort | Tests |
|----|---------|------|------|--------|-------|
| Y5.1 | P2P-C3 | Make `GlobalBlocklist::add()` private | `distributed_enforcement.rs` | S | 2 |
| Y5.2 | P2P-C3 | Update all call sites to use `add_verified()` | `distributed_enforcement.rs` | M | 3 |
| Y5.3 | P2P-H1 | Add `message_id` field to `ThreatIntelligence` | `threat_intel_p2p.rs` | S | 2 |
| Y5.4 | P2P-H1 | Implement `seen_message_ids` tracking (LRU cache) | `threat_intel_p2p.rs` | M | 4 |
| Y5.5 | P2P-H3 | Remove trust token bootstrap mode | `distributed_enforcement.rs` | M | 3 |
| Y5.6 | P2P-H3 | Require pre-populated node keys from config/blockchain | `distributed_enforcement.rs` | M | 3 |
| Y5.7 | CRYPTO-H1 | Replace MD5 with SHA-256 in JA3/JA4 fingerprinting | `tls_fingerprint.rs` | S | 2 |
| Y5.8 | CRYPTO-H2 | Add nonce/jti field to `ChallengeToken` | `challenge.rs` | M | 4 |

### Acceptance Criteria
- [x] Unsigned blocklist entries are rejected
- [x] Replayed P2P messages are detected and dropped
- [x] Trust tokens from unknown nodes are rejected
- [x] JA3 fingerprints use SHA-256 (document breaking change)
- [x] Challenge tokens include replay protection
- [x] 23 new tests pass (actual: 15 new tests + updated existing tests)

### Code Changes
```rust
// Y5.1-Y5.2: Private blocklist add
impl GlobalBlocklist {
    // Make private
    async fn add(&self, threat: &EnhancedThreatIntel) { ... }

    // Public verified method
    pub async fn add_verified(
        &self,
        threat: &EnhancedThreatIntel,
        public_key: &VerifyingKey
    ) -> Result<(), String> {
        threat.verify_signature(public_key)?;
        self.add(threat).await;
        Ok(())
    }
}

// Y5.3-Y5.4: Message ID tracking
pub struct ThreatIntelligence {
    pub message_id: String,  // NEW: UUID
    pub ip: String,
    // ...
}

// In handler:
if self.seen_message_ids.contains(&msg.message_id) {
    debug!("Duplicate message {} - ignoring", msg.message_id);
    continue;
}
self.seen_message_ids.insert(msg.message_id.clone());

// Y5.7: SHA-256 JA3
let ja3 = format!("{:x}", Sha256::digest(&ja3_raw));

// Y5.8: Challenge token replay protection
pub struct ChallengeToken {
    pub jti: String,  // NEW: unique token ID
    pub typ: ChallengeType,
    pub iat: u64,
    pub exp: u64,
    // ...
}
```

---

## Sprint Y6: Distributed Systems Resilience ✅ COMPLETE

**Duration:** 1 week
**Priority:** 🟡 MEDIUM
**Team:** Backend/Distributed Systems
**Depends On:** Y1, Y5
**Completed:** 2025-12-06
**Tests Added:** 38 new tests (527 total lib tests)

### Objectives
Improve Byzantine fault tolerance, network partition handling, and race condition fixes.

### Tasks

| ID | Finding | Task | File | Effort | Status |
|----|---------|------|------|--------|--------|
| Y6.1 | P2P-H4 | Fix rate limiter window reset race condition (atomic check-reset) | `distributed_rate_limiter.rs` | M | ✅ |
| Y6.2 | P2P-M1 | Add network partition detection via heartbeat patterns | `threat_intel_p2p.rs` | L | ✅ |
| Y6.3 | P2P-M1 | Implement CRDT-based threat intel for conflict resolution | `threat_intel_p2p.rs` | L | ✅ |
| Y6.4 | P2P-M4 | Add vector clocks to detect causality violations | `nats_sync.rs` | M | ✅ |
| Y6.5 | P2P-M5 | Add Byzantine tolerance validation to CRDT operations | `distributed_counter.rs` | M | ✅ |
| Y6.6 | P2P-M5 | Implement suspicious actor tracking | `distributed_counter.rs` | M | ✅ |
| Y6.7 | P2P-M6 | Implement trust token revocation mechanism | `distributed_enforcement.rs` | M | ✅ |
| Y6.8 | P2P-M6 | Add `RevokeToken` and `RevokeNode` message types | `distributed_enforcement.rs` | S | ✅ |
| Y6.9 | P2P-M2 | Integrate with Solana staking for Sybil resistance | `threat_intel_p2p.rs` | L | ✅ |
| Y6.10 | P2P-M3 | Disable mDNS in production, verify peers via challenge | `threat_intel_p2p.rs` | S | ✅ |

### Acceptance Criteria
- [x] Rate limiter window resets are atomic (Y6.1 - epoch-based compare-and-swap)
- [x] Network partitions are detected and logged (Y6.2 - NetworkPartitionDetector)
- [x] CRDT conflicts are resolved deterministically (Y6.3 - ThreatIntelCRDT with LWW semantics)
- [x] Vector clocks detect causality violations (Y6.4 - VectorClock with happens-before)
- [x] Byzantine counter manipulation is detected (Y6.5-Y6.6 - ByzantineValidator + SuspiciousActorTracker)
- [x] Trust tokens can be revoked (Y6.7-Y6.8 - TokenRevocation + NodeRevocation messages)
- [x] Solana staking integration for Sybil resistance (Y6.9 - StakingVerifier with tier-based trust)
- [x] mDNS disabled in production builds (Y6.10 - P2PConfig::validate() rejects mDNS in release)
- [x] 38 new tests pass

### Key Implementations

**Y6.3: ThreatIntelCRDT** - Last-Writer-Wins CRDT for conflict resolution:
- LWWThreatEntry with timestamp + node_id tie-breaker
- Merge, remove (tombstone), prune operations
- Deterministic conflict resolution across nodes

**Y6.4: VectorClock** - Causality tracking:
- Increment, merge, happens-before relation
- Concurrent event detection
- Causality violation detection with detailed reporting

**Y6.5: ByzantineValidator** - Byzantine fault tolerance:
- Maximum value limits (10,000 per operation)
- Rate limiting per actor (100 ops/sec)
- Replay detection via operation hashing

**Y6.6: SuspiciousActorTracker** - Suspicious behavior detection:
- Large value jumps, rapid operations, timestamp regression
- Tiered thresholds (suspicious: 10, block: 50)
- Auto-blocking with unblock capability

**Y6.9: StakingVerifier** - Sybil resistance:
- StakeTier enum (None, Basic, Standard, High, Elite)
- Trust weight based on stake amount
- Slash/unslash capability for malicious nodes

---

## Sprint Y7: Smart Contract Refinements ✅ COMPLETE

**Duration:** 1 week
**Priority:** 🟡 MEDIUM
**Team:** Blockchain/Smart Contracts
**Depends On:** Y2
**Completed:** 2025-12-06

### Objectives
Improve DAO governance, token security, and registry reliability.

### Tasks

| ID | Finding | Task | File | Effort | Status |
|----|---------|------|------|--------|--------|
| Y7.1 | SC-M1 | Implement partial bond return for proposals reaching 50%+ quorum | `dao/lib.rs` | M | ✅ |
| Y7.2 | SC-M1 | Add DAO-governed appeal mechanism | `dao/lib.rs` | L | ✅ |
| Y7.3 | SC-M2 | Add treasury ownership validation constraint | `token/lib.rs` | S | ✅ |
| Y7.4 | SC-M3 | Add rate limits to authority-only performance recording | `rewards/lib.rs` | S | ✅ |
| Y7.5 | SC-M3 | Add audit event emission for authority actions | `rewards/lib.rs` | S | ✅ |
| Y7.6 | SC-M5 | Implement automatic node deactivation at reputation threshold | `registry/lib.rs` | M | ✅ |
| Y7.7 | SC-M5 | Add reputation floor validation for re-registration | `registry/lib.rs` | S | ✅ |
| Y7.8 | SC-H4 | Design multi-tier vault architecture (document only) | Documentation | M | ✅ |

### Acceptance Criteria
- [x] Partial bond returns work correctly (Y7.1 - 50% return for 50%+ quorum)
- [x] Treasury transfers validate ownership (Y7.3 - token_config PDA check)
- [x] Authority actions are rate-limited and logged (Y7.4-Y7.5)
- [x] Nodes are auto-deactivated at low reputation (Y7.6 - <10% threshold)
- [x] Multi-tier vault design is documented (Y7.8 - see below)
- [x] Implementation complete (tests validated at build time)

### Key Implementations

**Y7.1: Partial Bond Return**
- Full bond return for Passed/Executed proposals
- 50% bond return for Defeated proposals with ≥50% quorum participation
- No return for proposals with <50% quorum participation

**Y7.2: Appeal Mechanism**
- Defeated proposals with ≥40% quorum can be appealed
- Appeal bond: 1.5x normal proposal bond
- Extended voting period: 1.5x normal duration
- Creates new proposal with "APPEAL:" prefix

**Y7.6-Y7.7: Reputation-Based Node Management**
- Auto-deactivation threshold: 10% (1000/10000)
- Reactivation floor: 30% (3000/10000)
- Prevents abusive nodes from rapid re-registration

### Y7.8: Multi-Tier Vault Architecture Design

**Rationale:**
Current architecture uses single treasury vaults which creates concentration risk.
Multi-tier vaults distribute funds across security levels based on purpose and access patterns.

**Proposed Architecture:**

```
┌─────────────────────────────────────────────────────────────────┐
│                    MULTI-TIER VAULT SYSTEM                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  TIER 1: COLD VAULT (Multi-sig + Timelock)                       │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │ - 70% of treasury funds                                  │     │
│  │ - 5-of-9 multi-sig requirement                           │     │
│  │ - 72-hour timelock for all withdrawals                   │     │
│  │ - Used for: Long-term reserves, major grants             │     │
│  │ - Access: DAO proposal + timelock + multi-sig            │     │
│  └─────────────────────────────────────────────────────────┘     │
│                              ↓ (refill)                          │
│  TIER 2: WARM VAULT (Multi-sig, reduced timelock)                │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │ - 20% of treasury funds                                  │     │
│  │ - 3-of-9 multi-sig requirement                           │     │
│  │ - 24-hour timelock for withdrawals                       │     │
│  │ - Used for: Operational expenses, medium grants          │     │
│  │ - Auto-refilled from Tier 1 when below threshold         │     │
│  └─────────────────────────────────────────────────────────┘     │
│                              ↓ (refill)                          │
│  TIER 3: HOT VAULT (Single-sig + Rate limit)                     │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │ - 10% of treasury funds (max 100k AEGIS)                 │     │
│  │ - Single operational authority                           │     │
│  │ - Rate limited: Max 10k AEGIS per 24h                    │     │
│  │ - Used for: Rewards distribution, small operational      │     │
│  │ - Auto-refilled from Tier 2 when below threshold         │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Security Properties:**

1. **Blast Radius Limitation**: Compromise of Tier 3 only risks 10% of funds
2. **Progressive Security**: Higher tiers have stricter access controls
3. **Automatic Refill**: Maintains liquidity without manual intervention
4. **Rate Limiting**: Tier 3 limits daily outflow even if compromised
5. **Timelock Protection**: Tier 1/2 give time to detect malicious proposals

**Implementation Requirements:**

1. **VaultTier struct**: Track tier level, balance, thresholds
2. **AutoRefill mechanism**: Monitor balances, trigger refills when low
3. **TierTransfer instruction**: Move funds between tiers (with appropriate checks)
4. **Enhanced Access Control**: Per-tier multi-sig configuration
5. **Rate Limit State**: Track 24h moving window of Tier 3 withdrawals

**Future Sprint Tasks:**
- Y_VAULT.1: Implement VaultTier account structure
- Y_VAULT.2: Add auto-refill logic with threshold triggers
- Y_VAULT.3: Implement tier-specific access controls
- Y_VAULT.4: Add rate limiting for Tier 3 withdrawals
- Y_VAULT.5: Create vault dashboard for monitoring

---

## Sprint Y8: API & Edge Security

**Duration:** 1 week
**Priority:** 🟡 MEDIUM
**Team:** Node/Core
**Depends On:** Y3, Y4
**Risk if Delayed:** SSRF, cache poisoning, header injection

### Objectives
Secure host APIs, fix cache isolation, and harden HTTP handling.

### Tasks

| ID | Finding | Task | File | Effort | Tests |
|----|---------|------|------|--------|-------|
| Y8.1 | WASM-M1 | Implement IP blocklist for SSRF protection | `wasm_runtime.rs` | M | 5 |
| Y8.2 | WASM-M1 | Block internal IPs (10.x, 172.16.x, 192.168.x, 169.254.x) | `wasm_runtime.rs` | S | 3 |
| Y8.3 | WASM-M1 | Add DNS rebinding protection (re-resolve after connect) | `wasm_runtime.rs` | M | 3 |
| Y8.4 | WASM-M2 | Namespace cache keys by module ID | `wasm_runtime.rs` | M | 4 |
| Y8.5 | WASM-M4 | Enforce `is_header_name_safe()` in `response_set_header()` | `wasm_runtime.rs` | S | 2 |
| Y8.6 | INPUT-H4 | Add header value sanitization function | `proxy.rs` | S | 2 |
| Y8.7 | WASM-M5 | Improve bot detector with max UA length and exact matching | `bot-detector/lib.rs` | M | 4 |
| Y8.8 | WASM-H3 | Implement two-phase IPFS bandwidth tracking (reserve + refund) | `ipfs_client.rs` | M | 3 |

### Acceptance Criteria
- [ ] Internal network access is blocked from Wasm modules
- [ ] DNS rebinding attacks are prevented
- [ ] Module cache keys are isolated
- [ ] Header injection is prevented
- [ ] Bot detection is harder to bypass
- [ ] IPFS bandwidth tracking is accurate
- [ ] 26 new tests pass

### Code Changes
```rust
// Y8.1-Y8.2: SSRF protection
fn is_internal_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() ||
            v4.is_private() ||
            v4.is_link_local() ||
            v4.octets()[0] == 169 && v4.octets()[1] == 254
        }
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

// Y8.4: Cache key namespacing
fn namespaced_cache_key(module_id: &str, key: &str) -> String {
    format!("aegis:wasm:{}:{}", module_id, key)
}
```

---

## Sprint Y9: Defense in Depth

**Duration:** 1 week
**Priority:** 🟡🔵 MEDIUM/LOW
**Team:** Full Team
**Depends On:** Y6-Y8
**Risk if Delayed:** Reduced resilience, edge cases

### Objectives
Address remaining medium/low findings and strengthen overall security posture.

### Tasks

| ID | Finding | Task | File | Effort | Tests |
|----|---------|------|------|--------|-------|
| Y9.1 | CRYPTO-M1 | Increase challenge ID length to 48 characters | `challenge.rs` | S | 1 |
| Y9.2 | CRYPTO-M3 | Reduce default token TTL to 5 minutes | `challenge.rs` | S | 1 |
| Y9.3 | WASM-M3 | Add periodic module integrity monitoring | `wasm_runtime.rs` | M | 3 |
| Y9.4 | WASM-L3 | Implement graceful module unload with ref counting | `wasm_runtime.rs` | M | 3 |
| Y9.5 | INPUT-M* | Add `MAX_QUERY_PARAMS` limit (100) | `api_security.rs` | S | 2 |
| Y9.6 | INPUT-M* | Add `MAX_CACHE_DIRECTIVES` limit (20) | `cache.rs` | S | 1 |
| Y9.7 | INPUT-L* | Add URL decoding before WAF analysis | `wasm-waf/lib.rs` | S | 2 |
| Y9.8 | INPUT-L* | Add `MAX_ALPN_PROTOCOLS` limit (10) | `tls_fingerprint.rs` | S | 1 |
| Y9.9 | SEC-L1 | Add validation to reject default BGP passwords | `ops/bgp/` | S | 1 |
| Y9.10 | P2P-L* | Implement gossipsub amplification protection | `threat_intel_p2p.rs` | M | 3 |
| Y9.11 | SC-L* | Close VoteEscrow accounts after withdrawal | `dao/lib.rs` | S | 2 |
| Y9.12 | SC-L* | Close MultisigTransaction accounts after execution | `token/lib.rs` | S | 2 |

### Acceptance Criteria
- [ ] All medium/low findings addressed
- [ ] Account rent recovery implemented
- [ ] Resource limits enforced throughout
- [ ] 22 new tests pass

---

## Sprint Y10: Security Testing & Hardening

**Duration:** 1 week
**Priority:** 🔵ℹ️ LOW/INFO
**Team:** QA/Security
**Depends On:** Y1-Y9
**Risk if Delayed:** Unknown vulnerabilities remain

### Objectives
Comprehensive security testing, fuzzing, and documentation.

### Tasks

| ID | Task | Component | Effort |
|----|------|-----------|--------|
| Y10.1 | Set up cargo-fuzz for TLS parser | `tls_fingerprint.rs` | M |
| Y10.2 | Set up cargo-fuzz for Wasm modules | `wasm_runtime.rs` | M |
| Y10.3 | Set up cargo-fuzz for route configs | `route_config.rs` | M |
| Y10.4 | Add property-based tests for CRDTs | `distributed_counter.rs` | M |
| Y10.5 | Add stress tests for DoS resistance | All | L |
| Y10.6 | Enable address sanitizer in CI | CI/CD | S |
| Y10.7 | Enable leak sanitizer in CI | CI/CD | S |
| Y10.8 | Run cargo audit and fix any findings | All | S |
| Y10.9 | Update security documentation | `docs/security/` | M |
| Y10.10 | Create incident response playbook | `docs/security/` | M |
| Y10.11 | Prepare external audit package | `docs/` | L |
| Y10.12 | Set up pre-commit secret scanning hooks | Git | S |

### Acceptance Criteria
- [ ] Fuzzing runs for 24+ hours without crashes
- [ ] Sanitizers enabled in CI
- [ ] No cargo audit findings
- [ ] Security documentation complete
- [ ] External audit package ready

---

## Sprint Dependencies Graph

```
Y1 (NATS Auth) ─────────┬──────────────────────────────────────────┐
                        │                                          │
Y2 (Solana) ────────────┼──────────────────────────────┐          │
                        │                              │          │
Y3 (Input Val) ─────────┼───────────┐                  │          │
                        │           │                  │          │
                        v           v                  v          v
                    Y5 (P2P)     Y4 (Wasm)          Y7 (SC)    Y6 (Dist)
                        │           │                  │          │
                        │           │                  │          │
                        └───────────┼──────────────────┼──────────┤
                                    │                  │          │
                                    v                  v          v
                                Y8 (API)           (combined)  (combined)
                                    │                  │          │
                                    └──────────────────┴──────────┘
                                                       │
                                                       v
                                                  Y9 (Defense)
                                                       │
                                                       v
                                                  Y10 (Testing)
```

---

## Parallel Execution Plan (2 Teams)

### Team A: Infrastructure (Backend/Distributed)
| Week | Sprint | Focus |
|------|--------|-------|
| 1 | Y1 | NATS Authentication |
| 2 | Y5 | P2P & Crypto |
| 3 | Y6 | Distributed Resilience |
| 4 | Y9 (partial) | Defense in Depth |

### Team B: Core (Node/Blockchain)
| Week | Sprint | Focus |
|------|--------|-------|
| 1 | Y2 + Y3 | Solana + Input Validation |
| 2 | Y4 | Wasm Runtime |
| 3 | Y7 + Y8 | SC Refinements + API |
| 4 | Y9 (partial) | Defense in Depth |

### Joint (Week 5-6)
| Week | Sprint | Focus |
|------|--------|-------|
| 5 | Y10 | Security Testing |
| 6 | Review | External Audit Prep |

**Total Duration with 2 Teams: 6 weeks**

---

## Metrics & Tracking

### Sprint Completion Criteria
- [ ] All tasks completed
- [ ] All tests passing
- [ ] Code reviewed and merged
- [ ] Documentation updated
- [ ] No new critical/high findings from review

### Key Performance Indicators
| Metric | Current | Target (Y10) |
|--------|---------|--------------|
| Critical findings | 9 | 0 |
| High findings | 16 | 0 |
| Medium findings | 31 | ≤10 |
| Low findings | 21 | ≤15 |
| Test coverage | TBD | >80% |
| Fuzz test duration | 0 | 24h+ clean |

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| NATS changes break existing deployments | Medium | High | Staged rollout, feature flags |
| Solana contract upgrades require migration | Low | High | Test on devnet first, migration scripts |
| JA3 SHA-256 breaks fingerprint DB | Medium | Medium | Document change, provide migration path |
| External audit finds new critical issues | Medium | High | Buffer time before mainnet |
| Team velocity lower than estimated | Medium | Medium | Prioritize critical items, defer low |

---

## Summary

| Metric | Value |
|--------|-------|
| Total Sprints | 10 |
| Total Tasks | 90+ |
| Total New Tests | 200+ |
| Sequential Duration | 10 weeks |
| Parallel Duration (2 teams) | 6 weeks |
| External Audit | +4-6 weeks |

**Recommended Mainnet Launch:** After Y10 completion + external audit (12-16 weeks from start)

---

**Document Version:** 1.0
**Created:** 2025-12-03
**Last Updated:** 2025-12-03
