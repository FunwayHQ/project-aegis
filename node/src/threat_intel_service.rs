use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::ebpf_loader::EbpfLoader;
use crate::threat_intel_p2p::{P2PConfig, ThreatIntelP2P, ThreatIntelligence};

/// Configuration for the threat intelligence service
#[derive(Debug, Clone)]
pub struct ThreatIntelConfig {
    /// Path to eBPF program
    pub ebpf_program_path: String,
    /// Network interface to attach eBPF program
    pub interface: String,
    /// P2P network configuration
    pub p2p_config: P2PConfig,
    /// Whether to auto-publish local threats
    pub auto_publish: bool,
    /// Minimum severity to block (1-10)
    pub min_severity: u8,
}

impl Default for ThreatIntelConfig {
    fn default() -> Self {
        Self {
            ebpf_program_path: "ebpf/syn-flood-filter/target/bpfel-unknown-none/release/syn-flood-filter".to_string(),
            interface: "lo".to_string(),
            p2p_config: P2PConfig::default(),
            auto_publish: true,
            min_severity: 5,
        }
    }
}

/// Threat Intelligence Service
/// Connects P2P threat intelligence network with eBPF blocklist
pub struct ThreatIntelService {
    ebpf: Arc<Mutex<EbpfLoader>>,
    config: ThreatIntelConfig,
    p2p_sender: mpsc::UnboundedSender<ThreatIntelligence>,
}

impl ThreatIntelService {
    /// Create a new threat intelligence service
    pub fn new(config: ThreatIntelConfig) -> Result<Self> {
        // Load eBPF program
        let ebpf_path = Path::new(&config.ebpf_program_path);
        let mut ebpf = EbpfLoader::load(ebpf_path)
            .context("Failed to load eBPF program")?;

        // Attach to interface
        ebpf.attach(&config.interface)
            .context("Failed to attach eBPF program")?;

        info!("eBPF program attached to interface: {}", config.interface);

        // Create P2P network
        let mut p2p = ThreatIntelP2P::new(config.p2p_config.clone())
            .context("Failed to create P2P network")?;

        let p2p_sender = p2p.get_sender();

        // Start listening on P2P network
        p2p.listen(config.p2p_config.listen_port)
            .context("Failed to start P2P listener")?;

        let ebpf = Arc::new(Mutex::new(ebpf));
        let ebpf_clone = ebpf.clone();
        let min_severity = config.min_severity;

        // Spawn P2P event loop
        tokio::spawn(async move {
            let handler = move |threat: ThreatIntelligence| -> Result<()> {
                // Check severity threshold
                if threat.severity < min_severity {
                    info!(
                        "Ignoring threat {} with severity {} (below threshold {})",
                        threat.ip, threat.severity, min_severity
                    );
                    return Ok(());
                }

                // Update eBPF blocklist
                let mut ebpf = ebpf_clone.lock().unwrap();
                ebpf.blocklist_ip(&threat.ip, threat.block_duration_secs)
                    .context("Failed to blocklist IP")?;

                info!(
                    "Blocklisted {} for {}s (threat: {}, severity: {})",
                    threat.ip, threat.block_duration_secs, threat.threat_type, threat.severity
                );

                Ok(())
            };

            if let Err(e) = p2p.run(handler).await {
                error!("P2P network error: {}", e);
            }
        });

        Ok(Self {
            ebpf,
            config,
            p2p_sender,
        })
    }

    /// Publish a threat to the P2P network
    pub fn publish_threat(&self, threat: ThreatIntelligence) -> Result<()> {
        // Validate threat before publishing
        threat.validate()
            .context("Invalid threat intelligence")?;

        self.p2p_sender
            .send(threat.clone())
            .map_err(|e| anyhow::anyhow!("Failed to send threat: {}", e))?;

        info!("Published threat: {} (type: {})", threat.ip, threat.threat_type);
        Ok(())
    }

    /// Manually blocklist an IP and optionally publish to network
    pub fn blocklist_and_publish(
        &self,
        ip: String,
        threat_type: String,
        severity: u8,
        block_duration_secs: u64,
        source_node: String,
    ) -> Result<()> {
        // Update local eBPF blocklist
        {
            let mut ebpf = self.ebpf.lock().unwrap();
            ebpf.blocklist_ip(&ip, block_duration_secs)
                .context("Failed to blocklist IP locally")?;
        }

        // Publish to network if auto-publish is enabled
        if self.config.auto_publish {
            let threat = ThreatIntelligence::new(
                ip.clone(),
                threat_type,
                severity,
                block_duration_secs,
                source_node,
            );

            self.publish_threat(threat)?;
        }

        Ok(())
    }

    /// Get current blocklist from eBPF
    pub fn get_blocklist(&self) -> Result<Vec<(String, u64)>> {
        let ebpf = self.ebpf.lock().unwrap();
        ebpf.get_blocklist()
    }

    /// Remove IP from blocklist
    pub fn remove_from_blocklist(&self, ip: &str) -> Result<()> {
        let mut ebpf = self.ebpf.lock().unwrap();
        ebpf.remove_from_blocklist(ip)
    }

    /// Check if IP is blocklisted
    pub fn is_blocklisted(&self, ip: &str) -> Result<bool> {
        let ebpf = self.ebpf.lock().unwrap();
        ebpf.is_blocklisted(ip)
    }

    /// Get eBPF statistics
    pub fn get_stats(&self) -> Result<crate::ebpf_loader::DDoSStats> {
        let ebpf = self.ebpf.lock().unwrap();
        ebpf.get_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ThreatIntelConfig::default();
        assert!(config.ebpf_program_path.contains("syn-flood-filter"));
        assert_eq!(config.interface, "lo");
        assert_eq!(config.min_severity, 5);
        assert!(config.auto_publish);
    }

    #[test]
    fn test_config_custom() {
        let mut config = ThreatIntelConfig::default();
        config.min_severity = 8;
        config.auto_publish = false;
        config.interface = "eth0".to_string();

        assert_eq!(config.min_severity, 8);
        assert!(!config.auto_publish);
        assert_eq!(config.interface, "eth0");
    }

    #[test]
    fn test_threat_validation_before_publish() {
        let threat = ThreatIntelligence::new(
            "192.168.1.100".to_string(),
            "syn_flood".to_string(),
            8,
            300,
            "test-node".to_string(),
        );

        assert!(threat.validate().is_ok());
    }

    #[test]
    fn test_invalid_threat_validation() {
        let invalid_threat = ThreatIntelligence::new(
            "invalid-ip".to_string(),
            "test".to_string(),
            5,
            300,
            "test-node".to_string(),
        );

        assert!(invalid_threat.validate().is_err());
    }
}
