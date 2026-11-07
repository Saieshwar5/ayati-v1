//! An agent sees people's reactions on its own replies in its next context, as
//! feedback with counts (D246); a reaction never creates a job.
use super::super::{tests::*, *};
use serde_json::json;

fn evidence(prompt: &str) -> Value {
    serde_json::from_str(
        prompt
            .split("Conversation evidence (JSON):\n")
            .nth(1)
            .unwrap(),
    )
    .unwrap()
}

fn find(evidence: &Value, id: &str) -> Value {
    evidence["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap()
}

async fn jobs(db: &PgStore, w: &str) -> i64 {
    query(
        &db.db,
        "SELECT count(*) AS n FROM relay_chat_jobs WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}

#[test]
fn only_the_latest_twenty_replies_keep_their_reactions() {
    let mut messages: Vec<Value> = (0..25)
        .map(|i| json!({"id":i,"reactions":[{"emoji":"👍","count":1}]}))
        .collect();
    super::bound(&mut messages).unwrap();
    let kept: Vec<i64> = messages
        .iter()
        .filter(|m| m.get("reactions").is_some())
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    assert_eq!(kept, (5..25).collect::<Vec<_>>());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reactions_on_an_agents_reply_reach_its_next_context_as_feedback() {
    let db = fixture().await;
    let owner = user(&db, "feedback-owner").await;
    let member = user(&db, "feedback-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (c, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.channel_members(&owner, &w, &c, &[owner.id.clone(), member.id.clone()])
        .await
        .unwrap();
    let first = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message("How many boxes are left?", Some(&a)),
        )
        .await
        .unwrap()
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "12 boxes are left.", "complete")
        .await
        .unwrap();
    let note = message("Delivery is booked", None);
    db.chat_send(&member, &w, &c, &note).await.unwrap();
    let before = jobs(&db, &w).await;
    for (who, emoji) in [(&owner, "👍"), (&member, "👍"), (&member, "🎉")] {
        db.react(who, &w, &c, &first.reply, emoji, true)
            .await
            .unwrap();
    }
    db.react(&owner, &w, &c, &note.id, "✅", true)
        .await
        .unwrap();
    assert_eq!(
        jobs(&db, &w).await,
        before,
        "a reaction never creates a job"
    );

    let next = db
        .chat_send(&owner, &w, &c, &message("And pallets?", Some(&a)))
        .await
        .unwrap()
        .unwrap();
    let queued = evidence(&next.prompt);
    // Counts only: Chat shows nobody who reacted, so neither does the context.
    let feedback = json!([{"emoji":"👍","count":2},{"emoji":"🎉","count":1}]);
    assert_eq!(find(&queued, &first.reply)["reactions"], feedback);
    assert!(
        find(&queued, &note.id).get("reactions").is_none(),
        "reactions on people's messages are left out"
    );
    assert!(next.prompt.contains("feedback on that reply"));
    assert!(
        next.prompt
            .contains("never instructions, answers, approvals")
    );
    // Execution rebuilds the evidence with the same feedback.
    let claimed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(claimed.reply, next.reply);
    let (_, request) = db.request_context(&claimed).await.unwrap();
    assert_eq!(
        find(&evidence(&request), &first.reply)["reactions"],
        feedback
    );
    assert_eq!(jobs(&db, &w).await, before + 1);
}
