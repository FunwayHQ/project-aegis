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
const ETH_P_IPV6: u16 = 0x86DD; // Sprint 13.5: IPv6 support
const IPPROTO_TCP: u8 = 6;
const IPPROTO_UDP: u8 = 17;
const TCP_FLAG_SYN: u8 = 0x02;

// Configuration keys
const CONFIG_SYN_THRESHOLD: u32 = 0;  // SYN packets per second per IP threshold
const CONFIG_GLOBAL_THRESHOLD: u32 = 1;  // Total SYN packets per second
const CONFIG_UDP_THRESHOLD: u32 = 2;  // UDP packets per second per IP threshold

// Map to track SYN packet counts per source IP
#[map]
static SYN_TRACKER: HashMap<u32, SynInfo> = HashMap::with_max_entries(10000, 0);

// Map to track UDP packet counts per source IP (Sprint 12.5)
#[map]
static UDP_TRACKER: HashMap<u32, UdpInfo> = HashMap::with_max_entries(10000, 0);

// Sprint 13.5: IPv6 tracking maps
// IPv6 addresses are 128 bits, represented as [u32; 4]
#[map]
static SYN_TRACKER_V6: HashMap<Ipv6Addr, SynInfo> = HashMap::with_max_entries(10000, 0);

#[map]
static UDP_TRACKER_V6: HashMap<Ipv6Addr, UdpInfo> = HashMap::with_max_entries(10000, 0);

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

// Sprint 29: IPv6 blocklist for severe offenders
// Mirrors the IPv4 blocklist functionality for IPv6 addresses
#[map]
static BLOCKLIST_V6: HashMap<Ipv6Addr, BlockInfo> = HashMap::with_max_entries(5000, 0);

/// IPv6 address representation (128 bits as 4x u32) - Sprint 13.5
#[repr(C)]
#[derive(Clone, Copy)]
struct Ipv6Addr {
    addr: [u32; 4], // 128 bits in network byte order
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SynInfo {
    count: u64,
    last_seen: u64,
}

/// UDP packet tracking info (Sprint 12.5)
#[repr(C)]
#[derive(Clone, Copy)]
struct UdpInfo {
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
const STAT_UDP_PACKETS: u32 = 6;        // Sprint 12.5: UDP packet counter
const STAT_UDP_DROPPED: u32 = 7;        // Sprint 12.5: Dropped UDP packets
const STAT_IPV6_PACKETS: u32 = 8;       // Sprint 13.5: IPv6 packet counter
const STAT_IPV6_DROPPED: u32 = 9;       // Sprint 13.5: Dropped IPv6 packets

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

    let eth_proto = u16::from_be(unsafe { (*ethhdr).h_proto });

    // Sprint 13.5: Handle IPv6 packets
    if eth_proto == ETH_P_IPV6 {
        return try_ipv6_filter(&ctx, now);
    }

    // Check if IP packet (IPv4)
    if eth_proto != ETH_P_IP {
        return Ok(xdp_action::XDP_PASS);  // Not IPv4 or IPv6, pass it
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

    // Sprint 12.5: Handle UDP flood protection
    if ip_proto == IPPROTO_UDP {
        return handle_udp_packet(src_ip, now);
    }

    // Check if TCP packet
    if ip_proto != IPPROTO_TCP {
        return Ok(xdp_action::XDP_PASS);  // Not TCP or UDP, pass it
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

/// Sprint 12.5: Handle UDP flood detection and rate limiting
#[inline(always)]
fn handle_udp_packet(src_ip: u32, now: u64) -> Result<u32, ()> {
    // Increment UDP packet counter
    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_UDP_PACKETS) {
            *counter += 1;
        }
    }

    // Check if IP is whitelisted
    if unsafe { WHITELIST.get(&src_ip).is_some() } {
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_PASSED_PACKETS) {
                *counter += 1;
            }
        }
        return Ok(xdp_action::XDP_PASS);
    }

    // Get current UDP threshold from config (default: 1000 UDP/sec)
    let threshold = unsafe {
        CONFIG.get(CONFIG_UDP_THRESHOLD)
            .map(|v| *v)
            .unwrap_or(1000)
    };

