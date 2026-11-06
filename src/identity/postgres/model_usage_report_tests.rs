//! MG2: what each model call is charged, and who sees which use.
use super::executor_slot_tests::agent;
use super::model_usage_reports::UsageScope;
use super::tests::*;
use super::{ModelCall, model_usage_micro_dollars};
use crate::identity::models::catalog::Price;

fn call<'a>(w: &'a str, agent: &'a str, person: Option<&'a str>, charge: i64) -> ModelCall<'a> {
    ModelCall {
        workspace: w,
        assignment: "fixture-assignment",
        agent,
        job: None,
        provider: "fireworks",
        model: "llama-70b",
        provider_model: "accounts/fireworks/models/llama",
        attempts: 1,
        input_tokens: 1000,
        output_tokens: 500,
        person,
        charge_micro: charge,
        cost_micro: charge / 2,
    }
}

#[test]
fn a_call_is_charged_tokens_times_dollars_per_million() {
    let price = Price {
        input: 0.9,
        output: 2.0,
        cached_input: None,
    };
    // 1,000 × $0.90 + 500 × $2 per million tokens = $0.0019.
    assert_eq!(model_usage_micro_dollars(1000, 500, &price), 1900);
    assert_eq!(model_usage_micro_dollars(-5, 0, &price), 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn admins_see_the_workspace_owners_their_agent_and_members_their_own_use() {
    let db = fixture().await;
    let owner = user(&db, "usage-owner").await;
    let member = user(&db, "usage-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_chat, a) = agent(&db, &owner, &w).await;
    for (agent, person, charge) in [
        (a.as_str(), Some(owner.id.as_str()), 1_500_000),
        (a.as_str(), Some(member.id.as_str()), 500_000),
        ("", None, 0),
    ] {
        db.record_model_usage(&call(&w, agent, person, charge))
            .await
            .unwrap();
    }
    let scope = |agent| UsageScope { agent, days: 30 };
    let all = db
        .model_usage_report(&owner, &w, &scope(None))
        .await
        .unwrap();
    assert_eq!(all["scope"], "workspace");
    assert_eq!(all["totals"]["calls"], 3);
    assert_eq!(all["totals"]["charge_usd"], 2.0);
    assert_eq!(all["totals"]["input_tokens"], 3000);
    let agents: Vec<_> = all["by_agent"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(agents, ["Pool agent", "Library"], "most charged first");
    assert_eq!(all["by_person"].as_array().unwrap().len(), 3);
    assert_eq!(all["by_day"].as_array().unwrap().len(), 1);
    assert_eq!(all["by_model"][0]["key"], "llama-70b");
    assert!(
        !all.to_string().contains("cost"),
        "our costs never reach a report"
    );

    let mine = db
        .model_usage_report(&member, &w, &scope(None))
        .await
        .unwrap();
    assert_eq!(mine["scope"], "mine");
    assert_eq!(mine["totals"]["calls"], 1);
    assert_eq!(mine["totals"]["charge_usd"], 0.5);
    assert!(
        db.model_usage_report(&member, &w, &scope(Some(&a)))
            .await
            .is_err(),
        "a member doesn't see another person's agent"
    );
    let one = db
        .model_usage_report(&owner, &w, &scope(Some(&a)))
        .await
        .unwrap();
    assert_eq!(
        (one["scope"].as_str(), &one["totals"]["calls"]),
        (Some("agent"), &serde_json::json!(2))
    );
    assert!(
        db.model_usage_report(
            &owner,
            &w,
            &UsageScope {
                agent: None,
                days: 91
            }
        )
        .await
        .is_err()
    );
    let stranger = user(&db, "usage-stranger").await;
    assert!(
        db.model_usage_report(&stranger, &w, &scope(None))
            .await
            .is_err()
    );
}
