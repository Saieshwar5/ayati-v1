use super::{tests::*, *};
use crate::identity::agents::config::{SaveAgent, Settings};
use serde_json::json;

async fn settings(db: &PgStore, owner: &User, w: &str, a: &str, settings: Settings) {
    let profile = db.managed_agent(owner, w, a, true).await.unwrap();
    let input = SaveAgent {
        id: a.into(),
        name: profile["name"].as_str().unwrap().into(),
        role: profile["role"].as_str().unwrap().into(),
        model: "fixture".into(),
        scope: "workspace".into(),
        revision: profile["revision"].as_i64().unwrap(),
        settings,
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
}
fn note(chat: Option<&str>, content: &str) -> KnowledgeInput {
    KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        title: content.into(),
        content: content.into(),
        chat: chat.map(str::to_owned),
        source_message: None,
        revision: 0,
        forgotten: false,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_preview_enforces_person_workspace_agent_and_note_audiences_without_work() {
    let db = fixture().await;
    let owner = user(&db, "preview-owner").await;
    let member = user(&db, "preview-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &member.id, &a, true)
        .await
        .unwrap();
    let other = chat(&db, &member, &w, vec![], vec![a.clone()]).await;
    settings(
        &db,
        &owner,
        &w,
        &a,
        Settings {
            memory: true,
            ..Default::default()
        },
    )
    .await;
    db.update_profile(&owner, "Alex").await.unwrap();
    db.chat_send(&owner, &w, &c, &message("OWNER_PRIVATE_MESSAGE", None))
        .await
        .unwrap();
    db.chat_send(
        &member,
        &w,
        &other,
        &message("MEMBER_PRIVATE_MESSAGE", None),
    )
    .await
    .unwrap();
    for input in [
        note(Some(&c), "OWNER_PRIVATE_NOTE"),
        note(Some(&other), "MEMBER_PRIVATE_NOTE"),
        note(None, "REVIEWED_SHARED_NOTE"),
    ] {
        let actor = if input.chat.as_deref() == Some(&other) {
            // The note maintainer must also have conversation access; an admin
            // cannot insert into another person's private conversation.
            assert!(
                db.save_agent_knowledge(&owner, &w, &a, &input)
                    .await
                    .is_err()
            );
            continue;
        } else {
            &owner
        };
        db.save_agent_knowledge(actor, &w, &a, &input)
            .await
            .unwrap();
    }
    let preview = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert_eq!(preview["mode"], "current_preview");
    assert_eq!(preview["messages"][0]["author"]["display_name"], "Alex");
    assert!(preview.to_string().contains("OWNER_PRIVATE_NOTE"));
    assert!(!preview.to_string().contains("MEMBER_PRIVATE_MESSAGE"));
    let member_preview = db
        .agent_context_preview(&member, &w, &a, &other)
        .await
        .unwrap();
    assert!(member_preview.to_string().contains("REVIEWED_SHARED_NOTE"));
    assert!(!member_preview.to_string().contains("OWNER_PRIVATE"));
    assert!(db.agent_context_preview(&member, &w, &a, &c).await.is_err());
    assert!(
        db.agent_context_preview(&owner, &w, &a, &other)
            .await
            .is_err()
    );
    let foreign = workspace(&db, &member).await;
    assert!(
        db.agent_context_preview(&member, &foreign, &a, &other)
            .await
            .is_err()
    );
    let (_, second_agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    assert!(
        db.agent_context_preview(&owner, &w, &second_agent, &c)
            .await
            .is_err()
    );
    db.chat_grant(&owner, &w, &member.id, &a, false)
        .await
        .unwrap();
    assert!(
        db.agent_context_preview(&member, &w, &a, &other)
            .await
            .is_err()
    );
    assert_eq!(
        query(&db.db, "SELECT count(*) AS n FROM relay_chat_jobs", vec![])
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "n")
            .unwrap(),
        0
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_preview_uses_current_join_policy_and_shared_note_budget_and_forgetting() {
    let db = fixture().await;
    let owner = user(&db, "preview-policy-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let old = message("BEFORE_JOIN", None);
    db.chat_send(&owner, &w, &c, &old).await.unwrap();
    execute(&db.db, "UPDATE relay_chat_agents SET joined_sequence=(SELECT sequence FROM relay_chat_messages WHERE id=$3) WHERE workspace=$1 AND agent=$2", vec![w.clone().into(), a.clone().into(), old.id.into()]).await.unwrap();
    settings(
        &db,
        &owner,
        &w,
        &a,
        Settings {
            memory: true,
            history: "since_join".into(),
            paused: true,
            ..Default::default()
        },
    )
    .await;
    execute(&db.db, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) SELECT gen_random_uuid()::text,$1,$2,$3,'user','<img src=x onerror=alert(1)> '||n||repeat('x',500),'sent',$4 FROM generate_series(1,80) n", vec![w.clone().into(), c.clone().into(), owner.id.clone().into(), now().into()]).await.unwrap();
    let mut private = note(Some(&c), "APPROVED_PRIVATE_FACT");
    db.save_agent_knowledge(&owner, &w, &a, &private)
        .await
        .unwrap();
    let preview = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert_eq!(preview["paused"], true);
    assert!(!preview.to_string().contains("BEFORE_JOIN"));
    assert!(preview["messages"].as_array().unwrap().len() < 60);
    assert!(serde_json::to_vec(&preview["messages"]).unwrap().len() <= 14000);
    assert_eq!(preview["notes"][0]["id"], private.id);
    let actual = db.agent_context_notes(&owner, &w, &a, &c).await.unwrap();
    assert_eq!(
        actual,
        preview["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
    private.forgotten = true;
    db.save_agent_knowledge(&owner, &w, &a, &private)
        .await
        .unwrap();
    assert_eq!(
        db.agent_context_preview(&owner, &w, &a, &c).await.unwrap()["notes"],
        json!([])
    );
    db.save_agent_knowledge(&owner, &w, &a, &note(None, "SHARED_NOTE"))
        .await
        .unwrap();
    settings(&db, &owner, &w, &a, Settings::default()).await;
    assert_eq!(
        db.agent_context_preview(&owner, &w, &a, &c).await.unwrap()["notes"],
        json!([])
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_preview_excludes_source_dependent_answers() {
    let (db, owner, w, connection, task) = super::control_tests::setup("block").await;
    db.control_propose(&task, &super::control_tests::request(&connection, "one"))
        .await
        .unwrap();
    db.chat_finish(&task, "PRIVATE_SOURCE_DERIVED_ANSWER", "complete")
        .await
        .unwrap();
    let preview = db
        .agent_context_preview(&owner, &w, &task.agent, &task.chat)
        .await
        .unwrap();
    assert!(
        !preview
            .to_string()
            .contains("PRIVATE_SOURCE_DERIVED_ANSWER")
    );
    assert!(
        preview["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["id"] != task.reply)
    );
}
