//! Schema 108 (connector layer P2) in the store: provider routes per workspace
//! and app, set by Cuckoo's administrators (a route only chooses the provider
//! for new connections); the Add app catalog, searched and paged on the server;
//! and one-step connect requests that answer the same way when repeated.
use super::control_actions::guard::MEANINGS;
use super::*;
use crate::identity::connectors::curated::CATEGORIES;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
#[cfg(test)]
#[path = "connector_catalog_tests.rs"]
mod catalog_tests;
#[cfg(test)]
#[path = "connector_routing_tests.rs"]
pub(super) mod tests;

/// Schema 108: routes, account identity, connected by and when, connect requests.
pub(crate) struct ConnectorRouting;
impl MigrationName for ConnectorRouting {
    fn name(&self) -> &str {
        "m20261108_000108_connector_routing"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConnectorRouting {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("connector_routing.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Connector routes require a forward migration".into(),
        ))
    }
}

impl PgStore {
    /// This workspace's own routes: app to provider.
    pub(crate) async fn connector_workspace_routes(
        &self,
        w: &str,
    ) -> Result<BTreeMap<String, String>> {
        let rows = rows(
            &self.db,
            "SELECT app,provider FROM relay_connector_routes WHERE workspace=$1",
            vec![w.into()],
        )
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get("", "app")?, r.try_get("", "provider")?)))
            .collect()
    }

    /// Every workspace route, newest first, for Cuckoo's administrators.
    pub(crate) async fn connector_routes(&self) -> Result<Vec<Value>> {
        json_rows(&self.db, "SELECT jsonb_build_object('workspace',r.workspace,'workspace_name',w.name,'app',r.app,'provider',r.provider,'note',r.note,'changed_by',COALESCE(u.display_name,u.email),'changed',r.changed) AS data FROM relay_connector_routes r JOIN relay_workspaces w ON w.id=r.workspace LEFT JOIN relay_users u ON u.id=r.changed_by ORDER BY r.changed DESC,r.workspace,r.app LIMIT 500", vec![]).await
    }

    /// Routes `app` in workspace `w` to `provider`, or back to the defaults
    /// with `None`. The caller checks the provider serves the app.
    pub(crate) async fn connector_route_set(
        &self,
        user: &User,
        w: &str,
        app: &str,
        provider: Option<&str>,
        note: &str,
    ) -> Result<()> {
        crate::identity::identifier(app)?;
        let note = note.trim();
        ensure!(
            note.chars().count() <= 300 && !note.chars().any(char::is_control),
            "Keep the note to one line of 300 characters"
        );
        let tx = self.db.begin().await?;
        ensure!(
            query(
                &tx,
                "SELECT id FROM relay_workspaces WHERE id=$1",
                vec![w.into()]
            )
            .await?
            .is_some(),
            "Workspace unavailable"
        );
        match provider {
            Some(provider) => execute(&tx, "INSERT INTO relay_connector_routes(workspace,app,provider,note,changed_by,changed) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(workspace,app) DO UPDATE SET provider=excluded.provider,note=excluded.note,changed_by=excluded.changed_by,changed=excluded.changed",
                vec![w.into(), app.into(), provider.into(), note.into(), user.id.clone().into(), now().into()]).await?,
            None => execute(&tx, "DELETE FROM relay_connector_routes WHERE workspace=$1 AND app=$2", vec![w.into(), app.into()]).await?,
        };
        audit(&tx, &user.id, Some(w), "connector_route_set", app).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Apps per catalog page.
pub(crate) const PAGE: usize = 30;

/// A catalog app this workspace can connect, through its routed provider.
pub(crate) struct Offer {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    /// The provider's operations: `{id, title, tool, effect}` each.
    pub operations: Value,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub cursor: String,
}

/// How a connect request starts.
pub(crate) enum Begun {
    /// This request already finished: its saved answer.
    Answered(Value),
    /// Connect this connection id, made from this revision.
    Start { connection: String, revision: i64 },
}

/// Connected first, then by name.
fn sort_key(app: &Value) -> (bool, String, String) {
    (
        app["state"] != "connected",
        app["name"].as_str().unwrap_or_default().to_lowercase(),
        app["id"].as_str().unwrap_or_default().to_owned(),
    )
}

impl PgStore {
    /// One page of the catalog for any member: matching `query` on name and
    /// description, in `category` when given, connected apps first.
    pub(crate) async fn connector_catalog(
        &self,
        user: &User,
        w: &str,
        offers: &[Offer],
        input: &CatalogQuery,
    ) -> Result<Value> {
        let query_text = input.query.trim().to_lowercase();
        ensure!(
            query_text.chars().count() <= 100,
            "Search with up to 100 characters"
        );
        ensure!(
            input.category.is_empty() || CATEGORIES.iter().any(|(id, _)| *id == input.category),
            "Unknown category"
        );
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let listed: Value = offers
            .iter()
            .map(|o| json!({"id":o.id,"operations":o.operations}))
            .collect();
        let facts = json_rows(&tx, "SELECT jsonb_build_object('id',o.id,'connection',c.id,
              'requested',EXISTS(SELECT 1 FROM relay_app_requests r WHERE r.workspace=$1 AND r.connector=o.id AND r.requester=$2 AND r.state='pending'),
              'kinds',(SELECT COALESCE(jsonb_agg(DISTINCT CASE WHEN c.id IS NULL THEN relay_guess_meaning(op) ELSE relay_action_meaning($1,c.id,op) END),'[]'::jsonb)
                FROM jsonb_array_elements(COALESCE(c.definition->'operations',o.operations)) op),
              'actions',jsonb_array_length(COALESCE(c.definition->'operations',o.operations))) AS data
            FROM jsonb_to_recordset($3::jsonb) AS o(id text, operations jsonb)
            LEFT JOIN relay_control_connections c ON c.workspace=$1 AND c.server=o.id AND c.enabled
             AND EXISTS(SELECT 1 FROM relay_control_systems s WHERE s.workspace=c.workspace AND s.server=c.server AND s.approved)",
            vec![w.into(), user.id.clone().into(), listed.into()]).await?;
        tx.commit().await?;
        let facts: std::collections::HashMap<String, Value> = facts
            .into_iter()
            .map(|f| (f["id"].as_str().unwrap_or_default().to_owned(), f))
            .collect();
        let mut apps: Vec<Value> = offers
            .iter()
            .filter_map(|offer| Some((offer, facts.get(&offer.id)?)))
            .map(|(offer, fact)| {
                let state = if !fact["connection"].is_null() {
                    "connected"
                } else if fact["requested"] == true {
                    "requested"
                } else {
                    "available"
                };
                let kinds: Vec<&str> = MEANINGS
                    .iter()
                    .copied()
                    .filter(|m| {
                        fact["kinds"]
                            .as_array()
                            .is_some_and(|k| k.iter().any(|v| v == m))
                    })
                    .collect();
                json!({"id":offer.id,"name":offer.name,"description":offer.description,
                    "category":offer.category,"kinds":kinds,"actions":fact["actions"],
                    "state":state,"connection":fact["connection"]})
            })
            .filter(|app| {
                query_text.is_empty()
                    || [&app["name"], &app["description"]].iter().any(|t| {
                        t.as_str()
                            .is_some_and(|t| t.to_lowercase().contains(&query_text))
                    })
            })
            .collect();
        let categories: Vec<Value> = CATEGORIES
            .iter()
            .map(|(id, name)| json!({"id":id,"name":name,"count":apps.iter().filter(|a| a["category"] == *id).count()}))
            .collect();
        apps.retain(|a| input.category.is_empty() || a["category"] == input.category.as_str());
        apps.sort_by_key(sort_key);
        if !input.cursor.is_empty() {
            let after = cursor_key(&input.cursor, offers)?;
            apps.retain(|a| sort_key(a) > after);
        }
        let more = apps.len() > PAGE;
        apps.truncate(PAGE);
        let next = more.then(|| {
            let last = apps.last().unwrap();
            format!(
                "{}.{}",
                u8::from(last["state"] != "connected"),
                last["id"].as_str().unwrap_or_default()
            )
        });
        Ok(json!({"categories":categories,"apps":apps,"next_cursor":next,"can_manage":admin}))
    }

    /// Starts one-step connect for an admin: approves the app and picks the
    /// connection to connect. A repeated `request` returns its saved answer.
    pub(crate) async fn connector_connect_begin(
        &self,
        user: &User,
        w: &str,
        request: &str,
        app: &str,
    ) -> Result<Begun> {
        crate::identity::worker_control::model::uuid(request)?;
        crate::identity::identifier(app)?;
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        if let Some(row) = query(&tx, "SELECT app,requested_by,answer FROM relay_connector_connects WHERE workspace=$1 AND request=$2", vec![w.into(), request.into()]).await? {
            ensure!(
                row.try_get::<String>("", "requested_by")? == user.id
                    && row.try_get::<String>("", "app")? == app,
                "This connect request was already used; start again"
            );
            let answer: Option<Value> = row.try_get("", "answer")?;
            tx.commit().await?;
            return Ok(Begun::Answered(answer.context(
                "This app is still connecting; wait a moment and try again",
            )?));
        }
        execute(
            &tx,
            "DELETE FROM relay_connector_connects WHERE workspace=$1 AND created<$2",
            vec![w.into(), (now() - 86_400).into()],
        )
        .await?;
        let approved = query(
            &tx,
            "SELECT approved FROM relay_control_systems WHERE workspace=$1 AND server=$2",
            vec![w.into(), app.into()],
        )
        .await?
        .map(|r| r.try_get::<bool>("", "approved"))
        .transpose()?;
        if approved != Some(true) {
            execute(&tx,"INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES($1,$2,true,1) ON CONFLICT(workspace,server) DO UPDATE SET approved=true,revision=relay_control_systems.revision+1",vec![w.into(),app.into()]).await?;
            audit(&tx, &user.id, Some(w), "control_system_approval_saved", app).await?;
        }
        let (connection, revision) = match query(
            &tx,
            "SELECT id,revision FROM relay_control_connections WHERE workspace=$1 AND server=$2",
            vec![w.into(), app.into()],
        )
        .await?
        {
            Some(row) => (
                row.try_get::<String>("", "id")?,
                row.try_get::<i64>("", "revision")?,
            ),
            None => (uuid::Uuid::new_v4().to_string(), 0),
        };
        execute(&tx, "INSERT INTO relay_connector_connects(workspace,request,app,connection,requested_by,created) VALUES($1,$2,$3,$4,$5,$6)",
            vec![w.into(), request.into(), app.into(), connection.clone().into(), user.id.clone().into(), now().into()]).await?;
        tx.commit().await?;
        Ok(Begun::Start {
            connection,
            revision,
        })
    }

    /// Saves a connect request's answer, or forgets a failed request so trying
    /// again runs again.
    pub(crate) async fn connector_connect_done(
        &self,
        w: &str,
        request: &str,
        answer: Option<&Value>,
    ) -> Result<()> {
        match answer {
            Some(answer) => execute(&self.db, "UPDATE relay_connector_connects SET answer=$3 WHERE workspace=$1 AND request=$2", vec![w.into(), request.into(), answer.clone().into()]).await?,
            None => execute(&self.db, "DELETE FROM relay_connector_connects WHERE workspace=$1 AND request=$2 AND answer IS NULL", vec![w.into(), request.into()]).await?,
        };
        Ok(())
    }
}

/// Where the previous page ended: `<0 connected | 1 other>.<app id>`.
fn cursor_key(cursor: &str, offers: &[Offer]) -> Result<(bool, String, String)> {
    let (group, id) = cursor.split_once('.').context("Invalid catalog cursor")?;
    ensure!(["0", "1"].contains(&group), "Invalid catalog cursor");
    let offer = offers
        .iter()
        .find(|o| o.id == id)
        .context("The catalog changed; reload it")?;
    Ok((group == "1", offer.name.to_lowercase(), offer.id.clone()))
}
