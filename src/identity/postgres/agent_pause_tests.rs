use super::super::super::automation_tests::{schedule, setup, state};
use super::super::super::tests::{fixture, join, message, user, workspace};
use super::*;

fn note(text: &str) -> PauseInput {
    PauseInput {
        note: Some(text.into()),
    }
}

async fn reply_state(db: &PgStore, reply: &str) -> (String, String) {
    let row = query(&db.db, "SELECT m.state,m.content,j.state AS job FROM relay_chat_messages m JOIN relay_chat_jobs j ON j.reply=m.id WHERE m.id=$1", vec![reply.into()])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "job").unwrap(), "finished");
    (
        row.try_get("", "state").unwrap(),
        row.try_get("", "content").unwrap(),
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pausing_stops_all_work_and_refuses_new_work_until_an_admin_resumes() {
    let db = fixture().await;
    let admin = user(&db, "pause-admin").await;
    let member = user(&db, "pause-member").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &member, "Member").await;
    let (c, agent) = super::super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    let first = db
        .chat_send(
            &admin,
            &w,
            &c,
            &message("Email every customer", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    let running = db.claim_job().await.unwrap().unwrap();
    assert_eq!(running.reply, first.reply);
    let queued = db
        .chat_send(
            &admin,
            &w,
            &c,
            &message("And then the suppliers", Some(&agent)),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        db.agents_pause_state(&admin, &w).await.unwrap()["active"],
        2
    );

    assert!(
        db.pause_agents(&member, &w, &note("No")).await.is_err(),
        "only admins pause"
    );
    assert!(
        db.agents_pause_state(&member, &w).await.unwrap()["active"].is_null(),
        "members don't see the count"
    );
    let paused = db
        .pause_agents(&admin, &w, &note("Wrong emails going out"))
        .await
        .unwrap();
    assert_eq!(
        (paused["paused"].clone(), paused["active"].clone()),
        (true.into(), 0.into())
    );
    assert_eq!(paused["note"], "Wrong emails going out");
    // Both requests stopped, running or not; the running one can't publish later.
    for reply in [&first.reply, &queued.reply] {
        assert_eq!(
            reply_state(&db, reply).await,
            (
                "cancelled".into(),
                "Stopped because all agents were paused.".into()
            )
        );
    }
    // A late completion from the stopped attempt can't bring the reply back.
    let _ = db
        .chat_finish(&running, "Sent 400 emails.", "complete")
        .await;
    assert_eq!(reply_state(&db, &first.reply).await.0, "cancelled");
    // New work is refused with a reason every path recognises.
    let refused = db
        .chat_send(&admin, &w, &c, &message("Anything", Some(&agent)))
        .await
        .unwrap_err();
    assert!(
        refused.is::<crate::identity::chat::model::AgentsPaused>(),
        "{refused}"
    );
    // A person-to-person message still works.
    db.chat_send(&admin, &w, &c, &message("Agents are paused for now", None))
        .await
        .unwrap();
    // Pausing again keeps the first time, person and reason.
    let again = db
        .pause_agents(&admin, &w, &note("Second reason"))
        .await
        .unwrap();
    assert_eq!(
        (again["since_ms"].clone(), again["note"].clone()),
        (paused["since_ms"].clone(), "Wrong emails going out".into())
    );
    assert_eq!(
        db.snapshot(&member).await.unwrap()["workspaces"][0]["agents_paused_ms"],
        paused["since_ms"]
    );

    // An agent paused on its own stays paused after the workspace resumes.
    execute(&db.db, "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{paused}','true'::jsonb) WHERE id=$1", vec![agent.clone().into()]).await.unwrap();
    assert!(
        db.resume_agents(&member, &w).await.is_err(),
        "only admins resume"
    );
    assert_eq!(db.resume_agents(&admin, &w).await.unwrap()["paused"], false);
    assert!(
        db.chat_send(&admin, &w, &c, &message("Try again", Some(&agent)))
            .await
            .is_err(),
        "its own pause still applies"
    );
    execute(&db.db, "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{paused}','false'::jsonb) WHERE id=$1", vec![agent.clone().into()]).await.unwrap();
    db.chat_send(&admin, &w, &c, &message("Try again", Some(&agent)))
        .await
        .unwrap();
    // Stopped work stays stopped, and the audit log says who did what.
    assert_eq!(reply_state(&db, &queued.reply).await.0, "cancelled");
    let log = db
        .workspace_activity(&admin, &w, "all", None)
        .await
        .unwrap();
    let sentences: Vec<&str> = log["activity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["sentence"].as_str().unwrap())
        .collect();
    assert!(
        sentences.contains(&"paused all agents and stopped 2 requests: Wrong emails going out"),
        "{sentences:?}"
    );
    assert!(sentences.contains(&"resumed all agents"));
    let events: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_sync_events WHERE workspace=$1 AND kind='agents'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(
        events, 2,
        "every open tab hears about the pause and the resume"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_schedule_due_while_paused_skips_that_run_and_stays_on() {
    let (db, owner, w, c, agent) = setup().await;
    let daily = schedule(&c, Some(86400));
    let once = schedule(&c, None);
    db.save_schedule(&owner, &w, &agent, &daily).await.unwrap();
    db.save_schedule(&owner, &w, &agent, &once).await.unwrap();
    db.pause_agents(&owner, &w, &PauseInput::default())
        .await
        .unwrap();
    assert_eq!(db.dispatch_schedules_at(daily.first_ms).await.unwrap(), 0);
    let skipped = state(&db, &w, &daily.id).await;
    assert_eq!(skipped["state"], "enabled");
    assert_eq!(skipped["next_ms"], daily.first_ms + 86_400_000);
    assert_eq!(
        skipped["detail"],
        "The last run was skipped because all agents were paused."
    );
    // A one-off can't skip to a later time: it waits for its person.
    let missed = state(&db, &w, &once.id).await;
    assert_eq!(
        (missed["state"].clone(), missed["next_ms"].clone()),
        ("blocked".into(), once.first_ms.into())
    );
    assert_eq!(
        missed["detail"],
        "Missed because all agents were paused. Resume it to run it again."
    );
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
    assert_eq!(jobs, 0, "nothing ran while paused");
    // After resuming, the daily schedule runs at its next time as usual.
    db.resume_agents(&owner, &w).await.unwrap();
    assert_eq!(
        db.dispatch_schedules_at(daily.first_ms + 86_400_000)
            .await
            .unwrap(),
        1
    );
}
