//! Agent computers (schema 103) against a local stand-in for E2B: commands and files run
//! in the request's own folder, one request holds the computer at a time, idle computers
//! sleep and their awake time is charged and budgeted, and reset or a lost computer start
//! a fresh one. See runtime/agent-computers.md.
use super::executor_slot_tests::agent;
use super::model_usage_reports::UsageScope;
use super::tests::*;
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::computers::{Computers, fake};
use crate::identity::control::broker::invoke;
use serde_json::json;

struct Setup {
    db: PgStore,
    fake: fake::Fake,
    _dir: tempfile::TempDir,
    owner: User,
    w: String,
    agent: String,
}
async fn setup() -> Setup {
    let mut db = fixture().await;
    let fake = fake::start().await;
    let dir = tempfile::tempdir().unwrap();
    db.computers = Some(Computers::new(fake.providers_file(dir.path())));
    let owner = user(&db, "computer-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, agent) = agent(&db, &owner, &w).await;
    Setup {
        db,
        fake,
        _dir: dir,
        owner,
        w,
        agent,
    }
}
/// Changes the agent's computer settings as its owner would in the Environment tab.
async fn computer(s: &Setup, settings: Value) {
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{computer}',$2) WHERE id=$1",
        vec![s.agent.clone().into(), settings.into()],
    )
    .await
    .unwrap();
}
/// A new request, each in its own conversation, claimed as an executor would.
async fn ask(s: &Setup, text: &str) -> Job {
    let room = chat(&s.db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    s.db.chat_send(&s.owner, &s.w, &room, &message(text, Some(&s.agent)))
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
    let mut job: Job = serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&s.db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",
        vec![job.reply.clone().into(), job.claim.clone().into()]).await.unwrap();
    job
}
async fn run(s: &Setup, job: &Job, command: &str) -> Value {
    invoke(&s.db, job, "computer_run", json!({"command": command}))
        .await
        .unwrap()
}
async fn provider_id(s: &Setup) -> String {
    query(
        &s.db.db,
        "SELECT provider_id FROM relay_computers WHERE agent=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "provider_id")
    .unwrap()
}
/// Makes the computer look idle for an hour, as if its request finished long ago.
async fn idle_for_an_hour(s: &Setup) {
    execute(&s.db.db, "UPDATE relay_computers SET held_until=0,last_used=last_used-3600,awake_since=awake_since-3600 WHERE agent=$1", vec![s.agent.clone().into()]).await.unwrap();
    execute(&s.db.db, "UPDATE relay_computer_sessions SET woke_at=woke_at-3600 WHERE agent=$1 AND slept_at IS NULL", vec![s.agent.clone().into()]).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_agent_works_with_commands_and_files_in_its_requests_folder() {
    let s = setup().await;
    let job = ask(&s, "Total the May invoices").await;
    // Off until its owner turns it on; nothing is made at the provider.
    let off = run(&s, &job, "ls").await;
    assert_eq!(off["state"], "denied");
    assert!(off["message"].as_str().unwrap().contains("switched off"));
    assert!(!s.db.computer_on(&s.w, &s.agent).await.unwrap());
    assert!(s.fake.log().is_empty());
    computer(&s, json!({"on": true, "size": "small"})).await;
    assert!(s.db.computer_on(&s.w, &s.agent).await.unwrap());
    assert!(s.db.prompt_features(&s.w, &s.agent).await.unwrap().computer);

    let ran = run(
        &s,
        &job,
        "printf 'a,1\\nb,2\\n' > may.csv && wc -l < may.csv",
    )
    .await;
    assert_eq!(ran["state"], "completed", "{ran}");
    assert_eq!(ran["exit_code"], 0);
    assert_eq!(ran["stdout"].as_str().unwrap().trim(), "2");
    assert_eq!(ran["folder"], format!("requests/{}", job.reply));
    assert_eq!(s.fake.log(), ["create cuckoo-small"]);
    // No internet unless the owner allows it.
    let id = provider_id(&s).await;
    assert_eq!(s.fake.network(&id)["denyOut"], json!(["0.0.0.0/0"]));
    let failed = run(&s, &job, "cat missing.txt").await;
    assert_eq!(failed["exit_code"], 1);
    assert!(failed["stderr"].as_str().unwrap().contains("missing.txt"));

    let saved = invoke(
        &s.db,
        &job,
        "computer_write_file",
        json!({"path": "shared/prices.csv", "content": "teak,410\n"}),
    )
    .await
    .unwrap();
    assert_eq!(saved["path"], "shared/prices.csv");
    let listed = invoke(&s.db, &job, "computer_list_files", json!({}))
        .await
        .unwrap();
    assert_eq!(listed["files"][0]["name"], "may.csv", "{listed}");
    assert_eq!(listed["files"][0]["bytes"], 8);
    let read = invoke(
        &s.db,
        &job,
        "computer_read_file",
        json!({"path": "shared/prices.csv"}),
    )
    .await
    .unwrap();
    assert_eq!(read["text"], "teak,410\n");
    for outside in ["../other/secret.txt", "/etc/passwd", "a/../../b"] {
        let refused = invoke(&s.db, &job, "computer_read_file", json!({"path": outside}))
            .await
            .unwrap();
        assert_eq!(refused["state"], "denied", "{outside}");
    }
    let missing = invoke(
        &s.db,
        &job,
        "computer_read_file",
        json!({"path": "nothing.txt"}),
    )
    .await
    .unwrap();
    assert_eq!(missing["state"], "invalid");
    assert!(
        invoke(
            &s.db,
            &job,
            "computer_run",
            json!({"command": "ls", "cwd": "/"})
        )
        .await
        .is_err(),
        "only its own fields"
    );

    // Activity shows each step, with the command's output.
    let request =
        s.db.activity_request(&s.owner, &s.w, &job.reply)
            .await
            .unwrap();
    let steps: Vec<&Value> = request["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["effect"] == "computer")
        .collect();
    let titles: Vec<&str> = steps.iter().map(|s| s["title"].as_str().unwrap()).collect();
    assert_eq!(
        titles,
        [
            "Ran: printf 'a,1\\nb,2\\n' > may.csv && wc -l < may.csv",
            "Ran: cat missing.txt",
            "Saved shared/prices.csv",
            &format!("Looked in requests/{}", job.reply),
            "Read shared/prices.csv",
        ]
    );
    assert_eq!(steps[0]["computer"]["output"].as_str().unwrap().trim(), "2");
    assert_eq!(steps[1]["state"], "failed");
    assert_eq!(steps[1]["computer"]["exit_code"], 1);

    // The Environment tab sees it awake, and never the provider.
    let status =
        s.db.computer_status(&s.owner, &s.w, &s.agent)
            .await
            .unwrap();
    assert_eq!(status["configured"], true);
    assert_eq!(status["computer"]["state"], "awake");
    assert_eq!(status["computer"]["in_use"], true);
    assert!(!status.to_string().contains("e2b"), "{status}");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn requests_take_turns_and_idle_computers_sleep_and_are_charged() {
    let s = setup().await;
    computer(&s, json!({"on": true, "size": "small", "internet": "listed", "websites": ["portal.sharmatraders.in"]})).await;
    let first = ask(&s, "Download the statement").await;
    assert_eq!(run(&s, &first, "echo one > one.txt").await["exit_code"], 0);
    let id = provider_id(&s).await;
    assert_eq!(
        s.fake.network(&id)["allowOut"],
        json!(["portal.sharmatraders.in"])
    );
    let second = ask(&s, "Convert the PDF").await;
    let busy = run(&s, &second, "ls").await;
    assert_eq!(busy["state"], "unavailable");
    assert!(busy["message"].as_str().unwrap().contains("busy"));
    // Finishing the first request lets the next one in, on the same computer.
    s.db.chat_finish(&first, "Done.", "complete").await.unwrap();
    let ran = run(&s, &second, "ls ../").await;
    assert!(
        ran["stdout"].as_str().unwrap().contains(&first.reply),
        "{ran}"
    );
    assert_eq!(s.fake.log(), ["create cuckoo-small"]);
    // A changed internet setting applies on the next use.
    computer(&s, json!({"on": true, "size": "small", "internet": "any"})).await;
    run(&s, &second, "true").await;
    assert_eq!(s.fake.network(&id)["denyOut"], json!([]));

    // Still in use: the sweep leaves it awake.
    assert_eq!(s.db.computers_sweep().await.unwrap(), 0);
    idle_for_an_hour(&s).await;
    assert_eq!(s.db.computers_sweep().await.unwrap(), 1);
    assert_eq!(s.fake.asleep(&id), Some(true));
    let session = query(
        &s.db.db,
        "SELECT seconds,charge_micro,cost_micro,person FROM relay_computer_sessions WHERE agent=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert!(session.try_get::<i64>("", "seconds").unwrap() >= 3600);
    // An hour of a small computer: $0.09 charged; it cost $0.10 at the provider.
    let charge: i64 = session.try_get("", "charge_micro").unwrap();
    assert!((90_000..90_100).contains(&charge), "{charge}");
    assert!(session.try_get::<i64>("", "cost_micro").unwrap() >= 100_000);
    assert_eq!(
        session.try_get::<Option<String>>("", "person").unwrap(),
        Some(s.owner.id.clone())
    );
    let status =
        s.db.computer_status(&s.owner, &s.w, &s.agent)
            .await
            .unwrap();
    assert_eq!(status["computer"]["state"], "asleep");
    assert!(status["month"]["seconds"].as_i64().unwrap() >= 3600);
    let usage =
        s.db.model_usage_report(
            &s.owner,
            &s.w,
            &UsageScope {
                agent: None,
                days: 30,
            },
        )
        .await
        .unwrap();
    assert_eq!(usage["computers"]["by_agent"][0]["name"], "Pool agent");
    assert!(usage["computers"]["charge_usd"].as_f64().unwrap() >= 0.09);
    assert!(!usage.to_string().contains("cost"), "{usage}");

    // Asleep, its files are kept: the next request wakes the same computer.
    let third = ask(&s, "What did the last request make?").await;
    let kept = run(&s, &third, &format!("cat ../{}/one.txt", first.reply)).await;
    assert_eq!(kept["stdout"].as_str().unwrap().trim(), "one");
    assert!(s.fake.log().contains(&format!("wake {id}")));

    // Computer time counts towards the monthly budget, which then stops it.
    execute(&s.db.db, "INSERT INTO relay_model_budgets(workspace,agent,monthly_micro,updated_by,updated_ms) VALUES($1,$2,50000,$3,0)",
        vec![s.w.clone().into(), s.agent.clone().into(), s.owner.id.clone().into()]).await.unwrap();
    let stopped = run(&s, &third, "true").await;
    assert_eq!(stopped["state"], "denied");
    assert!(
        stopped["message"]
            .as_str()
            .unwrap()
            .contains("monthly model budget")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reset_and_a_lost_computer_start_a_fresh_one() {
    let s = setup().await;
    let member = user(&s.db, "computer-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    computer(&s, json!({"on": true, "size": "small"})).await;
    let job = ask(&s, "Make a note").await;
    run(&s, &job, "echo kept > shared.txt").await;
    let first = provider_id(&s).await;
    // A lost computer is replaced, empty, on its next use.
    idle_for_an_hour(&s).await;
    s.db.computers_sweep().await.unwrap();
    s.fake.lose(&first);
    let again = run(&s, &job, "ls").await;
    assert_eq!(again["exit_code"], 0, "{again}");
    let second = provider_id(&s).await;
    assert_ne!(first, second);
    assert_eq!(again["stdout"], "");

    // Only its owner or an admin may reset it.
    assert!(
        s.db.computer_reset(&member, &s.w, &s.agent)
            .await
            .unwrap_err()
            .to_string()
            .contains("owner or an admin")
    );
    // A size chosen later applies to the computer made after a reset.
    computer(&s, json!({"on": true, "size": "standard"})).await;
    s.db.computer_reset(&s.owner, &s.w, &s.agent).await.unwrap();
    let resetting = run(&s, &job, "ls").await;
    assert!(
        resetting["message"]
            .as_str()
            .unwrap()
            .contains("being reset")
    );
    assert_eq!(s.db.computers_sweep().await.unwrap(), 1);
    assert!(!s.fake.exists(&second));
    run(&s, &job, "true").await;
    assert_eq!(s.fake.log().last().unwrap(), "create cuckoo-standard");
    let audited = query(
        &s.db.db,
        "SELECT count(*) AS n FROM relay_audit WHERE action='computer_reset' AND workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(audited.try_get::<i64>("", "n").unwrap(), 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn programs_can_ask_first_or_be_blocked_and_computers_need_setting_up() {
    let mut s = setup().await;
    computer(
        &s,
        json!({"on": true, "size": "small", "programs": "block"}),
    )
    .await;
    let job = ask(&s, "Run the report").await;
    let blocked = run(&s, &job, "python3 report.py").await;
    assert_eq!(blocked["state"], "denied");
    assert!(s.fake.log().is_empty());
    // Files still work when programs are blocked.
    let saved = invoke(
        &s.db,
        &job,
        "computer_write_file",
        json!({"path": "notes.txt", "content": "x"}),
    )
    .await
    .unwrap();
    assert_eq!(saved["state"], "completed");

    computer(&s, json!({"on": true, "size": "small", "programs": "ask"})).await;
    let asks = run(&s, &job, "python3 report.py").await;
    assert_eq!(asks["state"], "needs_confirmation");
    let question = &asks["question"];
    assert_eq!(
        question["specification"]["title"],
        "May Pool agent run this on its computer?"
    );
    assert_eq!(
        question["specification"]["description"],
        "python3 report.py"
    );
    let asked = invoke(&s.db, &job, "ask_question", question.clone())
        .await
        .unwrap();
    let id = asked["question"]["id"]
        .as_str()
        .or(asked["id"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("{asked}"));
    s.db.answer_question(
        &s.owner,
        &s.w,
        &id,
        &crate::identity::interactions::model::Submission {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            decision: "answer".into(),
            values: json!({"choices": ["yes"]}),
        },
    )
    .await
    .unwrap();
    let mut resumed = job.clone();
    resumed.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&s.db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",
        vec![job.reply.clone().into(), resumed.claim.clone().into()]).await.unwrap();
    let allowed = run(&s, &resumed, "echo ran").await;
    assert_eq!(
        allowed["state"], "needs_confirmation",
        "another command asks again"
    );
    let ran = run(&s, &resumed, "python3 report.py").await;
    assert_eq!(ran["state"], "completed", "{ran}");

    // Without a providers file nothing can be turned on, and agents are told plainly.
    s.db.computers = None;
    let save: crate::identity::agents::config::SaveAgent = serde_json::from_value(json!({
        "id": "new-agent", "name": "Mira", "role": "Accounts", "model": "fixture", "audience": "only_me",
        "revision": 0, "settings": {"computer": {"on": true}}
    }))
    .unwrap();
    assert!(s.db.computer_allowed(&save, false).is_err());
    assert!(
        s.db.computer_allowed(&save, true).is_ok(),
        "already on stays saveable"
    );
    assert!(!s.db.computer_on(&s.w, &s.agent).await.unwrap());
    assert_eq!(run(&s, &resumed, "ls").await["state"], "unavailable");
}
