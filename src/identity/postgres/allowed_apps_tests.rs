//! Schema 134: members connect only the apps an admin allowed, ask for the
//! others, and an admin's answer reaches everyone who asked.
use super::super::connection_owners::tests::{Setup, undecided};
use super::super::connector_routing::{CatalogQuery, Offer};
use super::super::*;
use serde_json::json;

fn offers() -> Vec<Offer> {
    [("gmail", "Gmail"), ("google_sheets", "Google Sheets"), ("google_drive", "Google Drive")]
        .into_iter()
        .map(|(id, name)| Offer {
            id: id.into(),
            name: name.into(),
            description: format!("Tools for {name}"),
            category: "files".into(),
            operations: json!([{"id":format!("{id}.find"),"title":"Find","tool":"FIND","effect":"read"}]),
        })
        .collect()
}

/// Each app's state in `agent`'s Add app catalog, as `viewer` sees it.
async fn states(s: &Setup, viewer: &User, agent: &str) -> Vec<(String, String)> {
    let input = CatalogQuery {
        query: String::new(),
        category: String::new(),
        cursor: String::new(),
        agent: Some(agent.into()),
    };
    s.db.connector_catalog(viewer, &s.w, &offers(), &input)
        .await
        .unwrap()["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["id"].as_str().unwrap().to_owned(),
                a["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn state_of(states: &[(String, String)], app: &str) -> String {
    states.iter().find(|(id, _)| id == app).unwrap().1.clone()
}

async fn ask(
    s: &Setup,
    who: &User,
    app: &str,
    agent: &str,
    reason: &str,
) -> anyhow::Result<String> {
    let input = AppRequest {
        id: uuid::Uuid::new_v4().to_string(),
        app: None,
        connector: Some(app.into()),
        agent: Some(agent.into()),
        name: String::new(),
        url: String::new(),
        reason: reason.into(),
    };
    s.db.app_request(who, &s.w, &input).await?;
    Ok(input.id)
}

async fn answer(
    s: &Setup,
    who: &User,
    id: &str,
    action: &str,
    note: &str,
) -> anyhow::Result<Value> {
    let input = AppDecision {
        action: action.into(),
        note: note.into(),
    };
    s.db.app_request_decide(who, &s.w, id, &input).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_member_asks_for_an_app_and_connects_it_once_an_admin_allows_it() {
    let s = undecided().await;
    // Nobody decided about Gmail: a member can't connect it, only ask.
    let refused = s.connect(&s.ravi, &s.sales).await.unwrap_err();
    assert!(refused.to_string().contains("Ask an admin to allow it"));
    assert_eq!(
        state_of(&states(&s, &s.ravi, &s.sales).await, "gmail"),
        "ask"
    );

    let asked = ask(&s, &s.ravi, "gmail", &s.sales, "Customer replies")
        .await
        .unwrap();
    assert_eq!(
        state_of(&states(&s, &s.ravi, &s.sales).await, "gmail"),
        "requested"
    );
    // Asking twice while it waits is refused, and a member can't ask for another's agent.
    assert!(ask(&s, &s.ravi, "gmail", &s.sales, "").await.is_err());
    assert!(ask(&s, &s.ravi, "google_sheets", &s.hr, "").await.is_err());
    // A second member waits for the same app.
    ask(&s, &s.priya, "gmail", &s.hr, "").await.unwrap();

    // The admin sees who asked, why and for which agent; members decide nothing.
    let waiting = s.db.app_requests(&s.admin, &s.w).await.unwrap();
    let first = waiting["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["agent_name"] == "Sales Desk")
        .unwrap();
    assert_eq!(
        (first["connector"].as_str(), first["reason"].as_str()),
        (Some("gmail"), Some("Customer replies"))
    );
    assert_eq!(waiting["requests"].as_array().unwrap().len(), 2);
    let ravis = s.db.app_requests(&s.ravi, &s.w).await.unwrap();
    assert!(ravis["requests"].as_array().unwrap().is_empty());
    assert_eq!(ravis["mine"][0]["state"], "pending");
    assert!(answer(&s, &s.ravi, &asked, "approve", "").await.is_err());

    // One yes allows the app for the workspace and answers everyone who asked.
    answer(&s, &s.admin, &asked, "approve", "Use your work account")
        .await
        .unwrap();
    let mine = s.db.app_requests(&s.ravi, &s.w).await.unwrap();
    assert_eq!(
        (
            mine["mine"][0]["state"].as_str(),
            mine["mine"][0]["note"].as_str()
        ),
        (Some("approved"), Some("Use your work account"))
    );
    assert_eq!(
        s.db.app_requests(&s.priya, &s.w).await.unwrap()["mine"][0]["state"],
        "approved"
    );
    assert!(
        s.db.app_requests(&s.admin, &s.w).await.unwrap()["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        state_of(&states(&s, &s.priya, &s.hr).await, "gmail"),
        "available"
    );
    // Deciding again keeps the first answer.
    assert_eq!(
        answer(&s, &s.admin, &asked, "decline", "").await.unwrap()["state"],
        "approved"
    );
    // Asking for an allowed app is refused: it can simply be added.
    assert!(
        ask(&s, &s.priya, "gmail", &s.hr, "")
            .await
            .unwrap_err()
            .to_string()
            .contains("already allowed")
    );

    let connection = s.connect(&s.ravi, &s.sales).await.unwrap();
    assert_eq!(s.stored(&connection).await["agent"], s.sales.as_str());
    assert_eq!(
        state_of(&states(&s, &s.ravi, &s.sales).await, "gmail"),
        "connected"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_declined_request_keeps_the_admins_note_and_the_app_stays_undecided() {
    let s = undecided().await;
    let asked = ask(&s, &s.ravi, "gmail", &s.sales, "").await.unwrap();
    assert!(
        answer(&s, &s.admin, &asked, "decline", &"x".repeat(301))
            .await
            .is_err()
    );
    answer(&s, &s.admin, &asked, "decline", "We use the shared inbox")
        .await
        .unwrap();
    let mine = s.db.app_requests(&s.ravi, &s.w).await.unwrap();
    assert_eq!(
        (
            mine["mine"][0]["state"].as_str(),
            mine["mine"][0]["note"].as_str()
        ),
        (Some("declined"), Some("We use the shared inbox"))
    );
    // Declining one request is not switching the app off: it can be asked for again.
    assert_eq!(
        state_of(&states(&s, &s.ravi, &s.sales).await, "gmail"),
        "ask"
    );
    assert!(s.connect(&s.ravi, &s.sales).await.is_err());
    ask(&s, &s.ravi, "gmail", &s.sales, "For the new client")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_admin_adding_an_app_allows_it_for_everyone() {
    let s = undecided().await;
    ask(&s, &s.priya, "gmail", &s.hr, "").await.unwrap();
    // Admins see every app as ready to add, whatever was decided.
    assert_eq!(
        state_of(&states(&s, &s.admin, &s.sales).await, "gmail"),
        "available"
    );
    s.connect(&s.admin, &s.sales).await.unwrap();
    assert_eq!(
        s.db.app_requests(&s.priya, &s.w).await.unwrap()["mine"][0]["state"],
        "approved"
    );
    // What can be added comes before what must be asked for.
    let order = states(&s, &s.priya, &s.hr).await;
    assert_eq!(order[0], ("gmail".to_owned(), "available".to_owned()));
    assert!(order[1..].iter().all(|(_, state)| state == "ask"));
    s.connect(&s.priya, &s.hr).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn switching_an_app_off_disconnects_every_agent_and_tells_members_why() {
    let s = undecided().await;
    assert_eq!(s.decide("gmail", true, 0, "").await.unwrap(), 0);
    let sales = s.connect(&s.ravi, &s.sales).await.unwrap();
    let hr = s.connect(&s.priya, &s.hr).await.unwrap();
    ask(&s, &s.ravi, "google_sheets", &s.sales, "")
        .await
        .unwrap();

    // Only admins decide, and a note stays short.
    let approval = crate::identity::control::connections_model::SystemApproval {
        server: "gmail".into(),
        approved: false,
        revision: 1,
    };
    assert!(
        s.db.control_system_noted(&s.ravi, &s.w, &approval, "")
            .await
            .is_err()
    );
    assert!(s.decide("gmail", false, 1, &"x".repeat(301)).await.is_err());
    // A change made from an old revision is refused.
    assert!(s.decide("gmail", false, 0, "").await.is_err());

    // Both agents' accounts end, and each is handed back to its provider.
    assert_eq!(
        s.decide("gmail", false, 1, "Mail stays in the shared inbox")
            .await
            .unwrap(),
        2
    );
    assert_eq!(s.stored(&sales).await["enabled"], false);
    assert_eq!(s.stored(&hr).await["enabled"], false);
    assert!(s.tools(&s.ravi, &s.sales).await.is_empty());
    let input = CatalogQuery {
        query: "gmail".into(),
        category: String::new(),
        cursor: String::new(),
        agent: Some(s.sales.clone()),
    };
    let seen =
        s.db.connector_catalog(&s.ravi, &s.w, &offers(), &input)
            .await
            .unwrap();
    assert_eq!(
        (
            seen["apps"][0]["state"].as_str(),
            seen["apps"][0]["note"].as_str()
        ),
        (Some("blocked"), Some("Mail stays in the shared inbox"))
    );
    assert!(
        s.connect(&s.ravi, &s.sales)
            .await
            .unwrap_err()
            .to_string()
            .contains("switched this app off")
    );
    assert!(
        ask(&s, &s.ravi, "gmail", &s.sales, "")
            .await
            .unwrap_err()
            .to_string()
            .contains("switched this app off")
    );

    // Switching off an app someone waits for answers them with the note.
    s.decide("google_sheets", false, 0, "Not for now")
        .await
        .unwrap();
    let mine = s.db.app_requests(&s.ravi, &s.w).await.unwrap();
    assert_eq!(
        (
            mine["mine"][0]["state"].as_str(),
            mine["mine"][0]["note"].as_str()
        ),
        (Some("declined"), Some("Not for now"))
    );

    // The admins' list names each decision, who made it and what it touches.
    assert!(s.db.allowed_apps(&s.ravi, &s.w, &offers()).await.is_err());
    let list = s.db.allowed_apps(&s.admin, &s.w, &offers()).await.unwrap();
    let apps = list["apps"].as_array().unwrap();
    let by = |id: &str| apps.iter().find(|a| a["id"] == id).unwrap().clone();
    assert_eq!(
        apps.iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Gmail", "Google Drive", "Google Sheets"]
    );
    assert_eq!(
        (
            by("gmail")["allowance"].as_str(),
            by("gmail")["note"].as_str(),
            by("gmail")["connections"].as_i64()
        ),
        (
            Some("blocked"),
            Some("Mail stays in the shared inbox"),
            Some(0)
        )
    );
    assert_eq!(
        by("gmail")["decided"]["by_name"],
        "owner-admin@example.test"
    );
    assert_eq!(by("google_drive")["allowance"], "undecided");
    assert!(by("google_drive")["decided"].is_null());

    // Allowing it again lets owners connect; the old accounts stay ended.
    s.decide("gmail", true, 2, "").await.unwrap();
    assert_eq!(s.stored(&sales).await["enabled"], false);
    s.connect(&s.ravi, &s.sales).await.unwrap();
}
