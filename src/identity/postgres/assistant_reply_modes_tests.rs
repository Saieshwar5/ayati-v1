//! One position per chat for a member's assistant (schema 141): replies that
//! wait for the member, replies sent for them, stepping down by itself, and off.
use super::super::LIMIT;
use super::super::tests::{one, queued, reply, room, stored, thread};
use super::SetReplies;
use crate::identity::chat::model::NewChat;
use crate::identity::postgres::{tests::*, *};
use serde_json::json;

/// Ravi and Priya in a direct chat. Priya asked, Ravi answered, and Priya
/// wrote once more: `unanswered`.
struct Scene {
    db: PgStore,
    ravi: User,
    priya: User,
    w: String,
    chat: String,
    own_room: String,
    unanswered: String,
}
fn chat_of(kind: &str, with: &[&User]) -> NewChat {
    NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Planning".into(),
        kind: kind.into(),
        description: String::new(),
        users: with.iter().map(|user| user.id.clone()).collect(),
        agents: vec![],
    }
}
async fn scene() -> Scene {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    let mine = db
        .ensure_assistant(&ravi, &w, "fixture", "openai")
        .await
        .unwrap();
    let own_room = room(&db, &mine).await;
    let chat = db
        .chat_create(&ravi, &w, &chat_of("direct", &[&priya]))
        .await
        .unwrap();
    db.chat_send(
        &priya,
        &w,
        &chat,
        &message("Any update on the funding?", None),
    )
    .await
    .unwrap();
    db.chat_send(&ravi, &w, &chat, &message("Let me check.", None))
        .await
        .unwrap();
    let unanswered = message("Can we meet on Friday at 3?", None);
    db.chat_send(&priya, &w, &chat, &unanswered).await.unwrap();
    Scene {
        db,
        ravi,
        priya,
        w,
        chat,
        own_room,
        unanswered: unanswered.id,
    }
}
fn set(chat: &str, mode: &str, instruction: Option<&str>) -> SetReplies {
    SetReplies {
        id: uuid::Uuid::new_v4().to_string(),
        chat: chat.into(),
        mode: mode.into(),
        instruction: instruction.map(str::to_owned),
    }
}
async fn posted(db: &PgStore, chat: &str) -> Value {
    one(
        db,
        "SELECT count(*) FROM relay_chat_messages WHERE chat=$1",
        vec![chat.into()],
    )
    .await
}
async fn audited(db: &PgStore, thread: &str, action: &str) -> Value {
    one(
        db,
        "SELECT count(*) FROM relay_audit WHERE target=$1 AND action=$2",
        vec![thread.into(), action.into()],
    )
    .await
}
/// The reply that waits for the member, as their thread list shows it.
async fn waiting(db: &PgStore, who: &User, w: &str) -> Value {
    db.threads(who, w).await.unwrap()["threads"][0]["draft"].clone()
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn on_ask_me_first_each_reply_waits_for_its_person_who_sends_it() {
    let s = scene().await;
    let (db, ravi, priya, w, chat) = (&s.db, &s.ravi, &s.priya, s.w.as_str(), s.chat.as_str());

    // Only a known position, in a direct chat you are in with one other side
    // that is not an assistant, with a short instruction.
    let meera = user(db, "meera").await;
    join(db, ravi, w, &meera, "Member").await;
    db.ensure_assistant(&meera, w, "fixture", "openai")
        .await
        .unwrap();
    let group = db
        .chat_create(ravi, w, &chat_of("group", &[priya, &meera]))
        .await
        .unwrap();
    let long = "x".repeat(1001);
    for input in [
        set(chat, "always", None),
        set(&s.own_room, "ask", None),
        set(&group, "ask", None),
        set(chat, "auto", Some(&long)),
    ] {
        assert!(
            db.thread_position(ravi, w, &input).await.is_err(),
            "{} in {}",
            input.mode,
            input.chat
        );
    }
    assert!(
        db.thread_position(&meera, w, &set(chat, "ask", None))
            .await
            .is_err(),
        "not in a chat you are not in"
    );
    assert_eq!(
        db.thread_position(ravi, w, &set(chat, "off", None))
            .await
            .unwrap(),
        None,
        "off where nothing is on changes nothing"
    );

    // Turned on: one thread for the chat, and nothing is sent.
    let before = posted(db, chat).await;
    let on = set(chat, "ask", None);
    let id = db.thread_position(ravi, w, &on).await.unwrap().unwrap();
    assert_eq!(id, on.id);
    let again = db.thread_position(ravi, w, &set(chat, "ask", None)).await;
    assert_eq!(again.unwrap().as_deref(), Some(id.as_str()));
    let saved = thread(db, &id).await;
    assert_eq!(
        [
            &saved["state"],
            &saved["mode"],
            &saved["sent"],
            &saved["anchor"]
        ],
        [&json!("running"), &json!("ask"), &json!(0), &Value::Null]
    );
    assert_eq!(audited(db, &id, "assistant_thread_started").await, 1);
    assert_eq!(posted(db, chat).await, before);

    // What Priya wrote since Ravi's last message is handed over. The answer is
    // the reply itself: the assistant can't send it.
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    assert_eq!(db.dispatch_threads().await.unwrap(), 0, "one run at a time");
    let run = queued(db, &s.own_room).await;
    assert!(
        run.prompt
            .contains("Your person reads your reply and sends it themselves")
    );
    assert!(run.prompt.contains("Can we meet on Friday at 3?"));
    assert_eq!(
        reply(db, &run, &id, "early", "Friday works.").await["state"],
        "invalid"
    );
    let writing = waiting(db, ravi, w).await;
    assert_eq!(
        [&writing["reply"], &writing["text"]],
        [&json!(run.reply), &json!("")]
    );
    db.chat_finish(&run, "Friday at 3 works for me.", "complete")
        .await
        .unwrap();
    let ready = waiting(db, ravi, w).await;
    assert_eq!(
        [&ready["state"], &ready["text"]],
        [&json!("complete"), &json!("Friday at 3 works for me.")]
    );
    assert_eq!(posted(db, chat).await, before, "it waits; nothing is sent");
    assert_eq!(db.threads(priya, w).await.unwrap()["threads"], json!([]));

    // Ravi sends it as written: his own message, marked for him, and it waits no more.
    let sent = message("Friday at 3 works for me.", None);
    db.chat_send(ravi, w, chat, &sent).await.unwrap();
    let learned = db.draft_used(ravi, w, &run.reply, &sent.id).await.unwrap();
    assert!(!learned, "a reply sent as written teaches nothing");
    assert_eq!(
        stored(db, &sent.id).await["assisted"]["draft"],
        run.reply.as_str()
    );
    assert!(thread(db, &id).await["draft"].is_null());

    // The next reply stops waiting once Ravi answers himself.
    db.chat_send(priya, w, chat, &message("Which room?", None))
        .await
        .unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    let next = queued(db, &s.own_room).await;
    db.chat_finish(&next, "The small room.", "complete")
        .await
        .unwrap();
    assert_eq!(waiting(db, ravi, w).await["text"], "The small room.");
    db.chat_send(ravi, w, chat, &message("I'll book one.", None))
        .await
        .unwrap();
    assert!(waiting(db, ravi, w).await.is_null());

    // A reply Ravi does not want is discarded, by him only.
    db.chat_send(priya, w, chat, &message("Thanks!", None))
        .await
        .unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    let third = queued(db, &s.own_room).await;
    db.chat_finish(&third, "You're welcome.", "complete")
        .await
        .unwrap();
    assert_eq!(waiting(db, ravi, w).await["text"], "You're welcome.");
    assert!(db.thread_draft_discard(priya, w, &id).await.is_err());
    db.thread_draft_discard(ravi, w, &id).await.unwrap();
    assert!(waiting(db, ravi, w).await.is_null());
    assert_eq!(posted(db, chat).await, json!(before.as_i64().unwrap() + 4));
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn reply_for_me_sends_as_its_person_steps_down_by_itself_and_off_sends_nothing() {
    let s = scene().await;
    let (db, ravi, priya, w, chat) = (&s.db, &s.ravi, &s.priya, s.w.as_str(), s.chat.as_str());
    let position = async |mode: &str| db.thread_position(ravi, w, &set(chat, mode, None)).await;
    let asked = "Settle Friday's meeting. Share nothing else.";

    // Turned on with what to do: replies begin at Priya's unanswered message.
    let on = set(chat, "auto", Some(&format!("  {asked}  ")));
    let id = db.thread_position(ravi, w, &on).await.unwrap().unwrap();
    let saved = thread(db, &id).await;
    assert_eq!(
        [
            &saved["mode"],
            &saved["instruction"],
            &saved["title"],
            &saved["anchor"]
        ],
        [
            &json!("auto"),
            &json!(asked),
            &json!(asked),
            &json!(s.unanswered)
        ]
    );
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    let run = queued(db, &s.own_room).await;
    assert!(
        run.prompt
            .contains(&format!("asked you to do there: {asked}"))
    );
    let sent = reply(db, &run, &id, "first", "Friday at 3 works for me.").await;
    assert_eq!(sent["state"], "sent");
    let landed = stored(db, sent["message"].as_str().unwrap()).await;
    assert_eq!(landed["author"], ravi.id.as_str());
    assert_eq!(landed["assisted"]["by"], "assistant");

    // A change of position keeps the thread and what was asked; replying for
    // them again starts its count again.
    assert_eq!(position("ask").await.unwrap().as_deref(), Some(id.as_str()));
    let waits = thread(db, &id).await;
    assert_eq!(
        [&waits["mode"], &waits["instruction"], &waits["sent"]],
        [&json!("ask"), &json!(asked), &json!(1)]
    );
    assert_eq!(
        reply(db, &run, &id, "second", "And 4?").await["state"],
        "invalid"
    );
    position("auto").await.unwrap();
    assert_eq!(thread(db, &id).await["sent"], 0);

    // At the limit, and when the matter is settled, it stops sending by itself.
    execute(
        &db.db,
        "UPDATE relay_work_threads SET sent=$2 WHERE id=$1",
        vec![id.clone().into(), LIMIT.into()],
    )
    .await
    .unwrap();
    assert_eq!(
        reply(db, &run, &id, "more", "One more").await["state"],
        "invalid"
    );
    let limited = thread(db, &id).await;
    assert_eq!(
        [&limited["state"], &limited["mode"]],
        [&json!("running"), &json!("ask")]
    );
    assert!(limited["detail"].as_str().unwrap().contains("30 messages"));
    position("auto").await.unwrap();
    let done = db
        .finish_thread(
            &run,
            &serde_json::from_value(json!({"thread":id,"summary":"Meeting set."})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(done["state"], "finished");
    let finished = thread(db, &id).await;
    assert_eq!(
        [&finished["state"], &finished["mode"], &finished["detail"]],
        [
            &json!("running"),
            &json!("ask"),
            &json!("Finished: Meeting set.")
        ]
    );
    db.chat_finish(&run, "The meeting is set.", "complete")
        .await
        .unwrap();

    // Off: nothing is handed over. On again: the same thread, from what came meanwhile.
    assert_eq!(position("off").await.unwrap(), None);
    let off = thread(db, &id).await;
    assert_eq!(
        [&off["state"], &off["detail"]],
        [&json!("stopped"), &json!("")]
    );
    db.chat_send(priya, w, chat, &message("See you then.", None))
        .await
        .unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 0);
    assert_eq!(position("ask").await.unwrap().as_deref(), Some(id.as_str()));
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    for (action, times) in [
        ("assistant_thread_started", 2),
        ("assistant_thread_mode", 3),
        ("assistant_thread_stopped", 1),
    ] {
        assert_eq!(audited(db, &id, action).await, times, "{action}");
    }
}
