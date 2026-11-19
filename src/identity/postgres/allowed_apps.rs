//! Schema 134: the workspace's allowed apps. A member connects an app for
//! their agent only after an admin allowed it; an app nobody decided about is
//! asked for (`company_apps::app_request`), and an admin's "no" stands. Admins
//! connect any app, which allows it. The decision lives here, in Cuckoo, so it
//! is the same whichever provider serves the app.
use super::connector_routing::Offer;
use super::*;
use serde::Deserialize;
use serde_json::json;

#[cfg(test)]
#[path = "allowed_apps_tests.rs"]
mod tests;

pub(crate) use migration::AllowedApps;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AllowedApps;
    impl MigrationName for AllowedApps {
        fn name(&self) -> &str {
            "m20261118_000134_allowed_apps"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AllowedApps {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("allowed_apps.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Allowed apps require a forward migration".into(),
            ))
        }
    }
}

/// An admin's yes or no to one app, from Settings › Connections.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppAllowance {
    pub app: String,
    pub allowed: bool,
    /// Shown to members beside a blocked app, and on declined requests.
    #[serde(default)]
    pub note: String,
    /// 0 for an app nobody decided about; otherwise the revision shown.
    pub revision: i64,
}

pub(super) fn note(text: &str) -> Result<&str> {
    let text = text.trim();
    ensure!(
        text.chars().count() <= 300,
        "Keep the note under 300 characters"
    );
    Ok(text)
}

/// What people call the app: the catalog's name, or its id for an operator's own.
pub(super) fn app_name(app: &str) -> String {
    crate::identity::connectors::curated::app(app)
        .map_or_else(|| app.to_owned(), |a| a.name.clone())
}

/// Whether this person may connect `app` now. Admins may always: their
/// connecting it is the decision.
pub(super) fn may_connect(approved: Option<bool>, admin: bool, app: &str) -> Result<()> {
    ensure!(
        admin || approved != Some(false),
        "A workspace admin switched this app off for the workspace"
    );
    ensure!(
        admin || approved == Some(true),
        "{} isn't allowed in this workspace yet. Ask an admin to allow it",
        app_name(app)
    );
    Ok(())
}

/// Records who decided about `app` and their note, after its row was saved.
pub(super) async fn decided(
    db: &impl ConnectionTrait,
    w: &str,
    app: &str,
    admin: &str,
    note: &str,
) -> Result<()> {
    execute(db, "UPDATE relay_control_systems SET note=$3,decided_by=$4,decided_at=$5 WHERE workspace=$1 AND server=$2",
        vec![w.into(), app.into(), note.into(), admin.into(), now().into()]).await?;
    Ok(())
}

/// Answers every waiting request for `app` at once: an app is allowed or
/// switched off for the whole workspace, so nobody keeps waiting.
pub(super) async fn settle_requests(
    db: &impl ConnectionTrait,
    w: &str,
    app: &str,
    approve: bool,
    admin: &str,
    note: &str,
) -> Result<u64> {
    execute(db, "UPDATE relay_app_requests SET state=$3,decided_by=$4,decided=$5,note=$6 WHERE workspace=$1 AND connector=$2 AND state='pending'",
        vec![w.into(), app.into(), if approve { "approved" } else { "declined" }.into(), admin.into(), now().into(), note.into()]).await
}

impl PgStore {
    /// Settings › Connections for admins: every app Cuckoo offers this
    /// workspace with the admins' decision about it.
    pub(crate) async fn allowed_apps(
        &self,
        user: &User,
        w: &str,
        offers: &[Offer],
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins choose the workspace's apps"
        );
        let listed: Value = offers.iter().map(|o| json!({"id":o.id})).collect();
        let facts = json_rows(&tx, "SELECT jsonb_build_object('id',o.id,
              'allowance',CASE s.approved WHEN true THEN 'allowed' WHEN false THEN 'blocked' ELSE 'undecided' END,
              'note',COALESCE(s.note,''),'revision',COALESCE(s.revision,0),
              'decided',CASE WHEN s.decided_at IS NULL THEN NULL ELSE jsonb_build_object('at',s.decided_at,'by_name',COALESCE(u.display_name,u.email)) END,
              'connections',(SELECT count(*) FROM relay_control_connections c WHERE c.workspace=$1 AND c.server=o.id AND c.enabled),
              'waiting',(SELECT count(*) FROM relay_app_requests r WHERE r.workspace=$1 AND r.connector=o.id AND r.state='pending')) AS data
            FROM jsonb_to_recordset($2::jsonb) AS o(id text)
            LEFT JOIN relay_control_systems s ON s.workspace=$1 AND s.server=o.id
            LEFT JOIN relay_users u ON u.id=s.decided_by",
            vec![w.into(), listed.into()]).await?;
        // MCP servers members connected, waiting for an admin's yes.
        let systems = json_rows(&tx, "SELECT jsonb_build_object('id',s.connection,'name',s.name,'url',c.binding->>'url','tools',jsonb_array_length(s.tools),
              'revision',s.revision,'created_ms',s.created_ms,'requester',COALESCE(u.display_name,u.email),'agent',s.agent,'agent_name',g.name) AS data
            FROM relay_connected_systems s JOIN relay_control_connections c ON c.workspace=s.workspace AND c.id=s.connection
            JOIN relay_shared_agents g ON g.workspace=s.workspace AND g.id=s.agent LEFT JOIN relay_users u ON u.id=s.created_by
            WHERE s.workspace=$1 AND s.waiting AND NOT s.removed ORDER BY s.created_ms,s.connection LIMIT 200",
            vec![w.into()]).await?;
        tx.commit().await?;
        let mut apps: Vec<Value> = offers
            .iter()
            .filter_map(|offer| {
                let mut app = facts.iter().find(|f| f["id"] == offer.id.as_str())?.clone();
                app["name"] = json!(offer.name);
                app["description"] = json!(offer.description);
                app["category"] = json!(offer.category);
                Some(app)
            })
            .collect();
        apps.sort_by_key(|a| a["name"].as_str().unwrap_or_default().to_lowercase());
        Ok(json!({"apps":apps,"systems":systems}))
    }
}
