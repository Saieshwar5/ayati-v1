use super::super::tests::{fixture, join, message, user, workspace};
use super::files::SkillFile;
use super::runtime::LoadSkill;
use super::*;

fn files(name: &str, steps: &str) -> Vec<SkillFile> {
    vec![
        SkillFile {
            path: "SKILL.md".into(),
            text: format!(
                "---\nname: {name}\ndescription: Use when a customer asks for {name}\n---\n{steps}\n"
            ),
        },
        SkillFile {
            path: "references/prices.csv".into(),
            text: "item,price\nchair,4500\n".into(),
        },
    ]
}
fn new(shared: bool, name: &str) -> NewSkill {
    NewSkill {
        id: uuid::Uuid::new_v4().to_string(),
        shared,
        files: files(name, "1. Check the price list."),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn workspace_skills_belong_to_admins_and_every_change_is_a_new_version() {
    let db = fixture().await;
    let admin = user(&db, "skill-admin").await;
    let member = user(&db, "skill-member").await;
    let other = user(&db, "skill-other").await;
    let w = workspace(&db, &admin).await;
    for person in [&member, &other] {
        join(&db, &admin, &w, person, "Member").await;
    }
    assert!(
        db.create_skill(&member, &w, &new(true, "quote-builder"))
            .await
            .is_err(),
        "members can't add workspace skills"
    );
    let shared = db
        .create_skill(&admin, &w, &new(true, "quote-builder"))
        .await
        .unwrap();
    let id = shared["id"].as_str().unwrap().to_string();
    // A retried upload returns the same skill; the name stays unique.
    let retry = NewSkill {
        id: id.clone(),
        shared: true,
        files: files("quote-builder", "1. Check the price list."),
    };
    assert_eq!(
        db.create_skill(&admin, &w, &retry).await.unwrap()["version"],
        1
    );
    assert!(
        db.create_skill(&member, &w, &new(false, "quote-builder"))
            .await
            .is_err()
    );
    let change = NewVersion {
        version: 1,
        files: files(
            "quote-builder",
            "1. Check the price list.\n2. Add delivery.",
        ),
    };
    assert!(
        db.update_skill(&member, &w, &id, &change).await.is_err(),
        "members can't change workspace skills"
    );
    let v2 = db.update_skill(&admin, &w, &id, &change).await.unwrap();
    assert_eq!(v2["version"], 2);
    assert_eq!(
        db.update_skill(&admin, &w, &id, &change).await.unwrap()["version"],
        2,
        "an exact retry"
    );
    let stale = NewVersion {
        version: 1,
        files: files("quote-builder", "Different"),
    };
    assert!(db.update_skill(&admin, &w, &id, &stale).await.is_err());
    let renamed = NewVersion {
        version: 2,
        files: files("price-builder", "x"),
    };
    assert!(
        db.update_skill(&admin, &w, &id, &renamed).await.is_err(),
        "agents find skills by name"
    );
    // The first version is still there, byte for byte.
    let first = db.skill(&member, &w, &id, Some(1)).await.unwrap();
    assert_eq!(
        first["hash"],
        super::files::hash(&files("quote-builder", "1. Check the price list."))
    );
    assert_eq!(first["versions"].as_array().unwrap().len(), 2);

    // My skills: the owner and admins see them; others don't. Admins can share them.
    let mine = db
        .create_skill(&member, &w, &new(false, "delivery-update"))
        .await
        .unwrap();
    let mine_id = mine["id"].as_str().unwrap().to_string();
    assert!(db.skill(&other, &w, &mine_id, None).await.is_err());
    assert_eq!(
        db.skills(&admin, &w).await.unwrap()["others"][0]["name"],
        "delivery-update"
    );
    assert!(db.share_skill(&member, &w, &mine_id, true).await.is_err());
    db.share_skill(&admin, &w, &mine_id, true).await.unwrap();
    assert_eq!(
        db.skills(&other, &w).await.unwrap()["workspace"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        db.delete_skill(&member, &w, &mine_id).await.is_err(),
        "now a workspace skill"
    );
    db.delete_skill(&admin, &w, &mine_id).await.unwrap();
    let log = db
        .workspace_activity(&admin, &w, "all", None)
        .await
        .unwrap();
    assert!(
        log["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["sentence"] == "shared the skill delivery-update")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_use_shared_skills_or_their_owners_own_within_a_budget() {
    let db = fixture().await;
    let admin = user(&db, "assign-admin").await;
    let member = user(&db, "assign-member").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &member, "Member").await;
    let (_, admins_agent) = super::super::executor_slot_tests::agent(&db, &admin, &w).await;
    let personal = db
        .create_skill(&member, &w, &new(false, "my-steps"))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let shared = db
        .create_skill(&admin, &w, &new(true, "quote-builder"))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        db.assign_skill(&member, &w, &admins_agent, &shared, true)
            .await
            .is_err(),
        "only its owner or an admin changes an agent"
    );
    let refused = db
        .assign_skill(&admin, &w, &admins_agent, &personal, true)
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("ask an admin to share it"),
        "{refused}"
    );
    let assigned = db
        .assign_skill(&admin, &w, &admins_agent, &shared, true)
        .await
        .unwrap();
    assert_eq!(assigned["skills"][0]["name"], "quote-builder");
    assert_eq!(
        db.agent_skill_listing(&w, &admins_agent).await.unwrap()[0].0,
        "quote-builder"
    );
    // At most AGENT_SKILLS per agent.
    for n in 1..AGENT_SKILLS {
        let id = db
            .create_skill(&admin, &w, &new(true, &format!("skill-{n}")))
            .await
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        db.assign_skill(&admin, &w, &admins_agent, &id, true)
            .await
            .unwrap();
    }
    let extra = db
        .create_skill(&admin, &w, &new(true, "one-too-many"))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        db.assign_skill(&admin, &w, &admins_agent, &extra, true)
            .await
            .is_err()
    );
    db.assign_skill(&admin, &w, &admins_agent, &shared, false)
        .await
        .unwrap();
    assert!(
        db.agent_skill_listing(&w, &admins_agent)
            .await
            .unwrap()
            .iter()
            .all(|(name, _)| name != "quote-builder")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_load_only_assigned_skills_at_the_version_their_request_started_with() {
    let db = fixture().await;
    let owner = user(&db, "load-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let skill = db
        .create_skill(&owner, &w, &new(true, "quote-builder"))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    db.create_skill(&owner, &w, &new(true, "not-assigned"))
        .await
        .unwrap();
    db.assign_skill(&owner, &w, &agent, &skill, true)
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Quote for Sharma Traders", Some(&agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    // A change after the request started doesn't reach it.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    db.update_skill(
        &owner,
        &w,
        &skill,
        &NewVersion {
            version: 1,
            files: files("quote-builder", "Version two steps"),
        },
    )
    .await
    .unwrap();
    let loaded = db
        .load_skill(
            &job,
            &LoadSkill {
                name: "quote-builder".into(),
                file: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (loaded["state"].clone(), loaded["version"].clone()),
        ("available".into(), 1.into())
    );
    assert!(
        loaded["content"]
            .as_str()
            .unwrap()
            .contains("Check the price list")
    );
    let file = db
        .load_skill(
            &job,
            &LoadSkill {
                name: "quote-builder".into(),
                file: Some("references/prices.csv".into()),
            },
        )
        .await
        .unwrap();
    assert!(file["content"].as_str().unwrap().starts_with("item,price"));
    for (name, path) in [
        ("not-assigned", None),
        ("quote-builder", Some("../secrets")),
    ] {
        let missing = db
            .load_skill(
                &job,
                &LoadSkill {
                    name: name.into(),
                    file: path.map(str::to_owned),
                },
            )
            .await
            .unwrap();
        assert_eq!(missing["state"], "not_found");
    }
    // Each load shows on the request in Activity, and counts as use this week.
    let details = db.activity_request(&owner, &w, &job.reply).await.unwrap();
    let titles: Vec<&str> = details["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Used skill: quote-builder (version 1)",
            "Read references/prices.csv from quote-builder"
        ]
    );
    assert_eq!(
        db.agent_skills(&owner, &w, &agent).await.unwrap()["skills"][0]["used_this_week"],
        1
    );
}
