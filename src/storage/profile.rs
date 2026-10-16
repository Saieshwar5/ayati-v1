use super::Store;
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub struct ProfileEdit {
    pub model: Option<String>,
    pub name: String,
    pub role: String,
    pub revision: i64,
    // Persisted field name retained from schema 7; now tracks role or model changes.
    pub role_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProfile {
    pub model: Option<String>,
    pub name: String,
    pub role: String,
    pub expected_revision: i64,
}
impl UpdateProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.expected_revision >= 0, "Invalid profile revision");
        for (label, value, max) in [
            ("Name", &self.name, 80),
            ("Responsibilities", &self.role, 4000),
        ] {
            ensure!(
                (label != "Name" || !value.trim().is_empty()) && value.len() <= max,
                "{label} is empty or too long"
            );
            ensure!(
                !value.chars().any(|c| c.is_control() && c != '\n'),
                "{label} contains control characters"
            );
        }
        if let Some(model) = &self.model {
            validate_model(model)?;
        }
        Ok(())
    }
}
pub fn validate_model(model: &str) -> Result<()> {
    ensure!(
        !model.is_empty()
            && model.len() <= 200
            && !model.chars().any(|c| c.is_whitespace() || c.is_control()),
        "Model must be 1–200 bytes with no whitespace or control characters"
    );
    Ok(())
}
impl Store {
    pub fn profile_edit(&self) -> Result<Option<ProfileEdit>> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT name,role,revision,role_revision,model FROM profile_edit WHERE id=1",
                [],
                |r| {
                    Ok(ProfileEdit {
                        model: r.get(4)?,
                        name: r.get(0)?,
                        role: r.get(1)?,
                        revision: r.get(2)?,
                        role_revision: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }
    // Called with the agent lifecycle gate held. Intake is serialized by this transaction:
    // already accepted work prevents editing; work accepted afterwards uses the new role.
    pub fn update_profile(
        &self,
        input: &UpdateProfile,
        original_role: &str,
        original_model: &str,
    ) -> Result<ProfileEdit> {
        input.validate()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let previous: Option<(String, i64, i64, Option<String>)> = tx
            .query_row(
                "SELECT role,revision,role_revision,model FROM profile_edit WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (role, revision, role_revision, previous_model) =
            previous.unwrap_or((original_role.into(), 0, 0, None));
        ensure!(
            revision == input.expected_revision,
            "Settings changed in another tab. Reopen the editor and review the latest settings."
        );
        let busy: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM tasks WHERE status IN ('queued','running')) OR EXISTS(SELECT 1 FROM permissions WHERE decision='pending')", [], |r| r.get(0))?;
        ensure!(
            !busy,
            "Finish or stop all active and queued work before editing this agent"
        );
        let changed_role = role != input.role.trim();
        let model = input.model.clone().or(previous_model.clone());
        let changed_model = model.as_deref().unwrap_or(original_model)
            != previous_model.as_deref().unwrap_or(original_model);
        let result = ProfileEdit {
            model,
            name: input.name.trim().into(),
            role: input.role.trim().into(),
            revision: revision + 1,
            role_revision: role_revision + i64::from(changed_role || changed_model),
        };
        tx.execute("INSERT INTO profile_edit(id,name,role,revision,role_revision,model) VALUES(1,?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,role=excluded.role,revision=excluded.revision,role_revision=excluded.role_revision,model=excluded.model", params![result.name,result.role,result.revision,result.role_revision,result.model])?;
        if changed_role || changed_model {
            tx.execute("DELETE FROM runtime_sessions", [])?;
        }
        tx.commit()?;
        Ok(result)
    }
}
