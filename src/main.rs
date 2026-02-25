use anyhow::Result;
use clap::Parser;

use aimcp::detection;
use aimcp::server::UnifiedServer;
use aimcp::transport::AdaptiveStdio;
use rmcp::ServiceExt;

#[derive(Parser)]
#[command(name = "aimcp", version, about = "Unified AI MCP Server")]
struct Cli {}

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    eprintln!("[aimcp] Starting...");

    let capabilities = detection::detect();

    let service = UnifiedServer::new(capabilities)
        .serve(AdaptiveStdio::new())
        .await
        .inspect_err(|e| eprintln!("[aimcp] serving error: {:?}", e))?;

    service.waiting().await?;
    Ok(())
}
