//! About you, drafts and regular checks (schema 129).
use super::super::super::{tests::*, *};
use super::{DraftRequest, SaveSettings};
use crate::identity::chat::model::{Job, NewChat};
use serde_json::json;

async fn one(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT to_jsonb(({sql})) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}
async fn claimed(db: &PgStore, job: &Job) -> Job {
    super::super::super::handoff_tests::claim(db, job).await
}
async fn queued(db: &PgStore, chat: &str) -> Job {
    let payload = one(
        db,
        "SELECT j.payload FROM relay_chat_jobs j WHERE j.payload->>'chat'=$1 AND j.state='queued'",
        vec![chat.into()],
    )
    .await;
    claimed(db, &serde_json::from_value(payload).unwrap()).await
}
async fn room_of(db: &PgStore, agent: &str) -> String {
    one(
        db,
        "SELECT chat FROM relay_chat_agents WHERE agent=$1",
        vec![agent.into()],
    )
    .await
    .as_str()
    .unwrap()
    .to_owned()
}
async fn direct_with(db: &PgStore, who: &User, w: &str, person: &User) -> String {
    db.chat_create(
        who,
        w,
        &NewChat {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Direct".into(),
            kind: "direct".into(),
            description: String::new(),
            users: vec![person.id.clone()],
            agents: vec![],
        },
    )
    .await
    .unwrap()
}
fn settings(about: &str, checks: bool, checklist: &str) -> SaveSettings {
    SaveSettings {
        about: Some(about.into()),
        style: Some("Short and friendly.".into()),
        checks: Some(checks),
        every_minutes: Some(60),
        checklist: Some(checklist.into()),
    }
}
fn draft_for(chat: &str, id: &str) -> DraftRequest {
    DraftRequest {
        id: id.into(),
        chat: chat.into(),
        thread: None,
        instruction: "say yes".into(),
    }
}
async fn make_due(db: &PgStore, who: &User) {
    execute(
        &db.db,
        "UPDATE relay_assistant_settings SET next_ms=0 WHERE user_id=$1",
        vec![who.id.clone().into()],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_assistant_keeps_notes_its_person_can_edit_and_drafts_without_posting() {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let arjun = user(&db, "arjun").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    join(&db, &ravi, &w, &arjun, "Member").await;
    let mine = db
        .ensure_assistant(&ravi, &w, "fixture", "openai")
        .await
        .unwrap();
    let room = room_of(&db, &mine).await;

    // Ravi writes what his assistant should know; it is given with every task.
    assert_eq!(db.assistant_settings(&ravi, &w).await.unwrap()["about"], "");
    db.save_assistant_settings(&ravi, &w, &settings("Head of sales.", false, ""))
        .await
        .unwrap();
    let told = db.about_person(&w, &ravi.id).await.unwrap();
    assert!(told.contains("Head of sales.") && told.contains("Short and friendly."));
    assert_eq!(
        db.about_person(&w, &priya.id).await.unwrap(),
        "",
        "each person's own"
    );

    // The agent page saves one part at a time: fields left out are kept.
    let checks_only = SaveSettings {
        about: None,
        style: None,
        checks: Some(true),
        every_minutes: Some(120),
        checklist: None,
    };
    db.save_assistant_settings(&ravi, &w, &checks_only)
        .await
        .unwrap();
    let kept = db.assistant_settings(&ravi, &w).await.unwrap();
    assert_eq!(kept["about"], "Head of sales.");
    assert_eq!(kept["style"], "Short and friendly.");
    assert_eq!(kept["checks"], true);
    assert_eq!(kept["every_minutes"], 120);
    let notes_only = SaveSettings {
        about: Some("Head of sales.".into()),
        style: None,
        checks: None,
        every_minutes: None,
        checklist: None,
    };
    db.save_assistant_settings(&ravi, &w, &notes_only)
        .await
        .unwrap();
    assert_eq!(
        db.assistant_settings(&ravi, &w).await.unwrap()["checks"],
        true,
        "saving notes leaves regular checks as they were"
    );
    db.save_assistant_settings(&ravi, &w, &settings("Head of sales.", false, ""))
        .await
        .unwrap();

    // A draft for a chat Ravi can read: asked in his assistant's room, nothing posted in the chat.
    let chat = direct_with(&db, &priya, &w, &ravi).await;
    let asked = message("Can we meet on Friday at 3?", None);
    db.chat_send(&priya, &w, &chat, &asked).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (place, job) = db
        .draft_request(&ravi, &w, &draft_for(&chat, &id))
        .await
        .unwrap();
    assert_eq!(place, room);
    assert_eq!(job.agent, mine);
    let (_, again) = db
        .draft_request(&ravi, &w, &draft_for(&chat, &id))
        .await
        .unwrap();
    assert_eq!(again.reply, job.reply, "a retried request asks once");
    let request = one(
        &db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![id.clone().into()],
    )
    .await;
    let request = request.as_str().unwrap();
    assert!(request.contains("Can we meet on Friday at 3?") && request.contains("say yes"));
    assert_eq!(
        one(
            &db,
            "SELECT count(*) FROM relay_chat_messages WHERE chat=$1",
            vec![chat.clone().into()]
        )
        .await,
        1,
        "nothing is posted in the chat"
    );
    // Someone who can't read the chat can't have it drafted.
    db.ensure_assistant(&arjun, &w, "fixture", "openai")
        .await
        .unwrap();
    assert!(
        db.draft_request(
            &arjun,
            &w,
            &draft_for(&chat, &uuid::Uuid::new_v4().to_string())
        )
        .await
        .is_err()
    );

    // The assistant writes the draft and learns one lasting thing, kept once.
    let job = claimed(&db, &job).await;
    assert_eq!(
        db.draft(&ravi, &w, &job.reply).await.unwrap()["state"],
        "queued"
    );
    let note = || {
        serde_json::from_value(json!({"kind":"about","text":"Prefers meetings after 2pm."}))
            .unwrap()
    };
    assert_eq!(
        db.remember_about_person(&job, &note()).await.unwrap()["state"],
        "kept"
    );
    db.remember_about_person(&job, &note()).await.unwrap();
    assert_eq!(
        db.assistant_settings(&ravi, &w).await.unwrap()["about"],
        "Head of sales.\nPrefers meetings after 2pm."
    );
    db.chat_finish(&job, "Yes, Friday at 3 works for me.", "complete")
        .await
        .unwrap();
    let draft = db.draft(&ravi, &w, &job.reply).await.unwrap();
    assert_eq!(draft["state"], "complete");
    assert_eq!(draft["text"], "Yes, Friday at 3 works for me.");
    assert!(
        db.draft(&priya, &w, &job.reply).await.is_err(),
        "only its person reads a draft"
    );

    // Ravi sends it himself; only he sees that his assistant wrote it.
    let sent = message("Yes, Friday at 3 works for me.", None);
    db.chat_send(&ravi, &w, &chat, &sent).await.unwrap();
    db.draft_used(&ravi, &w, &job.reply, &sent.id)
        .await
        .unwrap();
    let shown = |page: Value| {
        page["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == sent.id.as_str())
            .unwrap()
            .clone()
    };
    assert_eq!(
        shown(db.chat_messages(&ravi, &w, &chat).await.unwrap())["assisted"]["by"],
        "assistant"
    );
    assert!(shown(db.chat_messages(&priya, &w, &chat).await.unwrap())["assisted"].is_null());
    // Never on someone else's message.
    db.draft_used(&ravi, &w, &job.reply, &asked.id)
        .await
        .unwrap();
    let marked = one(
        &db,
        "SELECT count(*) FROM relay_chat_messages WHERE id=$1 AND assisted IS NOT NULL",
        vec![asked.id.clone().into()],
    )
    .await;
    assert_eq!(marked, 0);
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_regular_check_runs_only_when_something_reached_its_person() {
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
    let room = room_of(&db, &mine).await;
    let runs = "SELECT count(*) FROM relay_chat_jobs WHERE payload->>'chat'=$1";

    // Off: nothing runs. On, with nothing new: no run, and the next check is booked.
    assert_eq!(db.dispatch_checks().await.unwrap(), 0);
    db.save_assistant_settings(&ravi, &w, &settings("", true, ""))
        .await
        .unwrap();
    assert_eq!(db.dispatch_checks().await.unwrap(), 0, "not due yet");
    make_due(&db, &ravi).await;
    assert_eq!(db.dispatch_checks().await.unwrap(), 0);
    assert_eq!(
        one(&db, runs, vec![room.clone().into()]).await,
        0,
        "nothing new, no run"
    );
    let next = one(
        &db,
        "SELECT next_ms FROM relay_assistant_settings WHERE user_id=$1",
        vec![ravi.id.clone().into()],
    )
    .await;
    assert!(next.as_i64().unwrap() > 0);

    // Priya writes to Ravi: the next check hands it to his assistant, once.
    let chat = direct_with(&db, &priya, &w, &ravi).await;
    db.chat_send(
        &priya,
        &w,
        &chat,
        &message("Please approve the Acme quote today.", None),
    )
    .await
    .unwrap();
    make_due(&db, &ravi).await;
    assert_eq!(db.dispatch_checks().await.unwrap(), 1);
    let run = queued(&db, &room).await;
    let handed = one(
        &db,
        "SELECT content FROM relay_chat_messages WHERE id=(SELECT request_message FROM relay_chat_jobs WHERE reply=$1)",
        vec![run.reply.clone().into()],
    )
    .await;
    assert!(
        handed
            .as_str()
            .unwrap()
            .contains("Please approve the Acme quote today.")
    );
    make_due(&db, &ravi).await;
    assert_eq!(
        db.dispatch_checks().await.unwrap(),
        0,
        "one check at a time"
    );
    db.chat_finish(
        &run,
        "Priya needs the Acme quote approved today.",
        "complete",
    )
    .await
    .unwrap();
    make_due(&db, &ravi).await;
    assert_eq!(
        db.dispatch_checks().await.unwrap(),
        0,
        "already shown, nothing new"
    );
    // His own checklist makes every check run.
    db.save_assistant_settings(&ravi, &w, &settings("", true, "Are any orders late?"))
        .await
        .unwrap();
    make_due(&db, &ravi).await;
    assert_eq!(db.dispatch_checks().await.unwrap(), 1);
}
