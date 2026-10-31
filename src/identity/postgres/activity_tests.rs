use super::super::tests::{chat, fixture, join, message, user, workspace};
use super::Filters as ActivityFilters;
use super::*;
use crate::identity::chat::model::NewChat;

fn filters() -> ActivityFilters {
    ActivityFilters::default()
}

async fn ids(db: &PgStore, person: &User, w: &str, f: &ActivityFilters) -> Vec<String> {
    db.activity_history(person, w, f).await.unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn requests_show_to_the_requester_the_conversation_managers_and_admins_only() {
    let db = fixture().await;
    let admin = user(&db, "act-admin").await;
    let asker = user(&db, "act-asker").await;
    let manager = user(&db, "act-manager").await;
    let other = user(&db, "act-other").await;
    let w = workspace(&db, &admin).await;
    for person in [&asker, &manager, &other] {
        join(&db, &admin, &w, person, "Member").await;
    }
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET audience='everyone' WHERE id=$1",
        vec![agent.clone().into()],
    )
    .await
    .unwrap();
    execute(&db.db, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$3,1)", vec![w.clone().into(), agent.clone().into(), manager.id.clone().into()]).await.unwrap();
    // A direct chat between the asker and the agent.
    let direct = db
        .chat_create(
            &asker,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![agent.clone()],
            },
        )
        .await
        .unwrap();
    let job = db
        .chat_send(
            &asker,
            &w,
            &direct,
            &message("Quote for Sharma Traders", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    for (person, sees) in [
        (&asker, true),
        (&admin, true),
        (&manager, true),
        (&other, false),
    ] {
        assert_eq!(
            ids(&db, person, &w, &filters()).await,
            if sees {
                vec![job.reply.clone()]
            } else {
                vec![]
            },
            "{}",
            person.id
        );
    }
    // A manager or admin opening a conversation they aren't in is recorded.
    let details = db.activity_request(&manager, &w, &job.reply).await.unwrap();
    assert_eq!(details["in_conversation"], false);
    assert_eq!(details["message"], "Quote for Sharma Traders");
    let logged: i64 = query(&db.db, "SELECT count(*) AS n FROM relay_audit WHERE workspace=$1 AND action='activity_private_viewed'", vec![w.clone().into()]).await.unwrap().unwrap().try_get("", "n").unwrap();
    assert_eq!(logged, 1);
    assert!(db.activity_request(&other, &w, &job.reply).await.is_err());

    // In a channel, members see it; leaving the channel hides it again.
    let channel = chat(&db, &admin, &w, vec![other.id.clone()], vec![agent.clone()]).await;
    let shared = db
        .chat_send(&admin, &w, &channel, &message("Stock check", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ids(&db, &other, &w, &filters()).await,
        vec![shared.reply.clone()]
    );
    db.channel_members(&admin, &w, &channel, std::slice::from_ref(&admin.id))
        .await
        .unwrap();
    assert!(ids(&db, &other, &w, &filters()).await.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn paging_has_no_gaps_and_filters_and_totals_agree() {
    let db = fixture().await;
    let owner = user(&db, "page-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let mut sent = vec![];
    for n in 0..30 {
        let text = if n == 7 {
            "Find the Kapoor order".to_string()
        } else {
            format!("Request {n}")
        };
        sent.push(
            db.chat_send(&owner, &w, &c, &message(&text, Some(&agent)))
                .await
                .unwrap()
                .unwrap()
                .reply,
        );
    }
    let first = db.activity_history(&owner, &w, &filters()).await.unwrap();
    let mut seen = ids(&db, &owner, &w, &filters()).await;
    assert_eq!(seen.len(), 25);
    let next = ActivityFilters {
        before: first["next_before"].as_i64(),
        ..filters()
    };
    let rest = db.activity_history(&owner, &w, &next).await.unwrap();
    assert!(rest["next_before"].is_null());
    seen.extend(
        rest["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string()),
    );
    sent.reverse();
    assert_eq!(seen, sent, "newest first, every request exactly once");

    let found = ids(
        &db,
        &owner,
        &w,
        &ActivityFilters {
            q: Some("kapoor".into()),
            ..filters()
        },
    )
    .await;
    assert_eq!(found, vec![sent[22].clone()]);
    assert_eq!(
        ids(
            &db,
            &owner,
            &w,
            &ActivityFilters {
                status: Some("done".into()),
                ..filters()
            }
        )
        .await
        .len(),
        0
    );
    assert_eq!(
        ids(
            &db,
            &owner,
            &w,
            &ActivityFilters {
                kind: Some("schedule".into()),
                ..filters()
            }
        )
        .await
        .len(),
        0
    );
    assert_eq!(
        ids(
            &db,
            &owner,
            &w,
            &ActivityFilters {
                agent: Some("missing".into()),
                ..filters()
            }
        )
        .await
        .len(),
        0
    );
    assert!(
        db.activity_history(
            &owner,
            &w,
            &ActivityFilters {
                status: Some("bogus".into()),
                ..filters()
            }
        )
        .await
        .is_err()
    );

    let totals = db.activity_totals(&owner, &w, &filters()).await.unwrap();
    assert_eq!(totals["requests"], 30);
    assert_eq!(totals["people"], 1);
    assert_eq!(totals["by_agent"][0]["count"], 30);
    assert_eq!(
        totals["per_day"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["count"].as_i64().unwrap())
            .sum::<i64>(),
        30
    );

    // Requests from before intake recorded the asking message find it by position.
    db.db
        .execute_unprepared(&format!(
            "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target) VALUES
              ('old-ask','{w}','{c}','{owner}','user','An old question','complete',1,'{agent}'),
              ('old-reply','{w}','{c}','{agent}','agent','An old answer','complete',1,NULL);
             INSERT INTO relay_chat_jobs(workspace,reply,payload,dispatch,run_seconds,state,created_ms)
             SELECT workspace,'old-reply',payload||jsonb_build_object('reply','old-reply'),dispatch,run_seconds,'finished',1000
             FROM relay_chat_jobs WHERE reply='{}'",
            sent[0],
            owner = owner.id
        ))
        .await
        .unwrap();
    let old = db
        .activity_history(
            &owner,
            &w,
            &ActivityFilters {
                q: Some("old question".into()),
                ..filters()
            },
        )
        .await
        .unwrap();
    assert_eq!(old["items"][0]["summary"], "An old question");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn now_shows_agent_states_running_work_and_what_happened_while_away() {
    let db = fixture().await;
    let owner = user(&db, "now-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Update the delivery address", Some(&agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "I sent the new address to the customer.", "complete")
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Still working on this", Some(&agent)),
    )
    .await
    .unwrap();

    // Before the first "caught up", the last day counts as "while you were away".
    let first = db.activity_now(&owner, &w, 1).await.unwrap();
    assert!(first["away"]["since"].as_i64().unwrap() < now() * 1000 - 23 * 3600 * 1000);
    assert_eq!(first["away"]["totals"]["requests"], 2);
    // Catching up empties it, on every device.
    db.activity_seen(&owner, &w).await.unwrap();
    assert_eq!(
        db.activity_now(&owner, &w, 1).await.unwrap()["away"]["totals"]["requests"],
        0
    );
    // A visit an hour ago shows what happened since.
    execute(
        &db.db,
        "UPDATE relay_activity_visits SET seen_ms=seen_ms-3600000",
        vec![],
    )
    .await
    .unwrap();
    let since: i64 = query(&db.db, "SELECT seen_ms FROM relay_activity_visits", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "seen_ms")
        .unwrap();
    let now = db.activity_now(&owner, &w, 1).await.unwrap();
    assert_eq!(now["away"]["since"], since);
    assert_eq!(now["away"]["totals"]["requests"], 2);
    assert_eq!(now["away"]["totals"]["done"], 1);
    assert_eq!(now["agents"][0]["running"], 1);
    assert_eq!(now["agents"][0]["today"], 2);
    assert_eq!(now["running"].as_array().unwrap().len(), 1);

    // The reply claims a change the records don't show.
    let details = db.activity_request(&owner, &w, &job.reply).await.unwrap();
    assert!(
        details["disagreement"]
            .as_str()
            .unwrap()
            .contains("no record of a change")
    );
    assert_eq!(details["steps"].as_array().unwrap().len(), 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn thirty_agents_over_ninety_days_stay_within_the_response_budget() {
    let db = fixture().await;
    let owner = user(&db, "perf-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let job = db
        .chat_send(&owner, &w, &c, &message("Seed request", Some(&agent)))
        .await
        .unwrap()
        .unwrap();
    // 29 more agents, then 30 requests a day each for 90 days (81,000 requests),
    // each with its asking message, as intake records them.
    db.db
        .execute_unprepared(&format!(
            "INSERT INTO relay_shared_agents(workspace,id,name,role,model,owner_id,creator_id,revision,audience,settings)
         SELECT workspace,id||'-'||n,name||' '||n,role,model,owner_id,creator_id,revision,audience,settings
         FROM relay_shared_agents, generate_series(1,29) n WHERE id='{agent}';
         INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target)
         SELECT p||g,'{w}','{c}',CASE p WHEN 'ask-' THEN '{owner}' ELSE '{agent}' END,CASE p WHEN 'ask-' THEN 'user' ELSE 'agent' END,
          CASE p WHEN 'ask-' THEN 'Check order '||g ELSE 'Done' END,'complete',extract(epoch FROM now())::bigint-(g%90)*86400,
          CASE p WHEN 'ask-' THEN '{agent}' END
         FROM generate_series(1,81000) g, (VALUES('ask-'),('perf-')) v(p) ORDER BY g,p;
         INSERT INTO relay_chat_jobs(workspace,reply,payload,dispatch,run_seconds,state,created_ms,request_message)
         SELECT j.workspace,'perf-'||g,j.payload||jsonb_build_object('reply','perf-'||g,'agent',CASE WHEN (g/90)%30=0 THEN '{agent}' ELSE '{agent}-'||((g/90)%30) END),
          j.dispatch,j.run_seconds,'finished',(extract(epoch FROM now())::bigint-(g%90)*86400)*1000,'ask-'||g
         FROM relay_chat_jobs j, generate_series(1,81000) g WHERE j.reply='{}';
         ANALYZE relay_chat_jobs; ANALYZE relay_chat_messages;",
            job.reply,
            owner = owner.id
        ))
        .await
        .unwrap();
    let ninety = ActivityFilters {
        from: Some(now() * 1000 - 90 * 86400 * 1000),
        ..filters()
    };
    let week = ActivityFilters {
        from: Some(now() * 1000 - 7 * 86400 * 1000),
        ..filters()
    };
    let timed = |label: &'static str| (label, std::time::Instant::now());
    let mut times = vec![];
    for range in [&week, &ninety] {
        let t = timed("history");
        let page = db.activity_history(&owner, &w, range).await.unwrap();
        times.push((t.0, t.1.elapsed()));
        assert_eq!(page["items"].as_array().unwrap().len(), 25);
        assert!(
            page["items"][0]["summary"]
                .as_str()
                .unwrap()
                .starts_with("Check order")
        );
        let t = timed("totals");
        let totals = db.activity_totals(&owner, &w, range).await.unwrap();
        times.push((t.0, t.1.elapsed()));
        assert_eq!(totals["by_agent"].as_array().unwrap().len(), 12);
    }
    let t = timed("search");
    let found = db
        .activity_history(
            &owner,
            &w,
            &ActivityFilters {
                q: Some("order 80999".into()),
                ..ninety
            },
        )
        .await
        .unwrap();
    times.push((t.0, t.1.elapsed()));
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let t = timed("now");
    db.activity_now(&owner, &w, now() * 1000 - 3600 * 1000)
        .await
        .unwrap();
    times.push((t.0, t.1.elapsed()));
    eprintln!("activity with 81,000 requests: {times:?}");
    // Generous for a shared test machine: measured at 30-700 ms locally.
    for (label, took) in times {
        assert!(took.as_millis() < 2500, "{label} took {took:?}");
    }
}
