//! Schema 135: an admin adds an app from the library, people connect and use
//! it like a built-in app, and changing or removing it ends its connections.
use super::super::super::connection_owners::tests::{Setup, undecided};
use super::super::super::{tests::*, *};
use crate::identity::connectors::{
    self, LibraryPick, Outcome, Refusal, Saved, added::AddedApp, connect::connect_app_for,
};
use crate::identity::control::model::Operation;
use serde_json::json;

fn pick(tool: &str, effect: &str) -> LibraryPick {
    LibraryPick {
        tool: tool.into(),
        effect: effect.into(),
    }
}

/// HubSpot with one read and one write, as the admin reviewed it.
async fn hubspot(s: &Setup) -> AddedApp {
    let picks = [
        pick("HUBSPOT_SEARCH_CONTACTS", "read"),
        pick("HUBSPOT_CREATE_NOTE", "write"),
    ];
    s.provider.library_app("hubspot", &picks).await.unwrap()
}

/// Connects `app` for `agent` through the workspace's own provider.
async fn connect(s: &Setup, who: &User, agent: &str, app: &str) -> anyhow::Result<String> {
    let added = s.db.added_apps(&s.w).await?;
    let provider = connectors::composio_with_added(&s.fake, &added);
    let request = uuid::Uuid::new_v4().to_string();
    connect_app_for(
        &s.db,
        "http://127.0.0.1:1",
        who,
        &s.w,
        app,
        &request,
        Some(agent),
        provider.as_ref(),
    )
    .await?;
    let row = query(
        &s.db.db,
        "SELECT id,connection,binding->>'account' AS account FROM relay_connector_sign_ins WHERE workspace=$1 AND agent=$2 AND state='waiting'",
        vec![s.w.clone().into(), agent.into()],
    )
    .await?
    .expect("a sign-in is waiting");
    let id: String = row.try_get("", "id")?;
    s.fake.sign_in(&row.try_get::<String>("", "account")?);
    let sign_in = s.db.connector_sign_in(who, &id).await?;
    connectors::routes::finish_sign_in(&s.db, who, &id, sign_in, provider.as_ref()).await?;
    Ok(row.try_get("", "connection")?)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_library_lists_apps_and_actions_and_builds_only_what_was_reviewed() {
    let s = undecided().await;
    let found = s.provider.library("hub").await.unwrap();
    let state = |id: &str| {
        found["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(
        (state("hubspot"), state("docker_hub")),
        (json!("addable"), json!("unsupported"))
    );
    assert_eq!(
        s.provider.library("gmail").await.unwrap()["apps"][0]["state"],
        "builtin"
    );
    // Actions come a page at a time, each with what Cuckoo can offer.
    let first = s
        .provider
        .library_actions("hubspot", "", None)
        .await
        .unwrap();
    assert_eq!(
        (
            first["name"].as_str(),
            first["actions"].as_array().unwrap().len()
        ),
        (Some("HubSpot"), 2)
    );
    let bulk = &first["actions"][0];
    assert!(
        bulk["available"] == false && bulk["reason"].as_str().unwrap().contains("does not offer")
    );
    assert_eq!(
        (
            first["actions"][1]["effect"].as_str(),
            first["actions"][1]["may_read"].as_bool()
        ),
        (Some("write"), Some(false))
    );
    let second = s
        .provider
        .library_actions("hubspot", "", first["next"].as_str())
        .await
        .unwrap();
    assert_eq!(second["actions"][1]["effect"], "read");
    assert!(second["next"].is_null());
    assert!(
        s.provider
            .library_actions("docker_hub", "", None)
            .await
            .is_err()
    );

    let app = hubspot(&s).await;
    assert_eq!(
        (
            app.id.as_str(),
            app.category.as_str(),
            app.description.as_str()
        ),
        ("hubspot", "sales", "Customers, deals and marketing")
    );
    assert_eq!(app.actions[0].fields, vec!["query", "limit"]);
    assert_eq!(app.actions[1].id, "hubspot.create_note");
    // A change can't be called a read; an unavailable or foreign action is refused.
    for picks in [
        vec![pick("HUBSPOT_CREATE_NOTE", "read")],
        vec![pick("HUBSPOT_CREATE_CONTACTS", "write")],
        vec![pick("GMAIL_SEND_EMAIL", "write")],
        vec![pick("HUBSPOT_PING", "sometimes")],
        vec![],
    ] {
        assert!(s.provider.library_app("hubspot", &picks).await.is_err());
    }
    // Composio doesn't say what the ping does: the admin may call it a read.
    s.provider
        .library_app("hubspot", &[pick("HUBSPOT_PING", "read")])
        .await
        .unwrap();
    assert!(
        s.provider
            .library_app("gmail", &[pick("GMAIL_SEND_EMAIL", "write")])
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_added_app_is_allowed_connected_and_used_like_a_built_in_one() {
    let s = undecided().await;
    let app = hubspot(&s).await;
    // Only an admin adds an app; adding it allows it.
    assert!(s.db.added_app_save(&s.ravi, &s.w, &app, 0).await.is_err());
    assert!(connect(&s, &s.ravi, &s.sales, "hubspot").await.is_err());
    s.db.added_app_save(&s.admin, &s.w, &app, 0).await.unwrap();
    assert_eq!(s.db.added_apps(&s.w).await.unwrap(), vec![app.clone()]);
    let facts = s.db.added_apps_facts(&s.w).await.unwrap();
    assert_eq!(
        (
            facts["hubspot"]["revision"].as_i64(),
            facts["hubspot"]["actions"].as_array().unwrap().len()
        ),
        (Some(1), 2)
    );
    // Saving the same thing again is the same answer, not a second change.
    assert!(
        s.db.added_app_save(&s.admin, &s.w, &app, 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.db.added_apps_facts(&s.w).await.unwrap()["hubspot"]["revision"],
        1
    );

    // A member connects it for their own agent.
    let connection = connect(&s, &s.ravi, &s.sales, "hubspot").await.unwrap();
    let saved = s.stored(&connection).await;
    assert_eq!(
        (
            saved["server"].as_str(),
            saved["binding"]["toolkit"].as_str()
        ),
        (Some("hubspot"), Some("hubspot"))
    );
    let operations: Vec<Operation> =
        serde_json::from_value(saved["definition"]["operations"].clone()).unwrap();
    assert_eq!(
        operations
            .iter()
            .map(|o| (o.id.as_str(), o.effect.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("hubspot.search_contacts", "read"),
            ("hubspot.create_note", "write")
        ]
    );
    // The nested filter field is not offered; the plain ones are.
    assert_eq!(
        operations[0].schema["properties"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    let tools = s.tools(&s.ravi, &s.sales).await;
    let modes: Vec<&str> = tools[0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["mode"].as_str().unwrap())
        .collect();
    assert_eq!(
        modes,
        vec!["allow", "ask"],
        "reads are allowed and changes ask first"
    );

    // A saved connection runs through the provider as any process has it,
    // without the workspace's list of added apps.
    let stored = Saved {
        workspace: &s.w,
        server: "hubspot",
        binding: Some(&saved["binding"]),
        fingerprint: saved["fingerprint"].as_str().unwrap(),
    };
    let call = s.provider.prepare(&stored, &operations[1]).await.unwrap();
    let outcome = call
        .run(&json!({"body":"Called back","pinned":true}), "key")
        .await;
    assert!(matches!(outcome, Outcome::Completed(_)));
    assert_eq!(
        s.fake.state.lock().unwrap().executed.last().unwrap(),
        &(
            "HUBSPOT_CREATE_NOTE".to_owned(),
            json!({"body":"Called back","pinned":true})
        )
    );
    let described = s.provider.describe(&stored, &operations[1]).await.unwrap();
    assert_eq!(described["schema"], operations[1].schema);
    // A tool that changed since the review is refused before anything is sent.
    s.fake.state.lock().unwrap().changed = true;
    let refused = s
        .provider
        .prepare(&stored, &operations[1])
        .await
        .err()
        .unwrap();
    assert!(matches!(
        refused.downcast_ref::<Refusal>(),
        Some(Refusal::Changed)
    ));
    s.fake.state.lock().unwrap().changed = false;
    // A connection of an added app without its toolkit is never guessed.
    let bare = json!({"account":saved["binding"]["account"],"auth_config":saved["binding"]["auth_config"],"user":saved["binding"]["user"]});
    let guessed = Saved {
        binding: Some(&bare),
        ..stored
    };
    assert!(s.provider.prepare(&guessed, &operations[1]).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn changing_or_removing_an_added_app_ends_its_connections() {
    let s = undecided().await;
    let app = hubspot(&s).await;
    s.db.added_app_save(&s.admin, &s.w, &app, 0).await.unwrap();
    let first = connect(&s, &s.ravi, &s.sales, "hubspot").await.unwrap();
    let second = connect(&s, &s.priya, &s.hr, "hubspot").await.unwrap();

    // A stale page can't change it, and a member can't at all.
    let fewer = s
        .provider
        .library_app("hubspot", &[pick("HUBSPOT_SEARCH_CONTACTS", "read")])
        .await
        .unwrap();
    assert!(
        s.db.added_app_save(&s.admin, &s.w, &fewer, 7)
            .await
            .is_err()
    );
    assert!(
        s.db.added_app_save(&s.priya, &s.w, &fewer, 1)
            .await
            .is_err()
    );
    // Changing the actions ends both agents' connections; their owners connect again.
    let released =
        s.db.added_app_save(&s.admin, &s.w, &fewer, 1)
            .await
            .unwrap();
    assert_eq!(released.len(), 2);
    for id in [&first, &second] {
        assert_eq!(s.stored(id).await["enabled"], false);
    }
    assert!(s.tools(&s.ravi, &s.sales).await.is_empty());
    let again = connect(&s, &s.ravi, &s.sales, "hubspot").await.unwrap();
    assert_eq!(
        s.stored(&again).await["definition"]["operations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Removing it ends the connection and it is no longer offered or connectable.
    assert!(
        s.db.added_app_remove(&s.ravi, &s.w, "hubspot", 2)
            .await
            .is_err()
    );
    assert!(
        s.db.added_app_remove(&s.admin, &s.w, "hubspot", 1)
            .await
            .is_err()
    );
    assert_eq!(
        s.db.added_app_remove(&s.admin, &s.w, "hubspot", 2)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(s.db.added_apps(&s.w).await.unwrap().is_empty());
    assert!(connect(&s, &s.admin, &s.sales, "hubspot").await.is_err());
    assert!(
        s.db.added_app_remove(&s.admin, &s.w, "hubspot", 2)
            .await
            .unwrap()
            .is_empty()
    );
    // Adding it again starts from a clean review.
    s.db.added_app_save(&s.admin, &s.w, &app, 0).await.unwrap();
    assert_eq!(
        s.db.added_apps_facts(&s.w).await.unwrap()["hubspot"]["revision"],
        1
    );
    // Another workspace never sees it.
    let other = workspace(&s.db, &s.ravi).await;
    assert!(s.db.added_apps(&other).await.unwrap().is_empty());
}
