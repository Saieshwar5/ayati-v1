use super::*;
use crate::panels::presentation::shape;
use crate::{
    identity::{chat::model::Job, home_panels::maintenance::Refresh},
    panels::PanelDraft,
};
use serde_json::json;
#[cfg(test)]
#[path = "home_frame_tests.rs"]
mod frame_tests;
impl PgStore {
    pub(crate) async fn refresh_home_panel(&self, job: &Job, input: &Refresh) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let data = super::home_update_access::active(&tx, job)
            .await?
            .context("Only a controller-bound panel update can publish a refresh")?;
        let mut definition = data["definition"].clone();
        definition["sections"] = serde_json::to_value(&input.sections)?;
        ensure!(
            shape(definition.clone()) == shape(data["definition"].clone()),
            "Panel headings, columns and template require a new human-reviewed definition"
        );
        let draft: PanelDraft = serde_json::from_value(
            json!({"id":data["panel"],"expected_revision":0,"title":definition["title"],"purpose":definition["purpose"],"template":definition["template"],"sections":definition["sections"],"sources":[]}),
        )?;
        draft.validate()?;
        let hash = crate::identity::events::model::fingerprint(&definition)?;
        if !data["published_generation"].is_null() {
            ensure!(
                data["request_hash"] == hash,
                "This update already published different content"
            );
            let result = json!({"state":"published","generation":data["published_generation"]});
            tx.commit().await?;
            return Ok(result);
        }
        ensure!(
            data["policy"]["generation"] == data["expected_generation"],
            "A newer panel update has already published"
        );
        ensure!(query(&tx,"SELECT id FROM relay_control_actions WHERE workspace=$1 AND job=$2 AND state IN ('pending','executing','uncertain') LIMIT 1",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.is_none(),"Resolve pending source reads before publishing");
        let mut checked_ms = i64::MAX;
        for scope in data["policy"]["scopes"].as_array().unwrap() {
            let row=query(&tx,"SELECT min(updated)*1000 AS checked FROM relay_control_actions WHERE workspace=$1 AND job=$2 AND connection=$3 AND operation=$4 AND arguments=$5 AND rule_revision=$6 AND connection_revision=$7 AND effect='read' AND state='completed'",vec![job.workspace.clone().into(),job.reply.clone().into(),scope["connection"].as_str().unwrap().into(),scope["operation"].as_str().unwrap().into(),scope["arguments"].clone().into(),scope["rule_revision"].as_i64().unwrap().into(),scope["connection_revision"].as_i64().unwrap().into()]).await?.unwrap();
            checked_ms =
                checked_ms.min(row.try_get::<Option<i64>>("", "checked")?.context(
                    "Perform every reviewed source read in this update before publishing",
                )?);
        }
        let what_changed = input
            .what_changed
            .as_deref()
            .map(str::trim)
            .filter(|note| !note.is_empty());
        ensure!(
            what_changed.is_none_or(|note| note.chars().count() <= 280),
            "Keep what_changed to 280 characters: one or two plain sentences"
        );
        let bindings = match &input.bindings {
            None => Value::Null,
            Some(list) => match super::home_update_runs::living::keep_bindings(
                &tx,
                job,
                &data,
                &definition["sections"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
                list,
            )
            .await?
            {
                Ok(()) => {
                    json!({"saved": true, "message": "Saved: the controller now refreshes these numbers from the reads itself, and calls you only when something meaningful changes."})
                }
                Err(reason) => {
                    json!({"saved": false, "message": format!("Bindings not saved: {reason}")})
                }
            },
        };
        super::home_views::team::checked(
            &tx,
            job,
            &data,
            &definition["sections"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            bindings["saved"] == true,
        )
        .await?;
        let provenance = super::home_sources::capture(&tx, job, None, &[]).await?;
        let generation = data["expected_generation"].as_i64().unwrap() + 1;
        execute(&tx,"INSERT INTO relay_home_snapshots(workspace,panel,generation,panel_revision,definition,provenance,task,checked_ms,what_changed) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",vec![job.workspace.clone().into(),data["panel"].as_str().unwrap().into(),generation.into(),data["panel_revision"].as_i64().unwrap().into(),definition.into(),provenance.into(),job.reply.clone().into(),checked_ms.into(),what_changed.map(str::to_string).into()]).await?;
        execute(&tx,"UPDATE relay_home_update_runs SET published_generation=$3,request_hash=$4 WHERE workspace=$1 AND reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into(),generation.into(),hash.into()]).await?;
        execute(
            &tx,
            "UPDATE relay_home_maintenance SET generation=$3 WHERE workspace=$1 AND panel=$2",
            vec![
                job.workspace.clone().into(),
                data["panel"].as_str().unwrap().into(),
                generation.into(),
            ],
        )
        .await?;
        execute(
            &tx,
            "DELETE FROM relay_home_snapshots WHERE workspace=$1 AND panel=$2 AND generation<=$3",
            vec![
                job.workspace.clone().into(),
                data["panel"].as_str().unwrap().into(),
                (generation - 32).into(),
            ],
        )
        .await?;
        audit(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "home_snapshot_published",
            &job.reply,
        )
        .await?;
        tx.commit().await?;
        let mut result = json!({"state":"published","generation":generation});
        if !bindings.is_null() {
            result["bindings"] = bindings;
        }
        Ok(result)
    }
}
