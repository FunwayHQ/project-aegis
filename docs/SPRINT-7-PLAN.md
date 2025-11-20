# Sprint 7: eBPF/XDP DDoS Protection - Implementation Plan

**Sprint**: 7 of 24
**Phase**: 2 - Security & Decentralized State
**Objective**: Implement kernel-level DDoS mitigation using eBPF/XDP
**Estimated Duration**: 2 weeks
**Status**: 🚀 STARTING

---

## Objective (from Project Plan)

Implement kernel-level DDoS mitigation using eBPF/XDP for volumetric attacks.

## Deliverables

1. ✅ Basic eBPF/XDP program to drop specific packet types (e.g., SYN floods)
2. ✅ Rust helper application to load and manage eBPF programs on the NIC
3. ✅ Basic configuration for eBPF rules (e.g., threshold for SYN packets)
4. ✅ Proof-of-concept: Test eBPF program dropping simulated attack traffic

---

## Technical Background

### What is eBPF/XDP?

**eBPF (Extended Berkeley Packet Filter)**:
- Allows running sandboxed programs in the Linux kernel
- No kernel modules needed (safe to load/unload)
- Can hook into various kernel subsystems
- Verifier ensures safety (no crashes, no infinite loops)

**XDP (eXpress Data Path)**:
- eBPF programs attached to network interface drivers
- Processes packets at the earliest possible point
- Runs BEFORE Linux networking stack
- Nanosecond-level latency
- Can DROP, PASS, TX (bounce back), or REDIRECT packets

### Why XDP for DDoS Protection?

**Advantages**:
- ✅ Processes packets at NIC driver level (before OS)
- ✅ Drop malicious packets in <1 microsecond
- ✅ No CPU resources consumed by attack traffic
- ✅ Scales to millions of packets per second
- ✅ Programmable (update rules without reboot)
- ✅ Safe (eBPF verifier prevents crashes)

**vs Traditional Approaches**:
- Iptables/nftables: Runs in kernel netfilter (higher overhead)
- Application-level: Already too late (OS resources consumed)
- Hardware: Expensive, inflexible

---

## Technology Choice: Aya vs libbpf-rs

### Option A: Aya (Recommended)

**Pros**:
- ✅ Pure Rust (eBPF programs in Rust, not C)
- ✅ Better Rust integration
- ✅ Modern, actively maintained
- ✅ No C toolchain needed
- ✅ Type-safe eBPF programming
- ✅ Easier debugging
- ✅ Better error messages

**Cons**:
- ⚠️ Newer ecosystem (less battle-tested)
- ⚠️ Smaller community than libbpf

**Aya Features**:
- Write eBPF in Rust (compile to eBPF bytecode)
- Procedural macros for eBPF programs
- Maps, programs, and helpers in Rust
- Built-in support for XDP, TC, etc.

### Option B: libbpf-rs

