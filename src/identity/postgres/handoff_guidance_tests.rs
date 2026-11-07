//! A channel task is told when to hand off and which helpers it can ask; other
//! conversations get no handoff guidance. A handoff task may name agents.
use super::super::{
    handoff_tests::{Setup, claim, setup},
    tests::*,
};
use super::*;
use crate::identity::chat::execution::prepare;

async fn add_to_channel(db: &PgStore, w: &str, chat: &str, agent: &str) {
    execute(
        &db.db,
        "INSERT INTO relay_chat_agents(workspace,chat,agent) VALUES($1,$2,$3)",
        vec![w.into(), chat.into(), agent.into()],
    )
    .await
    .unwrap();
}
async fn conversation(s: &Setup, kind: &str) -> String {
    s.db.chat_create(
        &s.owner,
        &s.w,
        &crate::identity::chat::model::NewChat {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Only {kind}"),
            kind: kind.into(),
            description: String::new(),
            users: vec![],
            agents: vec![s.a.clone()],
        },
    )
    .await
    .unwrap()
}
/// The instructions the sender agent gets for a new request in `chat`.
async fn instructions_in(s: &Setup, chat: &str) -> String {
    let job =
        s.db.chat_send(&s.owner, &s.w, chat, &message("Ask Atlas", Some(&s.a)))
            .await
            .unwrap()
            .unwrap();
    prepare(&s.db, &claim(&s.db, &job).await, 180)
        .await
        .unwrap()
        .instructions
}
fn listed(instructions: &str, agent: &str) -> bool {
    instructions.contains(&format!("\n- Pool agent (ID {agent}): Bounded fixture"))
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn channel_tasks_list_their_eligible_helpers_and_other_conversations_get_no_handoff_guidance()
{
    let s = setup().await;
    let (_, c) = super::super::executor_slot_tests::agent(&s.db, &s.owner, &s.w).await;
    add_to_channel(&s.db, &s.w, &s.c, &c).await;
    let prepared = prepare(&s.db, &s.parent, 180).await.unwrap();
    let text = &prepared.instructions;
    assert!(text.contains("when someone asks you to ask, get something from"));
    assert!(text.contains("write @Name in your reply: that does not reach it"));
    assert!(listed(text, &s.b) && listed(text, &c));
    assert!(!text.contains(&format!("(ID {})", s.a)), "never itself");
    // The list is what handoff_targets returns.
    let targets = s.db.handoff_targets(&s.parent).await.unwrap();
    let mut ids: Vec<&str> = targets["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    ids.sort();
    let mut expected = vec![s.b.as_str(), c.as_str()];
    expected.sort();
    assert_eq!(ids, expected);

    // A helper never lists the agent that asked it: that would be a cycle.
    let (_, child) = s.handoff().await;
    let child = claim(&s.db, &child).await;
    let helper = prepare(&s.db, &child, 180).await.unwrap().instructions;
    assert!(listed(&helper, &c) && !helper.contains(&format!("(ID {})", s.a)));

    // A direct message has no handoff guidance at all.
    let direct = instructions_in(&s, &conversation(&s, "direct").await).await;
    assert!(!direct.contains("request_handoff") && !direct.contains("Asking other agents"));

    // A channel with no other managed agent says so, in one sentence.
    let solo = instructions_in(&s, &conversation(&s, "channel").await).await;
    assert!(solo.contains("there is no other managed shared agent in this channel"));
    assert!(!solo.contains("call request_handoff"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_handoff_task_may_name_other_agents_as_plain_text() {
    let s = setup().await;
    let task = format!(
        "[@Pool agent](mention:agent:{}) was asked for the stock count. Please send it; [@Pool agent](mention:agent:{}) can confirm.",
        s.a, s.b
    );
    let input = Request {
        key: "named-agents".into(),
        agent: s.b.clone(),
        task: task.clone(),
    };
    let saved = s.db.request_handoff(&s.parent, &input).await.unwrap();
    assert_eq!(saved["state"], "pending");
    let row = query(
        &s.db.db,
        "SELECT m.content,m.target FROM relay_chat_messages m JOIN relay_agent_handoffs h ON h.workspace=m.workspace AND h.request_message=m.id WHERE h.id=$1",
        vec![saved["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "content").unwrap(), task);
    // Only the handoff's target is asked.
    assert_eq!(row.try_get::<String>("", "target").unwrap(), s.b);
    // A person's message still asks only the agent it mentions.
    let person =
        s.db.chat_send(&s.owner, &s.w, &s.c, &message(&task, Some(&s.b)))
            .await;
    assert!(person.is_err());
}
