//! `ayati admin`: the same actions as the Cuckoo Control web console, from a
//! terminal, with the same change log (marked "cli"). Lists print a short table;
//! `--json` prints the full answer for scripts. Models, routes and prices live in
//! the reviewed model catalog file (runtime/model-catalog.md); `models` shows them.
use super::super::models::catalog::Files;
use super::super::postgres::{
    PgStore,
    operator::{Operator, Via},
};
use super::Deps;
use super::cli_args::Args;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

const HELP: &str = "Cuckoo Control from a terminal. Acts as AYATI_OPERATOR (or --as EMAIL).

  ayati admin overview
  ayati admin workspaces [SEARCH]
  ayati admin workspace show ID
  ayati admin workspace suspend ID --reason TEXT
  ayati admin workspace resume ID --reason TEXT
  ayati admin providers
  ayati admin provider key NAME --reason TEXT               (reads the API key from stdin)
  ayati admin provider add NAME --url URL --reason TEXT     (a provider outside the providers file; key from stdin)
  ayati admin provider (off|on) NAME --reason TEXT
  ayati admin provider remove NAME --reason TEXT
  ayati admin models [--hours 24]                           (catalog routes, traffic, failures and margin)
  ayati admin spend [--days 30]
  ayati admin settings
  ayati admin setting set KEY VALUE --reason TEXT
  ayati admin codes
  ayati admin code issue --for TEXT [--days 14]
  ayati admin code revoke ID --reason TEXT
  ayati admin log [--area AREA]
  ayati admin reports

Add --json to any command for the full answer.";

