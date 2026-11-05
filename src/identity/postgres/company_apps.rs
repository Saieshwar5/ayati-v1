//! Company systems on the Apps page (AP2/AP3): an app that opens the company's own
//! system through Cuckoo, which checks who may open it first. Members see every app,
//! open those they may, and request access or suggest an app; admins add and manage
//! apps; an app's owner and admins decide its access requests.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;

/// Schema 75: company apps, their people, and requests.
pub(crate) struct CompanyApps;
impl MigrationName for CompanyApps {
    fn name(&self) -> &str {
        "m20261105_000075_company_apps"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for CompanyApps {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("company_apps.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Company apps require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompanyApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    pub audience: String,
    #[serde(default)]
    pub people: Vec<String>,
    #[serde(default)]
    pub owner: Option<String>,
    /// 0 to add; otherwise the revision this change was made from.
    pub revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppRequest {
    /// The request's own id, so a retry never asks twice.
    pub id: String,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppDecision {
    pub action: String,
}

/// May this person open the app: everyone, the people chosen, its owner, or an admin.
const OPENS: &str = "(a.audience='everyone' OR a.owner=$2 OR $3 OR EXISTS(SELECT 1 FROM relay_company_app_people p WHERE p.workspace=a.workspace AND p.app=a.id AND p.user_id=$2))";

fn https(url: &str) -> Result<()> {
    let parsed =
        reqwest::Url::parse(url).context("Enter the app's full address, starting with https://")?;
    ensure!(
        parsed.scheme() == "https" && parsed.host_str().is_some() && url.len() <= 2000,
        "Apps open over https:// only"
    );
    Ok(())
}

impl PgStore {
    /// Company apps as the viewer sees them; the address only for those who may open.
    pub(super) async fn company_apps_view(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        admin: bool,
    ) -> Result<Vec<Value>> {
        let present: bool = query(
            tx,
            "SELECT to_regclass('relay_company_apps') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        if !present {
            return Ok(vec![]);
        }
        json_rows(tx, &format!("SELECT jsonb_build_object('id',a.id,'kind','link','name',a.name,'description',a.description,'audience',a.audience,'revision',a.revision,
            'can_open',{OPENS},'url',CASE WHEN {OPENS} THEN a.url END,
            'owner',a.owner,'owner_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=a.owner),
            'can_decide',($3 OR a.owner=$2),
            'people',CASE WHEN $3 OR a.owner=$2 THEN (SELECT COALESCE(jsonb_agg(jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email)) ORDER BY u.id),'[]'::jsonb) FROM relay_company_app_people p JOIN relay_users u ON u.id=p.user_id WHERE p.workspace=a.workspace AND p.app=a.id) END,
            'requested',EXISTS(SELECT 1 FROM relay_app_requests r WHERE r.workspace=a.workspace AND r.app=a.id AND r.requester=$2 AND r.state='pending')) AS data
            FROM relay_company_apps a WHERE a.workspace=$1 AND NOT a.removed ORDER BY a.name,a.id"), vec![w.into(), user.id.clone().into(), admin.into()]).await
    }

    pub(crate) async fn company_app_save(
        &self,
        user: &User,
        w: &str,
        input: &CompanyApp,
    ) -> Result<Value> {
        crate::identity::library::model::uuid(&input.id)?;
        let name = input.name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 80,
            "Name the app in up to 80 characters"
        );
        ensure!(
            input.description.chars().count() <= 200,
            "Describe it in up to 200 characters"
        );
        https(&input.url)?;
        ensure!(
            ["everyone", "selected"].contains(&input.audience.as_str()),
            "Choose who can open it"
        );
        ensure!(input.people.len() <= 500, "Choose up to 500 people");
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins add and change company apps"
        );
        for person in input.people.iter().chain(input.owner.iter()) {
            role(&tx, w, person)
                .await
                .context("Choose people in this workspace")?;
        }
        let existing = query(
            &tx,
            "SELECT revision FROM relay_company_apps WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.id.clone().into()],
        )
        .await?;
        match (existing, input.revision) {
            (None, 0) => {
                let count: i64 = query(&tx, "SELECT count(*) AS n FROM relay_company_apps WHERE workspace=$1 AND NOT removed", vec![w.into()]).await?.context("Count")?.try_get("", "n")?;
                ensure!(count < 200, "A workspace can list up to 200 company apps");
                execute(&tx, "INSERT INTO relay_company_apps(workspace,id,name,description,url,audience,owner,created,updated) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$8)", vec![w.into(), input.id.clone().into(), name.into(), input.description.trim().into(), input.url.clone().into(), input.audience.clone().into(), input.owner.clone().into(), now().into()]).await?;
            }
            (Some(row), revision) if row.try_get::<i64>("", "revision")? == revision => {
                execute(&tx, "UPDATE relay_company_apps SET name=$3,description=$4,url=$5,audience=$6,owner=$7,revision=revision+1,updated=$8 WHERE workspace=$1 AND id=$2", vec![w.into(), input.id.clone().into(), name.into(), input.description.trim().into(), input.url.clone().into(), input.audience.clone().into(), input.owner.clone().into(), now().into()]).await?;
            }
            _ => return Err(crate::identity::library::model::Conflict.into()),
        }
        execute(
            &tx,
            "DELETE FROM relay_company_app_people WHERE workspace=$1 AND app=$2",
            vec![w.into(), input.id.clone().into()],
        )
        .await?;
        if input.audience == "selected" {
            for person in &input.people {
                execute(&tx, "INSERT INTO relay_company_app_people(workspace,app,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", vec![w.into(), input.id.clone().into(), person.clone().into()]).await?;
            }
        }
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "company_app_saved",
            &input.id,
            &format!(
                "{} the company app \"{name}\" ({})",
                if input.revision == 0 {
                    "Added"
                } else {
                    "Changed"
                },
                if input.audience == "everyone" {
                    "everyone can open it".to_owned()
                } else {
                    format!("{} people can open it", input.people.len())
                }
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":input.id}))
    }

    pub(crate) async fn company_app_remove(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins remove company apps"
        );
        let name: String = query(&tx, "UPDATE relay_company_apps SET removed=true,revision=revision+1,updated=$3 WHERE workspace=$1 AND id=$2 AND NOT removed RETURNING name", vec![w.into(), id.into(), now().into()]).await?.context("App unavailable")?.try_get("", "name")?;
        execute(&tx, "UPDATE relay_app_requests SET state='declined',decided_by=$3,decided=$4 WHERE workspace=$1 AND app=$2 AND state='pending'", vec![w.into(), id.into(), user.id.clone().into(), now().into()]).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "company_app_removed",
            id,
            &format!("Removed the company app \"{name}\""),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"removed":true}))
    }

    /// The address to open, for someone who may; every opening is audited.
    pub(crate) async fn company_app_open(&self, user: &User, w: &str, id: &str) -> Result<String> {
        let tx = self.workspace_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let row = query(&tx, &format!("SELECT a.url,a.name FROM relay_company_apps a WHERE a.workspace=$1 AND a.id=$4 AND NOT a.removed AND {OPENS}"), vec![w.into(), user.id.clone().into(), admin.into(), id.into()]).await?.ok_or(crate::identity::library::model::Denied)?;
        let (url, name): (String, String) = (row.try_get("", "url")?, row.try_get("", "name")?);
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "company_app_opened",
            id,
            &format!("Opened \"{name}\""),
        )
        .await?;
        tx.commit().await?;
        Ok(url)
    }

    /// A member asks for access to an app, or suggests one the company should add.
    pub(crate) async fn app_request(
        &self,
        user: &User,
        w: &str,
        input: &AppRequest,
    ) -> Result<Value> {
        crate::identity::library::model::uuid(&input.id)?;
        ensure!(
            input.reason.chars().count() <= 500,
            "Keep the reason under 500 characters"
        );
        let tx = self.workspace_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        if let Some(row) = query(
            &tx,
            "SELECT state FROM relay_app_requests WHERE workspace=$1 AND id=$2 AND requester=$3",
            vec![w.into(), input.id.clone().into(), user.id.clone().into()],
        )
        .await?
        {
            tx.commit().await?;
            return Ok(json!({"id":input.id,"state":row.try_get::<String>("", "state")?}));
        }
        let summary = match &input.app {
            Some(app) => {
                let row = query(&tx, &format!("SELECT a.name,{OPENS} AS opens FROM relay_company_apps a WHERE a.workspace=$1 AND a.id=$4 AND NOT a.removed"), vec![w.into(), user.id.clone().into(), admin.into(), app.clone().into()]).await?.context("App unavailable")?;
                ensure!(
                    !row.try_get::<bool>("", "opens")?,
                    "You can already open this app"
                );
                ensure!(query(&tx, "SELECT 1 AS waiting FROM relay_app_requests WHERE workspace=$1 AND app=$2 AND requester=$3 AND state='pending'", vec![w.into(), app.clone().into(), user.id.clone().into()]).await?.is_none(), "You already asked; the app's owner or an admin decides");
                execute(&tx, "INSERT INTO relay_app_requests(workspace,id,kind,app,reason,requester,created) VALUES($1,$2,'access',$3,$4,$5,$6)", vec![w.into(), input.id.clone().into(), app.clone().into(), input.reason.trim().into(), user.id.clone().into(), now().into()]).await?;
                format!("Asked to open \"{}\"", row.try_get::<String>("", "name")?)
            }
            None => {
                let name = input.name.trim();
                ensure!(
                    !name.is_empty() && name.chars().count() <= 80,
                    "Name the app you suggest"
                );
                if !input.url.trim().is_empty() {
                    https(input.url.trim())?;
                }
                execute(&tx, "INSERT INTO relay_app_requests(workspace,id,kind,name,url,reason,requester,created) VALUES($1,$2,'suggestion',$3,$4,$5,$6,$7)", vec![w.into(), input.id.clone().into(), name.into(), input.url.trim().into(), input.reason.trim().into(), user.id.clone().into(), now().into()]).await?;
                format!("Suggested the app \"{name}\"")
            }
        };
        audit_with_summary(&tx, &user.id, Some(w), "app_requested", &input.id, &summary).await?;
        tx.commit().await?;
        Ok(json!({"id":input.id,"state":"pending"}))
    }

    /// Waiting requests this person decides: all for admins, their apps' for owners.
    pub(crate) async fn app_requests(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let requests = json_rows(&tx, "SELECT jsonb_build_object('id',r.id,'kind',r.kind,'app',r.app,'app_name',a.name,'name',r.name,'url',r.url,'reason',r.reason,'created',r.created,'requester',COALESCE(u.display_name,u.email)) AS data FROM relay_app_requests r LEFT JOIN relay_company_apps a ON a.workspace=r.workspace AND a.id=r.app JOIN relay_users u ON u.id=r.requester WHERE r.workspace=$1 AND r.state='pending' AND ($3 OR (r.kind='access' AND a.owner=$2)) ORDER BY r.created,r.id LIMIT 200", vec![w.into(), user.id.clone().into(), admin.into()]).await?;
        tx.commit().await?;
        Ok(json!({"requests":requests}))
    }

    /// Approving an access request lets that person open the app; approving a
    /// suggestion records that an admin will add it. Deciding twice keeps the first.
    pub(crate) async fn app_request_decide(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AppDecision,
    ) -> Result<Value> {
        ensure!(
            ["approve", "decline"].contains(&input.action.as_str()),
            "Approve or decline the request"
        );
        let tx = self.workspace_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let request = json_rows(&tx, "SELECT jsonb_build_object('kind',r.kind,'app',r.app,'state',r.state,'requester',r.requester,'owner',a.owner,'app_name',a.name,'name',r.name,'who',COALESCE(u.display_name,u.email)) AS data FROM relay_app_requests r LEFT JOIN relay_company_apps a ON a.workspace=r.workspace AND a.id=r.app JOIN relay_users u ON u.id=r.requester WHERE r.workspace=$1 AND r.id=$2", vec![w.into(), id.into()]).await?.into_iter().next().context("Request unavailable")?;
        ensure!(
            admin || (request["kind"] == "access" && request["owner"] == user.id.as_str()),
            "The app's owner or an admin decides this request"
        );
        if request["state"] != "pending" {
            tx.commit().await?;
            return Ok(json!({"state":request["state"]}));
        }
        let approve = input.action == "approve";
        if approve && request["kind"] == "access" {
            let app = request["app"].as_str().unwrap_or("");
            execute(&tx, "UPDATE relay_company_apps SET audience=CASE WHEN audience='everyone' THEN audience ELSE 'selected' END,revision=revision+1,updated=$3 WHERE workspace=$1 AND id=$2", vec![w.into(), app.into(), now().into()]).await?;
            execute(&tx, "INSERT INTO relay_company_app_people(workspace,app,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", vec![w.into(), app.into(), request["requester"].as_str().unwrap_or("").into()]).await?;
        }
        let state = if approve { "approved" } else { "declined" };
        execute(&tx, "UPDATE relay_app_requests SET state=$3,decided_by=$4,decided=$5 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), state.into(), user.id.clone().into(), now().into()]).await?;
        let who = request["who"].as_str().unwrap_or("someone");
        let summary = if request["kind"] == "access" {
            format!(
                "{} {who}'s request to open \"{}\"",
                if approve { "Approved" } else { "Declined" },
                request["app_name"].as_str().unwrap_or("")
            )
        } else {
            format!(
                "{} {who}'s suggestion \"{}\"",
                if approve { "Accepted" } else { "Declined" },
                request["name"].as_str().unwrap_or("")
            )
        };
        audit_with_summary(&tx, &user.id, Some(w), "app_request_decided", id, &summary).await?;
        tx.commit().await?;
        Ok(json!({"state":state}))
    }
}

#[cfg(test)]
#[path = "company_apps_tests.rs"]
mod tests;
