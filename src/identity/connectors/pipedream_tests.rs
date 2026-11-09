//! Pipedream-specific cases beyond the provider suite: sign-in for the
//! workspace's external user, account ownership, call outcomes, the access
//! token cache, event subscriptions, signed deliveries and settings.
use super::conformance_tests::operation;
use super::fake_mcp::WORKSPACE;
use super::fake_pipedream::{FakePipedream, SIGNING_KEY, TOKEN, VERSION};
use super::*;
use axum::http::HeaderMap;

const CALLBACK: &str = "http://127.0.0.1:1/api/identity/connectors/return/x";
const HOOK: &str = "http://127.0.0.1:1/hooks/pipedream/sub-1";

fn refused<T>(result: Result<T>) -> &'static str {
    match result {
        Ok(_) => "accepted",
        Err(error) => match error.downcast_ref::<Refusal>() {
            Some(Refusal::NeedsReconnect) => "needs_reconnect",
            Some(Refusal::Changed) => "changed",
            None => "error",
        },
    }
}

async fn signed_in(fake: &FakePipedream) -> (Arc<dyn ConnectorProvider>, Account) {
    let provider = pipedream_for_tests(fake);
    let Connect::SignIn { binding, .. } = provider
        .connect(WORKSPACE, "gmail", CALLBACK)
        .await
        .unwrap()
    else {
        panic!("Pipedream signs in through Connect Link");
    };
    fake.sign_in(&binding);
    let account = provider.finish(WORKSPACE, "gmail", &binding).await.unwrap();
    (provider, account)
}

fn saved(account: &Account) -> Saved<'_> {
    Saved {
        workspace: WORKSPACE,
        server: "gmail",
        binding: account.binding.as_ref(),
        fingerprint: &account.fingerprint,
    }
}

