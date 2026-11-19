//! The library part of the Composio stand-in: three apps and one app's tools,
//! shaped like Composio's answers, for the added-apps tests.
use super::fake_composio::{Shared, VERSION, keyed};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/v3.1/toolkits", get(toolkits))
        .route("/api/v3.1/toolkits/{slug}", get(toolkit))
        .route("/api/v3.1/tools", get(tools))
}

fn kit(slug: &str) -> Option<Value> {
    let (name, managed, about, count) = match slug {
        "hubspot" => (
            "HubSpot",
            json!(["OAUTH2"]),
            "Customers, deals and marketing. Used by sales teams.",
            4,
        ),
        "docker_hub" => ("Docker Hub", json!([]), "Container images.", 12),
        "gmail" => ("Gmail", json!(["OAUTH2"]), "Email.", 61),
        _ => return None,
    };
    Some(
        json!({"slug":slug,"name":name,"no_auth":false,"composio_managed_auth_schemes":managed,
        "auth_schemes":["OAUTH2"],"meta":{"description":about,"tools_count":count,
        "categories":[{"name":"CRM","slug":"crm"}]}}),
    )
}

/// HubSpot's tools: (schema, tags). The third needs a list of records.
pub(super) fn hubspot(slug: &str) -> Option<(Value, Value)> {
    Some(match slug {
        "HUBSPOT_SEARCH_CONTACTS" => (
            json!({"type":"object","properties":{"query":{"type":"string","description":"Name or email"},"limit":{"type":"integer"},
                "filters":{"type":"array","items":{"type":"object"}}},"required":["query"]}),
            json!(["readOnlyHint", "openWorldHint"]),
        ),
        "HUBSPOT_CREATE_NOTE" => (
            json!({"type":"object","properties":{"body":{"type":"string"},"contact_id":{"type":"string"},"pinned":{"type":"boolean"}},"required":["body"]}),
            json!(["createHint"]),
        ),
        "HUBSPOT_CREATE_CONTACTS" => (
            json!({"type":"object","properties":{"records":{"type":"array","items":{"type":"object"}}},"required":["records"]}),
            json!(["createHint"]),
        ),
        "HUBSPOT_PING" => (json!({"type":"object","properties":{}}), json!([])),
        _ => return None,
    })
}
const HUBSPOT: [&str; 4] = [
    "HUBSPOT_CREATE_CONTACTS",
    "HUBSPOT_CREATE_NOTE",
    "HUBSPOT_PING",
    "HUBSPOT_SEARCH_CONTACTS",
];

/// A HubSpot tool as Composio lists and returns it.
pub(super) fn described(slug: &str, version: &str) -> Option<Value> {
    let (input, tags) = hubspot(slug)?;
    let name = slug
        .trim_start_matches("HUBSPOT_")
        .replace('_', " ")
        .to_lowercase();
    Some(
        json!({"slug":slug,"name":name,"description":format!("Does {name}. In HubSpot."),
        "toolkit":{"slug":"hubspot","name":"hubspot","logo":""},"input_parameters":input,"output_parameters":{},
        "no_auth":false,"available_versions":[VERSION],"version":version,"scopes":[],"tags":tags,"is_deprecated":false}),
    )
}

async fn toolkits(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let search = q.get("search").cloned().unwrap_or_default().to_lowercase();
    let items: Vec<Value> = ["docker_hub", "gmail", "hubspot"]
        .iter()
        .filter(|slug| slug.contains(&search))
        .filter_map(|slug| kit(slug))
        .collect();
    Ok(Json(
        json!({"items":items,"next_cursor":null,"total_items":items.len()}),
    ))
}

async fn toolkit(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    kit(&slug).map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// Two tools a page, so paging is exercised.
async fn tools(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    if q.get("toolkit_slug").map(String::as_str) != Some("hubspot") {
        return Ok(Json(json!({"items":[],"next_cursor":null,"total_items":0})));
    }
    let search = q.get("search").cloned().unwrap_or_default().to_uppercase();
    let found: Vec<&str> = HUBSPOT
        .iter()
        .copied()
        .filter(|t| t.contains(&search))
        .collect();
    let start = if q.get("cursor").map(String::as_str) == Some("Mi0y") {
        2
    } else {
        0
    };
    let items: Vec<Value> = found
        .iter()
        .skip(start)
        .take(2)
        .filter_map(|t| described(t, VERSION))
        .collect();
    let next = (start == 0 && found.len() > 2).then_some("Mi0y");
    Ok(Json(
        json!({"items":items,"next_cursor":next,"total_items":found.len()}),
    ))
}
