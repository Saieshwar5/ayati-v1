//! Owners are equal (schema 122): any owner adds or removes others, and the
//! last one stays.
use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn owners_are_equal_and_add_or_remove_each_other_but_never_the_last_one() {
    let db = fixture().await;
    let admin = user(&db, "owners-admin").await;
    let maker = user(&db, "owners-maker").await;
    let second = user(&db, "owners-second").await;
    let other = user(&db, "owners-other").await;
    let w = workspace(&db, &admin).await;
    for person in [&maker, &second, &other] {
        join(&db, &admin, &w, person, "Member").await;
    }
    let a = agent("only_me");
    db.save_agent_profile(&maker, &w, &a).await.unwrap();
    // The creator is its first owner.
    assert_eq!(owner_ids(&db, &w, &a.id).await, vec![maker.id.clone()]);

    // Someone who doesn't own it can't add owners, or themselves.
    assert!(
        db.change_agent_owners(&other, &w, &a.id, &add(&other, 1))
            .await
            .is_err()
    );
    // A stale revision is refused; outsiders can't become owners.
    assert!(
        db.change_agent_owners(&maker, &w, &a.id, &add(&second, 0))
            .await
            .is_err()
    );
    let outsider = user(&db, "owners-outsider").await;
    assert!(
        db.change_agent_owners(&maker, &w, &a.id, &add(&outsider, 1))
            .await
            .is_err()
    );
    let added = db
        .change_agent_owners(&maker, &w, &a.id, &add(&second, 1))
        .await
        .unwrap();
    assert_eq!(added["revision"], 2);
    assert_eq!(added["owners"].as_array().unwrap().len(), 2);
    // An exact retry changes nothing.
    let again = db
        .change_agent_owners(&maker, &w, &a.id, &add(&second, 1))
        .await
        .unwrap();
    assert_eq!(again["revision"], 2);

    // Both owners are equal: the second one changes the agent, talks to this
    // only-me agent, pauses it and adds or removes owners without asking.
    assert!(db.agent_access(&second.id, &w, &a.id).await.unwrap());
    assert!(!db.agent_access(&other.id, &w, &a.id).await.unwrap());
    let mut renamed = a.clone();
    renamed.revision = 2;
    renamed.name = "Renamed".into();
    db.save_agent_profile(&second, &w, &renamed).await.unwrap();
    db.agent_pause_set(&second, &w, &a.id, true).await.unwrap();
    let listed = db.agent_profiles(&second, &w).await.unwrap();
    assert_eq!(listed[0]["is_owner"], true);
    assert_eq!(listed[0]["editable"], true);
    let names: Vec<_> = listed[0]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].clone())
        .collect();
    assert_eq!(
        names,
        [serde_json::json!(maker.id), serde_json::json!(second.id)]
    );

    // An owner removes another; the agent's owner_id follows the first left.
    db.change_agent_owners(&second, &w, &a.id, &remove(&maker, 4))
        .await
        .unwrap();
    assert_eq!(owner_ids(&db, &w, &a.id).await, vec![second.id.clone()]);
    let row = db.managed_agent(&second, &w, &a.id, true).await.unwrap();
    assert_eq!(row["owner_id"], second.id.as_str());
    assert!(!db.agent_access(&maker.id, &w, &a.id).await.unwrap());
    assert!(db.managed_agent(&maker, &w, &a.id, false).await.is_err());
    // The last owner can't be removed, not even by an admin.
    let last = db
        .change_agent_owners(&admin, &w, &a.id, &remove(&second, 5))
        .await;
    assert!(
        last.unwrap_err()
            .to_string()
            .contains("needs at least one owner")
    );
    // An admin adds an owner to any agent.
    db.change_agent_owners(&admin, &w, &a.id, &add(&maker, 5))
        .await
        .unwrap();
    let summaries: Vec<String> = rows(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action IN ('agent_owner_added','agent_owner_removed') ORDER BY id",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|r| r.try_get("", "summary").unwrap())
    .collect();
    assert_eq!(
        summaries,
        [
            "owners-maker@example.test made owners-second@example.test an owner of Atlas",
            "owners-second@example.test removed owners-maker@example.test as an owner of Renamed",
            "owners-admin@example.test made owners-maker@example.test an owner of Renamed",
        ]
    );

    // At most ten owners.
    for n in 0..8 {
        let person = user(&db, &format!("owners-many-{n}")).await;
        join(&db, &admin, &w, &person, "Member").await;
        let revision = db.managed_agent(&admin, &w, &a.id, true).await.unwrap()["revision"]
            .as_i64()
            .unwrap();
        let result = db
            .change_agent_owners(&admin, &w, &a.id, &add(&person, revision))
            .await;
        assert_eq!(result.is_ok(), n < 8, "{n}");
    }
    let revision = db.managed_agent(&admin, &w, &a.id, true).await.unwrap()["revision"]
        .as_i64()
        .unwrap();
    let eleventh = db
        .change_agent_owners(&admin, &w, &a.id, &add(&other, revision))
        .await;
    assert!(
        eleventh
            .unwrap_err()
            .to_string()
            .contains("up to 10 owners")
    );
}
