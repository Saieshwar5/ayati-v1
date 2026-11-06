mod actions;
pub(super) mod agent_tools;
mod agents;
pub(super) mod always;
pub(super) mod answers;
pub(super) mod edits;
pub(super) mod fence;
pub(super) mod folders;
pub(super) mod keeping;
pub(super) mod meaning;
mod pages;
pub(super) mod passages;
pub(super) mod rehearsals;
mod sharing;
pub(super) mod tables;
pub(super) mod upkeep;
mod uploads;
use super::*;
use crate::identity::library::model::*;
pub(super) use agents::source_allowed;
use serde_json::json;

/// The item `i` and the item whose sharing it follows, `s` (its folder, or itself).
const SOURCE: &str = "relay_library_items i JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id)";
const ACCESS: &str = "relay_library_access(i.workspace,i.id,$2) IS NOT NULL";
/// Fields every view of an item adds: sharing comes from its folder.
const SHARED: &str = "'visibility',s.visibility,'folder_title',CASE WHEN i.folder IS NULL THEN NULL ELSE s.title END,'editable',relay_library_access(i.workspace,i.id,$2)='edit','manageable',i.steward=$2,'starred',EXISTS(SELECT 1 FROM relay_library_stars st WHERE st.workspace=i.workspace AND st.item=i.id AND st.user_id=$2)";
async fn item(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
    trash: bool,
) -> Result<Value> {
    role(db, w, &user.id).await?;
    let sql = format!(
        "SELECT to_jsonb(i) || jsonb_build_object({SHARED}) AS data FROM {SOURCE} WHERE i.workspace=$1 AND i.id=$3 AND {ACCESS} AND ($4 OR NOT i.trashed)"
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
        let root = value["folder"].as_str().unwrap_or(id).to_owned();
        value["people"] = json!(json_rows(&tx,"SELECT jsonb_build_object('user',g.user_id,'access',g.access,'name',COALESCE(u.display_name,u.email)) AS data FROM relay_library_people g JOIN relay_users u ON u.id=g.user_id WHERE g.workspace=$1 AND g.item=$2 ORDER BY g.user_id",vec![w.into(),root.clone().into()]).await?);
        // Only expose agent identities that this requester can use.
        let mut agents = vec![];
        for grant in json_rows(
            &tx,
            "SELECT jsonb_build_object('agent',agent,'level',level,'use_for',use_for) AS data FROM relay_library_agents WHERE workspace=$1 AND item=$2 ORDER BY agent",
            vec![w.into(), root.into()],
        )
        .await?
        {
            let agent = grant["agent"].as_str().unwrap_or_default();
            if super::chat::agent_access(&tx, &user.id, w, agent).await? {
                agents.push(grant);
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
        folder: &str,
        offset: i64,
    ) -> Result<Value> {
        ensure!(
            ["all", "private", "shared", "starred", "trash", "review"].contains(&view)
                && search.len() <= 180
                && folder.len() <= 64
                && (0..=10000).contains(&offset),
            "Invalid Library filter"
        );
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if !folder.is_empty() {
            ensure!(
                item(&tx, user, w, folder, false).await?["kind"] == "folder",
                "This item is not a folder"
            );
        }
        // Browsing All shows one level (folders first); other views and searches show
        // every item wherever it is filed.
        let due = upkeep::DUE;
        let sql = format!(
            "SELECT (to_jsonb(i)-'document'-'text'-'object_key') || jsonb_build_object({SHARED},'review_due',{due},'agent_count',(SELECT count(*) FROM relay_library_agents a WHERE a.workspace=s.workspace AND a.item=s.id),'item_count',CASE WHEN i.kind='folder' THEN (SELECT count(*) FROM relay_library_items c WHERE c.workspace=i.workspace AND c.folder=i.id AND NOT c.trashed) END) AS data FROM {SOURCE} WHERE i.workspace=$1 AND {ACCESS} AND i.trashed=($3='trash') AND (to_jsonb(i)->>'purged') IS NULL AND ($3!='all' OR $4!='' OR i.folder IS NOT DISTINCT FROM NULLIF($6,'')) AND ($3 NOT IN ('private','shared') OR (CASE WHEN $3='private' THEN s.visibility='private' ELSE s.visibility!='private' END)) AND ($3!='review' OR {due}) AND ($3!='starred' OR EXISTS(SELECT 1 FROM relay_library_stars st WHERE st.workspace=i.workspace AND st.item=i.id AND st.user_id=$2)) AND ($4='' OR strpos(lower(i.title || ' ' || i.text),lower($4))>0) ORDER BY (i.kind='folder' AND $3='all' AND $4='') DESC,i.updated DESC,i.id LIMIT 51 OFFSET $5"
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
                folder.into(),
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
