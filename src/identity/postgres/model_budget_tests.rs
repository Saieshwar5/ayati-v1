//! MG3: monthly model budgets warn, then refuse new work, for the workspace or one agent.
use super::executor_slot_tests::agent;
use super::model_budgets::BudgetInput;
use super::tests::*;
use super::{ModelCall, query};

async fn spend(db: &super::PgStore, w: &str, agent: &str, micro: i64) {
    db.record_model_usage(&ModelCall {
        workspace: w,
        assignment: "fixture-assignment",
        agent,
        job: None,
        provider: "fireworks",
        model: "llama-70b",
        provider_model: "llama",
        attempts: 1,
        input_tokens: 1,
        output_tokens: 1,
        person: None,
        charge_micro: micro,
        cost_micro: 0,
    })
    .await
    .unwrap();
}
fn budget(agent: &str, usd: Option<f64>) -> BudgetInput {
    BudgetInput {
        agent: agent.into(),
        monthly_usd: usd,
        warn_percent: 80,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn budgets_warn_then_refuse_new_work_and_only_admins_change_them() {
    let db = fixture().await;
    let admin = user(&db, "budget-admin").await;
    let member = user(&db, "budget-member").await;
    let w = workspace(&db, &admin).await;
    join(&db, &admin, &w, &member, "Member").await;
    let (chat_a, a) = agent(&db, &admin, &w).await;
    let (chat_b, b) = agent(&db, &admin, &w).await;

    assert!(
        db.save_model_budget(&member, &w, &budget("", Some(5.0)))
            .await
            .is_err(),
        "members can't set budgets"
    );
    for bad in [Some(0.0), Some(-1.0), Some(f64::NAN), Some(2_000_000.0)] {
        assert!(
            db.save_model_budget(&admin, &w, &budget("", bad))
                .await
                .is_err()
        );
    }
    // Agent A may spend $1 a month; it has spent 85 cents, so it is warned.
    db.save_model_budget(&admin, &w, &budget(&a, Some(1.0)))
        .await
        .unwrap();
    spend(&db, &w, &a, 850_000).await;
    let seen = db.model_budgets(&member, &w).await.unwrap();
    assert_eq!(seen["can_change"], false, "members read budgets only");
    assert_eq!(seen["budgets"][0]["state"], "warning");
    assert_eq!(seen["budgets"][0]["percent"], 85);
    assert_eq!(seen["budgets"][0]["name"], "Pool agent");
    db.chat_send(&admin, &w, &chat_a, &message("Still fine", Some(&a)))
        .await
        .unwrap();

    // At the limit, A is refused with the reason; B, without a budget, keeps working.
    spend(&db, &w, &a, 150_000).await;
    let refused = db
        .chat_send(&admin, &w, &chat_a, &message("Refused", Some(&a)))
        .await
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("This agent has used its monthly model budget"),
        "{refused}"
    );
    assert!(db.model_budget_reached(&w, &a).await.unwrap().is_some());
    db.chat_send(&admin, &w, &chat_b, &message("B works", Some(&b)))
        .await
        .unwrap();

    // A workspace budget covers every agent.
    db.save_model_budget(&admin, &w, &budget("", Some(1.0)))
        .await
        .unwrap();
    let reason = db.model_budget_reached(&w, &b).await.unwrap().unwrap();
    assert!(reason.contains("This workspace has used its monthly model budget"));
    let all = db.model_budgets(&admin, &w).await.unwrap();
    assert_eq!(
        all["budgets"][0]["name"], "Workspace",
        "the workspace budget comes first"
    );
    assert_eq!(all["budgets"][0]["state"], "reached");

    // Raising or removing budgets lets work continue; each change is audited.
    db.save_model_budget(&admin, &w, &budget("", None))
        .await
        .unwrap();
    db.save_model_budget(&admin, &w, &budget(&a, Some(5.0)))
        .await
        .unwrap();
    db.chat_send(&admin, &w, &chat_a, &message("Raised", Some(&a)))
        .await
        .unwrap();
    let audit: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_audit WHERE workspace=$1 AND action='model_budget_saved'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(audit, 4);
}
