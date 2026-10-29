use super::{
    automation_tests::{schedule, setup, state},
    tests::*,
    *,
};
use crate::identity::{agents::config::SaveAgent, automations::model::Change};
use serde_json::json;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_source_revocation_blocks_future_runs_and_cancels_queued_work_without_regrant_revival()
 {
    let (db, owner, w, _, a) = setup().await;
    let other = user(&db, "schedule-other-owner").await;
    join(&db, &owner, &w, &other, "User").await;
    db.change(&owner, &w, &other.id, Some("Owner"))
        .await
        .unwrap();
    let c = chat(&db, &owner, &w, vec![other.id.clone()], vec![a.clone()]).await;
    let hidden = schedule(&c, Some(10));
    db.save_schedule(&owner, &w, &a, &hidden).await.unwrap();
    db.dispatch_schedules_at(hidden.first_ms).await.unwrap();
    db.channel_members(&other, &w, &c, std::slice::from_ref(&other.id))
        .await
        .unwrap();
    assert!(db.schedule_runs(&owner, &w, &a, &hidden.id).await.is_err());
    assert_eq!(
        db.schedules(&owner, &w, &a).await.unwrap()["schedules"],
        json!([])
    );
    assert_eq!(state(&db, &w, &hidden.id).await["state"], "blocked");
    assert_eq!(
        db.schedule_runs(&other, &w, &a, &hidden.id).await.unwrap()["runs"][0]["state"],
        "cancelled"
    );
    db.channel_members(&other, &w, &c, &[other.id.clone(), owner.id.clone()])
        .await
        .unwrap();
    assert_eq!(
        db.dispatch_schedules_at(hidden.first_ms + 100000)
            .await
            .unwrap(),
        0
    );
    let revision = state(&db, &w, &hidden.id).await["revision"]
        .as_i64()
        .unwrap();
    assert!(
        db.change_schedule(
            &other,
            &w,
            &a,
            &hidden.id,
            &Change {
                request: uuid::Uuid::new_v4().to_string(),
                revision,
                enabled: true
            }
        )
        .await
        .is_err()
    );
    db.change_schedule(
        &owner,
        &w,
        &a,
        &hidden.id,
        &Change {
            request: uuid::Uuid::new_v4().to_string(),
            revision,
            enabled: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.dispatch_schedules_at(hidden.first_ms + 100000)
            .await
            .unwrap(),
        1
    );
    let foreign = workspace(&db, &other).await;
    assert!(
        db.schedule_runs(&owner, &foreign, &a, &hidden.id)
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_archive_preserves_accepted_work_but_restore_never_reenables_the_routine() {
    let (db, owner, w, c, a) = setup().await;
    let input = schedule(&c, Some(10));
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    db.dispatch_schedules_at(input.first_ms).await.unwrap();
    let archived = crate::identity::chat::organization_model::Lifecycle {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        archived: true,
    };
    db.channel_lifecycle(&owner, &w, &c, Some(&archived))
        .await
        .unwrap();
    assert_eq!(state(&db, &w, &input.id).await["state"], "blocked");
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"][0]["state"],
        "queued"
    );
    db.channel_lifecycle(
        &owner,
        &w,
        &c,
        Some(&crate::identity::chat::organization_model::Lifecycle {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            archived: false,
        }),
    )
    .await
    .unwrap();
    let reply = db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"][0]["reply"]
        .as_str()
        .unwrap()
        .to_owned();
    db.cancel_assignment(&owner, &w, &c, &reply).await.unwrap();
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 100000)
            .await
            .unwrap(),
        0
    );
    assert_eq!(state(&db, &w, &input.id).await["state"], "blocked");
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_membership_receipts_and_agent_pause_do_not_revive_on_restore() {
    let (db, owner, w, c, a) = setup().await;
    let other = user(&db, "schedule-second-owner").await;
    join(&db, &owner, &w, &other, "User").await;
    db.change(&owner, &w, &other.id, Some("Owner"))
        .await
        .unwrap();
    let input = schedule(&c, Some(10));
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    db.dispatch_schedules_at(input.first_ms).await.unwrap();
    db.change(&other, &w, &owner.id, None).await.unwrap();
    assert_eq!(state(&db, &w, &input.id).await["state"], "blocked");
    join(&db, &other, &w, &owner, "User").await;
    db.change(&other, &w, &owner.id, Some("Owner"))
        .await
        .unwrap();
    // Rejoining does not restore former private conversation membership.
    assert!(db.save_schedule(&owner, &w, &a, &input).await.is_err());
    execute(
        &db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![w.clone().into(), c.clone().into(), owner.id.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.save_schedule(&owner, &w, &a, &input).await.unwrap()["state"],
        "blocked"
    );
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"][0]["state"],
        "cancelled"
    );
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 100000)
            .await
            .unwrap(),
        0
    );
    let revision = state(&db, &w, &input.id).await["revision"]
        .as_i64()
        .unwrap();
    db.change_schedule(
        &owner,
        &w,
        &a,
        &input.id,
        &Change {
            request: uuid::Uuid::new_v4().to_string(),
            revision,
            enabled: true,
        },
    )
    .await
    .unwrap();
    let current = db.managed_agent(&owner, &w, &a, true).await.unwrap();
    let mut profile:SaveAgent=serde_json::from_value(json!({"id":a,"name":current["name"],"role":current["role"],"model":current["model"],"scope":current["scope"],"revision":current["revision"],"settings":current["settings"]})).unwrap();
    profile.settings.paused = true;
    db.save_agent_profile(&owner, &w, &profile).await.unwrap();
    assert_eq!(state(&db, &w, &input.id).await["state"], "blocked");
    profile.revision += 1;
    profile.settings.paused = false;
    db.save_agent_profile(&owner, &w, &profile).await.unwrap();
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 100000)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_requires_current_maintainer_request_and_destination_authority_without_granting_tools()
 {
    let (db, owner, w, c, a) = setup().await;
    let member = user(&db, "schedule-observer").await;
    join(&db, &owner, &w, &member, "User").await;
    db.chat_grant(&owner, &w, &member.id, &a, true)
        .await
        .unwrap();
    let input = schedule(&c, None);
    assert!(db.save_schedule(&member, &w, &a, &input).await.is_err());
    assert!(db.schedules(&member, &w, &a).await.is_err());
    let missing = chat(&db, &owner, &w, vec![], vec![]).await;
    assert!(
        db.save_schedule(&owner, &w, &a, &schedule(&missing, None))
            .await
            .is_err()
    );
    let mut invalid = schedule(&c, None);
    invalid.timezone = "Invented/Timezone".into();
    assert!(db.save_schedule(&owner, &w, &a, &invalid).await.is_err());
    invalid = schedule(&c, None);
    invalid.first_ms = 1;
    assert!(db.save_schedule(&owner, &w, &a, &invalid).await.is_err());
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 0);
    assert_eq!(
        db.schedules(&owner, &w, &a).await.unwrap()["schedules"],
        json!([])
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_invalid_task_references_block_without_partial_messages_or_starving_another_routine()
 {
    let (db, owner, w, c, a) = setup().await;
    let mut invalid = schedule(&c, None);
    invalid.content = "Ask [another agent](mention:agent:some-other-agent) to do this".into();
    let valid = schedule(&c, None);
    db.save_schedule(&owner, &w, &a, &invalid).await.unwrap();
    db.save_schedule(&owner, &w, &a, &valid).await.unwrap();
    assert_eq!(db.dispatch_schedules_at(valid.first_ms).await.unwrap(), 1);
    assert_eq!(state(&db, &w, &invalid.id).await["state"], "blocked");
    assert_eq!(
        state(&db, &w, &invalid.id).await["next_ms"],
        invalid.first_ms
    );
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 1);
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["messages"], 2);
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &invalid.id).await.unwrap()["runs"],
        json!([])
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_private_agent_routines_remain_private_from_workspace_owners() {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "private-schedule-owner").await;
    let member = user(&db, "private-schedule-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let profile = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Private helper".into(),
        role: "Prepare personal drafts".into(),
        model: "fixture".into(),
        scope: "private".into(),
        revision: 0,
        settings: Default::default(),
    };
    db.save_agent_profile(&member, &w, &profile).await.unwrap();
    let c = db
        .chat_create(
            &member,
            &w,
            &crate::identity::chat::model::NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Personal automation".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![profile.id.clone()],
            },
        )
        .await
        .unwrap();
    let input = schedule(&c, None);
    db.save_schedule(&member, &w, &profile.id, &input)
        .await
        .unwrap();
    assert!(db.schedules(&owner, &w, &profile.id).await.is_err());
    assert!(
        db.schedule_runs(&owner, &w, &profile.id, &input.id)
            .await
            .is_err()
    );
    assert!(
        db.change_schedule(
            &owner,
            &w,
            &profile.id,
            &input.id,
            &Change {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                enabled: false
            }
        )
        .await
        .is_err()
    );
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 1);
    assert_eq!(
        db.schedule_runs(&member, &w, &profile.id, &input.id)
            .await
            .unwrap()["runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_list_bounds_instruction_transfer_and_exact_details_recheck_source_access() {
    let (db, owner, w, c, a) = setup().await;
    let mut input = schedule(&c, None);
    input.content = "界".repeat(2000);
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    let list = db.schedules(&owner, &w, &a).await.unwrap();
    assert_eq!(
        list["schedules"][0]["content"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        240
    );
    assert_eq!(list["schedules"][0]["instructions_long"], true);
    assert_eq!(
        db.schedule_details(&owner, &w, &a, &input.id)
            .await
            .unwrap()["content"],
        input.content
    );
    let other = user(&db, "routine-details-owner").await;
    join(&db, &owner, &w, &other, "User").await;
    db.change(&owner, &w, &other.id, Some("Owner"))
        .await
        .unwrap();
    assert!(
        db.schedule_details(&other, &w, &a, &input.id)
            .await
            .is_err()
    );
}
