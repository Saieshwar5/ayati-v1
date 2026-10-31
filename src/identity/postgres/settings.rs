//! Settings (D232): the Workspace page, personal preferences, problem reports and
//! the audit log. The workspace name keeps its rename-with-check below.
use super::*;

#[path = "audit_log.rs"]
mod audit_log;
#[path = "member_preferences.rs"]
pub(crate) mod preferences;
#[path = "problem_reports.rs"]
pub(crate) mod problems;
#[cfg(test)]
#[path = "workspace_settings_tests.rs"]
mod tests;
#[path = "workspace_settings.rs"]
pub(crate) mod workspace;
#[path = "workspace_settings_migration.rs"]
pub(super) mod workspace_migration;

impl PgStore {
    pub async fn rename_workspace(
        &self,
        user: &User,
        w: &str,
        name: &str,
        previous: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        ensure!(
            execute(
                &tx,
                "UPDATE relay_workspaces SET name=$2 WHERE id=$1 AND name=$3",
                vec![w.into(), name.into(), previous.into()]
            )
            .await?
                == 1,
            "Workspace changed; reload before saving"
        );
        audit(&tx, &user.id, Some(w), "workspace_renamed", name).await?;
        tx.commit().await?;
        Ok(())
    }
}
