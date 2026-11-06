//! Agents run app actions only once allowed, as the person they work for, never beyond
//! that person's roles; a retried run doesn't repeat; runs are audited.
use super::super::super::executor_slot_tests;
use super::super::super::tests::{chat, fixture, join, message, user, workspace};
use super::super::{AppRelease, NewApp};
use super::AgentRule;
use crate::identity::control::broker::invoke;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_file_leave_for_a_person_but_cannot_approve_it() {
    let db = fixture().await;
    let owner = user(&db, "actions-owner").await;
    let ravi = user(&db, "actions-ravi").await;
    let mina = user(&db, "actions-mina").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &ravi, "Member").await;
    join(&db, &owner, &w, &mina, "Member").await;
    let (_, agent) = executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &mina.id, &agent, true)
        .await
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let leave = NewApp {
        id: id.clone(),
        name: "Leave".into(),
        description: String::new(),
        audience: "everyone".into(),
        people: vec![],
        template: "leave".into(),
    };
    db.built_app_create(&ravi, &w, &leave).await.unwrap();
    assert!(
        db.built_app_files(&ravi, &w, &id, 0).await.unwrap()["checks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.built_app_publish(
        &ravi,
        &w,
        &id,
        &AppRelease {
            note: String::new(),
            revision: 1,
        },
    )
    .await
    .unwrap();
    let ask = |text: &'static str| {
        let (db, w, mina, agent) = (&db, w.clone(), mina.clone(), agent.clone());
        async move {
            let conversation = chat(db, &mina, &w, vec![], vec![agent.clone()]).await;
            db.chat_send(&mina, &w, &conversation, &message(text, Some(&agent)))
                .await
                .unwrap();
            db.claim_job().await.unwrap().unwrap()
        }
    };
    // Every action starts blocked.
    let job = ask("File two days of leave from 10 November").await;
    assert!(!db.built_app_actions_offered(&job).await.unwrap());
    assert_eq!(
        invoke(&db, &job, "app_actions", json!({})).await.unwrap()["apps"],
        json!([])
    );
    let file = json!({"app":id,"action":"file_leave","data":{"start":"2026-11-10","days":2,"reason":"Family"}});
    assert_eq!(
        invoke(&db, &job, "run_app_action", file.clone())
            .await
            .unwrap()["state"],
        "refused"
    );
    db.chat_finish(&job, "Not allowed.", "complete")
        .await
        .unwrap();
    // Members see the rules; only the builder or an admin changes them.
    let rules = db.built_app_agent_rules(&mina, &w, &id).await.unwrap();
    assert_eq!(rules["actions"].as_array().unwrap().len(), 3);
    assert!(
        rules["actions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["mode"] == "block")
    );
    let allow = |action: &str| AgentRule {
        action: action.into(),
        mode: "allow".into(),
    };
    assert!(
        db.built_app_agent_rule(&mina, &w, &id, &allow("file_leave"))
            .await
            .is_err()
    );
    assert!(
        db.built_app_agent_rule(&ravi, &w, &id, &allow("pay_salary"))
            .await
            .is_err()
    );
    db.built_app_agent_rule(&ravi, &w, &id, &allow("file_leave"))
        .await
        .unwrap();
    db.built_app_agent_rule(&owner, &w, &id, &allow("my_leave"))
        .await
        .unwrap();
    // Mina's agent sees the two allowed actions and files her leave, once.
    let job = ask("File two days of leave from 10 November").await;
    assert!(db.built_app_actions_offered(&job).await.unwrap());
    let offered = invoke(&db, &job, "app_actions", json!({})).await.unwrap();
    let names: Vec<&str> = offered["apps"][0]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["file_leave", "my_leave"]);
    assert_eq!(
        offered["apps"][0]["actions"][0]["fields"][1],
        json!({"name":"days","type":"number"})
    );
    let done = invoke(&db, &job, "run_app_action", file.clone())
        .await
        .unwrap();
    assert_eq!(done["state"], "done");
    assert_eq!(done["result"]["mine"], true, "filed as Mina");
    assert!(
        done["result"]["agent"].is_string(),
        "the record names the agent"
    );
    let again = invoke(&db, &job, "run_app_action", file.clone())
        .await
        .unwrap();
    assert_eq!(again["repeated"], true);
    let record = done["result"]["id"].as_str().unwrap().to_owned();
    // It can't set fields beyond the action's, approve, or exceed Mina's own access.
    let sneaky = json!({"app":id,"action":"file_leave","data":{"days":1,"status":"approved"}});
    assert!(
        invoke(&db, &job, "run_app_action", sneaky).await.unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("not \"status\"")
    );
    let approve = json!({"app":id,"action":"decide_leave","record":record,"data":{"status":"approved"},"revision":1});
    assert_eq!(
        invoke(&db, &job, "run_app_action", approve.clone())
            .await
            .unwrap()["state"],
        "refused"
    );
    let bad = json!({"app":id,"action":"file_leave","data":{"start":"soon","days":1}});
    assert_eq!(
        invoke(&db, &job, "run_app_action", bad).await.unwrap()["state"],
        "refused"
    );
    let seen = invoke(
        &db,
        &job,
        "run_app_action",
        json!({"app":id,"action":"my_leave"}),
    )
    .await
    .unwrap();
    assert_eq!(seen["result"]["records"].as_array().unwrap().len(), 1);
    db.chat_finish(&job, "Filed.", "complete").await.unwrap();
    // Even allowed, deciding needs the approver role, which Mina doesn't hold.
    db.built_app_agent_rule(&ravi, &w, &id, &allow("decide_leave"))
        .await
        .unwrap();
    let job = ask("Approve my leave").await;
    let refused = invoke(&db, &job, "run_app_action", approve).await.unwrap();
    assert_eq!(refused["state"], "refused");
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("can't update requests"),
        "{refused}"
    );
    db.chat_finish(&job, "No.", "complete").await.unwrap();
    // Blocking again closes it; runs and rule changes are audited.
    db.built_app_agent_rule(
        &ravi,
        &w,
        &id,
        &AgentRule {
            action: "file_leave".into(),
            mode: "block".into(),
        },
    )
    .await
    .unwrap();
    let job = ask("File one more day").await;
    let file_more = json!({"app":id,"action":"file_leave","data":{"start":"2026-12-01","days":1}});
    assert_eq!(
        invoke(&db, &job, "run_app_action", file_more)
            .await
            .unwrap()["state"],
        "refused"
    );
    let audit: Vec<String> = crate::identity::postgres::rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND summary IS NOT NULL",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get::<String>("", "summary").unwrap())
    .collect();
    let said = audit.join("\n");
    assert!(
        said.contains("did \"File a leave request\" in \"Leave\" for"),
        "{said}"
    );
    assert!(
        said.contains("Agents may \"File a leave request\" in \"Leave\""),
        "{said}"
    );
    assert!(
        said.contains("Agents may no longer \"File a leave request\""),
        "{said}"
    );
}
