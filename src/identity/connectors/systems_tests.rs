//! The `system` provider keeps the same call contract as every provider in the
//! conformance suite. Its connect step differs (an owner reviews the tools on
//! the agent's Tools tab), so this suite starts from a reviewed listing.
use super::fake_system::{self, FakeSystem};
use super::systems::{self, Address};
use super::*;

const WORKSPACE: &str = "00000000-0000-4000-8000-000000000116";

fn refusal(result: Result<Box<dyn Call>>) -> &'static str {
    match result {
        Ok(_) => "accepted",
        Err(error) => match error.downcast_ref::<Refusal>() {
            Some(Refusal::Changed) => "changed",
            Some(Refusal::NeedsReconnect) => "needs_reconnect",
            None => "error",
        },
    }
}

#[tokio::test]
async fn a_connected_system_keeps_the_provider_call_contract() {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let address = Address {
        url: fake.url.clone(),
        key: Some(fake_system::KEY.into()),
    };
    // Review: every tool is listed; the nested one is explained, not offered.
    let listed = systems::list(WORKSPACE, &address).await.unwrap();
    assert_eq!(listed.len(), 5);
    let unsupported = listed.iter().find(|t| t.name == "export_students").unwrap();
    assert!(
        unsupported.schema.is_none() && unsupported.reason.as_deref().unwrap().contains("filter")
    );
    assert_eq!(listed[0].suggested, Some("read"));
    let read = listed[0].operation("read").unwrap();
    let write = listed[2].operation("write").unwrap();
    // The key is sealed in the binding, never kept in plain text.
    let server = systems::server_name("6f1c2b4e-0d1a-4c5b-9e7f-1234567890ab");
    let binding = address.binding(WORKSPACE, &server).unwrap();
    assert!(!binding.to_string().contains(fake_system::KEY));
    let saved = Saved {
        workspace: WORKSPACE,
        server: &server,
        binding: Some(&binding),
        fingerprint: "reviewed",
    };
    let provider = Arc::new(systems::Systems);
    assert!(
        provider.apps(WORKSPACE).unwrap().is_empty(),
        "never offered in Add app"
    );
    // A read and a write run.
    for (op, args) in [
        (&read, json!({"student":"stu-104"})),
        (
            &write,
            json!({"student":"stu-104","plan":"Two weeks of fractions"}),
        ),
    ] {
        let call = provider.prepare(&saved, op).await.unwrap();
        assert!(matches!(
            call.run(&args, "key").await,
            Outcome::Completed(_)
        ));
    }
    assert_eq!(fake.state.lock().unwrap().calls.len(), 2);
    // A result the system doesn't confirm is uncertain, never a success.
    fake.state.lock().unwrap().fail = true;
    let call = provider.prepare(&saved, &write).await.unwrap();
    assert!(matches!(
        call.run(&json!({"student":"stu-104","plan":"x"}), "key")
            .await,
        Outcome::Uncertain
    ));
    fake.state.lock().unwrap().fail = false;
    let sent = fake.state.lock().unwrap().calls.len();
    // A changed or removed tool is held; the others keep working.
    fake.state.lock().unwrap().changes.insert(
        "get_marks".into(),
        Some(json!({"type":"object","properties":{"student":{"type":"string"},"term":{"type":"integer"}},"required":["student","term"]})),
    );
    assert_eq!(refusal(provider.prepare(&saved, &read).await), "changed");
    assert_eq!(refusal(provider.prepare(&saved, &write).await), "accepted");
    fake.state
        .lock()
        .unwrap()
        .changes
        .insert("get_marks".into(), None);
    assert_eq!(refusal(provider.prepare(&saved, &read).await), "changed");
    let described = provider.describe(&saved, &read).await.unwrap();
    assert!(
        described["schema"].is_null(),
        "the drift check sees it gone"
    );
    fake.state.lock().unwrap().changes.clear();
    assert_eq!(
        provider.describe(&saved, &read).await.unwrap()["schema"],
        read.schema
    );
    // A system that refuses the key sends nothing.
    fake.state.lock().unwrap().refuse = true;
    assert_eq!(refusal(provider.prepare(&saved, &read).await), "error");
    fake.state.lock().unwrap().refuse = false;
    assert_eq!(
        fake.state.lock().unwrap().calls.len(),
        sent,
        "refusals send nothing"
    );
    // A sealed key moved to another connection can't be opened.
    let elsewhere = Saved {
        server: "system-7a1c2b4e-0d1a-4c5b-9e7f-1234567890ab",
        ..saved
    };
    assert_eq!(refusal(provider.prepare(&elsewhere, &read).await), "error");
}

#[tokio::test]
async fn reaching_a_system_says_what_to_fix() {
    fake_system::secrets();
    let fake = FakeSystem::start().await;
    let wrong_key = Address {
        url: fake.url.clone(),
        key: Some("wrong".into()),
    };
    let error = systems::list(WORKSPACE, &wrong_key)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Check the key"), "{error}");
    let private = Address {
        url: "https://10.0.0.5/mcp".into(),
        key: None,
    };
    assert!(systems::list(WORKSPACE, &private).await.is_err());
}

#[tokio::test]
#[ignore = "runs tools/examples/studyhub_mcp.py with python3"]
async fn the_guide_example_server_connects_and_every_tool_is_usable() {
    fake_system::secrets();
    let mut child = std::process::Command::new("python3")
        .arg("tools/examples/studyhub_mcp.py")
        .env("STUDYHUB_KEY", "guide-example-key")
        .spawn()
        .unwrap();
    let address = Address {
        url: "http://127.0.0.1:8765/mcp".into(),
        key: Some("guide-example-key".into()),
    };
    let mut listed = None;
    for _ in 0..50 {
        if let Ok(tools) = systems::list(WORKSPACE, &address).await {
            listed = Some(tools);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    let listed = listed.expect("the example server answers");
    assert_eq!(listed.len(), 2);
    assert!(
        listed.iter().all(|t| t.schema.is_some()),
        "every example tool is usable"
    );
    assert_eq!(listed[0].suggested, Some("read"));
    assert_eq!(listed[1].suggested, Some("write"));
}
