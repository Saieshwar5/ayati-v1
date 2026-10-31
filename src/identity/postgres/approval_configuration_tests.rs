use super::*;
use crate::identity::library::model::{AskItem, SavePage, ShareItem};
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
    super::super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent,
        &s.connection,
        "order.update",
        "ask",
    )
    .await;
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
