//! Presence costs one write per online person per 30 s, announces only what
//! people can see, and shows nobody to anyone outside their workspace.
use super::*;
use crate::identity::postgres::{
    executor_slot_tests::agent,
    messaging::Live,
    settings::preferences::Preferences,
    tests::{fixture, join, user, workspace},
};
use futures_util::FutureExt;
use sea_orm_migration::MigratorTrait;
use std::time::Duration;
use tokio::sync::broadcast::Receiver;

#[path = "presence_status_tests.rs"]
mod status;

/// Starts this store's listener and waits until it is connected.
async fn listening(db: &PgStore) -> (tokio::task::JoinHandle<Result<()>>, Receiver<Live>) {
    let mut heard = db.changes.subscribe();
    let listener = tokio::spawn({
        let db = db.clone();
        async move { db.listen_changes().await }
    });
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), heard.recv()).await;
        if event.unwrap().unwrap() == Live::Everything {
            break;
        }
    }
    (listener, heard)
}

/// Presence changes announced to `w`'s streams within `wait`.
async fn announced(heard: &mut Receiver<Live>, w: &str, wait: Duration) -> Vec<Value> {
    let until = tokio::time::Instant::now() + wait;
    let mut people = vec![];
    while let Ok(Ok(event)) = tokio::time::timeout_at(until, heard.recv()).await {
        if let Live::Presence(change) = event
            && change.workspace == w
        {
            people.push(change.person.clone());
        }
    }
    people
}

/// What `viewer` sees of `person` at `at`.
async fn seen_by(db: &PgStore, viewer: &User, w: &str, person: &str, at: i64) -> Value {
    db.presence(viewer, w, at)
        .await
        .unwrap()
        .into_iter()
        .find(|view| view["id"] == person)
        .unwrap_or(Value::Null)
}

/// The row's version (`xmin` changes with every write) and last beat.
async fn written(db: &PgStore, w: &str, person: &str) -> (String, i64) {
    let row = query(&db.db, "SELECT xmin::text AS version,seen_ms FROM relay_member_presence WHERE workspace=$1 AND user_id=$2",
        vec![w.into(), person.into()]).await.unwrap().unwrap();
    (
        row.try_get("", "version").unwrap(),
        row.try_get("", "seen_ms").unwrap(),
    )
}

fn hide(shown: bool) -> Preferences {
    Preferences {
        show_online: Some(shown),
        ..Preferences::default()
    }
}

