mod actions;
pub(super) mod agent_folders;
pub(super) mod agent_tools;
mod agents;
pub(super) mod always;
pub(super) mod answers;
pub(super) mod descriptions;
pub(super) mod edits;
pub(super) mod fence;
pub(super) mod file_versions;
pub(super) mod folders;
pub(super) mod keeping;
pub(super) mod meaning;
pub(super) mod meaning_folders;
pub(super) mod nested;
mod pages;
pub(super) mod passages;
pub(super) mod rehearsals;
mod sharing;
pub(super) mod tables;
mod trash;
pub(super) mod upkeep;
mod uploads;
pub(super) mod watching;
pub(super) mod workbench;
mod workbench_save;
use super::*;
use crate::identity::library::model::*;
pub(super) use agents::source_allowed;
use serde_json::json;

/// The item `i` and the item whose sharing it follows, `s` (its folder, or itself).
const SOURCE: &str = "relay_library_items i JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=relay_library_source(i.workspace,i.id)";
const ACCESS: &str = "relay_library_access(i.workspace,i.id,$2) IS NOT NULL";
/// What a person was given, not counting an admin's access to everything.
const GRANTED: &str = "relay_library_granted(i.workspace,i.id,$2) IS NOT NULL";
/// Fields every view of an item adds: sharing comes from its folder.
const SHARED: &str = "'visibility',s.visibility,'folder_title',(SELECT f.title FROM relay_library_items f WHERE f.workspace=i.workspace AND f.id=i.folder),'editable',relay_library_access(i.workspace,i.id,$2)='edit','manageable',(i.steward=$2 OR relay_library_admin(i.workspace,$2)),'source',s.id,'granted',relay_library_granted(i.workspace,i.id,$2) IS NOT NULL,'starred',EXISTS(SELECT 1 FROM relay_library_stars st WHERE st.workspace=i.workspace AND st.item=i.id AND st.user_id=$2)";
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
        let root = value["source"].as_str().unwrap_or(id).to_owned();
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
        // An agent has this open on its computer in a request that is still running.
        if query(&tx, "SELECT to_regclass('relay_library_opened') IS NOT NULL AS present", vec![]).await?.context("Schema check")?.try_get::<bool>("", "present")? {
            value["working"] = json!(json_rows(&tx, "SELECT DISTINCT to_jsonb(a.name) AS data FROM relay_library_opened o JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.reply AND j.state='running' JOIN relay_shared_agents a ON a.workspace=o.workspace AND a.id=o.agent WHERE o.workspace=$1 AND o.item=$2", vec![w.into(), id.into()]).await?);
        }
        tx.commit().await?;
        nested::admin_opened(self, user, w, &value).await?;
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
            ["all", "private", "shared", "starred", "trash", "review", "everything", "agents"]
                .contains(&view)
                && search.len() <= 180
                && folder.len() <= 64
                && (0..=10000).contains(&offset),
            "Invalid Library filter"
        );
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        // Admins can open everything, but their own views list what was shared with
        // them; Everything lists the rest too.
        ensure!(
            view != "everything" || role(&tx, w, &user.id).await? == "Admin",
            "Only admins list everything"
        );
        // Inside a folder they opened, an admin sees what is in it.
        let reach = if view == "everything" || !folder.is_empty() {
            ACCESS
        } else {
            GRANTED
        };
        let mut opened = None;
        if !folder.is_empty() {
            let inside = item(&tx, user, w, folder, false).await?;
            ensure!(inside["kind"] == "folder", "This item is not a folder");
            opened = Some(inside);
        }
        // Browsing All shows one level (folders first); other views and searches show
        // every item wherever it is filed. People search like agents do: by the words'
        // forms ("refunded" finds "refund"), with the passage that matched.
        let due = upkeep::DUE;
        let sql = format!(
            "SELECT (to_jsonb(i)-'document'-'text'-'object_key') || jsonb_build_object({SHARED},'snippet',CASE WHEN $4<>'' THEN (SELECT regexp_replace(ts_headline('english',p.text,plainto_tsquery('english',$4),'MaxWords=24,MinWords=10'),'</?b>','','g') FROM relay_library_passages p WHERE p.workspace=i.workspace AND p.item=i.id AND p.revision=i.revision AND p.search @@ plainto_tsquery('english',$4) ORDER BY p.position LIMIT 1) END,'review_due',{due},'agent_count',(SELECT count(*) FROM relay_library_agents a WHERE a.workspace=s.workspace AND a.item=s.id),'item_count',CASE WHEN i.kind='folder' THEN (SELECT count(*) FROM relay_library_items c WHERE c.workspace=i.workspace AND c.folder=i.id AND NOT c.trashed) END) AS data FROM {SOURCE} WHERE i.workspace=$1 AND {reach} AND i.trashed=($3='trash') AND (to_jsonb(i)->>'purged') IS NULL AND ($3 NOT IN ('all','everything') OR $4!='' OR i.folder IS NOT DISTINCT FROM NULLIF($6,'')) AND ($3 NOT IN ('private','shared') OR (CASE WHEN $3='private' THEN s.visibility='private' ELSE s.visibility!='private' END)) AND ($3!='review' OR {due}) AND ($3!='agents' OR EXISTS(SELECT 1 FROM relay_library_versions v WHERE v.workspace=i.workspace AND v.item=i.id AND v.agent IS NOT NULL)) AND ($3!='starred' OR EXISTS(SELECT 1 FROM relay_library_stars st WHERE st.workspace=i.workspace AND st.item=i.id AND st.user_id=$2)) AND ($4='' OR strpos(lower(i.title || ' ' || i.text),lower($4))>0 OR EXISTS(SELECT 1 FROM relay_library_passages p WHERE p.workspace=i.workspace AND p.item=i.id AND p.revision=i.revision AND p.search @@ plainto_tsquery('english',$4))) ORDER BY (i.kind='folder' AND $3 IN ('all','everything') AND $4='') DESC,i.updated DESC,i.id LIMIT 51 OFFSET $5"
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
        // An admin looking inside a folder that was not shared with them is audited too.
        if let Some(inside) = &opened {
            nested::admin_opened(self, user, w, inside).await?;
        }
        Ok(json!({"items":items,"more":more}))
    }
    pub async fn library_file(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let value = item(&tx, user, w, id, false).await?;
        ensure!(value["kind"] == "file", "This item is a page");
        tx.commit().await?;
        nested::admin_opened(self, user, w, &value).await?;
        Ok(value)
    }
}
