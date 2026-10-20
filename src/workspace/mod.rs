//! Local-owner registry. Each agent has a separate database, runtime and guest.
mod lifecycle;
mod provision;
pub mod registry;
mod setup;

use crate::{
    agent::Agent, config::Config, files::Files, runtime::config::GooseConfig, storage::Store,
};
use anyhow::{Context, Result, ensure};
use registry::{AgentProfile, NewAgent, Registry};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio_util::sync::CancellationToken;

pub struct Workspace {
    setup_test: tokio::sync::Mutex<()>,
    registry: Registry,
    agents: Mutex<HashMap<String, Arc<Agent>>>,
    default: AgentProfile,
    data: PathBuf,
    config: Config,
    template: GooseConfig,
    operation: Mutex<()>,
    closing: AtomicBool,
    cancelled: CancellationToken,
    preparation: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    lifecycle: tokio::sync::Mutex<()>,
}

impl Workspace {
    pub fn open(data: &Path, config: Config, agent: Arc<Agent>) -> Result<Arc<Self>> {
        let mut template = GooseConfig::from_env(data, &config.model);
        template.platform = agent.platform();
        Self::open_with_template(data, config, agent, template)
    }

    pub fn open_with_template(
        data: &Path,
        config: Config,
        agent: Arc<Agent>,
        template: GooseConfig,
    ) -> Result<Arc<Self>> {
        let registry = Registry::open(&data.join("agents.sqlite"))?;
        let workspace = Arc::new(Self {
            setup_test: tokio::sync::Mutex::new(()),
            default: AgentProfile {
                id: "local-agent".into(),
                name: "Relay".into(),
                role: crate::runtime::guidance::GUIDE_ROLE.into(),
                model: config.model.clone(),
                memory: template.memory,
                browser: template.browser,
                state: "ready".into(),
                detail: String::new(),
                attempt: String::new(),
            },
            agents: Mutex::new(HashMap::from([("local-agent".into(), agent)])),
            registry,
            data: data.into(),
            template,
            config,
            operation: Mutex::new(()),
            closing: AtomicBool::new(false),
            cancelled: CancellationToken::new(),
            preparation: tokio::sync::Mutex::new(None),
            lifecycle: tokio::sync::Mutex::new(()),
        });
        for profile in workspace.registry.list()? {
            if profile.state == "ready" {
                workspace
                    .activate(&profile)
                    .context("Could not restore saved agent")?;
            }
        }
        Ok(workspace)
    }

    pub fn agent(&self, id: &str) -> Result<(Arc<Agent>, String, bool)> {
        let agent = self
            .agents
            .lock()
            .map_err(|_| anyhow::anyhow!("Workspace unavailable"))?
            .get(id)
            .cloned()
            .context("Agent is unknown or not ready")?;
        let profile = self
            .profiles()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Unknown agent")?;
        ensure!(profile.state == "ready", "Agent is not ready");
        let mut runtime = self.template.clone();
        if id != "local-agent" {
            runtime.vm_manifest = Some(self.vm_path(&profile));
        }
        let ready = if id == "local-agent" && self.config.prototype {
            self.config.api_key.is_some()
        } else {
            runtime.configured()
        };
        Ok((agent, profile.model, ready))
    }

    pub fn profiles(&self) -> Result<Vec<AgentProfile>> {
        let mut profiles = vec![self.default.clone()];
        profiles.extend(
            self.registry
                .list()?
                .into_iter()
                .filter(|p| p.state != "deleted"),
        );
        let agents = self
            .agents
            .lock()
            .map_err(|_| anyhow::anyhow!("Workspace unavailable"))?;
        for profile in &mut profiles {
            if let Some(agent) = agents.get(&profile.id)
                && let Some(edit) = agent.store.profile_edit()?
            {
                profile.name = edit.name;
                profile.role = edit.role;
                profile.memory = edit.memory.unwrap_or(profile.memory);
                profile.browser = edit.browser.unwrap_or(profile.browser);
                if let Some(model) = edit.model {
                    profile.model = model;
                }
            }
        }
        Ok(profiles)
    }
    pub fn snapshot(&self) -> Result<Value> {
        let profiles = self.profiles()?;
        let agents = self
            .agents
            .lock()
            .map_err(|_| anyhow::anyhow!("Workspace unavailable"))?;
        let mut result = Vec::new();
        for profile in profiles {
            let attention = match agents.get(&profile.id) {
                Some(agent) => json!({"pending_approvals":agent.store.permissions()?.len(),
                    "pending_tasks":agent.store.pending_count()?,"latest":agent.store.activity_preview()?}),
                None => json!({"pending_approvals":0,"pending_tasks":0}),
            };
            let mut profile_json = json!(profile);
            profile_json["revision"] = json!(0);
            profile_json["is_default"] = json!(profile.id == "local-agent");
            profile_json["can_delete"] = json!(false);
            if let Some(agent) = agents.get(&profile.id)
                && let Some(edit) = agent.store.profile_edit()?
            {
                profile_json["name"] = json!(edit.name);
                profile_json["role"] = json!(edit.role);
                profile_json["revision"] = json!(edit.revision);
                if let Some(memory) = edit.memory {
                    profile_json["memory"] = json!(memory);
                }
                if let Some(browser) = edit.browser {
                    profile_json["browser"] = json!(browser);
                }
                if let Some(model) = edit.model {
                    profile_json["model"] = json!(model);
                }
            }
            profile_json["editable"] = json!(
                agents
                    .get(&profile.id)
                    .is_some_and(|a| a.runtime_name() == "goose")
            );
            profile_json["browser_available"] =
                json!(agents.get(&profile.id).is_some_and(|a| a.browser_capable()));
            result.push(json!({"profile":profile_json,"activity":attention}));
        }
        Ok(
            json!({"agents":result,"creation_available":!self.config.prototype && self.template.vm_manifest.is_some(),
            "memory_available":self.template.memory,"browser_available":self.template.browser,
            "default_model":self.config.model,"limit":4}),
        )
    }

