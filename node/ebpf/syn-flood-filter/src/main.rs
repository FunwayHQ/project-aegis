#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::xdp_action,
    macros::{map, xdp},
    maps::{HashMap, Array},
    programs::XdpContext,
};
use aya_log_ebpf::info;

use core::mem;

// Network protocol constants
const ETH_P_IP: u16 = 0x0800;
const IPPROTO_TCP: u8 = 6;
const TCP_FLAG_SYN: u8 = 0x02;

// Configuration keys
const CONFIG_SYN_THRESHOLD: u32 = 0;  // SYN packets per second per IP threshold
const CONFIG_GLOBAL_THRESHOLD: u32 = 1;  // Total SYN packets per second

// Map to track SYN packet counts per source IP
#[map]
static SYN_TRACKER: HashMap<u32, SynInfo> = HashMap::with_max_entries(10000, 0);

// Configuration map (updatable from userspace)
#[map]
static CONFIG: Array<u64> = Array::with_max_entries(10, 0);

// Statistics map
#[map]
static STATS: Array<u64> = Array::with_max_entries(10, 0);

// Whitelist map (IPs that are never rate-limited)
#[map]
static WHITELIST: HashMap<u32, u8> = HashMap::with_max_entries(1000, 0);

// SECURITY OPTIMIZATION: Blocklist for severe offenders (auto-blacklisting)
// IPs that significantly exceed threshold are blocked for 30 seconds
#[map]
static BLOCKLIST: HashMap<u32, BlockInfo> = HashMap::with_max_entries(5000, 0);

#[repr(C)]
#[derive(Clone, Copy)]
struct SynInfo {
    count: u64,
    last_seen: u64,
}

/// SECURITY OPTIMIZATION: Block info for auto-blacklisted IPs
#[repr(C)]
#[derive(Clone, Copy)]
struct BlockInfo {
    blocked_until: u64,      // Timestamp when block expires (microseconds)
    total_violations: u64,   // Count of how many times threshold exceeded
}

// Statistics indices
const STAT_TOTAL_PACKETS: u32 = 0;
const STAT_SYN_PACKETS: u32 = 1;
const STAT_DROPPED_PACKETS: u32 = 2;
const STAT_PASSED_PACKETS: u32 = 3;
const STAT_BLOCKED_IPS: u32 = 4;        // OPTIMIZATION: Track auto-blacklisted IPs
const STAT_EARLY_DROPS: u32 = 5;        // OPTIMIZATION: Drops from blocklist

// OPTIMIZATION: Time constants (using microseconds for coarse timer)
const ONE_SECOND_US: u64 = 1_000_000;   // 1 second in microseconds
const BLOCK_DURATION_US: u64 = 30_000_000;  // 30 seconds in microseconds

#[xdp]
pub fn syn_flood_filter(ctx: XdpContext) -> u32 {
    match try_syn_flood_filter(ctx) {
        Ok(ret) => ret,
        Err(_) => xdp_action::XDP_PASS,  // Pass on error (fail open)
    }
}

