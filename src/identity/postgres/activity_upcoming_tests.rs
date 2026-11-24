//! Activity › Calendar › List: the people who look after an agent, and the person a
//! task is for, see the runs it booked. Schedules became tasks in schema 147 and
//! are never listed as schedules.
use crate::identity::postgres::automation_tests::setup;
use crate::identity::postgres::execute;
use crate::identity::postgres::tests::{join, user};
use serde_json::Value;

fn titles(page: &Value, kind: &str) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == kind)
        .map(|i| i["title"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn upcoming_lists_booked_task_runs_for_the_people_who_may_see_them() {
    let (db, owner, w, c, agent) = setup().await;
    let admin = user(&db, "upcoming-admin").await;
    let member = user(&db, "upcoming-member").await;
    join(&db, &owner, &w, &admin, "Member").await;
    execute(
        &db.db,
        "UPDATE relay_members SET role='Admin' WHERE workspace=$1 AND user_id=$2",
        vec![w.clone().into(), admin.id.clone().into()],
    )
    .await
    .unwrap();
    join(&db, &owner, &w, &member, "Member").await;

    let task = uuid::Uuid::new_v4().to_string();
    execute(&db.db, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,repeat,anchor_ms) VALUES($1,$2,$3,'commitment','schedule','Morning stock check',$4,$5,'{\"unit\":\"day\",\"at\":\"09:00\"}',1)",
        vec![w.clone().into(), task.clone().into(), agent.clone().into(), owner.id.clone().into(), c.clone().into()]).await.unwrap();
    execute(&db.db, "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind) VALUES($1,$2,$3,(extract(epoch FROM clock_timestamp())*1000)::bigint+3600000,'repeat')",
        vec![w.clone().into(), uuid::Uuid::new_v4().to_string().into(), task.clone().into()]).await.unwrap();
    // A schedule row left from before schema 147 is never listed.
    execute(&db.db, "INSERT INTO relay_agent_schedules(workspace,id,agent,principal,chat,name,content,first_ms,interval_seconds,timezone,request_hash,next_ms) VALUES($1,$2,$3,$4,NULL,'Old schedule','Old',$5,86400,'Asia/Kolkata',$2,$5)",
        vec![w.clone().into(), uuid::Uuid::new_v4().to_string().into(), agent.clone().into(), owner.id.clone().into(), (crate::identity::now() * 1000 + 60000).into()]).await.unwrap();

    let mine = db.activity_upcoming(&owner, &w, None).await.unwrap();
    assert_eq!(titles(&mine, "follow_up"), vec!["Morning stock check"]);
    assert!(titles(&mine, "schedule").is_empty());
    let theirs = db.activity_upcoming(&admin, &w, Some("all")).await.unwrap();
    assert_eq!(titles(&theirs, "follow_up"), vec!["Morning stock check"]);
    let narrowed = db
        .activity_upcoming(&admin, &w, Some("mine"))
        .await
        .unwrap();
    assert!(
        narrowed["items"].as_array().unwrap().is_empty(),
        "My agents: only agents the admin manages"
    );
    let outsider = db.activity_upcoming(&member, &w, None).await.unwrap();
    assert!(outsider["items"].as_array().unwrap().is_empty());
    assert!(
        db.activity_upcoming(&owner, &w, Some("everyone"))
            .await
            .is_err()
    );
}
