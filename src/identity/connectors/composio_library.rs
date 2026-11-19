//! Composio's library, for admins adding an app (schema 135): searching its
//! apps, listing one app's tools as actions to review, and building the added
//! app from the admin's choices. Everything shown or saved is read from
//! Composio here; the browser only names an app, its tools and their effects.
use super::added::{self, AddedAction, AddedApp, MAX_ACTIONS};
use super::composio_api::{self, ComposioApi, Reply};
use super::{LibraryPick, curated};
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Value, json};

const NAME: &str = "composio";
/// Apps or actions listed per request.
const PAGE: usize = 30;
/// A page of tools carries every tool's whole schema.
const MAX_LIST: usize = 2 * 1024 * 1024;

/// A search word as a query value: letters, digits and spaces only.
fn word(query: &str) -> String {
    query
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .take(60)
        .collect::<String>()
        .replace(' ', "%20")
}

async fn get(api: &ComposioApi, path: &str) -> Result<Option<Value>> {
    match api.send_up_to(Method::GET, path, None, MAX_LIST).await? {
        Reply::Ok(value) => Ok(Some(value)),
        Reply::Status(404) => Ok(None),
        Reply::Status(code) => anyhow::bail!("Composio refused the request (HTTP {code})"),
    }
}

/// Why an admin can't add this library app yet, or `None` when they can. Only
/// apps Composio signs in itself are offered: others need a pasted key or the
/// company's own sign-in setup.
fn unsupported(toolkit: &Value) -> Option<&'static str> {
    if toolkit["no_auth"] == true {
        return Some("It has no account to connect; not supported yet");
    }
    let managed = toolkit["composio_managed_auth_schemes"]
        .as_array()
        .is_some_and(|schemes| schemes.iter().any(|s| s == "OAUTH2"));
    (!managed).then_some("Needs an API key or its own sign-in setup; not supported yet")
}

fn listed(toolkit: &Value) -> Option<Value> {
    let slug = toolkit["slug"].as_str()?;
    let id = added::app_id(slug).ok()?;
    let name = toolkit["name"].as_str().filter(|n| !n.trim().is_empty())?;
    // A built-in app is offered already, under Cuckoo's own action names.
    let builtin = curated::catalog().apps.iter().any(|a| {
        a.id == id
            || a.providers
                .get(NAME)
                .is_some_and(|t| t.eq_ignore_ascii_case(slug))
    });
    let reason = unsupported(toolkit);
    Some(json!({
        "app": slug, "id": id, "name": name.chars().take(80).collect::<String>(),
        "description": added::short(toolkit["meta"]["description"].as_str().unwrap_or_default(), name),
        "actions": toolkit["meta"]["tools_count"].as_u64().unwrap_or(0),
        "state": if builtin { "builtin" } else if reason.is_some() { "unsupported" } else { "addable" },
        "reason": reason,
    }))
}

/// Library apps matching `query`: `{apps: [{app, id, name, description,
/// actions, state, reason}]}`.
pub(super) async fn search(api: &ComposioApi, query: &str) -> Result<Value> {
    let query = word(query);
    let path = if query.is_empty() {
        format!("toolkits?limit={PAGE}&sort_by=usage")
    } else {
        format!("toolkits?limit={PAGE}&search={query}")
    };
    let found = get(api, &path).await?.context("Composio has no library")?;
    let apps: Vec<Value> = found["items"]
        .as_array()
        .context("Composio omitted its apps")?
        .iter()
        .filter_map(listed)
        .collect();
    Ok(json!({"apps": apps}))
}

async fn toolkit(api: &ComposioApi, app: &str) -> Result<Value> {
    let found = get(api, &format!("toolkits/{}", composio_api::id(app)?))
        .await?
        .context("The library has no such app")?;
    ensure!(
        found["slug"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(app)),
        "Composio returned a different app"
    );
    Ok(found)
}

