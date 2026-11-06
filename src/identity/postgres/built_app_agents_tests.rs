//! An agent builds an app's draft only after its builder asks, and only in the builder's
//! own requests; it can't publish, and each file it writes is audited.
use super::super::super::executor_slot_tests;
use super::super::super::tests::{chat, fixture, join, message, user, workspace};
use super::super::NewApp;
use super::Helper;
use crate::identity::control::broker::invoke;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_agent_writes_the_draft_only_for_the_builder_who_asked() {
    let db = fixture().await;
    let owner = user(&db, "app-agent-owner").await;
    let ravi = user(&db, "app-agent-ravi").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &ravi, "Member").await;
    let (_, agent) = executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &ravi.id, &agent, true)
        .await
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let app = NewApp {
        id: id.clone(),
        name: "Stock count".into(),
        description: "Count the shelves".into(),
        audience: "everyone".into(),
        people: vec![],
        template: String::new(),
    };
    db.built_app_create(&ravi, &w, &app).await.unwrap();
    let ask = |person: crate::identity::User, text: &'static str| {
        let db = &db;
        let w = w.clone();
        let agent = agent.clone();
        async move {
            let conversation = chat(db, &person, &w, vec![], vec![agent.clone()]).await;
            db.chat_send(&person, &w, &conversation, &message(text, Some(&agent)))
                .await
                .unwrap();
            db.claim_job().await.unwrap().unwrap()
        }
    };
    // Before Ravi asks, the agent has no apps and can't touch this one.
    let job = ask(ravi.clone(), "Build my stock app").await;
    assert!(!db.built_app_agent_helps(&job).await.unwrap());
    assert_eq!(
        invoke(&db, &job, "my_apps", json!({})).await.unwrap()["apps"],
        json!([])
    );
    assert_eq!(
        invoke(&db, &job, "app_files", json!({"app":id}))
            .await
            .unwrap()["state"],
        "not_allowed"
    );
    db.chat_finish(&job, "Not yet.", "complete").await.unwrap();
    // Only the builder asks an agent; the owner (an admin) can't for Ravi's app.
    let helps = Helper {
        agent: agent.clone(),
        helps: true,
    };
    assert!(db.built_app_helper(&owner, &w, &id, &helps).await.is_err());
    db.built_app_helper(&ravi, &w, &id, &helps).await.unwrap();
    let view = db.workspace_apps(&ravi, &w).await.unwrap();
    assert_eq!(view["built"][0]["helpers"][0]["id"], agent.as_str());
    let job = ask(ravi.clone(), "Build my stock app").await;
    assert!(db.built_app_agent_helps(&job).await.unwrap());
    let mine = invoke(&db, &job, "my_apps", json!({})).await.unwrap();
    assert_eq!(mine["apps"][0]["name"], "Stock count");
    let draft = invoke(&db, &job, "app_files", json!({"app":id}))
        .await
        .unwrap();
    assert_eq!(draft["files"].as_array().unwrap().len(), 5);
    assert_eq!(draft["checks"], json!([]));
    let read = invoke(
        &db,
        &job,
        "read_app_file",
        json!({"app":id,"path":"index.html"}),
    )
    .await
    .unwrap();
    assert!(
        read["content"]
            .as_str()
            .unwrap()
            .contains("<h1>Stock count</h1>")
    );
    // It writes from the revision it read; a stale one is refused in plain words.
    let wrote = invoke(
        &db,
        &job,
        "write_app_file",
        json!({"app":id,"path":"app.js","content":"cuckoo.me();","revision":1}),
    )
    .await
    .unwrap();
    assert_eq!(wrote["state"], "saved");
    assert_eq!(wrote["draft_revision"], 2);
    let stale = invoke(
        &db,
        &job,
        "write_app_file",
        json!({"app":id,"path":"app.js","content":"x","revision":1}),
    )
    .await
    .unwrap();
    assert_eq!(stale["state"], "changed");
    // A key in its code shows in the checks it gets back.
    let keyed = invoke(&db, &job, "write_app_file", json!({"app":id,"path":"config.js","content":"const k='sk-abcdefghijklmnopqrstuvwxyz0123456789';","revision":2})).await.unwrap();
    assert_eq!(keyed["checks"][0]["check"], "secrets");
    assert!(
        invoke(
            &db,
            &job,
            "write_app_file",
            json!({"app":id,"path":"../x","content":"x","revision":3})
        )
        .await
        .is_err()
    );
    db.chat_finish(&job, "Done.", "complete").await.unwrap();
    assert_eq!(
        db.built_app_files(&ravi, &w, &id, 0).await.unwrap()["draft_revision"],
        3
    );
    // In someone else's request, the same agent can't reach Ravi's app.
    let job = ask(owner.clone(), "Change Ravi's app").await;
    assert_eq!(
        invoke(
            &db,
            &job,
            "read_app_file",
            json!({"app":id,"path":"app.js"})
        )
        .await
        .unwrap()["state"],
        "not_allowed"
    );
    db.chat_finish(&job, "No.", "complete").await.unwrap();
    // Stopping the agent closes the draft to it again; the writes were audited.
    db.built_app_helper(
        &ravi,
        &w,
        &id,
        &Helper {
            agent: agent.clone(),
            helps: false,
        },
    )
    .await
    .unwrap();
    let job = ask(ravi.clone(), "One more change").await;
    assert_eq!(
        invoke(&db, &job, "app_files", json!({"app":id}))
            .await
            .unwrap()["state"],
        "not_allowed"
    );
    let audit: Vec<String> = crate::identity::postgres::rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND summary IS NOT NULL",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get::<String>("", "summary").unwrap())
    .collect();
    let said = audit.join("\n");
    assert!(said.contains("changed app.js in \"Stock count\""), "{said}");
    assert!(said.contains("to build \"Stock count\""), "{said}");
}
