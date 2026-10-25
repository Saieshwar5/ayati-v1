use super::{tests::*, *};
use crate::identity::control::{
    connections_model::{Creator, Delegation, SystemApproval},
    model::{ActionRequest, Rule},
    setup_model::{Activate, Choice, Draft, Edit, Inventory, Preview, Start},
};
use serde_json::json;

async fn setup() -> (PgStore, User, String, String, String, Draft) {
    let db = fixture().await;
    let owner = user(&db, "setup-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.control_system(
        &owner,
        &w,
        &SystemApproval {
            server: "orders".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"orders","name":"Orders","operations":[{"id":"read","title":"Read order","effect":"read","tool":"read_order","schema":{"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false}},{"id":"update","title":"Update order","effect":"write","tool":"update_order","schema":{"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false}}]});
    db.control_connect(&owner, &w, &agent, &id, definition, "reviewed")
        .await
        .unwrap();
    let request = Start {
        id: uuid::Uuid::new_v4().to_string(),
        purpose: "Look up orders and ask before changing them".into(),
        chat: chat.clone(),
        connections: vec![id.clone()],
        assistant: false,
    };
    let context = db
        .setup_selected_context(&owner, &w, &agent, &request)
        .await
        .unwrap();
    let choices = vec![
        Choice {
            approver: None,
            approver_revision: None,
            approval_chat: None,
            connection: id.clone(),
            operation: "read".into(),
            mode: "allow".into(),
        },
        Choice {
            approver: None,
            approver_revision: None,
            approval_chat: None,
            connection: id.clone(),
            operation: "update".into(),
            mode: "ask".into(),
        },
    ];
    let draft = Draft {
        request,
        context,
        choices,
        inventories: vec![Inventory {
            connection: id,
            tools: vec![
                "read_order".into(),
                "update_order".into(),
                "unreviewed_tool".into(),
            ],
            unsupported: vec!["unreviewed_tool".into()],
        }],
        notes: vec![],
    };
    (db, owner, w, agent, chat, draft)
}
fn activate(revision: i64) -> Activate {
    Activate {
        revision,
        reviewed: true,
    }
}
fn preview(revision: i64) -> Preview {
    Preview {
        revision,
        example: None,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn draft_preview_has_no_grants_or_actions_and_activation_is_atomic_idempotent() {
    let (db, owner, w, agent, chat, draft) = setup().await;
    let id = &draft.request.id;
    db.setup_create(&owner, &w, &agent, &draft).await.unwrap();
    assert!(!db.control_enabled(&w, &agent, &chat).await.unwrap());
    assert!(
        db.setup_activate(&owner, &w, &agent, id, &activate(1))
            .await
            .is_err()
    );
    let result = db
        .setup_preview(&owner, &w, &agent, id, &preview(1))
        .await
        .unwrap();
    assert_eq!(result["results"][0]["decision"], "allow");
    assert_eq!(result["results"][1]["decision"], "ask");
    assert_eq!(result["business_operations_executed"], 0);
    assert!(
        db.control_snapshot(&owner, &w, &agent).await.unwrap()["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!db.control_enabled(&w, &agent, &chat).await.unwrap());
    let input = activate(1);
    let (a, b) = tokio::join!(
        db.setup_activate(&owner, &w, &agent, id, &input),
        db.setup_activate(&owner, &w, &agent, id, &input)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    let rules = db.control_snapshot(&owner, &w, &agent).await.unwrap()["rules"].clone();
    assert_eq!(rules.as_array().unwrap().len(), 2);
    assert!(rules.as_array().unwrap().iter().all(|r| r["revision"] == 1));
    assert!(db.control_enabled(&w, &agent, &chat).await.unwrap());
    db.control_rule(
        &owner,
        &w,
        &agent,
        &Rule {
            approver: None,
            approver_revision: None,
            approval_chat: None,
            connection: draft.request.connections[0].clone(),
            operation: "read".into(),
            mode: "block".into(),
            revision: 1,
            chat: chat.clone(),
            delegation_revision: None,
        },
    )
    .await
    .unwrap();
    db.setup_activate(&owner, &w, &agent, id, &activate(1))
        .await
        .unwrap();
    assert_eq!(
        db.control_snapshot(&owner, &w, &agent).await.unwrap()["rules"][0]["mode"],
        "block"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn draft_edits_are_revisioned_and_invalidate_previous_preview() {
    let (db, owner, w, agent, _, draft) = setup().await;
    let id = &draft.request.id;
    db.setup_create(&owner, &w, &agent, &draft).await.unwrap();
    db.setup_preview(&owner, &w, &agent, id, &preview(1))
        .await
        .unwrap();
    let mut choices = draft.choices.clone();
    choices[0].mode = "block".into();
    let edit = Edit {
        revision: 1,
        choices,
    };
    let result = db.setup_edit(&owner, &w, &agent, id, &edit).await.unwrap();
    assert_eq!(result["revision"], 2);
    assert_eq!(result["previewed"], false);
    assert_eq!(
        db.setup_edit(&owner, &w, &agent, id, &edit).await.unwrap()["revision"],
        2
    );
    assert!(
        db.setup_activate(&owner, &w, &agent, id, &activate(2))
            .await
            .is_err()
    );
    assert!(
        db.setup_edit(
            &owner,
            &w,
            &agent,
            id,
            &Edit {
                revision: 1,
                choices: draft.choices.clone()
            }
        )
        .await
        .is_err()
    );
    db.setup_preview(&owner, &w, &agent, id, &preview(2))
        .await
        .unwrap();
    db.setup_activate(&owner, &w, &agent, id, &activate(2))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn omitted_unknown_duplicate_choices_and_cross_identity_drafts_are_rejected() {
    let (db, owner, w, agent, _, mut draft) = setup().await;
    let original = draft.choices.clone();
    draft.choices.pop();
    assert!(db.setup_create(&owner, &w, &agent, &draft).await.is_err());
    draft.choices = original.clone();
    draft.choices[1].operation = "unknown".into();
    assert!(db.setup_create(&owner, &w, &agent, &draft).await.is_err());
    draft.choices = vec![original[0].clone(), original[0].clone()];
    assert!(db.setup_create(&owner, &w, &agent, &draft).await.is_err());
    draft.choices = original;
    db.setup_create(&owner, &w, &agent, &draft).await.unwrap();
    let other = user(&db, "other-owner").await;
    join(&db, &owner, &w, &other, "User").await;
    db.change(&owner, &w, &other.id, Some("Owner"))
        .await
        .unwrap();
    assert!(
        db.setup_draft(&other, &w, &agent, &draft.request.id)
            .await
            .is_err()
    );
    let other_workspace = workspace(&db, &owner).await;
    assert!(
        db.setup_draft(&owner, &other_workspace, &agent, &draft.request.id)
            .await
            .is_err()
    );
    assert!(
        db.setup_last_draft(&other, &w, &agent)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn concurrent_manual_rule_change_rejects_whole_activation_without_partial_grants() {
    let (db, owner, w, agent, chat, draft) = setup().await;
    db.setup_create(&owner, &w, &agent, &draft).await.unwrap();
    db.setup_preview(&owner, &w, &agent, &draft.request.id, &preview(1))
        .await
        .unwrap();
    db.control_rule(
        &owner,
        &w,
        &agent,
        &Rule {
            approver: None,
            approver_revision: None,
            approval_chat: None,
            connection: draft.request.connections[0].clone(),
            operation: "update".into(),
            mode: "block".into(),
            revision: 0,
            chat,
            delegation_revision: None,
        },
    )
    .await
    .unwrap();
    assert!(
        db.setup_activate(&owner, &w, &agent, &draft.request.id, &activate(1))
            .await
            .is_err()
    );
    let rules = db.control_snapshot(&owner, &w, &agent).await.unwrap()["rules"].clone();
    assert_eq!(rules.as_array().unwrap().len(), 1);
    assert_eq!(rules[0]["mode"], "block");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn example_uses_real_schema_checks_and_unreviewed_tools_stay_blocked() {
    let (db, owner, w, agent, _, draft) = setup().await;
    db.setup_create(&owner, &w, &agent, &draft).await.unwrap();
    for (operation, arguments, decision) in [
        ("read", json!({"order":"one"}), "allow"),
        ("read", json!({"order":9}), "block"),
        ("unreviewed_tool", json!({}), "block"),
    ] {
        let example = ActionRequest {
            connection: draft.request.connections[0].clone(),
            operation: operation.into(),
            arguments,
        };
        let result = db
            .setup_preview(
                &owner,
                &w,
                &agent,
                &draft.request.id,
                &Preview {
                    revision: 1,
                    example: Some(example),
                },
            )
            .await
            .unwrap();
        assert_eq!(result["example"]["decision"], decision);
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn delegated_limits_and_revocation_fence_a_saved_review() {
    let (db, owner, w, _, _, original) = setup().await;
    let member = user(&db, "setup-member").await;
    join(&db, &owner, &w, &member, "User").await;
    db.control_creator(
        &owner,
        &w,
        &Creator {
            user: member.id.clone(),
            enabled: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    let (private_chat, agent) = super::executor_slot_tests::agent(&db, &member, &w).await;
    let chat = chat(
        &db,
        &member,
        &w,
        vec![owner.id.clone()],
        vec![agent.clone()],
    )
    .await;
    // Public work remains visible after rejoining, isolating the preview-authority check.
    execute(
        &db.db,
        "UPDATE relay_chats SET visibility='public' WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), chat.clone().into()],
    )
    .await
    .unwrap();
    let connection = &original.request.connections[0];
    let grant = Delegation {
        user: member.id.clone(),
        connection: connection.clone(),
        operation: "read".into(),
        ceiling: "ask".into(),
        revision: 0,
        connection_revision: 1,
    };
    db.control_delegate(&owner, &w, &grant).await.unwrap();
    let mut draft = original.clone();
    draft.request.id = uuid::Uuid::new_v4().to_string();
    draft.request.chat = chat.clone();
    draft.context = db
        .setup_selected_context(&member, &w, &agent, &draft.request)
        .await
        .unwrap();
    draft.choices[0].mode = "allow".into();
    draft.choices[1].mode = "block".into();
    assert!(db.setup_create(&member, &w, &agent, &draft).await.is_err());
    draft.choices[0].mode = "ask".into();
    draft.request.chat = private_chat;
    assert!(db.setup_create(&member, &w, &agent, &draft).await.is_err());
    draft.request.chat = chat;
    db.setup_create(&member, &w, &agent, &draft).await.unwrap();
    db.setup_preview(&member, &w, &agent, &draft.request.id, &preview(1))
        .await
        .unwrap();
    db.control_delegate(
        &owner,
        &w,
        &Delegation {
            ceiling: "block".into(),
            revision: 1,
            ..grant
        },
    )
    .await
    .unwrap();
    assert!(
        db.setup_activate(&member, &w, &agent, &draft.request.id, &activate(1))
            .await
            .is_err()
    );
    assert!(
        db.control_snapshot(&owner, &w, &agent).await.unwrap()["rules"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.change(&owner, &w, &member.id, None).await.unwrap();
    join(&db, &owner, &w, &member, "User").await;
    db.control_delegate(
        &owner,
        &w,
        &Delegation {
            user: member.id.clone(),
            connection: connection.clone(),
            operation: "read".into(),
            ceiling: "ask".into(),
            revision: 0,
            connection_revision: 1,
        },
    )
    .await
    .unwrap();
    let mut current = db
        .setup_selected_context(&member, &w, &agent, &draft.request)
        .await
        .unwrap();
    current.approval_channels = draft.context.approval_channels.clone();
    assert_eq!(
        serde_json::to_value(current).unwrap(),
        serde_json::to_value(&draft.context).unwrap()
    );
    assert_eq!(
        db.setup_draft(&member, &w, &agent, &draft.request.id)
            .await
            .unwrap()["previewed"],
        false
    );
    assert!(
        db.setup_activate(&member, &w, &agent, &draft.request.id, &activate(1))
            .await
            .is_err()
    );
    db.setup_preview(&member, &w, &agent, &draft.request.id, &preview(1))
        .await
        .unwrap();
    db.setup_activate(&member, &w, &agent, &draft.request.id, &activate(1))
        .await
        .unwrap();
}
