use super::{tests::*, *};
use crate::identity::{
    chat::{execution::prepare, model::Job},
    control::broker::invoke,
};
use serde_json::json;

async fn prepared(db: &PgStore, job: &Job) -> crate::identity::chat::execution::Prepared {
    prepare(db, job, 180).await.unwrap()
}
async fn task(db: &PgStore, owner: &User, w: &str, c: &str, a: &str, text: &str) -> Job {
    db.chat_send(owner, w, c, &message(text, Some(a)))
        .await
        .unwrap();
    db.claim_job().await.unwrap().unwrap()
}
fn brief(revision: i64, source: &str, text: &str) -> Value {
    json!({"revision":revision,"objective":{"text":text,"sources":[source]},"decisions":[],"questions":[],"corrections":[]})
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn versioned_context_persists_exact_attempt_reads_and_summary_with_privacy() {
    let db = fixture().await;
    let owner = user(&db, "context-owner").await;
    let other = user(&db, "context-other").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &other, "User").await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &other.id, &a, true)
        .await
        .unwrap();
    let other_chat = chat(&db, &other, &w, vec![], vec![a.clone()]).await;
    let job = task(&db, &owner, &w, &c, &a, "My private salary is 48000").await;
    let initial = prepared(&db, &job).await;
    assert_eq!(initial.model, "fixture");
    assert!(!initial.instructions.contains("Respect evidence"));
    let (identity, _) = db.request_context(&job).await.unwrap();
    let id = identity["request_message"].as_str().unwrap();
    invoke(
        &db,
        &job,
        "read_conversation_history",
        json!({"message":id}),
    )
    .await
    .unwrap();
    let saved = invoke(
        &db,
        &job,
        "save_working_context",
        brief(0, id, "Review my own salary"),
    )
    .await
    .unwrap();
    assert_eq!(saved["revision"], 1);
    assert!(
        invoke(
            &db,
            &job,
            "save_working_context",
            brief(1, "foreign-id", "leak")
        )
        .await
        .is_err()
    );
    // Exact retries do not change the first input or create a second version.
    let replay = prepared(&db, &job).await;
    assert_eq!(
        serde_json::to_value(&initial).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    let inspection = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert!(!inspection["attempts"].to_string().contains("48000"));
    let detail = db
        .agent_context_detail(
            &owner,
            &w,
            &a,
            &c,
            &job.reply,
            job.claim.as_deref().unwrap(),
        )
        .await
        .unwrap();
    assert!(detail.to_string().contains("48000"));
    assert!(
        db.agent_context_detail(
            &other,
            &w,
            &a,
            &other_chat,
            &job.reply,
            job.claim.as_deref().unwrap()
        )
        .await
        .is_err()
    );
    assert_eq!(detail["reads"].as_array().unwrap().len(), 1);
    assert_eq!(inspection["working"]["revision"], 1);
    assert!(db.agent_context_preview(&other, &w, &a, &c).await.is_err());
    let isolated = db
        .agent_context_preview(&other, &w, &a, &other_chat)
        .await
        .unwrap();
    assert!(!isolated.to_string().contains("48000"));
    assert_eq!(isolated["working"]["revision"], 0);
    assert!(
        execute(
            &db.db,
            "UPDATE relay_context_attempts SET prepared='{}' WHERE workspace=$1",
            vec![w.clone().into()]
        )
        .await
        .is_err()
    );
    db.chat_finish(&job, "Your private answer", "complete")
        .await
        .unwrap();
    let next = task(&db, &owner, &w, &c, &a, "Continue my review").await;
    let next_context = prepared(&db, &next).await;
    assert!(next_context.request.contains("Review my own salary"));
    assert_eq!(db.working_context(&next).await.unwrap()["revision"], 1);
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_correction_invalidates_summary_history_and_inflight_publication() {
    let db = fixture().await;
    let owner = user(&db, "correction-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    execute(&db.db,"UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{memory}','true'::jsonb) WHERE workspace=$1 AND id=$2",vec![w.clone().into(),a.clone().into()]).await.unwrap();
    let original = message("OLD_SOURCE_48000", None);
    db.chat_send(&owner, &w, &c, &original).await.unwrap();
    let mut note = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: Some(c.clone()),
        title: "Salary correction".into(),
        content: "OLD_PRIVATE_VALUE 48000".into(),
        source_message: Some(original.id.clone()),
        revision: 0,
        forgotten: false,
    };
    db.save_agent_knowledge(&owner, &w, &a, &note)
        .await
        .unwrap();
    let job = task(&db, &owner, &w, &c, &a, "Review the approved salary note").await;
    prepared(&db, &job).await;
    invoke(
        &db,
        &job,
        "save_working_context",
        brief(0, &note.id, "OLD_PRIVATE_VALUE 48000"),
    )
    .await
    .unwrap();
    note.revision = 0;
    note.content = "CORRECTED_VALUE 52000".into();
    db.save_agent_knowledge(&owner, &w, &a, &note)
        .await
        .unwrap();
    assert!(!db.chat_job_allowed(&job).await);
    assert!(db.recorded_context(&job).await.is_err());
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
    db.chat_finish(&job, "MUST_NOT_PUBLISH_OLD_PRIVATE_VALUE", "complete")
        .await
        .unwrap();
    let preview = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert_eq!(preview["working"]["available"], false);
    assert_eq!(preview["attempts"][0]["available"], false);
    assert!(!preview.to_string().contains("OLD_PRIVATE_VALUE"));
    assert!(
        !db.chat_messages(&owner, &w, &c)
            .await
            .unwrap()
            .to_string()
            .contains("MUST_NOT_PUBLISH")
    );
    let next = task(&db, &owner, &w, &c, &a, "Use the correction").await;
    let fresh = prepared(&db, &next).await;
    assert!(fresh.reference_notes.contains("CORRECTED_VALUE"));
    assert!(!fresh.request.contains("OLD_PRIVATE_VALUE"));
    assert!(!fresh.request.contains("OLD_SOURCE_48000"));
    let hidden = invoke(
        &db,
        &next,
        "read_conversation_history",
        json!({"message":original.id}),
    )
    .await
    .unwrap();
    assert_eq!(hidden["state"], "not_found");
    invoke(
        &db,
        &next,
        "save_working_context",
        brief(1, &note.id, "CORRECTED_VALUE 52000"),
    )
    .await
    .unwrap();
    note.revision = 1;
    note.forgotten = true;
    db.save_agent_knowledge(&owner, &w, &a, &note)
        .await
        .unwrap();
    let forgotten = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert!(!forgotten.to_string().contains("CORRECTED_VALUE"));
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_versions_fence_assembly_races_and_regrant() {
    let db = fixture().await;
    let owner = user(&db, "version-race-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let job = task(&db, &owner, &w, &c, &a, "Context before policy change").await;
    let initial = prepared(&db, &job).await;
    let generation = db.context_generation(&job).await.unwrap().unwrap();
    let tx = db.workspace_tx(&w).await.unwrap();
    execute(
        &tx,
        "UPDATE relay_chats SET visibility='public' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &tx,
        "UPDATE relay_chats SET visibility='private' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(
        db.save_context_attempt(&job, &generation, &initial, &Value::Null)
            .await
            .is_err()
    );
    let preview = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert_eq!(preview["attempts"][0]["available"], false);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_summary_concurrent_tasks_preserve_newest_evidence_and_revision() {
    let db = fixture().await;
    let owner = user(&db, "summary-race-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let older = task(&db, &owner, &w, &c, &a, "Old plan: Friday").await;
    prepared(&db, &older).await;
    let (old_identity, _) = db.request_context(&older).await.unwrap();
    let newer = task(&db, &owner, &w, &c, &a, "Correction: Monday, not Friday").await;
    prepared(&db, &newer).await;
    let (new_identity, _) = db.request_context(&newer).await.unwrap();
    let new_id = new_identity["request_message"].as_str().unwrap();
    let old_id = old_identity["request_message"].as_str().unwrap();
    let update = brief(0, new_id, "Corrected delivery: Monday");
    invoke(&db, &newer, "save_working_context", update.clone())
        .await
        .unwrap();
    let replay = invoke(&db, &newer, "save_working_context", update)
        .await
        .unwrap();
    assert_eq!(replay["revision"], 1);
    assert!(
        invoke(
            &db,
            &older,
            "save_working_context",
            brief(0, old_id, "Friday")
        )
        .await
        .is_err()
    );
    assert!(
        invoke(
            &db,
            &older,
            "save_working_context",
            brief(1, old_id, "Friday")
        )
        .await
        .is_err()
    );
    assert_eq!(
        db.working_context(&older).await.unwrap()["available"],
        false
    );
    assert!(
        db.agent_context_preview(&owner, &w, &a, &c).await.unwrap()["working"]
            .to_string()
            .contains("Monday")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_helper_note_revocation_fences_the_entire_derived_handoff_tree() {
    let s = super::handoff_tests::setup().await;
    execute(&s.db.db,"UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{memory}','true'::jsonb) WHERE workspace=$1 AND id=$2",vec![s.w.clone().into(),s.b.clone().into()]).await.unwrap();
    let mut note = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: Some(s.c.clone()),
        title: "Helper private source".into(),
        content: "HELPER_SOURCE_SECRET".into(),
        source_message: None,
        revision: 0,
        forgotten: false,
    };
    s.db.save_agent_knowledge(&s.owner, &s.w, &s.b, &note)
        .await
        .unwrap();
    prepared(&s.db, &s.parent).await;
    let (_, child) = s.handoff().await;
    let child = super::handoff_tests::claim(&s.db, &child).await;
    assert!(
        prepared(&s.db, &child)
            .await
            .reference_notes
            .contains("HELPER_SOURCE_SECRET")
    );
    assert!(
        super::source_access::allowed(&s.db.db, &s.parent)
            .await
            .unwrap()
    );
    note.forgotten = true;
    s.db.save_agent_knowledge(&s.owner, &s.w, &s.b, &note)
        .await
        .unwrap();
    assert!(
        !super::source_access::allowed(&s.db.db, &s.parent)
            .await
            .unwrap()
    );
    assert!(
        !super::source_access::allowed(&s.db.db, &child)
            .await
            .unwrap()
    );
    assert!(!s.db.chat_job_allowed(&s.parent).await);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn context_access_epochs_do_not_replace_independent_artifact_source_rights() {
    let db = fixture().await;
    let owner = user(&db, "artifact-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let job = task(&db, &owner, &w, &c, &a, "Prepare independent records").await;
    prepared(&db, &job).await;
    execute(
        &db.db,
        "UPDATE relay_chats SET visibility='public' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chats SET visibility='private' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap();
    assert!(
        !super::context_attempts::allowed(&db.db, &job)
            .await
            .unwrap()
    );
    assert!(
        super::context_attempts::knowledge_allowed(&db.db, &job)
            .await
            .unwrap()
    );
    // A changed source message still fences derived content even after access returns.
    assert!(db.request_identity(&job).await.is_err());
    execute(&db.db,"UPDATE relay_chat_messages SET content='Corrected source' WHERE workspace=$1 AND id=(SELECT request_message FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2)",vec![w.into(),job.reply.clone().into()]).await.unwrap();
    assert!(
        !super::context_attempts::knowledge_allowed(&db.db, &job)
            .await
            .unwrap()
    );
}