    pub async fn create(self: &Arc<Self>, input: NewAgent) -> Result<AgentProfile> {
        input.validate()?;
        let mut preparation = self.preparation.lock().await;
        let _operation = self
            .operation
            .lock()
            .map_err(|_| anyhow::anyhow!("Workspace unavailable"))?;
        if let Some(existing) = self
            .registry
            .list()?
            .into_iter()
            .find(|p| p.id == input.request_id)
        {
            ensure!(
                existing.name == input.name.trim()
                    && existing.role == input.role.trim()
                    && existing.model == input.model
                    && existing.memory == input.memory
                    && existing.browser == input.browser,
                "Request ID was already used for different settings"
            );
            return Ok(existing);
        }
        self.ensure_creation()?;
        ensure!(
            self.registry.list()?.len() < 3,
            "This local workspace supports four agents including Relay"
        );
        ensure!(
            !input.browser || self.template.browser,
            "Enable browser support on the daemon first"
        );
        ensure!(
            !input.memory || self.template.memory,
            "Enable memory support on the daemon first"
        );
        let profile = AgentProfile {
            id: input.request_id,
            name: input.name.trim().into(),
            role: input.role.trim().into(),
            model: input.model,
            memory: input.memory,
            browser: input.browser,
            state: "preparing".into(),
            detail: "Preparing a private environment. This can take several minutes.".into(),
            attempt: uuid::Uuid::new_v4().simple().to_string()[..12].into(),
        };
        self.registry.save(&profile)?;
        *preparation = Some(self.spawn_preparation(profile.clone()));
        Ok(profile)
    }
    pub async fn retry(self: &Arc<Self>, id: &str) -> Result<AgentProfile> {
        let mut preparation = self.preparation.lock().await;
        let _operation = self
            .operation
            .lock()
            .map_err(|_| anyhow::anyhow!("Workspace unavailable"))?;
        self.ensure_creation()?;
        let mut profile = self
            .registry
            .list()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Unknown agent")?;
        ensure!(
            profile.state == "failed",
            "Only failed setup can be retried"
        );
        profile.state = "preparing".into();
        profile.detail = "Preparing a fresh environment; the failed attempt is preserved.".into();
        profile.attempt = uuid::Uuid::new_v4().simple().to_string()[..12].into();
        self.registry.save(&profile)?;
        *preparation = Some(self.spawn_preparation(profile.clone()));
        Ok(profile)
    }
    fn ensure_creation(&self) -> Result<()> {
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "Workspace is stopping"
        );
        ensure!(
            !self.config.prototype && self.template.vm_manifest.is_some(),
            "Agent setup requires Goose VM execution; complete the local VM prerequisites first"
        );
        ensure!(
            !self.registry.list()?.iter().any(|p| p.state == "preparing"),
            "Another agent is being prepared; wait for it to finish"
        );
        Ok(())
    }
    fn vm_path(&self, profile: &AgentProfile) -> PathBuf {
        self.data.join("vms").join(&profile.attempt).join("vm.json")
    }
    fn activate(&self, profile: &AgentProfile) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&profile.id).is_ok_and(|id| id.to_string() == profile.id)
                && profile.attempt.len() == 12
                && profile.attempt.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid saved agent identity"
        );
        let data = self.data.join("agents").join(&profile.id);
        std::fs::create_dir_all(&data)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
        let mut config = self.template.clone();
        config.agent_id = profile.id.clone();
        config.role = profile.role.clone();
        config.model = profile.model.clone();
        config.root = data.join("goose");
        config.workspace = data.join("workspace");
        config.vm_manifest = Some(self.vm_path(profile));
        config.memory = profile.memory;
        config.browser = profile.browser;
        crate::vm::config::VmConfig::load_for(config.vm_manifest.as_ref().unwrap(), &profile.id)?;
        config.validate_browser()?;
        let store = Arc::new(Store::open(&data.join("ayati.sqlite"))?);
        let files = Arc::new(Files::new(&data, store.clone())?);
        let agent = Agent::start_goose(
            store,
            files,
            config,
            (self.config.vm_idle_seconds > 0)
                .then(|| std::time::Duration::from_secs(self.config.vm_idle_seconds)),
        );
        self.agents
            .lock()
            .map_err(|_| anyhow::anyhow!("Workspace unavailable"))?
            .insert(profile.id.clone(), agent);
        Ok(())
    }
    pub fn begin_shutdown(&self) {
        let _operation = self.operation.lock().unwrap();
        self.closing.store(true, Ordering::SeqCst);
        self.cancelled.cancel();
        for agent in self.agents.lock().unwrap().values() {
            agent.begin_shutdown();
        }
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.begin_shutdown();
        if let Some(preparation) = self.preparation.lock().await.take() {
            preparation.await?;
        }
        let agents: Vec<_> = self.agents.lock().unwrap().values().cloned().collect();
        let results = futures_util::future::join_all(agents.iter().map(|agent| async {
            agent.wait_until_idle().await;
            agent.shutdown_runtime().await
        }))
        .await;
        for result in results {
            result?;
        }
        Ok(())
    }
}
