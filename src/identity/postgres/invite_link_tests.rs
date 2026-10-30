//! Invite links: preview before sign-in, accept only with the invited email,
//! replaced links stop working, finished invitations stay finished.
use super::tests::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_invite_link_shows_the_invitation_and_joins_only_the_invited_email() {
    let db = fixture().await;
    let owner = user(&db, "link-owner").await;
    let w = workspace(&db, &owner).await;
    let (guest, stranger) = (
        user(&db, "link-guest").await,
        user(&db, "link-stranger").await,
    );
    let id = db
        .reserve_invite(&owner, &w, &guest.email, "Member")
        .await
        .unwrap();
    db.delivered(&id, Some("invitation_fixture")).await.unwrap();
    let first = db.issue_invite_link(&owner, &w, &id).await.unwrap();
    assert!(
        db.issue_invite_link(&guest, &w, &id).await.is_err(),
        "only managers issue links"
    );

    let preview = db.invite_preview(&first, None).await.unwrap();
    assert_eq!(preview["status"], "pending");
    assert!(preview["email_hint"].as_str().unwrap().contains("***@"));
    assert!(
        preview.get("matches").is_none(),
        "anonymous viewers learn nothing about matching"
    );
    assert_eq!(
        db.invite_preview("not-a-real-link", None).await.unwrap()["status"],
        "invalid"
    );

    // A replaced link stops working; the new one carries on.
    let second = db.issue_invite_link(&owner, &w, &id).await.unwrap();
    assert_eq!(
        db.invite_preview(&first, None).await.unwrap()["status"],
        "invalid"
    );
    assert_eq!(
        db.invite_preview(&second, Some(&stranger)).await.unwrap()["matches"],
        false
    );
    let wrong = db.accept_invite_link(&stranger, &second).await.unwrap_err();
    assert!(wrong.to_string().starts_with("This invitation is for "));

    assert_eq!(db.accept_invite_link(&guest, &second).await.unwrap(), w);
    let done = db.invite_preview(&second, Some(&guest)).await.unwrap();
    assert_eq!(done["status"], "accepted");
    assert_eq!(done["workspace_id"], w.as_str());
    assert!(
        db.decline_invite_link(&guest, &second).await.is_err(),
        "an accepted invitation cannot be declined"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn sign_in_remembers_the_invite_page_to_return_to() {
    let db = fixture().await;
    let flow = crate::identity::Pending {
        state: "state".into(),
        verifier: "verifier".into(),
        invitation: None,
        return_to: Some("/invite/abcdefghijklmnopqrstuvwxyz0123456789".into()),
        expires: crate::identity::user::now() + 600,
    };
    let hash = crate::identity::provider::digest("return-flow");
    db.save_login_flow(&hash, &flow).await.unwrap();
    let taken = db.take_login_flow(&hash).await.unwrap().unwrap();
    assert_eq!(taken.return_to, flow.return_to);
}
