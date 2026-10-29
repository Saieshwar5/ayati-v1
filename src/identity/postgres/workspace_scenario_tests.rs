//! Multi-workspace and competing-administrator regression scenarios on real PostgreSQL.
use super::tests::{fixture, join, user, workspace};

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn uncertain_invitation_requires_explicit_cancellation_before_retry() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let guest = user(&db, "guest").await;
    let w = workspace(&db, &owner).await;
    let id = db
        .reserve_invite(&owner, &w, &guest.email, "User")
        .await
        .unwrap();
    db.delivered(&id, None).await.unwrap();
    assert!(db.accept(&guest, &id).await.is_err());
    assert!(
        db.reserve_invite(&owner, &w, &guest.email, "User")
            .await
            .is_err()
    );
    db.cancel(&owner, &w, &id).await.unwrap();
    let retry = db
        .reserve_invite(&owner, &w, &guest.email, "User")
        .await
        .unwrap();
    db.delivered(&retry, Some("fixture")).await.unwrap();
    db.accept(&guest, &retry).await.unwrap();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn ownership_and_revocation_are_scoped_to_each_workspace() {
    let db = fixture().await;
    let a = user(&db, "alice").await;
    let b = user(&db, "bob").await;
    let wa = workspace(&db, &a).await;
    let wb = workspace(&db, &b).await;
    join(&db, &a, &wa, &b, "User").await;
    join(&db, &b, &wb, &a, "User").await;
    assert!(db.workspace_settings(&b, &wa).await.is_err());
    assert!(db.workspace_settings(&a, &wb).await.is_err());
    assert!(db.change(&b, &wa, &b.id, Some("Owner")).await.is_err());
    db.change(&a, &wa, &b.id, Some("Owner")).await.unwrap();
    db.change(&a, &wa, &b.id, None).await.unwrap();
    assert_eq!(db.role(&wb, &b.id).await.unwrap(), "Owner");
    assert_eq!(
        db.snapshot(&b).await.unwrap()["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    join(&db, &a, &wa, &b, "User").await;
    assert_eq!(db.role(&wa, &b.id).await.unwrap(), "User");
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn concurrent_owners_preserve_last_owner_and_detect_stale_name_saves() {
    let db = fixture().await;
    let a = user(&db, "a").await;
    let b = user(&db, "b").await;
    let c = user(&db, "c").await;
    let w = workspace(&db, &a).await;
    for person in [&b, &c] {
        join(&db, &a, &w, person, "User").await;
        db.change(&a, &w, &person.id, Some("Owner")).await.unwrap();
    }
    let (one, two) = tokio::join!(
        db.rename_workspace(&a, &w, "First edit", "Test business"),
        db.rename_workspace(&b, &w, "Second edit", "Test business")
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let (one, two, three) = tokio::join!(
        db.change(&a, &w, &a.id, Some("User")),
        db.change(&b, &w, &b.id, Some("User")),
        db.change(&c, &w, &c.id, None)
    );
    assert_eq!([one, two, three].iter().filter(|r| r.is_ok()).count(), 2);
    let people = [&a, &b, &c];
    let mut owners = Vec::new();
    for person in people {
        if db
            .role(&w, &person.id)
            .await
            .is_ok_and(|role| role == "Owner")
        {
            owners.push(person);
        } else {
            assert!(db.workspace_activity(person, &w).await.is_err());
        }
    }
    assert_eq!(owners.len(), 1);
    assert!(db.change(owners[0], &w, &owners[0].id, None).await.is_err());
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn invitation_accept_cancel_races_leave_consistent_membership() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let guest = user(&db, "guest").await;
    let w = workspace(&db, &owner).await;
    for _ in 0..12 {
        let id = db
            .reserve_invite(&owner, &w, &guest.email, "User")
            .await
            .unwrap();
        db.delivered(&id, Some("fixture")).await.unwrap();
        let (accepted, cancelled) =
            tokio::join!(db.accept(&guest, &id), db.cancel(&owner, &w, &id));
        cancelled.unwrap();
        let member = db.role(&w, &guest.id).await.is_ok();
        assert_eq!(accepted.is_ok(), member);
        assert!(db.accept(&guest, &id).await.is_err());
        if member {
            db.change(&owner, &w, &guest.id, None).await.unwrap();
        }
    }
}
