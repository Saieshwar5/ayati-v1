//! Model providers: those added in the console (sealed keys) and those from the
//! deployment's providers file, each with an emergency switch and live health.
use super::*;

/// A provider from `AYATI_MODEL_PROVIDERS_FILE`: shown read-only, never its key.
#[derive(Clone, Debug)]
pub(crate) struct FileProvider {
    pub name: String,
    pub base_url: String,
}

/// A catalog route naming a provider, for the provider list and its switch.
#[derive(Clone, Debug)]
pub(crate) struct RouteUse {
    /// The catalog model people choose.
    pub model: String,
    pub provider: String,
    /// The provider's own name for it.
    pub provider_model: String,
    pub priority: u8,
}

/// What the gateway needs to call a console provider.
pub(crate) struct Stored {
    /// `None`: the address comes from the providers file.
    pub base_url: Option<String>,
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
    /// Every provider: console and deployment ones, any that only appear in
    /// recent calls, their switch, last-hour health and the catalog routes that
    /// name them (from the caller, who reads the catalog file).
    pub(crate) async fn operator_providers(
        &self,
        files: &[FileProvider],
        routes: &[RouteUse],
    ) -> Result<Value> {
        let hour_ago = now() * 1000 - 3_600_000;
        let health = rows(&self.db, "SELECT p.provider,
              (SELECT count(*) FROM relay_model_usage u WHERE u.provider=p.provider AND u.at_ms>=$1 AND u.assignment<>'library') AS calls,
              (SELECT count(*) FROM relay_model_failures f WHERE f.provider=p.provider AND f.at_ms>=$1) AS errors,
              (SELECT percentile_disc(0.95) WITHIN GROUP (ORDER BY u.latency_ms) FROM relay_model_usage u
                WHERE u.provider=p.provider AND u.at_ms>=$1 AND u.latency_ms IS NOT NULL)::bigint AS p95
            FROM (SELECT provider FROM relay_model_usage WHERE at_ms>=$1 AND assignment<>'library'
              UNION SELECT provider FROM relay_model_failures WHERE at_ms>=$1) p", vec![hour_ago.into()]).await?;
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
        let routes_of = |name: &str| {
            json!(routes.iter().filter(|r| r.provider == name)
                .map(|r| json!({"model": r.model, "provider_model": r.provider_model, "priority": r.priority}))
                .collect::<Vec<_>>())
        };
        let mut out = Vec::new();
        let console = rows(&self.db, "SELECT name,base_url,key_hint,key_rotated_ms,created_ms FROM relay_model_providers ORDER BY name", vec![]).await?;
        let mut keys = std::collections::HashMap::new();
        for row in console {
            let name: String = row.try_get("", "name")?;
            let key = json!({"key_hint": row.try_get::<String>("", "key_hint")?, "key_rotated_ms": row.try_get::<i64>("", "key_rotated_ms")?});
            match row.try_get::<Option<String>>("", "base_url")? {
                Some(base_url) => out.push(json!({"name": name, "base_url": base_url, "source": "console",
                    "key_hint": key["key_hint"], "key_rotated_ms": key["key_rotated_ms"], "key_in_console": true,
                    "created_ms": row.try_get::<i64>("", "created_ms")?})),
                None => { keys.insert(name, key); }
            }
        }
        for file in files {
            if !out.iter().any(|p| p["name"] == file.name.as_str()) {
                let mut entry = json!({"name": file.name, "base_url": file.base_url, "source": "deployment", "key_in_console": false});
                if let Some(key) = keys.remove(&file.name) {
                    entry["key_in_console"] = json!(true);
                    entry["key_hint"] = key["key_hint"].clone();
                    entry["key_rotated_ms"] = key["key_rotated_ms"].clone();
                }
                out.push(entry);
            }
        }
        // Calls recorded for providers no longer configured, so nothing hides.
        for name in stats.keys() {
            if !out.iter().any(|p| p["name"] == name.as_str()) {
                out.push(json!({"name": name, "source": "missing"}));
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
            provider["routes"] = routes_of(&name);
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

    /// Saves a new API key for a provider: a console provider's, or one from the
    /// providers file, whose key then comes from here instead of its key file.
    pub(crate) async fn operator_store_key(
        &self,
        by: &Operator,
        name: &str,
        sealed: Vec<u8>,
        key_hint: &str,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let replaced = query(
            &tx,
            "SELECT 1 AS found FROM relay_model_providers WHERE name=$1 FOR UPDATE",
            vec![name.into()],
        )
        .await?
        .is_some();
        execute(&tx, "INSERT INTO relay_model_providers(name,base_url,key_sealed,key_hint,key_rotated_ms,created_by,created_ms)
            VALUES($1,NULL,$2,$3,$4,$5,$4) ON CONFLICT(name) DO UPDATE SET key_sealed=excluded.key_sealed,key_hint=excluded.key_hint,
            key_rotated_ms=excluded.key_rotated_ms",
            vec![name.into(), sealed.into(), key_hint.into(), (now() * 1000).into(), by.user.id.clone().into()]).await?;
        let summary = if replaced {
            format!("replaced the API key for {name} (now ending {key_hint})")
        } else {
            format!("stored the API key for {name} in Cuckoo Control (ending {key_hint})")
        };
        record(&tx, by, "providers", "key_stored", name, &summary, why).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Removes a console provider, or a file provider's console key (it then
    /// uses its key file again). Callers check no catalog route needs it. Its
    /// calls stay in the records.
    pub(crate) async fn operator_remove_provider(
        &self,
        by: &Operator,
        name: &str,
        why: &str,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        let row = query(&tx, "DELETE FROM relay_model_providers WHERE name=$1 RETURNING base_url IS NULL AS key_only", vec![name.into()])
            .await?
            .context("Cuckoo Control keeps nothing for this provider")?;
        let summary = if row.try_get::<bool>("", "key_only")? {
            format!("removed {name}'s API key from Cuckoo Control; it uses its key file again")
        } else {
            format!("removed model provider {name}")
        };
        record(
            &tx,
            by,
            "providers",
            "provider_removed",
            name,
            &summary,
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
