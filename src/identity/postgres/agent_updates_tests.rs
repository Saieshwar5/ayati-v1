use super::super::tests::{chat, fixture, join, message, user, workspace};
use super::updates::{MAX_UPDATES, MIN_GAP_SECONDS, Update};
use super::*;
use crate::identity::chat::model::NewChat;

fn update(text: &str) -> Update {
    Update { text: text.into() }
}

async fn updates(db: &PgStore, reply: &str) -> Vec<(String, Option<String>)> {
    rows(
        &db.db,
        "SELECT content,thread FROM relay_chat_messages WHERE update_of=$1 ORDER BY sequence",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| {
        (
            r.try_get("", "content").unwrap(),
            r.try_get("", "thread").unwrap(),
        )
    })
    .collect()
}

async fn direct(db: &PgStore, who: &User, w: &str, agent: &str) -> String {
    db.chat_create(
        who,
        w,
        &NewChat {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Direct".into(),
            kind: "direct".into(),
            description: String::new(),
            users: vec![],
            agents: vec![agent.into()],
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn updates_go_inline_first_then_into_the_thread_in_channels_and_stay_inline_in_direct_chats()
{
    let db = fixture().await;
    let owner = user(&db, "voice-owner").await;
    let w = workspace(&db, &owner).await;
    let (channel, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let ask = message("Reconcile September", Some(&agent));
    let job = db
        .chat_send(&owner, &w, &channel, &ask)
        .await
        .unwrap()
        .unwrap();
    // Not running yet: the agent can't speak for a request it doesn't hold.
    assert!(db.send_update(&job, &update("Got it")).await.is_err());
    let running = db.claim_job().await.unwrap().unwrap();
    assert_eq!(
        db.send_update(&running, &update("  ")).await.unwrap()["state"],
        "invalid"
    );
    let first = db
        .send_update(
            &running,
            &update("Got it. I'll match the bank lines to invoices."),
        )
        .await
        .unwrap();
    assert_eq!(
        (first["state"].clone(), first["in_thread"].clone()),
        ("sent".into(), false.into())
    );
    assert_eq!(
        db.send_update(&running, &update("Half done"))
            .await
            .unwrap()["state"],
        "too_soon"
    );
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-$2 WHERE update_of=$1",
        vec![job.reply.clone().into(), MIN_GAP_SECONDS.into()],
    )
    .await
    .unwrap();
    let second = db
        .send_update(&running, &update("Half done: 6 lines don't match yet."))
        .await
        .unwrap();
    assert_eq!(second["in_thread"], true);
    let posted = updates(&db, &job.reply).await;
    assert_eq!(
        posted[0],
        (
            "Got it. I'll match the bank lines to invoices.".into(),
            None
        )
    );
    assert_eq!(
        posted[1],
        (
            "Half done: 6 lines don't match yet.".into(),
            Some(ask.id.clone())
        ),
        "later updates thread under the request"
    );
    // Updates are the agent's own messages, never "for" the requester and never the reply.
    let flags = query(&db.db, "SELECT count(*) AS n FROM relay_chat_messages WHERE update_of=$1 AND requester IS NULL AND state='complete' AND author=$2", vec![job.reply.clone().into(), agent.clone().into()])
        .await.unwrap().unwrap().try_get::<i64>("", "n").unwrap();
    assert_eq!(flags, 2);
    // The person sees them as steps on the request.
    let details = db.activity_request(&owner, &w, &job.reply).await.unwrap();
    let titles: Vec<&str> = details["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Said: Got it. I'll match the bank lines to invoices.",
            "Said: Half done: 6 lines don't match yet."
        ]
    );
    // At most MAX_UPDATES per request.
    for n in 2..MAX_UPDATES {
        execute(
            &db.db,
            "UPDATE relay_chat_messages SET at=at-$2 WHERE update_of=$1",
            vec![job.reply.clone().into(), MIN_GAP_SECONDS.into()],
        )
        .await
        .unwrap();
        assert_eq!(
            db.send_update(&running, &update(&format!("Update {n}")))
                .await
                .unwrap()["state"],
            "sent"
        );
    }
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-$2 WHERE update_of=$1",
        vec![job.reply.clone().into(), MIN_GAP_SECONDS.into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.send_update(&running, &update("One too many"))
            .await
            .unwrap()["state"],
        "limit"
    );
    db.chat_finish(&running, "Done. 206 matched, 6 unmatched.", "complete")
        .await
        .unwrap();
    assert!(
        db.send_update(&running, &update("After the fact"))
            .await
            .is_err()
    );

    // In a direct chat every update stays inline.
    let dm = direct(&db, &owner, &w, &agent).await;
    db.chat_send(&owner, &w, &dm, &message("Quote for Sharma", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    let running = db.claim_job().await.unwrap().unwrap();
    db.send_update(&running, &update("On it.")).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-$2 WHERE update_of=$1",
        vec![running.reply.clone().into(), MIN_GAP_SECONDS.into()],
    )
    .await
    .unwrap();
    db.send_update(&running, &update("Prices checked."))
        .await
        .unwrap();
    assert!(
        updates(&db, &running.reply)
            .await
            .iter()
            .all(|(_, thread)| thread.is_none())
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_request_that_queues_behind_other_work_hears_the_agent_is_busy_at_once() {
    let mut db = fixture().await;
    let owner = user(&db, "busy-owner").await;
    let member = user(&db, "busy-member").await;
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
    let dm = direct(&db, &owner, &w, &agent).await;
    // Free agent: no busy reply.
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
    assert!(updates(&db, &first.reply).await.is_empty());
    // A second request while the first is queued: it waits behind it.
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
    assert_eq!(
        updates(&db, &second.reply).await[0].0,
        "I'm working on busy-owner@example.test's request at the moment; I'll start on this right after."
    );
    let running = db.claim_job().await.unwrap().unwrap();
    assert_eq!(running.reply, first.reply);
    // The owner, from a private chat: the running work is theirs, and two are ahead.
    let third = db
        .chat_send(&owner, &w, &dm, &message("And the suppliers", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updates(&db, &third.reply).await[0].0,
        "I'm still working on your earlier request; I'll get to this after 2 others."
    );
    // Someone who can't see the running conversation isn't told whose it is.
    let private = direct(&db, &member, &w, &agent).await;
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET state='finished' WHERE reply=$1",
        vec![second.reply.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET state='finished' WHERE reply=$1",
        vec![third.reply.clone().into()],
    )
    .await
    .unwrap();
    db.chat_finish(&running, "Done.", "complete").await.unwrap();
    let owners = db
        .chat_send(&owner, &w, &dm, &message("Private follow-up", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    assert!(updates(&db, &owners.reply).await.is_empty(), "free again");
    db.claim_job().await.unwrap().unwrap();
    let fourth = db
        .chat_send(&member, &w, &private, &message("Stock check", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updates(&db, &fourth.reply).await[0].0,
        "I'm in the middle of another request; I'll start on this right after."
    );
    // Background work (schedules, triggers) never gets a busy reply.
    db.pooled = true;
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
    assert!(updates(&db, &background.reply).await.is_empty());
    // A busy reply is a step on the request too.
    let details = db
        .activity_request(&member, &w, &fourth.reply)
        .await
        .unwrap();
    assert_eq!(
        details["steps"][0]["title"],
        "Said: I'm in the middle of another request; I'll start on this right after."
    );
}
