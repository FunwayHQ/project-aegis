# Linux-Specific Security Tasks

**Sprint:** Y10 (Deferred)
**Created:** 2025-12-06
**Reason:** These tasks require Linux kernel features (eBPF, seccomp) not available on macOS

## Overview

The following security tasks from Sprint Y10 require a Linux environment and were deferred:

| Task | Description | Priority | Estimated Effort |
|------|-------------|----------|------------------|
| Y10.6 | eBPF verifier hardening | High | 2-3 days |
| Y10.7 | Syscall filtering for Wasm | Medium | 1-2 days |

---

## Y10.6: eBPF Verifier Hardening

### Background
The eBPF/XDP programs in `node/src/ebpf/` provide kernel-level DDoS protection. These programs need additional security hardening.

### Current State
- Basic eBPF loader exists (`node/src/ebpf_loader.rs`)
- IPv4 blocklist implemented
- IPv6 blocklist added in Sprint Y29

### Tasks

#### 1. Map Size Limits
```rust
// File: node/src/ebpf_loader.rs
// Add maximum entry limits to prevent memory exhaustion

const MAX_BLOCKLIST_V4_ENTRIES: u32 = 100_000;
const MAX_BLOCKLIST_V6_ENTRIES: u32 = 50_000;

// Enforce limits when adding entries
pub fn add_to_blocklist(&self, ip: IpAddr) -> Result<()> {
    match ip {
        IpAddr::V4(_) => {
            if self.blocklist_v4_count() >= MAX_BLOCKLIST_V4_ENTRIES {
                return Err(anyhow!("IPv4 blocklist full"));
            }
        }
        IpAddr::V6(_) => {
            if self.blocklist_v6_count() >= MAX_BLOCKLIST_V6_ENTRIES {
                return Err(anyhow!("IPv6 blocklist full"));
            }
        }
    }
    // ... add entry
}
```

#### 2. Program Verification
```rust
// Verify eBPF program before loading
pub fn verify_ebpf_program(program_bytes: &[u8]) -> Result<()> {
    // Check program size
    if program_bytes.len() > MAX_EBPF_PROGRAM_SIZE {
        return Err(anyhow!("eBPF program too large"));
    }

    // Verify ELF header
    if &program_bytes[0..4] != b"\x7fELF" {
        return Err(anyhow!("Invalid ELF header"));
    }

    // Use kernel verifier (via libbpf)
    // This happens automatically on load, but we can pre-check
    Ok(())
}
```

#### 3. Resource Limits
```rust
// Set resource limits for eBPF operations
use rlimit::{Resource, setrlimit};

pub fn configure_ebpf_limits() -> Result<()> {
    // Increase locked memory limit for eBPF maps
    setrlimit(Resource::MEMLOCK, 256 * 1024 * 1024, 256 * 1024 * 1024)?;

    // Set CPU time limit for eBPF programs (kernel enforced)
    // BPF programs have built-in instruction limits

    Ok(())
}
```

#### 4. Audit Logging
```rust
// Log all eBPF operations for audit trail
pub fn log_ebpf_operation(op: &str, details: &str) {
    info!(
        target: "ebpf_audit",
        operation = op,
        details = details,
        timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "eBPF operation"
    );
}
```

### Testing
```bash
# Run on Linux only
cargo test ebpf --features ebpf -- --ignored

# Test blocklist limits
cargo test test_blocklist_limits --features ebpf

# Verify program loading
cargo test test_ebpf_program_verification --features ebpf
```

---

## Y10.7: Syscall Filtering for Wasm

### Background
Wasm edge functions should be restricted from making dangerous syscalls even if they escape the Wasm sandbox.

### Current State
- Wasm runtime uses wasmtime with fuel limits
- No syscall filtering (seccomp) applied

### Tasks

