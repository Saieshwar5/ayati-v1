use super::*;
pub(in crate::identity) async fn agents(app: &App, user: &User, w: &str) -> Result<Vec<Value>> {
    let mut profiles = if let database::Database::Postgres(pg) = &app.store {
        pg.allowed_shared_agents(user, w).await?
    } else {
        vec![]
    };
    if app.simulation.is_none() && !hosting::hosted(app) {
        let state = app
            .runtimes
            .get(w, app.origin.rsplit(':').next().unwrap_or("8765").parse()?)
            .await?;
        for profile in state
            .workspace
            .context("Workspace unavailable")?
            .profiles()?
            .into_iter()
            .filter(|p| p.state == "ready")
        {
            profiles.push(json!({"id":profile.id,"name":profile.name,"role":profile.role,"model":profile.model}));
        }
    }
    let mut allowed = Vec::new();
    for profile in profiles {
        if profile["shared"] == true
            || app
                .store
                .agent_access(
                    &user.id,
                    w,
                    profile["id"].as_str().context("Invalid agent")?,
                )
                .await?
        {
            allowed.push(profile);
        }
    }
    Ok(allowed)
}
pub(in crate::identity) async fn validate_agent(
    app: &App,
    user: &User,
    w: &str,
    id: &str,
) -> Result<()> {
    if let database::Database::Postgres(pg) = &app.store
        && pg
            .allowed_shared_agents(user, w)
            .await?
            .iter()
            .any(|p| p["id"] == id)
    {
        return Ok(());
    }
    check(
        agents(app, user, w).await?.iter().any(|p| p["id"] == id),
        "Agent unavailable or not shared with you",
    )
}
