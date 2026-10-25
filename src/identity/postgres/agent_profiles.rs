use super::*;
use crate::identity::agents::config::{SaveAgent, Settings};

pub(super) async fn managed_access(
    db: &impl ConnectionTrait,
    user: &str,
    w: &str,
    id: &str,
    manage: bool,
) -> Result<Value> {
    let member_role = role(db, w, user).await?;
    let row = query(
        db,
        "SELECT to_jsonb(a) AS data FROM relay_shared_agents a WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .context("Agent unavailable")?;
    let a: Value = row.try_get("", "data")?;
    let own = a["owner_id"] == user;
    if a["scope"] == "private" {
        ensure!(own, "Agent unavailable");
    } else if manage {
        ensure!(
            own || member_role == "Owner",
            "Only agent maintainers can change this agent"
        );
    } else {
        ensure!(own || member_role == "Owner" || query(db,"SELECT agent FROM relay_chat_access WHERE workspace=$1 AND user_id=$2 AND agent=$3",vec![w.into(),user.into(),id.into()]).await?.is_some(), "Agent unavailable");
    }
    Ok(a)
}
impl PgStore {
    pub async fn managed_agent(
        &self,
        user: &User,
        w: &str,
        id: &str,
        manage: bool,
    ) -> Result<Value> {
        managed_access(&self.db, &user.id, w, id, manage).await
    }
    pub async fn agent_profiles(&self, user: &User, w: &str) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        let owner = role(&tx, w, &user.id).await? == "Owner";
        let profiles = json_rows(&tx, "SELECT to_jsonb(a) || jsonb_build_object('creator_name',COALESCE(u.display_name,u.email,'Unknown'),'owner_name',COALESCE(o.display_name,o.email,'Former member'),'editable',(a.owner_id=$2 OR (a.scope='workspace' AND $3)),'pending',(SELECT count(*) FROM relay_chat_jobs j WHERE j.workspace=a.workspace AND j.payload->>'agent'=a.id AND j.state IN ('queued','running'))) AS data FROM relay_shared_agents a LEFT JOIN relay_users u ON u.id=a.creator_id LEFT JOIN relay_users o ON o.id=a.owner_id WHERE a.workspace=$1 AND (a.owner_id=$2 OR (a.scope='workspace' AND ($3 OR EXISTS(SELECT 1 FROM relay_chat_access g WHERE g.workspace=a.workspace AND g.user_id=$2 AND g.agent=a.id)))) ORDER BY a.created_at,a.id LIMIT 500",vec![w.into(),user.id.clone().into(),owner.into()]).await?;
        tx.commit().await?;
        Ok(profiles)
    }
    pub async fn save_agent_profile(
        &self,
        user: &User,
        w: &str,
        input: &SaveAgent,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let settings = serde_json::to_value(&input.settings)?;
        let old = query(
            &tx,
            "SELECT revision FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.id.clone().into()],
        )
        .await?;
        if old.is_some() {
            let a = managed_access(&tx, &user.id, w, &input.id, true).await?;
            ensure!(
                a["scope"] == input.scope,
                "Visibility cannot change; create a separate agent without private history"
            );
            // An exact retry after a lost response succeeds without changing revision again.
            if a["revision"].as_i64() != Some(input.revision) {
                ensure!(
                    a["revision"].as_i64() == Some(input.revision + 1)
                        && a["name"] == input.name.trim()
                        && a["role"] == input.role.trim()
                        && a["model"] == input.model
                        && a["settings"] == settings,
                    "Agent changed; reload before saving"
                );
                tx.commit().await?;
                return Ok(a);
            }
            let pending:i64=query(&tx,"SELECT count(*) AS count FROM relay_chat_jobs WHERE workspace=$1 AND payload->>'agent'=$2 AND state IN ('queued','running')",vec![w.into(),input.id.clone().into()]).await?.unwrap().try_get("","count")?;
            ensure!(
                pending == 0 || input.settings.paused,
                "Wait for active work to finish or pause the agent first"
            );
            execute(&tx,"UPDATE relay_shared_agents SET name=$3,role=$4,model=$5,settings=$6,revision=revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),input.id.clone().into(),input.name.trim().into(),input.role.trim().into(),input.model.clone().into(),settings.into()]).await?;
        } else {
            ensure!(input.revision == 0, "Agent missing");
            ensure!(
                input.scope == "private"
                    || super::connection_authority::can_create(&tx, w, &user.id).await?,
                "An Owner must allow you to create workspace agents"
            );
            execute(&tx,"INSERT INTO relay_shared_agents(workspace,id,name,role,model,scope,owner_id,creator_id,revision,settings) VALUES($1,$2,$3,$4,$5,$6,$7,$7,1,$8)",vec![w.into(),input.id.clone().into(),input.name.trim().into(),input.role.trim().into(),input.model.clone().into(),input.scope.clone().into(),user.id.clone().into(),settings.into()]).await?;
        }
        if uuid::Uuid::parse_str(&input.settings.avatar).is_ok() {
            ensure!(
                query(
                    &tx,
                    "SELECT id FROM relay_agent_avatars WHERE workspace=$1 AND agent=$2 AND id=$3",
                    vec![
                        w.into(),
                        input.id.clone().into(),
                        input.settings.avatar.clone().into()
                    ]
                )
                .await?
                .is_some(),
                "Avatar does not belong to this agent"
            );
        }
        audit(&tx, &user.id, Some(w), "agent_profile_saved", &input.id).await?;
        let result = managed_access(&tx, &user.id, w, &input.id, true).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn agent_avatar_save(
        &self,
        user: &User,
        w: &str,
        id: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<String> {
        let tx = self.workspace_tx(w).await?;
        managed_access(&tx, &user.id, w, id, true).await?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= 2 * 1024 * 1024,
            "Avatar must be between 1 byte and 2 MiB"
        );
        let avatar = uuid::Uuid::new_v4().to_string();
        execute(
            &tx,
            "INSERT INTO relay_agent_avatars(workspace,agent,id,mime,bytes) VALUES($1,$2,$3,$4,$5)",
            vec![
                w.into(),
                id.into(),
                avatar.clone().into(),
                mime.into(),
                bytes.into(),
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(avatar)
    }
    pub async fn agent_avatar(
        &self,
        user: &User,
        w: &str,
        id: &str,
        avatar: &str,
    ) -> Result<(String, Vec<u8>)> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, id, false).await?;
        let row = query(
            &tx,
            "SELECT mime,bytes FROM relay_agent_avatars WHERE workspace=$1 AND agent=$2 AND id=$3",
            vec![w.into(), id.into(), avatar.into()],
        )
        .await?
        .context("Avatar unavailable")?;
        let result = (row.try_get("", "mime")?, row.try_get("", "bytes")?);
        tx.commit().await?;
        Ok(result)
    }
    pub async fn agent_settings(&self, w: &str, id: &str) -> Result<Option<Settings>> {
        let row = query(
            &self.db,
            "SELECT settings FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND revision>0",
            vec![w.into(), id.into()],
        )
        .await?;
        row.map(|r| Ok(serde_json::from_value(r.try_get("", "settings")?)?))
            .transpose()
    }
}
