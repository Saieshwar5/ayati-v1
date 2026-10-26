use super::{tests::*, *};
use crate::identity::{chat::store::Job, handoffs::model::Request};
pub(super) struct Setup {
    pub db: PgStore,
    pub owner: User,
    pub w: String,
    pub c: String,
    pub a: String,
    pub b: String,
    pub parent: Job,
}
pub(super) async fn setup() -> Setup {
    let (db, owner, w, _, a) = super::automation_tests::setup().await;
    let (_, b) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let c = db
        .chat_create(
            &owner,
            &w,
            &crate::identity::chat::store::NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Handoff operations".into(),
                kind: "channel".into(),
                description: String::new(),
                users: vec![],
                agents: vec![a.clone(), b.clone()],
            },
        )
        .await
        .unwrap();
    let task = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message("Coordinate the original business task", Some(&a)),
        )
        .await
        .unwrap()
        .unwrap();
    let parent = claim(&db, &task).await;
    Setup {
        db,
        owner,
        w,
        c,
        a,
        b,
        parent,
    }
}
pub(super) async fn claim(db: &PgStore, task: &Job) -> Job {
    let mut job = task.clone();
    job.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&db.db,"UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=attempts+1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1 AND state='queued'",vec![job.reply.clone().into(),job.claim.clone().into()]).await.unwrap();
    job
}
pub(super) fn request(agent: &str, key: &str) -> Request {
    Request {
        key: key.into(),
        agent: agent.into(),
        task: "Check the relevant business facts and report the result.".into(),
    }
}
pub(super) async fn state(db: &PgStore, task: &Job) -> String {
    query(
        &db.db,
        "SELECT state FROM relay_chat_jobs WHERE reply=$1",
        vec![task.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap()
}
impl Setup {
    pub async fn handoff(&self) -> (Value, Job) {
        let r = self
            .db
            .request_handoff(&self.parent, &request(&self.b, "helper"))
            .await
            .unwrap();
        let j = super::handoff_access::job(&self.db.db, &self.w, r["reply"].as_str().unwrap())
            .await
            .unwrap();
        (r, j)
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_exact_replica_replay_commits_one_agent_authored_message_and_original_principal_child()
 {
    let s = setup().await;
    let input = request(&s.b, "helper");
    let (one, two) = tokio::join!(
        s.db.request_handoff(&s.parent, &input),
        s.db.request_handoff(&s.parent, &input)
    );
    let one = one.unwrap();
    let two = two.unwrap();
    assert_eq!(one["id"], two["id"]);
    let mut changed = input.clone();
    changed.task = "A different instruction".into();
    assert!(s.db.request_handoff(&s.parent, &changed).await.is_err());
    let child = super::handoff_access::job(&s.db.db, &s.w, one["reply"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(child.requester, s.owner.id);
    assert_eq!(child.chat, s.c);
    assert_eq!(child.agent, s.b);
    let msg=query(&s.db.db,"SELECT m.author,m.kind,m.thread FROM relay_chat_messages m JOIN relay_agent_handoffs h ON h.workspace=m.workspace AND h.request_message=m.id",vec![]).await.unwrap().unwrap();
    assert_eq!(msg.try_get::<String>("", "author").unwrap(), s.a);
    assert_eq!(msg.try_get::<String>("", "kind").unwrap(), "agent");
    assert_eq!(msg.try_get::<String>("", "thread").unwrap(), s.parent.reply);
    assert!(
        s.db.request_handoff(
            &s.parent,
            &Request {
                task: "A distinct task while the helper is pending.".into(),
                ..request(&s.b, "second-while-pending")
            }
        )
        .await
        .is_err()
    );
    assert!(
        s.db.ask_question(&s.parent, &super::attention_tests::ask())
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_checkpoint_then_result_releases_parent_and_queues_one_exact_continuation() {
    let s = setup().await;
    let (_, child) = s.handoff().await;
    s.db.chat_finish(&s.parent, "I am waiting for the helper.", "complete")
        .await
        .unwrap();
    assert_eq!(state(&s.db, &s.parent).await, "waiting");
    let child = claim(&s.db, &child).await;
    s.db.chat_finish(&child, "HELPER_COMPLETED_RESULT", "complete")
        .await
        .unwrap();
    assert_eq!(state(&s.db, &s.parent).await, "queued");
    let parent = claim(&s.db, &s.parent).await;
    let saved = s.db.task_continuation(&parent).await.unwrap().unwrap();
    assert!(saved.contains("HELPER_COMPLETED_RESULT"));
    let data: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(data["handoffs"][0]["state"], "complete");
    s.db.chat_finish(&parent, "Final original task result.", "complete")
        .await
        .unwrap();
    assert_eq!(state(&s.db, &parent).await, "finished");
    let history = s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap();
    assert!(history.to_string().contains("coordination"));
    let next =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &message("Independent next request", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!next.prompt.contains("HELPER_COMPLETED_RESULT"));
    assert!(!next.prompt.contains("Final original task result"));
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_result_before_checkpoint_is_not_lost_and_self_cycles_are_rejected() {
    let s = setup().await;
    assert!(
        s.db.request_handoff(&s.parent, &request(&s.a, "self"))
            .await
            .is_err()
    );
    let (_, child) = s.handoff().await;
    let child = claim(&s.db, &child).await;
    assert!(
        s.db.request_handoff(&child, &request(&s.a, "cycle"))
            .await
            .is_err()
    );
    s.db.chat_finish(&child, "Finished before sender checkpoint.", "complete")
        .await
        .unwrap();
    assert_eq!(state(&s.db, &s.parent).await, "running");
    s.db.chat_finish(
        &s.parent,
        "Waiting checkpoint after the result.",
        "complete",
    )
    .await
    .unwrap();
    assert_eq!(state(&s.db, &s.parent).await, "queued");
    assert_eq!(
        query(
            &s.db.db,
            "SELECT run_cycle FROM relay_chat_jobs WHERE reply=$1",
            vec![s.parent.reply.clone().into()]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i32>("", "run_cycle")
        .unwrap(),
        1
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_child_stop_returns_an_outcome_but_parent_stop_permanently_cancels_descendants() {
    let s = setup().await;
    let (_, child) = s.handoff().await;
    s.db.chat_finish(&s.parent, "Waiting.", "complete")
        .await
        .unwrap();
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &child.reply)
        .await
        .unwrap();
    assert_eq!(state(&s.db, &s.parent).await, "queued");
    let parent = claim(&s.db, &s.parent).await;
    assert!(
        s.db.task_continuation(&parent)
            .await
            .unwrap()
            .unwrap()
            .contains("cancelled")
    );
    let next =
        s.db.request_handoff(
            &parent,
            &Request {
                task: "A distinct follow-up helper task.".into(),
                ..request(&s.b, "different-helper-task")
            },
        )
        .await
        .unwrap();
    let child = super::handoff_access::job(&s.db.db, &s.w, next["reply"].as_str().unwrap())
        .await
        .unwrap();
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &parent.reply)
        .await
        .unwrap();
    assert_eq!(state(&s.db, &child).await, "finished");
    assert!(
        s.db.request_handoff(&parent, &request(&s.b, "third"))
            .await
            .is_err()
    );
    let tx = s.db.read_tx(&s.w).await.unwrap();
    assert!(
        !super::handoff_access::execution_allowed(&tx, &child)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_expiry_is_terminal_and_resumes_only_the_original_task_without_a_new_helper() {
    let s = setup().await;
    let (_, child) = s.handoff().await;
    s.db.chat_finish(&s.parent, "Waiting.", "complete")
        .await
        .unwrap();
    let expires = query(
        &s.db.db,
        "SELECT expires_ms FROM relay_agent_handoffs WHERE child=$1",
        vec![child.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "expires_ms")
    .unwrap();
    assert_eq!(s.db.reconcile_handoffs_at(expires - 1).await.unwrap(), 0);
    assert_eq!(s.db.reconcile_handoffs_at(expires).await.unwrap(), 1);
    assert_eq!(state(&s.db, &child).await, "finished");
    assert_eq!(state(&s.db, &s.parent).await, "queued");
    assert_eq!(
        s.db.reconcile_handoffs_at(expires + 100000).await.unwrap(),
        0
    );
    let parent = claim(&s.db, &s.parent).await;
    assert!(
        s.db.task_continuation(&parent)
            .await
            .unwrap()
            .unwrap()
            .contains("expired")
    );
    let old =
        s.db.request_handoff(&parent, &request(&s.b, "helper"))
            .await
            .unwrap();
    assert_eq!(old["state"], "expired");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_depth_and_direct_fanout_are_bounded_and_a_descendant_cannot_extend_the_root_deadline()
 {
    let s = setup().await;
    let mut extra = vec![];
    for _ in 0..3 {
        let (_, agent) = super::executor_slot_tests::agent(&s.db, &s.owner, &s.w).await;
        execute(
            &s.db.db,
            "INSERT INTO relay_chat_agents(workspace,chat,agent) VALUES($1,$2,$3)",
            vec![s.w.clone().into(), s.c.clone().into(), agent.clone().into()],
        )
        .await
        .unwrap();
        extra.push(agent);
    }
    let (r, child) = s.handoff().await;
    let expiry = query(
        &s.db.db,
        "SELECT expires_ms FROM relay_agent_handoffs WHERE id=$1",
        vec![r["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "expires_ms")
    .unwrap();
    let child = claim(&s.db, &child).await;
    let second =
        s.db.request_handoff(&child, &request(&extra[0], "level-two"))
            .await
            .unwrap();
    let j = super::handoff_access::job(&s.db.db, &s.w, second["reply"].as_str().unwrap())
        .await
        .unwrap();
    let level_two = claim(&s.db, &j).await;
    let third =
        s.db.request_handoff(&level_two, &request(&extra[1], "level-three"))
            .await
            .unwrap();
    let j = super::handoff_access::job(&s.db.db, &s.w, third["reply"].as_str().unwrap())
        .await
        .unwrap();
    let level_three = claim(&s.db, &j).await;
    assert!(
        s.db.request_handoff(&level_three, &request(&extra[2], "level-four"))
            .await
            .is_err()
    );
    assert_eq!(
        query(
            &s.db.db,
            "SELECT max(expires_ms)-min(expires_ms) AS difference FROM relay_agent_handoffs",
            vec![]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "difference")
        .unwrap(),
        0
    );
    assert!(expiry > now() * 1000);
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, &child.reply)
        .await
        .unwrap();
    assert_eq!(state(&s.db, &level_three).await, "finished");
    s.db.chat_finish(&s.parent, "Saved coordination checkpoint.", "complete")
        .await
        .unwrap();
    let parent = claim(&s.db, &s.parent).await;
    let second =
        s.db.request_handoff(
            &parent,
            &Request {
                task: "A distinct second root helper task.".into(),
                ..request(&s.b, "root-second")
            },
        )
        .await
        .unwrap();
    s.db.cancel_assignment(&s.owner, &s.w, &s.c, second["reply"].as_str().unwrap())
        .await
        .unwrap();
    assert!(
        s.db.request_handoff(
            &parent,
            &Request {
                task: "A distinct third root helper task.".into(),
                ..request(&s.b, "root-third")
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn handoff_new_model_keys_for_identical_work_reuse_the_canonical_receipt_without_a_second_child()
 {
    let s = setup().await;
    let first = request(&s.b, "first-model-key");
    let mut second = first.clone();
    second.key = "recovered-model-key".into();
    let (one, two) = tokio::join!(
        s.db.request_handoff(&s.parent, &first),
        s.db.request_handoff(&s.parent, &second)
    );
    let one = one.unwrap();
    let two = two.unwrap();
    assert_eq!(one["id"], two["id"]);
    assert_eq!(
        query(
            &s.db.db,
            "SELECT count(*) AS n FROM relay_agent_handoffs",
            vec![]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap(),
        1
    );
}
