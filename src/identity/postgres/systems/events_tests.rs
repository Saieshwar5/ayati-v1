//! Events from a connected system (schema 117): its key, the fixed envelope,
//! duplicates, automations per event type, nested data, and Recent events.
use super::super::tests::*;
use super::automations::FromSystem;
use super::events::{Conflict, Refused};
use super::tests::{reviewed, studyhub};
use super::*;
use crate::identity::connectors::fake_system::{self, FakeSystem};
use crate::identity::events::system_events::Invalid;

pub(crate) struct Connected {
    pub db: PgStore,
    pub owner: User,
    pub w: String,
    pub agent: String,
    pub system: String,
    pub key: String,
    pub _fake: FakeSystem,
}

pub(crate) async fn connected() -> Connected {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let (db, owner, w, _c, agent) = super::super::automation_tests::setup().await;
    let input = studyhub(&fake.url);
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
    Connected {
        db,
        owner,
        w,
        agent,
        system: input.id,
        key,
        _fake: fake,
    }
}

pub(crate) fn automation(system: &str, fields: &[&str]) -> FromSystem {
    FromSystem {
        id: uuid::Uuid::new_v4().to_string(),
        system: system.into(),
        event: "exam.result_published".into(),
        fields: fields.iter().map(|f| (*f).to_owned()).collect(),
        name: "Study plan after results".into(),
        instructions: "Read the marks. If a subject is below 50, save a two-week plan.".into(),
        chat: None,
        memory: None,
        filter: json!({}),
        results: vec![],
    }
}

pub(crate) fn result(id: &str, subject: &str) -> Value {
    json!({"id":id,"type":"exam.result_published","subject":subject,"time":"2026-11-13T09:00:00Z",
        "data":{"exam":"Maths term 1","scores":{"algebra":41,"geometry":[38,52]},"private_note":"NEVER-SHOWN"}})
}

