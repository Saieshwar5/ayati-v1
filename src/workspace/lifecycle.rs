use super::*;

impl Workspace {
    /// Archive stops the runtime; deletion is a retained tombstone, never a disk wipe.
    pub async fn lifecycle(&self, id: &str, action: &str) -> Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        ensure!(
            id != "local-agent",
            "The built-in Relay guide cannot be removed"
        );
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "Workspace is stopping"
        );
        let mut profile = self
            .registry
            .list()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Unknown agent")?;
        match action {
            "archive" => {
                ensure!(
                    ["ready", "failed", "archiving", "archived"].contains(&profile.state.as_str()),
                    "Wait for agent preparation to finish"
                );
                profile.state = "archiving".into();
                profile.detail = "Stopping work before archival".into();
                self.registry.save(&profile)?;
                let agent = self.agents.lock().unwrap().get(id).cloned();
                if let Some(agent) = agent {
                    agent.begin_shutdown();
                    agent.stop_all()?;
                    agent.wait_until_idle().await;
                    agent.shutdown_runtime().await?;
                    self.agents.lock().unwrap().remove(id);
                }
                profile.state = "archived".into();
                profile.detail =
                    "Archived. Conversations, files and business resources are retained.".into();
            }
            "restore" => {
                ensure!(
                    profile.state == "archived",
                    "Only archived agents can be restored"
                );
                self.activate(&profile)?;
                profile.state = "ready".into();
                profile.detail.clear();
            }
            "delete" => {
                ensure!(
                    profile.state == "archived",
                    "Archive this agent before deleting it"
                );
                profile.state = "deleted".into();
                profile.detail =
                    "Removed from the directory. Stored history and files are retained.".into();
            }
            _ => anyhow::bail!("Unknown agent lifecycle action"),
        }
        self.registry.save(&profile)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn removal_requires_archival_and_retains_business_data() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&temp.path().join("ayati.sqlite")).unwrap());
        let files = Arc::new(Files::new(temp.path(), store.clone()).unwrap());
        let mut config = Config::from_env().unwrap();
        config.data_dir = temp.path().into();
        let mut template = GooseConfig::from_env(temp.path(), "fixture");
        template.binary = temp.path().join("not-installed");
        template.vm_manifest = None;
        template.browser = false;
        template.memory = false;
        let agent = Agent::start_goose(store, files, template.clone(), None);
        let workspace =
            Workspace::open_with_template(temp.path(), config, agent, template).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        workspace
            .registry
            .save(&AgentProfile {
                id: id.clone(),
                name: "Test".into(),
                role: "Test".into(),
                model: "fixture".into(),
                memory: false,
                browser: false,
                state: "failed".into(),
                detail: String::new(),
                attempt: "123456abcdef".into(),
            })
            .unwrap();
        std::fs::create_dir_all(temp.path().join("agents").join(&id)).unwrap();
        let saved = temp.path().join("agents").join(&id).join("saved.txt");
        std::fs::write(&saved, "retain history").unwrap();
        assert!(workspace.lifecycle("local-agent", "archive").await.is_err());
        assert!(workspace.lifecycle(&id, "delete").await.is_err());
        workspace.lifecycle(&id, "archive").await.unwrap();
        assert!(workspace.agent(&id).is_err());
        workspace.lifecycle(&id, "delete").await.unwrap();
        assert!(!workspace.profiles().unwrap().iter().any(|p| p.id == id));
        assert_eq!(std::fs::read_to_string(saved).unwrap(), "retain history");
        workspace.shutdown().await.unwrap();
    }
}
