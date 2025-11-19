mod server;
mod config;

use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use std::net::SocketAddr;
use tracing::info;

use config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    info!("╔════════════════════════════════════════════╗");
    info!("║  AEGIS Decentralized Edge Network Node    ║");
    info!("║  Sprint 1: HTTP Server Proof-of-Concept   ║");
    info!("╚════════════════════════════════════════════╝");
    info!("");
    info!("Starting server on http://{}", addr);
    info!("Endpoints:");
    info!("  - GET /          - Node information");
    info!("  - GET /health    - Health check (JSON)");
    info!("  - GET /metrics   - Node metrics (JSON)");
    info!("");

    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, std::convert::Infallible>(service_fn(server::handle_request))
    });

    let server = Server::bind(&addr).serve(make_svc);

    info!("Server ready! Press Ctrl+C to stop.");

    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }

    Ok(())
}
