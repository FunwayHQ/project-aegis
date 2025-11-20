use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};

/// Node metrics response from /metrics endpoint
#[derive(Debug, Deserialize, Serialize)]
struct NodeMetricsResponse {
    system: SystemMetrics,
    network: NetworkMetrics,
    performance: PerformanceMetrics,
    cache: CacheMetrics,
    status: StatusMetrics,
    timestamp: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct SystemMetrics {
    cpu_usage_percent: f32,
    memory_used_mb: u64,
    memory_total_mb: u64,
    memory_percent: f32,
}

#[derive(Debug, Deserialize, Serialize)]
struct NetworkMetrics {
    active_connections: u64,
    requests_total: u64,
    requests_per_second: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct PerformanceMetrics {
    avg_latency_ms: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheMetrics {
    hit_rate: f64,
    hits: u64,
    misses: u64,
    memory_mb: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct StatusMetrics {
    proxy: String,
    cache: String,
    uptime_seconds: u64,
}

/// Execute metrics display command
pub async fn execute(node_url: Option<String>) -> Result<()> {
    let url = node_url.unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let metrics_url = format!("{}/metrics", url);

    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!("{}", "        AEGIS Node Metrics Dashboard".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!();

    // Fetch metrics from node
    println!("{}", format!("Fetching metrics from {}...", url).dimmed());
    println!();

    let response = match reqwest::get(&metrics_url).await {
        Ok(resp) => resp,
        Err(e) => {
            println!("{}", "❌ Failed to connect to node".bright_red());
            println!("  Error: {}", e);
            println!();
            println!("{}", "Troubleshooting:".bright_yellow());
            println!("  • Ensure the AEGIS node is running");
            println!("  • Check the node URL: {}", url);
            println!("  • Verify the node is accessible");
            println!();
            println!("  Start node with: cd node && cargo run");
            return Err(e.into());
        }
    };

    let metrics: NodeMetricsResponse = match response.json().await {
        Ok(m) => m,
        Err(e) => {
            println!("{}", "❌ Failed to parse metrics".bright_red());
            println!("  Error: {}", e);
            return Err(e.into());
        }
    };

    // Display System Metrics
    println!("{}", "═══ System Resources ═══".bright_cyan());
    println!("  CPU Usage:     {}%", format_percent(metrics.system.cpu_usage_percent));
    println!(
        "  Memory:        {} MB / {} MB ({}%)",
        metrics.system.memory_used_mb.to_string().bright_white(),
        metrics.system.memory_total_mb.to_string().dimmed(),
        format_percent(metrics.system.memory_percent)
    );
    println!();

    // Display Network Metrics
    println!("{}", "═══ Network Activity ═══".bright_cyan());
    println!("  Active Connections: {}", metrics.network.active_connections.to_string().bright_green());
    println!("  Total Requests:     {}", metrics.network.requests_total.to_string().bright_white());
    println!("  Requests/Second:    {}", format!("{:.2}", metrics.network.requests_per_second).bright_white());
    println!();

    // Display Performance Metrics
    println!("{}", "═══ Performance (Latency) ═══".bright_cyan());
    println!("  Average:       {} ms", format_latency(metrics.performance.avg_latency_ms));
    println!("  P50 (Median):  {} ms", format_latency(metrics.performance.p50_latency_ms));
    println!("  P95:           {} ms", format_latency(metrics.performance.p95_latency_ms));
    println!("  P99:           {} ms", format_latency(metrics.performance.p99_latency_ms));
    println!();

    // Display Cache Metrics
    println!("{}", "═══ Cache Performance ═══".bright_cyan());
    println!("  Hit Rate:      {}%", format_percent(metrics.cache.hit_rate as f32));
    println!("  Hits:          {}", metrics.cache.hits.to_string().bright_green());
    println!("  Misses:        {}", metrics.cache.misses.to_string().yellow());
    println!("  Memory Used:   {} MB", metrics.cache.memory_mb.to_string().bright_white());

    let total_cache_ops = metrics.cache.hits + metrics.cache.misses;
    if total_cache_ops > 0 {
        println!("  Total Ops:     {}", total_cache_ops.to_string().dimmed());
    }
    println!();

    // Display Status
    println!("{}", "═══ Node Status ═══".bright_cyan());

    let proxy_status = if metrics.status.proxy == "running" {
        "Running".bright_green()
    } else {
        "Stopped".bright_red()
    };
    println!("  Proxy:         {}", proxy_status);

    let cache_status = if metrics.status.cache == "connected" {
        "Connected".bright_green()
    } else {
        "Disconnected".yellow()
    };
    println!("  Cache:         {}", cache_status);

    println!("  Uptime:        {}", format_uptime(metrics.status.uptime_seconds));
    println!();

    // Display timestamp
    let timestamp = chrono::DateTime::from_timestamp(metrics.timestamp, 0)
        .unwrap_or_else(|| chrono::Utc::now());
    println!("  Last Updated:  {}", timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string().dimmed());
    println!();

    println!("{}", "═══════════════════════════════════════════════════".bright_cyan());
    println!();

    // Health recommendations
    if metrics.system.cpu_usage_percent > 80.0 {
        println!("{}", "⚠ Warning: High CPU usage detected".yellow());
    }
    if metrics.system.memory_percent > 85.0 {
        println!("{}", "⚠ Warning: High memory usage detected".yellow());
    }
    if metrics.cache.hit_rate < 50.0 && total_cache_ops > 100 {
        println!("{}", "⚠ Notice: Cache hit rate is below 50%".yellow());
    }

    Ok(())
}

/// Format percentage with color coding
fn format_percent(percent: f32) -> colored::ColoredString {
    let formatted = format!("{:.2}", percent);
    if percent < 50.0 {
        formatted.bright_green()
    } else if percent < 80.0 {
        formatted.yellow()
    } else {
        formatted.bright_red()
    }
}

/// Format latency with color coding
fn format_latency(ms: f64) -> colored::ColoredString {
    let formatted = format!("{:.2}", ms);
    if ms < 50.0 {
        formatted.bright_green()
    } else if ms < 100.0 {
        formatted.yellow()
    } else {
        formatted.bright_red()
    }
}

/// Format uptime in human-readable format
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, secs)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime_seconds() {
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(90), "1m 30s");
    }

    #[test]
    fn test_format_uptime_hours() {
        assert_eq!(format_uptime(3661), "1h 1m 1s");
        assert_eq!(format_uptime(7200), "2h 0m 0s");
    }

    #[test]
    fn test_format_uptime_days() {
        assert_eq!(format_uptime(86400), "1d 0h 0m 0s");
        assert_eq!(format_uptime(90061), "1d 1h 1m 1s");
    }

    #[test]
    fn test_format_percent_values() {
        // Just test that it returns a string, color testing is visual
        let low = format_percent(25.5);
        let medium = format_percent(65.0);
        let high = format_percent(95.0);

        assert!(low.to_string().contains("25.50"));
        assert!(medium.to_string().contains("65.00"));
        assert!(high.to_string().contains("95.00"));
    }
}
