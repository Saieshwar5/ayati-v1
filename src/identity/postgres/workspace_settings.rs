//! The Workspace page (D232 slice 4): time zone, colour, approval backup delay,
//! the models agents may use, and deletion with a 7-day undo. Admins change it;
//! every member can read what their own screens need.
use super::*;
use serde::Deserialize;
use serde_json::json;

/// Providers the hosted service can run (`agents::config` checks the same list).
const PROVIDERS: [&str; 2] = ["openai", "fireworks-ai"];
const COLOURS: [&str; 6] = ["blue", "green", "orange", "purple", "brown", "teal"];
/// How long an admin can undo a deletion.
pub(crate) const DELETE_UNDO_MS: i64 = 7 * 24 * 3600 * 1000;

#[derive(Clone, Deserialize, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelChoice {
    pub provider: String,
    pub model: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceSettingsInput {
    pub timezone: String,
    pub colour: Option<String>,
    pub approval_backup_hours: i32,
    pub models: Vec<ModelChoice>,
    pub default_model: Option<String>,
}

impl WorkspaceSettingsInput {
    fn validate(&self) -> Result<()> {
        let zone = self.timezone.trim();
        ensure!(
            !zone.is_empty()
                && zone.len() <= 64
                && zone
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c)),
            "Choose a time zone"
        );
        ensure!(
            self.colour.as_deref().is_none_or(|c| COLOURS.contains(&c)),
            "Choose one of the colours"
        );
        ensure!(
            (1..=168).contains(&self.approval_backup_hours),
            "Choose an approval backup delay between 1 hour and 7 days"
        );
        ensure!(self.models.len() <= 20, "Allow at most 20 models");
        for (index, choice) in self.models.iter().enumerate() {
            ensure!(
                PROVIDERS.contains(&choice.provider.as_str()),
                "Unsupported model provider"
            );
            ensure!(
                !choice.model.is_empty()
                    && choice.model.len() <= 200
                    && !choice
                        .model
                        .chars()
                        .any(|c| c.is_whitespace() || c.is_control()),
                "Invalid model identifier"
            );
            ensure!(
                !self.models[..index].contains(choice),
                "Each model can be listed once"
            );
        }
        if let Some(default) = &self.default_model {
            ensure!(
                self.models.is_empty() || self.models.iter().any(|m| &m.model == default),
                "The default model must be one of the allowed models"
            );
            ensure!(
                !default.is_empty()
                    && default.len() <= 200
                    && !default.chars().any(char::is_whitespace),
                "Invalid model identifier"
            );
        }
        Ok(())
    }
}

/// Whether an agent may use this model: any model when the admin listed none.
/// Reads the column through `to_jsonb` so older schemas allow every model.
pub(crate) async fn model_allowed(
    db: &impl ConnectionTrait,
    w: &str,
    provider: &str,
    model: &str,
) -> Result<bool> {
    let row = query(
        db,
        "SELECT COALESCE(jsonb_array_length(to_jsonb(w)->'models'),0)=0
        OR EXISTS(SELECT 1 FROM jsonb_array_elements(to_jsonb(w)->'models') e
          WHERE e->>'model'=$2 AND ($3='' OR e->>'provider'=$3)) AS ok
        FROM relay_workspaces w WHERE w.id=$1",
        vec![w.into(), model.into(), provider.into()],
    )
    .await?
    .context("Workspace unavailable")?;
    Ok(row.try_get("", "ok")?)
}

