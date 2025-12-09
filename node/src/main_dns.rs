//! AEGIS DNS Server Entry Point
//!
//! Sprint 30.1: DNS Core Server
//! Sprint 30.2: DNS Management API & Usage Metering
//!
//! This binary runs the AEGIS authoritative DNS server with:
//! - UDP and TCP DNS on port 53
//! - Rate limiting for DoS protection
//! - Integration with zone store
//! - HTTP API for zone/record management
//! - SQLite persistence for zone durability
//! - Usage metering and analytics
//! - Account tier management
//!
//! ## Usage
//!
//! ```bash
//! # Run with default configuration
//! aegis-dns
//!
//! # Run with custom config file
//! aegis-dns --config /path/to/config.toml
//!
//! # Run on non-standard ports (for testing without root)
//! aegis-dns --udp-port 5053 --tcp-port 5053
//!
//! # Specify data directory for persistence
//! aegis-dns --data-dir /var/lib/aegis-dns
//! ```
//!
//! ## Configuration
//!
//! The server can be configured via:
//! - Command line arguments
//! - TOML configuration file
//! - Environment variables (AEGIS_DNS_* prefix)

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use aegis_node::dns::{
    AccountManager, DnsApi, DnsConfig, DnsMetering, DnsPersistence, DnsRecord, DnsServer, Zone,
    ZoneStore,
};

/// AEGIS DNS Server
#[derive(Parser, Debug)]
#[command(name = "aegis-dns")]
#[command(author = "AEGIS Team")]
#[command(version = "0.1.0")]
#[command(about = "AEGIS Authoritative DNS Server", long_about = None)]
struct Args {
    /// Path to configuration file (TOML format)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// UDP listen port (default: 53)
    #[arg(long, default_value = "53")]
    udp_port: u16,

    /// TCP listen port (default: 53)
    #[arg(long, default_value = "53")]
    tcp_port: u16,

    /// API server port (default: 8054)
    #[arg(long, default_value = "8054")]
    api_port: u16,

    /// Bind address (default: 0.0.0.0)
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    /// Data directory for persistence (default: ./data/dns)
    #[arg(long, default_value = "./data/dns")]
    data_dir: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Disable rate limiting (for testing)
    #[arg(long)]
    no_rate_limit: bool,

    /// Disable persistence (in-memory only)
    #[arg(long)]
    no_persistence: bool,

    /// Create example zone for testing
    #[arg(long)]
    example_zone: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!(
        "Starting AEGIS DNS Server v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Load or create configuration
    let mut config = if let Some(config_path) = &args.config {
        let content = std::fs::read_to_string(config_path)?;
        DnsConfig::from_toml(&content)?
    } else {
        DnsConfig::default()
    };

    // Override with command line arguments
    config.udp_addr = format!("{}:{}", args.bind, args.udp_port).parse()?;
    config.tcp_addr = format!("{}:{}", args.bind, args.tcp_port).parse()?;
    config.api.addr = format!("{}:{}", args.bind, args.api_port).parse()?;

    if args.no_rate_limit {
        config.rate_limit.enabled = false;
        info!("Rate limiting disabled");
    }

    // Validate configuration
    config.validate()?;

    // Create data directory if needed
    if !args.no_persistence {
        std::fs::create_dir_all(&args.data_dir)?;
        info!("Data directory: {}", args.data_dir.display());
    }

    // Create zone store
    let zone_store = Arc::new(ZoneStore::new());

    // Initialize persistence layer
    let persistence = if args.no_persistence {
        info!("Persistence disabled (in-memory only)");
        None
    } else {
        let db_path = args.data_dir.join("zones.db");
        match DnsPersistence::new(db_path.to_str().unwrap()) {
            Ok(p) => {
                // Restore zones from persistence
                let count = p.restore_to_store(&zone_store).await?;
                info!("Restored {} zones from persistence", count);
                Some(Arc::new(p))
            }
            Err(e) => {
                warn!("Failed to initialize persistence: {}. Running in-memory only.", e);
                None
            }
        }
    };

    // Initialize metering (will be used by DNS server for query tracking)
    let _metering = if args.no_persistence {
        Arc::new(DnsMetering::new())
    } else {
        let metering_db = args.data_dir.join("metering.db");
        match DnsMetering::with_persistence(metering_db.to_str().unwrap()) {
            Ok(m) => Arc::new(m),
            Err(e) => {
                warn!("Failed to initialize metering persistence: {}. Using in-memory.", e);
                Arc::new(DnsMetering::new())
            }
        }
    };

    // Initialize account manager (will be used for tier enforcement)
    let _account_manager = Arc::new(AccountManager::new());

    // Create example zone if requested
    if args.example_zone {
        create_example_zone(&zone_store).await?;
    }

    // Create DNS API server
    let api = Arc::new(DnsApi::new(zone_store.clone(), config.clone()));
    let api_addr = config.api.addr;

    // Spawn API server
    let api_clone = api.clone();
    tokio::spawn(async move {
        info!("Starting DNS API server on {}", api_addr);
        if let Err(e) = api_clone.run(api_addr).await {
            error!("DNS API server error: {}", e);
        }
    });

    // Create DNS server
    let server = DnsServer::new(config.clone(), zone_store)?;

    info!("DNS server configuration:");
    info!("  UDP: {}", config.udp_addr);
    info!("  TCP: {}", config.tcp_addr);
    info!("  API: {}", config.api.addr);
    info!(
        "  Rate limit: {} qps (burst: {})",
        config.rate_limit.queries_per_second, config.rate_limit.burst_size
    );
    info!("  Persistence: {}", if persistence.is_some() { "enabled" } else { "disabled" });

    // Run DNS server (blocks)
    if let Err(e) = server.run().await {
        error!("DNS server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

/// Create an example zone for testing
async fn create_example_zone(zone_store: &ZoneStore) -> anyhow::Result<()> {
    info!("Creating example zone: example.aegis.local");

    let mut zone = Zone::new("example.aegis.local", false);
    zone.create_default_records(&[
        "ns1.aegis.network".to_string(),
        "ns2.aegis.network".to_string(),
    ]);

    // Add some example records
    zone.add_record(DnsRecord::a("@", "192.168.1.1".parse()?, 300));
    zone.add_record(DnsRecord::a("www", "192.168.1.2".parse()?, 300));
    zone.add_record(DnsRecord::a("api", "192.168.1.3".parse()?, 300));
    zone.add_record(DnsRecord::cname("cdn", "www.example.aegis.local", 300));
    zone.add_record(DnsRecord::mx("@", "mail.example.aegis.local", 10, 300));
    zone.add_record(DnsRecord::txt("@", "v=spf1 include:_spf.aegis.network ~all", 300));

    zone_store.upsert_zone(zone).await?;

    info!("Example zone created with {} records", 6);
    info!("Test with: dig @127.0.0.1 -p {} www.example.aegis.local A",
          std::env::args().find(|a| a.starts_with("--udp-port")).map(|_| "5053").unwrap_or("53"));

    Ok(())
}