#[tokio::test]
async fn pipedream_signs_in_the_workspace_user_and_offers_only_reviewed_actions() {
    let fake = FakePipedream::start().await;
    let provider = pipedream_for_tests(&fake);
    let user = format!("cuckoo-{WORKSPACE}");
    let Connect::SignIn { binding, url, .. } = provider
        .connect(WORKSPACE, "gmail", CALLBACK)
        .await
        .unwrap()
    else {
        panic!("Pipedream signs in through Connect Link");
    };
    // Connect Link opens Gmail for the workspace's external user and returns to Cuckoo.
    assert!(url.contains("connectLink=true") && url.ends_with("&app=gmail"));
    assert_eq!(
        fake.state.lock().unwrap().connect_tokens[0],
        json!({"external_user_id":user,"app_id":"gmail","expires_in":900,
            "success_redirect_uri":CALLBACK,"error_redirect_uri":CALLBACK})
    );
    assert!(binding["started"].as_i64().unwrap() <= crate::identity::user::now());
    // Finishing before the person signed in saves nothing, and leaves nothing to remove.
    assert!(provider.finish(WORKSPACE, "gmail", &binding).await.is_err());
    provider
        .disconnect(WORKSPACE, "gmail", Some(&binding))
        .await
        .unwrap();
    fake.sign_in(&binding);
    let account = provider.finish(WORKSPACE, "gmail", &binding).await.unwrap();
    let offered: Vec<&str> = account.definition["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    // Pipedream replies by message, not by thread, so that action is not mapped.
    assert_eq!(
        offered,
        ["gmail.find_emails", "gmail.draft_email", "gmail.send_email"]
    );
    let send = operation(&account, "gmail.send_email");
    assert_eq!(send.version.as_deref(), Some(VERSION));
    assert_eq!(send.schema["required"], json!(["body", "subject", "to"]));
    let fields: Vec<&String> = send.schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(fields, ["body", "subject", "to"]);
    assert_eq!(account.identity.as_deref(), Some("owner@example.test"));
    assert_eq!(
        account.binding,
        Some(json!({"account":"apn_1","app":"gmail","user":user}))
    );
    let saved_text = format!("{}{:?}", account.definition, account.binding);
    assert!(!saved_text.contains(TOKEN) && !saved_text.contains("pd-fixture-access"));
    assert!(fake.state.lock().unwrap().removed.is_empty());
}

#[tokio::test]
async fn pipedream_refuses_accounts_of_another_workspace_or_external_user() {
    let fake = FakePipedream::start().await;
    let (provider, account) = signed_in(&fake).await;
    let send = operation(&account, "gmail.send_email");
    let other = Saved {
        workspace: "00000000-0000-4000-8000-000000000054",
        ..saved(&account)
    };
    assert_eq!(refused(provider.prepare(&other, &send).await), "changed");
    // An account Pipedream holds for another external user is refused even when
    // Pipedream lists it, and is never removed for this workspace.
    {
        let mut state = fake.state.lock().unwrap();
        state.accounts[0].user = "cuckoo-someone-else".into();
        state.ignore_user_filter = true;
    }
    assert_eq!(
        refused(provider.prepare(&saved(&account), &send).await),
        "needs_reconnect"
    );
    provider
        .disconnect(WORKSPACE, "gmail", account.binding.as_ref())
        .await
        .unwrap();
    assert!(fake.state.lock().unwrap().removed.is_empty());
    // Nor does a sign-in finish with another external user's new account.
    let Connect::SignIn { mut binding, .. } = provider
        .connect(WORKSPACE, "gmail", CALLBACK)
        .await
        .unwrap()
    else {
        panic!("Pipedream signs in through Connect Link");
    };
    let mine = binding.clone();
    binding["user"] = json!("cuckoo-someone-else");
    fake.sign_in(&binding);
    assert!(provider.finish(WORKSPACE, "gmail", &mine).await.is_err());
}

#[tokio::test]
async fn pipedream_calls_report_only_confirmed_results() {
    let fake = FakePipedream::start().await;
    let (provider, account) = signed_in(&fake).await;
    let send = operation(&account, "gmail.send_email");
    let args = json!({"to":"a@example.test","subject":"Hi","body":"Hello"});
    let thrown =
        json!({"ret":null,"os":[{"k":"error","ts":1,"err":{"name":"Error","message":"boom"}}]});
    for (status, reply) in [
        (500, json!({})),
        (202, json!({})),
        (429, json!({"error":"Throttled"})),
        (200, thrown),
    ] {
        fake.state.lock().unwrap().run_reply = Some((status, reply));
        let call = provider.prepare(&saved(&account), &send).await.unwrap();
        assert!(matches!(call.run(&args, "key").await, Outcome::Uncertain));
    }
    fake.state.lock().unwrap().run_reply = None;
    let call = provider.prepare(&saved(&account), &send).await.unwrap();
    let Outcome::Completed(result) = call.run(&args, "key").await else {
        panic!("a confirmed run completes");
    };
    assert_eq!(result["summary"], "Done");
    assert_eq!(result["data"]["echo"]["to"], json!(["a@example.test"]));
    // A component version Pipedream no longer serves is refused before sending.
    let mut retired = send.clone();
    retired.version = Some("0.3.3".into());
    let sent = fake.state.lock().unwrap().executed.len();
    assert_eq!(
        refused(provider.prepare(&saved(&account), &retired).await),
        "changed"
    );
    assert_eq!(fake.state.lock().unwrap().executed.len(), sent);
}

#[tokio::test]
async fn pipedream_reuses_its_access_token_until_the_credentials_change() {
    let fake = FakePipedream::start().await;
    let (provider, account) = signed_in(&fake).await;
    let read = operation(&account, "gmail.find_emails");
    for _ in 0..3 {
        provider.prepare(&saved(&account), &read).await.unwrap();
    }
    assert_eq!(fake.state.lock().unwrap().issued.len(), 1);
    // A rotated client secret takes effect on the next request.
    fake.rotate_secret();
    provider.prepare(&saved(&account), &read).await.unwrap();
    assert_eq!(fake.state.lock().unwrap().issued.len(), 2);
    // A token Pipedream stops accepting is replaced on the next request.
    fake.state
        .lock()
        .unwrap()
        .issued
        .push("pd-replaced-elsewhere".into());
    assert!(provider.prepare(&saved(&account), &read).await.is_err());
    provider.prepare(&saved(&account), &read).await.unwrap();
    assert_eq!(fake.state.lock().unwrap().issued.len(), 4);
}

#[tokio::test]
async fn pipedream_subscribes_events_with_a_signing_key_and_removes_them() {
    let fake = FakePipedream::start().await;
    let (provider, account) = signed_in(&fake).await;
    let saved = saved(&account);
    let trigger = "gmail-new-email-received";
    let config = json!({"withTextPayload":true,"labels":"INBOX"});
    let subscription = provider
        .subscribe(&saved, trigger, &config, HOOK)
        .await
        .unwrap();
    assert_eq!(
        subscription,
        Subscription {
            id: "dc_1".into(),
            signing_key: Some(SIGNING_KEY.into())
        }
    );
    assert_eq!(
        fake.state.lock().unwrap().deployed[0],
        json!({"id":trigger,"version":VERSION,"external_user_id":format!("cuckoo-{WORKSPACE}"),
            "configured_props":{"gmail":{"authProvisionId":"apn_1"},"withTextPayload":true,"labels":["INBOX"]},
            "webhook_url":HOOK,"emit_on_deploy":false})
    );
    // Settings the trigger doesn't take as plain values, and insecure addresses,
    // are refused before anything is deployed.
    for (config, webhook) in [
        (json!({"unknown":1}), HOOK),
        (json!({"withTextPayload":{"on":true}}), HOOK),
        (json!({}), "http://example.test/hooks/pipedream/sub-1"),
    ] {
        assert!(
            provider
                .subscribe(&saved, trigger, &config, webhook)
                .await
                .is_err()
        );
    }
    assert_eq!(fake.state.lock().unwrap().deployed.len(), 1);
    provider.unsubscribe(&saved, "dc_1").await.unwrap();
    assert!(fake.state.lock().unwrap().triggers.is_empty());
    // Removing one that is already gone succeeds.
    provider.unsubscribe(&saved, "dc_1").await.unwrap();
    // Without a signing key no delivery could be trusted, so the trigger is removed.
    fake.state.lock().unwrap().no_signing_key = true;
    assert!(
        provider
            .subscribe(&saved, trigger, &Value::Null, HOOK)
            .await
            .is_err()
    );
    assert!(fake.state.lock().unwrap().triggers.is_empty());
    // A broken account can't subscribe.
    fake.set_healthy(false);
    assert_eq!(
        refused(
            provider
                .subscribe(&saved, trigger, &Value::Null, HOOK)
                .await
        ),
        "needs_reconnect"
    );
}

fn signed(time: i64, body: &[u8], keys: &[&str]) -> HeaderMap {
    let mut message = format!("{time}.").into_bytes();
    message.extend_from_slice(body);
    let mut header = format!("t={time}");
    for key in keys {
        let tag = ring::hmac::sign(
            &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key.as_bytes()),
            &message,
        );
        let hex: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        header.push_str(&format!(",v1={hex}"));
    }
    let mut headers = HeaderMap::new();
    headers.insert("x-pd-signature", header.parse().unwrap());
    headers
}

