use super::*;
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
        for agent in &input.agents {
            super::super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
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
        for agent in &input.agents {
            if agents.insert(agent) {
                execute(
                    &tx,
                    "INSERT INTO relay_library_agents VALUES($1,$2,$3)",
                    vec![w.into(), id.into(), agent.clone().into()],
                )
                .await?;
            }
        }
        execute(&tx,"UPDATE relay_library_items SET visibility=$3,access_revision=access_revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),input.visibility.clone().into()]).await?;
        let result = json!({"ok":true});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        audit(&tx, &user.id, Some(w), "library_access_changed", id).await?;
        super::super::task_revocation::inaccessible(&tx, w).await?;
        tx.commit().await?;
        Ok(result)
    }
}
