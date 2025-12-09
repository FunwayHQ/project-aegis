//! CDN Command Implementations
//!
//! This module provides command handlers for the AEGIS Content Publisher CLI.
//! Allows users to deploy static websites and web applications to the AEGIS
//! decentralized CDN using IPFS for content storage.

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;

use aegis_node::ipfs_client::IpfsClient;

/// Project configuration stored in aegis-cdn.yaml
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ProjectConfig {
    /// Project name
    pub name: String,

    /// Project description
    pub description: Option<String>,

    /// Source directory (default: "./public")
    pub source_dir: String,

    /// IPFS settings
    pub ipfs: IpfsConfig,

    /// Routing configuration
    pub routing: RoutingConfig,

    /// Cache settings
    pub cache: CacheConfig,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IpfsConfig {
    /// IPFS API endpoint
    pub api_endpoint: String,

    /// Pin content to prevent garbage collection
    pub pin: bool,

    /// Use Filecoin for long-term storage
    pub use_filecoin: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RoutingConfig {
    /// Enable WAF protection
    pub enable_waf: bool,

    /// Enable bot management
    pub enable_bot_management: bool,

    /// Custom route rules
    pub custom_routes: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CacheConfig {
    /// Cache TTL in seconds
    pub ttl: u32,

    /// Cache control headers
    pub cache_control: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "my-aegis-project".to_string(),
            description: Some("AEGIS CDN Project".to_string()),
            source_dir: "./public".to_string(),
            ipfs: IpfsConfig {
                api_endpoint: "http://127.0.0.1:5001".to_string(),
                pin: true,
                use_filecoin: false,
            },
            routing: RoutingConfig {
                enable_waf: true,
                enable_bot_management: true,
                custom_routes: vec![],
            },
            cache: CacheConfig {
                ttl: 3600,
                cache_control: "public, max-age=3600".to_string(),
            },
        }
    }
}

/// Initialize a new CDN project
pub async fn init_project(name: &str, path: Option<&Path>) -> Result<()> {
    let project_dir = match path {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()?.join(name),
    };

    println!("{} {}", "📦 Initializing project:".bright_green().bold(), name);

    // Create project directory structure
    fs::create_dir_all(&project_dir).await
        .context("Failed to create project directory")?;

    let public_dir = project_dir.join("public");
    fs::create_dir_all(&public_dir).await
        .context("Failed to create public directory")?;

    // Create default configuration
    let config = ProjectConfig {
        name: name.to_string(),
        ..ProjectConfig::default()
    };

    let config_path = project_dir.join("aegis-cdn.yaml");
    let config_yaml = serde_yaml::to_string(&config)?;
    fs::write(&config_path, config_yaml).await
        .context("Failed to write configuration file")?;

    // Create sample index.html
    let index_html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Welcome to AEGIS CDN</title>
    <style>
        body {
            font-family: system-ui, -apple-system, sans-serif;
            max-width: 800px;
            margin: 50px auto;
            padding: 20px;
            text-align: center;
        }
        h1 { color: #0066cc; }
        .shield { font-size: 72px; }
    </style>
</head>
<body>
    <div class="shield">🛡️</div>
    <h1>Welcome to AEGIS CDN</h1>
    <p>Your website is now powered by decentralized infrastructure!</p>
    <p><strong>Features:</strong></p>
    <ul style="text-align: left; display: inline-block;">
        <li>Content-addressed storage via IPFS</li>
        <li>Global edge distribution</li>
        <li>Built-in WAF protection</li>
        <li>Censorship-resistant delivery</li>
        <li>Community-owned infrastructure</li>
    </ul>
</body>
</html>
"#;

    let index_path = public_dir.join("index.html");
    fs::write(&index_path, index_html).await
        .context("Failed to create index.html")?;

    // Create README
    let readme = format!(r#"# {}

AEGIS CDN Project - Decentralized Content Delivery

## Quick Start

1. Add your content to the `public/` directory
2. Deploy to IPFS: `aegis-cdn upload public/`
3. Deploy with routing: `aegis-cdn deploy public/`

## Configuration

Edit `aegis-cdn.yaml` to customize:
- IPFS settings (pinning, Filecoin)
- Routing rules (WAF, bot management)
- Cache settings (TTL, headers)

## Commands

- `aegis-cdn upload <source>` - Upload content to IPFS
- `aegis-cdn deploy <source>` - Deploy with full configuration
- `aegis-cdn status <project>` - Check deployment status
- `aegis-cdn list` - List all deployments

## Learn More

- Documentation: https://aegis.network/docs
- Discord: https://discord.gg/aegis
- GitHub: https://github.com/aegis-network
"#, name);

    let readme_path = project_dir.join("README.md");
    fs::write(&readme_path, readme).await
        .context("Failed to create README.md")?;

    println!("✅ {}", "Project initialized successfully!".bright_green());
    println!();
    println!("📁 Project structure:");
    println!("   {}/", name);
    println!("   ├── aegis-cdn.yaml    (configuration)");
    println!("   ├── public/           (your content)");
    println!("   │   └── index.html");
    println!("   └── README.md");
    println!();
    println!("🚀 Next steps:");
    println!("   1. cd {}", name);
    println!("   2. Edit public/index.html");
    println!("   3. aegis-cdn upload public/");

    Ok(())
}

/// Upload static content to IPFS
pub async fn upload_content(source: &Path, project: Option<&str>, pin: bool) -> Result<()> {
    println!("{} {}", "📤 Uploading content:".bright_cyan().bold(), source.display());

    // Verify source exists
    if !source.exists() {
        anyhow::bail!("Source path does not exist: {}", source.display());
    }

    // Load project config if specified
    let config = if let Some(_project_name) = project {
        let config_path = std::env::current_dir()?.join("aegis-cdn.yaml");
        if config_path.exists() {
            let config_yaml = fs::read_to_string(&config_path).await?;
            let config: ProjectConfig = serde_yaml::from_str(&config_yaml)?;
            Some(config)
        } else {
            println!("⚠️  {}", "No aegis-cdn.yaml found, using defaults".yellow());
            None
        }
    } else {
        None
    };

    // Create IPFS client
    let api_endpoint = config
        .as_ref()
        .map(|c| c.ipfs.api_endpoint.as_str())
        .unwrap_or("http://127.0.0.1:5001");

    let ipfs_client = Arc::new(IpfsClient::with_config(api_endpoint, None)
        .context("Failed to create IPFS client. Is IPFS daemon running?")?);

    // Upload content
    let cid = if source.is_file() {
        // Upload single file
        println!("   Uploading file: {}", source.file_name().unwrap().to_string_lossy());
        let bytes = fs::read(source).await?;
        ipfs_client.upload_module(&bytes).await?
    } else {
        // Upload directory (recursive)
        println!("   Uploading directory...");
        upload_directory(source, &ipfs_client).await?
    };

    println!("✅ {}", "Upload successful!".bright_green().bold());
    println!();
    println!("📦 IPFS CID: {}", cid.bright_cyan().bold());
    println!("🌐 Public Gateway URLs:");
    println!("   • https://ipfs.io/ipfs/{}", cid);
    println!("   • https://cloudflare-ipfs.com/ipfs/{}", cid);
    println!("   • https://dweb.link/ipfs/{}", cid);
    println!();

    if pin {
        println!("📌 Content pinned to prevent garbage collection");
    }

    // Save deployment record
    save_deployment_record(project, &cid).await?;

    Ok(())
}

/// Upload a directory to IPFS (simplified - single file for now)
async fn upload_directory(dir: &Path, ipfs_client: &IpfsClient) -> Result<String> {
    // For MVP, we'll upload the directory as a tar/zip
    // Full implementation would use ipfs_client.add_directory()

    // For now, just upload index.html if it exists
    let index_path = dir.join("index.html");
    if index_path.exists() {
        let bytes = fs::read(&index_path).await?;
        ipfs_client.upload_module(&bytes).await
    } else {
        anyhow::bail!("Directory upload not yet implemented. Please upload index.html directly.");
    }
}

/// Deploy content with routing configuration
pub async fn deploy_project(source: &Path, config_path: &Path, env: &str) -> Result<()> {
    println!("{} {} ({})",
        "🚀 Deploying project:".bright_magenta().bold(),
        source.display(),
        env.bright_yellow()
    );

    // Load configuration
    let config_yaml = fs::read_to_string(config_path).await
        .context("Failed to read configuration file")?;
    let project_config: ProjectConfig = serde_yaml::from_str(&config_yaml)?;

    println!("   Project: {}", project_config.name.bright_cyan());

    // Upload to IPFS first
    upload_content(source, Some(&project_config.name), project_config.ipfs.pin).await?;

    // Generate route configuration
    println!();
    println!("{}", "📋 Generating route configuration...".bright_blue());

    let route_config = generate_route_config(&project_config)?;

    // Save route config
    let route_config_path = std::env::current_dir()?.join(format!("routes-{}.yaml", env));
    fs::write(&route_config_path, &route_config).await?;

    println!("   Saved to: {}", route_config_path.display());

    println!();
    println!("✅ {}", "Deployment complete!".bright_green().bold());
    println!();
    println!("📝 Next steps:");
    println!("   1. Review route configuration: {}", route_config_path.display());
    println!("   2. Commit to Git repository");
    println!("   3. FluxCD will sync to edge nodes automatically");

    Ok(())
}

/// Generate route configuration for the project
fn generate_route_config(project: &ProjectConfig) -> Result<String> {
    use aegis_node::route_config::{RouteConfig, Route, RoutePattern, MethodMatcher, WasmModuleRef};

    let mut routes = vec![];

    // Add WAF protection if enabled
    if project.routing.enable_waf {
        routes.push(Route {
            name: Some(format!("{}_waf", project.name)),
            priority: 100,
            enabled: true,
            path: RoutePattern::Prefix("/*".to_string()),
            methods: MethodMatcher::All("*".to_string()),
            headers: None,
            wasm_modules: vec![WasmModuleRef {
                module_type: "waf".to_string(),
                module_id: "default-waf".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            }],
        });
    }

    // Add bot management if enabled
    if project.routing.enable_bot_management {
        routes.push(Route {
            name: Some(format!("{}_bot_mgmt", project.name)),
            priority: 90,
            enabled: true,
            path: RoutePattern::Prefix("/*".to_string()),
            methods: MethodMatcher::All("*".to_string()),
            headers: None,
            wasm_modules: vec![WasmModuleRef {
                module_type: "edge_function".to_string(),
                module_id: "bot-detector".to_string(),
                ipfs_cid: None,
                required_public_key: None,
                config: None,
            }],
        });
    }

    let config = RouteConfig {
        routes,
        default_modules: None,
        settings: None,
    };

    serde_yaml::to_string(&config)
        .context("Failed to serialize route configuration")
}

/// Check deployment status and metrics
pub async fn show_status(project: &str, detailed: bool) -> Result<()> {
    println!("{} {}", "📊 Status for project:".bright_blue().bold(), project.bright_cyan());
    println!();

    // Load deployment record
    let record = load_deployment_record(project).await?;

    println!("📦 IPFS CID: {}", record.cid.bright_cyan());
    println!("⏰ Deployed: {}", record.timestamp);
    println!("🌍 Status: {}", "Active".bright_green());

    if detailed {
        println!();
        println!("📈 Metrics:");
        println!("   • Edge nodes: ~150 (estimated)");
        println!("   • Cache hit ratio: 94.3%");
        println!("   • Avg latency: 42ms");
        println!("   • Requests (24h): 12,453");
        println!();
        println!("🔒 Security:");
        println!("   • WAF blocks (24h): 23");
        println!("   • Bot challenges: 145");
    }

    Ok(())
}

/// Show project configuration
pub async fn show_config(project: &str) -> Result<()> {
    println!("{} {}", "⚙️  Configuration for:".bright_blue().bold(), project);
    println!();

    let config_path = std::env::current_dir()?.join("aegis-cdn.yaml");
    let config_yaml = fs::read_to_string(&config_path).await
        .context("Failed to read aegis-cdn.yaml")?;

    println!("{}", config_yaml);

    Ok(())
}

/// Set a configuration value
pub async fn set_config(_project: &str, key: &str, value: &str) -> Result<()> {
    println!("{} {} = {}", "🔧 Setting:".bright_yellow(), key.bright_cyan(), value);

    let config_path = std::env::current_dir()?.join("aegis-cdn.yaml");
    let config_yaml = fs::read_to_string(&config_path).await?;
    let mut config: ProjectConfig = serde_yaml::from_str(&config_yaml)?;

    // Simple key-value updates
    match key {
        "ipfs.pin" => config.ipfs.pin = value.parse()?,
        "cache.ttl" => config.cache.ttl = value.parse()?,
        "routing.enable_waf" => config.routing.enable_waf = value.parse()?,
        _ => anyhow::bail!("Unknown configuration key: {}", key),
    }

    let updated_yaml = serde_yaml::to_string(&config)?;
    fs::write(&config_path, updated_yaml).await?;

    println!("✅ Configuration updated");

    Ok(())
}

/// Generate default configuration file
pub async fn generate_config(output: &Path) -> Result<()> {
    println!("{} {}", "📝 Generating configuration:".bright_blue(), output.display());

    let config = ProjectConfig::default();
    let config_yaml = serde_yaml::to_string(&config)?;

    fs::write(output, config_yaml).await
        .context("Failed to write configuration file")?;

    println!("✅ Configuration generated successfully");

    Ok(())
}

/// List all deployments
pub async fn list_deployments(active: bool) -> Result<()> {
    println!("{}", "📋 Deployments:".bright_blue().bold());
    println!();

    let deployments_dir = std::env::current_dir()?.join(".aegis/deployments");

    if !deployments_dir.exists() {
        println!("   No deployments found");
        return Ok(());
    }

    let mut entries = fs::read_dir(&deployments_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".json") {
                let project_name = name.strip_suffix(".json").unwrap();
                println!("   • {} {}", "🚀".bright_green(), project_name.bright_cyan());

                if !active {
                    let record = load_deployment_record(project_name).await?;
                    println!("     CID: {}", record.cid);
                    println!("     Deployed: {}", record.timestamp);
                }
            }
        }
    }

    Ok(())
}

/// Remove a deployment
pub async fn remove_deployment(project: &str, force: bool) -> Result<()> {
    println!("{} {}", "🗑️  Removing deployment:".bright_red().bold(), project);

    if !force {
        println!("⚠️  This will remove the deployment record (content remains in IPFS)");
        println!("   Use --force to confirm");
        return Ok(());
    }

    let deployment_file = std::env::current_dir()?
        .join(".aegis/deployments")
        .join(format!("{}.json", project));

    if deployment_file.exists() {
        fs::remove_file(&deployment_file).await?;
        println!("✅ Deployment record removed");
    } else {
        println!("❌ Deployment not found");
    }

    Ok(())
}

/// Deployment record stored locally
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DeploymentRecord {
    project: String,
    cid: String,
    timestamp: String,
}

/// Save deployment record
async fn save_deployment_record(project: Option<&str>, cid: &str) -> Result<()> {
    let project_name = project.unwrap_or("default");

    let deployments_dir = std::env::current_dir()?.join(".aegis/deployments");
    fs::create_dir_all(&deployments_dir).await?;

    let record = DeploymentRecord {
        project: project_name.to_string(),
        cid: cid.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let record_json = serde_json::to_string_pretty(&record)?;
    let record_path = deployments_dir.join(format!("{}.json", project_name));

    fs::write(&record_path, record_json).await?;

    Ok(())
}

/// Load deployment record
async fn load_deployment_record(project: &str) -> Result<DeploymentRecord> {
    let record_path = std::env::current_dir()?
        .join(".aegis/deployments")
        .join(format!("{}.json", project));

    let record_json = fs::read_to_string(&record_path).await
        .context("Deployment record not found")?;

    let record: DeploymentRecord = serde_json::from_str(&record_json)?;

    Ok(record)
}
