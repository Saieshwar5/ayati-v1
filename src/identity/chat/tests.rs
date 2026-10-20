use super::*;
use store::{NewChat, NewMessage};
fn setup() -> (Store, User, User, User, String) {
    let db = Store::open(Path::new(":memory:")).unwrap();
    let owner = db.login("owner", "owner@example.com", "a", "s").unwrap();
    let member = db.login("member", "member@example.com", "b", "s").unwrap();
    let outsider = db
        .login("outside", "outside@example.com", "c", "s")
        .unwrap();
    let workspace = uuid::Uuid::new_v4().to_string();
    db.create(&owner, "Business", &workspace).unwrap();
    let invite = db
        .reserve_invite(&owner, &workspace, &member.email, "Member")
        .unwrap();
    db.delivered(&invite, Some("provider")).unwrap();
    db.accept(&member, &invite).unwrap();
    (db, owner, member, outsider, workspace)
}
fn conversation(
    db: &Store,
    owner: &User,
    workspace: &str,
    users: Vec<String>,
    agents: Vec<String>,
) -> String {
    db.chat_create(
        owner,
        workspace,
        &NewChat {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Operations".into(),
            users,
            agents,
        },
    )
    .unwrap()
}
fn message(agent: Option<&str>) -> NewMessage {
    NewMessage {
        id: uuid::Uuid::new_v4().to_string(),
        content: "Check the shared plan".into(),
        agent: agent.map(str::to_owned),
    }
}
#[test]
fn human_messages_are_private_durable_and_deduplicated() {
    let (db, owner, member, outsider, w) = setup();
    let chat = conversation(&db, &owner, &w, vec![member.id.clone()], vec![]);
    let input = message(None);
    assert!(db.chat_send(&member, &w, &chat, &input).unwrap().is_none());
    db.chat_send(&member, &w, &chat, &input).unwrap();
    let data = db.chat_messages(&owner, &w, &chat).unwrap();
    assert_eq!(data["messages"].as_array().unwrap().len(), 1);
    assert!(db.chat_messages(&outsider, &w, &chat).is_err());
    assert!(db.chat_send(&outsider, &w, &chat, &message(None)).is_err());
    let private = conversation(&db, &owner, &w, vec![], vec![]);
    assert!(db.chat_messages(&member, &w, &private).is_err());
    assert!(db.chat_send(&owner, &w, &private, &input).is_err());
}
#[test]
fn agent_participation_requires_requester_grant_and_does_not_inherit_private_history() {
    let (db, owner, member, _, w) = setup();
    let chat = conversation(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec!["local-agent".into()],
    );
    assert!(
        db.chat_send(&member, &w, &chat, &message(Some("local-agent")))
            .is_err()
    );
    assert!(
        db.chat_grant(&member, &w, &member.id, "local-agent", true)
            .is_err()
    );
    db.chat_grant(&owner, &w, &member.id, "local-agent", true)
        .unwrap();
    let private = conversation(&db, &owner, &w, vec![], vec![]);
    let mut secret = message(None);
    secret.content = "PRIVATE-MARKER".into();
    db.chat_send(&owner, &w, &private, &secret).unwrap();
    let input = message(Some("local-agent"));
    let job = db.chat_send(&member, &w, &chat, &input).unwrap().unwrap();
    assert!(!job.prompt.contains("PRIVATE-MARKER"));
    assert!(job.prompt.contains(&input.content));
    assert!(db.chat_send(&member, &w, &chat, &input).unwrap().is_none());
    db.chat_grant(&owner, &w, &member.id, "local-agent", false)
        .unwrap();
    assert!(!db.chat_job_allowed(&job));
    db.chat_finish(&job, "SHOULD-NOT-PUBLISH", "complete")
        .unwrap();
    let result = db.chat_messages(&owner, &w, &chat).unwrap();
    assert!(!result.to_string().contains("SHOULD-NOT-PUBLISH"));
    assert_eq!(result["messages"][1]["state"], "cancelled");
}
#[test]
fn removal_revokes_chat_and_grants_even_after_reinvitation() {
    let (db, owner, member, _, w) = setup();
    let chat = conversation(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec!["local-agent".into()],
    );
    db.chat_grant(&owner, &w, &member.id, "local-agent", true)
        .unwrap();
    let job = db
        .chat_send(&member, &w, &chat, &message(Some("local-agent")))
        .unwrap()
        .unwrap();
    db.change(&owner, &w, &member.id, None).unwrap();
    assert!(!db.chat_job_allowed(&job));
    assert!(db.chat_messages(&member, &w, &chat).is_err());
    let invite = db
        .reserve_invite(&owner, &w, &member.email, "Member")
        .unwrap();
    db.delivered(&invite, Some("new")).unwrap();
    db.accept(&member, &invite).unwrap();
    assert!(db.chat_messages(&member, &w, &chat).is_err());
    assert!(!store::agent_access(&db.0.lock().unwrap(), &member.id, &w, "local-agent").unwrap());
}
#[test]
fn foreign_workspace_members_cannot_be_added_or_read_conversations() {
    let (db, owner, member, outsider, w) = setup();
    let other = uuid::Uuid::new_v4().to_string();
    db.create(&outsider, "Other", &other).unwrap();
    let invalid = NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Bad".into(),
        users: vec![outsider.id.clone()],
        agents: vec![],
    };
    assert!(db.chat_create(&owner, &w, &invalid).is_err());
    let chat = conversation(&db, &owner, &w, vec![member.id], vec![]);
    assert!(db.chat_messages(&outsider, &other, &chat).is_err());
}
#[test]
fn restart_keeps_messages_but_does_not_repeat_agent_calls() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("identity.sqlite");
    let db = Store::open(&path).unwrap();
    let owner = db.login("o", "o@example.com", "h", "s").unwrap();
    let w = uuid::Uuid::new_v4().to_string();
    db.create(&owner, "Business", &w).unwrap();
    let chat = conversation(&db, &owner, &w, vec![], vec!["local-agent".into()]);
    db.chat_send(&owner, &w, &chat, &message(Some("local-agent")))
        .unwrap();
    drop(db);
    let reopened = Store::open(&path).unwrap();
    let messages = reopened.chat_messages(&owner, &w, &chat).unwrap();
    assert_eq!(messages["messages"].as_array().unwrap().len(), 2);
    assert_eq!(messages["messages"][1]["state"], "failed");
}
