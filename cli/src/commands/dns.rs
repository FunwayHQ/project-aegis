//! DNS management commands for AEGIS CLI
//!
//! Sprint 30.6: Provides commands to manage DNS zones and records via the
//! AEGIS DNS API server.

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_DNS_API_URL: &str = "http://localhost:8054";

// =============================================================================
// SUBCOMMANDS
// =============================================================================

#[derive(Subcommand)]
pub enum DnsCommands {
    /// List all DNS zones
    ListZones,

    /// Create a new DNS zone
    CreateZone {
        /// Domain name (e.g., "example.com")
        #[arg(long)]
        domain: String,

        /// Enable proxying through AEGIS (default: true)
        #[arg(long, default_value = "true")]
        proxied: bool,
    },

    /// Delete a DNS zone
    DeleteZone {
        /// Domain name to delete
        domain: String,
    },

    /// List records for a zone
    ListRecords {
        /// Domain name
        domain: String,
    },

    /// Add a DNS record
    AddRecord {
        /// Domain name
        #[arg(long)]
        domain: String,

        /// Record name (e.g., "www" or "@" for root)
        #[arg(long)]
        name: String,

        /// Record type (A, AAAA, CNAME, MX, TXT, NS, CAA, SRV)
        #[arg(long, short = 't')]
        record_type: String,

        /// Record value
        #[arg(long)]
        value: String,

        /// TTL in seconds (default: 300)
        #[arg(long, default_value = "300")]
        ttl: u32,

        /// Priority (for MX/SRV records)
        #[arg(long)]
        priority: Option<u16>,

        /// Proxy through AEGIS (for A/AAAA/CNAME only)
        #[arg(long, default_value = "false")]
        proxied: bool,
    },

    /// Delete a DNS record
    DeleteRecord {
        /// Domain name
        domain: String,

        /// Record ID
        record_id: String,
    },

    /// Show DNSSEC status and DS record
    DnssecStatus {
        /// Domain name
        domain: String,
    },

    /// Enable DNSSEC for a zone
    EnableDnssec {
        /// Domain name
        domain: String,
    },

    /// Disable DNSSEC for a zone
    DisableDnssec {
        /// Domain name
        domain: String,
    },

    /// Show DNS server statistics
    Stats,

    /// Show AEGIS nameservers
    Nameservers,
}