/// Refuses new work for an agent whose model the admin has since removed.
pub(crate) async fn agent_model_allowed(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<bool> {
    let Some(row) = query(db, "SELECT model,COALESCE(settings->>'provider','') AS provider FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![w.into(), agent.into()]).await? else {
        return Ok(true);
    };
    model_allowed(
        db,
        w,
        &row.try_get::<String>("", "provider")?,
        &row.try_get::<String>("", "model")?,
    )
    .await
}

impl PgStore {
    /// The models agents may use and the workspace's default, for agent screens.
    pub(crate) async fn workspace_models(
        &self,
        user: &User,
        w: &str,
    ) -> Result<(Value, Option<String>)> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let row = query(&tx, "SELECT COALESCE(to_jsonb(w)->'models','[]'::jsonb) AS models,to_jsonb(w)->>'default_model' AS default_model
            FROM relay_workspaces w WHERE w.id=$1", vec![w.into()]).await?.context("Workspace unavailable")?;
        tx.commit().await?;
        Ok((
            row.try_get("", "models")?,
            row.try_get("", "default_model")?,
        ))
    }

    /// Everything on the Workspace page, for admins.
    pub async fn workspace_settings(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let data = settings_row(&tx, w).await?;
        tx.commit().await?;
        Ok(data)
    }

    pub(crate) async fn save_workspace_settings(
        &self,
        user: &User,
        w: &str,
        input: &WorkspaceSettingsInput,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let before = settings_row(&tx, w).await?;
        execute(&tx, "UPDATE relay_workspaces SET timezone=$2,colour=$3,approval_backup_hours=$4,models=$5,default_model=$6 WHERE id=$1",
            vec![w.into(), input.timezone.trim().into(), input.colour.clone().into(), input.approval_backup_hours.into(),
                serde_json::to_value(&input.models)?.into(), input.default_model.clone().into()]).await?;
        let after = settings_row(&tx, w).await?;
        let summary = changes(&before, &after);
        if !summary.is_empty() {
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "workspace_settings_saved",
                w,
                &summary,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(after)
    }

    /// Schedules the deletion. The workspace keeps working until then and any
    /// admin can undo it. The name must be typed to confirm.
    pub async fn schedule_workspace_deletion(
        &self,
        user: &User,
        w: &str,
        name: &str,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let row = query(
            &tx,
            "SELECT name,delete_at_ms FROM relay_workspaces WHERE id=$1",
            vec![w.into()],
        )
        .await?
        .context("Workspace unavailable")?;
        ensure!(
            row.try_get::<String>("", "name")? == name.trim(),
            "Type the workspace name exactly to confirm"
        );
        let at = match row.try_get::<Option<i64>>("", "delete_at_ms")? {
            // A repeated request keeps the first date.
            Some(at) => at,
            None => {
                let at = now() * 1000 + DELETE_UNDO_MS;
                execute(&tx, "UPDATE relay_workspaces SET delete_at_ms=$2,delete_requested_by=$3 WHERE id=$1",
                    vec![w.into(), at.into(), user.id.clone().into()]).await?;
                audit_with_summary(
                    &tx,
                    &user.id,
                    Some(w),
                    "workspace_deletion_scheduled",
                    w,
                    "scheduled this workspace for deletion",
                )
                .await?;
                at
            }
        };
        tx.commit().await?;
        Ok(json!({"delete_at_ms": at}))
    }

    pub async fn cancel_workspace_deletion(&self, user: &User, w: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        if execute(&tx, "UPDATE relay_workspaces SET delete_at_ms=NULL,delete_requested_by=NULL WHERE id=$1 AND delete_at_ms IS NOT NULL",
            vec![w.into()]).await? == 1 {
            audit_with_summary(&tx, &user.id, Some(w), "workspace_deletion_cancelled", w, "cancelled the deletion of this workspace").await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Completes deletions whose undo time has passed: the workspace closes for
    /// everyone and its agents, schedules and triggers stop. Returns how many
    /// closed. Erasing the stored records is a separate operator step.
    pub async fn close_deleted_workspaces(&self, limit: i64) -> Result<u64> {
        let due = rows(&self.db, "SELECT id FROM relay_workspaces WHERE delete_at_ms<=$1 AND closed_ms IS NULL ORDER BY delete_at_ms LIMIT $2",
            vec![(now() * 1000).into(), limit.into()]).await?;
        let mut closed = 0;
        for row in due {
            let w: String = row.try_get("", "id")?;
            let tx = self.db.begin().await?;
            // Re-check under the row lock: an admin may have undone it meanwhile.
            if execute(&tx, "UPDATE relay_workspaces SET closed_ms=$2 WHERE id=$1 AND delete_at_ms<=$2 AND closed_ms IS NULL",
                vec![w.clone().into(), (now() * 1000).into()]).await? == 0 {
                continue;
            }
            execute(&tx, "UPDATE relay_agent_schedules SET state='paused' WHERE workspace=$1 AND state='enabled'", vec![w.clone().into()]).await?;
            execute(&tx, "UPDATE relay_event_triggers SET state='paused' WHERE workspace=$1 AND state='enabled'", vec![w.clone().into()]).await?;
            execute(&tx, "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{paused}','true'::jsonb) WHERE workspace=$1", vec![w.clone().into()]).await?;
            audit(&tx, "system", Some(&w), "workspace_closed", &w).await?;
            tx.commit().await?;
            tracing::info!(workspace = %w, "Closed a workspace whose deletion was due");
            closed += 1;
        }
        Ok(closed)
    }
}

async fn settings_row(db: &impl ConnectionTrait, w: &str) -> Result<Value> {
    let row = query(db, "SELECT jsonb_build_object('name',w.name,'timezone',w.timezone,'colour',w.colour,
        'approval_backup_hours',w.approval_backup_hours,'models',w.models,'default_model',w.default_model,
        'delete_at_ms',w.delete_at_ms,'delete_requested_by',COALESCE(u.display_name,u.email)) AS data
        FROM relay_workspaces w LEFT JOIN relay_users u ON u.id=w.delete_requested_by WHERE w.id=$1", vec![w.into()])
        .await?
        .context("Workspace unavailable")?;
    Ok(row.try_get("", "data")?)
}

/// "changed the time zone to Asia/Kolkata and allowed models" — for the audit log.
fn changes(before: &Value, after: &Value) -> String {
    let mut parts = vec![];
    if before["timezone"] != after["timezone"] {
        parts.push(format!(
            "set the time zone to {}",
            after["timezone"].as_str().unwrap_or_default()
        ));
    }
    if before["colour"] != after["colour"] {
        parts.push("changed the colour".to_string());
    }
    if before["approval_backup_hours"] != after["approval_backup_hours"] {
        parts.push(format!(
            "let admins decide approvals after {} hours",
            after["approval_backup_hours"]
        ));
    }
    if before["models"] != after["models"] {
        let count = after["models"].as_array().map_or(0, Vec::len);
        parts.push(if count == 0 {
            "allowed any model".to_string()
        } else {
            format!("allowed {count} models")
        });
    }
    if before["default_model"] != after["default_model"] {
        parts.push(format!(
            "made {} the default model",
            after["default_model"]
                .as_str()
                .unwrap_or("the service default")
        ));
    }
    parts.join(" and ")
}