#[tokio::test]
async fn pipedream_deliveries_need_a_fresh_signature_from_the_subscription_key() {
    let fake = FakePipedream::start().await;
    let provider = pipedream_for_tests(&fake);
    let parse = |headers: &HeaderMap, body: &[u8], now: i64| {
        provider.parse_delivery(headers, body, SIGNING_KEY, now)
    };
    let now = 1_800_000_000;
    let user = format!("cuckoo-{WORKSPACE}");
    let body = json!({"event":{"id":"18c1","subject":"Invoice"},
        "context":{"id":"evt_1","ts":"2027-01-15T08:00:00.000Z","emitter_id":"dc_1","external_user_id":user}})
    .to_string();
    let body = body.as_bytes();
    assert_eq!(
        parse(&signed(now, body, &[SIGNING_KEY]), body, now + 10).unwrap(),
        vec![Delivery {
            event_id: "evt_1".into(),
            user: Some(user.clone()),
            account: None,
            kind: DeliveryKind::Event {
                subscription: "dc_1".into(),
                trigger: String::new(),
                data: json!({"id":"18c1","subject":"Invoice"}),
            },
        }]
    );
    // Another key, a late or early signature, a changed body or none at all is refused.
    assert!(parse(&signed(now, body, &["another-key"]), body, now).is_err());
    assert!(parse(&signed(now - 301, body, &[SIGNING_KEY]), body, now).is_err());
    assert!(parse(&signed(now + 301, body, &[SIGNING_KEY]), body, now).is_err());
    let tampered = String::from_utf8(body.to_vec())
        .unwrap()
        .replace("Invoice", "Invoices");
    assert!(parse(&signed(now, body, &[SIGNING_KEY]), tampered.as_bytes(), now).is_err());
    assert!(parse(&HeaderMap::new(), body, now).is_err());
    // While a key rotates, one of several signatures must match.
    let rotating = signed(now, body, &["old-key", SIGNING_KEY]);
    assert_eq!(parse(&rotating, body, now).unwrap()[0].event_id, "evt_1");
    // An event without Pipedream's context is named by its bytes, so a retry is
    // the same event.
    let bare = br#"{"id":"18c2","subject":"Hi"}"#;
    let first = parse(&signed(now, bare, &[SIGNING_KEY]), bare, now).unwrap();
    let again = parse(&signed(now + 30, bare, &[SIGNING_KEY]), bare, now + 30).unwrap();
    assert_eq!(first[0].event_id, again[0].event_id);
    assert!(first[0].event_id.starts_with("sha256:"));
    let DeliveryKind::Event {
        subscription, data, ..
    } = &first[0].kind
    else {
        panic!("Pipedream deliveries are events");
    };
    assert!(subscription.is_empty() && data["id"] == "18c2");
}

#[tokio::test]
async fn pipedream_settings_follow_the_providers_file_entry() {
    let entry = json!({"provider":"pipedream","client_id_file":"/run/secrets/pd-id",
        "client_secret_file":"/run/secrets/pd-secret","project_id":"proj_abc123",
        "environment":"production","usd_per_1k_calls":0});
    let settings = pipedream::Settings::from_entry(&entry).unwrap();
    assert_eq!(settings.base, super::pipedream_api::OFFICIAL);
    assert_eq!(
        (settings.project_id.as_str(), settings.environment.as_str()),
        ("proj_abc123", "production")
    );
    for (key, value) in [
        ("provider", "composio"),
        ("environment", "staging"),
        ("project_id", "abc123"),
        ("client_id_file", "pd-id"),
        ("base", "http://127.0.0.1:1/v1/"),
    ] {
        let mut wrong = entry.clone();
        wrong[key] = json!(value);
        assert!(pipedream::Settings::from_entry(&wrong).is_err(), "{key}");
    }
    // Only the official API or a loopback fixture, with private client files.
    let fake = FakePipedream::start().await;
    let mut remote = fake.settings();
    remote.base = "http://example.test/v1/".into();
    assert!(pipedream::Pipedream::new(remote).is_err());
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        &fake.client_secret_file,
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(pipedream::Pipedream::new(fake.settings()).is_err());
}
