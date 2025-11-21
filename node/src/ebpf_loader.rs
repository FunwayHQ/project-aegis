use anyhow::{anyhow, Context, Result};
use aya::{
    maps::{Array, HashMap},
    programs::{
        xdp::XdpLinkId,
        Xdp, XdpFlags,
    },
    Bpf,
};
use std::net::Ipv4Addr;
use std::path::Path;
use std::str::FromStr;
use tracing::{info, warn};

// Configuration keys (must match eBPF program)
const CONFIG_SYN_THRESHOLD: u32 = 0;
const CONFIG_GLOBAL_THRESHOLD: u32 = 1;

// Statistics indices
const STAT_TOTAL_PACKETS: u32 = 0;
const STAT_SYN_PACKETS: u32 = 1;
const STAT_DROPPED_PACKETS: u32 = 2;
const STAT_PASSED_PACKETS: u32 = 3;

/// eBPF DDoS protection statistics
#[derive(Debug, Clone, Default)]
pub struct DDoSStats {
    pub total_packets: u64,
    pub syn_packets: u64,
    pub dropped_packets: u64,
    pub passed_packets: u64,
}

impl DDoSStats {
    /// Calculate drop rate percentage
    pub fn drop_rate(&self) -> f64 {
        if self.total_packets == 0 {
            0.0
        } else {
            (self.dropped_packets as f64 / self.total_packets as f64) * 100.0
        }
    }

    /// Calculate SYN packet percentage
    pub fn syn_percentage(&self) -> f64 {
        if self.total_packets == 0 {
            0.0
        } else {
            (self.syn_packets as f64 / self.total_packets as f64) * 100.0
        }
    }
}

/// eBPF program loader and manager
pub struct EbpfLoader {
    ebpf: Bpf,
    interface: String,
    attached: bool,
    link_id: Option<XdpLinkId>,
}

impl EbpfLoader {
    /// Load eBPF program from file
    pub fn load(program_path: &Path) -> Result<Self> {
        // Load eBPF bytecode
        let ebpf_data = std::fs::read(program_path)
            .with_context(|| format!("Failed to read eBPF program from {:?}", program_path))?;

        let ebpf = Bpf::load(&ebpf_data).context("Failed to load eBPF program")?;

        Ok(Self {
            ebpf,
            interface: String::new(),
            attached: false,
            link_id: None,
        })
    }

    /// Load eBPF program from embedded bytes
    /// Note: Requires eBPF program to be built first
    #[allow(dead_code)]
    pub fn load_embedded() -> Result<Self> {
        // This would include the compiled eBPF program at compile time
        // Disabled for now to allow tests to run without building eBPF program
        // Uncomment when eBPF program is built
        /*
        let ebpf = Bpf::load(include_bytes_aligned!(
            "../ebpf/syn-flood-filter/target/bpfel-unknown-none/release/syn-flood-filter"
        ))
        .context("Failed to load embedded eBPF program")?;

        Ok(Self {
            ebpf,
            interface: String::new(),
            attached: false,
        })
        */
        anyhow::bail!("load_embedded is not available without building eBPF program first")
    }

    /// Attach XDP program to network interface
    pub fn attach(&mut self, interface: &str) -> Result<()> {
        info!("Attaching XDP program to interface: {}", interface);

        let program: &mut Xdp = self
            .ebpf
            .program_mut("syn_flood_filter")
            .ok_or_else(|| anyhow!("XDP program not found"))?
            .try_into()
            .context("Program is not XDP type")?;

        // Load the program
        program.load().context("Failed to load XDP program")?;

        // Attach to interface with SKB mode (compatible, slower than native)
        // For production, use XdpFlags::default() for native mode
        let link_id = program
            .attach(interface, XdpFlags::SKB_MODE)
            .context("Failed to attach XDP program to interface")?;

        self.interface = interface.to_string();
        self.attached = true;
        self.link_id = Some(link_id);

        info!("XDP program attached successfully to {}", interface);
        Ok(())
    }

    /// Detach XDP program from interface
    pub fn detach(&mut self) -> Result<()> {
        if !self.attached {
            return Ok(());
        }

        if let Some(link_id) = self.link_id.take() {
            let program: &mut Xdp = self
                .ebpf
                .program_mut("syn_flood_filter")
                .ok_or_else(|| anyhow!("XDP program not found"))?
                .try_into()
                .context("Program is not XDP type")?;

            program.detach(link_id)?;

            self.attached = false;
            info!("XDP program detached from {}", self.interface);
        }
        Ok(())
    }

    /// Set SYN flood threshold (packets per second per IP)
    pub fn set_syn_threshold(&mut self, threshold: u64) -> Result<()> {
        let mut config: Array<_, u64> = Array::try_from(
            self.ebpf
                .map_mut("CONFIG")
                .ok_or_else(|| anyhow!("CONFIG map not found"))?,
        )?;

        config
            .set(CONFIG_SYN_THRESHOLD, threshold, 0)
            .context("Failed to set SYN threshold")?;

        info!("SYN flood threshold set to: {}", threshold);
        Ok(())
    }