/// What Composio says a tool does: `Some(true)` only reads, `Some(false)`
/// changes something, `None` when it doesn't say.
fn reads(tool: &Value) -> Option<bool> {
    let tags: Vec<&str> = tool["tags"]
        .as_array()
        .map(|t| t.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if ["createHint", "updateHint", "destructiveHint"]
        .iter()
        .any(|hint| tags.contains(hint))
    {
        return Some(false);
    }
    tags.contains(&"readOnlyHint").then_some(true)
}

/// One library tool as an action to review: its name, what it does, and either
/// the fields Cuckoo offers or why it can't be chosen.
fn reviewable(app: &str, slug: &str, tool: &Value) -> Option<(Value, Option<AddedAction>)> {
    let name = tool["slug"].as_str()?;
    let title: String = tool["name"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(name)
        .chars()
        .take(120)
        .collect();
    let fields = if tool["is_deprecated"] == true {
        Err("Composio is retiring this action.")
    } else {
        added::offered_fields(name, &tool["input_parameters"])
    };
    let says = reads(tool);
    let action = fields.as_ref().ok().map(|fields| AddedAction {
        id: added::action_id(app, slug, name),
        title: title.clone(),
        // Asking first is the safe answer when Composio doesn't say.
        effect: if says == Some(true) { "read" } else { "write" }.into(),
        tool: name.to_owned(),
        fields: fields.clone(),
    });
    let shown = json!({
        "tool": name, "title": title,
        "description": added::short(tool["description"].as_str().unwrap_or_default(), &title),
        "effect": action.as_ref().map(|a| a.effect.clone()),
        // An action Composio says changes something can't be marked as reading.
        "may_read": says != Some(false),
        "available": action.is_some(),
        "reason": fields.err(),
    });
    Some((shown, action))
}

/// One page of an app's actions to review: `{app, id, name, description,
/// total, actions: [{tool, title, description, effect, may_read, available,
/// reason}], next}`.
pub(super) async fn actions(
    api: &ComposioApi,
    app: &str,
    query: &str,
    cursor: Option<&str>,
) -> Result<Value> {
    let kit = toolkit(api, app).await?;
    let about = listed(&kit).context("The library has no such app")?;
    ensure!(about["state"] == "addable", "This app can't be added");
    let slug = kit["slug"].as_str().unwrap_or(app);
    let mut path = format!(
        "tools?toolkit_slug={}&limit={PAGE}",
        composio_api::id(slug)?
    );
    let query = word(query);
    if !query.is_empty() {
        path.push_str(&format!("&search={query}"));
    }
    if let Some(cursor) = cursor.filter(|c| !c.is_empty()) {
        // Composio's cursors are base64.
        ensure!(
            cursor.len() <= 200
                && cursor
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"=_-".contains(&b)),
            "Invalid page"
        );
        path.push_str(&format!("&cursor={}", cursor.replace('=', "%3D")));
    }
    let found = get(api, &path).await?.context("Composio has no such app")?;
    let id = about["id"].as_str().unwrap_or_default();
    let actions: Vec<Value> = found["items"]
        .as_array()
        .context("Composio omitted the app's actions")?
        .iter()
        .filter_map(|tool| reviewable(id, slug, tool).map(|(shown, _)| shown))
        .collect();
    Ok(json!({
        "app": slug, "id": id, "name": about["name"], "description": about["description"],
        "total": about["actions"], "actions": actions,
        "next": found["next_cursor"].as_str(),
    }))
}

/// The added app for the admin's choices, each tool read from Composio now.
pub(super) async fn build(api: &ComposioApi, app: &str, picks: &[LibraryPick]) -> Result<AddedApp> {
    ensure!(
        (1..=MAX_ACTIONS).contains(&picks.len()),
        "Choose 1 to {MAX_ACTIONS} actions"
    );
    let kit = toolkit(api, app).await?;
    let about = listed(&kit).context("The library has no such app")?;
    ensure!(
        about["state"] == "addable",
        "{}",
        about["reason"]
            .as_str()
            .unwrap_or("This app is already offered")
    );
    let slug = kit["slug"].as_str().unwrap_or(app).to_owned();
    let id = about["id"].as_str().unwrap_or_default().to_owned();
    let mut actions = vec![];
    for pick in picks {
        let tool = get(api, &format!("tools/{}", composio_api::id(&pick.tool)?))
            .await?
            .context("The library no longer has one of these actions")?;
        ensure!(
            tool["slug"] == pick.tool.as_str()
                && tool["toolkit"]["slug"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&slug)),
            "An action belongs to another app"
        );
        let (shown, action) = reviewable(&id, &slug, &tool).context("An action is invalid")?;
        let mut action = action.with_context(|| {
            format!(
                "\"{}\" can't be added: {}",
                shown["title"].as_str().unwrap_or_default(),
                shown["reason"].as_str().unwrap_or("it isn't available")
            )
        })?;
        ensure!(
            ["read", "write"].contains(&pick.effect.as_str()),
            "Choose Reads or Changes for each action"
        );
        ensure!(
            pick.effect == "write" || shown["may_read"] == true,
            "\"{}\" changes something, so it can't be marked as reading",
            action.title
        );
        action.effect = pick.effect.clone();
        actions.push(action);
    }
    let categories: Vec<&str> = kit["meta"]["categories"]
        .as_array()
        .map(|c| c.iter().filter_map(|c| c["name"].as_str()).collect())
        .unwrap_or_default();
    let added = AddedApp {
        id,
        name: about["name"].as_str().unwrap_or_default().to_owned(),
        description: about["description"].as_str().unwrap_or_default().to_owned(),
        category: added::category(&categories).into(),
        provider: NAME.into(),
        toolkit: slug,
        actions,
    };
    added.validate()?;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composios_hints_decide_what_may_be_marked_as_reading() {
        let tool = |tags: Value| {
            json!({"slug":"HUBSPOT_X","name":"X","tags":tags,
            "input_parameters":{"type":"object","properties":{"q":{"type":"string"}}}})
        };
        let (read, action) =
            reviewable("hubspot", "hubspot", &tool(json!(["readOnlyHint"]))).unwrap();
        assert_eq!(
            (read["effect"].as_str(), read["may_read"].as_bool()),
            (Some("read"), Some(true))
        );
        assert_eq!(action.unwrap().id, "hubspot.x");
        let (write, _) = reviewable("hubspot", "hubspot", &tool(json!(["updateHint"]))).unwrap();
        assert_eq!(
            (write["effect"].as_str(), write["may_read"].as_bool()),
            (Some("write"), Some(false))
        );
        // A read hint beside a change hint is a change.
        let (both, _) = reviewable(
            "hubspot",
            "hubspot",
            &tool(json!(["readOnlyHint", "createHint"])),
        )
        .unwrap();
        assert_eq!(both["may_read"], false);
        // Nothing said: asks first, and the admin may still call it a read.
        let (unsaid, _) = reviewable("hubspot", "hubspot", &tool(json!([]))).unwrap();
        assert_eq!(
            (unsaid["effect"].as_str(), unsaid["may_read"].as_bool()),
            (Some("write"), Some(true))
        );
        let mut retiring = tool(json!([]));
        retiring["is_deprecated"] = json!(true);
        let (gone, action) = reviewable("hubspot", "hubspot", &retiring).unwrap();
        assert!(gone["available"] == false && action.is_none());
    }

    #[test]
    fn only_apps_composio_signs_in_can_be_added_and_built_in_apps_are_named() {
        let kit = |slug: &str, managed: Value| {
            json!({"slug":slug,"name":"App","no_auth":false,
            "composio_managed_auth_schemes":managed,"meta":{"description":"What it is. More.","tools_count":3}})
        };
        assert_eq!(
            listed(&kit("hubspot", json!(["OAUTH2"]))).unwrap()["state"],
            "addable"
        );
        let keyed = listed(&kit("docker_hub", json!([]))).unwrap();
        assert!(
            keyed["state"] == "unsupported"
                && keyed["reason"].as_str().unwrap().contains("API key")
        );
        assert_eq!(
            listed(&kit("gmail", json!(["OAUTH2"]))).unwrap()["state"],
            "builtin"
        );
        assert_eq!(
            listed(&kit("googlesheets", json!(["OAUTH2"]))).unwrap()["state"],
            "builtin"
        );
        assert_eq!(word(" hub&x=1 spot "), "hubx1%20spot");
    }
}
