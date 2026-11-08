//! The writer avoids every name in the workspace but only offers skills the
//! person may add.
use super::skills::{NewSkill, files::SkillFile};
use super::tests::{fixture, join, user, workspace};

fn skill(name: &str, shared: bool) -> NewSkill {
    NewSkill {
        id: uuid::Uuid::new_v4().to_string(),
        shared,
        files: vec![SkillFile {
            path: "SKILL.md".into(),
            text: format!("---\nname: {name}\ndescription: Use when {name}\n---\n1. Do it.\n"),
        }],
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_writer_avoids_every_name_but_offers_only_usable_skills() {
    let db = fixture().await;
    let admin = user(&db, "writer-admin").await;
    let member = user(&db, "writer-member").await;
    let other = user(&db, "writer-other").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &member, "Member").await;
    join(&db, &admin, &w, &other, "Member").await;
    db.create_skill(&admin, &w, &skill("complaint-reply", true))
        .await
        .unwrap();
    db.create_skill(&member, &w, &skill("my-checklist", false))
        .await
        .unwrap();
    db.create_skill(&other, &w, &skill("private-gst-check", false))
        .await
        .unwrap();
    let context = db
        .skill_writer_context(&member, &w, None, &[])
        .await
        .unwrap();
    let mut names = context.names.clone();
    names.sort();
    assert_eq!(
        names,
        ["complaint-reply", "my-checklist", "private-gst-check"]
    );
    let mut offered: Vec<&str> = context.library.iter().map(|s| s.name.as_str()).collect();
    offered.sort();
    assert_eq!(
        offered,
        ["complaint-reply", "my-checklist"],
        "never another person's own skill"
    );
    assert!(context.apps.is_empty(), "no apps chosen");
}