    /// Set global SYN threshold
    pub fn set_global_threshold(&mut self, threshold: u64) -> Result<()> {
        let mut config: Array<_, u64> = Array::try_from(
            self.ebpf
                .map_mut("CONFIG")
                .ok_or_else(|| anyhow!("CONFIG map not found"))?,
        )?;

        config
            .set(CONFIG_GLOBAL_THRESHOLD, threshold, 0)
            .context("Failed to set global threshold")?;

        info!("Global SYN threshold set to: {}", threshold);
        Ok(())
    }

    /// Add IP to whitelist (never rate-limited)
    pub fn whitelist_ip(&mut self, ip: &str) -> Result<()> {
        let ip_addr = Ipv4Addr::from_str(ip)?;
        let ip_u32 = u32::from(ip_addr).to_be(); // Network byte order

        let mut whitelist: HashMap<_, u32, u8> = HashMap::try_from(
            self.ebpf
                .map_mut("WHITELIST")
                .ok_or_else(|| anyhow!("WHITELIST map not found"))?,
        )?;

        whitelist
            .insert(ip_u32, 1, 0)
            .context("Failed to add IP to whitelist")?;

        info!("Added IP to whitelist: {}", ip);
        Ok(())
    }

    /// Remove IP from whitelist
    pub fn remove_from_whitelist(&mut self, ip: &str) -> Result<()> {
        let ip_addr = Ipv4Addr::from_str(ip)?;
        let ip_u32 = u32::from(ip_addr).to_be();

        let mut whitelist: HashMap<_, u32, u8> = HashMap::try_from(
            self.ebpf
                .map_mut("WHITELIST")
                .ok_or_else(|| anyhow!("WHITELIST map not found"))?,
        )?;

        whitelist
            .remove(&ip_u32)
            .context("Failed to remove IP from whitelist")?;

        info!("Removed IP from whitelist: {}", ip);
        Ok(())
    }

    /// Get current statistics
    pub fn get_stats(&self) -> Result<DDoSStats> {
        let stats_map: Array<_, u64> = Array::try_from(
            self.ebpf
                .map("STATS")
                .ok_or_else(|| anyhow!("STATS map not found"))?,
        )?;

        let total_packets = stats_map.get(&STAT_TOTAL_PACKETS, 0).unwrap_or(0);
        let syn_packets = stats_map.get(&STAT_SYN_PACKETS, 0).unwrap_or(0);
        let dropped_packets = stats_map.get(&STAT_DROPPED_PACKETS, 0).unwrap_or(0);
        let passed_packets = stats_map.get(&STAT_PASSED_PACKETS, 0).unwrap_or(0);

        Ok(DDoSStats {
            total_packets,
            syn_packets,
            dropped_packets,
            passed_packets,
        })
    }

    /// Check if attached to an interface
    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// Get attached interface name
    pub fn interface(&self) -> Option<&str> {
        if self.attached {
            Some(&self.interface)
        } else {
            None
        }
    }

    /// Add IP to blocklist (for threat intelligence sharing)
    /// The IP will be blocked for the specified duration in seconds
    pub fn blocklist_ip(&mut self, ip: &str, duration_secs: u64) -> Result<()> {
        let ip_addr = Ipv4Addr::from_str(ip)?;
        let ip_u32 = u32::from(ip_addr).to_be(); // Network byte order

        // Get current time in microseconds (matches eBPF program)
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let blocked_until = now_us + (duration_secs * 1_000_000);

        let mut blocklist: HashMap<_, u32, BlockInfo> = HashMap::try_from(
            self.ebpf
                .map_mut("BLOCKLIST")
                .ok_or_else(|| anyhow!("BLOCKLIST map not found"))?,
        )?;

        let block_info = BlockInfo {
            blocked_until,
            total_violations: 1,
        };

        blocklist
            .insert(ip_u32, block_info, 0)
            .context("Failed to add IP to blocklist")?;

        info!("Added IP to blocklist: {} (until {}us)", ip, blocked_until);
        Ok(())
    }

    /// Remove IP from blocklist
    pub fn remove_from_blocklist(&mut self, ip: &str) -> Result<()> {
        let ip_addr = Ipv4Addr::from_str(ip)?;
        let ip_u32 = u32::from(ip_addr).to_be();

        let mut blocklist: HashMap<_, u32, BlockInfo> = HashMap::try_from(
            self.ebpf
                .map_mut("BLOCKLIST")
                .ok_or_else(|| anyhow!("BLOCKLIST map not found"))?,
        )?;

        blocklist
            .remove(&ip_u32)
            .context("Failed to remove IP from blocklist")?;

        info!("Removed IP from blocklist: {}", ip);
        Ok(())
    }

