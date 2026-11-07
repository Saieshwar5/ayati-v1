//! One message asks up to three agents, each answering separately: one reply and
//! job per agent, all or nothing at intake, and every later lookup by the asking
//! message (replay, delete, retry, receipts, context) handles each job.
use super::super::{
    ModelCall,
    model_budgets::BudgetInput,
    tests::{fixture, join, message, user, workspace},
    *,
};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::{Job, NewChat, NewMessage},
};
use sea_orm_migration::MigratorTrait;

#[path = "several_agents_lookup_tests.rs"]
mod lookups;

async fn agent(db: &PgStore, owner: &User, w: &str, name: &str) -> String {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        role: "Stock checks".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        managers: None,
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    input.id
}

struct Room {
    db: PgStore,
    owner: User,
    member: User,
    w: String,
    c: String,
    atlas: String,
    nova: String,
    orion: String,
    vega: String,
}

/// A channel with the owner, a member and four agents.
async fn room() -> Room {
    let db = fixture().await;
    let owner = user(&db, "several-owner").await;
    let member = user(&db, "several-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let atlas = agent(&db, &owner, &w, "Atlas").await;
    let nova = agent(&db, &owner, &w, "Nova").await;
    let orion = agent(&db, &owner, &w, "Orion").await;
    let vega = agent(&db, &owner, &w, "Vega").await;
    let c = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Stock".into(),
                kind: "channel".into(),
                description: String::new(),
                users: vec![member.id.clone()],
                agents: vec![atlas.clone(), nova.clone(), orion.clone(), vega.clone()],
            },
        )
        .await
        .unwrap();
    Room {
        db,
        owner,
        member,
        w,
        c,
        atlas,
        nova,
        orion,
        vega,
    }
}

fn mention(name: &str, id: &str) -> String {
    format!("[@{name}](mention:agent:{id})")
}

/// A message that mentions and asks `agents` (name, id) in order.
fn ask(text: &str, agents: &[(&str, &str)]) -> NewMessage {
    let names: Vec<String> = agents.iter().map(|(n, id)| mention(n, id)).collect();
    NewMessage {
        agents: agents.iter().map(|(_, id)| id.to_string()).collect(),
        ..message(&format!("{} {text}", names.join(" ")), None)
    }
}