fn meeting(clear_at: Option<i64>) -> Status {
    Status {
        kind: "meeting".into(),
        emoji: Some("🗓️".into()),
        text: None,
        clear_at,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn two_tabs_on_two_controllers_write_one_beat_per_thirty_seconds() {
    let db = fixture().await;
    let owner = user(&db, "beat-owner").await;
    let w = workspace(&db, &owner).await;
    let me = (w.clone(), owner.id.clone());
    let t = now_ms();
    // Two controllers record the first beat together: one comes online.
    let other = db.clone();
    let both_tabs = [me.clone(), me.clone()];
    let (a, b) = tokio::join!(
        db.beat_presence(std::slice::from_ref(&me), t),
        other.beat_presence(&both_tabs, t)
    );
    assert_eq!(a.unwrap() + b.unwrap(), 1, "one transition");
    let first = written(&db, &w, &owner.id).await;
    assert_eq!(first.1, t);
    // The other tab's beats within the gap write nothing at all.
    for later in [1_000, 15_000, MIN_GAP_MS - 1] {
        assert_eq!(
            db.beat_presence(std::slice::from_ref(&me), t + later)
                .await
                .unwrap(),
            0
        );
        assert_eq!(written(&db, &w, &owner.id).await, first, "{later} ms later");
    }
    let revision = seen_by(&db, &owner, &w, &owner.id, t).await["revision"].clone();
    assert_eq!(
        db.beat_presence(std::slice::from_ref(&me), t + MIN_GAP_MS)
            .await
            .unwrap(),
        0,
        "still online"
    );
    let refreshed = written(&db, &w, &owner.id).await;
    assert_ne!(refreshed.0, first.0, "one write after the gap");
    assert_eq!(refreshed.1, t + MIN_GAP_MS);
    assert_eq!(
        seen_by(&db, &owner, &w, &owner.id, t + MIN_GAP_MS).await["revision"],
        revision,
        "a refresh changes nothing anyone sees"
    );
    // A clock behind the last beat never moves it back.
    db.beat_presence(std::slice::from_ref(&me), t)
        .await
        .unwrap();
    assert_eq!(written(&db, &w, &owner.id).await, refreshed);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn people_are_online_within_the_ttl_then_offline_with_one_announcement() {
    let db = fixture().await;
    let owner = user(&db, "ttl-owner").await;
    let member = user(&db, "ttl-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (listener, mut heard) = listening(&db).await;
    let _ = db.work.presence.notified().now_or_never();
    let t = now_ms();
    assert_eq!(
        db.beat_presence(&[(w.clone(), member.id.clone())], t)
            .await
            .unwrap(),
        1
    );
    let came = announced(&mut heard, &w, Duration::from_secs(2)).await;
    assert_eq!(came.len(), 1, "{came:?}");
    assert_eq!(
        (came[0]["id"].clone(), came[0]["online"].clone()),
        (json!(member.id), json!(true))
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(3), db.work.presence.notified())
            .await
            .is_ok(),
        "coming online wakes the sweep"
    );
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, t + TTL_MS - 1).await["online"],
        true
    );

    // Not yet expired: nothing changes, and the sweep knows when to look again.
    let next = db.expire_presence(t + TTL_MS - 1).await.unwrap();
    assert_eq!(next, Some(Duration::from_millis(1)));
    // Expired by the time it is read, before any sweep.
    let read = seen_by(&db, &owner, &w, &member.id, t + TTL_MS).await;
    assert_eq!(
        (read["online"].clone(), read["last_seen"].clone()),
        (json!(false), json!(t))
    );
    assert_eq!(
        db.expire_presence(t + TTL_MS).await.unwrap(),
        None,
        "nobody is left online"
    );
    assert_eq!(db.expire_presence(t + TTL_MS + 30_000).await.unwrap(), None);
    let went = announced(&mut heard, &w, Duration::from_secs(2)).await;
    assert_eq!(went.len(), 1, "exactly one offline change: {went:?}");
    assert_eq!(
        (
            went[0]["online"].clone(),
            went[0]["last_seen"].clone(),
            went[0]["revision"].clone()
        ),
        (
            json!(false),
            json!(t),
            json!(came[0]["revision"].as_i64().unwrap() + 1)
        )
    );
    // Back again: a new transition.
    assert_eq!(
        db.beat_presence(&[(w.clone(), member.id.clone())], t + TTL_MS + 40_000)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        announced(&mut heard, &w, Duration::from_secs(2))
            .await
            .len(),
        1
    );
    listener.abort();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn hidden_presence_is_never_shown_but_its_owner_still_sees_others() {
    let db = fixture().await;
    let owner = user(&db, "hide-owner").await;
    let member = user(&db, "hide-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let saved = db.save_preferences(&owner, &w, &hide(false)).await.unwrap();
    assert_eq!(saved["show_online"], false);
    assert_eq!(
        db.preferences(&owner, &w).await.unwrap()["show_online"],
        false
    );
    assert_eq!(
        db.preferences(&member, &w).await.unwrap()["show_online"],
        true,
        "on by default"
    );
    let t = now_ms();
    db.set_status(&owner, &w, Some(&meeting(None)), t)
        .await
        .unwrap();
    let (listener, mut heard) = listening(&db).await;
    let people = [
        (w.clone(), owner.id.clone()),
        (w.clone(), member.id.clone()),
    ];
    assert_eq!(
        db.beat_presence(&people, t).await.unwrap(),
        2,
        "both are online"
    );
    let shown = announced(&mut heard, &w, Duration::from_secs(2)).await;
    assert_eq!(
        shown.len(),
        1,
        "only the visible person is announced: {shown:?}"
    );
    assert_eq!(shown[0]["id"], json!(member.id));

    let hidden = seen_by(&db, &member, &w, &owner.id, t).await;
    assert_eq!(
        (hidden["online"].clone(), hidden["last_seen"].clone()),
        (json!(false), Value::Null)
    );
    assert_eq!(
        hidden["status"]["kind"], "meeting",
        "the status still shows"
    );
    assert_eq!(
        seen_by(&db, &owner, &w, &owner.id, t).await["online"],
        false,
        "hidden from themselves too"
    );
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, t).await["online"],
        true,
        "they still see others"
    );
    let directory = db.directory(&member, &w).await.unwrap();
    let listed = directory["presence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|view| view["id"] == owner.id)
        .unwrap()
        .clone();
    assert_eq!(
        (listed["online"].clone(), listed["last_seen"].clone()),
        (json!(false), Value::Null)
    );
    // Coming and going while hidden changes nothing anyone sees.
    let revision = hidden["revision"].clone();
    assert_eq!(db.expire_presence(t + TTL_MS).await.unwrap(), None);
    assert_eq!(
        announced(&mut heard, &w, Duration::from_secs(1))
            .await
            .iter()
            .filter(|view| view["id"] == owner.id)
            .count(),
        0
    );
    db.beat_presence(&people[..1], t + TTL_MS + 1)
        .await
        .unwrap();
    assert_eq!(
        seen_by(&db, &member, &w, &owner.id, t + TTL_MS + 1).await["revision"],
        revision
    );

    // Showing again announces them at once; a notification save without the
    // choice keeps it as it is.
    db.save_preferences(&owner, &w, &hide(true)).await.unwrap();
    let back = announced(&mut heard, &w, Duration::from_secs(2)).await;
    assert_eq!(back.len(), 1);
    assert_eq!(
        (back[0]["id"].clone(), back[0]["online"].clone()),
        (json!(owner.id), json!(true))
    );
    db.save_preferences(&owner, &w, &Preferences::default())
        .await
        .unwrap();
    assert_eq!(
        db.preferences(&owner, &w).await.unwrap()["show_online"],
        true
    );
    db.save_preferences(&member, &w, &hide(false))
        .await
        .unwrap();
    db.save_preferences(&member, &w, &Preferences::default())
        .await
        .unwrap();
    assert_eq!(
        db.preferences(&member, &w).await.unwrap()["show_online"],
        false,
        "an older page cannot unhide anyone"
    );
    listener.abort();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_and_other_workspaces_have_no_presence_here() {
    let db = fixture().await;
    let owner = user(&db, "scope-owner").await;
    let outsider = user(&db, "scope-outsider").await;
    let w = workspace(&db, &owner).await;
    let theirs = workspace(&db, &outsider).await;
    let (_, robot) = agent(&db, &owner, &w).await;
    let t = now_ms();
    db.set_status(&owner, &w, Some(&meeting(None)), t)
        .await
        .unwrap();
    let beats = [
        (w.clone(), owner.id.clone()),
        (w.clone(), robot.clone()),
        (w.clone(), outsider.id.clone()),
        (theirs.clone(), outsider.id.clone()),
    ];
    assert_eq!(
        db.beat_presence(&beats, t).await.unwrap(),
        2,
        "only members, each in their own workspace"
    );
    let people = db.presence(&owner, &w, t).await.unwrap();
    let ids: Vec<&str> = people
        .iter()
        .map(|view| view["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [owner.id.as_str()], "no agent and no outsider");
    assert!(
        db.directory(&owner, &w).await.unwrap()["presence"]
            .as_array()
            .unwrap()
            .iter()
            .all(|view| view["id"] != robot)
    );
    // The outsider reads nothing of this workspace, and cannot set a status in it.
    assert!(db.presence(&outsider, &w, t).await.is_err());
    assert!(db.directory(&outsider, &w).await.is_err());
    assert!(
        db.set_status(&outsider, &w, Some(&meeting(None)), t)
            .await
            .is_err()
    );
    let own: Vec<Value> = db.presence(&outsider, &theirs, t).await.unwrap();
    assert!(own.iter().all(|view| view["id"] == outsider.id), "{own:?}");
    assert!(db.presence(&owner, &theirs, t).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_98_starts_everyone_offline_and_shown() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_presence_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    super::super::super::migration::Migrator::up(&db.db, Some(97))
        .await
        .unwrap();
    assert!(db.verify_schema().await.is_err(), "schema 97 is refused");
    let owner = user(&db, "presence-upgrade-owner").await;
    let member = user(&db, "presence-upgrade-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    // Notification choices saved before the upgrade.
    execute(&db.db, "INSERT INTO relay_member_preferences(workspace,user_id,desktop_alerts) VALUES($1,$2,FALSE)",
        vec![w.clone().into(), member.id.clone().into()]).await.unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let t = now_ms();
    assert_eq!(
        db.presence(&owner, &w, t).await.unwrap(),
        Vec::<Value>::new(),
        "nobody is online yet"
    );
    let kept = db.preferences(&member, &w).await.unwrap();
    assert_eq!(
        (kept["desktop_alerts"].clone(), kept["show_online"].clone()),
        (json!(false), json!(true))
    );
    assert_eq!(
        db.beat_presence(&[(w.clone(), member.id.clone())], t)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, t).await["online"],
        true
    );
}
