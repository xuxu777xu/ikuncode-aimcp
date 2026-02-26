use anyhow::Result;
use clap::Parser;

use ikuncode_aimcp::detection;
use ikuncode_aimcp::server::UnifiedServer;
use ikuncode_aimcp::transport::AdaptiveStdio;
use rmcp::ServiceExt;

#[derive(Parser)]
#[command(name = "ikuncode-aimcp", version, about = "Unified AI MCP Server")]
struct Cli {}

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();

    eprintln!("[ikuncode-aimcp] Starting...");

    let capabilities = detection::detect();

    let service = UnifiedServer::new(capabilities)
        .serve(AdaptiveStdio::new())
        .await
        .inspect_err(|e| eprintln!("[ikuncode-aimcp] serving error: {:?}", e))?;

    service.waiting().await?;
    Ok(())
}
