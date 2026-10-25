use super::*;
use crate::identity::control::{
    model::Operation,
    setup_model::{Account, Context, Permission},
};
use anyhow::Context as _;

pub(super) async fn context(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
) -> Result<Context> {
    let profile = super::agent_profiles::managed_access(db, &user.id, w, agent, true).await?;
    let owner = role(db, w, &user.id).await? == "Owner";
    let rows = rows(db,"SELECT c.id,c.server,c.definition,c.revision,c.fingerprint FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.enabled AND s.approved AND ($2 OR EXISTS(SELECT 1 FROM relay_control_delegations d JOIN relay_members m ON m.workspace=d.workspace AND m.user_id=d.approver AND m.role='Owner' WHERE d.workspace=c.workspace AND d.connection=c.id AND d.user_id=$3 AND d.ceiling<>'block')) ORDER BY c.id",vec![w.into(),owner.into(),user.id.clone().into()]).await?;
    let mut accounts = Vec::new();
    for row in rows {
        let id: String = row.try_get("", "id")?;
        let definition: Value = row.try_get("", "definition")?;
        let operations: Vec<Operation> = serde_json::from_value(definition["operations"].clone())?;
        let mut permissions = Vec::new();
        for operation in operations {
            let grant = query(db,"SELECT d.ceiling,d.approver,d.revision,COALESCE(u.display_name,u.email) AS name FROM relay_control_delegations d JOIN relay_members m ON m.workspace=d.workspace AND m.user_id=d.approver AND m.role='Owner' JOIN relay_users u ON u.id=d.approver WHERE d.workspace=$1 AND d.connection=$2 AND d.user_id=$3 AND d.operation=$4",vec![w.into(),id.clone().into(),user.id.clone().into(),operation.id.clone().into()]).await?;
            let revision = query(db,"SELECT revision FROM relay_control_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4",vec![w.into(),agent.into(),id.clone().into(),operation.id.clone().into()]).await?.map(|r| r.try_get::<i64>("","revision")).transpose()?.unwrap_or(0);
            let (ceiling, approver, name, delegation_revision) = if owner {
                (
                    "allow".to_owned(),
                    Some(user.id.clone()),
                    Some(
                        user.display_name
                            .clone()
                            .unwrap_or_else(|| user.email.clone()),
                    ),
                    None,
                )
            } else if let Some(g) = grant {
                (
                    g.try_get("", "ceiling")?,
                    Some(g.try_get("", "approver")?),
                    Some(g.try_get("", "name")?),
                    Some(g.try_get("", "revision")?),
                )
            } else {
                ("block".into(), None, None, None)
            };
            let approvers = super::approval_authority::eligible(
                db,
                w,
                agent,
                &id,
                &operation.id,
                if owner { None } else { approver.as_deref() },
            )
            .await?;
            permissions.push(Permission {
                approvers,
                operation,
                ceiling,
                approver,
                approver_name: name,
                delegation_revision,
                rule_revision: revision,
            });
        }
        accounts.push(Account {
            id,
            server: row.try_get("", "server")?,
            name: definition["name"].as_str().unwrap_or_default().into(),
            account: definition["account"].as_str().map(str::to_owned),
            revision: row.try_get("", "revision")?,
            fingerprint: row.try_get("", "fingerprint")?,
            permissions,
        });
    }
    let conversations = json_rows(db,"SELECT jsonb_build_object('id',c.id,'name',c.name) AS data FROM relay_chats c JOIN relay_chat_agents a ON a.workspace=c.workspace AND a.chat=c.id WHERE c.workspace=$1 AND a.agent=$2 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)) ORDER BY c.id LIMIT 100",vec![w.into(),agent.into(),user.id.clone().into()]).await?;
    Ok(Context {
        approval_channels: super::approval_authority::channels(db, user, w).await?,
        agent_revision: profile["revision"]
            .as_i64()
            .context("Missing agent revision")?,
        name: profile["name"].as_str().unwrap_or_default().into(),
        responsibility: profile["role"].as_str().unwrap_or_default().into(),
        model: profile["model"].as_str().unwrap_or_default().into(),
        provider: profile["settings"]["provider"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(str::to_owned),
        accounts,
        conversations,
    })
}

pub(crate) fn select(mut context: Context, ids: &[String], chat: &str) -> Result<Context> {
    ensure!(
        context.conversations.iter().any(|c| c["id"] == chat),
        "Choose an accessible conversation containing this agent"
    );
    context.accounts.retain(|a| ids.contains(&a.id));
    ensure!(
        context.accounts.len() == ids.len(),
        "An approved account is unavailable or has not been delegated to you"
    );
    context.conversations.clear(); // Unrelated conversations do not invalidate this review.
    Ok(context)
}

impl PgStore {
    pub(crate) async fn setup_selected_context(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &crate::identity::control::setup_model::Start,
    ) -> Result<Context> {
        select(
            self.setup_context(user, w, agent).await?,
            &input.connections,
            &input.chat,
        )
    }
    pub(crate) async fn setup_context(&self, user: &User, w: &str, agent: &str) -> Result<Context> {
        let tx = self.read_tx(w).await?;
        let result = context(&tx, user, w, agent).await?;
        tx.commit().await?;
        Ok(result)
    }
}
