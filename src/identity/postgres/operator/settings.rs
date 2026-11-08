//! Platform settings an operator changes in the console. Each one takes effect
//! on its next use; settings from the deployment's environment are shown by the
//! HTTP layer, read-only.
use super::*;

pub(crate) const SIGNUP_PAUSED: &str = "signup.paused";
pub(crate) const DAILY_CEILING: &str = "platform.daily_ceiling_usd";
pub(crate) const COMPUTERS_PAUSED: &str = "computers.paused";

struct Definition {
    key: &'static str,
    about: &'static str,
    /// `bool` or `usd` (a dollar amount, or null for no limit).
    kind: &'static str,
    default: fn() -> Value,
}

const DEFINITIONS: &[Definition] = &[
    Definition {
        key: SIGNUP_PAUSED,
        about: "Stop new workspaces, even with an invite code. Existing workspaces keep working; operators can still create one.",
        kind: "bool",
        default: || json!(false),
    },
    Definition {
        key: COMPUTERS_PAUSED,
        about: "Stop agents' computers everywhere: none wakes or starts, and awake ones go to sleep within a minute. Files are kept.",
        kind: "bool",
        default: || json!(false),
    },
    Definition {
        key: DAILY_CEILING,
        about: "Total model spend per UTC day across every workspace. Above it, new model calls are refused; empty means no ceiling.",
        kind: "usd",
        default: || Value::Null,
    },
];

fn definition(key: &str) -> Result<&'static Definition> {
    DEFINITIONS
        .iter()
        .find(|d| d.key == key)
        .with_context(|| format!("Unknown setting {key}"))
}

/// Parses and checks a value: `true`/`false`, or dollars / `none`.
pub(crate) fn parse(key: &str, value: &Value) -> Result<Value> {
    let def = definition(key)?;
    match (def.kind, value) {
        ("bool", Value::Bool(_)) => Ok(value.clone()),
        ("bool", Value::String(s)) if s == "true" || s == "false" => Ok(json!(s == "true")),
        ("usd", Value::Null) => Ok(Value::Null),
        ("usd", Value::String(s)) if s.trim().is_empty() || s.trim() == "none" => Ok(Value::Null),
        ("usd", Value::Number(n)) => {
            micros(n.as_f64().unwrap_or(-1.0))?;
            Ok(value.clone())
        }
        ("usd", Value::String(s)) => {
            let n: f64 = s
                .trim()
                .trim_start_matches('$')
                .parse()
                .context("Use a dollar amount or none")?;
            micros(n)?;
            Ok(json!(n))
        }
        ("bool", _) => anyhow::bail!("{key} is true or false"),
        _ => anyhow::bail!("{key} is a dollar amount or none"),
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::Null => "none".into(),
        other => other.to_string(),
    }
}

impl PgStore {
    /// A setting's value, or its default.
    pub(crate) async fn platform_setting(&self, key: &str) -> Result<Value> {
        let def = definition(key)?;
        Ok(query(
            &self.db,
            "SELECT value FROM relay_platform_settings WHERE key=$1",
            vec![key.into()],
        )
        .await?
        .map(|row| row.try_get::<Value>("", "value"))
        .transpose()?
        .unwrap_or_else(def.default))
    }

    /// A dollar setting as micro-dollars; `None` means no limit.
    pub(crate) async fn platform_limit(&self, key: &str) -> Result<Option<i64>> {
        Ok(match self.platform_setting(key).await? {
            Value::Number(n) => Some(micros(n.as_f64().unwrap_or(0.0))?),
            _ => None,
        })
    }

    pub(crate) async fn operator_settings(&self) -> Result<Value> {
        let mut out = Vec::new();
        for def in DEFINITIONS {
            let row = query(&self.db, "SELECT s.value,s.changed_ms,COALESCE(u.display_name,u.email) AS by FROM relay_platform_settings s
                JOIN relay_users u ON u.id=s.changed_by WHERE s.key=$1", vec![def.key.into()]).await?;
            let history = json_rows(&self.db, "SELECT jsonb_build_object('summary',l.summary,'reason',l.reason,'at_ms',l.at_ms,'via',l.via,
                'by',COALESCE(u.display_name,u.email)) AS data FROM relay_operator_log l JOIN relay_users u ON u.id=l.actor
                WHERE l.area='settings' AND l.target=$1 ORDER BY l.id DESC LIMIT 10", vec![def.key.into()]).await?;
            out.push(json!({
                "key": def.key, "about": def.about, "kind": def.kind, "default": (def.default)(),
                "value": match &row { Some(r) => r.try_get::<Value>("", "value")?, None => (def.default)() },
                "changed_ms": row.as_ref().map(|r| r.try_get::<i64>("", "changed_ms")).transpose()?,
                "by": row.as_ref().map(|r| r.try_get::<String>("", "by")).transpose()?,
                "history": history,
            }));
        }
        Ok(json!({"settings": out}))
    }

    pub(crate) async fn operator_set_setting(
        &self,
        by: &Operator,
        key: &str,
        value: &Value,
        why: &str,
    ) -> Result<Value> {
        let value = parse(key, value)?;
        let before = self.platform_setting(key).await?;
        let tx = self.db.begin().await?;
        execute(&tx, "INSERT INTO relay_platform_settings(key,value,changed_by,changed_ms) VALUES($1,$2,$3,$4)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value,changed_by=excluded.changed_by,changed_ms=excluded.changed_ms",
            vec![key.into(), value.clone().into(), by.user.id.clone().into(), (now() * 1000).into()]).await?;
        record(
            &tx,
            by,
            "settings",
            "setting_changed",
            key,
            &format!("changed {key} from {} to {}", shown(&before), shown(&value)),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(value)
    }
}
