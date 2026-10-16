use anyhow::{Context, Result};
use serde_json::json;

// The local owner can recover a closed tab using their private control file.
// The capability is never printed or passed on the command line.
pub async fn release(config: &ayati::config::Config) -> Result<()> {
    let path = config.data_dir.join("browser-owner.json");
    let owner: Option<String> = serde_json::from_slice(
        &std::fs::read(path).context("No saved browser controller in this AYATI_DATA_DIR")?,
    )?;
    let Some(token) = owner else {
        println!("The browser is already handed back.");
        return Ok(());
    };
    uuid::Uuid::parse_str(&token).context("Invalid saved browser controller")?;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/browser/control", config.port);
    for op in ["take", "resume"] {
        let response = client
            .post(&url)
            .json(&json!({"op":op,"token":token}))
            .send()
            .await
            .context("Ayati is not reachable; start the daemon before releasing browser control")?;
        anyhow::ensure!(
            response.status().is_success(),
            "Browser control could not be released; inspect the browser panel. No action was retried."
        );
    }
    println!("Browser control handed back. Interrupted work needs a follow-up message.");
    Ok(())
}
