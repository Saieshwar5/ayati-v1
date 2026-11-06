//! Model providers: those added in the console (sealed keys) and those from the
//! deployment's providers file, each with an emergency switch and live health.
use super::*;

/// A provider from `AYATI_MODEL_PROVIDERS_FILE`: shown read-only, never its key.
#[derive(Clone, Debug)]
pub(crate) struct FileProvider {
    pub name: String,
    pub base_url: String,
}

/// What the gateway needs to call a console provider.
pub(crate) struct Stored {
    pub base_url: String,
    pub sealed: Vec<u8>,
}

/// Provider names are lowercase, so they read the same in the CLI and in routes.
pub(crate) fn name(text: &str) -> Result<String> {
    let text = text.trim();
    ensure!(
        !text.is_empty()
            && text.len() <= 32
            && text.bytes().enumerate().all(|(i, b)| b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || (i > 0 && (b == b'-' || b == b'_'))),
        "Use a provider name of lowercase letters, digits, - and _ (up to 32)"
    );
    Ok(text.to_owned())
}

/// The same rule as the gateway: HTTPS, or loopback HTTP for local checks.
pub(crate) fn base_url(text: &str) -> Result<String> {
    let url = reqwest::Url::parse(text.trim()).context("Use a full provider address")?;
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")),
        "Model providers need HTTPS; loopback HTTP is for local checks"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.query().is_none(),
        "Put the API key in the key field, not in the address"
    );
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// The last four characters, so people can tell keys apart.
pub(crate) fn hint(key: &str) -> Result<String> {
    let key = key.trim();
    ensure!(
        (8..=4096).contains(&key.len()) && !key.chars().any(char::is_whitespace),
        "Paste the whole API key (at least 8 characters, no spaces)"
    );
    Ok(key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

impl PgStore {
    pub(crate) async fn operator_providers(&self, files: &[FileProvider]) -> Result<Value> {
        let hour_ago = now() * 1000 - 3_600_000;
        let health = rows(&self.db, "SELECT provider, count(*) AS calls,
              count(*) FILTER (WHERE outcome='error') AS errors,
              (percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) FILTER (WHERE outcome='ok'))::bigint AS p95
            FROM relay_model_usage WHERE at_ms>=$1 GROUP BY provider", vec![hour_ago.into()]).await?;
        let mut stats = std::collections::HashMap::new();
        for row in health {
            stats.insert(
                row.try_get::<String>("", "provider")?,
                json!({"calls": row.try_get::<i64>("", "calls")?, "errors": row.try_get::<i64>("", "errors")?,
                    "p95_ms": row.try_get::<Option<i64>>("", "p95")?}),
            );
        }
        let switches = json_rows(&self.db, "SELECT jsonb_build_object('provider',s.provider,'enabled',s.enabled,'note',s.note,'changed_ms',s.changed_ms,
            'by',COALESCE(u.display_name,u.email)) AS data FROM relay_model_provider_switches s JOIN relay_users u ON u.id=s.changed_by", vec![]).await?;
        let switch_of = |name: &str| switches.iter().find(|s| s["provider"] == name).cloned();
        let models = json_rows(&self.db, "SELECT jsonb_build_object('provider',provider,'model',model,'enabled',enabled,
            'input_per_mtok',input_micros_per_mtok/1000000.0,'output_per_mtok',output_micros_per_mtok/1000000.0,'changed_ms',changed_ms) AS data
            FROM relay_platform_models ORDER BY provider,model", vec![]).await?;
        let mut out = Vec::new();
        let console = rows(&self.db, "SELECT name,base_url,key_hint,key_rotated_ms,created_ms FROM relay_model_providers ORDER BY name", vec![]).await?;
        for row in console {
            let name: String = row.try_get("", "name")?;
            out.push(json!({"name": name, "base_url": row.try_get::<String>("", "base_url")?, "source": "console",
                "key_hint": row.try_get::<String>("", "key_hint")?, "key_rotated_ms": row.try_get::<i64>("", "key_rotated_ms")?,
                "created_ms": row.try_get::<i64>("", "created_ms")?}));
        }
        for file in files {
            if !out.iter().any(|p| p["name"] == file.name.as_str()) {
                out.push(
                    json!({"name": file.name, "base_url": file.base_url, "source": "deployment"}),
                );
            }
        }
        for provider in &mut out {
            let name = provider["name"].as_str().unwrap_or_default().to_owned();
            let switch = switch_of(&name);
            provider["enabled"] = json!(switch.as_ref().is_none_or(|s| s["enabled"] == true));
            provider["switch"] = switch.unwrap_or(Value::Null);
            provider["health"] = stats
                .remove(&name)
                .unwrap_or(json!({"calls":0,"errors":0,"p95_ms":null}));
            provider["models"] = json!(
                models
                    .iter()
                    .filter(|m| m["provider"] == name.as_str())
                    .collect::<Vec<_>>()
            );
        }
        // Calls recorded for providers no longer configured, so nothing hides.
        for (name, health) in stats {
            out.push(json!({"name": name, "source": "missing", "enabled": switch_of(&name).is_none_or(|s| s["enabled"] == true),
                "health": health, "models": models.iter().filter(|m| m["provider"] == name.as_str()).collect::<Vec<_>>()}));
        }
        Ok(json!({"providers": out}))
    }

    pub(crate) async fn operator_add_provider(
        &self,
        by: &Operator,
        name: &str,
        base_url: &str,
        sealed: Vec<u8>,
        key_hint: &str,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let added = execute(&tx, "INSERT INTO relay_model_providers(name,base_url,key_sealed,key_hint,key_rotated_ms,created_by,created_ms)
            VALUES($1,$2,$3,$4,$5,$6,$5) ON CONFLICT(name) DO NOTHING",
            vec![name.into(), base_url.into(), sealed.into(), key_hint.into(), (now() * 1000).into(), by.user.id.clone().into()]).await?;
        ensure!(added == 1, "A provider named {name} already exists");
        record(
            &tx,
            by,
            "providers",
            "provider_added",
            name,
            &format!("added model provider {name} at {base_url}"),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn operator_rotate_key(
        &self,
        by: &Operator,
        name: &str,
        sealed: Vec<u8>,
        key_hint: &str,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let changed = execute(&tx, "UPDATE relay_model_providers SET key_sealed=$2,key_hint=$3,key_rotated_ms=$4 WHERE name=$1",
            vec![name.into(), sealed.into(), key_hint.into(), (now() * 1000).into()]).await?;
        ensure!(
            changed == 1,
            "Only providers added in Cuckoo Control keep their key here; change {name}'s key file in the deployment"
        );
        record(
            &tx,
            by,
            "providers",
            "key_rotated",
            name,
            &format!("replaced the API key for {name} (now ending {key_hint})"),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Removes a console provider no route uses; its calls stay in the records.
    pub(crate) async fn operator_remove_provider(
        &self,
        by: &Operator,
        name: &str,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let used: i64 = query(&tx, "SELECT count(*) AS n FROM relay_model_routes WHERE main_provider=$1 OR second_provider=$1", vec![name.into()])
            .await?.context("Missing route count")?.try_get("", "n")?;
        ensure!(used == 0, "Routes still use {name}; change them first");
        let removed = execute(
            &tx,
            "DELETE FROM relay_model_providers WHERE name=$1",
            vec![name.into()],
        )
        .await?;
        ensure!(
            removed == 1,
            "Only providers added in Cuckoo Control can be removed here"
        );
        record(
            &tx,
            by,
            "providers",
            "provider_removed",
            name,
            &format!("removed model provider {name}"),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The emergency switch. Turning off takes effect on the gateway's next call.
    pub(crate) async fn operator_switch_provider(
        &self,
        by: &Operator,
        name: &str,
        enabled: bool,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        execute(&tx, "INSERT INTO relay_model_provider_switches(provider,enabled,note,changed_by,changed_ms) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(provider) DO UPDATE SET enabled=excluded.enabled,note=excluded.note,changed_by=excluded.changed_by,changed_ms=excluded.changed_ms",
            vec![name.into(), enabled.into(), why.into(), by.user.id.clone().into(), (now() * 1000).into()]).await?;
        let (action, summary) = if enabled {
            (
                "provider_on",
                format!("turned model provider {name} back on"),
            )
        } else {
            (
                "provider_off",
                format!("turned model provider {name} off for every workspace"),
            )
        };
        record(&tx, by, "providers", action, name, &summary, why).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A console provider's address and sealed key, for the gateway.
    pub(crate) async fn model_provider_stored(&self, name: &str) -> Result<Option<Stored>> {
        query(
            &self.db,
            "SELECT base_url,key_sealed FROM relay_model_providers WHERE name=$1",
            vec![name.into()],
        )
        .await?
        .map(|row| {
            Ok(Stored {
                base_url: row.try_get("", "base_url")?,
                sealed: row.try_get("", "key_sealed")?,
            })
        })
        .transpose()
    }

    /// The operator's note while `name` is switched off; `None` while it is on.
    pub(crate) async fn model_provider_off(&self, name: &str) -> Result<Option<String>> {
        query(
            &self.db,
            "SELECT note FROM relay_model_provider_switches WHERE provider=$1 AND NOT enabled",
            vec![name.into()],
        )
        .await?
        .map(|row| Ok(row.try_get::<String>("", "note")?))
        .transpose()
    }
}