// =============================================================================
// API TYPES
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Zone {
    domain: String,
    proxied: bool,
    dnssec_enabled: bool,
    nameservers: Vec<String>,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct DnsRecord {
    id: String,
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    value: String,
    ttl: u32,
    priority: Option<u16>,
    proxied: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateZoneRequest {
    domain: String,
    proxied: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateRecordRequest {
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    value: String,
    ttl: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<u16>,
    proxied: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DnssecStatus {
    enabled: bool,
    algorithm: Option<String>,
    key_tag: Option<u16>,
    ds_record: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DnsStats {
    total_queries: u64,
    queries_today: u64,
    cache_hit_rate: f64,
    top_queried_domains: Vec<TopDomain>,
    query_types: HashMap<String, u64>,
    rate_limited_queries: u64,
    dnssec_queries: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TopDomain {
    domain: String,
    count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct NameserverInfo {
    primary: String,
    secondary: Vec<String>,
    anycast_ips: AnycastIps,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnycastIps {
    ipv4: Vec<String>,
    ipv6: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    message: Option<String>,
    data: Option<T>,
    error: Option<String>,
}

// =============================================================================
// API CLIENT
// =============================================================================

struct DnsApiClient {
    client: Client,
    base_url: String,
}

impl DnsApiClient {
    fn new(base_url: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.unwrap_or(DEFAULT_DNS_API_URL).to_string(),
        }
    }

    async fn list_zones(&self) -> Result<Vec<Zone>> {
        let resp: ApiResponse<Vec<Zone>> = self
            .client
            .get(format!("{}/aegis/dns/api/zones", self.base_url))
            .send()
            .await?
            .json()
            .await?;

        resp.data.context("Failed to fetch zones")
    }

    async fn create_zone(&self, domain: &str, proxied: bool) -> Result<Zone> {
        let req = CreateZoneRequest {
            domain: domain.to_string(),
            proxied,
        };

        let resp: ApiResponse<Zone> = self
            .client
            .post(format!("{}/aegis/dns/api/zones", self.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            anyhow::bail!(resp.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        resp.data.context("Failed to create zone")
    }

    async fn delete_zone(&self, domain: &str) -> Result<()> {
        let resp: ApiResponse<()> = self
            .client
            .delete(format!("{}/aegis/dns/api/zones/{}", self.base_url, domain))
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            anyhow::bail!(resp.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        Ok(())
    }

    async fn list_records(&self, domain: &str) -> Result<Vec<DnsRecord>> {
        let resp: ApiResponse<Vec<DnsRecord>> = self
            .client
            .get(format!(
                "{}/aegis/dns/api/zones/{}/records",
                self.base_url, domain
            ))
            .send()
            .await?
            .json()
            .await?;

        resp.data.context("Failed to fetch records")
    }

    async fn create_record(&self, domain: &str, req: CreateRecordRequest) -> Result<DnsRecord> {
        let resp: ApiResponse<DnsRecord> = self
            .client
            .post(format!(
                "{}/aegis/dns/api/zones/{}/records",
                self.base_url, domain
            ))
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            anyhow::bail!(resp.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        resp.data.context("Failed to create record")
    }

    async fn delete_record(&self, domain: &str, record_id: &str) -> Result<()> {
        let resp: ApiResponse<()> = self
            .client
            .delete(format!(
                "{}/aegis/dns/api/zones/{}/records/{}",
                self.base_url, domain, record_id
            ))
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            anyhow::bail!(resp.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        Ok(())
    }

    async fn get_dnssec_status(&self, domain: &str) -> Result<DnssecStatus> {
        let resp: ApiResponse<DnssecStatus> = self
            .client
            .get(format!(
                "{}/aegis/dns/api/zones/{}/dnssec",
                self.base_url, domain
            ))
            .send()
            .await?
            .json()
            .await?;

        resp.data.context("Failed to fetch DNSSEC status")
    }

    async fn enable_dnssec(&self, domain: &str) -> Result<DnssecStatus> {
        let resp: ApiResponse<DnssecStatus> = self
            .client
            .post(format!(
                "{}/aegis/dns/api/zones/{}/dnssec/enable",
                self.base_url, domain
            ))
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            anyhow::bail!(resp.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        resp.data.context("Failed to enable DNSSEC")
    }

    async fn disable_dnssec(&self, domain: &str) -> Result<()> {
        let resp: ApiResponse<()> = self
            .client
            .post(format!(
                "{}/aegis/dns/api/zones/{}/dnssec/disable",
                self.base_url, domain
            ))
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            anyhow::bail!(resp.error.unwrap_or_else(|| "Unknown error".to_string()));
        }

        Ok(())
    }

    async fn get_stats(&self) -> Result<DnsStats> {
        let resp: ApiResponse<DnsStats> = self
            .client
            .get(format!("{}/aegis/dns/api/stats", self.base_url))
            .send()
            .await?
            .json()
            .await?;

        resp.data.context("Failed to fetch stats")
    }

    async fn get_nameservers(&self) -> Result<NameserverInfo> {
        let resp: ApiResponse<NameserverInfo> = self
            .client
            .get(format!("{}/aegis/dns/api/nameservers", self.base_url))
            .send()
            .await?
            .json()
            .await?;

        resp.data.context("Failed to fetch nameservers")
    }
}

// =============================================================================
// COMMAND HANDLERS
// =============================================================================

pub async fn execute(cmd: DnsCommands) -> Result<()> {
    let client = DnsApiClient::new(None);

    match cmd {
        DnsCommands::ListZones => list_zones(&client).await,
        DnsCommands::CreateZone { domain, proxied } => create_zone(&client, &domain, proxied).await,
        DnsCommands::DeleteZone { domain } => delete_zone(&client, &domain).await,
        DnsCommands::ListRecords { domain } => list_records(&client, &domain).await,
        DnsCommands::AddRecord {
            domain,
            name,
            record_type,
            value,
            ttl,
            priority,
            proxied,
        } => {
            add_record(
                &client,
                &domain,
                &name,
                &record_type,
                &value,
                ttl,
                priority,
                proxied,
            )
            .await
        }
        DnsCommands::DeleteRecord { domain, record_id } => {
            delete_record(&client, &domain, &record_id).await
        }
        DnsCommands::DnssecStatus { domain } => dnssec_status(&client, &domain).await,
        DnsCommands::EnableDnssec { domain } => enable_dnssec(&client, &domain).await,
        DnsCommands::DisableDnssec { domain } => disable_dnssec(&client, &domain).await,
        DnsCommands::Stats => show_stats(&client).await,
        DnsCommands::Nameservers => show_nameservers(&client).await,
    }
}

async fn list_zones(client: &DnsApiClient) -> Result<()> {
    println!("{}", "Fetching DNS zones...".bright_cyan());
    println!();

    let zones = client.list_zones().await?;

    if zones.is_empty() {
        println!("{}", "No zones configured yet.".yellow());
        println!();
        println!(
            "Use {} to add your first zone.",
            "aegis-cli dns create-zone --domain example.com".bright_green()
        );
        return Ok(());
    }

    println!(
        "{:<30} {:<10} {:<10} {}",
        "DOMAIN".bold(),
        "PROXIED".bold(),
        "DNSSEC".bold(),
        "NAMESERVERS".bold()
    );
    println!("{}", "-".repeat(80));

    for zone in zones {
        println!(
            "{:<30} {:<10} {:<10} {}",
            zone.domain.bright_white(),
            if zone.proxied {
                "Yes".green()
            } else {
                "No".red()
            },
            if zone.dnssec_enabled {
                "Yes".green()
            } else {
                "No".yellow()
            },
            zone.nameservers.join(", ").dimmed()
        );
    }

    Ok(())
}

async fn create_zone(client: &DnsApiClient, domain: &str, proxied: bool) -> Result<()> {
    println!(
        "{}",
        format!("Creating zone for {}...", domain).bright_cyan()
    );
    println!();

    let zone = client.create_zone(domain, proxied).await?;

    println!("{}", "Zone created successfully!".bright_green());
    println!();
    println!("  {} {}", "Domain:".bold(), zone.domain.bright_white());
    println!(
        "  {} {}",
        "Proxied:".bold(),
        if zone.proxied {
            "Yes".green()
        } else {
            "No".red()
        }
    );
    println!();
    println!("{}", "Nameservers (update at your registrar):".bright_yellow());
    for ns in &zone.nameservers {
        println!("  {}", ns.bright_cyan());
    }

    Ok(())
}

async fn delete_zone(client: &DnsApiClient, domain: &str) -> Result<()> {
    println!(
        "{}",
        format!("Deleting zone {}...", domain).bright_cyan()
    );

    client.delete_zone(domain).await?;

    println!(
        "{}",
        format!("Zone {} deleted successfully!", domain).bright_green()
    );

    Ok(())
}

async fn list_records(client: &DnsApiClient, domain: &str) -> Result<()> {
    println!(
        "{}",
        format!("Fetching records for {}...", domain).bright_cyan()
    );
    println!();

    let records = client.list_records(domain).await?;

    if records.is_empty() {
        println!("{}", "No records configured yet.".yellow());
        return Ok(());
    }

    println!(
        "{:<8} {:<20} {:<40} {:<8} {}",
        "TYPE".bold(),
        "NAME".bold(),
        "VALUE".bold(),
        "TTL".bold(),
        "PROXIED".bold()
    );
    println!("{}", "-".repeat(90));

    for record in records {
        let type_color = match record.record_type.as_str() {
            "A" => record.record_type.bright_blue(),
            "AAAA" => record.record_type.bright_magenta(),
            "CNAME" => record.record_type.bright_yellow(),
            "MX" => record.record_type.bright_green(),
            "TXT" => record.record_type.bright_cyan(),
            _ => record.record_type.white(),
        };

        let value = if record.value.len() > 38 {
            format!("{}...", &record.value[..35])
        } else {
            record.value.clone()
        };

        println!(
            "{:<8} {:<20} {:<40} {:<8} {}",
            type_color,
            record.name.bright_white(),
            value.dimmed(),
            format!("{}s", record.ttl),
            if record.proxied {
                "Yes".green()
            } else {
                "No".dimmed()
            }
        );
    }

    Ok(())
}

async fn add_record(
    client: &DnsApiClient,
    domain: &str,
    name: &str,
    record_type: &str,
    value: &str,
    ttl: u32,
    priority: Option<u16>,
    proxied: bool,
) -> Result<()> {
    println!(
        "{}",
        format!("Adding {} record to {}...", record_type, domain).bright_cyan()
    );

    let req = CreateRecordRequest {
        name: name.to_string(),
        record_type: record_type.to_uppercase(),
        value: value.to_string(),
        ttl,
        priority,
        proxied,
    };

    let record = client.create_record(domain, req).await?;

    println!("{}", "Record added successfully!".bright_green());
    println!();
    println!("  {} {}", "ID:".bold(), record.id.dimmed());
    println!("  {} {}", "Name:".bold(), record.name.bright_white());
    println!("  {} {}", "Type:".bold(), record.record_type.bright_cyan());
    println!("  {} {}", "Value:".bold(), record.value);
    println!("  {} {}s", "TTL:".bold(), record.ttl);
    if let Some(p) = record.priority {
        println!("  {} {}", "Priority:".bold(), p);
    }

    Ok(())
}

async fn delete_record(client: &DnsApiClient, domain: &str, record_id: &str) -> Result<()> {
    println!(
        "{}",
        format!("Deleting record {} from {}...", record_id, domain).bright_cyan()
    );

    client.delete_record(domain, record_id).await?;

    println!("{}", "Record deleted successfully!".bright_green());

    Ok(())
}

async fn dnssec_status(client: &DnsApiClient, domain: &str) -> Result<()> {
    println!(
        "{}",
        format!("Fetching DNSSEC status for {}...", domain).bright_cyan()
    );
    println!();

    let status = client.get_dnssec_status(domain).await?;

    println!(
        "  {} {}",
        "Enabled:".bold(),
        if status.enabled {
            "Yes".bright_green()
        } else {
            "No".yellow()
        }
    );

    if status.enabled {
        if let Some(algo) = &status.algorithm {
            println!("  {} {}", "Algorithm:".bold(), algo);
        }
        if let Some(tag) = status.key_tag {
            println!("  {} {}", "Key Tag:".bold(), tag);
        }
        if let Some(ds) = &status.ds_record {
            println!();
            println!(
                "{}",
                "DS Record (add to your registrar):".bright_yellow()
            );
            println!("  {}", ds.bright_cyan());
        }
    } else {
        println!();
        println!(
            "Use {} to enable DNSSEC",
            format!("aegis-cli dns enable-dnssec {}", domain).bright_green()
        );
    }

    Ok(())
}

async fn enable_dnssec(client: &DnsApiClient, domain: &str) -> Result<()> {
    println!(
        "{}",
        format!("Enabling DNSSEC for {}...", domain).bright_cyan()
    );
    println!();

    let status = client.enable_dnssec(domain).await?;

    println!("{}", "DNSSEC enabled successfully!".bright_green());
    println!();

    if let Some(ds) = &status.ds_record {
        println!(
            "{}",
            "DS Record (add this to your registrar):".bright_yellow()
        );
        println!();
        println!("  {}", ds.bright_cyan());
        println!();
        println!(
            "{}",
            "After adding the DS record, DNSSEC validation will be active.".dimmed()
        );
    }

    Ok(())
}

async fn disable_dnssec(client: &DnsApiClient, domain: &str) -> Result<()> {
    println!(
        "{}",
        format!("Disabling DNSSEC for {}...", domain).bright_cyan()
    );

    client.disable_dnssec(domain).await?;

    println!("{}", "DNSSEC disabled successfully.".bright_green());
    println!();
    println!(
        "{}",
        "Remember to remove the DS record from your registrar.".yellow()
    );

    Ok(())
}

async fn show_stats(client: &DnsApiClient) -> Result<()> {
    println!("{}", "Fetching DNS statistics...".bright_cyan());
    println!();

    let stats = client.get_stats().await?;

    println!("{}", "=== DNS Statistics ===".bold());
    println!();
    println!(
        "  {} {}",
        "Total Queries:".bold(),
        stats.total_queries.to_string().bright_white()
    );
    println!(
        "  {} {}",
        "Queries Today:".bold(),
        stats.queries_today.to_string().bright_cyan()
    );
    println!(
        "  {} {:.1}%",
        "Cache Hit Rate:".bold(),
        stats.cache_hit_rate * 100.0
    );
    println!(
        "  {} {}",
        "Rate Limited:".bold(),
        stats.rate_limited_queries.to_string().yellow()
    );
    println!(
        "  {} {}",
        "DNSSEC Queries:".bold(),
        stats.dnssec_queries.to_string().green()
    );

    if !stats.query_types.is_empty() {
        println!();
        println!("{}", "Query Types:".bold());
        for (qtype, count) in &stats.query_types {
            println!("  {} {}", format!("{:<6}", qtype), count);
        }
    }

    if !stats.top_queried_domains.is_empty() {
        println!();
        println!("{}", "Top Queried Domains:".bold());
        for (i, item) in stats.top_queried_domains.iter().enumerate() {
            println!(
                "  {}. {} ({})",
                i + 1,
                item.domain.bright_white(),
                item.count
            );
        }
    }

    Ok(())
}

async fn show_nameservers(client: &DnsApiClient) -> Result<()> {
    println!("{}", "AEGIS Nameservers".bold());
    println!();

    let info = client.get_nameservers().await?;

    println!("{}", "Nameserver Hostnames:".bold());
    println!("  {} {}", "Primary:".dimmed(), info.primary.bright_cyan());
    for ns in &info.secondary {
        println!("  {} {}", "Secondary:".dimmed(), ns.bright_cyan());
    }

    println!();
    println!("{}", "Anycast IPs:".bold());
    println!("  {} {:?}", "IPv4:".dimmed(), info.anycast_ips.ipv4);
    println!("  {} {:?}", "IPv6:".dimmed(), info.anycast_ips.ipv6);

    println!();
    println!(
        "{}",
        "Update your domain's nameservers at your registrar to use AEGIS DNS."
            .bright_yellow()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_zone_request_serialization() {
        let req = CreateZoneRequest {
            domain: "example.com".to_string(),
            proxied: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("example.com"));
        assert!(json.contains("proxied"));
    }

    #[test]
    fn test_create_record_request_serialization() {
        let req = CreateRecordRequest {
            name: "www".to_string(),
            record_type: "A".to_string(),
            value: "192.168.1.1".to_string(),
            ttl: 300,
            priority: None,
            proxied: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("www"));
        assert!(json.contains("192.168.1.1"));
        assert!(!json.contains("priority")); // skip_serializing_if = None
    }

    #[test]
    fn test_create_record_request_with_priority() {
        let req = CreateRecordRequest {
            name: "@".to_string(),
            record_type: "MX".to_string(),
            value: "mail.example.com".to_string(),
            ttl: 3600,
            priority: Some(10),
            proxied: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("priority"));
        assert!(json.contains("10"));
    }
}
