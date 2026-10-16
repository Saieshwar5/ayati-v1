use super::Workspace;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Duration;

impl Workspace {
    // Read-only checks: never boot a guest or open a saved conversation.
    pub async fn setup_status(&self) -> Value {
        let config = &self.template;
        let key_name = match config.provider.as_str() {
            "fireworks-ai" => "FIREWORKS_API_KEY",
            "openai" => "OPENAI_API_KEY",
            _ => "unsupported provider",
        };
        let credential = std::env::var(key_name).is_ok_and(|s| !s.trim().is_empty());
        let endpoint = crate::runtime::provider::endpoint(&config.provider).is_ok();
        let mut checks = vec![
            json!({"label":"Provider endpoint", "ok":endpoint, "help":"Use AYATI_GOOSE_PROVIDER=fireworks-ai or openai with a valid HTTPS endpoint."}),
            json!({"label":key_name,"ok":credential,"help":"Set this credential in your local .env, then restart Ayati. Never paste it into chat."}),
            json!({"label":"Model selected","ok":!config.model.trim().is_empty(),"help":"Set AYATI_MODEL to a model available to your provider account, then restart Ayati."}),
        ];
        if let Some(path) = &config.vm_manifest {
            let valid = crate::vm::config::VmConfig::load_for(path, &config.agent_id).is_ok();
            checks.push(json!({"label":"VM manifest and files","ok":valid,"help":"Follow the Local VM setup in README.md. Preserve existing disks; prepare a fresh directory if files are missing."}));
            let kvm = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/kvm")
                .is_ok();
            checks.push(json!({"label":"KVM access","ok":kvm,"help":"Enable hardware virtualization and ensure this user can read and write /dev/kvm."}));
        } else {
            let version = tokio::time::timeout(
                Duration::from_secs(5),
                tokio::process::Command::new(&config.binary)
                    .arg("--version")
                    .kill_on_drop(true)
                    .output(),
            )
            .await;
            let valid = matches!(version, Ok(Ok(output)) if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == crate::runtime::config::GOOSE_VERSION);
            checks.push(json!({"label":"Goose 1.54.0","ok":valid,"help":"Run ./tools/install-goose.sh, then restart Ayati."}));
        }
        json!({"provider":config.provider,"model":config.model,"execution":if config.vm_manifest.is_some(){"vm"}else{"host"},
            "supported":!self.config.prototype,"can_test":!self.config.prototype && credential && endpoint && !config.model.trim().is_empty(),"checks":checks})
    }

    pub async fn test_provider(&self) -> Result<Value> {
        ensure!(
            !self.config.prototype,
            "Connection testing is available for the Goose runtime only"
        );
        let _guard = self.setup_test.try_lock().map_err(|_| {
            anyhow::anyhow!("A connection test is already running; wait for its result")
        })?;
        let (url, key_name) = crate::runtime::provider::endpoint(&self.template.provider)?;
        let key = std::env::var(key_name).unwrap_or_default();
        ensure!(
            !key.trim().is_empty(),
            "Configure the provider credential locally and restart Ayati"
        );
        ensure!(
            !self.template.model.trim().is_empty(),
            "Configure a model and restart Ayati"
        );
        // Fixed synthetic input, no tools, no history, no automatic retry or redirects.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()?;
        let request = async {
            let response = client.post(url).bearer_auth(key).json(&json!({
                "model":self.template.model,"messages":[{"role":"user","content":"Reply with OK."}],
                "stream":false,"max_tokens":64
            })).send().await.map_err(|_| anyhow::anyhow!("Connection failed or timed out. Check network and provider endpoint; the request may have been received."))?;
            ensure!(
                response.status().is_success(),
                "Provider returned HTTP {}. Check credentials, model access, quota and provider availability.",
                response.status().as_u16()
            );
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| anyhow::anyhow!("Provider response interrupted; no retry was sent"))?
            {
                ensure!(
                    bytes.len() + chunk.len() <= 65536,
                    "Provider response exceeded the test limit"
                );
                bytes.extend_from_slice(&chunk);
            }
            let body: Value = serde_json::from_slice(&bytes)
                .map_err(|_| anyhow::anyhow!("Provider returned an invalid response"))?;
            ensure!(
                body["choices"][0]["message"]["content"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty()),
                "Provider returned no text. This limited test may be insufficient for a reasoning model; no retry was sent."
            );
            Ok(
                json!({"ok":true,"message":"Provider returned text for the configured model. This checks the connection only; agent tools, saved sessions and VM execution were not exercised."}),
            )
        };
        tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => anyhow::bail!("Ayati is stopping; test cancelled without retry"),
            result = request => result,
        }
    }
}
