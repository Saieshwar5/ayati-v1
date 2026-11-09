//! Schema 108 on screens: services' states and account facts on Apps, the Add
//! app catalog, one-step connect, states where agents meet apps, and company
//! rules meeting list fields.
use super::super::{tests::*, *};
use super::tests::{Setup, setup};
use super::{CatalogQuery, Offer};
use crate::identity::connectors::{connect::connect_app, fake_composio::MAILBOX};
use serde_json::json;

fn offer(id: &str, name: &str, category: &str) -> Offer {
    Offer {
        id: id.into(),
        name: name.into(),
        description: format!("Tools for {name}"),
        category: category.into(),
        operations: json!([{"id":format!("{id}.find"),"title":"Find","tool":"FIND","effect":"read"},
            {"id":format!("{id}.delete"),"title":"Delete a record","tool":"DELETE","effect":"write"}]),
    }
}

async fn page(
    s: &Setup,
    viewer: &User,
    offers: &[Offer],
    query: &str,
    category: &str,
    cursor: &str,
) -> Value {
    let input = CatalogQuery {
        query: query.into(),
        category: category.into(),
        cursor: cursor.into(),
    };
    s.db.connector_catalog(viewer, &s.w, offers, &input)
        .await
        .unwrap()
}

fn ids(page: &Value) -> Vec<String> {
    page["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn apps_name_each_services_state_and_who_connected_it_and_only_admins_see_the_provider() {
    let s = setup().await;
    s.connect().await;
    let member = user(&s.db, "routing-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    let admin = s.db.workspace_apps(&s.owner, &s.w).await.unwrap();
    let app = &admin["apps"][0];
    assert_eq!(
        (
            app["via"].as_str(),
            app["provider"].as_str(),
            app["account"].as_str(),
            app["state"].as_str()
        ),
        (
            Some("Composio"),
            Some("composio"),
            Some(MAILBOX),
            Some("ok")
        )
    );
    assert_eq!(app["connected"]["by_name"], "routing-owner@example.test");
    assert!(app["connected"]["at"].as_i64().unwrap() > 0 && app["paused_action"].is_null());
    for flag in [
        "sign_in",
        "revoke",
        "list_fields",
        "idempotency_key",
        "find_outcome",
        "triggers",
    ] {
        assert!(app["capabilities"][flag].is_boolean(), "{flag}");
    }
    let seen = s.db.workspace_apps(&member, &s.w).await.unwrap();
    let theirs = &seen["apps"][0];
    assert!(
        theirs.get("via").is_none()
            && theirs.get("provider").is_none()
            && theirs["account"].is_null()
    );
    assert_eq!(
        (theirs["state"].clone(), theirs["connected"].clone()),
        (json!("ok"), app["connected"].clone())
    );
    // An unconfirmed write pauses writes and names the action to check.
    s.grant("gmail.send_email", "allow").await;
    s.fake.state.lock().unwrap().fail_execute = true;
    let job = s.job("Send Sharma the balance").await;
    let sent = s
        .request(
            &job,
            "gmail.send_email",
            json!({"to":"sharma@example.test","subject":"Balance","body":"Due"}),
        )
        .await;
    assert_eq!(sent["state"], "uncertain", "{sent}");
    let state = |apps: Value| {
        (
            apps["apps"][0]["state"].clone(),
            apps["apps"][0]["paused_action"].clone(),
        )
    };
    assert_eq!(
        state(s.db.workspace_apps(&member, &s.w).await.unwrap()),
        (json!("writes_paused"), sent["id"].clone())
    );
    // A sign-in that stopped working comes before it, and Cuckoo's pause before both.
    s.db.connector_needs_reconnect(&s.w, &s.connection)
        .await
        .unwrap();
    assert_eq!(
        state(s.db.workspace_apps(&member, &s.w).await.unwrap()).0,
        "reconnect"
    );
    s.db.connector_provider_set(&s.owner, "composio", false, "Provider incident")
        .await
        .unwrap();
    assert_eq!(
        state(s.db.workspace_apps(&member, &s.w).await.unwrap()),
        (json!("paused"), Value::Null)
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_catalog_searches_filters_and_pages_with_connected_apps_first() {
    let s = setup().await;
    s.connect().await;
    let mut offers: Vec<Offer> = (0..40)
        .map(|n| {
            offer(
                &format!("app_{n:02}"),
                &format!("App {n:02}"),
                if n % 2 == 0 { "files" } else { "sales" },
            )
        })
        .collect();
    offers.push(offer("google_drive", "Google Drive", "files"));
    let gmail = json!([{"id":"gmail.find_emails","title":"Find emails","tool":"GMAIL_FETCH_EMAILS","effect":"read"}]);
    offers.push(Offer {
        operations: gmail,
        ..offer("gmail", "Gmail", "email_calendar")
    });
    let first = page(&s, &s.owner, &offers, "", "", "").await;
    assert_eq!(first["can_manage"], true);
    let listed = first["apps"].as_array().unwrap();
    assert_eq!(listed.len(), 30);
    assert_eq!(
        (listed[0]["id"].as_str(), listed[0]["state"].as_str()),
        (Some("gmail"), Some("connected"))
    );
    assert_eq!(listed[0]["connection"], s.connection.as_str());
    // A connected app counts its reviewed actions; the others the catalog's.
    assert_eq!(listed[0]["actions"], 3);
    assert_eq!(
        (listed[1]["id"].as_str(), listed[1]["kinds"].clone()),
        (Some("app_00"), json!(["read", "delete"]))
    );
    let counts: Vec<i64> = first["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["count"].as_i64().unwrap())
        .collect();
    assert_eq!(counts, [1, 21, 20, 0, 0, 0, 0, 0]);
    let cursor = first["next_cursor"].as_str().unwrap();
    let second = page(&s, &s.owner, &offers, "", "", cursor).await;
    assert!(second["next_cursor"].is_null());
    let mut all = ids(&first);
    all.extend(ids(&second));
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 42, "every app once across pages");
    // Search on name and description; a category narrows it.
    assert_eq!(
        ids(&page(&s, &s.owner, &offers, "DRIVE", "", "").await),
        ["google_drive"]
    );
    assert_eq!(
        ids(&page(&s, &s.owner, &offers, "tools for app 1", "", "").await).len(),
        10
    );
    let sales = page(&s, &s.owner, &offers, "app 1", "sales", "").await;
    assert_eq!(ids(&sales).len(), 5);
    assert_eq!(
        sales["categories"][2]["count"], 5,
        "counts follow the search"
    );
    let bad = |q: &str, c: &str, k: &str| CatalogQuery {
        query: q.into(),
        category: c.into(),
        cursor: k.into(),
    };
    for input in [
        bad("", "toys", ""),
        bad(&"x".repeat(101), "", ""),
        bad("", "", "2.app_01"),
        bad("", "", "1.gone"),
    ] {
        assert!(
            s.db.connector_catalog(&s.owner, &s.w, &offers, &input)
                .await
                .is_err()
        );
    }
    // A member asks for an app: it shows as requested to them, once.
    let member = user(&s.db, "catalog-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    let ask = |id: &str| AppRequest {
        id: id.into(),
        app: None,
        connector: Some("google_drive".into()),
        name: String::new(),
        url: String::new(),
        reason: "For invoices".into(),
    };
    let request = uuid::Uuid::new_v4().to_string();
    s.db.app_request(&member, &s.w, &ask(&request))
        .await
        .unwrap();
    assert_eq!(
        s.db.app_request(&member, &s.w, &ask(&request))
            .await
            .unwrap()["state"],
        "pending"
    );
    assert!(
        s.db.app_request(&member, &s.w, &ask(&uuid::Uuid::new_v4().to_string()))
            .await
            .is_err()
    );
    let theirs = page(&s, &member, &offers, "drive", "", "").await;
    assert_eq!(
        (
            theirs["apps"][0]["state"].as_str(),
            theirs["can_manage"].as_bool()
        ),
        (Some("requested"), Some(false))
    );
    assert_eq!(
        page(&s, &s.owner, &offers, "drive", "", "").await["apps"][0]["state"],
        "available"
    );
    let waiting = s.db.app_requests(&s.owner, &s.w).await.unwrap();
    assert_eq!(waiting["requests"][0]["connector"], "google_drive");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn one_step_connect_approves_the_app_answers_repeats_the_same_way_and_is_for_admins() {
    let s = setup().await;
    let origin = "http://127.0.0.1:1";
    let provider = s.provider.as_ref();
    let request = uuid::Uuid::new_v4().to_string();
    let first = connect_app(
        &s.db,
        origin,
        &s.owner,
        &s.w,
        "google_drive",
        &request,
        provider,
    )
    .await
    .unwrap();
    assert!(
        first["redirect_url"]
            .as_str()
            .unwrap()
            .starts_with("http://127.0.0.1")
    );
    let accounts = s.fake.state.lock().unwrap().accounts.len();
    let again = connect_app(
        &s.db,
        origin,
        &s.owner,
        &s.w,
        "google_drive",
        &request,
        provider,
    )
    .await
    .unwrap();
    assert_eq!(again, first);
    assert_eq!(
        s.fake.state.lock().unwrap().accounts.len(),
        accounts,
        "a repeat starts nothing"
    );
    let approved: bool = query(
        &s.db.db,
        "SELECT approved FROM relay_control_systems WHERE workspace=$1 AND server='google_drive'",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "approved")
    .unwrap();
    assert!(approved);
    // The same request for another app, and members, are refused.
    assert!(
        connect_app(&s.db, origin, &s.owner, &s.w, "gmail", &request, provider)
            .await
            .is_err()
    );
    let member = user(&s.db, "connect-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    let theirs = uuid::Uuid::new_v4().to_string();
    assert!(
        connect_app(&s.db, origin, &member, &s.w, "gmail", &theirs, provider)
            .await
            .is_err()
    );
    assert!(
        connect_app(
            &s.db,
            origin,
            &member,
            &s.w,
            "google_drive",
            &request,
            provider
        )
        .await
        .is_err()
    );
    // A failed attempt keeps no answer, so the same request runs again later.
    let retry = uuid::Uuid::new_v4().to_string();
    s.db.connector_provider_set(&s.owner, "composio", false, "Provider incident")
        .await
        .unwrap();
    assert!(
        connect_app(&s.db, origin, &s.owner, &s.w, "gmail", &retry, provider)
            .await
            .is_err()
    );
    s.db.connector_provider_set(&s.owner, "composio", true, "")
        .await
        .unwrap();
    let later = connect_app(&s.db, origin, &s.owner, &s.w, "gmail", &retry, provider)
        .await
        .unwrap();
    assert!(later["redirect_url"].is_string());
    // Signing in finishes like any connection, with Cuckoo's safe defaults.
    let account = later["redirect_url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    s.fake.sign_in(&account);
    let id: String = query(
        &s.db.db,
        "SELECT id FROM relay_connector_sign_ins WHERE workspace=$1 AND binding->>'account'=$2",
        vec![s.w.clone().into(), account.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "id")
    .unwrap();
    let sign_in = s.db.connector_sign_in(&s.owner, &id).await.unwrap();
    crate::identity::connectors::routes::finish_sign_in(&s.db, &s.owner, &id, sign_in, provider)
        .await
        .unwrap();
    let available = s.db.control_available(&s.owner, &s.w).await.unwrap();
    let modes: Vec<(String, String)> = available["connections"][0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["id"].as_str().unwrap().to_owned(),
                o["workspace_mode"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert!(modes.contains(&("gmail.find_emails".into(), "allow".into())));
    assert!(modes.contains(&("gmail.send_email".into(), "ask".into())));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_tools_show_each_connections_state() {
    let s = setup().await;
    s.connect().await;
    s.grant("gmail.find_emails", "allow").await;
    let available = s.db.control_available(&s.owner, &s.w).await.unwrap();
    assert_eq!(available["connections"][0]["state"], "ok");
    s.db.connector_needs_reconnect(&s.w, &s.connection)
        .await
        .unwrap();
    let available = s.db.control_available(&s.owner, &s.w).await.unwrap();
    assert_eq!(available["connections"][0]["state"], "reconnect");
    let tools =
        s.db.control_snapshot(&s.owner, &s.w, &s.agent)
            .await
            .unwrap();
    assert_eq!(
        (
            tools["connections"][0]["state"].as_str(),
            tools["connections"][0]["on"].as_bool()
        ),
        (Some("reconnect"), Some(true))
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn company_rules_never_match_a_list_and_still_check_plain_fields() {
    let s = setup().await;
    s.connect().await;
    s.grant("gmail.send_email", "allow").await;
    let rule = |field: &str, value: Value| super::super::control_actions::guard::GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: None,
        connection: None,
        meaning: Some("sends_outside".into()),
        source: None,
        kind: "field".into(),
        field: field.into(),
        comparator: "=".into(),
        value,
        effect: "block".into(),
        note: String::new(),
        enabled: true,
        revision: 0,
        trial: false,
    };
    s.db.guard_rule_save(&s.owner, &s.w, &rule("cc", json!(5)))
        .await
        .unwrap();
    s.db.guard_rule_save(&s.owner, &s.w, &rule("to", json!("rival@example.test")))
        .await
        .unwrap();
    let job = s.job("Send the balance").await;
    let copied = json!({"to":"sharma@example.test","cc":["accounts@example.test"],"subject":"Balance","body":"Due"});
    let sent = s.request(&job, "gmail.send_email", copied).await;
    assert_eq!(sent["state"], "completed", "{sent}");
    let rival = json!({"to":"rival@example.test","cc":[],"subject":"Hi","body":"Hello"});
    let held = s.request(&job, "gmail.send_email", rival).await;
    assert_ne!(held["state"], "completed", "{held}");
    assert_eq!(s.fake.state.lock().unwrap().executed.len(), 1);
}
