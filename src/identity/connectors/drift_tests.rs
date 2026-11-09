use super::super::fake_composio::{FakeComposio, VERSION};
use super::super::fake_mcp::{FakeMcp, WORKSPACE};
use super::super::{Account, Connect, Refusal, mcp_servers};
use super::*;

fn operation(account: &Account, id: &str) -> Operation {
    let found = account.definition["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap()
        .clone();
    serde_json::from_value(found).unwrap()
}

fn saved<'a>(account: &'a Account, server: &'a str) -> Saved<'a> {
    Saved {
        workspace: WORKSPACE,
        server,
        binding: account.binding.as_ref(),
        fingerprint: &account.fingerprint,
    }
}

async fn found(provider: &dyn ConnectorProvider, saved: &Saved<'_>, op: &Operation) -> Finding {
    compare(op, &provider.describe(saved, op).await.unwrap())
}

fn finding(status: &'static str, fields: &[&str], version: Option<&str>) -> Finding {
    Finding {
        status,
        fields: fields.iter().map(|f| f.to_string()).collect(),
        version: version.map(str::to_owned),
    }
}

#[test]
fn findings_name_the_cuckoo_fields_that_differ() {
    let reviewed = Operation {
        id: "gmail.send_email".into(),
        title: "Send an email".into(),
        tool: "GMAIL_SEND_EMAIL".into(),
        effect: "write".into(),
        schema: json!({"type":"object","properties":{"to":{"type":"string"},"subject":{"type":"string"}},
            "required":["to"],"additionalProperties":false}),
        version: Some("v1".into()),
    };
    let same = reviewed.schema.clone();
    let latest = |version: &str, schema: &Value| json!({"schema": same, "latest": {"version": version, "schema": schema}});
    assert_eq!(
        compare(&reviewed, &latest("v1", &same)),
        finding("ok", &[], None)
    );
    assert_eq!(
        compare(&reviewed, &json!({"schema": null})).status,
        "missing"
    );
    // A field changed type and another became required.
    let changed = json!({"type":"object","properties":{"to":{"type":"array"},"subject":{"type":"string"}},
        "required":["to","subject"],"additionalProperties":false});
    assert_eq!(
        compare(&reviewed, &json!({ "schema": changed })),
        finding("changed", &["subject", "to"], None)
    );
    // A newer version: the same reviewed fields, then one that drops a field.
    assert_eq!(
        compare(&reviewed, &latest("v2", &same)),
        finding("new_version", &[], Some("v2"))
    );
    let dropped = json!({"type":"object","properties":{"to":{"type":"string"}},"required":["to"],"additionalProperties":false});
    let newer = compare(&reviewed, &latest("v2", &dropped));
    assert_eq!(newer, finding("new_version", &["subject"], Some("v2")));
    assert!(!newer.problem(), "calls keep the pinned version");
    // A newer version that can't be offered at all differs in every field.
    let gone = compare(&reviewed, &latest("v2", &Value::Null));
    assert_eq!(gone.fields, ["subject", "to"]);
    // Providers that don't version their tools (MCP) have no latest.
    assert_eq!(
        compare(&reviewed, &json!({"schema": same, "latest": null})).status,
        "ok"
    );
    assert!(finding("failed", &[], None).problem() && finding("missing", &[], None).problem());
}

#[tokio::test]
async fn composio_describes_the_pinned_and_newest_tool_with_the_reviewed_fields() {
    let fake = FakeComposio::start().await;
    let provider = super::super::composio_for_tests(&fake);
    let Connect::SignIn { binding, .. } = provider
        .connect(
            WORKSPACE,
            "gmail",
            "http://127.0.0.1:1/api/identity/connectors/return/x",
        )
        .await
        .unwrap()
    else {
        panic!("Composio signs in through its page");
    };
    fake.sign_in(binding["account"].as_str().unwrap());
    let account = provider.finish(WORKSPACE, "gmail", &binding).await.unwrap();
    let saved = saved(&account, "gmail");
    let send = operation(&account, "gmail.send_email");
    let find = operation(&account, "gmail.find_emails");
    assert_eq!(send.version.as_deref(), Some(VERSION));
    let provider = provider.as_ref();
    assert_eq!(
        found(provider, &saved, &send).await,
        finding("ok", &[], None)
    );
    // Composio publishes a newer version with the same reviewed fields...
    fake.state.lock().unwrap().latest = Some(("20261101_00".into(), false));
    assert_eq!(
        found(provider, &saved, &send).await,
        finding("new_version", &[], Some("20261101_00"))
    );
    // ...then one whose fields differ; the pinned version still matches. The
    // newer search drops its query; the newer send can't be offered at all
    // (its recipient is no longer a plain value), so every field differs.
    fake.state.lock().unwrap().latest = Some(("20261101_00".into(), true));
    assert_eq!(found(provider, &saved, &find).await.fields, ["query"]);
    assert_eq!(
        found(provider, &saved, &send).await,
        finding(
            "new_version",
            &["body", "cc", "is_html", "subject", "to"],
            Some("20261101_00")
        )
    );
    // The pinned version itself changed.
    {
        let mut state = fake.state.lock().unwrap();
        state.latest = None;
        state.changed = true;
    }
    assert_eq!(
        found(provider, &saved, &find).await,
        finding("changed", &["query"], None)
    );
    assert_eq!(found(provider, &saved, &send).await.status, "missing");
    // A tool Composio no longer has is missing, and prepare refuses it as changed
    // (a stale tool must not count as the provider failing).
    {
        let mut state = fake.state.lock().unwrap();
        state.changed = false;
        state.retired.push("GMAIL_SEND_EMAIL".into());
    }
    assert_eq!(found(provider, &saved, &send).await.status, "missing");
    let refused = provider.prepare(&saved, &send).await.err().unwrap();
    assert!(matches!(
        refused.downcast_ref::<Refusal>(),
        Some(Refusal::Changed)
    ));
    // Describing reads tools only: nothing ran.
    assert!(fake.state.lock().unwrap().executed.is_empty());
}

#[tokio::test]
async fn operator_mcp_servers_are_described_from_their_tool_list() {
    let fake = FakeMcp::start().await;
    let provider = mcp_servers::McpServers::new(Some(fake.file.clone()));
    let Connect::Ready(account) = provider.connect(WORKSPACE, "orders", "").await.unwrap() else {
        panic!("operator accounts connect at once");
    };
    let saved = saved(&account, "orders");
    let read = operation(&account, "order.read");
    let update = operation(&account, "order.update");
    assert_eq!(
        found(&provider, &saved, &read).await,
        finding("ok", &[], None)
    );
    {
        let mut state = fake.state.lock().unwrap();
        state.tool_changes.insert(
            "read_order".into(),
            Some(json!({"type":"object","properties":{"order":{"type":"string"},"archive":{"type":"boolean"}},
                "required":["order"],"additionalProperties":false})),
        );
        state.tool_changes.insert("update_order".into(), None);
    }
    assert_eq!(
        found(&provider, &saved, &read).await,
        finding("changed", &["archive"], None)
    );
    assert_eq!(found(&provider, &saved, &update).await.status, "missing");
    assert!(
        fake.state.lock().unwrap().calls.is_empty(),
        "describing calls no tool"
    );
}

#[test]
fn verify_prints_one_line_per_action_and_a_summary() {
    let report = json!({"run":"r1","skipped":1,"problems":3,
        "counts":{"ok":1,"missing":1,"changed":1,"new_version":1,"failed":1},
        "accounts":[{"workspace":"w-1","workspace_name":"Sharma Traders","connection":"c-1","app":"gmail","name":"Gmail","provider":"composio",
            "actions":[{"id":"gmail.find_emails","status":"ok","fields":[],"version":null},
                {"id":"gmail.send_email","status":"changed","fields":["subject","to"],"version":null},
                {"id":"gmail.draft_email","status":"new_version","fields":[],"version":"20261101_00"},
                {"id":"gmail.reply_to_thread","status":"missing","fields":[],"version":null},
                {"id":"google_drive.find_files","status":"failed","fields":[],"version":null}]}]});
    let text = report_text(&report);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "Gmail · gmail (composio) · workspace Sharma Traders (w-1)"
    );
    assert!(
        lines[1].starts_with("  gmail.find_emails") && lines[1].ends_with(" ok"),
        "{text}"
    );
    assert!(lines[2].ends_with("changed fields: subject, to"), "{text}");
    assert!(
        lines[3].ends_with("new version 20261101_00 (reviewed fields unchanged)"),
        "{text}"
    );
    assert!(lines[4].ends_with("missing at the provider"), "{text}");
    assert!(
        lines[5].ends_with("not checked: the provider did not answer"),
        "{text}"
    );
    assert_eq!(
        lines[6],
        "1 accounts: 1 ok, 1 new version, 1 changed, 1 missing, 1 not checked; 1 skipped (needs reconnecting or provider off)"
    );
}

#[test]
fn the_nightly_check_is_due_once_a_day_after_two_utc() {
    let (hour, day) = (3_600_000, 86_400_000);
    // 2026-11-08 00:00 UTC. At 01:00 the slot is still the previous night's.
    let midnight = 1_794_096_000_000;
    assert_eq!(nightly_slot(midnight + hour), midnight - day + 2 * hour);
    assert_eq!(nightly_slot(midnight + 2 * hour), midnight + 2 * hour);
    assert_eq!(nightly_slot(midnight + 23 * hour), midnight + 2 * hour);
}
