//! A workspace's own computer (B3, D248): admins add it, its connector signs
//! in once, and the chosen agents' commands and files go through it.
use super::super::computer_tests::{ask, computer, setup};
use super::super::tests::*;
use super::*;
use crate::identity::control::broker::invoke;
use serde_json::json;
use std::time::Duration;

/// Plays the connector for one piece of work: takes it, checks it, answers it.
async fn answer_one(db: &PgStore, computer: &str, key: &str, kind: &str, result: Value) -> Value {
    let next = db
        .machine_next(computer, key, Duration::from_secs(10))
        .await
        .unwrap();
    let call = next["call"].clone();
    assert_eq!(call["kind"], kind, "{next}");
    let done = db
        .machine_done(computer, key, call["id"].as_str().unwrap(), &result)
        .await
        .unwrap();
    assert_eq!(done["saved"], true);
    call
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_chosen_agent_works_on_the_workspaces_own_computer_through_its_connector() {
    let mut s = setup().await;
    // No Cuckoo computers at all: the workspace's own machine is enough.
    s.db.computers = None;
    computer(&s, json!({"on": false, "size": "small"})).await;
    let member = user(&s.db, "shared-computer-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    let id = uuid::Uuid::new_v4().to_string();

    // Only admins add machines; the code is shown once, and a repeat adds none.
    let denied =
        s.db.add_workspace_computer(&member, &s.w, &id, "Office PC")
            .await;
    assert!(denied.unwrap_err().to_string().contains("Only admins"));
    let added =
        s.db.add_workspace_computer(&s.owner, &s.w, &id, "Office PC")
            .await
            .unwrap();
    let code = added["code"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 19, "{code}");
    let again =
        s.db.add_workspace_computer(&s.owner, &s.w, &id, "Office PC")
            .await
            .unwrap();
    assert_eq!(again["repeat"], true);
    assert!(again["code"].is_null());

    let list = s.db.workspace_computers(&s.owner, &s.w).await.unwrap();
    let revision = list["computers"][0]["revision"].as_i64().unwrap();
    assert_eq!(list["computers"][0]["state"], "waiting");
    assert!(!list.to_string().contains(&code));
    s.db.choose_computer_agents(&s.owner, &s.w, &id, revision, &[s.agent.clone()])
        .await
        .unwrap();
    let status =
        s.db.computer_status(&s.owner, &s.w, &s.agent)
            .await
            .unwrap();
    assert_eq!(status["workspace_computer"], "Office PC");

    // Until the connector signs in, the agent is told plainly.
    let job = ask(&s, "List the invoices on the office PC").await;
    let waiting = invoke(&s.db, &job, "computer_run", json!({"command": "ls"}))
        .await
        .unwrap();
    assert!(
        waiting["message"]
            .as_str()
            .unwrap()
            .contains("isn't connected yet"),
        "{waiting}"
    );

    // A wrong code is refused; the right one works once and gives a key.
    let wrong =
        s.db.machine_sign_in(&id, "AAAA-BBBB-CCCC-DDDD", "linux")
            .await;
    assert!(wrong.unwrap_err().is::<Refused>());
    let key =
        s.db.machine_sign_in(&id, &code.to_lowercase(), "linux")
            .await
            .unwrap()["key"]
            .as_str()
            .unwrap()
            .to_owned();
    assert!(key.starts_with("ckc_"));
    let reused = s.db.machine_sign_in(&id, &code, "linux").await;
    assert!(reused.unwrap_err().is::<Refused>());
    let wrong_key =
        s.db.machine_next(&id, "ckc_wrong", Duration::from_secs(0))
            .await;
    assert!(wrong_key.unwrap_err().is::<Refused>());
    let idle =
        s.db.machine_next(&id, &key, Duration::from_secs(0))
            .await
            .unwrap();
    assert!(idle["call"].is_null());

    // A command goes to the machine and its answer comes back to the agent.
    let machine = {
        let db = s.db.clone();
        let (id, key) = (id.clone(), key.clone());
        tokio::spawn(async move {
            answer_one(
                &db,
                &id,
                &key,
                "run",
                json!({"exit_code":0,"stdout":"march.pdf\n","stderr":"","output_bytes":10}),
            )
            .await
        })
    };
    let ran = invoke(
        &s.db,
        &job,
        "computer_run",
        json!({"command": "ls invoices"}),
    )
    .await
    .unwrap();
    let call = machine.await.unwrap();
    assert_eq!(call["command"], "ls invoices");
    assert_eq!(call["agent"], s.agent.as_str());
    assert_eq!(ran["state"], "completed", "{ran}");
    assert!(ran.to_string().contains("march.pdf"));

    // Files go both ways, as base64 so any bytes survive.
    let machine = {
        let db = s.db.clone();
        let (id, key) = (id.clone(), key.clone());
        tokio::spawn(async move { answer_one(&db, &id, &key, "write", json!({})).await })
    };
    let wrote = invoke(
        &s.db,
        &job,
        "computer_write_file",
        json!({"path":"notes.txt","content":"Paid in full"}),
    )
    .await
    .unwrap();
    assert_eq!(machine.await.unwrap()["content_b64"], "UGFpZCBpbiBmdWxs");
    assert_eq!(wrote["state"], "completed", "{wrote}");
    let machine = {
        let db = s.db.clone();
        let (id, key) = (id.clone(), key.clone());
        tokio::spawn(async move {
            answer_one(
                &db,
                &id,
                &key,
                "read",
                json!({"content_b64":"UGFpZCBpbiBmdWxs"}),
            )
            .await
        })
    };
    let read = invoke(
        &s.db,
        &job,
        "computer_read_file",
        json!({"path":"notes.txt"}),
    )
    .await
    .unwrap();
    machine.await.unwrap();
    assert!(read.to_string().contains("Paid in full"), "{read}");

    // The browser stays on Cuckoo's computers for now.
    let browse = invoke(
        &s.db,
        &job,
        "browser_open",
        json!({"url":"https://example.com"}),
    )
    .await
    .unwrap();
    assert!(
        browse["message"]
            .as_str()
            .unwrap()
            .contains("isn't available"),
        "{browse}"
    );

    // Another request waits while this one holds the machine, then is told it's busy.
    let other = ask(&s, "Another question").await;
    let busy = invoke(&s.db, &other, "computer_run", json!({"command": "ls"}))
        .await
        .unwrap();
    assert!(busy["message"].as_str().unwrap().contains("busy"), "{busy}");
    release(&s.db.db, &s.w, &job.reply).await.unwrap();

    // A machine not heard from is offline.
    execute(
        &s.db.db,
        "UPDATE relay_workspace_computers SET last_seen_ms=1 WHERE id=$1",
        vec![id.clone().into()],
    )
    .await
    .unwrap();
    let offline = invoke(&s.db, &other, "computer_run", json!({"command": "ls"}))
        .await
        .unwrap();
    assert!(
        offline["message"].as_str().unwrap().contains("offline"),
        "{offline}"
    );

    // Removing it stops new work and refuses its connector.
    let revision =
        s.db.workspace_computers(&s.owner, &s.w).await.unwrap()["computers"][0]["revision"]
            .as_i64()
            .unwrap();
    s.db.remove_workspace_computer(&s.owner, &s.w, &id, revision)
        .await
        .unwrap();
    assert!(
        s.db.machine_next(&id, &key, Duration::from_secs(0))
            .await
            .unwrap_err()
            .is::<Refused>()
    );
    let after = invoke(&s.db, &other, "computer_run", json!({"command": "ls"}))
        .await
        .unwrap();
    assert_ne!(after["state"], "completed", "{after}");
    assert!(
        s.db.workspace_computers(&s.owner, &s.w).await.unwrap()["computers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn unanswered_work_is_abandoned_and_a_new_code_replaces_the_old_key() {
    let mut s = setup().await;
    s.db.computers = None;
    computer(&s, json!({"on": false, "size": "small"})).await;
    let id = uuid::Uuid::new_v4().to_string();
    let code =
        s.db.add_workspace_computer(&s.owner, &s.w, &id, "Lab Mac")
            .await
            .unwrap()["code"]
            .as_str()
            .unwrap()
            .to_owned();
    let revision =
        s.db.workspace_computers(&s.owner, &s.w).await.unwrap()["computers"][0]["revision"]
            .as_i64()
            .unwrap();
    s.db.choose_computer_agents(&s.owner, &s.w, &id, revision, &[s.agent.clone()])
        .await
        .unwrap();
    let key = s.db.machine_sign_in(&id, &code, "macos").await.unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();

    // The machine takes the work but never answers: the agent is told, and
    // a late answer is not saved. Nothing is retried.
    let job = ask(&s, "Run the report").await;
    let machine = {
        let db = s.db.clone();
        let (id, key) = (id.clone(), key.clone());
        tokio::spawn(async move {
            db.machine_next(&id, &key, Duration::from_secs(10))
                .await
                .unwrap()
        })
    };
    let ran = invoke(
        &s.db,
        &job,
        "computer_run",
        json!({"command": "sleep 99", "seconds": 1}),
    )
    .await;
    let call = machine.await.unwrap()["call"].clone();
    assert!(
        format!("{ran:?}").contains("didn't answer in time"),
        "{ran:?}"
    );
    let late =
        s.db.machine_done(
            &id,
            &key,
            call["id"].as_str().unwrap(),
            &json!({"exit_code":0}),
        )
        .await
        .unwrap();
    assert_eq!(late["saved"], false);
    let states: i64 = query(&s.db.db, "SELECT count(*) AS n FROM relay_workspace_computer_calls WHERE computer=$1 AND state='abandoned'", vec![id.clone().into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap();
    assert_eq!(states, 1);

    // A new code (a new PC) stops the old key at once.
    let revision =
        s.db.workspace_computers(&s.owner, &s.w).await.unwrap()["computers"][0]["revision"]
            .as_i64()
            .unwrap();
    let fresh =
        s.db.new_computer_code(&s.owner, &s.w, &id, revision)
            .await
            .unwrap();
    assert!(
        s.db.machine_next(&id, &key, Duration::from_secs(0))
            .await
            .unwrap_err()
            .is::<Refused>()
    );
    let stale = s.db.new_computer_code(&s.owner, &s.w, &id, revision).await;
    assert!(stale.unwrap_err().to_string().contains("changed"));
    let second =
        s.db.machine_sign_in(&id, fresh["code"].as_str().unwrap(), "macos")
            .await
            .unwrap();
    assert_ne!(second["key"], key.as_str());
}
