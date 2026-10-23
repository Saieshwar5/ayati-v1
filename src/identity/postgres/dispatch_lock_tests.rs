use super::{
    executor_slot_tests::{context, report, reserve, setup},
    tests::*,
    *,
};
use crate::identity::{provider::digest, worker_control::model::AssignedRequest};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn dispatch_fk_locks_do_not_deadlock_workspace_writers_and_revocation_still_fences_context() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Concurrent intake and dispatch", Some(&a)),
    )
    .await
    .unwrap();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let writer = db.workspace_tx(&w).await.unwrap();
    execute(&writer,"UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{paused}','true') WHERE workspace=$1 AND id=$2",vec![w.clone().into(),a.into()]).await.unwrap();
    // Offers carry metadata, not context authority. FK writes must coexist with
    // an uncommitted workspace writer while retaining durable ownership.
    let offered = tokio::time::timeout(Duration::from_millis(500), reserve(&db, &e)).await;
    if offered.is_err() {
        writer.rollback().await.unwrap();
        panic!("Dispatch blocked on the workspace/FK lock cycle");
    }
    let scope = offered.unwrap().unwrap();
    let request = AssignedRequest {
        scope: scope.clone(),
        session: uuid::Uuid::new_v4().to_string(),
    };
    // Starting work still waits for the current workspace authorization decision.
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            db.start_controlled(&request, &digest(&slot.token), &context(&scope))
        )
        .await
        .is_err()
    );
    writer.commit().await.unwrap();
    assert!(
        db.start_controlled(&request, &digest(&slot.token), &context(&scope))
            .await
            .is_err()
    );
    let row = query(
        &db.db,
        "SELECT session,context FROM relay_assignment_credentials WHERE assignment=$1",
        vec![scope.assignment.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        row.try_get::<Option<String>>("", "session")
            .unwrap()
            .is_none()
    );
    assert!(
        row.try_get::<Option<Value>>("", "context")
            .unwrap()
            .is_none()
    );
}
