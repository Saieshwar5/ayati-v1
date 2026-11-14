//! Subjects (schema 118): a room per subject, subjects side by side, the
//! subject filled into marked tools, and forgetting a subject.
use super::events_tests::{automation, result};
use super::tests::{choice, reviewed, studyhub};
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::connectors::fake_system::{self, FakeSystem};
use crate::identity::control::model::ActionRequest;

struct Subjects {
    db: PgStore,
    owner: User,
    w: String,
    agent: String,
    system: String,
    key: String,
    _fake: FakeSystem,
}

/// StudyHub with get_profile marked "Needs the subject", events on, and a
/// "Per subject" automation.
async fn connected() -> Subjects {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let (db, owner, w, _c, agent) = super::super::automation_tests::setup().await;
    let mut input = studyhub(&fake.url);
    let mut profile = choice("get_profile", "read", "allow");
    profile.subject = true;
    input.tools.push(profile);
    let reviewed = reviewed(&w, &input).await;
    db.connect_system(&owner, &w, &agent, &input, &reviewed)
        .await
        .unwrap();
    let key = db
        .system_events_key(&owner, &w, &agent, &input.id, true)
        .await
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut per_subject = automation(&input.id, &["exam"]);
    per_subject.memory = Some("subject".into());
    db.create_system_trigger(&owner, &w, &agent, &per_subject)
        .await
        .unwrap();
    Subjects {
        db,
        owner,
        w,
        agent,
        system: input.id,
        key,
        _fake: fake,
    }
}

impl Subjects {
    async fn send(&self, id: &str, subject: &str) {
        self.db
            .system_event(&self.system, &self.key, &result(id, subject), 100000)
            .await
            .unwrap();
    }
    async fn job(&self, event: &str) -> Option<Job> {
        query(&self.db.db, "SELECT j.payload FROM relay_event_deliveries d JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply WHERE d.event_id=$1",
            vec![event.into()]).await.unwrap()
            .map(|r| serde_json::from_value(r.try_get("", "payload").unwrap()).unwrap())
    }
    async fn finish(&self, job: &Job) {
        let mut claimed = job.clone();
        claimed.claim = Some(uuid::Uuid::new_v4().to_string());
        execute(&self.db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",
            vec![job.reply.clone().into(), claimed.claim.clone().into()]).await.unwrap();
        self.db
            .chat_finish(&claimed, &format!("PLAN FOR {}", job.reply), "complete")
            .await
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn each_subject_has_its_own_room_and_subjects_run_side_by_side() {
    let s = connected().await;
    s.send("ravi-1", "stu-104").await;
    s.send("meena-1", "stu-221").await;
    s.send("ravi-2", "stu-104").await;
    assert_eq!(
        s.db.dispatch_events().await.unwrap(),
        2,
        "two subjects start together"
    );
    let (ravi, meena) = (
        s.job("ravi-1").await.unwrap(),
        s.job("meena-1").await.unwrap(),
    );
    assert!(
        s.job("ravi-2").await.is_none(),
        "one subject's events wait their turn"
    );
    assert_ne!(ravi.chat, meena.chat, "each subject has its own room");
    s.finish(&ravi).await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let again = s.job("ravi-2").await.unwrap();
    assert_eq!(
        again.chat, ravi.chat,
        "a subject's later run sees its earlier ones"
    );
    let room: Vec<Value> = json_rows(
        &s.db.db,
        "SELECT jsonb_build_object('c',content) AS data FROM relay_chat_messages WHERE chat=$1",
        vec![meena.chat.clone().into()],
    )
    .await
    .unwrap();
    assert!(
        !json!(room)
            .to_string()
            .contains(&format!("PLAN FOR {}", ravi.reply)),
        "subjects never see each other"
    );

    // Forgetting a subject clears its room; its next event starts fresh.
    s.finish(&again).await;
    let forgotten =
        s.db.forget_subject(&s.owner, &s.w, &s.agent, &s.system, "stu-104")
            .await
            .unwrap();
    assert_eq!(forgotten["forgotten"], 1);
    let cleared = json_rows(
        &s.db.db,
        "SELECT jsonb_build_object('c',content) AS data FROM relay_chat_messages WHERE chat=$1",
        vec![ravi.chat.clone().into()],
    )
    .await
    .unwrap();
    assert!(!json!(cleared).to_string().contains("PLAN FOR"));
    s.send("ravi-3", "stu-104").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    assert_ne!(s.job("ravi-3").await.unwrap().chat, ravi.chat);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_marked_tool_gets_its_subject_from_the_event_and_never_from_the_agent() {
    let s = connected().await;
    s.send("ravi-1", "stu-104").await;
    s.db.dispatch_events().await.unwrap();
    let ravi = s.job("ravi-1").await.unwrap();
    // The agent's listing leaves the subject out.
    let mut claimed = ravi.clone();
    claimed.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&s.db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1",
        vec![ravi.reply.clone().into(), claimed.claim.clone().into()]).await.unwrap();
    let listed = s.db.control_operations(&claimed).await.unwrap();
    let profile = listed["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["operation"] == "get_profile")
        .unwrap()
        .clone();
    assert!(
        profile["schema"]["properties"].get("subject").is_none(),
        "{profile}"
    );
    // Cuckoo fills it in from the event.
    let mut request = ActionRequest {
        connection: s.system.clone(),
        operation: "get_profile".into(),
        arguments: json!({}),
    };
    s.db.fill_subject(&claimed, &mut request).await.unwrap();
    assert_eq!(request.arguments, json!({"subject":"stu-104"}));
    // The agent can't name a subject itself, even its own.
    let mut forged = ActionRequest {
        connection: s.system.clone(),
        operation: "get_profile".into(),
        arguments: json!({"subject":"stu-221"}),
    };
    assert!(s.db.fill_subject(&claimed, &mut forged).await.is_err());
    // A run without a subject (a request in Chat) can't use the tool.
    let chat = Job {
        reply: uuid::Uuid::new_v4().to_string(),
        ..claimed.clone()
    };
    let mut plain = ActionRequest {
        connection: s.system.clone(),
        operation: "get_profile".into(),
        arguments: json!({}),
    };
    assert!(s.db.fill_subject(&chat, &mut plain).await.is_err());
    // Unmarked tools are untouched.
    let mut marks = ActionRequest {
        connection: s.system.clone(),
        operation: "get_marks".into(),
        arguments: json!({"student":"stu-221"}),
    };
    s.db.fill_subject(&claimed, &mut marks).await.unwrap();
    assert_eq!(marks.arguments, json!({"student":"stu-221"}));
}
