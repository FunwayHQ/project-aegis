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

#[repr(C)]
#[derive(Clone, Copy)]
struct SynInfo {
    count: u64,
    last_seen: u64,
}

// Statistics indices
const STAT_TOTAL_PACKETS: u32 = 0;
const STAT_SYN_PACKETS: u32 = 1;
const STAT_DROPPED_PACKETS: u32 = 2;
const STAT_PASSED_PACKETS: u32 = 3;

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

    // Parse Ethernet header
    let ethhdr = ptr_at::<EthHdr>(&ctx, 0)?;

    // Check if IP packet (IPv4)
    if u16::from_be(unsafe { (*ethhdr).h_proto }) != ETH_P_IP {
        return Ok(xdp_action::XDP_PASS);  // Not IPv4, pass it
    }

    // Parse IP header
    let iphdr = ptr_at::<IpHdr>(&ctx, EthHdr::LEN)?;
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

    // Extract source IP
    let src_ip = u32::from_be(unsafe { (*iphdr).saddr });

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

    // Get current time (nanoseconds)
    let now = unsafe { bpf_ktime_get_ns() };

    // Check SYN rate for this source IP
    let should_drop = unsafe {
        match SYN_TRACKER.get(&src_ip) {
            Some(info) => {
                let mut info = *info;

                // Check if this is within the same second
                let time_diff = now.saturating_sub(info.last_seen);
                let one_second_ns = 1_000_000_000;

                if time_diff < one_second_ns {
                    // Within same second, increment count
                    info.count += 1;

                    // Check if exceeded threshold
                    if info.count > threshold {
                        // Rate limit exceeded!
                        info!(
                            &ctx,
                            "SYN flood detected from IP: {} (count: {})",
                            src_ip,
                            info.count
                        );
                        true  // Drop packet
                    } else {
                        // Update tracker
                        SYN_TRACKER.insert(&src_ip, &info, 0).ok();
                        false  // Pass packet
                    }
                } else {
                    // New second, reset count
                    let new_info = SynInfo {
                        count: 1,
                        last_seen: now,
                    };
                    SYN_TRACKER.insert(&src_ip, &new_info, 0).ok();
                    false  // Pass packet (first in new window)
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

// External function from kernel
extern "C" {
    fn bpf_ktime_get_ns() -> u64;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
