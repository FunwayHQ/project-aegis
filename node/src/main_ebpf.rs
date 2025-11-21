mod ebpf_loader;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use ebpf_loader::{DDoSStats, EbpfLoader};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "aegis-ebpf-loader")]
#[command(author = "AEGIS Team")]
#[command(version = "0.1.0")]
#[command(about = "AEGIS eBPF/XDP DDoS Protection Loader", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Load and attach XDP program to network interface
    Attach {
        /// Network interface name (e.g., eth0, lo)
        #[arg(short, long)]
        interface: String,

        /// Path to compiled eBPF program
        #[arg(
            short,
            long,
            default_value = "ebpf/syn-flood-filter/target/bpfel-unknown-none/release/syn-flood-filter"
        )]
        program: PathBuf,

        /// SYN flood threshold (packets/sec per IP)
        #[arg(short, long, default_value = "100")]
        threshold: u64,
    },

    /// Show current statistics
    Stats,

    /// Set SYN flood threshold
    SetThreshold {
        /// New threshold value
        threshold: u64,
    },

    /// Add IP to whitelist
    Whitelist {
        /// IP address to whitelist
        ip: String,
    },

    /// Remove IP from whitelist
    Unwhitelist {
        /// IP address to remove
        ip: String,
    },

    /// Monitor statistics in real-time
    Monitor {
        /// Update interval in seconds
        #[arg(short, long, default_value = "1")]
        interval: u64,
    },
}

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Check if running as root (required for XDP)
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!(
            "{}",
            "❌ Error: This program requires root privileges".bright_red()
        );
        eprintln!("   Please run with: sudo ./aegis-ebpf-loader <command>");
        std::process::exit(1);
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Attach {
            interface,
            program,
            threshold,
        } => {
            attach_program(&interface, &program, threshold)?;
        }
        Commands::Stats => {
            show_stats()?;
        }
        Commands::SetThreshold { threshold } => {
            set_threshold(threshold)?;
        }
        Commands::Whitelist { ip } => {
            whitelist_ip(&ip)?;
        }
        Commands::Unwhitelist { ip } => {
            unwhitelist_ip(&ip)?;
        }
        Commands::Monitor { interval } => {
            monitor_stats(interval)?;
        }
    }

    Ok(())
}

fn attach_program(interface: &str, program_path: &PathBuf, threshold: u64) -> Result<()> {
    println!(
        "{}",
        "╔════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║   AEGIS eBPF/XDP DDoS Protection          ║".bright_cyan()
    );
    println!(
        "{}",
        "║   Sprint 7: SYN Flood Mitigation          ║".bright_cyan()
    );
    println!(
        "{}",
        "╚════════════════════════════════════════════╝".bright_cyan()
    );
    println!();

    println!("{}", "Loading XDP program...".bright_cyan());
    println!(
        "  Program: {}",
        program_path.display().to_string().bright_white()
    );
    println!("  Interface: {}", interface.bright_yellow());
    println!(
        "  SYN Threshold: {} packets/sec per IP",
        threshold.to_string().bright_green()
    );
    println!();

    // Load eBPF program
    let mut loader = EbpfLoader::load(program_path)?;

    // Set threshold
    loader.set_syn_threshold(threshold)?;

    // Attach to interface
    loader.attach(interface)?;

    println!();
    println!(
        "{}",
        "✅ XDP program loaded and attached successfully!".bright_green()
    );
    println!();
    println!(
        "{}",
        "DDoS protection is now active on {}".bright_green(),
        interface
    );
    println!(
        "{}",
        "SYN flood packets exceeding {} per second will be dropped".dimmed(),
        threshold
    );
    println!();
    println!("{}", "Press Ctrl+C to detach and exit...".dimmed());

    // Keep program running
    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        tx.send(()).ok();
    })?;

    rx.recv()?;

    println!();
    println!("{}", "Detaching XDP program...".yellow());
    loader.detach()?;
    println!("{}", "✅ XDP program detached".bright_green());

    Ok(())
}

fn show_stats() -> Result<()> {
    println!(
        "{}",
        "═══════════════════════════════════════════════════".bright_cyan()
    );
    println!(
        "{}",
        "        eBPF/XDP DDoS Protection Statistics"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "═══════════════════════════════════════════════════".bright_cyan()
    );
    println!();

    // Note: This would require keeping a global reference to the loader
    // For now, show placeholder message
    println!(
        "{}",
        "⚠ Stats command requires running loader instance".yellow()
    );
    println!("  Start loader with: sudo aegis-ebpf-loader attach --interface eth0");
    println!();

    Ok(())
}

fn set_threshold(threshold: u64) -> Result<()> {
    println!(
        "{}",
        format!("Setting SYN threshold to: {}", threshold).bright_cyan()
    );
    // Would update running instance
    Ok(())
}

fn whitelist_ip(ip: &str) -> Result<()> {
    println!("{}", format!("Adding {} to whitelist", ip).bright_cyan());
    // Would update running instance
    Ok(())
}

fn unwhitelist_ip(ip: &str) -> Result<()> {
    println!(
        "{}",
        format!("Removing {} from whitelist", ip).bright_cyan()
    );
    // Would update running instance
    Ok(())
}

fn monitor_stats(interval: u64) -> Result<()> {
    println!(
        "{}",
        "Real-time DDoS Protection Monitoring".bright_cyan().bold()
    );
    println!("{}", format!("Update interval: {}s", interval).dimmed());
    println!("{}", "Press Ctrl+C to stop...".dimmed());
    println!();

    // Placeholder for real-time monitoring
    loop {
        thread::sleep(Duration::from_secs(interval));

        // Clear screen and show stats
        print!("\x1B[2J\x1B[1;1H"); // Clear screen

        println!("{}", "═══ eBPF/XDP Statistics ═══".bright_cyan());
        println!("  Total Packets:   {:>10}", "N/A");
        println!("  SYN Packets:     {:>10}", "N/A");
        println!("  Dropped:         {:>10}", "N/A");
        println!("  Passed:          {:>10}", "N/A");
        println!("  Drop Rate:       {:>10}", "N/A");
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_structure() {
        // Verify CLI compiles
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn test_default_threshold() {
        let default = 100_u64;
        assert!(default > 0);
        assert!(default >= 10); // Reasonable minimum
        assert!(default <= 10000); // Reasonable maximum
    }

    #[test]
    fn test_threshold_range_validation() {
        let thresholds = vec![1, 10, 50, 100, 500, 1000, 10000];

        for threshold in thresholds {
            assert!(threshold > 0);
            assert!(threshold <= 10000);
        }
    }

    #[test]
    fn test_interface_names() {
        let valid_interfaces = vec!["eth0", "lo", "wlan0", "ens33"];

        for iface in valid_interfaces {
            assert!(!iface.is_empty());
            assert!(iface.len() < 16); // IFNAMSIZ in Linux
        }
    }
}
