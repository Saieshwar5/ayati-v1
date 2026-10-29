//! AI setup suggestions run as executor tasks and only ever fill in a
//! reviewable draft; they never activate permissions.
use super::{executor_task_tests::claim, *};
use crate::identity::control::{
    setup_assistant,
    setup_model::{Draft, Edit, Suggestion},
};

/// A saved draft with every action blocked and suggestions queued.
async fn suggested() -> (PgStore, User, String, String, Draft) {
    let (db, owner, w, agent, _, mut draft) = super::setup_tests::setup().await;
    draft.request.assistant = true;
    for choice in &mut draft.choices {
        choice.mode = "block".into();
    }
    draft.suggestion = Some(Suggestion {
        state: "pending".into(),
        detail: "Preparing".into(),
    });
    let context = setup_assistant::context(&draft, &w, &agent);
    let saved = db
        .setup_create_suggested(&owner, &w, &agent, &draft, &context)
        .await
        .unwrap();
    assert_eq!(saved["data"]["suggestion"]["state"], "pending");
    assert!(saved["data"].get("suggestion_context").is_none());
    (db, owner, w, agent, draft)
}
fn read_and_ask(draft: &Draft) -> String {
    let id = &draft.choices[0].connection;
    serde_json::json!({"rules":[{"connection":id,"operation":"read","mode":"allow"},{"connection":id,"operation":"update","mode":"ask"}],"warnings":["Fixture warning"]}).to_string()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_executor_suggestion_fills_the_blocked_draft_for_review_without_activating() {
    let (db, owner, w, agent, draft) = suggested().await;
    let run = claim(&db).await.unwrap();
    assert_eq!(run.id(), draft.request.id);
    assert!(!run.context.control_tools && run.context.request.starts_with("permission_draft "));
    assert!(run.renew().await);
    assert!(run.finish(&read_and_ask(&draft)).await);
    let saved = db
        .setup_draft(&owner, &w, &agent, &draft.request.id)
        .await
        .unwrap();
    assert_eq!(saved["revision"], 2);
    assert_eq!(saved["state"], "draft");
    assert_eq!(saved["data"]["suggestion"]["state"], "ready");
    assert_eq!(saved["data"]["choices"][0]["mode"], "allow");
    assert_eq!(saved["data"]["choices"][1]["mode"], "ask");
    assert!(
        query(
            &db.db,
            "SELECT id FROM relay_control_drafts WHERE id=$1 AND suggestion_context IS NULL",
            vec![draft.request.id.clone().into()]
        )
        .await
        .unwrap()
        .is_some()
    );
    assert!(
        !run.finish("Late replacement").await,
        "a different late result is refused"
    );
    assert!(
        claim(&db).await.is_none(),
        "a finished suggestion is never rerun"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn edits_win_over_late_suggestions_and_bad_or_lost_runs_leave_the_draft_blocked() {
    let (db, owner, w, agent, draft) = suggested().await;
    let run = claim(&db).await.unwrap();
    let mut choices = draft.choices.clone();
    choices[0].mode = "allow".into();
    db.setup_edit(
        &owner,
        &w,
        &agent,
        &draft.request.id,
        &Edit {
            revision: 1,
            choices,
        },
    )
    .await
    .unwrap();
    assert!(run.finish(&read_and_ask(&draft)).await);
    let saved = db
        .setup_draft(&owner, &w, &agent, &draft.request.id)
        .await
        .unwrap();
    assert_eq!(saved["data"]["suggestion"]["state"], "discarded");
    assert_eq!(
        saved["revision"], 2,
        "the requester's own revision still applies"
    );
    assert_eq!(saved["data"]["choices"][1]["mode"], "block");

    let (db, owner, w, agent, draft) = suggested().await;
    let run = claim(&db).await.unwrap();
    assert!(run.finish("not json").await);
    let saved = db
        .setup_draft(&owner, &w, &agent, &draft.request.id)
        .await
        .unwrap();
    assert_eq!(saved["data"]["suggestion"]["state"], "failed");
    assert!(
        saved["data"]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["mode"] == "block")
    );

    let (db, owner, w, agent, draft) = suggested().await;
    let run = claim(&db).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_executor_tasks SET lease_until=0 WHERE id=$1",
        vec![draft.request.id.clone().into()],
    )
    .await
    .unwrap();
    assert!(!run.renew().await);
    assert!(
        claim(&db).await.is_none(),
        "recovery never reruns the suggestion"
    );
    let saved = db
        .setup_draft(&owner, &w, &agent, &draft.request.id)
        .await
        .unwrap();
    assert_eq!(saved["data"]["suggestion"]["state"], "failed");
    assert!(!run.finish(&read_and_ask(&draft)).await);
}
