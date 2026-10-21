use super::*;
pub(super) async fn reports(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let sim = app.simulation.as_ref().context("Simulation is disabled")?;
    let user = identity(&app, &headers).await?;
    check(
        app.store.role(&sim.workspace, &user.id).await? == "Owner",
        "Only the owner can inspect simulation reports",
    )?;
    let root = sim
        .root
        .parent()
        .context("Simulation root unavailable")?
        .join("runs");
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Json(json!({"reports":[]})));
        }
        Err(error) => return Err(error.into()),
    };
    let mut paths = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    paths.reverse();
    let mut reports = Vec::new();
    for path in paths.into_iter().take(20) {
        let mut bundle = serde_json::Map::new();
        for (name, file) in [
            ("rehearsal", "report.json"),
            ("load", "sustained-load.json"),
            ("browser", "browser-results.json"),
            ("retention", "retention-result.json"),
            ("recovery", "recovery-result.json"),
        ] {
            let file = path.join(file);
            if let Ok(meta) = tokio::fs::metadata(&file).await {
                if meta.len() > 2 * 1024 * 1024 {
                    continue;
                }
                if let Ok(bytes) = tokio::fs::read(file).await
                    && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                {
                    bundle.insert(name.into(), value);
                }
            }
        }
        if bundle.contains_key("rehearsal") {
            reports.push(Value::Object(bundle));
        }
    }
    Ok(Json(json!({"reports":reports})))
}
