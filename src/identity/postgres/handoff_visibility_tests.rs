//! D246: the people in a conversation see agents talking. A handoff asked on the
//! main timeline is posted there with the helper's answer; one asked in a thread
//! stays in that thread. Groups hand off like channels; direct messages can't.
//! What later requests remember: `conversation_context::handoff_memory_tests`.
use super::super::{
    handoff_authority_tests::{pause, rule},
    handoff_tests::*,
    tests::*,
};
use super::*;
use crate::identity::chat::{
    execution::prepare,
    model::{NewChat, NewMessage},
};
use sea_orm_migration::MigratorTrait;

/// The thread a message is in; `None` on the main timeline.
async fn thread_of(db: &PgStore, table: &str, id: &str) -> Option<String> {
    query(
        &db.db,
        &format!("SELECT thread FROM {table} WHERE id=$1"),
        vec![id.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "thread")
    .unwrap()
}
async fn request_of(db: &PgStore, receipt: &Value) -> String {
    query(
        &db.db,
        "SELECT request_message FROM relay_agent_handoffs WHERE id=$1",
        vec![receipt["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "request_message")
    .unwrap()
}
pub(in crate::identity::postgres) fn asking(agent: &str, key: &str, task: &str) -> Request {
    Request {
        key: key.into(),
        agent: agent.into(),
        task: task.into(),
    }
}
fn shown<'a>(page: &'a Value, id: &str) -> &'a Value {
    page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .unwrap()
}
/// The helper task a handoff receipt started, claimed to run.
pub(in crate::identity::postgres) async fn helper(db: &PgStore, w: &str, receipt: &Value) -> Job {
    let job = super::super::handoff_access::job(&db.db, w, receipt["reply"].as_str().unwrap())
        .await
        .unwrap();
    claim(db, &job).await
}
async fn conversation(s: &Setup, kind: &str, users: Vec<String>, agents: Vec<String>) -> String {
    s.db.chat_create(
        &s.owner,
        &s.w,
        &NewChat {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Kapoor {kind}"),
            kind: kind.into(),
            description: String::new(),
            users,
            agents,
        },
    )
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_main_timeline_handoff_shows_the_request_and_answer_there_to_channel_members() {
    let s = setup().await;
    let member = user(&s.db, "visible-handoff-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    s.db.channel_members(
        &s.owner,
        &s.w,
        &s.c,
        &[s.owner.id.clone(), member.id.clone()],
    )
    .await
    .unwrap();
    let task = "Nova, can you check the walnut stock for Kapoor?";
    let receipt =
        s.db.request_handoff(&s.parent, &asking(&s.b, "walnut", task))
            .await
            .unwrap();
    let request = request_of(&s.db, &receipt).await;
    let answer = receipt["reply"].as_str().unwrap().to_owned();
    assert_eq!(
        thread_of(&s.db, "relay_chat_messages", &request).await,
        None
    );
    assert_eq!(thread_of(&s.db, "relay_chat_messages", &answer).await, None);
    let id = receipt["id"].as_str().unwrap();
    assert_eq!(thread_of(&s.db, "relay_agent_handoffs", id).await, None);
    s.db.chat_finish(
        &s.parent,
        "I asked Nova; waiting for the count.",
        "complete",
    )
    .await
    .unwrap();
    let child = helper(&s.db, &s.w, &receipt).await;
    s.db.chat_finish(&child, "WALNUT_STOCK_42 boards for Kapoor.", "complete")
        .await
        .unwrap();

    // A member who did not ask reads the whole exchange on the main timeline.
    let page = s.db.chat_page(&member, &s.w, &s.c, None).await.unwrap();
    let posted = shown(&page, &request);
    assert_eq!(posted["thread"], Value::Null);
    assert_eq!(
        (posted["kind"].as_str(), posted["author"].as_str()),
        (Some("agent"), Some(s.a.as_str()))
    );
    assert_eq!(posted["content"], task);
    assert_eq!(posted["handoff"]["target"], s.b.as_str());
    assert_eq!(posted["handoff"]["state"], "complete");
    let answered = shown(&page, &answer);
    assert_eq!(answered["thread"], Value::Null);
    assert!(
        answered["content"]
            .as_str()
            .unwrap()
            .contains("WALNUT_STOCK_42")
    );
    assert_eq!(answered["coordination"]["incoming"]["sender"], s.a.as_str());
    let outgoing = &shown(&page, &s.parent.reply)["coordination"]["outgoing"][0];
    assert_eq!(outgoing["thread"], Value::Null);
    assert_eq!(outgoing["request_message"], request.as_str());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_handoff_asked_in_a_thread_stays_in_that_thread() {
    let s = setup().await;
    let root = message("Kapoor's order", None);
    s.db.chat_send(&s.owner, &s.w, &s.c, &root).await.unwrap();
    let asked = NewMessage {
        thread: Some(root.id.clone()),
        ..message("Ask the other agent about walnut", Some(&s.a))
    };
    let job =
        s.db.chat_send(&s.owner, &s.w, &s.c, &asked)
            .await
            .unwrap()
            .unwrap();
    let parent = claim(&s.db, &job).await;
    let receipt =
        s.db.request_handoff(&parent, &request(&s.b, "in-thread"))
            .await
            .unwrap();
    let request = request_of(&s.db, &receipt).await;
    let id = receipt["id"].as_str().unwrap();
    for (table, id) in [
        ("relay_chat_messages", request.as_str()),
        ("relay_chat_messages", receipt["reply"].as_str().unwrap()),
        ("relay_agent_handoffs", id),
    ] {
        assert_eq!(
            thread_of(&s.db, table, id).await.as_deref(),
            Some(root.id.as_str())
        );
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn group_rooms_hand_off_with_the_channel_rules_and_direct_messages_cannot() {
    let s = setup().await;
    let member = user(&s.db, "group-handoff-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    let group = conversation(
        &s,
        "group",
        vec![member.id.clone()],
        vec![s.a.clone(), s.b.clone()],
    )
    .await;
    let job =
        s.db.chat_send(
            &member,
            &s.w,
            &group,
            &message("Ask about walnut", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    let parent = claim(&s.db, &job).await;
    // Preparing a run reads the agent's profile with the member's own access.
    s.db.chat_grant(&s.owner, &s.w, &member.id, &s.a, true)
        .await
        .unwrap();
    let guidance = prepare(&s.db, &parent, 180).await.unwrap().instructions;
    assert!(
        guidance.contains("call request_handoff") && guidance.contains(&format!("(ID {})", s.b))
    );
    assert!(guidance.contains("the people here read both"));
    let targets = s.db.handoff_targets(&parent).await.unwrap()["agents"].clone();
    assert_eq!(targets.as_array().unwrap().len(), 1);
    assert_eq!(targets[0]["id"], s.b.as_str());

    // The same eligibility as a channel: the person may ask the helper now, it
    // is in this group, and an agent never asks itself.
    pause(&s, &s.b, true).await;
    assert!(
        s.db.handoff_targets(&parent).await.unwrap()["agents"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        s.db.request_handoff(&parent, &request(&s.b, "paused"))
            .await
            .is_err()
    );
    pause(&s, &s.b, false).await;
    let (_, outsider) = super::super::executor_slot_tests::agent(&s.db, &s.owner, &s.w).await;
    assert!(
        s.db.request_handoff(&parent, &request(&outsider, "outsider"))
            .await
            .is_err()
    );
    assert!(
        s.db.request_handoff(&parent, &request(&s.a, "self"))
            .await
            .is_err()
    );

    let receipt =
        s.db.request_handoff(&parent, &request(&s.b, "group-helper"))
            .await
            .unwrap();
    let child = helper(&s.db, &s.w, &receipt).await;
    assert_eq!(
        (child.chat.as_str(), child.requester.as_str()),
        (group.as_str(), member.id.as_str())
    );
    assert_eq!(
        thread_of(
            &s.db,
            "relay_chat_messages",
            &request_of(&s.db, &receipt).await
        )
        .await,
        None
    );
    assert!(
        s.db.request_handoff(&child, &request(&s.a, "back"))
            .await
            .is_err(),
        "no cycle"
    );

    // A direct message with an agent has no other agent to ask and no guidance.
    let direct = conversation(&s, "direct", vec![], vec![s.a.clone()]).await;
    let job =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &direct,
            &message("Ask about walnut", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    let private = claim(&s.db, &job).await;
    assert!(s.db.handoff_targets(&private).await.is_err());
    assert!(
        s.db.request_handoff(&private, &request(&s.b, "direct"))
            .await
            .is_err()
    );
    let guidance = prepare(&s.db, &private, 180).await.unwrap().instructions;
    assert!(!guidance.contains("Asking other agents"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_source_bearing_exchange_is_on_the_main_timeline_and_redacted_when_source_access_changes()
{
    let (s, conn, _) = super::super::handoff_source_tests::event_setup().await;
    rule(&s, &s.b, &conn, "order.read", "allow", 0).await;
    let receipt =
        s.db.request_handoff(
            &s.parent,
            &asking(&s.b, "event", "PRIVATE_EVENT_ASK check order"),
        )
        .await
        .unwrap();
    let request = request_of(&s.db, &receipt).await;
    let answer = receipt["reply"].as_str().unwrap().to_owned();
    s.db.chat_finish(&s.parent, "Waiting.", "complete")
        .await
        .unwrap();
    let child = helper(&s.db, &s.w, &receipt).await;
    s.db.chat_finish(&child, "PRIVATE_EVENT_ANSWER", "complete")
        .await
        .unwrap();
    let page = s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap();
    assert_eq!(shown(&page, &request)["thread"], Value::Null);
    assert_eq!(shown(&page, &answer)["thread"], Value::Null);
    assert!(page.to_string().contains("PRIVATE_EVENT_ANSWER"));

    // Without source access the viewer sees the same places, redacted as before.
    rule(&s, &s.b, &conn, "order.read", "block", 1).await;
    let page = s.db.chat_page(&s.owner, &s.w, &s.c, None).await.unwrap();
    let posted = shown(&page, &request);
    assert_eq!(posted["thread"], Value::Null);
    assert_eq!(
        posted["content"],
        "Handoff unavailable because source access changed."
    );
    assert!(posted.get("handoff").is_none_or(Value::is_null));
    assert_eq!(shown(&page, &answer)["response_unavailable"], true);
    for marker in [
        "PRIVATE_EVENT_ASK",
        "PRIVATE_EVENT_ANSWER",
        "PRIVATE-ORDER-42",
    ] {
        assert!(!page.to_string().contains(marker), "{marker}");
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_100_keeps_earlier_handoffs_and_lets_new_ones_stay_on_the_main_timeline() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_visible_handoffs_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let mut db = PgStore::connect(scoped.as_str()).await.unwrap();
    db.pooled = true;
    super::super::migration::Migrator::up(&db.db, Some(99))
        .await
        .unwrap();
    assert!(db.verify_schema().await.is_err(), "schema 99 is refused");
    let owner = user(&db, "visible-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (_, b) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let c = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Upgrade handoffs".into(),
                kind: "channel".into(),
                description: String::new(),
                users: vec![],
                agents: vec![a.clone(), b.clone()],
            },
        )
        .await
        .unwrap();
    // Before schema 100 every receipt had a thread; one asked in a thread still does.
    let root = message("Kapoor's order", None);
    db.chat_send(&owner, &w, &c, &root).await.unwrap();
    let asked = NewMessage {
        thread: Some(root.id.clone()),
        ..message("Ask in the thread", Some(&a))
    };
    let job = db.chat_send(&owner, &w, &c, &asked).await.unwrap().unwrap();
    let earlier = db
        .request_handoff(&claim(&db, &job).await, &request(&b, "earlier"))
        .await
        .unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let id = earlier["id"].as_str().unwrap();
    assert_eq!(
        thread_of(&db, "relay_agent_handoffs", id).await,
        Some(root.id.clone())
    );
    let job = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message("Ask on the main timeline", Some(&a)),
        )
        .await
        .unwrap()
        .unwrap();
    let later = db
        .request_handoff(&claim(&db, &job).await, &request(&b, "later"))
        .await
        .unwrap();
    let id = later["id"].as_str().unwrap();
    assert_eq!(thread_of(&db, "relay_agent_handoffs", id).await, None);
}
