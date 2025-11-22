mod config;
mod proxy;

use anyhow::Result;
use std::fs;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
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

    let config: proxy::ProxyConfig = if config_str.is_empty() {
        proxy::ProxyConfig::default()
    } else {
        toml::from_str(&config_str)?
    };

    tracing::info!("╔════════════════════════════════════════════╗");
    tracing::info!("║   AEGIS Edge Node - Reverse Proxy v0.2    ║");
    tracing::info!("║         Sprint 3: HTTP/S & TLS             ║");
    tracing::info!("╚════════════════════════════════════════════╝");
    tracing::info!("");
    tracing::info!("HTTP:   {}", config.http_addr);
    if let Some(https) = &config.https_addr {
        tracing::info!("HTTPS:  {} (TLS enabled)", https);
    }
    tracing::info!("Origin: {}", config.origin);
    tracing::info!("");
    tracing::info!("Access logs enabled: {}", config.log_requests);
    tracing::info!("");

    // Run HTTP proxy
    proxy::run_http_proxy(config).await?;

    Ok(())
}
