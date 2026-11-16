//! Agents invited into one thread of a person's chat with their own agent
//! (schema 125): who may be invited, where the work goes, and that a guest reads
//! its thread and nothing else of the chat.
use super::super::{handoff_tests::claim, tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::{Job, NewChat, NewMessage},
    handoffs::model::Request,
};
use serde_json::json;

async fn agent(db: &PgStore, owner: &User, w: &str, name: &str, audience: &str) -> String {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        role: "Pay, leave and policies".into(),
        model: "fixture".into(),
        audience: audience.into(),
        people: None,
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    input.id
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
fn mention(name: &str, id: &str) -> String {
    format!("[@{name}](mention:agent:{id})")
}
fn asking(text: &str, agent: &str) -> NewMessage {
    let mut sent = message(text, None);
    sent.agents = vec![agent.into()];
    sent
}
async fn job_of(db: &PgStore, reply: &str) -> Job {
    let payload: Value = query(
        &db.db,
        "SELECT payload FROM relay_chat_jobs WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "payload")
    .unwrap();
    claim(db, &serde_json::from_value(payload).unwrap()).await
}
async fn value(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT to_jsonb(({sql})) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}
struct Setup {
    db: PgStore,
    ravi: User,
    w: String,
    own: String,
    hr: String,
    chat: String,
}
async fn setup() -> Setup {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let w = workspace(&db, &ravi).await;
    let own = agent(&db, &ravi, &w, "My assistant", "only_me").await;
    let hr = agent(&db, &ravi, &w, "HR Desk", "everyone").await;
    let chat = direct(&db, &ravi, &w, &own).await;
    Setup {
        db,
        ravi,
        w,
        own,
        hr,
        chat,
    }
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn your_agent_works_with_the_agent_you_mention_in_a_thread_it_alone_can_read() {
    let s = setup().await;
    let db = &s.db;
    // Earlier private work in the same chat, which HR Desk must never read.
    let private = asking("My payslip PRIVATE-MAIL-4471 shows 62,400.", &s.own);
    let earlier = db
        .chat_send(&s.ravi, &s.w, &s.chat, &private)
        .await
        .unwrap()
        .unwrap();
    db.chat_finish(
        &claim(db, &earlier).await,
        "Noted the PRIVATE-MAIL-4471 figure.",
        "complete",
    )
    .await
    .unwrap();

    let ask = asking(
        &format!("{} please get this fixed.", mention("HR Desk", &s.hr)),
        &s.own,
    );
    let jobs = db
        .chat_send_all(&s.ravi, &s.w, &s.chat, &ask)
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1, "one request, for your own agent");
    assert_eq!(jobs[0].agent, s.own);
    // HR Desk is a guest of the thread under the message, never a chat member.
    assert_eq!(
        value(db, "SELECT count(*) FROM relay_thread_guests WHERE workspace=$1 AND chat=$2 AND thread=$3 AND agent=$4",
            vec![s.w.clone().into(), s.chat.clone().into(), ask.id.clone().into(), s.hr.clone().into()]).await,
        json!(1)
    );
    assert_eq!(
        value(
            db,
            "SELECT count(*) FROM relay_chat_agents WHERE chat=$1 AND agent=$2",
            vec![s.chat.clone().into(), s.hr.clone().into()]
        )
        .await,
        json!(0)
    );
    // Your agent answers in the thread under your message.
    assert_eq!(
        value(
            db,
            "SELECT thread FROM relay_chat_messages WHERE id=$1",
            vec![jobs[0].reply.clone().into()]
        )
        .await,
        json!(ask.id)
    );

    let parent = claim(db, &jobs[0]).await;
    let targets = db.handoff_targets(&parent).await.unwrap();
    assert_eq!(targets["agents"][0]["id"], s.hr.as_str());
    let helpers = db.handoff_helpers(&parent).await.unwrap().unwrap();
    assert!(helpers.invited);
    assert!(
        helpers
            .instructions()
            .contains("the person you work for mentioned the agents below")
    );
    let sent = db
        .request_handoff(
            &parent,
            &Request {
                key: "salary".into(),
                agent: s.hr.clone(),
                task: "HR Desk, can you check the 5,600 difference in Ravi's October pay?".into(),
            },
        )
        .await
        .unwrap();
    let child = job_of(db, sent["reply"].as_str().unwrap()).await;
    assert_eq!(child.agent, s.hr);
    // The exchange happens in the thread.
    assert_eq!(
        value(
            db,
            "SELECT thread FROM relay_chat_messages WHERE id=$1",
            vec![child.reply.clone().into()]
        )
        .await,
        json!(ask.id)
    );
    // HR Desk reads the thread, and nothing else of the chat.
    for prompt in [
        child.prompt.clone(),
        db.request_context(&child).await.unwrap().1,
    ] {
        assert!(prompt.contains("5,600 difference"));
        assert!(prompt.contains("please get this fixed"));
        assert!(!prompt.contains("PRIVATE-MAIL-4471"), "{prompt}");
    }
    let tx = db.read_tx(&s.w).await.unwrap();
    let wider = super::super::conversation_context::gather(
        &tx,
        super::super::conversation_context::RecentScope {
            workspace: &s.w,
            chat: &s.chat,
            agent: &s.hr,
            thread: None,
            main_only: false,
            since_join: false,
            limit: 50,
            before: i64::MAX,
            request: &child.reply,
        },
    )
    .await;
    assert!(wider.is_err(), "a guest can't read the whole chat");
    tx.commit().await.unwrap();
    assert_eq!(
        db.working_context(&child).await.unwrap()["available"],
        false
    );
    // What its executor is really given stays inside the thread too.
    let prepared = crate::identity::chat::execution::prepare(db, &child, 180)
        .await
        .unwrap();
    assert!(prepared.request.contains("5,600 difference"));
    assert!(!prepared.request.contains("PRIVATE-MAIL-4471"));
    assert!(!prepared.instructions.contains("PRIVATE-MAIL-4471"));
    // Your own agent is told whom you invited and to share only what is needed.
    let own_told = crate::identity::chat::execution::prepare(db, &parent, 180)
        .await
        .unwrap();
    assert!(
        own_told
            .instructions
            .contains("Working with the agents your person invited")
    );
    assert!(own_told.instructions.contains("(ID ".to_owned().as_str()));
    // It can't hand the work back to your agent or on to agents not invited.
    let back = db
        .request_handoff(
            &child,
            &Request {
                key: "back".into(),
                agent: s.own.clone(),
                task: "Please do it yourself.".into(),
            },
        )
        .await;
    assert!(back.is_err());
    // Its answer is saved in the thread.
    db.chat_finish(&child, "October was paid on the old salary.", "complete")
        .await
        .unwrap();
    assert_eq!(
        value(
            db,
            "SELECT state FROM relay_chat_messages WHERE id=$1",
            vec![child.reply.clone().into()]
        )
        .await,
        json!("complete")
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn only_agents_you_may_ask_join_only_your_own_agents_chat_and_only_that_thread() {
    let s = setup().await;
    let db = &s.db;
    let priya = user(db, "priya").await;
    join(db, &s.ravi, &s.w, &priya, "Member").await;
    let hers = agent(db, &priya, &s.w, "Priya's assistant", "only_me").await;
    let finance = agent(db, &s.ravi, &s.w, "Finance Desk", "everyone").await;
    let legal = agent(db, &s.ravi, &s.w, "Legal Desk", "everyone").await;
    let busy = agent(db, &s.ravi, &s.w, "Stock Desk", "everyone").await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET settings=settings||'{\"helper\":false}'::jsonb WHERE id=$1",
        vec![busy.clone().into()],
    )
    .await
    .unwrap();
    let send = |text: String| asking(&text, &s.own);

    // Someone else's only-me agent, an agent that takes no work from agents,
    // and more than two agents are refused, and nothing is saved.
    assert!(
        db.chat_send(
            &s.ravi,
            &s.w,
            &s.chat,
            &send(mention("Priya's assistant", &hers))
        )
        .await
        .is_err()
    );
    let refused = db
        .chat_send(&s.ravi, &s.w, &s.chat, &send(mention("Stock Desk", &busy)))
        .await
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("doesn't take work from other agents")
    );
    let three = format!(
        "{} {} {}",
        mention("HR Desk", &s.hr),
        mention("Finance Desk", &finance),
        mention("Legal Desk", &legal)
    );
    assert!(
        db.chat_send(&s.ravi, &s.w, &s.chat, &send(three))
            .await
            .is_err()
    );
    assert_eq!(
        value(db, "SELECT count(*) FROM relay_thread_guests", vec![]).await,
        json!(0)
    );

    // Two agents join one thread together; the same message retried changes nothing.
    let both = send(format!(
        "{} {} sort this out.",
        mention("HR Desk", &s.hr),
        mention("Finance Desk", &finance)
    ));
    db.chat_send(&s.ravi, &s.w, &s.chat, &both).await.unwrap();
    assert!(
        db.chat_send_all(&s.ravi, &s.w, &s.chat, &both)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        value(
            db,
            "SELECT count(*) FROM relay_thread_guests WHERE thread=$1",
            vec![both.id.clone().into()]
        )
        .await,
        json!(2)
    );
    // A third agent can't join that thread later.
    let mut later = send(mention("Legal Desk", &legal));
    later.thread = Some(both.id.clone());
    assert!(db.chat_send(&s.ravi, &s.w, &s.chat, &later).await.is_err());

    // A second matter without a mention stays in the main chat: your agent can't
    // hand it to HR Desk, who was invited only into the first thread.
    let other = db
        .chat_send(
            &s.ravi,
            &s.w,
            &s.chat,
            &send("And my leave balance?".into()),
        )
        .await
        .unwrap()
        .unwrap();
    let other = claim(db, &other).await;
    assert!(db.handoff_helpers(&other).await.unwrap().is_none());
    assert!(
        db.request_handoff(
            &other,
            &Request {
                key: "leave".into(),
                agent: s.hr.clone(),
                task: "Check leave.".into()
            }
        )
        .await
        .is_err()
    );

    // In a direct chat with a shared agent, or in a channel, a mention invites
    // nobody: every mentioned agent must still be asked there.
    let with_hr = direct(db, &s.ravi, &s.w, &s.hr).await;
    let mut there = message(&format!("{} help", mention("Finance Desk", &finance)), None);
    there.agents = vec![s.hr.clone()];
    assert!(db.chat_send(&s.ravi, &s.w, &with_hr, &there).await.is_err());
    assert_eq!(
        value(
            db,
            "SELECT count(*) FROM relay_thread_guests WHERE chat<>$1",
            vec![s.chat.clone().into()]
        )
        .await,
        json!(0)
    );
}
