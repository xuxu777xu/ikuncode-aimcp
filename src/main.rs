use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "aimcp", version, about = "Unified AI MCP Server")]
struct Cli {}

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();
    eprintln!("[aimcp] Starting...");
    Ok(())
}
