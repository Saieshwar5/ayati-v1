//! The agent's browser (schema 104) against the local E2B stand-in with a real Chromium
//! (set AYATI_TEST_CHROMIUM to its path): pages as text with numbered elements, the
//! browsing and button controls, passwords left to people, and a hand-over a person
//! watches and controls. See runtime/agent-computers.md.
use super::computer_tests::{Setup, ask, computer, setup};
use super::tests::*;
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::control::broker::invoke;
use serde_json::json;

/// A small website on this machine: a form with a password and a button, and a link.
async fn website() -> String {
    use axum::{Router, response::Html, routing::get};
    let form = r#"<!doctype html><title>Sharma Traders portal</title><h1>Invoices</h1>
        <form action="/done"><label>Invoice number <input name="n"></label>
        <label>Password <input type="password" name="p"></label>
        <select name="m"><option>April</option><option>May</option></select>
        <button type="submit">Search</button></form><a href="/done">Download May invoices</a>"#;
    let app = Router::new()
        .route("/", get(move || async move { Html(form) }))
        .route(
            "/done",
            get(|| async { Html("<!doctype html><title>Done</title><p>Found invoice</p>") }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    url
}

/// Browser tests need a local Chromium; without one they say so and pass.
fn chromium() -> bool {
    let found =
        std::env::var("AYATI_TEST_CHROMIUM").is_ok_and(|p| std::path::Path::new(&p).exists());
    if !found {
        eprintln!("Skipping browser checks: set AYATI_TEST_CHROMIUM to a Chromium binary");
    }
    found
}

async fn tool(s: &Setup, job: &Job, name: &str, input: Value) -> Value {
    invoke(&s.db, job, name, input).await.unwrap()
}

/// Takes the request back as an executor would, after a question or to look.
async fn reclaim(s: &Setup, job: &Job) -> Job {
    let mut resumed = job.clone();
    resumed.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&s.db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",
        vec![job.reply.clone().into(), resumed.claim.clone().into()]).await.unwrap();
    resumed
}

/// Asks the question a tool handed back and answers it as the person who asked.
async fn answer(s: &Setup, job: &Job, waiting: &Value, choice: &str) -> Job {
    let asked = tool(s, job, "ask_question", waiting["question"].clone()).await;
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
            values: json!({"choices": [choice]}),
        },
    )
    .await
    .unwrap();
    reclaim(s, job).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_browse_by_numbered_elements_and_ask_before_pressing_buttons() {
    if !chromium() {
        return;
    }
    let s = setup().await;
    let site = website().await;
    computer(&s, json!({"on": true, "size": "small", "internet": "any"})).await;
    let job = ask(&s, "Find invoice INV-204").await;
    let page = tool(&s, &job, "browser_open", json!({"url": format!("{site}/")})).await;
    assert_eq!(page["state"], "completed", "{page}");
    assert_eq!(page["title"], "Sharma Traders portal");
    assert_eq!(
        page["elements"][0],
        json!({"ref": 1, "role": "textbox", "name": "Invoice number"})
    );
    assert_eq!(page["elements"][1]["password"], true);
    let typed = tool(
        &s,
        &job,
        "browser_type",
        json!({"ref": 1, "text": "INV-204"}),
    )
    .await;
    assert_eq!(typed["elements"][0]["value"], "INV-204");
    let refused = tool(
        &s,
        &job,
        "browser_type",
        json!({"ref": 2, "text": "secret"}),
    )
    .await;
    assert_eq!(refused["state"], "failed");
    assert!(refused["message"].as_str().unwrap().contains("hand-over"));
    let chosen = tool(
        &s,
        &job,
        "browser_choose",
        json!({"ref": 3, "option": "may"}),
    )
    .await;
    assert_eq!(chosen["elements"][2]["value"], "May");

    // Pressing a button asks the person first by default; their yes lets it through.
    let waiting = tool(&s, &job, "browser_click", json!({"ref": 4})).await;
    assert_eq!(waiting["state"], "needs_confirmation", "{waiting}");
    assert_eq!(
        waiting["question"]["specification"]["title"],
        "May Pool agent press “Search” on 127.0.0.1?"
    );
    let resumed = answer(&s, &job, &waiting, "yes").await;
    let sent = tool(&s, &resumed, "browser_click", json!({"ref": 4})).await;
    assert_eq!(sent["title"], "Done", "{sent}");
    assert!(sent["url"].as_str().unwrap().contains("n=INV-204"));
    let shot = tool(&s, &resumed, "browser_screenshot", json!({"name": "found"})).await;
    assert_eq!(shot["state"], "completed", "{shot}");

    // Activity lists each step; typed text isn't kept.
    let request =
        s.db.activity_request(&s.owner, &s.w, &job.reply)
            .await
            .unwrap();
    let titles: Vec<String> = request["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["effect"] == "computer")
        .map(|step| step["title"].as_str().unwrap().to_owned())
        .collect();
    assert!(titles.contains(&format!("Opened {site}/")), "{titles:?}");
    assert!(
        titles
            .iter()
            .any(|t| t == "Typed 7 characters into element 1"),
        "{titles:?}"
    );
    assert!(
        titles
            .iter()
            .any(|t| t.starts_with("Asked to press “Search”")),
        "{titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t.contains("INV-204")),
        "typed text stays out of Activity"
    );

    // Blocked buttons are refused while links still work; blocked browsing stops it all.
    computer(
        &s,
        json!({"on": true, "size": "small", "internet": "any", "submit": "block"}),
    )
    .await;
    tool(
        &s,
        &resumed,
        "browser_open",
        json!({"url": format!("{site}/")}),
    )
    .await;
    assert_eq!(
        tool(&s, &resumed, "browser_click", json!({"ref": 4})).await["state"],
        "denied"
    );
    assert_eq!(
        tool(&s, &resumed, "browser_press", json!({"key": "Enter"})).await["state"],
        "denied"
    );
    assert_eq!(
        tool(&s, &resumed, "browser_click", json!({"ref": 5})).await["title"],
        "Done"
    );
    computer(
        &s,
        json!({"on": true, "size": "small", "internet": "any", "browser": "ask"}),
    )
    .await;
    let site_question = tool(
        &s,
        &resumed,
        "browser_open",
        json!({"url": format!("{site}/")}),
    )
    .await;
    assert_eq!(
        site_question["question"]["specification"]["title"],
        "May Pool agent open 127.0.0.1 in its browser?"
    );
    computer(
        &s,
        json!({"on": true, "size": "small", "internet": "any", "browser": "block"}),
    )
    .await;
    assert_eq!(
        tool(&s, &resumed, "browser_read", json!({})).await["state"],
        "denied"
    );
    assert!(
        invoke(&s.db, &resumed, "browser_click", json!({"ref": 1, "x": 3}))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_person_takes_over_the_browser_in_a_hand_over_and_others_cannot() {
    if !chromium() {
        return;
    }
    let s = setup().await;
    let site = website().await;
    let member = user(&s.db, "browser-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    computer(&s, json!({"on": true, "size": "small", "internet": "any"})).await;
    let job = ask(&s, "Download the statement").await;
    tool(&s, &job, "browser_open", json!({"url": format!("{site}/")})).await;
    // Nobody controls the browser until the agent hands it over.
    let watching = s.db.computer_watch(&s.owner, &s.w, &s.agent).await.unwrap();
    assert_eq!(watching.hello["watching"]["control"], false);
    s.db.computer_unwatch(&watching.id);
    let refused = s.db.computer_watch(&member, &s.w, &s.agent).await;
    assert!(
        refused
            .err()
            .is_some_and(|e| e.to_string().contains("Only the person"))
    );

    let waiting = tool(
        &s,
        &job,
        "browser_hand_over",
        json!({"need": "sign_in", "message": "Sign in to the Sharma Traders portal"}),
    )
    .await;
    assert_eq!(waiting["state"], "needs_confirmation", "{waiting}");
    assert_eq!(
        waiting["question"]["specification"]["options"][0]["label"],
        "Done, give back"
    );
    let asked = tool(&s, &job, "ask_question", waiting["question"].clone()).await;
    // The person sees the page they are asked to help with.
    let question_id = asked["question"]["id"]
        .as_str()
        .or(asked["id"].as_str())
        .unwrap()
        .to_owned();
    let shown = s.db.question(&s.owner, &s.w, &question_id).await.unwrap();
    assert!(
        shown["screenshot"]
            .as_str()
            .is_some_and(|src| src.starts_with("data:image/jpeg;base64,/9j/")),
        "{}",
        shown["screenshot"]
    );
    assert_eq!(shown["agent"], json!(s.agent));
    // Finishing the attempt keeps the computer for this request while the person works.
    PgStore::computer_release(&s.db.db, &s.w, &job.reply)
        .await
        .unwrap();
    let held: Option<String> = query(
        &s.db.db,
        "SELECT held_by FROM relay_computers WHERE agent=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "held_by")
    .unwrap();
    assert_eq!(held.as_deref(), Some(job.reply.as_str()));

    let mut watching = s.db.computer_watch(&s.owner, &s.w, &s.agent).await.unwrap();
    assert_eq!(watching.hello["watching"]["control"], true);
    // The first frame arrives, then the person types into the invoice field.
    let mut frame = false;
    for _ in 0..50 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            watching.watch.lines.recv(),
        )
        .await
        {
            Ok(Ok(line)) if line.starts_with("{\"frame\"") => {
                frame = true;
                break;
            }
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            other => panic!("no frame: {other:?}"),
        }
    }
    assert!(frame);
    let input = |value: Value| serde_json::from_value::<super::LiveInput>(value).unwrap();
    // The person moves to the invoice field and types.
    s.db.computer_live_input(
        &s.owner,
        &s.w,
        &s.agent,
        &input(json!({"mouse": {"type": "mouseMoved", "x": 10.0, "y": 10.0}})),
    )
    .await
    .unwrap();
    s.db.computer_live_input(&s.owner, &s.w, &s.agent, &input(json!({"key": "Tab"})))
        .await
        .unwrap();
    s.db.computer_live_input(
        &s.owner,
        &s.w,
        &s.agent,
        &input(json!({"text": "typed by a person"})),
    )
    .await
    .unwrap();
    assert!(
        s.db.computer_live_input(&s.owner, &s.w, &s.agent, &input(json!({"key": "F12"})))
            .await
            .is_err()
    );
    assert!(
        s.db.computer_live_input(&member, &s.w, &s.agent, &input(json!({"text": "x"})))
            .await
            .is_err()
    );
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let looking = reclaim(&s, &job).await;
    let read = tool(&s, &looking, "browser_read", json!({})).await;
    assert_eq!(read["elements"][0]["value"], "typed by a person", "{read}");

    // Done gives the browser back: the person no longer controls it.
    let id = asked["question"]["id"]
        .as_str()
        .or(asked["id"].as_str())
        .unwrap()
        .to_owned();
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
    s.db.computer_unwatch(&watching.id);
    let again = s.db.computer_watch(&s.owner, &s.w, &s.agent).await.unwrap();
    assert_eq!(again.hello["watching"]["control"], false);
    s.db.computer_unwatch(&again.id);
    let resumed = reclaim(&s, &job).await;
    let back = tool(
        &s,
        &resumed,
        "browser_hand_over",
        json!({"need": "sign_in", "message": "Sign in to the Sharma Traders portal"}),
    )
    .await;
    assert_eq!(back["state"], "completed", "{back}");
    tool(
        &s,
        &resumed,
        "computer_run",
        json!({"command": "pkill -f \"$HOME/.cuckoo/browser-profile\" || true"}),
    )
    .await;
}