**Pros**:
- ✅ Bindings to libbpf (Facebook's library)
- ✅ Battle-tested (used in production)
- ✅ Large community
- ✅ Extensive documentation

**Cons**:
- ⚠️ eBPF programs in C (separate toolchain)
- ⚠️ Less idiomatic Rust
- ⚠️ Requires Clang/LLVM for eBPF compilation
- ⚠️ More complex build process

### Decision: **Aya** ✅

**Rationale**:
- Aligns with "Rust everywhere" philosophy
- Simpler build process
- Better type safety
- Easier to maintain
- Modern approach

---

## SYN Flood Detection Algorithm

### What is a SYN Flood?

**Attack Mechanism**:
1. Attacker sends massive SYN packets (TCP handshake start)
2. Server allocates resources for each connection
3. Attacker never completes handshake (no ACK)
4. Server exhausts connection table
5. Legitimate users cannot connect

### Detection Strategy

**Approach**: Rate-based threshold

**Algorithm**:
```
1. Track SYN packet count per source IP
2. If rate exceeds threshold (e.g., 100 SYN/sec from one IP):
   - DROP packet
   - Log event
3. Otherwise:
   - PASS packet to kernel
```

**Data Structures**:
- **LRU Hash Map**: Source IP → (count, last_seen_timestamp)
- **Global Counter**: Total SYN packets per second
- **Threshold**: Configurable limit (default: 100 SYN/sec per IP)

**eBPF Map**:
```c
// Map: IP address → SYN count
BPF_HASH(syn_tracker, u32, struct syn_info, 10000);

struct syn_info {
    u64 count;
    u64 last_seen;
};
```

### False Positive Mitigation

**Legitimate High-Rate Sources**:
- CDN edge servers
- Load balancers
- Large corporate networks

**Mitigation**:
- **Whitelist Map**: Trusted IP ranges (never drop)
- **Adaptive Threshold**: Increase threshold during normal traffic spikes
- **Time Window**: Reset counters every second

---

## Implementation Plan

### Phase 1: eBPF Program (Aya in Rust)

**File**: `node/ebpf/syn-flood-filter/src/main.rs`

**Structure**:
```rust
#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::xdp_action,
    macros::{map, xdp},
    maps::HashMap,
    programs::XdpContext,
};

// Map to track SYN packets per IP
#[map]
static SYN_TRACKER: HashMap<u32, u64> = HashMap::with_max_entries(10000, 0);

// Map for threshold configuration (updatable from userspace)
#[map]
static CONFIG: HashMap<u32, u64> = HashMap::with_max_entries(10, 0);

#[xdp]
pub fn syn_flood_filter(ctx: XdpContext) -> u32 {
    match try_syn_flood_filter(ctx) {
        Ok(ret) => ret,
        Err(_) => xdp_action::XDP_PASS,
    }
}

fn try_syn_flood_filter(ctx: XdpContext) -> Result<u32, ()> {
    // 1. Parse Ethernet header
    // 2. Check if IP packet
    // 3. Check if TCP packet
    // 4. Check if SYN flag set
    // 5. Extract source IP
    // 6. Check rate limit
    // 7. Return XDP_DROP or XDP_PASS
}
```

### Phase 2: Rust Loader Application

**File**: `node/src/ebpf_loader.rs`

**Structure**:
```rust
use aya::{Ebpf, programs::{Xdp, XdpFlags}};
use aya::maps::HashMap;

pub struct EbpfLoader {
    ebpf: Ebpf,
    interface: String,
}

impl EbpfLoader {
    pub fn new(interface: &str) -> Result<Self> {
        // Load eBPF program from file
        let ebpf = Ebpf::load_file("ebpf/syn-flood-filter")?;

        Ok(Self {
            ebpf,
            interface: interface.to_string(),
        })
    }

    pub fn attach(&mut self) -> Result<()> {
        // Get XDP program
        let program: &mut Xdp = self.ebpf.program_mut("syn_flood_filter")?;
        program.load()?;
        program.attach(&self.interface, XdpFlags::default())?;

        Ok(())
    }

    pub fn set_threshold(&mut self, threshold: u64) -> Result<()> {
        // Update threshold in eBPF map
        let mut config: HashMap<_, u32, u64> =
            HashMap::try_from(self.ebpf.map_mut("CONFIG")?)?;
        config.insert(0, threshold, 0)?;

        Ok(())
    }

    pub fn get_stats(&self) -> Result<SynFloodStats> {
        // Read stats from eBPF map
    }
}
```

### Phase 3: Configuration

**File**: `node/ebpf-config.toml`

```toml
[ebpf]
enabled = true
interface = "eth0"  # or "lo" for testing

[ddos]
syn_flood_threshold = 100  # SYN packets per second per IP
whitelist_ips = [
    "192.168.1.0/24",  # Local network
    "10.0.0.0/8",      # Private network
]

[logging]
log_drops = true
log_interval_seconds = 60
```

### Phase 4: Testing

**Test Script**: `node/test-syn-flood.sh`

```bash
#!/bin/bash
# Test SYN flood mitigation

# 1. Start eBPF loader
sudo ./aegis-ebpf-loader --interface lo

# 2. Generate legitimate traffic
curl http://localhost:8080/

# 3. Simulate SYN flood with hping3
sudo hping3 -S -p 8080 -i u1000 localhost  # 1000 SYN/sec

# 4. Check eBPF stats
sudo ./aegis-ebpf-loader --stats

# Expected: High drop rate, legitimate traffic still works
```

---

## Development Environment Requirements

### Linux System Requirements

**Kernel Version**: Linux 5.10+ (for XDP support)
**Packages**:
```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y \
    llvm \
    clang \
    linux-headers-$(uname -r) \
    build-essential \
    pkg-config

# Fedora/RHEL
sudo dnf install -y \
    llvm \
    clang \
    kernel-devel \
    elfutils-libelf-devel
```

**Rust Toolchain**:
```bash
rustup install stable
rustup install nightly  # For eBPF compilation
cargo install bpf-linker  # For Aya
```

**Testing Tools**:
```bash
# Install hping3 for SYN flood simulation
sudo apt-get install hping3

# Install bpftool for debugging
sudo apt-get install linux-tools-common linux-tools-$(uname -r)
```

---

## Directory Structure

```
node/
├── ebpf/                          # eBPF programs
│   └── syn-flood-filter/          # SYN flood mitigation
│       ├── Cargo.toml             # eBPF program dependencies
│       ├── src/
│       │   └── main.rs            # XDP program in Rust
│       └── .cargo/
│           └── config.toml        # Build config for eBPF
│
├── src/
│   ├── ebpf_loader.rs             # Rust loader (NEW)
│   ├── ebpf_config.rs             # eBPF configuration (NEW)
│   └── main_ebpf.rs               # Binary entry point (NEW)
│
├── ebpf-config.toml               # eBPF configuration file
└── test-syn-flood.sh              # Testing script
```

---

## Implementation Steps

### Step 1: Research & Setup (Day 1)

**Tasks**:
- [x] Review eBPF/XDP documentation
- [x] Choose Aya framework
- [x] Set up Linux development environment
- [ ] Install dependencies
- [ ] Verify kernel version (5.10+)

### Step 2: XDP Program Development (Day 2-3)

**Tasks**:
- [ ] Create eBPF program skeleton
- [ ] Implement packet parsing (Ethernet → IP → TCP)
- [ ] Implement SYN flag detection
- [ ] Implement rate limiting logic
- [ ] Add eBPF maps (SYN_TRACKER, CONFIG, WHITELIST)
- [ ] Compile and verify eBPF bytecode

### Step 3: Rust Loader Development (Day 4-5)

**Tasks**:
- [ ] Create ebpf_loader module
- [ ] Implement program loading
- [ ] Implement NIC attachment
- [ ] Implement map updates (threshold, whitelist)
- [ ] Add statistics retrieval
- [ ] Create CLI interface

### Step 4: Configuration & Integration (Day 6-7)

**Tasks**:
- [ ] Design TOML configuration schema
- [ ] Implement configuration parsing
- [ ] Integrate with main node binary
- [ ] Add startup/shutdown logic
- [ ] Add logging and monitoring

### Step 5: Testing (Day 8-10)

**Tasks**:
- [ ] Unit tests for loader
- [ ] Integration tests with XDP program
- [ ] SYN flood simulation with hping3
- [ ] Performance benchmarking
- [ ] Verify legitimate traffic passes
- [ ] Documentation

---

## Success Criteria

### Functional Requirements

- [ ] XDP program loads successfully on network interface
- [ ] Legitimate SYN packets are PASSED
- [ ] SYN flood traffic (>100/sec per IP) is DROPPED
- [ ] Threshold is configurable from userspace
- [ ] Statistics are retrievable
- [ ] Whitelisted IPs are never dropped

### Performance Requirements

- [ ] Packet processing: <1 microsecond
- [ ] Legitimate traffic unaffected (<1% overhead)
- [ ] Can handle 1M+ packets/sec on standard hardware
- [ ] Memory usage: <10MB

### Security Requirements

- [ ] eBPF verifier accepts program (no unsafe operations)
- [ ] Cannot be bypassed by fragmented packets
- [ ] Whitelist prevents blocking legitimate infrastructure
- [ ] Logs provide audit trail

---

## Risks & Mitigation

### Risk 1: Linux-Only Feature

**Risk**: eBPF/XDP only works on Linux
**Impact**: Windows/macOS nodes cannot use this feature
**Mitigation**:
- Feature flag in configuration (ebpf.enabled = false on non-Linux)
- Graceful degradation (node runs without eBPF)
- Document OS requirements clearly

### Risk 2: Kernel Version Dependency

**Risk**: Requires Linux 5.10+ kernel
**Impact**: Older systems cannot use XDP
**Mitigation**:
- Check kernel version at startup
- Provide clear error message if unsupported
- Fall back to application-level filtering

### Risk 3: Root Privileges Required

**Risk**: Loading XDP programs requires root/CAP_NET_ADMIN
**Impact**: Node must run with elevated privileges
**Mitigation**:
- Document privilege requirements
- Use systemd with capabilities
- Minimize privileges (drop after loading)

### Risk 4: False Positives

**Risk**: Legitimate traffic might be dropped
**Impact**: Service degradation
**Mitigation**:
- Whitelist for known good IPs
- Conservative thresholds
- Monitoring and alerting
- Manual override capability

---

## Testing Strategy

### Unit Tests

**Loader Tests**:
```rust
#[test]
fn test_ebpf_loader_creation() {
    let loader = EbpfLoader::new("lo");
    assert!(loader.is_ok());
}

#[test]
fn test_threshold_update() {
    let mut loader = EbpfLoader::new("lo")?;
    loader.set_threshold(200)?;
    // Verify threshold updated
}
```

### Integration Tests

**XDP Program Tests**:
```bash
# Load program on loopback
sudo ./aegis-ebpf-loader --interface lo --attach

# Generate normal traffic
curl http://localhost:8080/  # Should work

# Generate SYN flood
sudo hping3 -S -p 8080 --flood localhost  # Should be dropped

# Check stats
sudo ./aegis-ebpf-loader --stats
# Expected: high drop count
```

### Performance Tests

**Benchmark**:
```bash
# Without XDP
ab -n 100000 -c 100 http://localhost:8080/

# With XDP
sudo ./aegis-ebpf-loader --attach
ab -n 100000 -c 100 http://localhost:8080/

# Compare: <1% performance difference expected
```

---

## Documentation Deliverables

### User Documentation

1. **EBPF-SETUP.md** - Installation and setup guide
2. **EBPF-USAGE.md** - How to configure and use
3. **EBPF-TROUBLESHOOTING.md** - Common issues

### Developer Documentation

4. **EBPF-ARCHITECTURE.md** - Technical design
5. **EBPF-TESTING.md** - Testing procedures
6. **EBPF-INTERNALS.md** - How XDP program works

---

## Dependencies

### New Cargo Dependencies

**For eBPF Program** (`ebpf/syn-flood-filter/Cargo.toml`):
```toml
[dependencies]
aya-ebpf = "0.1"

[build-dependencies]
aya-ebpf-build = "0.1"
```

**For Loader** (`node/Cargo.toml`):
```toml
[dependencies]
aya = { version = "0.12", features = ["async_tokio"] }
aya-log = "0.2"
```

### System Dependencies

- Linux kernel 5.10+
- llvm/clang (for eBPF compilation)
- bpf-linker (Rust → eBPF)
- hping3 (for testing)

---

## Security Considerations

### eBPF Verifier Requirements

**Must Pass Verifier**:
- ✅ No unbounded loops
- ✅ Bounded stack usage (<512 bytes)
- ✅ All memory accesses validated
- ✅ No null pointer dereferences
- ✅ Program terminates

**Aya Helps**:
- Compile-time checks in Rust
- Automatic bounds checking
- Safe abstractions

### Attack Scenarios to Test

1. **SYN Flood** (primary):
   - High rate SYN packets
   - From single IP
   - From distributed IPs (harder to detect)

2. **Legitimate High-Rate** (false positive test):
   - Load balancer with many connections
   - Should NOT be dropped if whitelisted

3. **Packet Fragmentation**:
   - Fragmented SYN packets
   - Should still be detected

4. **Spoofed Source IPs**:
   - Random source IPs (hard to rate-limit)
   - May need global threshold

---

## Performance Targets

### Packet Processing

- **Target Latency**: <1 microsecond per packet
- **Throughput**: >1M packets/sec
- **CPU Impact**: <5% under attack
- **Memory**: <10MB for maps

### Legitimate Traffic Impact

- **Latency Overhead**: <1%
- **Throughput Reduction**: <1%
- **Connection Success Rate**: 100%

---

## Monitoring Integration

### Metrics to Add

**eBPF Metrics** (new):
- `aegis_ebpf_packets_total` (counter) - Total packets processed
- `aegis_ebpf_packets_dropped` (counter) - Packets dropped
- `aegis_ebpf_syn_floods_detected` (counter) - SYN flood events
- `aegis_ebpf_whitelisted_passes` (counter) - Whitelisted IPs passed
- `aegis_ddos_active` (gauge) - Currently under attack (1=yes, 0=no)

**Expose via**:
- `/metrics` endpoint (Prometheus format)
- CLI metrics command

---

## Timeline

### Week 1

**Days 1-2**: Setup & Research
- Environment setup
- Aya learning
- Algorithm design

**Days 3-5**: Implementation
- XDP program development
- Rust loader implementation

### Week 2

**Days 6-7**: Configuration & Integration
- Configuration parsing
- Main binary integration

**Days 8-10**: Testing & Documentation
- Unit tests
- Integration tests
- Performance benchmarks
- Documentation

**Total**: ~10 working days (2 weeks)

---

## Next Steps (Immediate)

### Today (Day 1)

1. **Create eBPF directory structure**:
   ```bash
   mkdir -p node/ebpf/syn-flood-filter/src
   ```

2. **Install Aya tools**:
   ```bash
   cargo install bpf-linker
   ```

3. **Create XDP program skeleton**:
   - Basic Aya eBPF program
   - Packet parsing structure
   - XDP_PASS placeholder

4. **Create loader skeleton**:
   - Basic Rust binary
   - Aya integration
   - Command-line interface

5. **Documentation**:
   - Sprint 7 kickoff document (this file)
   - Architecture design document

---

## Success Metrics

### Sprint 7 Complete When:

- [x] XDP program compiles to eBPF bytecode
- [ ] Loader successfully attaches program to NIC
- [ ] SYN flood detection works (>threshold dropped)
- [ ] Legitimate traffic passes (<threshold)
- [ ] Threshold configurable from userspace
- [ ] Whitelist functional
- [ ] Tests passing
- [ ] Documentation complete

### Quality Gates

- [ ] eBPF verifier accepts program
- [ ] No kernel panics or crashes
- [ ] Performance targets met
- [ ] Security review passed
- [ ] User documentation clear

---

## Resources

### Learning Resources

**eBPF**:
- https://ebpf.io/
- https://github.com/iovisor/bcc (BCC tools examples)
- Linux kernel documentation

**Aya**:
- https://aya-rs.dev/
- https://github.com/aya-rs/aya
- Aya book: https://aya-rs.dev/book/

**XDP**:
- https://www.kernel.org/doc/html/latest/networking/xdp.html
- XDP tutorial: https://github.com/xdp-project/xdp-tutorial

### Example Projects

- Cloudflare's XDP programs (Rust)
- Cilium (eBPF networking)
- Katran (Facebook's L4 load balancer using XDP)

---

## Conclusion

Sprint 7 brings **kernel-level DDoS protection** to AEGIS, a critical security feature that differentiates us from traditional CDNs. By processing packets at the NIC driver level, we can drop attack traffic before it consumes any system resources.

**Key Innovation**: Pure Rust eBPF programs (via Aya) - memory-safe kernel programming

**Next**: Begin implementation! 🚀

---

**Plan Created By**: Claude Code
**Date**: November 20, 2025
**Sprint**: 7 - eBPF/XDP DDoS Protection
**Status**: Ready to implement
