//! Taking turns on computers (schema 106) and operating them: requests wait in line
//! instead of being turned away, a workspace can limit awake computers, a command
//! never outlasts its request, replies say what was done, and operators see every
//! computer and can pause them all.
use super::computer_tests::{ask, computer, setup};
use super::operator::{Operator, Via, settings};
use super::tests::*;
use super::*;
use crate::identity::agents::computer_policy::ComputerPolicy;
use crate::identity::control::broker::invoke;
use serde_json::json;

async fn run(
    s: &super::computer_tests::Setup,
    job: &crate::identity::chat::model::Job,
    command: &str,
) -> Value {
    invoke(&s.db, job, "computer_run", json!({"command": command}))
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_second_request_waits_its_turn_and_says_what_it_did() {
    let s = setup().await;
    computer(&s, json!({"on": true, "size": "small"})).await;
    let first = ask(&s, "Count the invoices").await;
    assert_eq!(run(&s, &first, "echo first").await["exit_code"], 0);
    // While the first holds the computer, the second waits, then is told its place.
    let second = ask(&s, "Convert the PDF").await;
    let waiting = run(&s, &second, "echo second").await;
    assert_eq!(waiting["state"], "unavailable");
    assert!(
        waiting["message"]
            .as_str()
            .unwrap()
            .contains("number 1 in line"),
        "{waiting}"
    );
    // When the first finishes during the wait, the second gets the computer.
    let db = s.db.clone();
    let done = first.clone();
    let finishing = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        db.chat_finish(&done, "Counted.", "complete").await.unwrap();
    });
    let ran = run(&s, &second, "echo second").await;
    finishing.await.unwrap();
    assert_eq!(ran["stdout"].as_str().unwrap().trim(), "second", "{ran}");
    let line: i64 = query(
        &s.db.db,
        "SELECT count(*)::bigint AS n FROM relay_computer_waits",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(line, 0, "nobody is left in line");
    // The finished reply says what it did on its computer, with the steps' address.
    let reply: String = query(
        &s.db.db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![first.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "content")
    .unwrap();
    assert_eq!(
        reply,
        format!(
            "Counted.\n\nUsed its computer: 1 command. [See the steps](/w/{}/?view=activity&tab=history&run={})",
            s.w, first.reply
        )
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_command_never_outlasts_its_request_and_the_workspace_limits_awake_computers() {
    let s = setup().await;
    computer(&s, json!({"on": true, "size": "small"})).await;
    let job = ask(&s, "Run the long report").await;
    // The request started 170 of its 180 seconds ago: the command gets 5.
    execute(&s.db.db, "INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,state,lease_seconds,started_ms)
        VALUES($1,$2,$3,1,'local','running',120,(extract(epoch FROM clock_timestamp())*1000)::bigint-170000)",
        vec![job.claim.clone().into(), s.w.clone().into(), job.reply.clone().into()]).await.unwrap();
    let started = std::time::Instant::now();
    let ran = invoke(
        &s.db,
        &job,
        "computer_run",
        json!({"command": "sleep 30", "seconds": 120}),
    )
    .await
    .unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert_eq!(ran["timed_out"], true, "{ran}");
    assert!(ran["note"].as_str().unwrap().contains("nearly up"));

    // One computer awake at a time: another agent's computer waits.
    s.db.save_computer_policy(
        &s.owner,
        &s.w,
        &ComputerPolicy {
            max_awake: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let (_, other) = super::executor_slot_tests::agent(&s.db, &s.owner, &s.w).await;
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{computer}',$2) WHERE id=$1",
        vec![
            other.clone().into(),
            json!({"on": true, "size": "small"}).into(),
        ],
    )
    .await
    .unwrap();
    let room = chat(&s.db, &s.owner, &s.w, vec![], vec![other.clone()]).await;
    s.db.chat_send(
        &s.owner,
        &s.w,
        &room,
        &message("Use your computer", Some(&other)),
    )
    .await
    .unwrap();
    let row = query(
        &s.db.db,
        "SELECT payload FROM relay_chat_jobs WHERE state='queued' ORDER BY sequence DESC LIMIT 1",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    let mut second: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    second.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&s.db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",
        vec![second.reply.clone().into(), second.claim.clone().into()]).await.unwrap();
    let full = invoke(&s.db, &second, "computer_run", json!({"command": "ls"}))
        .await
        .unwrap();
    assert!(
        full["message"]
            .as_str()
            .unwrap()
            .contains("Too many computers"),
        "{full}"
    );
    let admin = s.db.computer_policy(&s.owner, &s.w).await.unwrap();
    assert_eq!(admin["computers"].as_array().unwrap().len(), 2, "{admin}");
    let waiting: Vec<(String, i64)> = admin["computers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["agent"].as_str().unwrap().to_owned(),
                c["waiting"].as_i64().unwrap(),
            )
        })
        .collect();
    assert!(
        waiting.contains(&(other.clone(), 1)),
        "the other agent's request is in line: {waiting:?}"
    );
    assert!(waiting.contains(&(s.agent.clone(), 0)));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn operators_see_every_computer_and_can_pause_them_all() {
    let s = setup().await;
    computer(&s, json!({"on": true, "size": "small"})).await;
    let job = ask(&s, "List the files").await;
    run(&s, &job, "true").await;
    let seen = s.db.operator_computers().await.unwrap();
    assert_eq!(seen["computers"][0]["provider"], "e2b");
    assert_eq!(seen["computers"][0]["state"], "awake");
    assert_eq!(seen["providers"][0]["awake"], 1);
    assert_eq!(seen["sizes"][0]["routes"][0]["image"], "cuckoo-small");
    let operator = Operator {
        user: s.owner.clone(),
        via: Via::Web,
    };
    s.db.operator_set_setting(
        &operator,
        settings::COMPUTERS_PAUSED,
        &json!(true),
        "Provider incident",
    )
    .await
    .unwrap();
    let paused = run(&s, &job, "true").await;
    assert!(
        paused["message"].as_str().unwrap().contains("paused"),
        "{paused}"
    );
    // Awake computers sleep on the next sweep, even while held.
    assert_eq!(s.db.computers_sweep().await.unwrap(), 1);
    let id = seen["computers"][0]["provider_id"].as_str().unwrap();
    assert_eq!(s.fake.asleep(id), Some(true));
}