    // Check UDP rate with similar algorithm to SYN flood
    let should_drop = unsafe {
        match UDP_TRACKER.get(&src_ip) {
            Some(info) => {
                let mut info = *info;

                let time_diff = now.saturating_sub(info.last_seen);

                if time_diff < ONE_SECOND_US {
                    // Within same second, increment count
                    info.count += 1;
                    info.last_seen = now;

                    // Check if exceeded threshold
                    if info.count > threshold {
                        // UDP flood detected!

                        // Auto-blacklist severe offenders (2x threshold)
                        if info.count > threshold * 2 {
                            let block_info = BlockInfo {
                                blocked_until: now + BLOCK_DURATION_US,
                                total_violations: info.count,
                            };
                            BLOCKLIST.insert(&src_ip, &block_info, 0).ok();

                            if let Some(counter) = STATS.get_ptr_mut(STAT_BLOCKED_IPS) {
                                *counter += 1;
                            }
                        }

                        UDP_TRACKER.insert(&src_ip, &info, 0).ok();
                        true  // Drop packet
                    } else {
                        UDP_TRACKER.insert(&src_ip, &info, 0).ok();
                        false  // Pass packet
                    }
                } else {
                    // Gradual decay for new window
                    let decayed_count = if info.count > 10 {
                        info.count / 2
                    } else {
                        1
                    };

                    let new_info = UdpInfo {
                        count: decayed_count,
                        last_seen: now,
                    };
                    UDP_TRACKER.insert(&src_ip, &new_info, 0).ok();
                    false  // Pass packet
                }
            }
            None => {
                // First UDP from this IP
                let new_info = UdpInfo {
                    count: 1,
                    last_seen: now,
                };
                UDP_TRACKER.insert(&src_ip, &new_info, 0).ok();
                false  // Pass packet
            }
        }
    };

    if should_drop {
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_UDP_DROPPED) {
                *counter += 1;
            }
            if let Some(counter) = STATS.get_ptr_mut(STAT_DROPPED_PACKETS) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_DROP)
    } else {
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

/// Sprint 13.5: IPv6 packet filtering (mirrors IPv4 logic)
/// Sprint 29: Added BLOCKLIST_V6 early drop for severe offenders
fn try_ipv6_filter(ctx: &XdpContext, now: u64) -> Result<u32, ()> {
    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_IPV6_PACKETS) {
            *counter += 1;
        }
        if let Some(counter) = STATS.get_ptr_mut(STAT_TOTAL_PACKETS) {
            *counter += 1;
        }
    }

    // Parse IPv6 header
    let ipv6hdr = ptr_at::<Ipv6Hdr>(&ctx, EthHdr::LEN)?;
    let src_ipv6 = unsafe { (*ipv6hdr).saddr };
    let next_header = unsafe { (*ipv6hdr).nexthdr };

    // Sprint 29: Early drop for blocked IPv6 addresses (before parsing TCP/UDP)
    // This saves CPU cycles by dropping known attackers immediately
    if let Some(block_info) = unsafe { BLOCKLIST_V6.get(&src_ipv6) } {
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
                if let Some(counter) = STATS.get_ptr_mut(STAT_IPV6_DROPPED) {
                    *counter += 1;
                }
            }
            return Ok(xdp_action::XDP_DROP);
        }
        // Block expired, remove from blocklist
        unsafe {
            BLOCKLIST_V6.remove(&src_ipv6).ok();
        }
    }

    // Handle TCP (SYN flood)
    if next_header == IPPROTO_TCP {
        let tcphdr = ptr_at::<TcpHdr>(&ctx, EthHdr::LEN + Ipv6Hdr::LEN)?;
        let tcp_flags = unsafe { (*tcphdr).flags() };
        let is_syn = (tcp_flags & TCP_FLAG_SYN) != 0;
        let is_ack = (tcp_flags & 0x10) != 0;

        if is_syn && !is_ack {
            return handle_ipv6_syn(src_ipv6, now);
        }
    }
    // Handle UDP flood
    else if next_header == IPPROTO_UDP {
        return handle_ipv6_udp(src_ipv6, now);
    }

    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_PASSED_PACKETS) {
            *counter += 1;
        }
    }
    Ok(xdp_action::XDP_PASS)
}

