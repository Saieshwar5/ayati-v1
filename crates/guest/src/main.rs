#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("mcp") => ayati_guest::proof_tool::serve().await,
        Some("bridge") => ayati_guest::bridge::serve().await,
        Some("--version") => {
            println!("0.1.0");
            Ok(())
        }
        _ => anyhow::bail!("Expected bridge or mcp"),
    }
}
