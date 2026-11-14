//! Connected systems (schema 116): an agent's owner connects a company's own
//! MCP server without an operator, and its tools follow the reviewed rules.
use super::super::tests::*;
use super::*;
use crate::identity::connectors::{
    fake_system::{self, FakeSystem},
    systems::{self, Address},
};

pub(crate) fn choice(name: &str, kind: &str, rule: &str) -> ToolChoice {
    ToolChoice {
        name: name.into(),
        kind: kind.into(),
        rule: rule.into(),
        subject: false,
    }
}

/// Mirrors the route: lists the tools now and builds the reviewed actions.
pub(crate) async fn reviewed(w: &str, input: &NewSystem) -> Reviewed {
    let address = Address {
        url: input.url.clone(),
        key: input.key.clone(),
    };
    let listed = systems::list(w, &address).await.unwrap();
    Reviewed {
        operations: input
            .tools
            .iter()
            .map(|c| {
                listed
                    .iter()
                    .find(|t| t.name == c.name)
                    .unwrap()
                    .operation(&c.kind)
                    .unwrap()
            })
            .collect(),
        binding: address
            .binding(w, &systems::server_name(&input.id))
            .unwrap(),
    }
}

pub(crate) fn studyhub(url: &str) -> NewSystem {
    NewSystem {
        id: uuid::Uuid::new_v4().to_string(),
        name: "StudyHub".into(),
        url: url.into(),
        key: Some(fake_system::KEY.into()),
        tools: vec![
            choice("get_marks", "read", "allow"),
            choice("list_homework", "read", "allow"),
            choice("save_study_plan", "write", "ask"),
        ],
    }
}