impl Connected {
    pub async fn send(&self, body: &Value) -> Result<Value> {
        self.db
            .system_event(&self.system, &self.key, body, 100000)
            .await
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_system_event_starts_its_automation_with_the_chosen_fields_whole() {
    let s = connected().await;
    // Before any automation, an event is saved and matches nothing.
    let first = s.send(&result("evt-0", "stu-104")).await.unwrap();
    assert_eq!(first, json!({"status":"accepted","matched":0,"id":"evt-0"}));
    let listed =
        s.db.system_events(&s.owner, &s.w, &s.agent, &s.system)
            .await
            .unwrap();
    assert_eq!(listed["recent"][0]["type"], "exam.result_published");
    assert_eq!(listed["recent"][0]["matched"], 0);
    assert_eq!(
        listed["types"][0]["fields"],
        json!(["exam", "private_note", "scores"])
    );
    assert!(
        !listed.to_string().contains("NEVER-SHOWN"),
        "values never shown"
    );

    let saved =
        s.db.create_system_trigger(
            &s.owner,
            &s.w,
            &s.agent,
            &automation(&s.system, &["exam", "scores"]),
        )
        .await
        .unwrap();
    assert_eq!(saved["state"], "enabled");
    // An earlier event is never matched later.
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);

    assert_eq!(
        s.send(&result("evt-1", "stu-104")).await.unwrap()["matched"],
        1
    );
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let row = query(&s.db.db, "SELECT j.payload FROM relay_chat_jobs j JOIN relay_event_deliveries d ON d.workspace=j.workspace AND d.reply=j.reply LIMIT 1", vec![])
        .await.unwrap().unwrap();
    let job: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    let work: bool = query(
        &s.db.db,
        "SELECT work FROM relay_chats WHERE id=$1",
        vec![job.chat.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "work")
    .unwrap();
    assert!(work, "the run happens in a work room");
    let context = json!(s.db.event_job_context(&job).await.unwrap()).to_string();
    assert!(
        context.contains("geometry"),
        "nested data reaches the agent: {context}"
    );
    assert!(context.contains("stu-104") && context.contains("exam.result_published"));
    assert!(
        !context.contains("NEVER-SHOWN"),
        "unchosen fields never reach it"
    );
    assert!(
        !job.prompt.contains("Maths term 1"),
        "event data is not the instructions"
    );

    // A repeat starts nothing; the same id with other content is refused.
    assert_eq!(
        s.send(&result("evt-1", "stu-104")).await.unwrap()["status"],
        "duplicate"
    );
    let mut changed = result("evt-1", "stu-104");
    changed["data"]["exam"] = json!("Science");
    assert!(s.send(&changed).await.unwrap_err().is::<Conflict>());
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn wrong_keys_bad_envelopes_and_oversized_data_are_refused() {
    let s = connected().await;
    s.db.create_system_trigger(
        &s.owner,
        &s.w,
        &s.agent,
        &automation(&s.system, &["exam", "essay"]),
    )
    .await
    .unwrap();
    let wrong =
        s.db.system_event(&s.system, "cke_wrong", &result("e1", "stu-1"), 100000)
            .await;
    assert!(wrong.unwrap_err().is::<Refused>());
    let other = uuid::Uuid::new_v4().to_string();
    assert!(
        s.db.system_event(&other, &s.key, &result("e1", "stu-1"), 100000)
            .await
            .unwrap_err()
            .is::<Refused>()
    );
    for bad in [
        json!({"type":"exam.result_published"}),
        json!({"id":"e2","type":"Exam"}),
        json!({"id":"e3","type":"a","data":[1]}),
    ] {
        assert!(s.send(&bad).await.unwrap_err().is::<Invalid>(), "{bad}");
    }
    let big = json!({"id":"e4","type":"exam.result_published","data":{"exam":"x","essay":"y".repeat(20000)}});
    let error = s.send(&big).await.unwrap_err();
    assert!(error.to_string().contains("essay"), "{error}");
    // A refused event saves no receipt: sending a fixed one with the same id works.
    let fixed =
        json!({"id":"e4","type":"exam.result_published","data":{"exam":"x","essay":"short"}});
    assert_eq!(s.send(&fixed).await.unwrap()["matched"], 1);

    // A new key replaces the old one at once; off refuses everything.
    let rotated =
        s.db.system_events_key(&s.owner, &s.w, &s.agent, &s.system, true)
            .await
            .unwrap();
    assert!(
        s.send(&result("e5", "stu-1"))
            .await
            .unwrap_err()
            .is::<Refused>()
    );
    let new_key = rotated["key"].as_str().unwrap();
    assert!(
        s.db.system_event(&s.system, new_key, &result("e5", "stu-1"), 100000)
            .await
            .is_ok()
    );
    s.db.system_events_key(&s.owner, &s.w, &s.agent, &s.system, false)
        .await
        .unwrap();
    assert!(
        s.db.system_event(&s.system, new_key, &result("e6", "stu-1"), 100000)
            .await
            .unwrap_err()
            .is::<Refused>()
    );
    // The key is only ever stored as a digest.
    let stored: String = query(
        &s.db.db,
        "SELECT events_key FROM relay_connected_systems WHERE connection=$1",
        vec![s.system.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "events_key")
    .unwrap();
    assert_ne!(stored, new_key);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_the_owner_or_an_admin_adds_automations_and_a_test_event_is_marked() {
    let s = connected().await;
    let member = user(&s.db, "events-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    assert!(
        s.db.create_system_trigger(&member, &s.w, &s.agent, &automation(&s.system, &["exam"]))
            .await
            .is_err()
    );
    assert!(
        s.db.system_events_key(&member, &s.w, &s.agent, &s.system, true)
            .await
            .is_err()
    );
    // A filter may only use the fields the agent sees.
    let mut filtered = automation(&s.system, &["exam"]);
    filtered.filter = json!({"private_note":"x"});
    assert!(
        s.db.create_system_trigger(&s.owner, &s.w, &s.agent, &filtered)
            .await
            .is_err()
    );
    s.db.create_system_trigger(&s.owner, &s.w, &s.agent, &automation(&s.system, &["exam"]))
        .await
        .unwrap();
    let sample = json!({"type":"exam.result_published","data":{"exam":"Sample"}});
    let tested =
        s.db.system_test_event(&s.owner, &s.w, &s.agent, &s.system, &sample, 100000)
            .await
            .unwrap();
    assert_eq!(tested["matched"], 1);
    let listed =
        s.db.system_events(&s.owner, &s.w, &s.agent, &s.system)
            .await
            .unwrap();
    assert_eq!(listed["recent"][0]["test"], true);
}

async fn claim(db: &PgStore, event: &str) -> crate::identity::chat::model::Job {
    let claim = uuid::Uuid::new_v4().to_string();
    let row = query(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$1,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300
        WHERE reply=(SELECT d.reply FROM relay_event_deliveries d WHERE d.event_id=$2) RETURNING payload",
        vec![claim.clone().into(), event.into()]).await.unwrap().unwrap();
    let mut job: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(claim);
    job
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_system_reads_each_runs_state_and_its_checked_result_fields() {
    use super::results::ResultField;
    let s = connected().await;
    let mut input = automation(&s.system, &["exam", "scores"]);
    input.results = vec![
        ResultField {
            name: "plan_saved".into(),
            kind: "yes_no".into(),
        },
        ResultField {
            name: "weak_subject".into(),
            kind: "text".into(),
        },
    ];
    s.db.create_system_trigger(&s.owner, &s.w, &s.agent, &input)
        .await
        .unwrap();
    s.send(&result("evt-1", "stu-104")).await.unwrap();
    let read = |id: &str| {
        let (db, system, key, id) = (s.db.clone(), s.system.clone(), s.key.clone(), id.to_owned());
        async move { db.system_event_runs(&system, &key, &id).await.unwrap()["runs"][0].clone() }
    };
    assert_eq!(read("evt-1").await["state"], "queued");
    assert!(
        s.db.system_event_runs(&s.system, "cke_wrong", "evt-1")
            .await
            .unwrap_err()
            .is::<Refused>()
    );
    assert!(
        s.db.system_event_runs(&s.system, &s.key, "never-sent")
            .await
            .is_err()
    );

    s.db.dispatch_events().await.unwrap();
    let job = claim(&s.db, "evt-1").await;
    assert!(
        job.prompt.contains("report_result")
            || {
                let message: String = query(&s.db.db, "SELECT content FROM relay_chat_messages WHERE id=(SELECT message FROM relay_event_deliveries WHERE event_id='evt-1')", vec![])
            .await.unwrap().unwrap().try_get("", "content").unwrap();
                message.contains("report_result once with these fields: plan_saved (yes/no), weak_subject (text)")
            }
    );
    assert_eq!(read("evt-1").await["state"], "working");
    // Wrong fields are refused with the reason; the right ones are kept.
    let wrong =
        s.db.report_result(&job, &json!({"plan_saved":"yes","weak_subject":"Maths"}))
            .await
            .unwrap();
    assert_eq!(wrong["state"], "invalid");
    assert!(wrong["message"].as_str().unwrap().contains("plan_saved"));
    let right =
        s.db.report_result(&job, &json!({"plan_saved":true,"weak_subject":"Maths"}))
            .await
            .unwrap();
    assert_eq!(right["state"], "completed");
    s.db.chat_finish(&job, "A two-week Maths plan is saved.", "complete")
        .await
        .unwrap();
    let done = read("evt-1").await;
    assert_eq!(done["state"], "done");
    assert_eq!(done["answer"], "A two-week Maths plan is saved.");
    assert_eq!(
        done["result"],
        json!({"plan_saved":true,"weak_subject":"Maths"})
    );

    // A run that finishes without its result fields counts as failed.
    s.send(&result("evt-2", "stu-221")).await.unwrap();
    s.db.dispatch_events().await.unwrap();
    let second = claim(&s.db, "evt-2").await;
    s.db.chat_finish(&second, "Done.", "complete")
        .await
        .unwrap();
    let failed = read("evt-2").await;
    assert_eq!(failed["state"], "failed");
    assert!(failed["reason"].as_str().unwrap().contains("result fields"));

    // Cancel stops a run that hasn't finished; a finished one keeps its result.
    s.send(&result("evt-3", "stu-300")).await.unwrap();
    let stopped =
        s.db.cancel_system_event(&s.system, &s.key, "evt-3")
            .await
            .unwrap();
    assert_eq!(stopped["stopped"], 1);
    assert_eq!(read("evt-3").await["state"], "cancelled");
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    assert_eq!(read("evt-1").await["state"], "done");
    // A request from no automation has nothing to report.
    let other =
        s.db.report_result(
            &crate::identity::chat::model::Job {
                reply: uuid::Uuid::new_v4().to_string(),
                ..job.clone()
            },
            &json!({}),
        )
        .await
        .unwrap();
    assert_eq!(other["state"], "denied");
}