#### 1. Seccomp Profile for Wasm Workers
```rust
// File: node/src/wasm_sandbox.rs (new file)

use seccompiler::{BpfMap, SeccompAction, SeccompFilter, SeccompRule};

/// Create seccomp filter for Wasm execution threads
pub fn create_wasm_seccomp_filter() -> Result<SeccompFilter> {
    let mut rules = BpfMap::new();

    // Allow safe syscalls
    let allowed_syscalls = [
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_close,
        libc::SYS_fstat,
        libc::SYS_mmap,
        libc::SYS_mprotect,
        libc::SYS_munmap,
        libc::SYS_brk,
        libc::SYS_futex,
        libc::SYS_clock_gettime,
        libc::SYS_getrandom,
        libc::SYS_exit,
        libc::SYS_exit_group,
    ];

    for syscall in allowed_syscalls {
        rules.insert(syscall, vec![SeccompRule::new(vec![])?]);
    }

    // Block dangerous syscalls
    let blocked_syscalls = [
        libc::SYS_execve,      // No process execution
        libc::SYS_execveat,
        libc::SYS_fork,        // No forking
        libc::SYS_vfork,
        libc::SYS_clone,       // No thread creation outside runtime
        libc::SYS_ptrace,      // No debugging
        libc::SYS_mount,       // No filesystem mounting
        libc::SYS_umount2,
        libc::SYS_chroot,      // No chroot
        libc::SYS_pivot_root,
        libc::SYS_setuid,      // No privilege changes
        libc::SYS_setgid,
        libc::SYS_socket,      // No raw sockets (use host API)
        libc::SYS_bind,
        libc::SYS_listen,
        libc::SYS_accept,
    ];

    SeccompFilter::new(
        rules,
        SeccompAction::Errno(libc::EPERM as u32), // Default: deny with EPERM
        SeccompAction::Allow,                       // Matched rules: allow
        std::env::consts::ARCH.parse()?,
    )
}
```

#### 2. Apply Filter Before Wasm Execution
```rust
// In wasm_runtime.rs, before executing Wasm

#[cfg(target_os = "linux")]
fn apply_seccomp_filter() -> Result<()> {
    use seccompiler::apply_filter;

    let filter = create_wasm_seccomp_filter()?;
    apply_filter(&filter)?;

    info!("Seccomp filter applied to Wasm worker thread");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_seccomp_filter() -> Result<()> {
    // No-op on non-Linux
    Ok(())
}
```

#### 3. Namespace Isolation (Optional, Advanced)
```rust
// Additional isolation using Linux namespaces
use nix::sched::{unshare, CloneFlags};
use nix::unistd::{setuid, setgid, Uid, Gid};

pub fn isolate_wasm_worker() -> Result<()> {
    // Create new user namespace (for unprivileged containers)
    unshare(CloneFlags::CLONE_NEWUSER)?;

    // Create new network namespace (no network access)
    unshare(CloneFlags::CLONE_NEWNET)?;

    // Create new mount namespace
    unshare(CloneFlags::CLONE_NEWNS)?;

    // Drop to nobody user
    setgid(Gid::from_raw(65534))?;
    setuid(Uid::from_raw(65534))?;

    Ok(())
}
```

### Dependencies to Add
```toml
# Cargo.toml
[target.'cfg(target_os = "linux")'.dependencies]
seccompiler = "0.3"
nix = { version = "0.27", features = ["sched", "user"] }
```

### Testing
```bash
# Test seccomp filter creation
cargo test test_seccomp_filter_creation --features seccomp

# Test blocked syscalls
cargo test test_blocked_syscalls --features seccomp

# Integration test with Wasm execution
cargo test test_wasm_with_seccomp --features seccomp
```

---

## Implementation Instructions

### Environment Setup
```bash
# Recommended: Use a Linux VM or container
docker run -it --privileged ubuntu:22.04

# Install dependencies
apt-get update
apt-get install -y build-essential clang llvm libelf-dev linux-headers-$(uname -r)

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and build
git clone <repo>
cd project-aegis/node
cargo build --features ebpf,seccomp
```

### Testing Checklist

#### Y10.6 Tests
- [ ] Blocklist size limits enforced
- [ ] Invalid eBPF programs rejected
- [ ] Resource limits applied
- [ ] Audit logging working
- [ ] IPv4 and IPv6 blocklists functional

#### Y10.7 Tests
- [ ] Seccomp filter created successfully
- [ ] Allowed syscalls work (read, write, mmap)
- [ ] Blocked syscalls return EPERM
- [ ] Wasm execution works with filter
- [ ] No sandbox escapes possible

### Security Considerations

1. **Privilege Requirements**
   - eBPF loading requires `CAP_BPF` or `CAP_SYS_ADMIN`
   - Consider using unprivileged eBPF where possible
   - Document required capabilities

2. **Performance Impact**
   - Seccomp filtering adds ~100ns per syscall
   - eBPF map lookups are O(1)
   - Benchmark before/after

3. **Compatibility**
   - Test on kernel 5.4+ (LTS)
   - Test on kernel 6.x (latest)
   - Document minimum kernel version

---

## References

- [eBPF Documentation](https://ebpf.io/what-is-ebpf/)
- [Seccomp BPF](https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html)
- [Linux Namespaces](https://man7.org/linux/man-pages/man7/namespaces.7.html)
- [Wasmtime Security](https://docs.wasmtime.dev/security.html)

---

## Completion Criteria

- [ ] All Y10.6 tasks implemented
- [ ] All Y10.7 tasks implemented
- [ ] Tests passing on Linux
- [ ] Documentation updated
- [ ] Security review completed
- [ ] Commit pushed to main
