use super::*;
use crate::identity::chat::store::{NewChat, NewMessage};

#[test]
fn names_survive_restart_login_and_change_historical_attribution_without_changing_access() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("identity.sqlite");
    // Exercise upgrading a populated database from before display names existed.
    let legacy = Connection::open(&path).unwrap();
    legacy.execute_batch("CREATE TABLE users(id TEXT PRIMARY KEY,provider TEXT UNIQUE NOT NULL,email TEXT NOT NULL); INSERT INTO users VALUES('owner','owner','owner@example.com');").unwrap();
    drop(legacy);
    let db = Store::open(&path).unwrap();
    let owner = db.login("owner", "owner@example.com", "a", "s").unwrap();
    assert!(owner.display_name.is_none());
    let other = db.login("other", "other@example.com", "b", "s").unwrap();
    let w = uuid::Uuid::new_v4().to_string();
    db.create(&owner, "Business", &w).unwrap();
    let chat = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Team".into(),
                kind: "group".into(),
                description: String::new(),
                users: vec![],
                agents: vec![],
            },
        )
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &chat,
        &NewMessage {
            id: uuid::Uuid::new_v4().to_string(),
            content: "Hello".into(),
            agent: None,
            thread: None,
        },
    )
    .unwrap();
    let named = db.update_profile(&owner, "  Sai Eshwar  ").unwrap();
    assert_eq!(named.display_name.as_deref(), Some("Sai Eshwar"));
    assert_eq!(named.id, owner.id);
    assert_eq!(named.email, owner.email);
    assert_eq!(
        db.chat_messages(&owner, &w, &chat).unwrap()["messages"][0]["name"],
        "Sai Eshwar"
    );
    assert_eq!(
        db.people(&owner, &w).unwrap()["members"][0]["display_name"],
        "Sai Eshwar"
    );
    assert_eq!(
        db.directory(&owner, &w).unwrap()["users"][0]["name"],
        "Sai Eshwar"
    );
    db.update_profile(&other, "Sai Eshwar").unwrap(); // Names are not unique identities.
    assert!(db.chat_messages(&other, &w, &chat).is_err());
    assert!(db.people(&other, &w).is_err());
    drop(db);
    let db = Store::open(&path).unwrap();
    assert_eq!(
        db.session("a").unwrap().unwrap().0.display_name,
        named.display_name
    );
    let again = db.login("owner", "new@example.com", "c", "s").unwrap();
    assert_eq!(again.display_name, named.display_name);
    assert_eq!(again.id, owner.id);
    db.update_profile(&again, "సాయి").unwrap();
    assert_eq!(
        db.chat_messages(&again, &w, &chat).unwrap()["messages"][0]["name"],
        "సాయి"
    );
    assert_eq!(db.role(&w, &owner.id).unwrap(), "Owner");
}

#[test]
fn invalid_names_cannot_overwrite_saved_name() {
    let db = Store::open(Path::new(":memory:")).unwrap();
    let user = db.login("owner", "owner@example.com", "a", "s").unwrap();
    db.update_profile(&user, "Sai").unwrap();
    for name in ["", "  ", "Sai\nEshwar", "Sai\0", &"a".repeat(81)] {
        assert!(db.update_profile(&user, name).is_err());
        assert_eq!(
            db.session("a").unwrap().unwrap().0.display_name.as_deref(),
            Some("Sai")
        );
    }
    assert!(db.update_profile(&user, &"స".repeat(80)).is_ok());
}
