#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("mcp") => ayati_guest::proof_tool::serve().await,
        Some("memory-mcp") => ayati_guest::memory_mcp::serve().await,
        Some("workspace-mcp") => ayati_guest::workspace_mcp::serve().await,
        Some("bridge") => ayati_guest::bridge::serve().await,
        Some("browser-mcp") => ayati_guest::browser_mcp::serve().await,
        Some("browser-service") => ayati_guest::browser_service::serve().await,
        Some("--version") => {
            println!("0.1.0");
            Ok(())
        }
        _ => anyhow::bail!("Expected bridge or mcp"),
    }
}