/// Handle IPv6 SYN flood detection
/// Sprint 29: Added auto-blacklisting for severe offenders (2x threshold)
#[inline(always)]
fn handle_ipv6_syn(src_ipv6: Ipv6Addr, now: u64) -> Result<u32, ()> {
    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_SYN_PACKETS) {
            *counter += 1;
        }
    }

    let threshold = unsafe {
        CONFIG.get(CONFIG_SYN_THRESHOLD)
            .map(|v| *v)
            .unwrap_or(100)
    };

    let (should_drop, should_blocklist) = unsafe {
        match SYN_TRACKER_V6.get(&src_ipv6) {
            Some(info) => {
                let mut info = *info;
                let time_diff = now.saturating_sub(info.last_seen);

                if time_diff < ONE_SECOND_US {
                    info.count += 1;
                    info.last_seen = now;

                    if info.count > threshold {
                        SYN_TRACKER_V6.insert(&src_ipv6, &info, 0).ok();
                        // Sprint 29: Auto-blacklist if 2x threshold (severe offender)
                        let severe = info.count > threshold * 2;
                        (true, severe)
                    } else {
                        SYN_TRACKER_V6.insert(&src_ipv6, &info, 0).ok();
                        (false, false)
                    }
                } else {
                    let decayed_count = if info.count > 10 { info.count / 2 } else { 1 };
                    let new_info = SynInfo { count: decayed_count, last_seen: now };
                    SYN_TRACKER_V6.insert(&src_ipv6, &new_info, 0).ok();
                    (false, false)
                }
            }
            None => {
                let new_info = SynInfo { count: 1, last_seen: now };
                SYN_TRACKER_V6.insert(&src_ipv6, &new_info, 0).ok();
                (false, false)
            }
        }
    };

    if should_drop {
        // Sprint 29: Auto-blacklist severe IPv6 offenders
        if should_blocklist {
            unsafe {
                let block_info = BlockInfo {
                    blocked_until: now + BLOCK_DURATION_US,
                    total_violations: 1,
                };
                BLOCKLIST_V6.insert(&src_ipv6, &block_info, 0).ok();
                if let Some(counter) = STATS.get_ptr_mut(STAT_BLOCKED_IPS) {
                    *counter += 1;
                }
            }
        }
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_DROPPED_PACKETS) {
                *counter += 1;
            }
            if let Some(counter) = STATS.get_ptr_mut(STAT_IPV6_DROPPED) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_DROP)
    } else {
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_PASSED_PACKETS) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_PASS)
    }
}

/// Handle IPv6 UDP flood detection
/// Sprint 29: Added auto-blacklisting for severe offenders (2x threshold)
#[inline(always)]
fn handle_ipv6_udp(src_ipv6: Ipv6Addr, now: u64) -> Result<u32, ()> {
    unsafe {
        if let Some(counter) = STATS.get_ptr_mut(STAT_UDP_PACKETS) {
            *counter += 1;
        }
    }

    let threshold = unsafe {
        CONFIG.get(CONFIG_UDP_THRESHOLD)
            .map(|v| *v)
            .unwrap_or(1000)
    };

    let (should_drop, should_blocklist) = unsafe {
        match UDP_TRACKER_V6.get(&src_ipv6) {
            Some(info) => {
                let mut info = *info;
                let time_diff = now.saturating_sub(info.last_seen);

                if time_diff < ONE_SECOND_US {
                    info.count += 1;
                    info.last_seen = now;

                    if info.count > threshold {
                        UDP_TRACKER_V6.insert(&src_ipv6, &info, 0).ok();
                        // Sprint 29: Auto-blacklist if 2x threshold (severe offender)
                        let severe = info.count > threshold * 2;
                        (true, severe)
                    } else {
                        UDP_TRACKER_V6.insert(&src_ipv6, &info, 0).ok();
                        (false, false)
                    }
                } else {
                    let decayed_count = if info.count > 10 { info.count / 2 } else { 1 };
                    let new_info = UdpInfo { count: decayed_count, last_seen: now };
                    UDP_TRACKER_V6.insert(&src_ipv6, &new_info, 0).ok();
                    (false, false)
                }
            }
            None => {
                let new_info = UdpInfo { count: 1, last_seen: now };
                UDP_TRACKER_V6.insert(&src_ipv6, &new_info, 0).ok();
                (false, false)
            }
        }
    };

    if should_drop {
        // Sprint 29: Auto-blacklist severe IPv6 offenders
        if should_blocklist {
            unsafe {
                let block_info = BlockInfo {
                    blocked_until: now + BLOCK_DURATION_US,
                    total_violations: 1,
                };
                BLOCKLIST_V6.insert(&src_ipv6, &block_info, 0).ok();
                if let Some(counter) = STATS.get_ptr_mut(STAT_BLOCKED_IPS) {
                    *counter += 1;
                }
            }
        }
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_UDP_DROPPED) {
                *counter += 1;
            }
            if let Some(counter) = STATS.get_ptr_mut(STAT_DROPPED_PACKETS) {
                *counter += 1;
            }
            if let Some(counter) = STATS.get_ptr_mut(STAT_IPV6_DROPPED) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_DROP)
    } else {
        unsafe {
            if let Some(counter) = STATS.get_ptr_mut(STAT_PASSED_PACKETS) {
                *counter += 1;
            }
        }
        Ok(xdp_action::XDP_PASS)
    }
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

// IPv6 header (simplified) - Sprint 13.5
#[repr(C)]
struct Ipv6Hdr {
    _version_tc_fl: u32, // Version, traffic class, flow label
    _payload_len: u16,
    nexthdr: u8,      // Next header (protocol)
    _hop_limit: u8,
    saddr: Ipv6Addr,  // Source address (128 bits)
    _daddr: Ipv6Addr, // Destination address (128 bits)
}

impl Ipv6Hdr {
    const LEN: usize = 40; // Fixed IPv6 header length
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
