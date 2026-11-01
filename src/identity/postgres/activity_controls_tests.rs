//! Controls on Activity's agent cards and requests: pause or resume one agent
//! (owner, managers, admins) and retry a failed request (only the person who
//! asked; never when an action's outcome is unknown).
use super::super::tests::{chat, fixture, join, message, user, workspace};
use super::super::*;

async fn add_manager(db: &PgStore, w: &str, agent: &str, person: &User, by: &User) {
    execute(&db.db, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) VALUES($1,$2,$3,$4,1)", vec![w.into(), agent.into(), person.id.clone().into(), by.id.clone().into()]).await.unwrap();
}

async fn everyone_may_talk(db: &PgStore, agent: &str) {
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET audience='everyone' WHERE id=$1",
        vec![agent.into()],
    )
    .await
    .unwrap();
}

async fn paused(db: &PgStore, agent: &str) -> (bool, i64) {
    let row = query(
        &db.db,
        "SELECT COALESCE((settings->>'paused')::boolean,false) AS p,revision FROM relay_shared_agents WHERE id=$1",
        vec![agent.into()],
    )
    .await
    .unwrap()
    .unwrap();
    (
        row.try_get("", "p").unwrap(),
        row.try_get("", "revision").unwrap(),
    )
}

/// Mark a request failed, as the executor does when an attempt ends in error.
async fn fail(db: &PgStore, reply: &str) {
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET state='finished',lease_until=0 WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET state='failed',content='The connector refused the request.' WHERE id=$1",
        vec![reply.into()],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn managers_owners_and_admins_pause_one_agent_and_others_cannot() {
    let db = fixture().await;
    let admin = user(&db, "pause-admin").await;
    let manager = user(&db, "pause-manager").await;
    let other = user(&db, "pause-other").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &manager, "Member").await;
    join(&db, &admin, &w, &other, "Member").await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    everyone_may_talk(&db, &agent).await;
    add_manager(&db, &w, &agent, &manager, &admin).await;

    // A room with the agent, made before it is paused (a paused agent can't join).
    let room = chat(&db, &admin, &w, vec![other.id.clone()], vec![agent.clone()]).await;
    let refused = db.agent_pause_set(&other, &w, &agent, true).await;
    assert!(refused.is_err(), "a member who doesn't manage it");
    assert!(!paused(&db, &agent).await.0);

    let before = paused(&db, &agent).await.1;
    let result = db
        .agent_pause_set(&manager, &w, &agent, true)
        .await
        .unwrap();
    assert_eq!(result["paused"], true);
    assert_eq!(paused(&db, &agent).await, (true, before + 1));
    // Asking again changes nothing, so a double click is harmless.
    db.agent_pause_set(&manager, &w, &agent, true)
        .await
        .unwrap();
    assert_eq!(paused(&db, &agent).await, (true, before + 1));
    let logged: String = query(&db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='agent_profile_saved' AND target=$2 ORDER BY id DESC LIMIT 1", vec![w.clone().into(), agent.clone().into()]).await.unwrap().unwrap().try_get("", "summary").unwrap();
    assert!(logged.ends_with(" paused Pool agent"), "{logged}");

    // A paused agent takes no new work, and Activity's card says so.
    let sent = db
        .chat_send(&other, &w, &room, &message("Stock count", Some(&agent)))
        .await;
    assert!(!matches!(sent, Ok(Some(_))), "a paused agent took work");
    let now = db.activity_now(&admin, &w, 1, None).await.unwrap();
    let card = now["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == agent.as_str())
        .unwrap()
        .clone();
    assert_eq!(card["paused"], true);
    assert_eq!(card["can_pause"], true, "an admin may pause any agent");
    // A member who doesn't manage it sees the card but may not pause it.
    let theirs = db.activity_now(&other, &w, 1, None).await.unwrap();
    let seen = theirs["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == agent.as_str())
        .unwrap()
        .clone();
    assert_eq!(seen["can_pause"], false);

    // The owner (here also the admin) resumes it.
    db.agent_pause_set(&admin, &w, &agent, false).await.unwrap();
    assert!(!paused(&db, &agent).await.0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_the_asker_retries_a_failed_request_once_and_never_an_unsure_one() {
    let db = fixture().await;
    let admin = user(&db, "retry-admin").await;
    let asker = user(&db, "retry-asker").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &asker, "Member").await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    everyone_may_talk(&db, &agent).await;
    let room = chat(&db, &admin, &w, vec![asker.id.clone()], vec![agent.clone()]).await;
    let job = db
        .chat_send(
            &asker,
            &w,
            &room,
            &message("Update the Kapoor order address", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();

    // A request still running can't be retried.
    assert!(db.request_retry(&asker, &w, &job.reply).await.is_err());
    fail(&db, &job.reply).await;
    // The agent's card points "How to fix it" at today's latest failed request.
    let now = db.activity_now(&asker, &w, 1, None).await.unwrap();
    assert_eq!(now["agents"][0]["last_failed"], job.reply.as_str());
    // Someone else, even an admin, can't start it again in the asker's name.
    let other = db.request_retry(&admin, &w, &job.reply).await;
    assert!(
        other
            .unwrap_err()
            .to_string()
            .contains("Only the person who asked")
    );

    // The details say what kind of request it is, so the screen can offer Retry.
    let details = db.activity_request(&asker, &w, &job.reply).await.unwrap();
    assert_eq!(details["kind"], "chat");
    assert_eq!(details["requester"], asker.id.as_str());
    assert_eq!(details["state"], "failed");
    let first = db.request_retry(&asker, &w, &job.reply).await.unwrap();
    let retry = first["reply"].as_str().unwrap().to_string();
    assert_ne!(retry, job.reply);
    assert_eq!(first["retry_of"], job.reply.as_str());
    // The new request sends the same words, quoting the failed reply.
    let asked = query(&db.db, "SELECT r.content,r.reply_to,r.author FROM relay_chat_jobs j JOIN relay_chat_messages r ON r.id=j.request_message WHERE j.reply=$1", vec![retry.clone().into()]).await.unwrap().unwrap();
    assert_eq!(
        asked.try_get::<String>("", "content").unwrap(),
        "Update the Kapoor order address"
    );
    assert_eq!(
        asked.try_get::<Option<String>>("", "reply_to").unwrap(),
        Some(job.reply.clone())
    );
    assert_eq!(asked.try_get::<String>("", "author").unwrap(), asker.id);
    // Clicking again (or from another tab) gives the same single retry.
    let again = db.request_retry(&asker, &w, &job.reply).await.unwrap();
    assert_eq!(again["reply"], retry.as_str());
    let jobs: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_chat_jobs WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(jobs, 2);

    // A failed request whose action may already have happened is never retried.
    let unsure = db
        .chat_send(
            &asker,
            &w,
            &room,
            &message("Refund order 123", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    fail(&db, &unsure.reply).await;
    db.db
        .execute_unprepared(&format!(
            "INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES('{w}','payments',true,1);
             INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES('{w}','pay','payments','{{\"id\":\"payments\",\"operations\":[]}}','fp');
             INSERT INTO relay_control_actions(id,workspace,agent,chat,job,requester,request_id,connection,operation,arguments,title,effect,rule_revision,connection_revision,approver,state,created,updated)
              VALUES('refund-1','{w}','{agent}','{room}','{reply}','{asker}','r1','pay','refund','{{}}','Refund','write',1,1,'{asker}','uncertain',1,1);",
            reply = unsure.reply,
            asker = asker.id,
        ))
        .await
        .unwrap();
    let refused = db
        .request_retry(&asker, &w, &unsure.reply)
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("may or may not have happened"),
        "{refused}"
    );
}