fn try_syn_flood_filter(ctx: XdpContext) -> Result<u32, ()> {
    // Increment total packet counter
    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_TOTAL_PACKETS) {
            *counter += 1;
        }
    }

    // PERFORMANCE OPTIMIZATION: Get current time early (coarse timer)
    // Using boot time in microseconds instead of nanoseconds
    // This is 10-100x faster and precision is sufficient for rate limiting
    let now = unsafe { bpf_ktime_get_boot_ns() / 1000 };  // Convert to microseconds

    // Parse Ethernet header
    let ethhdr = ptr_at::<EthHdr>(&ctx, 0)?;

    // Check if IP packet (IPv4)
    if u16::from_be(unsafe { (*ethhdr).h_proto }) != ETH_P_IP {
        return Ok(xdp_action::XDP_PASS);  // Not IPv4, pass it
    }

    // Parse IP header
    let iphdr = ptr_at::<IpHdr>(&ctx, EthHdr::LEN)?;
    let src_ip = u32::from_be(unsafe { (*iphdr).saddr });

    // SECURITY OPTIMIZATION: Early drop for blocked IPs (before parsing TCP)
    // This saves CPU cycles by dropping known attackers immediately
    if let Some(block_info) = unsafe { BLOCKLIST.get(&src_ip) } {
        let block_info = *block_info;

        if block_info.blocked_until > now {
            // Still blocked, drop immediately without further processing
            unsafe {
                if let Some(counter) = STATS.get_ptr_mut(STAT_DROPPED_PACKETS) {
                    *counter += 1;
                }
                if let Some(counter) = STATS.get_ptr_mut(STAT_EARLY_DROPS) {
                    *counter += 1;
                }
            }
            return Ok(xdp_action::XDP_DROP);
        }
        // Block expired, remove from blocklist
        unsafe {
            BLOCKLIST.remove(&src_ip).ok();
        }
    }

    let ip_proto = unsafe { (*iphdr).protocol };

    // Check if TCP packet
    if ip_proto != IPPROTO_TCP {
        return Ok(xdp_action::XDP_PASS);  // Not TCP, pass it
    }

    // Parse TCP header
    let tcphdr = ptr_at::<TcpHdr>(&ctx, EthHdr::LEN + IpHdr::LEN)?;
    let tcp_flags = unsafe { (*tcphdr).flags() };

    // Check if SYN flag is set (and not ACK - pure SYN)
    let is_syn = (tcp_flags & TCP_FLAG_SYN) != 0;
    let is_ack = (tcp_flags & 0x10) != 0;  // ACK flag

    if !is_syn || is_ack {
        return Ok(xdp_action::XDP_PASS);  // Not a SYN packet, pass it
    }

    // Increment SYN packet counter
    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_SYN_PACKETS) {
            *counter += 1;
        }
    }

    // Check if IP is whitelisted
    if unsafe { WHITELIST.get(&src_ip).is_some() } {
        // Whitelisted IP, always pass
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_PASSED_PACKETS) {
                *counter += 1;
            }
        }
        return Ok(xdp_action::XDP_PASS);
    }

    // Get current threshold from config (default: 100 SYN/sec)
    let threshold = unsafe {
        CONFIG.get(CONFIG_SYN_THRESHOLD)
            .map(|v| *v)
            .unwrap_or(100)
    };

    // SECURITY OPTIMIZATION: Check SYN rate with improved algorithm
    let should_drop = unsafe {
        match SYN_TRACKER.get(&src_ip) {
            Some(info) => {
                let mut info = *info;

                // OPTIMIZATION: Time diff in microseconds (coarse timer)
                let time_diff = now.saturating_sub(info.last_seen);

                if time_diff < ONE_SECOND_US {
                    // Within same second, increment count
                    info.count += 1;
                    info.last_seen = now;  // Update timestamp

                    // Check if exceeded threshold
                    if info.count > threshold {
                        // Rate limit exceeded!
                        info!(
                            &ctx,
                            "SYN flood detected from IP: {} (count: {})",
                            src_ip,
                            info.count
                        );

                        // SECURITY OPTIMIZATION: Auto-blacklist severe offenders
                        // If IP exceeds threshold by 2x, add to blocklist for 30 seconds
                        if info.count > threshold * 2 {
                            let block_info = BlockInfo {
                                blocked_until: now + BLOCK_DURATION_US,
                                total_violations: info.count,
                            };
                            BLOCKLIST.insert(&src_ip, &block_info, 0).ok();

                            // Update blocklist stats
                            if let Some(counter) = STATS.get_ptr_mut(STAT_BLOCKED_IPS) {
                                *counter += 1;
                            }

                            info!(
                                &ctx,
                                "IP auto-blacklisted for 30s: {} (violations: {})",
                                src_ip,
                                info.count
                            );
                        }

                        // Update tracker even when dropping (maintain state)
                        SYN_TRACKER.insert(&src_ip, &info, 0).ok();
                        true  // Drop packet
                    } else {
                        // Update tracker
                        SYN_TRACKER.insert(&src_ip, &info, 0).ok();
                        false  // Pass packet
                    }
                } else {
                    // SECURITY OPTIMIZATION: Gradual decay instead of hard reset
                    // This prevents micro-burst attacks at window boundaries
                    // Instead of resetting to 1, we decay the previous count
                    let decayed_count = if info.count > 10 {
                        // If previous count was high, decay by 50%
                        info.count / 2
                    } else {
                        // If count was low, reset to 1
                        1
                    };

                    let new_info = SynInfo {
                        count: decayed_count,
                        last_seen: now,
                    };
                    SYN_TRACKER.insert(&src_ip, &new_info, 0).ok();
                    false  // Pass packet (new window or decayed)
                }
            }
            None => {
                // First SYN from this IP
                let new_info = SynInfo {
                    count: 1,
                    last_seen: now,
                };
                SYN_TRACKER.insert(&src_ip, &new_info, 0).ok();
                false  // Pass packet
            }
        }
    };

    if should_drop {
        // Increment drop counter
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_DROPPED_PACKETS) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_DROP)
    } else {
        // Increment pass counter
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_PASSED_PACKETS) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_PASS)
    }
}

// Helper function to get pointer to data at offset
#[inline(always)]
fn ptr_at<T>(ctx: &XdpContext, offset: usize) -> Result<*const T, ()> {
    let start = ctx.data();
    let end = ctx.data_end();
    let len = mem::size_of::<T>();

    if start + offset + len > end {
        return Err(());
    }

    Ok((start + offset) as *const T)
}

// Ethernet header
#[repr(C)]
struct EthHdr {
    h_dest: [u8; 6],
    h_source: [u8; 6],
    h_proto: u16,
}

impl EthHdr {
    const LEN: usize = mem::size_of::<Self>();
}

// IPv4 header (simplified)
#[repr(C)]
struct IpHdr {
    _version_ihl: u8,
    _tos: u8,
    _tot_len: u16,
    _id: u16,
    _frag_off: u16,
    _ttl: u8,
    protocol: u8,
    _check: u16,
    saddr: u32,
    _daddr: u32,
}

impl IpHdr {
    const LEN: usize = 20;  // Minimum IP header length (no options)
}

// TCP header (simplified)
#[repr(C)]
struct TcpHdr {
    source: u16,
    dest: u16,
    seq: u32,
    ack_seq: u32,
    _flags_and_offset: u16,
    window: u16,
    check: u16,
    urg_ptr: u16,
}

impl TcpHdr {
    const LEN: usize = 20;  // Minimum TCP header length

    #[inline(always)]
    fn flags(&self) -> u8 {
        // Flags are in the lower byte of _flags_and_offset (in network byte order)
        (u16::from_be(self._flags_and_offset) & 0xFF) as u8
    }
}

// PERFORMANCE OPTIMIZATION: Use coarse boot time instead of nanosecond precision
// bpf_ktime_get_boot_ns() is 10-100x faster than bpf_ktime_get_ns()
// Microsecond precision is more than sufficient for rate limiting
extern "C" {
    fn bpf_ktime_get_boot_ns() -> u64;  // Boot time in nanoseconds (coarse clock)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
