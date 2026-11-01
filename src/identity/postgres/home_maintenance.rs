use super::*;
use crate::identity::home_panels::maintenance::Agreement;
use serde_json::json;

pub(super) async fn owned(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
) -> Result<Value> {
    role(db, w, &user.id).await?;
    query(db,"SELECT to_jsonb(p)||jsonb_build_object('definition',v.definition,'provenance',v.provenance,'maintenance',to_jsonb(u)) AS data FROM relay_home_panels p JOIN relay_home_panel_versions v ON v.workspace=p.workspace AND v.panel=p.id AND v.revision=p.active_revision LEFT JOIN relay_home_maintenance u ON u.workspace=p.workspace AND u.panel=p.id WHERE p.workspace=$1 AND p.id=$2 AND p.owner_id=$3",vec![w.into(),id.into(),user.id.clone().into()]).await?.context("Reviewed personal panel unavailable")?.try_get("","data").map_err(Into::into)
}
pub(super) async fn authorized(
    db: &DatabaseTransaction,
    user: &User,
    w: &str,
    p: &Value,
) -> Result<()> {
    let profile =
        super::agent_profiles::managed_access(db, &user.id, w, p["agent"].as_str().unwrap(), false)
            .await?;
    ensure!(
        profile["revision"].as_i64().is_some_and(|n| n > 0),
        "Use a managed agent for panel updates"
    );
    super::chat::access(db, user, w, p["chat"].as_str().unwrap()).await?;
    ensure!(
        super::chat::agent_access(db, &user.id, w, p["agent"].as_str().unwrap()).await?,
        "Current agent request permission is required"
    );
    ensure!(query(db,"SELECT a.agent FROM relay_chat_agents a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.chat=$2 AND a.agent=$3 AND (NOT c.announcements OR EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=$1 AND m.user_id=$4 AND m.role='Admin'))",vec![w.into(),p["chat"].as_str().unwrap().into(),p["agent"].as_str().unwrap().into(),user.id.clone().into()]).await?.is_some(),"Agent participation and posting authority are required");
    super::channel_lifecycle::accepting(db, w, p["chat"].as_str().unwrap()).await?;
    ensure!(
        p["revision"] == p["active_revision"],
        "Review the current draft before authorizing panel updates"
    );
    ensure!(
        super::home_sources::allowed(db, w, &user.id, &p["provenance"]).await?,
        "Panel source unavailable"
    );
    if !p["maintenance"].is_null() {
        ensure!(
            p["maintenance"]["panel_revision"] == p["active_revision"],
            "Review a new update agreement for this panel revision"
        );
        ensure!(
            super::home_update_access::scopes(db, w, &p["provenance"]).await?
                == p["maintenance"]["scopes"],
            "Reviewed source authority changed"
        );
    }
    Ok(())
}
impl PgStore {
    pub(crate) async fn save_home_agreement(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Agreement,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        let p = owned(&tx, user, w, id).await?;
        let payload = json!({"agreement":id,"input":input});
        if let Some(saved) =
            super::home_receipts::retry(&tx, user, w, &input.request, &payload).await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        let previous = &p["maintenance"];
        ensure!(
            previous["revision"].as_i64().unwrap_or(0) == input.revision
                && p["active_revision"] == input.panel_revision,
            "Panel or agreement changed; refresh before saving"
        );
        let scopes = if input.enabled {
            ensure!(
                self.pooled,
                "Panel updates require pooled background execution"
            );
            // A new reviewed agreement may replace an obsolete one.
            let mut fresh = p.clone();
            fresh["maintenance"] = Value::Null;
            authorized(&tx, user, w, &fresh).await?;
            super::home_update_access::scopes(&tx, w, &p["provenance"]).await?
        } else {
            ensure!(!previous.is_null(), "No update agreement to pause");
            previous["scopes"].clone()
        };
        let old_schedule = previous["schedule"].as_str();
        super::home_update_runs::cancel_unstarted(&tx, w, id).await?;
        if let Some(old) = old_schedule {
            execute(&tx,"UPDATE relay_agent_schedules SET state='paused',revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),old.into()]).await?;
            super::automation_dispatch::cancel_unstarted(&tx, w, old).await?;
        }
        let schedule = if let Some(interval) = input.interval_seconds {
            if old_schedule.is_some() && previous["interval_seconds"] == interval {
                old_schedule.map(str::to_owned)
            } else if input.enabled {
                ensure!(query(&tx,"SELECT count(*) AS n FROM relay_agent_schedules WHERE workspace=$1 AND agent=$2",vec![w.into(),p["agent"].as_str().unwrap().into()]).await?.unwrap().try_get::<i64>("","n")?<100,"Agent routine limit reached");
                let sid = uuid::Uuid::new_v4().to_string();
                execute(&tx,"INSERT INTO relay_agent_schedules(workspace,id,agent,principal,chat,name,content,first_ms,interval_seconds,timezone,request_hash,next_ms) VALUES($1,$2,$3,$4,$5,'Personal Home panel update','Refresh the controller-bound personal Home panel. Use home_panel_update_context, perform its reviewed reads, then refresh_home_panel.',(extract(epoch FROM clock_timestamp())*1000)::bigint+$6::bigint*1000,$6,'UTC',$2,(extract(epoch FROM clock_timestamp())*1000)::bigint+$6::bigint*1000)",vec![w.into(),sid.clone().into(),p["agent"].as_str().unwrap().into(),user.id.clone().into(),p["chat"].as_str().unwrap().into(),interval.into()]).await?;
                execute(
                    &tx,
                    "INSERT INTO relay_home_schedule_bindings VALUES($1,$2,$3)",
                    vec![w.into(), sid.clone().into(), id.into()],
                )
                .await?;
                Some(sid)
            } else {
                None
            }
        } else {
            None
        };
        if input.enabled
            && let Some(sid) = &schedule
        {
            execute(&tx,"UPDATE relay_agent_schedules SET state='enabled',revision=revision+1,detail='' WHERE workspace=$1 AND id=$2",vec![w.into(),sid.clone().into()]).await?;
        }
        execute(&tx,"INSERT INTO relay_home_maintenance(workspace,panel,revision,panel_revision,enabled,instructions,interval_seconds,stale_seconds,scopes,schedule) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(workspace,panel) DO UPDATE SET revision=excluded.revision,panel_revision=excluded.panel_revision,enabled=excluded.enabled,instructions=excluded.instructions,interval_seconds=excluded.interval_seconds,stale_seconds=excluded.stale_seconds,scopes=excluded.scopes,schedule=excluded.schedule",vec![w.into(),id.into(),(input.revision+1).into(),input.panel_revision.into(),input.enabled.into(),input.instructions.trim().into(),input.interval_seconds.into(),input.stale_seconds.into(),scopes.into(),schedule.into()]).await?;
        if super::home_update_runs::living::available(&tx).await? {
            execute(&tx,"UPDATE relay_home_maintenance SET follow_sources=COALESCE($3,follow_sources),min_gap_seconds=COALESCE($4,min_gap_seconds),bindings=NULL WHERE workspace=$1 AND panel=$2",vec![w.into(),id.into(),input.follow_sources.into(),input.min_gap_seconds.into()]).await?;
        }
        let result = json!({"revision":input.revision+1,"enabled":input.enabled});
        super::home_receipts::remember(&tx, user, w, &input.request, &payload, &result).await?;
        audit(&tx, &user.id, Some(w), "home_update_agreement", id).await?;
        tx.commit().await?;
        Ok(result)
    }
}
