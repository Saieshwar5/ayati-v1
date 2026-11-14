//! `POST /hooks/composio` intake (schema 109) against PostgreSQL: routing by
//! workspace and subscription, refusals, receipts and account expiry.
use super::super::*;
use super::subscription_tests::{Setup, setup};
use crate::identity::connectors::fake_composio::{self, WEBHOOK_SECRET};
use crate::identity::events::provider_intake;
use axum::http::StatusCode;
use serde_json::json;

/// Delivers a signed V3 envelope through the Composio intake.
async fn deliver(s: &Setup, body: &Value, secret: &str, at: i64) -> Result<Value, StatusCode> {
    deliver_signed(s, body, WEBHOOK_SECRET, secret, at).await
}

/// Signs with `sign`; the intake verifies with `secret`.
async fn deliver_signed(
    s: &Setup,
    body: &Value,
    sign: &str,
    secret: &str,
    at: i64,
) -> Result<Value, StatusCode> {
    let bytes = serde_json::to_vec(body).unwrap();
    let headers = fake_composio::signed(sign, body["id"].as_str().unwrap(), at, &bytes);
    provider_intake::composio(
        &s.db,
        s.provider.as_ref(),
        &headers,
        &bytes,
        secret,
        100_000,
    )
    .await
    .map_err(|e| e.1)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn composio_intake_routes_to_the_subscription_and_refuses_anything_else() {
    let s = setup().await;
    s.create(&s.input(json!({"subject":"Invoice"})).await)
        .await
        .unwrap();
    let sub = &s.subscriptions().await[0];
    let (ti, account) = (
        sub["provider_id"].as_str().unwrap().to_owned(),
        sub["binding"]["account"].as_str().unwrap().to_owned(),
    );
    let now = crate::identity::user::now();
    let data = json!({"sender":"sharma@example.test","subject":"Invoice","message_text":"₹1.48L due","payload":{"raw":"NEVER-COPY"}});
    let event = |id: &str, w: &str, account: &str, ti: &str, data: &Value| {
        fake_composio::message(id, w, account, ti, data.clone())
    };
    let good = event("msg_1", &s.w, &account, &ti, &data);
    let first = deliver(&s, &good, WEBHOOK_SECRET, now).await.unwrap();
    assert_eq!(
        (first["accepted"].as_bool(), first["duplicate"].as_bool()),
        (Some(true), Some(false))
    );
    // A retry is a duplicate; a changed retry is refused.
    assert_eq!(
        deliver(&s, &good, WEBHOOK_SECRET, now).await.unwrap()["duplicate"],
        true
    );
    let mut changed = data.clone();
    changed["subject"] = json!("Changed");
    assert_eq!(
        deliver(
            &s,
            &event("msg_1", &s.w, &account, &ti, &changed),
            WEBHOOK_SECRET,
            now
        )
        .await,
        Err(StatusCode::CONFLICT)
    );
    // An event the filter doesn't match is received but starts nothing.
    let mut other = data.clone();
    other["subject"] = json!("Hello");
    deliver(
        &s,
        &event("msg_2", &s.w, &account, &ti, &other),
        WEBHOOK_SECRET,
        now,
    )
    .await
    .unwrap();
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let job: crate::identity::chat::model::Job = serde_json::from_value(
        query(&s.db.db, "SELECT j.payload FROM relay_chat_jobs j JOIN relay_event_deliveries d ON d.workspace=j.workspace AND d.reply=j.reply LIMIT 1", vec![])
            .await.unwrap().unwrap().try_get("", "payload").unwrap(),
    )
    .unwrap();
    // The run happens in a private work room; the conversation only gets a summary.
    assert_eq!(job.requester.as_str(), s.owner.id.as_str());
    assert_ne!(job.chat, s.chat);
    let context = json!(s.db.event_job_context(&job).await.unwrap()).to_string();
    assert!(
        context.contains("sharma@example.test") && context.contains("Invoice"),
        "{context}"
    );
    assert!(!context.contains("NEVER-COPY"));
    // Another workspace's user, a user that isn't a workspace, an unknown
    // subscription and another account are all refused.
    let elsewhere = uuid::Uuid::new_v4().to_string();
    for (body, status) in [
        (
            event("msg_3", &elsewhere, &account, &ti, &data),
            StatusCode::UNAUTHORIZED,
        ),
        (
            event("msg_4", &s.w, &account, "ti_unknown", &data),
            StatusCode::NOT_FOUND,
        ),
        (
            event("msg_5", &s.w, "ca_other", &ti, &data),
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        assert_eq!(
            deliver(&s, &body, WEBHOOK_SECRET, now).await,
            Err(status),
            "{body}"
        );
    }
    let mut stranger = event("msg_6", &s.w, &account, &ti, &data);
    stranger["metadata"]["user_id"] = json!("someone-else");
    assert_eq!(
        deliver(&s, &stranger, WEBHOOK_SECRET, now).await,
        Err(StatusCode::UNAUTHORIZED)
    );
    // A wrong secret or a timestamp outside ±300 s is refused before reading.
    let fresh = event("msg_7", &s.w, &account, &ti, &data);
    assert_eq!(
        deliver(&s, &fresh, "whsec_not_the_project_secret", now).await,
        Err(StatusCode::UNAUTHORIZED)
    );
    assert_eq!(
        deliver(&s, &fresh, WEBHOOK_SECRET, now - 301).await,
        Err(StatusCode::UNAUTHORIZED)
    );
    // None of the refused events left a receipt.
    let receipts: i64 = query(
        &s.db.db,
        "SELECT count(*) AS n FROM relay_event_inbox WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(receipts, 2);
    // Other project events are acknowledged and ignored.
    let ignored = json!({"id":"evt_9","type":"composio.connected_account.created","data":{}});
    assert_eq!(
        deliver(&s, &ignored, WEBHOOK_SECRET, now).await.unwrap()["ignored"],
        true
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn composio_account_expiry_marks_the_account_for_reconnecting() {
    let s = setup().await;
    let binding: Value = query(
        &s.db.db,
        "SELECT binding FROM relay_control_connections WHERE workspace=$1 AND id=$2",
        vec![s.w.clone().into(), s.connection.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "binding")
    .unwrap();
    let expired = json!({"id":"evt_1","type":"composio.connected_account.expired",
        "metadata":{"project_id":"pr_1","org_id":"ok_1"},
        "data":{"id":binding["account"],"toolkit":{"slug":"gmail"},"status":"EXPIRED"},
        "timestamp":"2026-11-08T12:00:00Z"});
    let ready = s.input(json!({})).await;
    let now = crate::identity::user::now();
    assert_eq!(
        deliver(&s, &expired, WEBHOOK_SECRET, now).await.unwrap()["reconnect"],
        1
    );
    let marked = query(
        &s.db.db,
        "SELECT 1 AS one FROM relay_connector_reconnects WHERE workspace=$1 AND connection=$2",
        vec![s.w.clone().into(), s.connection.clone().into()],
    )
    .await
    .unwrap();
    assert!(marked.is_some());
    assert_eq!(s.choice(&s.owner).await["state"], "reconnect");
    let refused = s.create(&ready).await.unwrap_err();
    assert!(refused.to_string().contains("reconnecting"), "{refused}");
    assert_eq!(s.state().upserts, 0);
    // An account no connection uses marks nothing.
    let mut unknown = expired.clone();
    unknown["id"] = json!("evt_2");
    unknown["data"]["id"] = json!("ca_unknown");
    assert_eq!(
        deliver(&s, &unknown, WEBHOOK_SECRET, now).await.unwrap()["reconnect"],
        0
    );
}