impl Room {
    async fn count(&self, sql: &str, id: &str) -> i64 {
        query(&self.db.db, sql, vec![id.into()])
            .await
            .unwrap()
            .unwrap()
            .try_get("", "n")
            .unwrap()
    }
    async fn jobs(&self, request: &str) -> i64 {
        self.count(
            "SELECT count(*) AS n FROM relay_chat_jobs WHERE request_message=$1",
            request,
        )
        .await
    }
    /// Nothing is saved for a refused message: no message, job or reply.
    async fn nothing_saved(&self, input: &NewMessage) {
        let saved = self
            .count(
                "SELECT count(*) AS n FROM relay_chat_messages WHERE id=$1",
                &input.id,
            )
            .await;
        assert_eq!((saved, self.jobs(&input.id).await), (0, 0));
        let replies = self
            .count(
                "SELECT count(*) AS n FROM relay_chat_messages WHERE chat=$1 AND kind='agent'",
                &self.c,
            )
            .await;
        assert_eq!(replies, 0, "no reply is left waiting");
    }
    async fn refused(&self, input: &NewMessage) -> String {
        let error = self
            .db
            .chat_send_all(&self.member, &self.w, &self.c, input)
            .await
            .unwrap_err()
            .to_string();
        self.nothing_saved(input).await;
        error
    }
    async fn state(&self, reply: &str) -> (String, String) {
        let row = query(&self.db.db, "SELECT j.state AS job,m.state AS reply FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE j.reply=$1", vec![reply.into()])
            .await
            .unwrap()
            .unwrap();
        (
            row.try_get("", "job").unwrap(),
            row.try_get("", "reply").unwrap(),
        )
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_message_to_two_agents_gives_each_its_own_reply_and_job_once() {
    let r = room().await;
    let input = ask(
        "how many boxes are left?",
        &[("Atlas", &r.atlas), ("Nova", &r.nova)],
    );
    let jobs =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &input)
            .await
            .unwrap();
    let asked: Vec<&str> = jobs.iter().map(|job| job.agent.as_str()).collect();
    assert_eq!(
        asked,
        [r.atlas.as_str(), r.nova.as_str()],
        "in the order asked"
    );
    assert_ne!(jobs[0].reply, jobs[1].reply);
    assert!(jobs.iter().all(|job| job.requester == r.member.id));
    assert_eq!(r.jobs(&input.id).await, 2);
    // Each reply is its own queued message, written by its own agent.
    let history = r.db.chat_messages(&r.member, &r.w, &r.c).await.unwrap();
    let replies: Vec<(String, String)> = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kind"] == "agent")
        .map(|m| {
            (
                m["author"].as_str().unwrap().into(),
                m["state"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        replies,
        [
            (r.atlas.clone(), "queued".to_string()),
            (r.nova.clone(), "queued".to_string())
        ]
    );
    let target: Option<String> = query(
        &r.db.db,
        "SELECT target FROM relay_chat_messages WHERE id=$1",
        vec![input.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "target")
    .unwrap();
    assert_eq!(
        target.as_deref(),
        Some(r.atlas.as_str()),
        "the first agent asked"
    );

    // The outbox's exact replay starts nothing new.
    let again =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &input)
            .await
            .unwrap();
    assert!(again.is_empty());
    assert_eq!(r.jobs(&input.id).await, 2);
    // The same ID asking a different set of agents is not a replay.
    let fewer = NewMessage {
        id: input.id.clone(),
        agents: vec![r.atlas.clone()],
        ..message(&input.content, None)
    };
    let error =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &fewer)
            .await
            .unwrap_err();
    assert!(
        error.to_string().contains("Message ID already used"),
        "{error}"
    );
    assert_eq!(r.jobs(&input.id).await, 2);

    // The database keeps one job per message and agent, and allows another agent.
    let copy = |agent: &str| {
        let reply = uuid::Uuid::new_v4();
        format!("INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES('{reply}','{w}','{c}','{agent}','agent','','queued',1);
             INSERT INTO relay_chat_jobs(workspace,reply,payload,request_message) SELECT workspace,'{reply}',jsonb_set(payload,'{{agent}}',to_jsonb('{agent}'::text)),request_message FROM relay_chat_jobs WHERE reply='{first}'",
            w = r.w, c = r.c, first = jobs[0].reply)
    };
    assert!(
        r.db.db.execute_unprepared(&copy(&r.atlas)).await.is_err(),
        "Atlas twice"
    );
    r.db.db.execute_unprepared(&copy(&r.orion)).await.unwrap();
    assert_eq!(r.jobs(&input.id).await, 3);

    // Three agents is the most one message asks; a fourth refuses it all.
    let three = ask(
        "check the count",
        &[("Atlas", &r.atlas), ("Nova", &r.nova), ("Orion", &r.orion)],
    );
    let jobs =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &three)
            .await
            .unwrap();
    assert_eq!(jobs.len(), 3);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn one_agent_that_cannot_take_it_refuses_the_whole_message_by_name() {
    let r = room().await;
    // A fourth agent refuses the message before anything is checked or saved.
    let four = ask(
        "everyone check",
        &[
            ("Atlas", &r.atlas),
            ("Nova", &r.nova),
            ("Orion", &r.orion),
            ("Vega", &r.vega),
        ],
    );
    assert!(
        r.refused(&four)
            .await
            .contains("Ask at most 3 agents in one message")
    );

    // An agent the member may not ask.
    execute(
        &r.db.db,
        "UPDATE relay_shared_agents SET audience='only_me' WHERE id=$1",
        vec![r.vega.clone().into()],
    )
    .await
    .unwrap();
    let error = r
        .refused(&ask("stock?", &[("Atlas", &r.atlas), ("Vega", &r.vega)]))
        .await;
    assert_eq!(
        error,
        "Vega can't take this message: Agent is not shared with you. Nothing was sent; remove @Vega to ask the others."
    );

    // A paused agent says so.
    r.db.agent_pause_set(&r.owner, &r.w, &r.nova, true)
        .await
        .unwrap();
    let error = r
        .refused(&ask("stock?", &[("Atlas", &r.atlas), ("Nova", &r.nova)]))
        .await;
    assert!(
        error.starts_with("Nova can't take this message: This agent is paused."),
        "{error}"
    );
    // Asking only the paused agent refuses it with the same reason.
    let error = r.refused(&ask("stock?", &[("Nova", &r.nova)])).await;
    assert_eq!(
        error,
        "This agent is paused. Its manager or an admin can resume it."
    );

    // An agent that used its monthly budget, named with the others refused.
    r.db.save_model_budget(
        &r.owner,
        &r.w,
        &BudgetInput {
            agent: r.orion.clone(),
            monthly_usd: Some(1.0),
            warn_percent: 80,
        },
    )
    .await
    .unwrap();
    r.db.record_model_usage(&ModelCall {
        workspace: &r.w,
        assignment: "fixture-assignment",
        agent: &r.orion,
        job: None,
        provider: "fireworks",
        model: "llama-70b",
        provider_model: "llama",
        attempts: 1,
        input_tokens: 1,
        output_tokens: 1,
        person: None,
        charge_micro: 1_000_000,
        cost_micro: 0,
        latency_ms: None,
    })
    .await
    .unwrap();
    let error = r
        .refused(&ask(
            "stock?",
            &[("Atlas", &r.atlas), ("Orion", &r.orion), ("Nova", &r.nova)],
        ))
        .await;
    assert!(
        error.contains(
            "Orion can't take this message: This agent has used its monthly model budget."
        ),
        "{error}"
    );
    assert!(
        error.contains("Nova can't take this message: This agent is paused."),
        "{error}"
    );
    assert!(
        error.ends_with("remove @Orion and @Nova to ask the others."),
        "{error}"
    );

    // Every mentioned agent must be asked.
    let unasked = NewMessage {
        agents: vec![r.atlas.clone()],
        ..ask("stock?", &[("Atlas", &r.atlas), ("Nova", &r.nova)])
    };
    assert!(
        r.refused(&unasked)
            .await
            .contains("mentions an agent it doesn't ask")
    );

    // Each asked agent needs a place in the queue.
    r.db.agent_pause_set(&r.owner, &r.w, &r.nova, false)
        .await
        .unwrap();
    execute(&r.db.db, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) SELECT gen_random_uuid()::text,$1,$2,$3,'agent','','queued',1 FROM generate_series(1,63)",
        vec![r.w.clone().into(), r.c.clone().into(), r.atlas.clone().into()]).await.unwrap();
    let full = ask("stock?", &[("Atlas", &r.atlas), ("Nova", &r.nova)]);
    let error =
        r.db.chat_send_all(&r.member, &r.w, &r.c, &full)
            .await
            .unwrap_err();
    assert!(error.is::<crate::identity::chat::model::QueueFull>());
    assert_eq!(r.jobs(&full.id).await, 0);
    let one = ask("stock?", &[("Nova", &r.nova)]);
    assert_eq!(
        r.db.chat_send_all(&r.member, &r.w, &r.c, &one)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_99_keeps_earlier_requests_and_allows_one_job_per_agent() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_several_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    super::super::migration::Migrator::up(&db.db, Some(98))
        .await
        .unwrap();
    assert!(db.verify_schema().await.is_err(), "schema 98 is refused");
    let owner = user(&db, "several-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let atlas = agent(&db, &owner, &w, "Atlas").await;
    let nova = agent(&db, &owner, &w, "Nova").await;
    let c =
        super::super::tests::chat(&db, &owner, &w, vec![], vec![atlas.clone(), nova.clone()]).await;
    // A request asked before the upgrade, one agent per message.
    let earlier = ask("stock?", &[("Atlas", &atlas)]);
    let job = db
        .chat_send(&owner, &w, &c, &earlier)
        .await
        .unwrap()
        .unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let kept: String = query(
        &db.db,
        "SELECT request_message FROM relay_chat_jobs WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "request_message")
    .unwrap();
    assert_eq!(kept, earlier.id);
    assert!(
        db.chat_send_all(&owner, &w, &c, &earlier)
            .await
            .unwrap()
            .is_empty(),
        "its replay starts nothing"
    );
    let both = ask("and pallets?", &[("Atlas", &atlas), ("Nova", &nova)]);
    assert_eq!(
        db.chat_send_all(&owner, &w, &c, &both).await.unwrap().len(),
        2
    );
}
