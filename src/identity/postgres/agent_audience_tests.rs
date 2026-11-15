use super::tests::{chat, fixture, join, message, user, workspace};
use super::*;
use crate::identity::agents::config::{SaveAgent, Settings};
use crate::identity::chat::model::NewChat;
use sea_orm_migration::MigratorTrait;

fn agent(audience: &str) -> SaveAgent {
    SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Atlas".into(),
        role: "Orders".into(),
        model: "fixture".into(),
        audience: audience.into(),
        people: None,
        revision: 0,
        settings: Settings::default(),
    }
}

fn direct(agent: &str) -> NewChat {
    NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Direct".into(),
        kind: "direct".into(),
        description: String::new(),
        users: vec![],
        agents: vec![agent.into()],
    }
}

/// The managers an agent had before schema 122, kept as a record only.
async fn former_managers(db: &PgStore, w: &str, agent: &str) -> Vec<String> {
    rows(
        &db.db,
        "SELECT user_id FROM relay_agent_former_managers WHERE workspace=$1 AND agent=$2 ORDER BY user_id",
        vec![w.into(), agent.into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|row| row.try_get("", "user_id").unwrap())
    .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn who_can_talk_is_enforced_directly_and_channel_members_may_ask_there() {
    let db = fixture().await;
    let admin = user(&db, "talk-admin").await;
    let maker = user(&db, "talk-maker").await;
    let chosen = user(&db, "talk-chosen").await;
    let other = user(&db, "talk-other").await;
    let w = workspace(&db, &admin).await;
    for person in [&maker, &chosen, &other] {
        join(&db, &admin, &w, person, "Member").await;
    }

    // A member creates an agent for selected people without any admin grant.
    let mut a = agent("selected");
    a.people = Some(vec![chosen.id.clone()]);
    db.save_agent_profile(&maker, &w, &a).await.unwrap();
    for (person, allowed) in [
        (&maker, true),
        (&chosen, true),
        (&admin, true),
        (&other, false),
    ] {
        assert_eq!(
            db.agent_access(&person.id, &w, &a.id).await.unwrap(),
            allowed,
            "{}",
            person.id
        );
    }
    assert!(db.chat_create(&other, &w, &direct(&a.id)).await.is_err());
    assert!(db.agent_profiles(&other, &w).await.unwrap().is_empty());
    let listed = db.agent_profiles(&maker, &w).await.unwrap();
    assert_eq!(listed[0]["people"], serde_json::json!([chosen.id]));
    assert_eq!(listed[0]["audience"], "selected");
    // One owner and no managers (schema 122).
    assert_eq!(listed[0]["owner_id"], maker.id.as_str());
    assert_eq!(listed[0]["needs_owner"], false);
    assert!(listed[0].get("managers").is_none());

    // In a group the agent belongs to, everyone in it may ask.
    let group = chat(&db, &maker, &w, vec![other.id.clone()], vec![a.id.clone()]).await;
    let job = db
        .chat_send(
            &other,
            &w,
            &group,
            &message("Where is order 12?", Some(&a.id)),
        )
        .await
        .unwrap();
    assert!(job.is_some());
    assert!(db.chat_create(&other, &w, &direct(&a.id)).await.is_err());

    // Only me is refused while the agent is in a group; people must be members.
    let mut narrowed = a.clone();
    narrowed.revision = 1;
    narrowed.audience = "only_me".into();
    narrowed.settings.paused = true;
    assert!(db.save_agent_profile(&maker, &w, &narrowed).await.is_err());
    let mut everyone = a.clone();
    everyone.revision = 1;
    everyone.settings.paused = true;
    everyone.audience = "everyone".into();
    everyone.people = Some(vec!["not-a-member".into()]);
    assert!(db.save_agent_profile(&maker, &w, &everyone).await.is_err());
    everyone.people = None;
    db.save_agent_profile(&maker, &w, &everyone).await.unwrap();

    // Another member may not change someone else's agent; an admin may.
    let mut theirs = everyone.clone();
    theirs.revision = 2;
    theirs.name = "Renamed".into();
    assert!(db.save_agent_profile(&other, &w, &theirs).await.is_err());
    db.save_agent_profile(&admin, &w, &theirs).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn owners_hand_agents_over_to_a_new_owner() {
    let db = fixture().await;
    let admin = user(&db, "transfer-admin").await;
    let maker = user(&db, "transfer-maker").await;
    let heir = user(&db, "transfer-heir").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &maker, "Member").await;
    join(&db, &admin, &w, &heir, "Member").await;
    let a = agent("only_me");
    db.save_agent_profile(&maker, &w, &a).await.unwrap();
    let to = |owner: &User, revision| TransferAgent {
        owner: owner.id.clone(),
        revision,
    };

    assert!(
        db.agent_transfer(&heir, &w, &a.id, &to(&heir, 1))
            .await
            .is_err()
    );
    assert!(
        db.agent_transfer(&maker, &w, &a.id, &to(&heir, 0))
            .await
            .is_err()
    );
    let outsider = user(&db, "transfer-outsider").await;
    assert!(
        db.agent_transfer(&maker, &w, &a.id, &to(&outsider, 1))
            .await
            .is_err()
    );
    let moved = db
        .agent_transfer(&maker, &w, &a.id, &to(&heir, 1))
        .await
        .unwrap();
    assert_eq!(moved["revision"], 2);
    // An exact retry succeeds without a second change.
    db.agent_transfer(&maker, &w, &a.id, &to(&heir, 1))
        .await
        .unwrap();

    let owned = db.managed_agent(&heir, &w, &a.id, true).await.unwrap();
    assert_eq!(owned["owner_id"], heir.id.as_str());
    assert!(db.agent_access(&heir.id, &w, &a.id).await.unwrap());
    assert!(!db.agent_access(&maker.id, &w, &a.id).await.unwrap());
    assert!(db.managed_agent(&maker, &w, &a.id, false).await.is_err());
    let summary: String = query(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='agent_owner_changed'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "summary")
    .unwrap();
    assert_eq!(
        summary,
        "transfer-maker@example.test made transfer-heir@example.test the owner of Atlas"
    );
}

async fn last_summary(db: &PgStore, w: &str) -> String {
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
#[ignore = "requires disposable PostgreSQL"]
async fn an_agent_whose_owner_leaves_pauses_until_an_admin_gives_it_an_owner() {
    let db = fixture().await;
    let admin = user(&db, "remove-admin").await;
    let leaving = user(&db, "remove-leaving").await;
    let next = user(&db, "remove-next").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &leaving, "Member").await;
    join(&db, &admin, &w, &next, "Member").await;
    let mine = agent("only_me");
    db.save_agent_profile(&leaving, &w, &mine).await.unwrap();
    let running = agent("everyone");
    db.save_agent_profile(&leaving, &w, &running).await.unwrap();
    let mut resting = agent("everyone");
    resting.settings.paused = true;
    db.save_agent_profile(&leaving, &w, &resting).await.unwrap();

    let before = db
        .member_responsibilities(&admin, &w, &leaving.id)
        .await
        .unwrap();
    assert_eq!(before["agents"].as_array().unwrap().len(), 3);
    // Removed without naming a new owner: every agent pauses with no owner.
    db.change(&admin, &w, &leaving.id, None, None)
        .await
        .unwrap();
    assert_eq!(
        last_summary(&db, &w).await,
        "remove-admin@example.test removed remove-leaving@example.test; 3 agents paused until an admin gives them an owner"
    );
    for a in [&mine, &running, &resting] {
        let left = db.managed_agent(&admin, &w, &a.id, true).await.unwrap();
        assert!(left["owner_id"].is_null());
        assert_eq!(left["settings"]["paused"], true);
    }
    let listed = db.agent_profiles(&admin, &w).await.unwrap();
    assert!(
        listed
            .iter()
            .all(|a| a["needs_owner"] == true && a["owner_name"] == "Former member")
    );
    // Nobody talks to it or resumes it while it has no owner, not even an admin.
    assert!(
        db.chat_create(&next, &w, &direct(&running.id))
            .await
            .is_err()
    );
    let refused = db.agent_pause_set(&admin, &w, &running.id, false).await;
    assert!(refused.unwrap_err().to_string().contains("no owner"));
    let mut resumed = running.clone();
    resumed.revision = 2;
    assert!(db.save_agent_profile(&admin, &w, &resumed).await.is_err());
    // Only an admin gives it an owner.
    let to = |revision| TransferAgent {
        owner: next.id.clone(),
        revision,
    };
    assert!(
        db.agent_transfer(&next, &w, &running.id, &to(2))
            .await
            .is_err()
    );

    // With an owner again, the agent that was running resumes; the one its
    // owner had paused, and the only-me agent, wait for their new owner.
    for a in [&mine, &running, &resting] {
        db.agent_transfer(&admin, &w, &a.id, &to(2)).await.unwrap();
    }
    for (a, paused) in [(&mine, true), (&running, false), (&resting, true)] {
        let owned = db.managed_agent(&next, &w, &a.id, true).await.unwrap();
        assert_eq!(owned["owner_id"], next.id.as_str());
        assert_eq!(owned["settings"]["paused"], paused, "{}", a.audience);
    }
    assert!(
        db.chat_create(&admin, &w, &direct(&running.id))
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_admin_may_name_the_new_owner_on_removal_but_a_person_leaving_may_not() {
    let db = fixture().await;
    let admin = user(&db, "name-admin").await;
    let removed = user(&db, "name-removed").await;
    let leaver = user(&db, "name-leaver").await;
    let next = user(&db, "name-next").await;
    let w = workspace(&db, &admin).await;
    for person in [&removed, &leaver, &next] {
        join(&db, &admin, &w, person, "Member").await;
    }
    let shared = agent("everyone");
    db.save_agent_profile(&removed, &w, &shared).await.unwrap();
    let private = agent("only_me");
    db.save_agent_profile(&removed, &w, &private).await.unwrap();
    let theirs = agent("everyone");
    db.save_agent_profile(&leaver, &w, &theirs).await.unwrap();

    // The admin names who takes over: the agents keep working under them, and
    // an only-me agent pauses for its new owner to look at.
    db.change_member(&admin, &w, &removed.id, None, None, Some(&next.id))
        .await
        .unwrap();
    assert_eq!(
        last_summary(&db, &w).await,
        "name-admin@example.test removed name-removed@example.test; 2 agents moved to name-next@example.test; 1 only-me agent paused for review"
    );
    let moved = db.managed_agent(&next, &w, &shared.id, true).await.unwrap();
    assert_eq!(moved["owner_id"], next.id.as_str());
    assert_eq!(moved["settings"]["paused"], false);
    let moved = db
        .managed_agent(&next, &w, &private.id, true)
        .await
        .unwrap();
    assert_eq!(moved["settings"]["paused"], true);

    // A person leaving names who takes their Library items, not their agents.
    let named = db
        .change_member(
            &leaver,
            &w,
            &leaver.id,
            None,
            Some(&next.id),
            Some(&next.id),
        )
        .await;
    assert!(named.unwrap_err().to_string().contains("Only an admin"));
    db.change(&leaver, &w, &leaver.id, None, Some(&next.id))
        .await
        .unwrap();
    assert_eq!(
        last_summary(&db, &w).await,
        "name-leaver@example.test left the workspace; 1 agent paused until an admin gives them an owner"
    );
    let left = db
        .managed_agent(&admin, &w, &theirs.id, true)
        .await
        .unwrap();
    assert!(left["owner_id"].is_null());
    assert!(db.managed_agent(&next, &w, &theirs.id, true).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_45_keeps_who_could_talk_and_former_managers_stay_on_record() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_agent_audience_{}", uuid::Uuid::new_v4().simple());
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
    migration::Migrator::up(&db.db, Some(44)).await.unwrap();
    db.db
        .execute_unprepared(
            "INSERT INTO relay_users(id,provider,email) VALUES('lead','p_lead','lead@test'),('staff','p_staff','staff@test'),('guest','p_guest','guest@test');
             INSERT INTO relay_workspaces(id,name) VALUES('shop','Shop');
             INSERT INTO relay_members(workspace,user_id,role) VALUES('shop','lead','Admin'),('shop','staff','Member'),('shop','guest','Member');
             INSERT INTO relay_shared_agents(workspace,id,name,role,model,scope,owner_id,creator_id,revision)
               VALUES('shop','shared','Atlas','Orders','m','workspace','lead','lead',1),('shop','mine','Nova','Notes','m','private','staff','staff',1),
                ('shop','orphan','Lost','Nobody','m','workspace',NULL,'lead',1);
             INSERT INTO relay_chat_access(workspace,user_id,agent) VALUES('shop','staff','shared');
             INSERT INTO relay_agent_creators(workspace,user_id,enabled,revision) VALUES('shop','staff',true,1);",
        )
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    let audience = |id: &'static str| {
        let db = &db;
        async move {
            query(
                &db.db,
                "SELECT audience FROM relay_shared_agents WHERE id=$1",
                vec![id.into()],
            )
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "audience")
            .unwrap()
        }
    };
    assert_eq!(audience("shared").await, "selected");
    assert_eq!(audience("mine").await, "only_me");
    // An agent that already had no owner is paused by schema 122, to resume
    // once an admin gives it one.
    let orphan = query(
        &db.db,
        "SELECT settings->>'paused' AS paused,resume_with_owner FROM relay_shared_agents WHERE id='orphan'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(orphan.try_get::<String>("", "paused").unwrap(), "true");
    assert!(orphan.try_get::<bool>("", "resume_with_owner").unwrap());
    // Schema 45 made owners managers; schema 122 keeps that list as a record.
    assert_eq!(former_managers(&db, "shop", "shared").await, ["lead"]);
    assert_eq!(former_managers(&db, "shop", "mine").await, ["staff"]);
    for (person, agent, allowed) in [
        ("staff", "shared", true),
        ("lead", "shared", true),
        ("guest", "shared", false),
        ("staff", "mine", true),
        ("lead", "mine", false),
    ] {
        assert_eq!(
            db.agent_access(person, "shop", agent).await.unwrap(),
            allowed,
            "{person} {agent}"
        );
    }
    assert!(
        db.db
            .execute_unprepared("SELECT 1 FROM relay_agent_creators")
            .await
            .is_err()
    );
}
