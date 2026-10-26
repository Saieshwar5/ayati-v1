use super::*;
use crate::identity::home_panels::model::{Layout, Review};
use serde_json::json;
async fn version(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    id: &str,
    revision: i64,
) -> Result<Value> {
    let row=query(tx,"SELECT v.definition,v.provenance,v.saved_ms,v.task,m.chat,m.thread FROM relay_home_panel_versions v JOIN relay_chat_messages m ON m.workspace=v.workspace AND m.id=v.task WHERE v.workspace=$1 AND v.panel=$2 AND v.revision=$3",vec![w.into(),id.into(),revision.into()]).await?.context("Panel snapshot unavailable")?;
    let provenance: Value = row.try_get("", "provenance")?;
    if !super::home_sources::allowed(tx, w, &user.id, &provenance).await? {
        return Ok(json!({"revision":revision,"available":false}));
    }
    Ok(
        json!({"revision":revision,"available":true,"definition":row.try_get::<Value>("","definition")?,"saved_ms":row.try_get::<i64>("","saved_ms")?,"source_operations":provenance["operations"].as_array().map_or(0,Vec::len),"chat":row.try_get::<String>("","chat")?,"thread":row.try_get::<Option<String>>("","thread")?,"task":row.try_get::<String>("","task")?}),
    )
}
impl PgStore {
    pub(crate) async fn home_panels(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let mut panels = vec![];
        for r in rows(&tx,"SELECT p.id,p.agent,p.revision,p.active_revision,a.name AS agent_name FROM relay_home_panels p JOIN relay_shared_agents a ON a.workspace=p.workspace AND a.id=p.agent WHERE p.workspace=$1 AND p.owner_id=$2 ORDER BY p.created_ms,p.id LIMIT 32",vec![w.into(),user.id.clone().into()]).await? {
   let id:String=r.try_get("","id")?;let revision:i64=r.try_get("","revision")?;let active:Option<i64>=r.try_get("","active_revision")?;
   let draft=version(&tx,user,w,&id,revision).await?;
   let saved=if active==Some(revision){Some(draft.clone())}else if let Some(n)=active {Some(version(&tx,user,w,&id,n).await?)}else{None};
   let visible=draft["available"]==true || saved.as_ref().is_some_and(|s|s["available"]==true);
   panels.push(json!({"id":id,"revision":revision,"active_revision":active,"agent":if visible {Some(r.try_get::<String>("","agent")?)}else{None},"agent_name":if visible {Some(r.try_get::<String>("","agent_name")?)}else{None},"draft":if active==Some(revision){None}else{Some(draft)},"active":saved}));
  }
        let layout = super::home_receipts::layout(&tx, user, w).await?;
        tx.commit().await?;
        Ok(json!({"available":true,"panels":panels,"layout":layout}))
    }
    pub(crate) async fn review_home_panel(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Review,
    ) -> Result<Value> {
        ensure!(
            input.revision > 0 && input.revision <= 100,
            "Invalid panel review revision"
        );
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let row = query(
            &tx,
            "SELECT revision FROM relay_home_panels WHERE workspace=$1 AND id=$2 AND owner_id=$3",
            vec![w.into(), id.into(), user.id.clone().into()],
        )
        .await?
        .context("Personal panel unavailable")?;
        let payload = json!({"review":id,"input":input});
        if let Some(saved) =
            super::home_receipts::retry(&tx, user, w, &input.request, &payload).await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        ensure!(
            row.try_get::<i64>("", "revision")? == input.revision,
            "Panel changed; review the latest draft before keeping it"
        );
        ensure!(
            version(&tx, user, w, id, input.revision).await?["available"] == true,
            "Panel source unavailable; no publication was saved"
        );
        execute(
            &tx,
            "UPDATE relay_home_panels SET active_revision=$3 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), input.revision.into()],
        )
        .await?;
        let result = json!({"id":id,"active_revision":input.revision});
        super::home_receipts::remember(&tx, user, w, &input.request, &payload, &result).await?;
        audit(&tx, &user.id, Some(w), "home_panel_kept", id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn save_home_layout(
        &self,
        user: &User,
        w: &str,
        input: &Layout,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let payload = json!({"layout":input});
        if let Some(saved) =
            super::home_receipts::retry(&tx, user, w, &input.request, &payload).await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        let current = super::home_receipts::layout(&tx, user, w).await?;
        ensure!(
            current["revision"] == input.revision,
            "Home arrangement changed on another device; refresh before arranging again"
        );
        let ids = input
            .order
            .iter()
            .chain(&input.wide)
            .chain(&input.hidden)
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let n=rows(&tx,"SELECT id FROM relay_home_panels WHERE workspace=$1 AND owner_id=$2 AND id IN (SELECT jsonb_array_elements_text($3::jsonb))",vec![w.into(),user.id.clone().into(),json!(ids).into()]).await?.len();
        ensure!(
            n == ids.len(),
            "Home arrangement contains an unavailable personal panel"
        );
        let revision = input.revision + 1;
        let result = json!({"revision":revision,"order":input.order,"wide":input.wide,"hidden":input.hidden});
        execute(&tx,"INSERT INTO relay_home_layouts VALUES($1,$2,$3,$4) ON CONFLICT(workspace,owner_id) DO UPDATE SET revision=excluded.revision,data=excluded.data",vec![w.into(),user.id.clone().into(),revision.into(),result.clone().into()]).await?;
        super::home_receipts::remember(&tx, user, w, &input.request, &payload, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}
