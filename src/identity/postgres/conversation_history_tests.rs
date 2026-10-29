use super::{tests::*, *};
use crate::identity::{chat::model::Job, control::broker::invoke};
use serde_json::json;

async fn search(db: &PgStore, job: &Job, query: &str, before: Option<i64>) -> Value {
    invoke(
        db,
        job,
        "search_conversation_history",
        json!({"query":query,"before":before}),
    )
    .await
    .unwrap()
}
async fn read(db: &PgStore, job: &Job, id: &str, offset: i64) -> Value {
    invoke(
        db,
        job,
        "read_conversation_history",
        json!({"message":id,"offset":offset}),
    )
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_search_retrieves_older_evidence_with_bounded_complete_pagination() {
    let db = fixture().await;
    let owner = user(&db, "history-page-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let old = message(
        "OLDEST_EVIDENCE: delivery was Friday, literal %_ pattern",
        None,
    );
    db.chat_send(&owner, &w, &c, &old).await.unwrap();
    // Retained evidence exceeds both the 60-message recent window and one scan.
    execute(
        &db.db,
        "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at)
         SELECT gen_random_uuid()::text,$1,$2,$3,'user','ordinary history '||n,'sent',$4
         FROM generate_series(1,300) n",
        vec![
            w.clone().into(),
            c.clone().into(),
            owner.id.clone().into(),
            now().into(),
        ],
    )
    .await
    .unwrap();
    let queued = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message("Find the older delivery decision", Some(&a)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!queued.prompt.contains("OLDEST_EVIDENCE"));
    let job = db.claim_job().await.unwrap().unwrap();
    let first = search(&db, &job, "oldest_evidence", None).await;
    assert_eq!(first["messages"], json!([]));
    assert_eq!(first["scanned"], 256);
    let second = search(&db, &job, "oldest_evidence", first["next_before"].as_i64()).await;
    assert_eq!(second["messages"].as_array().unwrap().len(), 1);
    assert_eq!(second["messages"][0]["id"], old.id);
    assert_eq!(second["messages"][0]["author"]["id"], owner.id);
    assert_eq!(second["next_before"], Value::Null);
    let exact = read(&db, &job, &old.id, 0).await;
    assert_eq!(exact["message"]["content"], old.content);
    assert_eq!(exact["next_offset"], Value::Null);
    let literal = search(&db, &job, "%_", Some(i64::MAX)).await;
    assert_eq!(literal["messages"], json!([]));
    let literal = search(&db, &job, "%_", literal["next_before"].as_i64()).await;
    assert_eq!(literal["messages"].as_array().unwrap().len(), 1);
    let mut before = None;
    let mut ids = std::collections::HashSet::new();
    for _ in 0..20 {
        let page = search(&db, &job, "", before).await;
        let messages = page["messages"].as_array().unwrap();
        assert!(messages.len() <= 20);
        assert!(
            messages
                .windows(2)
                .all(|pair| pair[0]["sequence"].as_i64() < pair[1]["sequence"].as_i64())
        );
        for message in messages {
            assert!(ids.insert(message["id"].as_str().unwrap().to_owned()));
        }
        let next = page["next_before"].as_i64();
        if let (Some(prior), Some(next)) = (before, next) {
            assert!(next < prior);
        }
        before = next;
        if before.is_none() {
            break;
        }
    }
    assert!(before.is_none());
    assert_eq!(ids.len(), 302);
    assert!(ids.contains(&old.id));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_reads_are_unicode_chunked_and_never_expand_task_scope() {
    let db = fixture().await;
    let owner = user(&db, "history-isolation-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let other = chat(&db, &owner, &w, vec![], vec![a.clone()]).await;
    let secret = message("FOREIGN_CONVERSATION_SALARY", None);
    db.chat_send(&owner, &w, &other, &secret).await.unwrap();
    let original = message(&format!("{}🦀end", "α".repeat(4500)), None);
    db.chat_send(&owner, &w, &c, &original).await.unwrap();
    db.chat_send(&owner, &w, &c, &message("Read earlier text", Some(&a)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let later = message("LATER_TASK_DATA", None);
    db.chat_send(&owner, &w, &c, &later).await.unwrap();
    let first = read(&db, &job, &original.id, 0).await;
    assert_eq!(
        first["message"]["content"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        4000
    );
    assert_eq!(first["next_offset"], 4000);
    let rest = read(&db, &job, &original.id, 4000).await;
    assert_eq!(rest["next_offset"], Value::Null);
    assert_eq!(
        format!(
            "{}{}",
            first["message"]["content"].as_str().unwrap(),
            rest["message"]["content"].as_str().unwrap()
        ),
        original.content
    );
    assert!(read(&db, &job, &original.id, 4504).await["message"]["content"] == "");
    for id in [&secret.id, &later.id, &uuid::Uuid::new_v4().to_string()] {
        assert_eq!(read(&db, &job, id, 0).await["state"], "not_found");
    }
    assert_eq!(
        search(&db, &job, "FOREIGN_CONVERSATION", None).await["messages"],
        json!([])
    );
    assert_eq!(
        search(&db, &job, "LATER_TASK", Some(i64::MAX)).await["messages"],
        json!([])
    );
    for arguments in [
        json!({"query":"","chat":other}),
        json!({"query":"","requester":owner.id}),
        json!({"query":"","before":-1}),
        json!({"query":"x".repeat(257)}),
    ] {
        assert!(
            invoke(&db, &job, "search_conversation_history", arguments)
                .await
                .is_err()
        );
    }
    for arguments in [
        json!({"message":original.id,"workspace":w}),
        json!({"message":original.id,"offset":-1}),
        json!({"message":original.id,"offset":4505}),
    ] {
        assert!(
            invoke(&db, &job, "read_conversation_history", arguments)
                .await
                .is_err()
        );
    }
    let mut stale = job.clone();
    stale.claim = Some("old-claim".into());
    assert!(
        invoke(
            &db,
            &stale,
            "search_conversation_history",
            json!({"query":""})
        )
        .await
        .is_err()
    );
    let mut foreign = job.clone();
    foreign.chat = other;
    assert!(
        invoke(
            &db,
            &foreign,
            "read_conversation_history",
            json!({"message":secret.id})
        )
        .await
        .is_err()
    );
    db.chat_finish(&job, "Done", "complete").await.unwrap();
    assert!(
        invoke(
            &db,
            &job,
            "search_conversation_history",
            json!({"query":""})
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_rechecks_join_policy_membership_and_request_permission_without_writes() {
    let db = fixture().await;
    let owner = user(&db, "history-policy-owner").await;
    let member = user(&db, "history-policy-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.channel_members(&owner, &w, &c, &[owner.id.clone(), member.id.clone()])
        .await
        .unwrap();
    db.chat_grant(&owner, &w, &member.id, &a, true)
        .await
        .unwrap();
    let before_join = message("BEFORE_AGENT_JOIN", None);
    db.chat_send(&owner, &w, &c, &before_join).await.unwrap();
    execute(&db.db,"UPDATE relay_chat_agents SET joined_sequence=(SELECT sequence FROM relay_chat_messages WHERE id=$3) WHERE workspace=$1 AND chat=$2",vec![w.clone().into(),c.clone().into(),before_join.id.clone().into()]).await.unwrap();
    let current = message("AFTER_AGENT_JOIN", None);
    db.chat_send(&member, &w, &c, &current).await.unwrap();
    db.chat_send(&member, &w, &c, &message("Review history", Some(&a)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(
        read(&db, &job, &before_join.id, 0).await["state"],
        "available"
    );
    execute(&db.db,"UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{history}','\"since_join\"') WHERE workspace=$1 AND id=$2",vec![w.clone().into(),a.clone().into()]).await.unwrap();
    let saved = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Value>("", "data")
    .unwrap();
    assert_eq!(
        read(&db, &job, &before_join.id, 0).await["state"],
        "not_found"
    );
    assert_eq!(read(&db, &job, &current.id, 0).await["state"], "available");
    assert!(
        !search(&db, &job, "", None)
            .await
            .to_string()
            .contains("BEFORE_AGENT_JOIN")
    );
    assert_eq!(
        query(
            &db.db,
            "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE reply=$1",
            vec![job.reply.clone().into()]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<Value>("", "data")
        .unwrap(),
        saved
    );
    db.chat_grant(&owner, &w, &member.id, &a, false)
        .await
        .unwrap();
    assert!(
        invoke(
            &db,
            &job,
            "search_conversation_history",
            json!({"query":""})
        )
        .await
        .is_err()
    );
    db.chat_grant(&owner, &w, &member.id, &a, true)
        .await
        .unwrap();
    db.channel_members(&owner, &w, &c, std::slice::from_ref(&owner.id))
        .await
        .unwrap();
    assert!(
        invoke(
            &db,
            &job,
            "read_conversation_history",
            json!({"message":current.id})
        )
        .await
        .is_err()
    );
}
