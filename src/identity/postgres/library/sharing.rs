use super::*;

/// Agents read until agent editing exists (LB3); customer answers need customer mode.
fn validate_agent_grant(grant: &AgentGrant) -> Result<()> {
    ensure!(
        grant.level == "read",
        "Agents can read Library items for now; editing levels come with agent editing"
    );
    ensure!(
        ["viewers", "staff"].contains(&grant.use_for.as_str()),
        "Choose whose questions this may answer: people who can see it, or all staff"
    );
    Ok(())
}
impl PgStore {
    pub async fn library_share(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &ShareItem,
    ) -> Result<Value> {
        ensure!(
            ["private", "selected", "workspace"].contains(&input.visibility.as_str())
                && input.people.len() <= 100
                && input.agents.len() <= 30,
            "Invalid sharing selection"
        );
        ensure!(
            input.visibility != "private" || input.people.is_empty(),
            "Private items cannot have people grants"
        );
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        ensure!(
            current["manageable"] == true,
            "Only the creator can manage sharing"
        );
        ensure!(
            current["folder"].is_null(),
            "This item follows its folder's sharing; share the folder or move the item out"
        );
        let payload = json!({"share":id,"input":input});
        if let Some(result) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(result);
        }
        if current["access_revision"].as_i64() != Some(input.revision) {
            return Err(Conflict.into());
        }
        let mut seen = std::collections::HashSet::new();
        for p in &input.people {
            ensure!(
                ["view", "edit"].contains(&p.access.as_str()) && seen.insert(p.user.as_str()),
                "Invalid or duplicate person grant"
            );
            role(&tx, w, &p.user).await?;
        }
        for grant in &input.agents {
            validate_agent_grant(grant)?;
            super::super::agent_profiles::managed_access(&tx, &user.id, w, &grant.agent, false)
                .await?;
        }
        execute(
            &tx,
            "DELETE FROM relay_library_people WHERE workspace=$1 AND item=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        execute(
            &tx,
            "DELETE FROM relay_library_agents WHERE workspace=$1 AND item=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        for p in &input.people {
            execute(
                &tx,
                "INSERT INTO relay_library_people VALUES($1,$2,$3,$4)",
                vec![
                    w.into(),
                    id.into(),
                    p.user.clone().into(),
                    p.access.clone().into(),
                ],
            )
            .await?;
        }
        let mut agents = std::collections::HashSet::new();
        for grant in &input.agents {
            ensure!(agents.insert(&grant.agent), "Each agent can be listed once");
            execute(
                &tx,
                "INSERT INTO relay_library_agents(workspace,item,agent,level,use_for) VALUES($1,$2,$3,$4,$5)",
                vec![
                    w.into(),
                    id.into(),
                    grant.agent.clone().into(),
                    grant.level.clone().into(),
                    grant.use_for.clone().into(),
                ],
            )
            .await?;
        }
        execute(&tx,"UPDATE relay_library_items SET visibility=$3,access_revision=access_revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),input.visibility.clone().into()]).await?;
        // Items in a folder follow it: their source receipts must see the change too.
        execute(&tx,"UPDATE relay_library_items SET access_revision=access_revision+1 WHERE workspace=$1 AND folder=$2",vec![w.into(),id.into()]).await?;
        let result = json!({"ok":true});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        let mut said = vec![];
        for grant in &input.agents {
            let name: String = query(
                &tx,
                "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![w.into(), grant.agent.clone().into()],
            )
            .await?
            .map(|r| r.try_get("", "name"))
            .transpose()?
            .unwrap_or_default();
            said.push(format!(
                "{name} reads it for {}",
                if grant.use_for == "staff" {
                    "all staff"
                } else {
                    "people who can see it"
                }
            ));
        }
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_access_changed",
            id,
            &format!(
                "Changed sharing of \"{}\"{}",
                current["title"].as_str().unwrap_or("Library item"),
                if said.is_empty() {
                    String::new()
                } else {
                    format!(": {}", said.join("; "))
                }
            ),
        )
        .await?;
        super::super::task_revocation::inaccessible(&tx, w).await?;
        tx.commit().await?;
        Ok(result)
    }
}