async fn effective(db: &PgStore, w: &str, agent: &str, connection: &str) -> Vec<(String, String)> {
    rows(&db.db, "SELECT operation,mode FROM relay_control_effective_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 ORDER BY operation",
        vec![w.into(), agent.into(), connection.into()]).await.unwrap()
        .iter()
        .map(|r| (r.try_get("", "operation").unwrap(), r.try_get("", "mode").unwrap()))
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_owner_connects_a_system_and_its_tools_follow_the_reviewed_rules() {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let (db, owner, w, _c, agent) = super::super::automation_tests::setup().await;
    let input = studyhub(&fake.url);
    let reviewed = reviewed(&w, &input).await;
    let saved = db
        .connect_system(&owner, &w, &agent, &input, &reviewed)
        .await
        .unwrap();
    assert_eq!(saved["state"], "connected");
    assert_eq!(saved["has_key"], true);
    assert!(!saved.to_string().contains(fake_system::KEY));
    // The same request again is the same system; the same id otherwise is refused.
    let again = db
        .connect_system(&owner, &w, &agent, &input, &reviewed)
        .await
        .unwrap();
    assert_eq!(again["id"], saved["id"]);
    let mut other = input.clone();
    other.tools.pop();
    assert!(
        db.connect_system(&owner, &w, &agent, &other, &reviewed)
            .await
            .is_err()
    );
    // Reads are allowed, the change asks first, and nothing else is offered.
    assert_eq!(
        effective(&db, &w, &agent, &input.id).await,
        vec![
            ("get_marks".into(), "allow".into()),
            ("list_homework".into(), "allow".into()),
            ("save_study_plan".into(), "ask".into())
        ]
    );
    let row = query(&db.db, "SELECT provider,binding::text AS binding FROM relay_control_connections WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), input.id.clone().into()]).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "provider").unwrap(), "system");
    assert!(
        !row.try_get::<String>("", "binding")
            .unwrap()
            .contains(fake_system::KEY)
    );
    let listed = db.agent_systems(&owner, &w, &agent).await.unwrap();
    assert_eq!(listed["can_connect"], true);
    assert_eq!(listed["systems"].as_array().unwrap().len(), 1);

    // Switching it off stops every tool; on brings them back.
    let revision = |db: &PgStore| {
        let (db, owner, w, agent) = (db.clone(), owner.clone(), w.clone(), agent.clone());
        async move {
            db.agent_systems(&owner, &w, &agent).await.unwrap()["systems"][0]["revision"]
                .as_i64()
                .unwrap()
        }
    };
    db.change_system(&owner, &w, &agent, &input.id, revision(&db).await, "off")
        .await
        .unwrap();
    assert!(effective(&db, &w, &agent, &input.id).await.is_empty());
    assert_eq!(
        db.agent_systems(&owner, &w, &agent).await.unwrap()["systems"][0]["state"],
        "off"
    );
    assert!(
        db.change_system(&owner, &w, &agent, &input.id, 1, "on")
            .await
            .is_err(),
        "a stale revision is refused"
    );
    db.change_system(&owner, &w, &agent, &input.id, revision(&db).await, "on")
        .await
        .unwrap();
    assert_eq!(effective(&db, &w, &agent, &input.id).await.len(), 3);

    // A new review switches a tool off and makes another stricter.
    let tools = vec![
        choice("get_marks", "read", "ask"),
        choice("save_study_plan", "write", "block"),
    ];
    let mut review = input.clone();
    review.tools = tools.clone();
    let reviewed = reviewed_again(&w, &review).await;
    let current = revision(&db).await;
    db.review_system(
        &owner,
        &w,
        &agent,
        &input.id,
        &SystemReview {
            revision: current,
            tools: tools.clone(),
        },
        &reviewed,
    )
    .await
    .unwrap();
    assert_eq!(
        effective(&db, &w, &agent, &input.id).await,
        vec![("get_marks".into(), "ask".into())]
    );
    // Removing it leaves nothing for the agent.
    db.change_system(&owner, &w, &agent, &input.id, revision(&db).await, "remove")
        .await
        .unwrap();
    assert!(effective(&db, &w, &agent, &input.id).await.is_empty());
    assert!(
        db.agent_systems(&owner, &w, &agent).await.unwrap()["systems"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        fake.state.lock().unwrap().calls.is_empty(),
        "connecting sends no tool calls"
    );
}

async fn reviewed_again(w: &str, input: &NewSystem) -> Reviewed {
    reviewed(w, input).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_the_owner_or_an_admin_connects_or_changes_a_system() {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let (db, owner, w, _c, agent) = super::super::automation_tests::setup().await;
    let member = user(&db, "studyhub-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let input = studyhub(&fake.url);
    let reviewed = reviewed(&w, &input).await;
    assert!(
        db.connect_system(&member, &w, &agent, &input, &reviewed)
            .await
            .is_err()
    );
    db.connect_system(&owner, &w, &agent, &input, &reviewed)
        .await
        .unwrap();
    assert!(
        db.change_system(&member, &w, &agent, &input.id, 1, "off")
            .await
            .is_err()
    );
    assert!(
        db.system_binding(&member, &w, &agent, &input.id)
            .await
            .is_err()
    );
    // A member who may talk to the agent sees the tile, never the key.
    if let Ok(seen) = db.agent_systems(&member, &w, &agent).await {
        assert_eq!(seen["can_connect"], false);
        assert!(!seen.to_string().contains(fake_system::KEY));
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn another_agents_owner_cannot_take_a_connected_system() {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let (db, owner, w, _c, agent) = super::super::automation_tests::setup().await;
    let input = studyhub(&fake.url);
    let reviewed = reviewed(&w, &input).await;
    db.connect_system(&owner, &w, &agent, &input, &reviewed)
        .await
        .unwrap();
    let member = user(&db, "other-agent-owner").await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_, other) = super::super::executor_slot_tests::agent(&db, &member, &w).await;
    // It isn't offered on the other agent's Tools tab, and switching it on is refused.
    if let Ok(tools) = db.control_snapshot(&member, &w, &other).await {
        assert!(
            !tools["connections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == input.id.as_str())
        );
    }
    let on = crate::identity::control::model::AgentConnection {
        connection: input.id.clone(),
        enabled: true,
    };
    assert!(
        db.control_agent_connection(&member, &w, &other, &on)
            .await
            .is_err()
    );
    // An admin may.
    db.control_agent_connection(&owner, &w, &other, &on)
        .await
        .unwrap();
}
