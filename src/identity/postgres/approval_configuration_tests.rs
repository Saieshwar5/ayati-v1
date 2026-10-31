use super::*;
use crate::identity::control::connections_model::Delegation;
use crate::identity::library::model::{AskItem, SavePage, ShareItem};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn delegated_creator_can_only_select_people_authorized_by_their_delegating_owner() {
    let s = setup().await;
    s.db.change(&s.owner, &s.w, &s.other.id, Some("Admin"), None)
        .await
        .unwrap();
    let creator = user(&s.db, "route-creator").await;
    join(&s.db, &s.owner, &s.w, &creator, "Member").await;
    let (_, agent) = super::super::executor_slot_tests::agent(&s.db, &creator, &s.w).await;
    s.db.chat_grant(&s.owner, &s.w, &s.approver.id, &agent, true)
        .await
        .unwrap();
    let work = chat(
        &s.db,
        &creator,
        &s.w,
        vec![s.owner.id.clone(), s.approver.id.clone()],
        vec![agent.clone()],
    )
    .await;
    s.db.control_delegate(
        &s.owner,
        &s.w,
        &Delegation {
            user: creator.id.clone(),
            connection: s.connection.clone(),
            operation: "order.update".into(),
            ceiling: "ask".into(),
            revision: 0,
            connection_revision: 1,
        },
    )
    .await
    .unwrap();
    s.db.control_approver_grant(&s.other, &s.w, &s.grant(true, 0))
        .await
        .unwrap();
    let mut rule = Rule {
        approver: Some(s.approver.id.clone()),
        approver_revision: Some(1),
        approval_chat: None,
        connection: s.connection.clone(),
        operation: "order.update".into(),
        mode: "ask".into(),
        revision: 0,
        chat: work.clone(),
        delegation_revision: Some(1),
    };
    assert!(
        s.db.control_rule(&creator, &s.w, &agent, &rule)
            .await
            .is_err()
    );
    let context = s.db.setup_context(&creator, &s.w, &agent).await.unwrap();
    assert!(
        !context.accounts[0].permissions[0]
            .approvers
            .iter()
            .any(|p| p["id"] == s.approver.id)
    );
    s.db.control_approver_grant(&s.owner, &s.w, &s.grant(true, 1))
        .await
        .unwrap();
    rule.approver_revision = Some(2);
    s.db.control_rule(&creator, &s.w, &agent, &rule)
        .await
        .unwrap();
    assert!(s.db.control_enabled(&s.w, &agent, &work).await.unwrap());
    let other_workspace = workspace(&s.db, &s.owner).await;
    let foreign = chat(&s.db, &s.owner, &other_workspace, vec![], vec![]).await;
    rule.approval_chat = Some(foreign);
    rule.revision = 1;
    assert!(
        s.db.control_rule(&creator, &s.w, &agent, &rule)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn library_source_details_never_publish_to_new_channel_members_even_with_agent_access() {
    let s = setup().await;
    s.db.chat_grant(&s.owner, &s.w, &s.other.id, &s.agent, true)
        .await
        .unwrap();
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Private policy".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"PRIVATE_LIBRARY_POLICY"}]}]}),
    };
    s.db.library_save(&s.owner, &s.w, &page).await.unwrap();
    s.db.library_share(
        &s.owner,
        &s.w,
        &page.id,
        &ShareItem {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            visibility: "private".into(),
            people: vec![],
            agents: vec![s.agent.clone()],
        },
    )
    .await
    .unwrap();
    let result =
        s.db.library_ask(
            &s.owner,
            &s.w,
            &page.id,
            &AskItem {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                agent: s.agent.clone(),
                question: "Use this policy to update the address".into(),
            },
        )
        .await
        .unwrap();
    let source = result["chat"].as_str().unwrap();
    let rule = Rule {
        approver: None,
        approver_revision: None,
        approval_chat: None,
        connection: s.connection.clone(),
        operation: "order.update".into(),
        mode: "ask".into(),
        revision: 0,
        chat: source.into(),
        delegation_revision: None,
    };
    s.db.control_rule(&s.owner, &s.w, &s.agent, &rule)
        .await
        .unwrap();
    let job = s.db.claim_job().await.unwrap().unwrap();
    let action =
        s.db.control_propose(&job, &s.request())
            .await
            .unwrap()
            .action;
    assert!(
        s.db.approval_details(&s.owner, &s.w, action["id"].as_str().unwrap())
            .await
            .is_ok()
    );
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![s.w.clone().into(), source.into(), s.other.id.clone().into()],
    )
    .await
    .unwrap();
    let history = s.db.chat_page(&s.other, &s.w, source, None).await.unwrap();
    assert!(!history.to_string().contains("PRIVATE_EXACT_ADDRESS"));
    assert!(!history.to_string().contains("PRIVATE_LIBRARY_POLICY"));
    let card = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["approval"].is_object())
        .unwrap();
    assert_eq!(card["approval"]["restricted"], true);
    assert!(
        s.db.approval_details(&s.other, &s.w, action["id"].as_str().unwrap())
            .await
            .is_err()
    );
    assert!(
        s.db.control_snapshot(&s.other, &s.w, &s.agent)
            .await
            .unwrap()["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        s.db.control_decide(
            &s.owner,
            &s.w,
            &s.agent,
            action["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
}
