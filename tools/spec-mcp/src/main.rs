use anyhow::Result;
use rmcp::ServiceExt;
use spec_mcp::tools::SpecServer;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Spec text, BNF grammars, and figure images are embedded in the binary;
    // the only filesystem dependency is the crates/ directory used by
    // find_implementation, resolved against the cwd unless overridden by the
    // first CLI argument.
    let crates_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates"));

    let server = SpecServer::embedded(crates_dir)?;

    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
