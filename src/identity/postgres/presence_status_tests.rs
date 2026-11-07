//! A status shows to every member until it is cleared or its end passes,
//! which is checked when it is read; the database refuses what the HTTP layer
//! would have.
use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_status_is_set_cleared_and_expires_when_read() {
    let db = fixture().await;
    let owner = user(&db, "status-owner").await;
    let member = user(&db, "status-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (listener, mut heard) = listening(&db).await;
    let t = now_ms();
    let hour = t + 3_600_000;
    let mine = db
        .set_status(&member, &w, Some(&meeting(Some(hour))), t)
        .await
        .unwrap();
    assert_eq!(
        (
            mine["status"]["kind"].clone(),
            mine["status"]["clear_at"].clone(),
            mine["status"]["set_at"].clone()
        ),
        (json!("meeting"), json!(hour), json!(t))
    );
    let set = announced(&mut heard, &w, Duration::from_secs(2)).await;
    assert_eq!(set.len(), 1);
    assert_eq!(set[0]["status"]["emoji"], "🗓️");
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, hour - 1).await["status"]["kind"],
        "meeting"
    );
    // Its end is checked when read; nothing had to clear it.
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, hour).await["status"],
        Value::Null
    );
    let stored: Option<String> = query(
        &db.db,
        "SELECT status_kind FROM relay_member_presence WHERE workspace=$1 AND user_id=$2",
        vec![w.clone().into(), member.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "status_kind")
    .unwrap();
    assert_eq!(stored.as_deref(), Some("meeting"));

    let custom = Status {
        kind: "custom".into(),
        emoji: Some("🚚".into()),
        text: Some("Out on deliveries".into()),
        clear_at: None,
    };
    db.set_status(&member, &w, Some(&custom), t).await.unwrap();
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, t + 30 * 86_400_000).await["status"]["text"],
        "Out on deliveries",
        "never clears"
    );
    let before = seen_by(&db, &owner, &w, &member.id, t).await["revision"]
        .as_i64()
        .unwrap();
    let cleared = db.set_status(&member, &w, None, t).await.unwrap();
    assert_eq!(
        (cleared["status"].clone(), cleared["revision"].clone()),
        (Value::Null, json!(before + 1))
    );
    db.set_status(&member, &w, None, t).await.unwrap();
    assert_eq!(
        seen_by(&db, &owner, &w, &member.id, t).await["revision"],
        json!(before + 1),
        "clearing nothing changes nothing"
    );
    assert_eq!(
        announced(&mut heard, &w, Duration::from_secs(2))
            .await
            .len(),
        2,
        "the custom status and the clearing"
    );

    // The database refuses what the HTTP layer would have: too long, or no text.
    let long = Status {
        text: Some("a".repeat(81)),
        ..custom.clone()
    };
    assert!(db.set_status(&member, &w, Some(&long), t).await.is_err());
    let empty = Status {
        text: None,
        ..custom
    };
    assert!(db.set_status(&member, &w, Some(&empty), t).await.is_err());
    assert!(
        Status {
            emoji: Some("🙂🙂".into()),
            ..meeting(None)
        }
        .checked(t)
        .is_err()
    );
    listener.abort();
}
