//! Makes sure the signed-in member has their assistant (schema 127).
use super::*;

/// The member's assistant, made with the workspace's default model the first
/// time they open Chat or Agents. A failure (for example a model the workspace
/// no longer allows) never blocks the page: it is logged and tried again on
/// the next visit.
pub(super) async fn ensure(app: &App, user: &User, w: &str) -> Option<String> {
    match app.store.assistant_of(user, w).await {
        Ok(Some(agent)) => return Some(agent),
        Ok(None) => {}
        Err(_) => return None,
    }
    let made = async {
        let (models, chosen) = app.store.workspace_models(user, w).await?;
        let model = models::default_model(app, chosen);
        // With a catalog Cuckoo picks who serves a model; otherwise the
        // workspace's own entry for it names the provider.
        let provider = if app.store.model_catalog.is_some() {
            String::new()
        } else {
            models
                .as_array()
                .and_then(|list| list.iter().find(|m| m["model"] == model.as_str()))
                .and_then(|m| m["provider"].as_str())
                .unwrap_or_default()
                .to_owned()
        };
        app.store.ensure_assistant(user, w, &model, &provider).await
    }
    .await;
    match made {
        Ok(agent) => Some(agent),
        Err(error) => {
            tracing::warn!(workspace = w, %error, "Assistant could not be made");
            None
        }
    }
}
