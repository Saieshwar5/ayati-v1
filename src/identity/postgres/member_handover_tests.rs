use super::{tests::*, *};

async fn audit_summary(db: &PgStore, w: &str) -> String {
    query(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='membership_changed' ORDER BY id DESC LIMIT 1",
        vec![w.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "summary")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn removal_hands_agents_library_and_invitations_to_the_chosen_person() {
    let db = fixture().await;
    let admin = user(&db, "handover-admin").await;
    let leaving = user(&db, "handover-leaving").await;
    let heir = user(&db, "handover-heir").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &leaving, "Member").await;
    join(&db, &admin, &w, &heir, "Member").await;
    db.change(&admin, &w, &leaving.id, Some("Admin"), None)
        .await
        .unwrap();
    assert_eq!(
        audit_summary(&db, &w).await,
        "handover-admin@example.test made handover-leaving@example.test an admin"
    );

    // The leaving admin owns an agent, looks after a Library page and sent an invitation.
    let (_, agent) = super::executor_slot_tests::agent(&db, &admin, &w).await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET owner_id=$3 WHERE workspace=$1 AND id=$2",
        vec![
            w.clone().into(),
            agent.clone().into(),
            leaving.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    execute(&db.db, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,visibility,document,text,created,updated) VALUES($1,'page-1','page','Price list',$2,$2,'workspace','{}'::jsonb,'',1,1),($1,'page-2','page','My notes',$2,$2,'private','{}'::jsonb,'',1,1)", vec![w.clone().into(), leaving.id.clone().into()]).await.unwrap();
    db.reserve_invite(&leaving, &w, "pending@example.test", "Member")
        .await
        .unwrap();

    let before = db
        .member_responsibilities(&admin, &w, &leaving.id)
        .await
        .unwrap();
    assert_eq!(before["agents"].as_array().unwrap().len(), 1);
    assert_eq!(before["library_items"], 1);
    assert_eq!(before["invitations"], 1);
    assert!(
        db.member_responsibilities(&heir, &w, &leaving.id)
            .await
            .is_err(),
        "members cannot see this"
    );

    // An invalid choice changes nothing.
    assert!(
        db.change(&admin, &w, &leaving.id, None, Some(&leaving.id))
            .await
            .is_err()
    );
    let outsider = user(&db, "handover-outsider").await;
    assert!(
        db.change(&admin, &w, &leaving.id, None, Some(&outsider.id))
            .await
            .is_err()
    );
    assert_eq!(db.role(&w, &leaving.id).await.unwrap(), "Admin");
    assert_eq!(
        db.member_responsibilities(&admin, &w, &leaving.id)
            .await
            .unwrap()["agents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    db.change(&admin, &w, &leaving.id, None, Some(&heir.id))
        .await
        .unwrap();
    assert!(db.role(&w, &leaving.id).await.is_err());
    let owner: String = query(
        &db.db,
        "SELECT owner_id FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), agent.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "owner_id")
    .unwrap();
    assert_eq!(owner, heir.id);
    let steward: String = query(
        &db.db,
        "SELECT steward FROM relay_library_items WHERE workspace=$1 AND id='page-1'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "steward")
    .unwrap();
    assert_eq!(steward, heir.id);
    let pending: i64 = query(
        &db.db,
        "SELECT count(*) AS count FROM relay_invitations WHERE workspace=$1 AND status='pending'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "count")
    .unwrap();
    assert_eq!(pending, 0);
    assert_eq!(
        audit_summary(&db, &w).await,
        "handover-admin@example.test removed handover-leaving@example.test; 1 agent and 1 Library item moved to handover-heir@example.test; 1 invitation cancelled; 1 private or selected Library item left with their sharing unchanged"
    );
    // The private page stays private: nobody manages it and the admin cannot read it.
    let private_steward: Option<String> = query(
        &db.db,
        "SELECT steward FROM relay_library_items WHERE workspace=$1 AND id='page-2'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "steward")
    .unwrap();
    assert_eq!(private_steward, None);
    assert!(db.library_get(&admin, &w, "page-2").await.is_err());
    // The heir now looks after the item without an admin takeover.
    assert!(
        db.member_responsibilities(&admin, &w, &heir.id)
            .await
            .unwrap()["library_items"]
            == 1
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn removal_defaults_to_the_acting_admin_and_members_cannot_remove() {
    let db = fixture().await;
    let admin = user(&db, "default-heir-admin").await;
    let member = user(&db, "default-heir-member").await;
    let other = user(&db, "default-heir-other").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &member, "Member").await;
    join(&db, &admin, &w, &other, "Member").await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &admin, &w).await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET owner_id=$3 WHERE workspace=$1 AND id=$2",
        vec![
            w.clone().into(),
            agent.clone().into(),
            member.id.clone().into(),
        ],
    )
    .await
    .unwrap();

    assert!(db.change(&other, &w, &member.id, None, None).await.is_err());
    assert!(
        db.member_responsibilities(&other, &w, &member.id)
            .await
            .is_err()
    );
    db.change(&admin, &w, &member.id, None, None).await.unwrap();
    let owner: String = query(
        &db.db,
        "SELECT owner_id FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), agent.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "owner_id")
    .unwrap();
    assert_eq!(owner, admin.id);
    assert_eq!(
        audit_summary(&db, &w).await,
        "default-heir-admin@example.test removed default-heir-member@example.test; 1 agent moved to default-heir-admin@example.test"
    );
    let activity = db
        .workspace_activity(&admin, &w, "all", None)
        .await
        .unwrap();
    assert_eq!(
        activity["activity"][0]["actor"],
        "default-heir-admin@example.test"
    );
    assert_eq!(
        activity["activity"][0]["sentence"],
        "removed default-heir-member@example.test; 1 agent moved to default-heir-admin@example.test"
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn members_leave_on_their_own_and_the_last_admin_cannot() {
    let db = fixture().await;
    let admin = user(&db, "leave-admin").await;
    let member = user(&db, "leave-member").await;
    let other = user(&db, "leave-other").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &member, "Member").await;
    join(&db, &admin, &w, &other, "Member").await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &admin, &w).await;
    execute(
        &db.db,
        "UPDATE relay_shared_agents SET owner_id=$3 WHERE workspace=$1 AND id=$2",
        vec![
            w.clone().into(),
            agent.clone().into(),
            member.id.clone().into(),
        ],
    )
    .await
    .unwrap();

    // They see what they look after, but not what someone else does.
    let own = db
        .member_responsibilities(&member, &w, &member.id)
        .await
        .unwrap();
    assert_eq!(own["agents"].as_array().unwrap().len(), 1);
    assert!(
        db.member_responsibilities(&member, &w, &other.id)
            .await
            .is_err()
    );
    // Leaving names who takes over; a member still cannot change anyone else.
    assert!(
        db.change(&member, &w, &member.id, None, None)
            .await
            .is_err()
    );
    assert!(
        db.change(&member, &w, &member.id, Some("Admin"), None)
            .await
            .is_err()
    );
    db.change(&member, &w, &member.id, None, Some(&other.id))
        .await
        .unwrap();
    assert!(db.role(&w, &member.id).await.is_err());
    let owner: String = query(
        &db.db,
        "SELECT owner_id FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), agent.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "owner_id")
    .unwrap();
    assert_eq!(owner, other.id);
    assert_eq!(
        audit_summary(&db, &w).await,
        "leave-member@example.test left the workspace; 1 agent moved to leave-other@example.test"
    );

    let refused = db
        .change(&admin, &w, &admin.id, None, Some(&other.id))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("at least one admin"));
    assert_eq!(db.role(&w, &admin.id).await.unwrap(), "Admin");
}
