//! Shared rooms: one brief for everyone in a channel, thread-first reading, and
//! a private question moved to the asker's direct conversation.
use super::super::{tests::*, *};
use super::{Audience, audience};
use crate::identity::{
    chat::{
        execution::prepare,
        model::{Job, NewMessage},
    },
    control::broker::invoke,
};
use serde_json::json;

async fn task(db: &PgStore, who: &User, w: &str, c: &str, a: &str, text: &str) -> Job {
    db.chat_send(who, w, c, &message(text, Some(a)))
        .await
        .unwrap();
    db.claim_job().await.unwrap().unwrap()
}
fn brief(revision: i64, source: &str, text: &str) -> Value {
    json!({"revision":revision,"objective":{"text":text,"sources":[source]},"decisions":[],"questions":[],"corrections":[]})
}
async fn request_id(db: &PgStore, job: &Job) -> String {
    db.request_context(job).await.unwrap().0["request_message"]
        .as_str()
        .unwrap()
        .to_owned()
}
/// A workspace with an agent, its owner, a second member who may use it, a
/// channel for both and the owner's own group with the agent.
async fn setup(name: &str) -> (PgStore, User, User, String, String, String, String) {
    let db = fixture().await;
    let owner = user(&db, &format!("{name}-owner")).await;
    let other = user(&db, &format!("{name}-other")).await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &other, "Member").await;
    let (solo, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &other.id, &a, true)
        .await
        .unwrap();
    let channel = chat(&db, &owner, &w, vec![other.id.clone()], vec![a.clone()]).await;
    (db, owner, other, w, a, channel, solo)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_shared_room_has_one_brief_everyone_in_it_reads() {
    let (db, owner, other, w, a, channel, solo) = setup("brief").await;
    assert_eq!(
        audience(&db.db, &w, &channel).await.unwrap(),
        Some(Audience {
            public: false,
            people: 2
        })
    );
    assert_eq!(
        audience(&db.db, &w, &solo).await.unwrap(),
        None,
        "one person's own group is private"
    );

    let first = task(&db, &owner, &w, &channel, &a, "We launch on Friday").await;
    prepare(&db, &first, 180).await.unwrap();
    let id = request_id(&db, &first).await;
    invoke(
        &db,
        &first,
        "save_working_context",
        brief(0, &id, "Launch on Friday"),
    )
    .await
    .unwrap();
    db.chat_finish(&first, "Noted", "complete").await.unwrap();
    let owners: Vec<String> = rows(
        &db.db,
        "SELECT requester FROM relay_working_context WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|r| r.try_get("", "requester").unwrap())
    .collect();
    assert_eq!(
        owners,
        vec![String::new()],
        "a shared brief belongs to the room, not to who wrote it"
    );

    // Another person in the room reads the same brief, and is told who reads replies.
    let second = task(&db, &other, &w, &channel, &a, "When do we launch?").await;
    let working = db.working_context(&second).await.unwrap();
    assert_eq!(
        (
            working["available"].as_bool(),
            working["brief"]["objective"]["text"].as_str()
        ),
        (Some(true), Some("Launch on Friday"))
    );
    let prepared = prepare(&db, &second, 180).await.unwrap();
    assert!(
        prepared
            .request
            .contains("Shared brief for this conversation")
    );
    assert!(
        prepared
            .instructions
            .contains("This conversation is shared: 2 people can read your reply")
    );
    let preview = db
        .agent_context_preview(&other, &w, &a, &channel)
        .await
        .unwrap();
    assert_eq!(
        preview["working"]["brief"]["objective"]["text"],
        "Launch on Friday"
    );
    db.chat_finish(&second, "Friday", "complete").await.unwrap();

    // The owner's private group never sees it.
    let private = task(&db, &owner, &w, &solo, &a, "My own plans").await;
    assert_eq!(db.working_context(&private).await.unwrap()["revision"], 0);
    let prepared = prepare(&db, &private, 180).await.unwrap();
    assert!(
        prepared.request.contains("Working conversation summary")
            && !prepared.request.contains("Launch on Friday")
    );
    assert!(
        !prepared
            .instructions
            .contains("This conversation is shared")
    );
    db.chat_finish(&private, "Done", "complete").await.unwrap();

    // Someone leaving the room fences the brief: it is rebuilt from what remains.
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            w.clone().into(),
            channel.clone().into(),
            other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    let later = task(&db, &owner, &w, &channel, &a, "Anything new?").await;
    assert_eq!(
        db.working_context(&later).await.unwrap()["available"],
        false
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_thread_request_reads_its_thread_and_the_main_timeline_not_other_threads() {
    let (db, owner, _, w, a, channel, _) = setup("threads").await;
    let post = |text: &str, thread: Option<String>| NewMessage {
        thread,
        ..message(text, None)
    };
    let root = post("MAIN_ROOT budget question", None);
    db.chat_send(&owner, &w, &channel, &root).await.unwrap();
    db.chat_send(
        &owner,
        &w,
        &channel,
        &post("IN_THREAD the figure is 40", Some(root.id.clone())),
    )
    .await
    .unwrap();
    let other_root = post("MAIN_OTHER hiring", None);
    db.chat_send(&owner, &w, &channel, &other_root)
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &channel,
        &post("OTHER_THREAD candidate names", Some(other_root.id.clone())),
    )
    .await
    .unwrap();

    let ask = NewMessage {
        thread: Some(root.id.clone()),
        ..message("What was the figure?", Some(&a))
    };
    db.chat_send(&owner, &w, &channel, &ask).await.unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let (_, request) = db.request_context(&job).await.unwrap();
    for present in [
        "MAIN_ROOT",
        "IN_THREAD",
        "MAIN_OTHER",
        "What was the figure?",
    ] {
        assert!(request.contains(present), "{present} missing");
    }
    assert!(
        !request.contains("OTHER_THREAD"),
        "another thread stays out"
    );
    db.chat_finish(&job, "40", "complete").await.unwrap();

    // A request on the main timeline still reads the whole conversation.
    let main = task(&db, &owner, &w, &channel, &a, "Summarise everything").await;
    assert!(
        db.request_context(&main)
            .await
            .unwrap()
            .1
            .contains("OTHER_THREAD")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_private_question_moves_to_the_askers_direct_conversation() {
    let (db, owner, other, w, a, channel, solo) = setup("moved").await;
    let text = format!("[@Owner](mention:user:{}) what is my salary?", owner.id);
    let asked = task(&db, &other, &w, &channel, &a, &text).await;
    let moved = invoke(&db, &asked, "continue_privately", json!({}))
        .await
        .unwrap();
    assert_eq!(moved["state"], "moved");
    assert_eq!(
        invoke(&db, &asked, "continue_privately", json!({}))
            .await
            .unwrap()["reply"],
        moved["reply"],
        "an exact retry moves it once"
    );
    let dm = moved["conversation"].as_str().unwrap().to_owned();
    let room = json_rows(&db.db, "SELECT jsonb_build_object('kind',c.kind,'people',(SELECT jsonb_agg(user_id) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id),'agents',(SELECT jsonb_agg(agent) FROM relay_chat_agents x WHERE x.workspace=c.workspace AND x.chat=c.id)) AS data FROM relay_chats c WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), dm.clone().into()]).await.unwrap().remove(0);
    assert_eq!(
        room,
        json!({"kind":"direct","people":[other.id],"agents":[a]})
    );
    let written = json_rows(&db.db, "SELECT jsonb_build_object('author',author,'kind',kind,'content',content) AS data FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND kind='agent' AND state='sent'",
        vec![w.clone().into(), dm.clone().into()]).await.unwrap().remove(0);
    assert_eq!(
        written,
        json!({"author":a,"kind":"agent","content":"You asked in Operations: @Owner what is my salary?"})
    );
    db.chat_finish(&asked, "I've answered you privately.", "complete")
        .await
        .unwrap();

    // The private request answers the same person, as their own question.
    let private = db.claim_job().await.unwrap().unwrap();
    assert_eq!(
        (
            private.chat.as_str(),
            private.requester.as_str(),
            private.reply.as_str()
        ),
        (
            dm.as_str(),
            other.id.as_str(),
            moved["reply"].as_str().unwrap()
        )
    );
    let (identity, _) = db.request_context(&private).await.unwrap();
    assert_eq!(
        (
            identity["origin"].as_str(),
            identity["initiating_actor"]["id"].as_str()
        ),
        (Some("private_continuation"), Some(other.id.as_str()))
    );
    let prepared = prepare(&db, &private, 180).await.unwrap();
    assert!(
        prepared
            .instructions
            .contains("you moved the question here")
            && !prepared
                .instructions
                .contains("This conversation is shared")
    );
    db.chat_finish(&private, "Your salary is private.", "complete")
        .await
        .unwrap();

    // Already private: nothing to move.
    let own = task(&db, &owner, &w, &solo, &a, "My own question").await;
    assert_eq!(
        invoke(&db, &own, "continue_privately", json!({}))
            .await
            .unwrap()["state"],
        "invalid"
    );
    db.chat_finish(&own, "Here", "complete").await.unwrap();

    // Someone who may ask in the channel but can't message the agent directly.
    let guest = user(&db, "moved-guest").await;
    join(&db, &owner, &w, &guest, "Member").await;
    let shared = chat(&db, &owner, &w, vec![guest.id.clone()], vec![a.clone()]).await;
    let asked = task(&db, &guest, &w, &shared, &a, "What is my pay?").await;
    assert_eq!(
        invoke(&db, &asked, "continue_privately", json!({}))
            .await
            .unwrap()["state"],
        "denied"
    );
}
