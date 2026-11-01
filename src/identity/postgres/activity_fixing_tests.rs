//! Needs fixing groups what is wrong by cause, for the people who can fix it; agents
//! report needs in a fixed shape; Coming up shows the next 24 hours.
use super::super::super::super::automation_tests::schedule;
use super::super::super::super::control_tests::{request, setup};
use super::super::super::super::tests::{join, message, user};
use super::super::super::super::*;
use super::Problem;
use serde_json::json;

fn today() -> i64 {
    (now() - now() % 86400) * 1000
}

fn causes(now: &Value) -> Vec<String> {
    now["fixing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["cause"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn needs_fixing_groups_causes_with_one_fix_for_the_people_who_can_fix_them() {
    let (mut db, owner, w, connection, job) = setup("allow").await;
    db.pooled = true;
    let member = user(&db, "fixing-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET audience='everyone' WHERE id=$1",
        vec![job.agent.clone().into()],
    )
    .await
    .unwrap();

    // The account needs reconnecting.
    execute(&db.db, "INSERT INTO relay_agent_connections(workspace,agent,connection,revision,enabled_by,at) VALUES($1,$2,$3,nextval('relay_control_revision'),$4,0) ON CONFLICT DO NOTHING",
        vec![w.clone().into(), job.agent.clone().into(), connection.clone().into(), owner.id.clone().into()]).await.unwrap();
    execute(
        &db.db,
        "INSERT INTO relay_connector_reconnects(workspace,connection,since) VALUES($1,$2,1)",
        vec![w.clone().into(), connection.clone().into()],
    )
    .await
    .unwrap();
    // An action whose outcome couldn't be confirmed.
    let proposed = db
        .control_propose(&job, &request(&connection, "U-1"))
        .await
        .unwrap();
    db.control_ready(&w, &proposed.dispatch.unwrap())
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_control_actions SET updated=updated-120 WHERE job=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap();
    // The agent says what it needs, in a fixed shape; a made-up cause is refused.
    let need: Problem = serde_json::from_value(json!({"cause":"permission","detail":"I need refunds allowed on Orders","fix":"Allow order.refund for me"})).unwrap();
    db.report_agent_need(&job, &need).await.unwrap();
    assert!(
        db.report_agent_need(
            &job,
            &serde_json::from_value(json!({"cause":"bug","detail":"x"})).unwrap()
        )
        .await
        .is_err()
    );
    // It fails twice today.
    db.chat_finish(&job, "Couldn't update the order", "failed")
        .await
        .unwrap();
    let again = db
        .chat_send(
            &owner,
            &w,
            &job.chat,
            &message("Try again", Some(&job.agent)),
        )
        .await
        .unwrap()
        .unwrap();
    let again =
        super::super::super::super::home_maintenance_tests::claim(&db, &w, &again.reply).await;
    db.chat_finish(&again, "Still couldn't", "failed")
        .await
        .unwrap();
    // A stopped routine.
    let routine = schedule(&job.chat, Some(3600));
    db.save_schedule(&owner, &w, &job.agent, &routine)
        .await
        .unwrap();
    execute(&db.db, "UPDATE relay_agent_schedules SET state='blocked',detail='Routine access changed' WHERE id=$1", vec![routine.id.clone().into()]).await.unwrap();

    let seen = db.activity_now(&owner, &w, today(), None).await.unwrap();
    let found = causes(&seen);
    for cause in [
        "reconnect",
        "unconfirmed",
        "failing",
        "schedule",
        "reported",
    ] {
        assert!(found.contains(&cause.to_string()), "{cause} in {found:?}");
    }
    let group = |cause: &str| {
        seen["fixing"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["cause"] == cause)
            .unwrap()
            .clone()
    };
    assert!(
        group("reconnect")["title"]
            .as_str()
            .unwrap()
            .ends_with("needs reconnecting · 1 agent stopped"),
        "{}",
        group("reconnect")["title"]
    );
    assert_eq!(group("failing")["fix"]["kind"], "request");
    assert_eq!(
        group("failing")["fix"]["ref"],
        again.reply.as_str(),
        "the latest failure"
    );
    assert_eq!(group("reported")["title"], "1 agent needs a permission");
    assert_eq!(
        group("reported")["items"][0]["fix"],
        "Allow order.refund for me"
    );
    assert!(
        group("schedule")["items"][0]["detail"]
            .as_str()
            .unwrap()
            .ends_with("Routine access changed")
    );
    assert!(
        seen["agents"][0]["needs_fixing"].as_u64().unwrap() >= 5,
        "the agent card says so"
    );

    // Someone who only talks to the agent isn't asked to fix it.
    let theirs = db.activity_now(&member, &w, today(), None).await.unwrap();
    assert!(theirs["fixing"].as_array().unwrap().is_empty());
    // Dismissing what the agent reported clears it; only fixers may.
    let id = group("reported")["items"][0]["ref"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(db.dismiss_need(&member, &w, &id).await.is_err());
    db.dismiss_need(&owner, &w, &id).await.unwrap();
    assert!(
        !causes(&db.activity_now(&owner, &w, today(), None).await.unwrap())
            .contains(&"reported".to_string())
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn coming_up_lists_the_next_day_of_routines_follow_ups_and_due_tasks() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let routine = schedule(&job.chat, Some(3600));
    db.save_schedule(&owner, &w, &job.agent, &routine)
        .await
        .unwrap();
    let task = db
        .create_task(
            &job,
            &serde_json::from_value(
                json!({"title":"Call Acme back","start_in_minutes":120,"due_in_minutes":600}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let later = db
        .chat_send(
            &owner,
            &w,
            &job.chat,
            &message("Send the report by tonight", Some(&job.agent)),
        )
        .await
        .unwrap()
        .unwrap();
    db.chat_finish(&job, "Saved", "complete").await.unwrap();
    let second =
        super::super::super::super::home_maintenance_tests::claim(&db, &w, &later.reply).await;
    db.create_task(
        &second,
        &serde_json::from_value(json!({"title":"Send the report","due_in_minutes":300})).unwrap(),
    )
    .await
    .unwrap();
    db.chat_finish(&second, "Will do", "complete")
        .await
        .unwrap();

    let items = db.activity_now(&owner, &w, today(), None).await.unwrap()["coming_up"].clone();
    let kinds: Vec<(String, String)> = items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["kind"].as_str().unwrap().to_string(),
                i["title"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("routine".to_string(), "Delivery review".to_string()),
            ("follow_up".to_string(), "Call Acme back".to_string()),
            ("due".to_string(), "Send the report".to_string()),
        ],
        "soonest first"
    );
    assert_eq!(items[1]["id"], task["id"]);
    assert_eq!(items[0]["can_pause"], true);
}
