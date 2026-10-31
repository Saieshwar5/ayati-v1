//! One suite every connector provider must pass, so "can we replace this
//! provider?" has a yes-or-no answer. Each provider runs it against a local fake.
use super::fake_composio::{self, FakeComposio};
use super::fake_mcp::{FakeMcp, WORKSPACE};
use super::*;

/// What the suite needs from each provider's fake.
trait Fixture {
    fn provider(&self) -> Arc<dyn ConnectorProvider>;
    fn app(&self) -> &'static str;
    fn read(&self) -> (&'static str, Value);
    fn write(&self) -> (&'static str, Value);
    /// The person finishes the provider's sign-in page.
    fn sign_in(&self, binding: &Value);
    /// The account stops working (expired, or its credential changed).
    fn break_account(&self, broken: bool);
    /// The provider changes what an action takes.
    fn change_action(&self, changed: bool);
    fn fail_calls(&self, fail: bool);
    fn calls(&self) -> usize;
    /// How a broken account is refused.
    fn broken(&self) -> &'static str;
    fn revoked(&self, binding: Option<&Value>) -> bool;
}

fn operation(account: &Account, id: &str) -> Operation {
    let found = account.definition["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap_or_else(|| panic!("{id} is offered"))
        .clone();
    serde_json::from_value(found).unwrap()
}

fn refusal(result: Result<Box<dyn Call>>) -> &'static str {
    match result {
        Ok(_) => "accepted",
        Err(error) => match error.downcast_ref::<Refusal>() {
            Some(Refusal::NeedsReconnect) => "needs_reconnect",
            Some(Refusal::Changed) => "changed",
            None => "error",
        },
    }
}

async fn run(
    provider: &dyn ConnectorProvider,
    saved: &Saved<'_>,
    op: &Operation,
    args: &Value,
) -> Outcome {
    let call = provider.prepare(saved, op).await.expect("prepared");
    call.run(args).await
}

async fn conformance(fixture: &dyn Fixture) {
    let provider = fixture.provider();
    // 1. List: the app, with reads and writes.
    let apps = provider.apps(WORKSPACE).unwrap();
    let app = apps
        .iter()
        .find(|a| a.id == fixture.app())
        .expect("the app is listed");
    assert!(
        app.operations.iter().any(|o| o.effect == "read")
            && app.operations.iter().any(|o| o.effect == "write")
    );
    // 2. Connect, through a sign-in page if the provider uses one.
    let account = match provider
        .connect(
            WORKSPACE,
            fixture.app(),
            "http://127.0.0.1:1/api/identity/connectors/return/x",
        )
        .await
        .unwrap()
    {
        Connect::Ready(account) => account,
        Connect::SignIn {
            binding,
            url,
            expires,
        } => {
            assert!(url.starts_with("http") && expires > crate::identity::user::now());
            fixture.sign_in(&binding);
            provider
                .finish(WORKSPACE, fixture.app(), &binding)
                .await
                .unwrap()
        }
    };
    assert_eq!(account.definition["id"], fixture.app());
    for op in account.definition["operations"].as_array().unwrap() {
        serde_json::from_value::<Operation>(op.clone())
            .unwrap()
            .validate()
            .unwrap();
    }
    let saved_text = format!("{}{:?}", account.definition, account.binding);
    assert!(
        !saved_text.contains(fake_composio::TOKEN),
        "tokens are never saved"
    );
    let saved = Saved {
        workspace: WORKSPACE,
        server: fixture.app(),
        binding: account.binding.as_ref(),
        fingerprint: &account.fingerprint,
    };
    let (read, read_args) = fixture.read();
    let (write, write_args) = fixture.write();
    let (read, write) = (operation(&account, read), operation(&account, write));
    // 3. Run a read and a write.
    assert!(matches!(
        run(provider.as_ref(), &saved, &read, &read_args).await,
        Outcome::Completed(_)
    ));
    assert!(matches!(
        run(provider.as_ref(), &saved, &write, &write_args).await,
        Outcome::Completed(_)
    ));
    assert_eq!(fixture.calls(), 2);
    // 4. A result the provider doesn't confirm is uncertain, never a success.
    fixture.fail_calls(true);
    assert!(matches!(
        run(provider.as_ref(), &saved, &write, &write_args).await,
        Outcome::Uncertain
    ));
    fixture.fail_calls(false);
    let sent = fixture.calls();
    // 5. A changed action is refused before anything is sent.
    fixture.change_action(true);
    assert_eq!(refusal(provider.prepare(&saved, &read).await), "changed");
    fixture.change_action(false);
    // 6. So is a broken account.
    fixture.break_account(true);
    assert_eq!(
        refusal(provider.prepare(&saved, &read).await),
        fixture.broken()
    );
    fixture.break_account(false);
    assert_eq!(fixture.calls(), sent, "refusals send nothing");
    assert_eq!(refusal(provider.prepare(&saved, &read).await), "accepted");
    // 7. Disconnect revokes at the provider.
    provider
        .disconnect(WORKSPACE, fixture.app(), account.binding.as_ref())
        .await
        .unwrap();
    assert!(fixture.revoked(account.binding.as_ref()));
}

struct Mcp(FakeMcp);
impl Fixture for Mcp {
    fn provider(&self) -> Arc<dyn ConnectorProvider> {
        Arc::new(mcp_servers::McpServers::new(Some(self.0.file.clone())))
    }
    fn app(&self) -> &'static str {
        "orders"
    }
    fn read(&self) -> (&'static str, Value) {
        ("order.read", json!({"order":"A-1"}))
    }
    fn write(&self) -> (&'static str, Value) {
        ("order.update", json!({"order":"A-1"}))
    }
    fn sign_in(&self, _: &Value) {
        panic!("operator accounts need no sign-in");
    }
    fn break_account(&self, broken: bool) {
        if broken {
            self.0.rotate()
        } else {
            self.0.restore()
        }
    }
    fn change_action(&self, changed: bool) {
        // The operator's reviewed file changes the action's meaning.
        let text = std::fs::read_to_string(&self.0.file).unwrap();
        let text = if changed {
            text.replace("Read an order", "Read and archive an order")
        } else {
            text.replace("Read and archive an order", "Read an order")
        };
        std::fs::write(&self.0.file, text).unwrap();
    }
    fn fail_calls(&self, fail: bool) {
        self.0.state.lock().unwrap().fail_call = fail;
    }
    fn calls(&self) -> usize {
        self.0.state.lock().unwrap().calls.len()
    }
    fn broken(&self) -> &'static str {
        "changed"
    }
    fn revoked(&self, binding: Option<&Value>) -> bool {
        // The operator holds MCP credentials; there is nothing to revoke here.
        binding.is_none()
    }
}

