use super::GooseRuntime;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Mutex};

pub(super) struct BrowserControl {
    path: PathBuf,
    owner: Mutex<Option<String>>,
    pub(super) gate: tokio::sync::Mutex<()>,
}

impl BrowserControl {
    pub fn load(root: &std::path::Path) -> Result<Self> {
        let path = root.join("browser-owner.json");
        let owner = if path.exists() {
            serde_json::from_slice(&std::fs::read(&path)?)?
        } else {
            None
        };
        Ok(Self {
            path,
            owner: Mutex::new(owner),
            gate: tokio::sync::Mutex::new(()),
        })
    }
    pub fn owned(&self) -> bool {
        self.owner.lock().map_or(true, |v| v.is_some())
    }
    fn save(&self, owner: Option<String>) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("Missing browser control directory")?;
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        let temp = self.path.with_extension("new");
        std::fs::write(&temp, serde_json::to_vec(&owner)?)?;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
        std::fs::File::open(&temp)?.sync_all()?;
        std::fs::rename(temp, &self.path)?;
        *self
            .owner
            .lock()
            .map_err(|_| anyhow::anyhow!("Browser ownership unavailable"))? = owner;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

impl GooseRuntime {
    pub async fn prepare_browser_take(&self, token: &str) -> Result<()> {
        let control = self.browser.as_ref().context("Browser is not enabled")?;
        let _gate = control.gate.lock().await;
        ensure!(self.shared.browser_enabled(), "Browser is disabled");
        self.prepare_take(control, token)
    }

    fn prepare_take(&self, control: &BrowserControl, token: &str) -> Result<()> {
        uuid::Uuid::parse_str(token).context("Invalid controller token")?;
        ensure!(
            control
                .owner
                .lock()
                .map_err(|_| anyhow::anyhow!("Browser ownership unavailable"))?
                .as_deref()
                .is_none_or(|old| old == token),
            "Another tab controls this browser"
        );
        control.save(Some(token.to_owned()))?;
        if let Some(turn) = self
            .shared
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Runtime ownership unavailable"))?
            .as_ref()
        {
            self.shared.store.interrupt_for_owner_browser()?;
            turn.cancel.cancel();
        }
        self.shared.store.cancel_all_browser_permissions()?;
        Ok(())
    }
    pub fn browser_owned(&self) -> bool {
        self.browser.as_ref().is_some_and(BrowserControl::owned)
    }
    pub async fn browser_status(&self) -> Value {
        let vm = self.environment_status().await;
        let guest = if self.shared.browser_enabled() && vm["state"] == "ready" {
            match &self.vm {
                Some(vm) => vm.browser(json!({"op":"status"})).await.ok(),
                None => None,
            }
        } else {
            None
        };
        json!({"configured":self.shared.browser_enabled(),"owner":self.browser_owned(),"vm":vm,"guest":guest})
    }
    pub async fn browser_idle_safe(&self) -> Result<bool> {
        if !self.browser_capable() {
            return Ok(true);
        }
        let state = self
            .vm
            .as_ref()
            .context("Browser requires VM execution")?
            .browser(json!({"op":"status"}))
            .await?;
        Ok(state["owner"] == false
            && state["enabled"] == false
            && state["uncertain"] == false
            && state["interaction_pending"] == false)
    }
    pub async fn browser_request(&self, request: Value) -> Result<Value> {
        let control = self
            .browser
            .as_ref()
            .context("Enable AYATI_BROWSER=1 with a browser-enabled VM")?;
        let _gate = control.gate.lock().await;
        ensure!(self.shared.browser_enabled(), "Browser is disabled");
        let vm = self
            .vm
            .as_ref()
            .context("Browser execution requires a real VM")?;
        let op = request["op"]
            .as_str()
            .context("Missing browser operation")?
            .to_owned();
        ensure!(
            [
                "take", "resume", "finish", "frame", "input", "put_file", "get_file"
            ]
            .contains(&op.as_str()),
            "Unsupported owner browser operation"
        );
        let token = request["token"].as_str();
        if op == "take" {
            let token = token.context("Missing controller token")?;
            self.prepare_take(control, token)?;
            vm.start().await?;
        } else if op != "frame" || control.owned() {
            ensure!(
                token.is_some()
                    && control
                        .owner
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Browser ownership unavailable"))?
                        .as_deref()
                        == token,
                "This browser controller has expired"
            );
        }
        let result = vm.browser(request).await?;
        if op == "resume" || op == "finish" {
            control.save(None)?;
        }
        Ok(result)
    }
    pub(super) async fn arm_browser(&self, run: &str) -> Result<()> {
        if self.shared.browser_enabled() {
            ensure!(!self.browser_owned(), "Waiting for owner browser control");
            self.vm
                .as_ref()
                .context("Browser requires VM execution")?
                .browser(json!({"op":"arm","token":run}))
                .await?;
        }
        Ok(())
    }
    pub(super) async fn disarm_browser(&self) -> Result<()> {
        if self.shared.browser_enabled() {
            self.vm
                .as_ref()
                .context("Browser requires VM execution")?
                .browser(json!({"op":"disarm"}))
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_control_survives_restart_and_requires_explicit_release() {
        let directory = tempfile::tempdir().unwrap();
        let control = BrowserControl::load(directory.path()).unwrap();
        control
            .save(Some(uuid::Uuid::new_v4().to_string()))
            .unwrap();
        assert!(BrowserControl::load(directory.path()).unwrap().owned());
        control.save(None).unwrap();
        assert!(!BrowserControl::load(directory.path()).unwrap().owned());
        std::fs::write(&control.path, "invalid").unwrap();
        assert!(BrowserControl::load(directory.path()).is_err());
    }
}
