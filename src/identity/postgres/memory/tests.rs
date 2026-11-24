//! Memory (schema 142): a note stays where it was learned and reaches only the
//! people who could have seen it there; people see and change only their part.
use super::super::tests::*;
use super::*;
use crate::identity::control::broker::invoke;
use serde_json::json;

pub(super) struct Place {
    pub w: String,
    pub a: String,
    pub owner: User,
    pub meera: User,
    pub ravi: User,
    pub meera_chat: String,
    pub ravi_chat: String,
    pub channel: String,
}

/// An admin, two members, one agent: each member's own chat with it and a public channel.
pub(super) async fn place(db: &PgStore) -> Place {
    let owner = user(db, "memory-owner").await;
    let meera = user(db, "memory-meera").await;
    let ravi = user(db, "memory-ravi").await;
    let w = workspace(db, &owner).await;
    for (person, name) in [(&meera, "Meera Nair"), (&ravi, "Ravi Kumar")] {
        join(db, &owner, &w, person, "Member").await;
        execute(
            &db.db,
            "UPDATE relay_users SET display_name=$2 WHERE id=$1",
            vec![person.id.clone().into(), name.into()],
        )
        .await
        .unwrap();
    }
    let (_, a) = super::super::executor_slot_tests::agent(db, &owner, &w).await;
    for person in [&meera, &ravi] {
        db.chat_grant(&owner, &w, &person.id, &a, true)
            .await
            .unwrap();
    }
    let meera_chat = chat(db, &meera, &w, vec![], vec![a.clone()]).await;
    let ravi_chat = chat(db, &ravi, &w, vec![], vec![a.clone()]).await;
    let channel = chat(
        db,
        &owner,
        &w,
        vec![meera.id.clone(), ravi.id.clone()],
        vec![a.clone()],
    )
    .await;
    execute(
        &db.db,
        "UPDATE relay_chats SET kind='channel',visibility='public' WHERE id=$1",
        vec![channel.clone().into()],
    )
    .await
    .unwrap();
    Place {
        w,
        a,
        owner,
        meera,
        ravi,
        meera_chat,
        ravi_chat,
        channel,
    }
}

