//! A task's run happens in its work room but its result is posted to the task's
//! conversation (schema 145), so an agent may use a "viewers" document in a run only
//! when everyone in that conversation may see it too (schema 150).
use super::super::{tests::*, *};
use super::folders::setup;
use crate::identity::control::broker::invoke;
use serde_json::json;

/// A run of a new task of the owner's, working in its own room and posting to `chat`.
async fn run(s: &super::folders::Setup, chat: Option<&str>) -> crate::identity::chat::model::Job {
    let tx = s.db.workspace_tx(&s.w).await.unwrap();
    let room = super::super::work_rooms::open(
        &tx,
        &s.w,
        "Salary summary",
        "Runs of a task",
        &s.owner.id,
        &s.agent,
    )
    .await
    .unwrap();
    let task = uuid::Uuid::new_v4().to_string();
    execute(&tx, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,room) VALUES($1,$2,$3,'commitment','person','Salary summary',$4,$5,$6)",
        vec![s.w.clone().into(), task.clone().into(), s.agent.clone().into(), s.owner.id.clone().into(),
            chat.unwrap_or(&room).into(), chat.map(|_| room.clone()).into()]).await.unwrap();
    let job =
        s.db.background_in(
            &tx,
            &s.owner,
            &s.w,
            &room,
            &message("Task run · Salary summary", Some(&s.agent)),
        )
        .await
        .unwrap()
        .unwrap();
    execute(
        &tx,
        "UPDATE relay_chat_jobs SET task=$2 WHERE reply=$1",
        vec![job.reply.clone().into(), task.into()],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    super::super::home_maintenance_tests::claim(&s.db, &s.w, &job.reply).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_task_run_uses_only_what_everyone_where_it_posts_may_see() {
    let mut s = setup().await;
    s.db.pooled = true;
    // Posting nowhere but its own room: only the owner reads the result.
    let private = run(&s, None).await;
    assert_eq!(
        invoke(&s.db, &private, "read_document", json!({"id":s.salaries}))
            .await
            .unwrap()["state"],
        "available"
    );
    // Posting to a conversation with a colleague who can't see the HR folder.
    let shared = chat(
        &s.db,
        &s.owner,
        &s.w,
        vec![s.member.id.clone()],
        vec![s.agent.clone()],
    )
    .await;
    let posted = run(&s, Some(&shared)).await;
    assert_eq!(
        invoke(&s.db, &posted, "read_document", json!({"id":s.salaries}))
            .await
            .unwrap()["state"],
        "not_found"
    );
}
