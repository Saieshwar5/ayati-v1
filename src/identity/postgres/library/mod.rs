mod actions;
mod agents;
mod pages;
mod sharing;
mod uploads;
use super::*;
use crate::identity::library::model::*;
pub(super) use agents::source_allowed;
use serde_json::json;

const ACCESS: &str = "(i.steward=$2 OR i.visibility='workspace' OR (i.visibility='selected' AND EXISTS(SELECT 1 FROM relay_library_people g WHERE g.workspace=i.workspace AND g.item=i.id AND g.user_id=$2)))";
async fn item(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
    trash: bool,
) -> Result<Value> {
    role(db, w, &user.id).await?;
    let sql = format!(
        "SELECT to_jsonb(i) || jsonb_build_object('editable',i.steward=$2 OR EXISTS(SELECT 1 FROM relay_library_people g WHERE g.workspace=i.workspace AND g.item=i.id AND g.user_id=$2 AND g.access='edit'),'manageable',i.steward=$2,'starred',EXISTS(SELECT 1 FROM relay_library_stars s WHERE s.workspace=i.workspace AND s.item=i.id AND s.user_id=$2)) AS data FROM relay_library_items i WHERE i.workspace=$1 AND i.id=$3 AND {ACCESS} AND ($4 OR NOT i.trashed)"
    );
    query(
        db,
        &sql,
        vec![w.into(), user.id.clone().into(), id.into(), trash.into()],
    )
    .await?
    .ok_or(Denied.into())
    .and_then(|r| Ok(r.try_get("", "data")?))
}
fn public(mut item: Value) -> Value {
    if let Some(v) = item.as_object_mut() {
        v.remove("object_key");
        v.remove("text");
    }
    item
}
async fn retry(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    request: &str,
    payload: &Value,
) -> Result<Option<Value>> {
    uuid(request)?;
    let row = query(db,"SELECT payload,result FROM relay_library_requests WHERE workspace=$1 AND actor=$2 AND id=$3",vec![w.into(),user.id.clone().into(),request.into()]).await?;
    if let Some(row) = row {
        ensure!(
            row.try_get::<Value>("", "payload")? == json!(checksum(payload.to_string().as_bytes())),
            "Request ID already used for different changes"
        );
        return Ok(Some(row.try_get("", "result")?));
    }
    Ok(None)
}
async fn remember(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    request: &str,
    payload: Value,
    result: &Value,
) -> Result<()> {
    execute(db,"INSERT INTO relay_library_requests(workspace,actor,id,payload,result) VALUES($1,$2,$3,$4,$5)",vec![w.into(),user.id.clone().into(),request.into(),json!(checksum(payload.to_string().as_bytes())).into(),result.clone().into()]).await?;
    Ok(())
}
impl PgStore {
    pub async fn library_get(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let mut value = public(item(&tx, user, w, id, false).await?);
        value["people"] = json!(json_rows(&tx,"SELECT jsonb_build_object('user',g.user_id,'access',g.access,'name',COALESCE(u.display_name,u.email)) AS data FROM relay_library_people g JOIN relay_users u ON u.id=g.user_id WHERE g.workspace=$1 AND g.item=$2 ORDER BY g.user_id",vec![w.into(),id.into()]).await?);
        // Only expose agent identities that this requester can use.
        let mut agents = vec![];
        for row in rows(
            &tx,
            "SELECT agent FROM relay_library_agents WHERE workspace=$1 AND item=$2",
            vec![w.into(), id.into()],
        )
        .await?
        {
            let agent: String = row.try_get("", "agent")?;
            if super::chat::agent_access(&tx, &user.id, w, &agent).await? {
                agents.push(agent);
            }
        }
        value["agents"] = json!(agents);
        tx.commit().await?;
        Ok(value)
    }
    pub async fn library_list(
        &self,
        user: &User,
        w: &str,
        view: &str,
        search: &str,
        offset: i64,
    ) -> Result<Value> {
        ensure!(
            ["all", "private", "shared", "starred", "trash"].contains(&view)
                && search.len() <= 180
                && (0..=10000).contains(&offset),
            "Invalid Library filter"
        );
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let sql = format!(
            "SELECT (to_jsonb(i)-'document'-'text'-'object_key') || jsonb_build_object('manageable',i.steward=$2,'agent_count',(SELECT count(*) FROM relay_library_agents a WHERE a.workspace=i.workspace AND a.item=i.id),'editable',i.steward=$2 OR EXISTS(SELECT 1 FROM relay_library_people g WHERE g.workspace=i.workspace AND g.item=i.id AND g.user_id=$2 AND g.access='edit'),'starred',EXISTS(SELECT 1 FROM relay_library_stars s WHERE s.workspace=i.workspace AND s.item=i.id AND s.user_id=$2)) AS data FROM relay_library_items i WHERE i.workspace=$1 AND {ACCESS} AND i.trashed=($3='trash') AND ($3 NOT IN ('private','shared') OR (CASE WHEN $3='private' THEN i.visibility='private' ELSE i.visibility!='private' END)) AND ($3!='starred' OR EXISTS(SELECT 1 FROM relay_library_stars s WHERE s.workspace=i.workspace AND s.item=i.id AND s.user_id=$2)) AND ($4='' OR strpos(lower(i.title || ' ' || i.text),lower($4))>0) ORDER BY i.updated DESC,i.id LIMIT 51 OFFSET $5"
        );
        let mut items = json_rows(
            &tx,
            &sql,
            vec![
                w.into(),
                user.id.clone().into(),
                view.into(),
                search.into(),
                offset.into(),
            ],
        )
        .await?;
        let more = items.len() > 50;
        items.truncate(50);
        tx.commit().await?;
        Ok(json!({"items":items,"more":more}))
    }
    pub async fn library_file(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let value = item(&tx, user, w, id, false).await?;
        ensure!(value["kind"] == "file", "This item is a page");
        tx.commit().await?;
        Ok(value)
    }
}
