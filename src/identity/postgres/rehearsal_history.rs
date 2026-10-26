use super::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    at: i64,
    id: String,
}

impl PgStore {
    pub(crate) async fn rehearsal_history(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        before: Option<&str>,
    ) -> Result<Value> {
        let mut cursor = Cursor {
            at: i64::MAX,
            id: "z".into(),
        };
        if let Some(before) = before {
            ensure!(before.len() <= 256, "Invalid sample history cursor");
            cursor = serde_json::from_str(before).context("Invalid sample history cursor")?;
            ensure!(cursor.at >= 0, "Invalid sample history cursor");
            crate::identity::rehearsals::model::uuid(&cursor.id)?;
        }
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let mut items = Vec::new();
        loop {
            let records=json_rows(&tx,"SELECT jsonb_build_object('workspace',workspace,'id',id,'agent',agent,'actor',actor,'input',input,'profile_revision',profile->'revision','note_refs',note_refs,'created_ms',created_ms,'started_ms',started_ms,'finished_ms',finished_ms,'state',state,'detail',detail) AS data FROM relay_agent_rehearsals WHERE workspace=$1 AND actor=$2 AND agent=$3 AND (created_ms,id)<($4,$5) ORDER BY created_ms DESC,id DESC LIMIT 26",vec![w.into(),user.id.clone().into(),agent.into(),cursor.at.into(),cursor.id.clone().into()]).await?;
            let exhausted = records.len() < 26;
            for data in records {
                cursor = Cursor {
                    at: data["created_ms"].as_i64().unwrap(),
                    id: data["id"].as_str().unwrap().into(),
                };
                if super::rehearsal_context::readable(&tx, user, w, agent, &data)
                    .await
                    .is_err()
                {
                    continue;
                }
                items.push(json!({"id":data["id"],"state":data["state"],"detail":data["detail"],
                    "task":data["input"]["task"].as_str().unwrap_or("Sample task").chars().take(180).collect::<String>(),
                    "profile_revision":data["profile_revision"],"created_ms":data["created_ms"],
                    "started_ms":data["started_ms"],"finished_ms":data["finished_ms"]}));
                if items.len() == 26 {
                    break;
                }
            }
            if exhausted || items.len() == 26 {
                break;
            }
        }
        let more = items.len() > 25;
        items.truncate(25);
        let next = if more {
            items
                .last()
                .map(|i| {
                    serde_json::to_string(&Cursor {
                        at: i["created_ms"].as_i64().unwrap(),
                        id: i["id"].as_str().unwrap().into(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        tx.commit().await?;
        Ok(json!({"samples":items,"next_before":next}))
    }
}
