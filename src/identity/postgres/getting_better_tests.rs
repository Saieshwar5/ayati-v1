//! Measures, rules from repeated decisions, goals and weekly reviews.
use super::super::super::super::control_tests::{request, setup};
use super::super::super::super::tests::{join, user};
use super::super::super::super::*;
use super::{Goal, GoalProgress, Review};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn repeated_approvals_offer_a_rule_that_the_admin_ceiling_bounds() {
    let (db, owner, w, connection, job) = setup("ask").await;
    // Five approvals of the same write, decided by the owner, no rejection.
    for i in 0..5 {
        let proposed = db
            .control_propose(&job, &request(&connection, &format!("A-{i}")))
            .await
            .unwrap();
        let id = proposed.action["id"].as_str().unwrap().to_string();
        execute(&db.db, "UPDATE relay_control_actions SET state='completed',decision='approve',decided_by=$2,decided_at=floor(extract(epoch FROM clock_timestamp()))::bigint WHERE id=$1",
            vec![id.into(), owner.id.clone().into()]).await.unwrap();
    }
    // While the admin's rule says ask, nothing is offered: never looser than the admin.
    assert!(
        db.activity_review(&owner, &w, 30, None).await.unwrap()["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    execute(&db.db, "INSERT INTO relay_connector_rules(workspace,connection,operation,mode,revision,updated_by,updated_at) VALUES($1,$2,'order.update','allow',1,$3,0) ON CONFLICT(workspace,connection,operation) DO UPDATE SET mode='allow'",
        vec![w.clone().into(), connection.clone().into(), owner.id.clone().into()]).await.unwrap();
    let review = db.activity_review(&owner, &w, 30, None).await.unwrap();
    let suggestion = review["suggestions"][0].clone();
    assert_eq!(suggestion["approved"], 5);
    assert_eq!(suggestion["operation"], "order.update");
    // A member can't take it; the owner can, through the ordinary rule path.
    let member = user(&db, "better-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(
        db.accept_rule_suggestion(&member, &w, &job.agent, &connection, "order.update")
            .await
            .is_err()
    );
    db.accept_rule_suggestion(&owner, &w, &job.agent, &connection, "order.update")
        .await
        .unwrap();
    let mode: String = query(&db.db, "SELECT mode FROM relay_control_rules WHERE agent=$1 AND connection=$2 AND operation='order.update'",
        vec![job.agent.clone().into(), connection.clone().into()]).await.unwrap().unwrap().try_get("", "mode").unwrap();
    assert_eq!(mode, "allow");
    assert!(
        db.activity_review(&owner, &w, 30, None).await.unwrap()["suggestions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "already a rule"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn goals_reviews_and_measures_follow_the_agents_work() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let goal: Goal = serde_json::from_value(json!({"agent": job.agent, "name": "Quotes sent", "target": 50, "unit": "quotes", "period": "week"})).unwrap();
    let id = db.save_goal(&owner, &w, &goal).await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        db.my_goals(&job).await.unwrap()["goals"][0]["name"],
        "Quotes sent"
    );
    let progress: GoalProgress = serde_json::from_value(
        json!({"goal": id, "current": 42, "risk": "Two suppliers are late"}),
    )
    .unwrap();
    db.update_goal(&job, &progress).await.unwrap();
    assert!(
        db.update_goal(
            &job,
            &serde_json::from_value(json!({"goal": "other", "current": 1})).unwrap()
        )
        .await
        .is_err()
    );
    let review: Review = serde_json::from_value(json!({"done":"Sent 42 quotes","went_wrong":"Two failed on Zoho","learned":"Check the token first","suggestion":"Add the price list skill"})).unwrap();
    db.write_review(&job, &review).await.unwrap();
    db.write_review(&job, &review).await.unwrap();
    db.create_task(
        &job,
        &serde_json::from_value(json!({"title":"Send quotes to Acme","due_in_minutes":60}))
            .unwrap(),
    )
    .await
    .unwrap();
    db.chat_finish(&job, "Done", "complete").await.unwrap();

    let seen = db.activity_review(&owner, &w, 30, None).await.unwrap();
    assert_eq!(seen["goals"][0]["current"], 42.0);
    assert_eq!(seen["goals"][0]["risk"], "Two suppliers are late");
    assert_eq!(
        seen["reviews"].as_array().unwrap().len(),
        1,
        "one review per request"
    );
    assert_eq!(seen["reviews"][0]["suggestion"], "Add the price list skill");
    let agent = seen["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == job.agent.as_str())
        .unwrap()
        .clone();
    assert_eq!(agent["tasks"], 1);
    assert_eq!(agent["open"], 1);
    assert!(agent["requests"].as_i64().unwrap() >= 1);
    // Weekly review: a weekly task in its own room the manager can turn off again.
    let on = db
        .set_weekly_review(&owner, &w, &job.agent, true)
        .await
        .unwrap();
    let task = on["task"].as_str().unwrap().to_string();
    let made = query(
        &db.db,
        "SELECT t.repeat->>'unit' AS unit,t.instructions LIKE 'Write your weekly review%' AS review,c.work,
          (SELECT k.due_ms FROM relay_task_wakeups k WHERE k.task=t.id AND k.state='pending')-(extract(epoch FROM clock_timestamp())*1000)::bigint AS wait
         FROM relay_tasks t JOIN relay_chats c ON c.workspace=t.workspace AND c.id=t.chat WHERE t.id=$1",
        vec![task.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(made.try_get::<String>("", "unit").unwrap(), "week");
    assert!(
        made.try_get::<bool>("", "review").unwrap() && made.try_get::<bool>("", "work").unwrap()
    );
    let wait = made.try_get::<i64>("", "wait").unwrap();
    assert!((604_700_000..=604_800_000).contains(&wait), "{wait}");
    let weekly = db.activity_review(&owner, &w, 30, None).await.unwrap()["weekly"][0].clone();
    assert_eq!(
        (weekly["enabled"].clone(), weekly["task"].clone()),
        (json!(true), json!(task))
    );
    db.set_weekly_review(&owner, &w, &job.agent, false)
        .await
        .unwrap();
    assert_eq!(
        db.activity_review(&owner, &w, 30, None).await.unwrap()["weekly"][0]["enabled"],
        false
    );
    // On again: the same task resumes.
    let again = db
        .set_weekly_review(&owner, &w, &job.agent, true)
        .await
        .unwrap();
    assert_eq!(again["task"], task.as_str());
    assert_eq!(
        db.activity_review(&owner, &w, 30, None).await.unwrap()["weekly"][0]["enabled"],
        true
    );
    // Someone who only talks to the agent sees nothing to review and can't set goals.
    let member = user(&db, "better-viewer").await;
    join(&db, &owner, &w, &member, "Member").await;
    let theirs = db.activity_review(&member, &w, 30, None).await.unwrap();
    assert!(
        theirs["goals"].as_array().unwrap().is_empty()
            && theirs["reviews"].as_array().unwrap().is_empty()
    );
    assert!(db.save_goal(&member, &w, &goal).await.is_err());
    db.archive_goal(&owner, &w, &id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_question_asked_again_and_again_suggests_a_panel_to_its_asker_only() {
    let (db, owner, w, _, job) = setup("allow").await;
    for text in [
        "How many orders today?",
        "how many orders today",
        "How many orders today?!",
    ] {
        db.chat_send(
            &owner,
            &w,
            &job.chat,
            &super::super::super::super::tests::message(text, Some(&job.agent)),
        )
        .await
        .unwrap();
    }
    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &super::super::super::super::tests::message("Thanks!", Some(&job.agent)),
    )
    .await
    .unwrap();
    let mine = db.panel_suggestions(&owner, &w).await.unwrap();
    assert_eq!(mine["suggestions"].as_array().unwrap().len(), 1, "{mine}");
    assert_eq!(mine["suggestions"][0]["count"], 3);
    assert_eq!(mine["suggestions"][0]["agent"], job.agent.as_str());
    let member = user(&db, "suggest-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(
        db.panel_suggestions(&member, &w).await.unwrap()["suggestions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "only from their own messages"
    );
}
