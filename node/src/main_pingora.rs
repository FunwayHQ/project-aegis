mod cache;
mod config;
mod pingora_proxy;

use anyhow::Result;
use pingora_proxy::ProxyConfig;
use std::fs;

fn main() -> Result<()> {
    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config_str = fs::read_to_string(&config_path).unwrap_or_else(|_| {
        println!("Config file not found, using defaults");
        String::new()
    });

    let config: ProxyConfig = if config_str.is_empty() {
        ProxyConfig::default()
    } else {
        toml::from_str(&config_str)?
    };

    println!("╔════════════════════════════════════════════╗");
    println!("║     AEGIS Edge Node - Pingora Proxy       ║");
    println!("╚════════════════════════════════════════════╝");
    println!();
    println!("HTTP:  {}", config.http_addr);
    if let Some(https) = &config.https_addr {
        println!("HTTPS: {}", https);
    }
    println!("Origin: {}", config.origin);
    println!();

    // Run the proxy
    pingora_proxy::run_proxy(config)?;

    Ok(())
}
