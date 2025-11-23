use anyhow::Result;
use aegis_node::pingora_proxy::ProxyConfig;
use std::fs;
use tracing_subscriber;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config_str = fs::read_to_string(&config_path).unwrap_or_else(|_| {
        tracing::info!("Config file not found, using defaults");
        String::new()
    });

    let config: ProxyConfig = if config_str.is_empty() {
        ProxyConfig::default()
    } else {
        toml::from_str(&config_str)?
    };

    tracing::info!("╔════════════════════════════════════════════╗");
    tracing::info!("║     AEGIS Edge Node - Pingora Proxy       ║");
    tracing::info!("╚════════════════════════════════════════════╝");
    tracing::info!("");
    tracing::info!("HTTP:  {}", config.http_addr);
    if let Some(https) = &config.https_addr {
        tracing::info!("HTTPS: {}", https);
    }
    tracing::info!("Origin: {}", config.origin);
    tracing::info!("");

    // Run the proxy
    aegis_node::pingora_proxy::run_proxy(config)?;

    Ok(())
}