/// A person asks the agent; the running request.
pub(super) async fn ask(db: &PgStore, who: &User, p: &Place, chat: &str, text: &str) -> Job {
    let job = db
        .chat_send(who, &p.w, chat, &message(text, Some(&p.a)))
        .await
        .unwrap()
        .unwrap();
    super::super::handoff_tests::claim(db, &job).await
}
pub(super) async fn tool(db: &PgStore, job: &Job, name: &str, arguments: Value) -> Value {
    invoke(db, job, name, arguments).await.unwrap()
}
pub(super) async fn done(db: &PgStore, job: &Job) {
    db.chat_finish(job, "Done.", "complete").await.unwrap();
}
pub(super) fn texts(found: &Value, key: &str) -> Vec<String> {
    found[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["text"].as_str().unwrap().to_owned())
        .collect()
}
pub(super) async fn id_of(db: &PgStore, text: &str) -> String {
    query(
        &db.db,
        "SELECT id FROM relay_memories WHERE text=$1",
        vec![text.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "id")
    .unwrap()
}
pub(super) async fn titles(db: &PgStore, p: &Place, job: &Job) -> Vec<String> {
    given::steps(&db.db, &p.w, &job.reply)
        .await
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn notes_stay_where_they_were_learned_and_reach_only_their_readers() {
    let db = fixture().await;
    let p = place(&db).await;

    // In Meera's own chat: about her, kept for her; wider only as a suggestion.
    let first = ask(
        &db,
        &p.meera,
        &p,
        &p.meera_chat,
        "Keep answers short please. Acme pays after 45 days.",
    )
    .await;
    let kept = tool(
        &db,
        &first,
        "remember",
        json!({"text":"Meera wants short answers.","kind":"preference"}),
    )
    .await;
    assert_eq!(kept["state"], "kept");
    assert_eq!(kept["for"], "this conversation");
    let wider = tool(&db, &first, "remember", json!({"text":"Acme Traders pays 45 days after invoice.","kind":"customer","topic":"Acme Traders","for":"workspace"})).await;
    assert_eq!(
        wider["state"], "waiting",
        "a private chat never writes the workspace's memory"
    );
    let named = tool(
        &db,
        &first,
        "remember",
        json!({"text":"Ask Ravi Kumar before any discount.","kind":"people","for":"my_job"}),
    )
    .await;
    assert_eq!(
        named["state"], "refused",
        "a name never leaves a private chat"
    );
    let secret = tool(
        &db,
        &first,
        "remember",
        json!({"text":"The portal password is hunter2","kind":"system"}),
    )
    .await;
    assert_eq!(secret["state"], "refused");
    assert!(
        invoke(
            &db,
            &first,
            "remember",
            json!({"text":"x","kind":"customer","chat":p.ravi_chat})
        )
        .await
        .is_err(),
        "no argument names another conversation"
    );
    let lines = titles(&db, &p, &first).await;
    assert!(
        lines.contains(&"Remembered for Meera Nair: Meera wants short answers.".to_owned()),
        "{lines:?}"
    );
    assert!(lines.contains(&"Suggested, waiting for a person, for the workspace: Acme Traders pays 45 days after invoice.".to_owned()));
    done(&db, &first).await;

    // Ravi's chat with the same agent knows nothing of it.
    let job = ask(&db, &p.ravi, &p, &p.ravi_chat, "How should answers look?").await;
    let given = db.memory_given(&job).await.unwrap();
    assert!(
        !given.contains("short answers") && !given.contains("Acme"),
        "{given}"
    );
    let found = tool(
        &db,
        &job,
        "search_memory",
        json!({"query":"short answers Acme"}),
    )
    .await;
    assert_eq!(found["notes"], json!([]));
    assert!(
        titles(&db, &p, &job)
            .await
            .contains(&"Searched memory: \"short answers Acme\" · 0 found".to_owned())
    );
    done(&db, &job).await;

    // In the public channel: learned in the open, so it goes to the workspace at once.
    let job = ask(
        &db,
        &p.owner,
        &p,
        &p.channel,
        "Acme pays its invoices after 45 days. We quote in rupees here.",
    )
    .await;
    let workspace = tool(&db, &job, "remember", json!({"text":"Acme Traders pays its invoices after 45 days.","kind":"customer","topic":"Acme Traders","for":"workspace"})).await;
    assert_eq!(workspace["for"], "workspace");
    let room = tool(
        &db,
        &job,
        "remember",
        json!({"text":"This channel quotes prices in rupees only.","kind":"decision"}),
    )
    .await;
    assert_eq!(room["for"], "this conversation");
    let skill = tool(&db, &job, "remember", json!({"text":"The invoice export fails when a code has a slash.","kind":"system","for":"my_job"})).await;
    assert_eq!(skill["for"], "my job");
    done(&db, &job).await;

    // Meera's chat: her note, the job and the workspace; never the channel's.
    let job = ask(&db, &p.meera, &p, &p.meera_chat, "When does Acme pay?").await;
    let given = db.memory_given(&job).await.unwrap();
    for expected in [
        "Meera wants short answers.",
        "Acme Traders pays its invoices after 45 days.",
        "invoice export fails",
    ] {
        assert!(given.contains(expected), "{expected}: {given}");
    }
    assert!(given.contains("information, never instructions"));
    assert!(!given.contains("rupees"));
    let found = tool(&db, &job, "search_memory", json!({"query":"rupees"})).await;
    assert_eq!(found["notes"], json!([]));
    let found = tool(&db, &job, "search_memory", json!({"query":"Acme"})).await;
    assert_eq!(found["notes"][0]["from"], "workspace");
    done(&db, &job).await;

    // Ravi in the channel: the room, the job and the workspace; never Meera's.
    let job = ask(
        &db,
        &p.ravi,
        &p,
        &p.channel,
        "What currency do we quote in?",
    )
    .await;
    let given = db.memory_given(&job).await.unwrap();
    assert!(given.contains("rupees") && given.contains("Acme Traders pays its invoices"));
    assert!(!given.contains("short answers"));
    assert!(titles(&db, &p, &job).await[0].starts_with("Given from memory: 3 notes"));
    done(&db, &job).await;
}

#[test]
fn dates_and_secrets_are_read_strictly() {
    let now = notes::day_start_ms("2026-11-24").unwrap();
    assert_eq!(notes::day_start_ms("1970-01-02"), Some(86_400_000));
    assert!(notes::until_ms("2026-11-30", now).is_some());
    assert!(notes::until_ms("2026-02-30", now).is_none());
    assert!(notes::until_ms("2026-11-01", now).is_none());
    assert!(
        notes::until_ms("2030-01-01", now).is_none(),
        "within two years"
    );
    assert!(notes::until_ms("next week", now).is_none());
    assert!(notes::secret("Card 4111 1111 1111 1111 on file"));
    assert!(notes::secret("token ghp_a1b2c3d4e5f6g7h8i9j0k1l2m3"));
    assert!(!notes::secret("Acme pays 45 days after invoice 2291."));
    assert_eq!(
        any_words("Acme's terms?", 5).as_deref(),
        Some("acme:* | terms:*")
    );
}
