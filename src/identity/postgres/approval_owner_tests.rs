//! Every owner of an agent decides its "Ask first" actions (schema 122):
//! owners are equal, nobody asks another, and the first decision wins.
use super::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn every_owner_may_decide_without_asking_the_others_and_the_first_decision_wins() {
    let s = setup().await;
    s.configure().await;
    // `owner` (an admin) is made a second, equal owner by the first owner.
    let revision =
        s.db.managed_agent(&s.approver, &s.w, &s.agent, true)
            .await
            .unwrap()["revision"]
            .as_i64()
            .unwrap();
    s.db.change_agent_owners(
        &s.approver,
        &s.w,
        &s.agent,
        &super::super::agent_owners::OwnersChange {
            add: Some(s.owner.id.clone()),
            remove: None,
            revision,
        },
    )
    .await
    .unwrap();
    // The request comes from `other`, so either owner may decide it.
    execute(
        &s.db.db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![
            s.w.clone().into(),
            s.source.clone().into(),
            s.other.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    s.db.chat_send(
        &s.other,
        &s.w,
        &s.source,
        &message("Change the address", Some(&s.agent)),
    )
    .await
    .unwrap();
    let job = s.db.claim_job().await.unwrap().unwrap();
    let a =
        s.db.control_propose(&job, &s.request())
            .await
            .unwrap()
            .action;
    let id = a["id"].as_str().unwrap();
    let details = s.db.approval_details(&s.other, &s.w, id).await.unwrap();
    let mut names: Vec<String> = details["deciders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    let mut both = vec![s.approver.id.clone(), s.owner.id.clone()];
    both.sort();
    assert_eq!(names, both, "every owner who can see it may decide");
    assert_eq!(details["can_decide"], false, "the requester doesn't own it");
    // The second owner decides without the first; the first can't reverse it.
    let approved =
        s.db.control_decide(&s.owner, &s.w, &s.agent, id, &decision("approve"))
            .await
            .unwrap();
    assert!(approved.dispatch.is_some());
    assert!(
        s.db.control_decide(&s.approver, &s.w, &s.agent, id, &decision("reject"))
            .await
            .is_err()
    );
    let seen = s.db.approval_details(&s.approver, &s.w, id).await.unwrap();
    assert_eq!(seen["decided_by_name"], "approval-owner@example.test");
}
