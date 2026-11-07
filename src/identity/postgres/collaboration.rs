use super::*;
use crate::identity::collaboration::ChannelSettings;
impl PgStore {
    pub async fn check_chat(&self, user: &User, w: &str, chat: &str) -> Result<()> {
        super::chat::access(&self.db, user, w, chat).await
    }
    pub async fn allowed_shared_agents(&self, user: &User, w: &str) -> Result<Vec<Value>> {
        self.read_allowed_agents(user, w, None).await
    }
    pub(crate) async fn allowed_shared_agent(
        &self,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<Option<Value>> {
        Ok(self.read_allowed_agents(user, w, Some(agent)).await?.pop())
    }
    async fn read_allowed_agents(
        &self,
        user: &User,
        w: &str,
        agent: Option<&str>,
    ) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let mut values = vec![w.into(), user.id.clone().into()];
        let filter = if let Some(agent) = agent {
            values.push(agent.into());
            " AND a.id=$3"
        } else {
            ""
        };
        let profiles=json_rows(&tx,&format!("SELECT jsonb_build_object('id',a.id,'name',a.name,'role',a.role,'model',a.model,'shared',true,'audience',a.audience,'avatar',a.settings->>'avatar','manager',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=a.owner_id)) AS data FROM relay_shared_agents a WHERE a.workspace=$1 AND relay_agent_can_talk(a.workspace,a.id,$2){filter} ORDER BY a.name"),values).await?;
        tx.commit().await?;
        Ok(profiles)
    }
    pub async fn shared_agents(&self, workspace: &str) -> Result<Vec<Value>> {
        json_rows(&self.db,"SELECT jsonb_build_object('id',id,'name',name,'role',role,'model',model,'shared',true) AS data FROM relay_shared_agents WHERE workspace=$1 ORDER BY name",vec![workspace.into()]).await
    }
    pub async fn save_shared_agent(
        &self,
        user: &User,
        w: &str,
        id: &str,
        name: &str,
        description: &str,
        model: &str,
    ) -> Result<()> {
        ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid agent ID");
        ensure!(
            !name.trim().is_empty()
                && name.len() <= 80
                && description.len() <= 4000
                && !model.is_empty()
                && model.len() <= 200,
            "Invalid agent profile"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can create shared agents"
        );
        let changed = execute(&tx,"INSERT INTO relay_shared_agents(workspace,id,name,role,model,owner_id,creator_id) VALUES($1,$2,$3,$4,$5,$6,$6) ON CONFLICT(workspace,id) DO NOTHING",vec![w.into(),id.into(),name.into(),description.into(),model.into(),user.id.clone().into()]).await?;
        if changed == 0 {
            let r = query(
                &tx,
                "SELECT name,role,model FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![w.into(), id.into()],
            )
            .await?
            .context("Agent missing")?;
            ensure!(
                r.try_get::<String>("", "name")? == name
                    && r.try_get::<String>("", "role")? == description
                    && r.try_get::<String>("", "model")? == model,
                "Agent ID already used"
            );
        } else {
            audit(&tx, &user.id, Some(w), "shared_agent_created", id).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn teams(&self, user: &User, w: &str) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let result = json_rows(&tx,"SELECT jsonb_build_object('id',t.id,'name',t.name,'users',coalesce((SELECT jsonb_agg(p.user_id) FROM relay_team_people p WHERE p.workspace=t.workspace AND p.team=t.id),'[]'::jsonb)) AS data FROM relay_teams t WHERE workspace=$1 ORDER BY name",vec![w.into()]).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn save_team(
        &self,
        user: &User,
        w: &str,
        id: &str,
        name: &str,
        users: &[String],
    ) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(id).is_ok()
                && !name.trim().is_empty()
                && name.len() <= 80
                && users.len() <= 100,
            "Invalid team"
        );
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        for member in users {
            role(&tx, w, member).await?;
        }
        execute(&tx,"INSERT INTO relay_teams(workspace,id,name) VALUES($1,$2,$3) ON CONFLICT(workspace,id) DO UPDATE SET name=excluded.name",vec![w.into(),id.into(),name.into()]).await?;
        execute(
            &tx,
            "DELETE FROM relay_team_people WHERE workspace=$1 AND team=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        for member in users {
            execute(&tx,"INSERT INTO relay_team_people(workspace,team,user_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",vec![w.into(),id.into(),member.clone().into()]).await?;
        }
        audit(&tx, &user.id, Some(w), "team_updated", id).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn channel_members(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        users: &[String],
    ) -> Result<()> {
        ensure!(
            users.len() <= 100 && !users.is_empty(),
            "Choose between one and 100 members"
        );
        let tx = self.workspace_tx(w).await?;
        managed_channel(&tx, user, w, chat).await?;
        ensure!(
            users.contains(&user.id),
            "Keep yourself in the channel while managing it"
        );
        for member in users {
            role(&tx, w, member).await?;
        }
        // People who stay keep when they joined, so their unread is unchanged
        // (unread.rs); only people added now start from now.
        let joined = rows(
            &tx,
            "SELECT user_id,joined_ms FROM relay_chat_people WHERE workspace=$1 AND chat=$2",
            vec![w.into(), chat.into()],
        )
        .await?
        .iter()
        .map(|row| Ok((row.try_get("", "user_id")?, row.try_get("", "joined_ms")?)))
        .collect::<Result<std::collections::HashMap<String, i64>>>()?;
        execute(
            &tx,
            "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2",
            vec![w.into(), chat.into()],
        )
        .await?;
        for member in users {
            execute(&tx,"INSERT INTO relay_chat_people(workspace,chat,user_id,joined_ms) VALUES($1,$2,$3,COALESCE($4,(extract(epoch FROM now())*1000)::bigint)) ON CONFLICT DO NOTHING",vec![w.into(),chat.into(),member.clone().into(),joined.get(member).copied().into()]).await?;
        }
        audit(&tx, &user.id, Some(w), "channel_members_updated", chat).await?;
        super::task_revocation::inaccessible(&tx, w).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Sets which agents work in a channel. Removing an agent revokes work
    /// that the channel no longer permits.
    pub async fn channel_agents(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        agents: &[String],
    ) -> Result<()> {
        ensure!(agents.len() <= 20, "Add at most 20 agents to a channel");
        let tx = self.workspace_tx(w).await?;
        managed_channel(&tx, user, w, chat).await?;
        for agent in agents {
            ensure!(
                query(&tx,"SELECT 1 AS n FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND audience<>'only_me'",vec![w.into(),agent.clone().into()]).await?.is_some(),
                "Only-me agents cannot join channels"
            );
            ensure!(
                super::chat::agent_access(&tx, &user.id, w, agent).await?,
                "Agent is not shared with you"
            );
        }
        let current = rows(
            &tx,
            "SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2",
            vec![w.into(), chat.into()],
        )
        .await?
        .iter()
        .map(|r| r.try_get::<String>("", "agent"))
        .collect::<Result<Vec<_>, _>>()?;
        for gone in current.iter().filter(|a| !agents.contains(a)) {
            execute(
                &tx,
                "DELETE FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
                vec![w.into(), chat.into(), gone.clone().into()],
            )
            .await?;
        }
        for agent in agents.iter().filter(|a| !current.contains(a)) {
            execute(&tx,"INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES($1,$2,$3,(SELECT COALESCE(max(sequence),0) FROM relay_chat_messages WHERE workspace=$1 AND chat=$2))",vec![w.into(),chat.into(),agent.clone().into()]).await?;
        }
        audit(&tx, &user.id, Some(w), "channel_agents_updated", chat).await?;
        super::task_revocation::inaccessible(&tx, w).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Saves a channel's settings. Its managers (creator or workspace admin)
    /// change the name, description and who can post; only admins change who
    /// can find it, because that changes who can read its history and files.
    pub(crate) async fn channel_settings(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        input: &ChannelSettings,
    ) -> Result<()> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        let admin = managed_channel(&tx, user, w, chat).await?;
        let current: String = query(
            &tx,
            "SELECT visibility FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![w.into(), chat.into()],
        )
        .await?
        .context("Conversation missing")?
        .try_get("", "visibility")?;
        ensure!(
            admin || current == input.visibility,
            "Only workspace admins can change who can find a channel"
        );
        let (visibility, announcements) = (&input.visibility, input.announcements);
        let name = input.name.as_deref().map(str::trim);
        let description = input.description.as_deref().map(str::trim);
        execute(&tx,"UPDATE relay_chats SET visibility=$3,announcements=$4,name=coalesce($5,name),description=coalesce($6,description) WHERE workspace=$1 AND id=$2 AND kind<>'direct'",vec![w.into(),chat.into(),visibility.clone().into(),announcements.into(),name.into(),description.into()]).await?;
        super::task_revocation::inaccessible(&tx, w).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            "channel_settings_updated",
            &format!("{chat}/{visibility}/{announcements}/{name:?}"),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    /// The caller leaves a channel or group they are in. Every workspace admin
    /// can manage a public channel, so only a private one needs someone who can
    /// manage it to remain: its creator or an admin who is still a member. A
    /// public channel stays readable to everyone in the workspace, so leaving it
    /// takes the person off its people list. Leaving again changes nothing.
    pub async fn channel_leave(&self, user: &User, w: &str, chat: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        super::communications::access::ordinary_chat(&tx, w, chat).await?;
        let row = query(
            &tx,
            "SELECT kind,visibility,creator,name FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![w.into(), chat.into()],
        )
        .await?
        .context("Conversation missing")?;
        ensure!(
            row.try_get::<String>("", "kind")? != "direct",
            "Direct messages cannot be left"
        );
        let left = execute(
            &tx,
            "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
            vec![w.into(), chat.into(), user.id.clone().into()],
        )
        .await?;
        if left == 0 {
            tx.commit().await?;
            return Ok(());
        }
        let creator: String = row.try_get("", "creator")?;
        let manages = creator == user.id || role(&tx, w, &user.id).await? == "Admin";
        if manages && row.try_get::<String>("", "visibility")? != "public" {
            let remaining = query(
                &tx,
                "SELECT 1 AS n FROM relay_chat_people p JOIN relay_members m ON m.workspace=p.workspace AND m.user_id=p.user_id WHERE p.workspace=$1 AND p.chat=$2 AND (m.role='Admin' OR p.user_id=$3) LIMIT 1",
                vec![w.into(), chat.into(), creator.into()],
            )
            .await?;
            ensure!(
                remaining.is_some(),
                "Nobody else in #{} could manage it. Add a workspace admin to it before you leave.",
                row.try_get::<String>("", "name")?
            );
        }
        audit(&tx, &user.id, Some(w), "channel_left", chat).await?;
        // Saved and waiting work the person can no longer reach stops now.
        super::task_revocation::inaccessible(&tx, w).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn queue_metrics(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can inspect queue metrics"
        );
        let result=query(&tx,"SELECT jsonb_build_object('measured_jobs',count(*),'queue_p50_ms',percentile_cont(0.5) WITHIN GROUP(ORDER BY started_ms-created_ms),'queue_p95_ms',percentile_cont(0.95) WITHIN GROUP(ORDER BY started_ms-created_ms),'execution_p95_ms',percentile_cont(0.95) WITHIN GROUP(ORDER BY finished_ms-started_ms),'end_to_end_p95_ms',percentile_cont(0.95) WITHIN GROUP(ORDER BY finished_ms-created_ms),'retry_attempts',coalesce(sum(attempts-1),0)) AS data FROM relay_chat_jobs WHERE workspace=$1 AND created_ms IS NOT NULL AND started_ms IS NOT NULL AND finished_ms IS NOT NULL",vec![w.into()]).await?.context("Missing metrics")?.try_get("","data")?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn company_counts(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins can inspect company metrics"
        );
        let result=query(&tx,"SELECT jsonb_build_object('messages',(SELECT count(*) FROM relay_chat_messages WHERE workspace=$1),'jobs',(SELECT count(*) FROM relay_chat_jobs WHERE workspace=$1),'pending',(SELECT count(*) FROM relay_chat_jobs WHERE workspace=$1 AND state<>'finished'),'objects',(SELECT count(*) FROM relay_objects WHERE workspace=$1),'feedback',(SELECT count(*) FROM relay_feedback WHERE workspace=$1)) AS data",vec![w.into()]).await?.context("Missing metrics")?.try_get("","data")?;
        tx.commit().await?;
        Ok(result)
    }
}

/// A channel is managed by its creator or a workspace admin with access to it
/// (D231): they change its members, agents and settings. Returns whether the
/// caller is an admin, who alone may change who can find it.
async fn managed_channel(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    chat: &str,
) -> Result<bool> {
    super::chat::access(tx, user, w, chat).await?;
    super::communications::access::ordinary_chat(tx, w, chat).await?;
    let own_role = role(tx, w, &user.id).await?;
    let row = query(
        tx,
        "SELECT creator,kind FROM relay_chats WHERE workspace=$1 AND id=$2",
        vec![w.into(), chat.into()],
    )
    .await?
    .context("Conversation missing")?;
    ensure!(
        row.try_get::<String>("", "kind")? != "direct",
        "Direct messages have no channel members or settings"
    );
    ensure!(
        own_role == "Admin" || row.try_get::<String>("", "creator")? == user.id,
        "Only the channel creator or a workspace admin can change this channel"
    );
    Ok(own_role == "Admin")
}