struct Composio(FakeComposio);
impl Fixture for Composio {
    fn provider(&self) -> Arc<dyn ConnectorProvider> {
        Arc::new(
            composio::Composio::new(composio::Settings {
                base: self.0.base.clone(),
                key_file: self.0.key_file.clone(),
                auth_configs: Default::default(),
            })
            .unwrap(),
        )
    }
    fn app(&self) -> &'static str {
        "gmail"
    }
    fn read(&self) -> (&'static str, Value) {
        ("gmail.find_emails", json!({"query":"from:sharma"}))
    }
    fn write(&self) -> (&'static str, Value) {
        (
            "gmail.send_email",
            json!({"to":"sharma@example.test","subject":"Balance","body":"₹1.48L due"}),
        )
    }
    fn sign_in(&self, binding: &Value) {
        self.0.sign_in(binding["account"].as_str().unwrap());
    }
    fn break_account(&self, broken: bool) {
        let accounts: Vec<String> = self
            .0
            .state
            .lock()
            .unwrap()
            .accounts
            .keys()
            .cloned()
            .collect();
        for account in accounts {
            self.0
                .set_status(&account, if broken { "EXPIRED" } else { "ACTIVE" });
        }
    }
    fn change_action(&self, changed: bool) {
        self.0.state.lock().unwrap().changed = changed;
    }
    fn fail_calls(&self, fail: bool) {
        self.0.state.lock().unwrap().fail_execute = fail;
    }
    fn calls(&self) -> usize {
        self.0.state.lock().unwrap().executed.len()
    }
    fn broken(&self) -> &'static str {
        "needs_reconnect"
    }
    fn revoked(&self, binding: Option<&Value>) -> bool {
        let account = binding.unwrap()["account"].as_str().unwrap().to_owned();
        self.0.state.lock().unwrap().removed.contains(&account)
    }
}

#[tokio::test]
async fn operator_mcp_servers_pass_the_provider_suite() {
    conformance(&Mcp(FakeMcp::start().await)).await;
}

#[tokio::test]
async fn composio_passes_the_provider_suite() {
    let fake = Composio(FakeComposio::start().await);
    conformance(&fake).await;
    let state = fake.0.state.lock().unwrap();
    assert_eq!(state.bad_keys, 0);
    // Cuckoo's field names were sent under Composio's names, and nothing else.
    let (tool, sent) = state
        .executed
        .iter()
        .find(|(t, _)| t == "GMAIL_SEND_EMAIL")
        .unwrap();
    assert_eq!(tool, "GMAIL_SEND_EMAIL");
    assert_eq!(
        sent,
        &json!({"recipient_email":"sharma@example.test","subject":"Balance","body":"₹1.48L due"})
    );
}

#[tokio::test]
async fn composio_offers_only_actions_it_can_run_with_reviewed_plain_fields() {
    let fake = Composio(FakeComposio::start().await);
    let provider = fake.provider();
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
    // Finishing before the person signed in fails rather than saving a half-made account.
    fake.0
        .set_status(binding["account"].as_str().unwrap(), "FAILED");
    assert!(provider.finish(WORKSPACE, "gmail", &binding).await.is_err());
    fake.sign_in(&binding);
    let account = provider.finish(WORKSPACE, "gmail", &binding).await.unwrap();
    let offered: Vec<&str> = account.definition["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    // Replying needs an attachment field Cuckoo doesn't offer, so it is left out.
    assert_eq!(
        offered,
        ["gmail.find_emails", "gmail.draft_email", "gmail.send_email"]
    );
    let send = operation(&account, "gmail.send_email");
    assert_eq!(send.version.as_deref(), Some(fake_composio::VERSION));
    assert!(
        send.schema["properties"].get("cc").is_none()
            && send.schema["properties"].get("user_id").is_none()
    );
    // Another workspace can't use this account.
    let other = Saved {
        workspace: "00000000-0000-4000-8000-000000000054",
        server: "gmail",
        binding: account.binding.as_ref(),
        fingerprint: &account.fingerprint,
    };
    assert_eq!(refusal(provider.prepare(&other, &send).await), "changed");
    // One auth config per toolkit: the one created first is reused.
    provider
        .connect(
            WORKSPACE,
            "gmail",
            "http://127.0.0.1:1/api/identity/connectors/return/y",
        )
        .await
        .unwrap();
    assert_eq!(fake.0.state.lock().unwrap().auth_configs.len(), 1);
    // A personal user key is refused; a project key is required.
    let user_key = fake.0.key_file.with_file_name("user-key");
    std::fs::write(&user_key, "uak_personal").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&user_key, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        composio::Composio::new(composio::Settings {
            base: fake.0.base.clone(),
            key_file: user_key,
            auth_configs: Default::default()
        })
        .is_err()
    );
}
