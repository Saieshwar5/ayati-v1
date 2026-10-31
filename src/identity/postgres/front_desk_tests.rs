//! The front desk and channel manners: a request that queues behind the agent's
//! other work is answered in the agent's own words; email threads get one
//! complete reply with no acknowledgement or updates.
use super::super::tests::{chat, fixture, join, message, user, workspace};
use super::updates_tests::{direct, update, updates};
use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_front_desk_answers_a_request_that_queues_behind_other_work_in_the_agents_voice() {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "desk-owner").await;
    let member = user(&db, "desk-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET audience='everyone' WHERE id=$1",
        vec![agent.clone().into()],
    )
    .await
    .unwrap();
    let shared = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec![agent.clone()],
    )
    .await;
    let desk = |reply: &str| {
        let db = db.clone();
        let reply = reply.to_string();
        async move {
            query(
                &db.db,
                "SELECT to_jsonb(f) AS data FROM relay_front_desk f WHERE f.reply=$1",
                vec![reply.into()],
            )
            .await
            .unwrap()
            .map(|r| r.try_get::<Value>("", "data").unwrap())
        }
    };
    // A free agent gets no front desk: its work lane starts at once.
    let first = db
        .chat_send(
            &owner,
            &w,
            &shared,
            &message("Reconcile September", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(desk(&first.reply).await.is_none());
    // The first request is running on a pooled executor; the second queues behind it.
    let (executor, _) = super::super::executor_task_tests::executor(&db).await;
    let scope = super::super::executor_slot_tests::reserve(&db, &executor)
        .await
        .unwrap();
    assert_eq!(scope.reply, first.reply);
    let second = db
        .chat_send(
            &member,
            &w,
            &shared,
            &message("Sharma's balance?", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    let record = desk(&second.reply).await.unwrap();
    assert_eq!(record["state"], "queued");
    let prepared = record["prepared"]["request"].as_str().unwrap();
    assert!(prepared.contains("Right now you are working on a request from desk-owner@example.test (they can see that conversation): \"Reconcile September\", started just now"), "{prepared}");
    assert!(
        prepared.contains("This request is next.")
            && prepared.ends_with("Front desk: Sharma's balance?"),
        "{prepared}"
    );
    assert!(
        record["prepared"]["instructions"]
            .as_str()
            .unwrap()
            .contains("answering at the front desk")
    );
    // A pooled executor runs the desk turn and the model answers in JSON.
    let run = super::super::executor_task_tests::claim(&db)
        .await
        .expect("the front desk is offered while the work lane is busy");
    assert_eq!(run.id(), record["id"].as_str().unwrap());
    assert!(run.finish("{\"reply\": \"Sure — I'm in the middle of the September reconciliation; I'll pull Sharma's balance right after.\", \"answer_now\": false}").await);
    let posted = updates(&db, &second.reply).await;
    assert_eq!(posted, vec![("Sure — I'm in the middle of the September reconciliation; I'll pull Sharma's balance right after.".to_string(), None)]);
    assert_eq!(
        desk(&second.reply).await.unwrap()["decision"],
        "acknowledged"
    );
    assert_eq!(
        query(
            &db.db,
            "SELECT state FROM relay_chat_jobs WHERE reply=$1",
            vec![second.reply.clone().into()]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "state")
        .unwrap(),
        "queued",
        "the work still waits its turn"
    );
    // The work lane is told what was already said, so it doesn't repeat it.
    let said = db.front_desk_said(&second).await.unwrap().unwrap();
    assert!(said.starts_with("Sure — I'm in the middle"), "{said}");

    // A quick question answered at the desk closes the request without any work.
    let third = db
        .chat_send(
            &member,
            &w,
            &shared,
            &message("What's your name?", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        desk(&third.reply).await.unwrap()["prepared"]["request"]
            .as_str()
            .unwrap()
            .contains("1 more requests are waiting before this one.")
    );
    let run = super::super::executor_task_tests::claim(&db).await.unwrap();
    assert!(run.finish("```json\n{\"reply\": \"I'm Pool agent, the bounded fixture.\", \"answer_now\": true}\n```").await);
    let answered = query(&db.db, "SELECT m.state,m.content,j.state AS job FROM relay_chat_messages m JOIN relay_chat_jobs j ON j.reply=m.id WHERE m.id=$1", vec![third.reply.clone().into()]).await.unwrap().unwrap();
    assert_eq!(
        (
            answered.try_get::<String>("", "state").unwrap(),
            answered.try_get::<String>("", "job").unwrap()
        ),
        ("complete".into(), "finished".into())
    );
    assert_eq!(
        answered.try_get::<String>("", "content").unwrap(),
        "I'm Pool agent, the bounded fixture."
    );
    assert!(updates(&db, &third.reply).await.is_empty());
    assert_eq!(desk(&third.reply).await.unwrap()["decision"], "answered");
    let published = query(&db.db, "SELECT state,envelope->>'text' AS text FROM relay_communication_outbox WHERE workspace=$1 AND id=$2", vec![w.clone().into(), third.reply.clone().into()]).await.unwrap().unwrap();
    assert_eq!(
        (
            published.try_get::<String>("", "state").unwrap(),
            published.try_get::<String>("", "text").unwrap()
        ),
        (
            "published".into(),
            "I'm Pool agent, the bounded fixture.".into()
        )
    );

    // Someone who can't see the running conversation isn't told whose it is or what it says.
    let outsider = user(&db, "desk-outsider").await;
    join(&db, &owner, &w, &outsider, "Member").await;
    let private = direct(&db, &outsider, &w, &agent).await;
    let fourth = db
        .chat_send(
            &outsider,
            &w,
            &private,
            &message("Stock check", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    let note = desk(&fourth.reply).await.unwrap()["prepared"]["request"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        note.contains("someone else, in a conversation this person can't see")
            && !note.contains("Reconcile September"),
        "{note}"
    );
    // Plain text from the model still counts as an acknowledgement.
    let run = super::super::executor_task_tests::claim(&db).await.unwrap();
    assert!(
        run.finish("Give me a few minutes, I'm finishing something.")
            .await
    );
    assert_eq!(
        updates(&db, &fourth.reply).await[0].0,
        "Give me a few minutes, I'm finishing something."
    );
    // Background work (schedules, triggers) never gets a front desk.
    let background = db
        .chat_background(
            &owner,
            &w,
            &shared,
            &message("Scheduled summary", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(desk(&background.reply).await.is_none());
    // The desk's words are steps on the request, like the agent's own updates.
    let details = db
        .activity_request(&member, &w, &second.reply)
        .await
        .unwrap();
    assert_eq!(
        details["steps"][0]["title"],
        "Said: Sure — I'm in the middle of the September reconciliation; I'll pull Sharma's balance right after."
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_front_desk_whose_request_already_finished_says_nothing() {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "desk-late-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let first = db
        .chat_send(&owner, &w, &c, &message("Long work", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    let (executor, _) = super::super::executor_task_tests::executor(&db).await;
    super::super::executor_slot_tests::reserve(&db, &executor)
        .await
        .unwrap();
    let second = db
        .chat_send(&owner, &w, &c, &message("And this", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    let desk: String = query(
        &db.db,
        "SELECT id FROM relay_front_desk WHERE reply=$1",
        vec![second.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "id")
    .unwrap();
    // The person stops the request before any executor picks the desk turn up.
    db.cancel_assignment(&owner, &w, &c, &second.reply)
        .await
        .unwrap();
    assert!(
        super::super::executor_task_tests::claim(&db)
            .await
            .is_none(),
        "a desk turn for a finished request is never offered"
    );
    let closed = query(&db.db, "SELECT f.state,f.decision,t.state AS task FROM relay_front_desk f JOIN relay_executor_tasks t ON t.id=f.id WHERE f.id=$1", vec![desk.into()]).await.unwrap().unwrap();
    assert_eq!(
        (
            closed.try_get::<String>("", "state").unwrap(),
            closed.try_get::<String>("", "decision").unwrap(),
            closed.try_get::<String>("", "task").unwrap()
        ),
        ("finished".into(), "nothing".into(), "finished".into())
    );
    let _ = first;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn email_threads_get_one_complete_reply_with_no_acknowledgement_or_updates() {
    use crate::identity::communications::model::{Binding, Endpoint, Message, Transport};
    let mut db = fixture().await;
    let owner = user(&db, "desk-email-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let endpoint = Endpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent: agent.clone(),
        transport: Transport::Email,
        address: "nova@simulation.invalid".into(),
    };
    db.communication_endpoint(&owner, &w, &endpoint)
        .await
        .unwrap();
    let binding = Binding {
        sender: "owner@simulation.invalid".into(),
        user: owner.id.clone(),
    };
    db.communication_binding(&owner, &w, &endpoint.id, &binding)
        .await
        .unwrap();
    let email = |event: &str, text: &str| Message {
        event: event.into(),
        sender: "owner@simulation.invalid".into(),
        recipient: "nova@simulation.invalid".into(),
        conversation: "email-thread-1".into(),
        text: text.into(),
        attachments: vec![],
        automated: false,
    };
    // An agent working on an email can't post progress into the thread.
    db.simulate_incoming(
        &owner,
        &w,
        &endpoint.id,
        &email("first", "Send me the quarterly report"),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let refused = db
        .send_update(&job, &update("Got it, pulling the numbers"))
        .await
        .unwrap();
    assert_eq!(refused["state"], "not_here");
    assert!(updates(&db, &job.reply).await.is_empty());
    db.chat_finish(&job, "Here is the quarterly report.", "complete")
        .await
        .unwrap();
    // While the agent is busy, a chat message gets the front desk; an email does not.
    db.pooled = true;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Reconcile September", Some(&agent)),
    )
    .await
    .unwrap()
    .unwrap();
    let (executor, _) = super::super::executor_task_tests::executor(&db).await;
    super::super::executor_slot_tests::reserve(&db, &executor)
        .await
        .unwrap();
    db.simulate_incoming(
        &owner,
        &w,
        &endpoint.id,
        &email("second", "And last year's too"),
    )
    .await
    .unwrap();
    let chat = db
        .chat_send(&owner, &w, &c, &message("Sharma's balance?", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    let desks: Vec<String> = rows(
        &db.db,
        "SELECT reply FROM relay_front_desk WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get("", "reply").unwrap())
    .collect();
    assert_eq!(
        desks,
        vec![chat.reply],
        "only the chat message is answered at the front desk"
    );
}