pub async fn run(raw: &[String]) -> Result<()> {
    let args = Args::parse(raw)?;
    if args.words.is_empty() || matches!(args.words[0].as_str(), "help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let config = crate::config::Admin::from_env()?;
    let store = PgStore::connect(&config.database_url).await?;
    store.verify_schema().await?;
    let catalog = config
        .model_catalog_file
        .clone()
        .zip(config.model_providers_file.clone())
        .map(|(catalog, providers)| Files { catalog, providers });
    let deps = Deps {
        store: &store,
        providers_file: config.model_providers_file.as_deref(),
        secrets_key_file: config.secrets_key_file.as_deref(),
        catalog: catalog.as_ref(),
    };
    let email = args
        .option("as")
        .map(str::to_lowercase)
        .or(config.operator.clone())
        .context("Set AYATI_OPERATOR to your email, or add --as EMAIL")?;
    ensure!(
        config.platform_admins.contains(&email),
        "{email} is not in AYATI_PLATFORM_ADMINS"
    );
    let user = store
        .operator_user(&email)
        .await?
        .with_context(|| format!("{email} has not signed in to Cuckoo yet; sign in once first"))?;
    let by = Operator {
        user,
        via: Via::Cli,
    };
    let words: Vec<&str> = args.words.iter().map(String::as_str).collect();
    let answer = command(&deps, &by, &args, &words).await?;
    if args.switch("json") {
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        print_human(&words, &answer);
    }
    Ok(())
}

fn reason(args: &Args) -> Result<String> {
    super::super::postgres::operator::reason(args.required("reason")?)
}

fn read_key() -> Result<String> {
    eprintln!("Paste the API key, then press Enter (it is not shown again):");
    let mut key = String::new();
    std::io::stdin().read_line(&mut key)?;
    Ok(key.trim().to_owned())
}

fn number<T: std::str::FromStr>(args: &Args, name: &str, default: T) -> Result<T> {
    args.option(name)
        .map(str::parse)
        .transpose()
        .map_err(|_| anyhow::anyhow!("Use a number for --{name}"))
        .map(|v| v.unwrap_or(default))
}

async fn command(deps: &Deps<'_>, by: &Operator, args: &Args, words: &[&str]) -> Result<Value> {
    let store = deps.store;
    Ok(match words {
        ["overview"] => {
            args.only(&[])?;
            store.operator_overview().await?
        }
        ["workspaces", rest @ ..] => {
            args.only(&[])?;
            store.operator_workspaces(&rest.join(" ")).await?
        }
        ["workspace", "show", id] => store.operator_workspace(id).await?,
        ["workspace", "suspend", id] => {
            args.only(&["reason"])?;
            store.operator_suspend(by, id, &reason(args)?).await?;
            store.operator_workspace(id).await?
        }
        ["workspace", "resume", id] => {
            args.only(&["reason"])?;
            store.operator_resume(by, id, &reason(args)?).await?;
            store.operator_workspace(id).await?
        }
        ["providers"] => deps.providers().await?,
        ["provider", "key", name] => {
            args.only(&["reason"])?;
            let why = reason(args)?;
            deps.store_key(by, name, &read_key()?, &why).await?;
            deps.providers().await?
        }
        ["provider", "add", name] => {
            args.only(&["url", "reason"])?;
            let why = reason(args)?;
            deps.add_provider(by, name, args.required("url")?, &read_key()?, &why)
                .await?;
            deps.providers().await?
        }
        ["provider", state @ ("on" | "off"), name] => {
            args.only(&["reason"])?;
            deps.switch_provider(by, name, *state == "on", &reason(args)?)
                .await?;
            deps.providers().await?
        }
        ["provider", "remove", name] => {
            args.only(&["reason"])?;
            deps.remove_provider(by, name, &reason(args)?).await?;
            deps.providers().await?
        }
        ["models"] => {
            args.only(&["hours"])?;
            let hours: i64 = number(args, "hours", 24)?;
            ensure!((1..=24 * 31).contains(&hours), "Choose 1 hour to 31 days");
            let mut report = store
                .model_operations(super::super::user::now() * 1000 - hours * 3_600_000)
                .await?;
            report["catalog"] = match deps.catalog.map(Files::load) {
                None => json!({"on": false, "models": []}),
                Some(Err(error)) => {
                    json!({"on": true, "error": format!("{error:#}"), "models": []})
                }
                Some(Ok(catalog)) => json!({"on": true, "models": catalog.models}),
            };
            report
        }
        ["spend"] => {
            args.only(&["days"])?;
            store.operator_spend(number(args, "days", 30)?).await?
        }
        ["settings"] => store.operator_settings().await?,
        ["setting", "set", key, value] => {
            args.only(&["reason"])?;
            let value = store
                .operator_set_setting(by, key, &json!(value), &reason(args)?)
                .await?;
            json!({"key": key, "value": value})
        }
        ["codes"] => store.workspace_codes().await?,
        ["code", "issue"] => {
            args.only(&["for", "days"])?;
            let note = args.required("for")?;
            let days: i64 = number(args, "days", 14)?;
            let issued = store.issue_workspace_code(&by.user, note, days).await?;
            store
                .operator_record(
                    by,
                    "codes",
                    "code_issued",
                    note,
                    &format!("issued an invite code for {note}, valid {days} days"),
                    &format!("Invite code for {note}"),
                )
                .await?;
            issued
        }
        ["code", "revoke", id] => {
            args.only(&["reason"])?;
            let why = reason(args)?;
            store.revoke_workspace_code(&by.user, id).await?;
            store
                .operator_record(
                    by,
                    "codes",
                    "code_revoked",
                    id,
                    "revoked an unused invite code",
                    &why,
                )
                .await?;
            json!({"revoked": id})
        }
        ["log"] => {
            args.only(&["area"])?;
            store.operator_log(args.option("area"), None, 50).await?
        }
        ["reports"] => json!({"reports": store.problem_reports(100).await?}),
        _ => bail!("Unknown command. Run ayati admin help"),
    })
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => "—".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Milliseconds since 1970 as `YYYY-MM-DD HH:MM` UTC (Howard Hinnant's civil date).
fn utc(value: &Value) -> String {
    let Some(ms) = value.as_i64() else {
        return "—".into();
    };
    let (days, minute) = (
        ms.div_euclid(86_400_000),
        ms.rem_euclid(86_400_000) / 60_000,
    );
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (day, month) = (
        doy - (153 * mp + 2) / 5 + 1,
        if mp < 10 { mp + 3 } else { mp - 9 },
    );
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        minute / 60,
        minute % 60
    )
}

fn money(value: &Value) -> String {
    value
        .as_f64()
        .map(|v| format!("${v:.2}"))
        .unwrap_or_else(|| "—".into())
}

/// A readable summary for people; scripts use --json.
fn print_human(words: &[&str], answer: &Value) {
    let rows = |key: &str| answer[key].as_array().cloned().unwrap_or_default();
    match words.first().copied() {
        Some("workspaces") => {
            println!(
                "{:<38} {:<28} {:>6} {:>6} {:>10} {:>10}  STATUS",
                "ID", "NAME", "PEOPLE", "AGENTS", "CHARGED", "BUDGET"
            );
            for w in rows("workspaces") {
                println!(
                    "{:<38} {:<28} {:>6} {:>6} {:>10} {:>10}  {}",
                    text(&w["id"]),
                    text(&w["name"]).chars().take(28).collect::<String>(),
                    text(&w["people"]),
                    text(&w["agents"]),
                    money(&w["charge"]),
                    money(&w["budget"]),
                    text(&w["status"])
                );
            }
            println!(
                "{} of {} workspaces",
                rows("workspaces").len(),
                text(&answer["total"])
            );
        }
        Some("providers" | "provider") => {
            for p in rows("providers") {
                let health = &p["health"];
                let key = if p["key_in_console"] == true {
                    format!("key …{} in console", text(&p["key_hint"]))
                } else {
                    "key file".into()
                };
                println!(
                    "{:<16} {:<4} {:<10} {:<22} {}  calls/h {} failed {} p95 {} ms",
                    text(&p["name"]),
                    if p["enabled"] == true { "on" } else { "OFF" },
                    text(&p["source"]),
                    key,
                    text(&p["base_url"]),
                    text(&health["calls"]),
                    text(&health["errors"]),
                    text(&health["p95_ms"])
                );
                for r in p["routes"].as_array().into_iter().flatten() {
                    println!(
                        "    serves {:<28} as {} (priority {})",
                        text(&r["model"]),
                        text(&r["provider_model"]),
                        text(&r["priority"])
                    );
                }
            }
        }
        Some("models") => {
            for p in rows("providers") {
                println!(
                    "{:<16} calls {:>7}  after fallback {:>5}  p95 {:>6} ms  charged {}  cost {}  margin {}",
                    text(&p["key"]),
                    text(&p["calls"]),
                    text(&p["after_fallback"]),
                    text(&p["latency_p95_ms"]),
                    money(&p["charge_usd"]),
                    money(&p["cost_usd"]),
                    money(&p["margin_usd"])
                );
            }
            for f in rows("failures") {
                println!(
                    "  failure {:<14} status {:>3} × {}",
                    text(&f["provider"]),
                    text(&f["status"]),
                    text(&f["count"])
                );
            }
        }
        Some("log") => {
            for e in rows("entries") {
                println!(
                    "{} {:<4} {:<10} {} {} — {}",
                    utc(&e["at_ms"]),
                    text(&e["via"]),
                    text(&e["area"]),
                    text(&e["by"]),
                    text(&e["summary"]),
                    text(&e["reason"])
                );
            }
        }
        Some("spend") => {
            let (m, t, f) = (&answer["month"], &answer["today"], &answer["forecast"]);
            println!(
                "This month charged {} cost {} margin {}  ·  today charged {} cost {}  ·  forecast charged {} cost {}",
                money(&m["charge"]),
                money(&m["cost"]),
                money(&m["margin"]),
                money(&t["charge"]),
                money(&t["cost"]),
                money(&f["charge"]),
                money(&f["cost"])
            );
            for m in rows("by_model") {
                println!(
                    "  {:<40} calls {:>7}  charged {}  cost {}",
                    text(&m["model"]),
                    text(&m["calls"]),
                    money(&m["charge"]),
                    money(&m["cost"])
                );
            }
        }
        Some("code") if answer.get("code").is_some() => {
            println!(
                "Code for {}: {}  (shown once)",
                text(&answer["note"]),
                text(&answer["code"])
            )
        }
        _ => println!(
            "{}",
            serde_json::to_string_pretty(answer).unwrap_or_default()
        ),
    }
}

#[cfg(test)]
#[test]
fn log_times_read_as_utc() {
    assert_eq!(utc(&json!(1_793_962_080_000i64)), "2026-11-06 10:48");
    assert_eq!(utc(&json!(1_709_164_800_000i64)), "2024-02-29 00:00");
    assert_eq!(utc(&Value::Null), "—");
}