    /// Check if IP is in blocklist
    pub fn is_blocklisted(&self, ip: &str) -> Result<bool> {
        let ip_addr = Ipv4Addr::from_str(ip)?;
        let ip_u32 = u32::from(ip_addr).to_be();

        let blocklist: HashMap<_, u32, BlockInfo> = HashMap::try_from(
            self.ebpf
                .map("BLOCKLIST")
                .ok_or_else(|| anyhow!("BLOCKLIST map not found"))?,
        )?;

        Ok(blocklist.get(&ip_u32, 0).is_ok())
    }

    /// Get all blocklisted IPs with their expiration times
    pub fn get_blocklist(&self) -> Result<Vec<(String, u64)>> {
        let blocklist: HashMap<_, u32, BlockInfo> = HashMap::try_from(
            self.ebpf
                .map("BLOCKLIST")
                .ok_or_else(|| anyhow!("BLOCKLIST map not found"))?,
        )?;

        let mut result = Vec::new();

        // Iterate through all keys in the blocklist
        // Note: HashMap iteration in aya 0.12 requires iterating over keys
        for key in blocklist.keys() {
            if let Ok(ip_u32) = key {
                if let Ok(block_info) = blocklist.get(&ip_u32, 0) {
                    let ip = Ipv4Addr::from(u32::from_be(ip_u32));
                    result.push((ip.to_string(), block_info.blocked_until));
                }
            }
        }

        Ok(result)
    }
}

/// BlockInfo structure (must match eBPF program definition)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct BlockInfo {
    blocked_until: u64,
    total_violations: u64,
}

// SAFETY: BlockInfo is a simple C-compatible struct with no padding
unsafe impl aya::Pod for BlockInfo {}

impl Drop for EbpfLoader {
    fn drop(&mut self) {
        if self.attached {
            if let Err(e) = self.detach() {
                warn!("Failed to detach XDP program on drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ddos_stats_default() {
        let stats = DDoSStats::default();
        assert_eq!(stats.total_packets, 0);
        assert_eq!(stats.syn_packets, 0);
        assert_eq!(stats.dropped_packets, 0);
        assert_eq!(stats.passed_packets, 0);
    }

    #[test]
    fn test_ddos_stats_drop_rate() {
        let stats = DDoSStats {
            total_packets: 1000,
            syn_packets: 800,
            dropped_packets: 100,
            passed_packets: 900,
        };

        assert_eq!(stats.drop_rate(), 10.0); // 100/1000 = 10%
    }

    #[test]
    fn test_ddos_stats_drop_rate_zero_packets() {
        let stats = DDoSStats::default();
        assert_eq!(stats.drop_rate(), 0.0);
    }

    #[test]
    fn test_ddos_stats_syn_percentage() {
        let stats = DDoSStats {
            total_packets: 1000,
            syn_packets: 200,
            dropped_packets: 50,
            passed_packets: 950,
        };

        assert_eq!(stats.syn_percentage(), 20.0); // 200/1000 = 20%
    }

    #[test]
    fn test_ddos_stats_100_percent_drop() {
        let stats = DDoSStats {
            total_packets: 1000,
            syn_packets: 1000,
            dropped_packets: 1000,
            passed_packets: 0,
        };

        assert_eq!(stats.drop_rate(), 100.0);
        assert_eq!(stats.syn_percentage(), 100.0);
    }

    #[test]
    fn test_ipv4_addr_to_u32_conversion() {
        let ip = Ipv4Addr::from_str("192.168.1.100").unwrap();
        let ip_u32 = u32::from(ip);

        // Should convert correctly
        assert_ne!(ip_u32, 0);

        // Convert back
        let ip_back = Ipv4Addr::from(ip_u32);
        assert_eq!(ip, ip_back);
    }

    #[test]
    fn test_ipv4_network_byte_order() {
        let ip = Ipv4Addr::from_str("127.0.0.1").unwrap();
        let ip_u32 = u32::from(ip);
        let ip_be = ip_u32.to_be();

        // Should be in network byte order for eBPF
        assert_ne!(ip_u32, ip_be); // Different endianness
    }

    #[test]
    fn test_threshold_values() {
        let default_threshold = 100_u64;
        let low_threshold = 10_u64;
        let high_threshold = 1000_u64;

        assert!(default_threshold > 0);
        assert!(low_threshold < default_threshold);
        assert!(high_threshold > default_threshold);
    }

    #[test]
    fn test_whitelist_ip_parsing() {
        let valid_ips = vec!["192.168.1.1", "10.0.0.1", "172.16.0.1", "127.0.0.1"];

        for ip_str in valid_ips {
            let result = Ipv4Addr::from_str(ip_str);
            assert!(result.is_ok(), "Failed to parse: {}", ip_str);
        }
    }

    #[test]
    fn test_invalid_ip_handling() {
        let invalid_ips = vec![
            "256.1.1.1", // Out of range
            "not-an-ip", // Invalid format
            "192.168",   // Incomplete
            "",          // Empty
        ];

        for ip_str in invalid_ips {
            let result = Ipv4Addr::from_str(ip_str);
            assert!(result.is_err(), "Should fail for: {}", ip_str);
        }
    }
}
