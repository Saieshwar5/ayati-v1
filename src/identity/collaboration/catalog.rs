use super::*;
pub(in crate::identity) async fn agents(app: &App, user: &User, w: &str) -> Result<Vec<Value>> {
    let profiles = if let database::Database::Postgres(pg) = &app.store {
        pg.allowed_shared_agents(user, w).await?
    } else {
        vec![]
    };
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
