//! Threat Intelligence Service (Linux-only)
//!
//! This binary combines P2P threat intelligence sharing with eBPF integration.
//! It requires the Linux kernel for eBPF functionality.

#[cfg(target_os = "linux")]
use aegis_node::threat_intel_p2p::ThreatIntelligence;

#[cfg(target_os = "linux")]
use aegis_node::threat_intel_service::{ThreatIntelConfig, ThreatIntelService};

#[cfg(target_os = "linux")]
use anyhow::Result;

#[cfg(target_os = "linux")]
use std::io::{self, Write};

#[cfg(target_os = "linux")]
use tracing::{error, info};

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info,aegis_node=debug")
        .init();

    info!("AEGIS Threat Intelligence Service - Sprint 10");
    info!("Initializing P2P network and eBPF integration...");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    let interface = if args.len() > 1 {
        args[1].clone()
    } else {
        "lo".to_string()
    };

    let port = if args.len() > 2 {
        args[2].parse().unwrap_or(9001)
    } else {
        9001
    };

    let ebpf_program_path = if args.len() > 3 {
        args[3].clone()
    } else {
        "ebpf/syn-flood-filter/target/bpfel-unknown-none/release/syn-flood-filter".to_string()
    };

    // Create configuration
    let mut config = ThreatIntelConfig::default();
    config.interface = interface;
    config.p2p_config.listen_port = port;
    config.ebpf_program_path = ebpf_program_path;
    config.min_severity = 5;
    config.auto_publish = true;

    info!("Configuration:");
    info!("  Interface: {}", config.interface);
    info!("  P2P Port: {}", config.p2p_config.listen_port);
    info!("  eBPF Program: {}", config.ebpf_program_path);
    info!("  Min Severity: {}", config.min_severity);
    info!("  Auto-publish: {}", config.auto_publish);

    // Create threat intelligence service
    let service = match ThreatIntelService::new(config) {
        Ok(s) => {
            info!("Threat Intelligence Service started successfully!");
            s
        }
        Err(e) => {
            error!("Failed to start service: {}", e);
            error!("Make sure you have:");
            error!("  1. Built the eBPF program");
            error!("  2. Sufficient permissions (run as root or with CAP_NET_ADMIN)");
            error!("  3. Specified a valid network interface");
            return Err(e);
        }
    };

    info!("");
    info!("Service is running!");
    info!("Commands:");
    info!("  stats        - Show eBPF statistics");
    info!("  list         - List blocklisted IPs");
    info!("  block <ip> <duration> <type> <severity> - Blocklist an IP and publish");
    info!("  unblock <ip> - Remove IP from blocklist");
    info!("  publish <ip> <duration> <type> <severity> - Publish threat without local block");
    info!("  check <ip>   - Check if IP is blocklisted");
    info!("  quit         - Exit");
    info!("");

    // Command loop
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let mut input = String::new();
        stdin.read_line(&mut input)?;

        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "stats" => {
                match service.get_stats() {
                    Ok(stats) => {
                        info!("=== eBPF DDoS Statistics ===");
                        info!("Total packets:   {}", stats.total_packets);
                        info!("SYN packets:     {}", stats.syn_packets);
                        info!("Dropped packets: {}", stats.dropped_packets);
                        info!("Passed packets:  {}", stats.passed_packets);
                        info!("Drop rate:       {:.2}%", stats.drop_rate());
                        info!("SYN percentage:  {:.2}%", stats.syn_percentage());
                    }
                    Err(e) => error!("Failed to get stats: {}", e),
                }
            }

            "list" => {
                match service.get_blocklist() {
                    Ok(list) => {
                        if list.is_empty() {
                            info!("No IPs currently blocklisted");
                        } else {
                            info!("=== Blocklisted IPs ===");
                            for (ip, expires) in list {
                                info!("  {} (expires at: {}us)", ip, expires);
                            }
                        }
                    }
                    Err(e) => error!("Failed to get blocklist: {}", e),
                }
            }

            "block" => {
                if parts.len() < 5 {
                    error!("Usage: block <ip> <duration_secs> <threat_type> <severity>");
                    continue;
                }

                let ip = parts[1].to_string();
                let duration: u64 = match parts[2].parse() {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Invalid duration: {}", e);
                        continue;
                    }
                };
                let threat_type = parts[3].to_string();
                let severity: u8 = match parts[4].parse() {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Invalid severity: {}", e);
                        continue;
                    }
                };

                match service.blocklist_and_publish(
                    ip.clone(),
                    threat_type,
                    severity,
                    duration,
                    "local-node".to_string(),
                ) {
                    Ok(()) => info!("Blocklisted {} and published to network", ip),
                    Err(e) => error!("Failed to blocklist: {}", e),
                }
            }

            "unblock" => {
                if parts.len() < 2 {
                    error!("Usage: unblock <ip>");
                    continue;
                }

                let ip = parts[1];
                match service.remove_from_blocklist(ip) {
                    Ok(()) => info!("Removed {} from blocklist", ip),
                    Err(e) => error!("Failed to remove: {}", e),
                }
            }

            "publish" => {
                if parts.len() < 5 {
                    error!("Usage: publish <ip> <duration_secs> <threat_type> <severity>");
                    continue;
                }

                let ip = parts[1].to_string();
                let duration: u64 = match parts[2].parse() {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Invalid duration: {}", e);
                        continue;
                    }
                };
                let threat_type = parts[3].to_string();
                let severity: u8 = match parts[4].parse() {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Invalid severity: {}", e);
                        continue;
                    }
                };

                let threat = ThreatIntelligence::new(
                    ip.clone(),
                    threat_type,
                    severity,
                    duration,
                    "local-node".to_string(),
                );

                match service.publish_threat(threat) {
                    Ok(()) => info!("Published threat for {} to network", ip),
                    Err(e) => error!("Failed to publish: {}", e),
                }
            }

            "check" => {
                if parts.len() < 2 {
                    error!("Usage: check <ip>");
                    continue;
                }

                let ip = parts[1];
                match service.is_blocklisted(ip) {
                    Ok(is_blocked) => {
                        if is_blocked {
                            info!("{} is BLOCKLISTED", ip);
                        } else {
                            info!("{} is NOT blocklisted", ip);
                        }
                    }
                    Err(e) => error!("Failed to check: {}", e),
                }
            }

            "quit" | "exit" => {
                info!("Shutting down...");
                break;
            }

            _ => {
                error!("Unknown command: {}", parts[0]);
                error!("Type 'help' for available commands");
            }
        }
    }

    Ok(())
}

// Non-Linux stub
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Error: aegis-threat-intel is only available on Linux");
    eprintln!("This service requires eBPF/XDP which is Linux kernel-specific");
    std::process::exit(1);
}
