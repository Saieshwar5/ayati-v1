use super::*;
use serde_json::json;

fn routed(fake: &fake::Fake, dir: &std::path::Path) -> (Routed, String) {
    let file = ProvidersFile::read(&fake.providers_file(dir)).unwrap();
    let routed = file.routed("e2b").unwrap();
    let key = routed.key().unwrap();
    (routed, key)
}

#[test]
fn the_providers_file_is_checked() {
    let good = json!({
        "sizes": [{"size": "standard", "cpu": 2, "memory_gb": 4, "disk_gb": 20, "usd_per_hour": 0.17,
                   "routes": [{"provider": "e2b", "image": "cuckoo-computer-standard", "usd_per_hour": 0.1}]}],
        "providers": [{"provider": "e2b", "key_file": "/run/secrets/e2b"}]});
    let file = ProvidersFile::parse_bytes(good.to_string().as_bytes()).unwrap();
    assert_eq!(
        (file.sleep_after_minutes, file.longest_work_minutes),
        (5, 120)
    );
    assert_eq!(
        file.size("standard").unwrap().routes[0].image,
        "cuckoo-computer-standard"
    );
    let routed = file.routed("e2b").unwrap();
    assert_eq!(
        (routed.api.as_str(), routed.computers.as_str()),
        ("https://api.e2b.app", "https://sandbox.e2b.app")
    );
    let broken = |change: &dyn Fn(&mut serde_json::Value), message: &str| {
        let mut value = good.clone();
        change(&mut value);
        let error = ProvidersFile::parse_bytes(value.to_string().as_bytes()).unwrap_err();
        assert!(
            format!("{error:#}").contains(message),
            "{error:#} should say {message}"
        );
    };
    broken(
        &|v| v["providers"][0]["provider"] = json!("nimbus"),
        "Unknown computer provider",
    );
    broken(
        &|v| v["sizes"][0]["size"] = json!("huge"),
        "small, standard and large",
    );
    broken(&|v| v["sizes"][0]["memory_gb"] = json!(1), "2-64 GB memory");
    broken(&|v| v["sizes"][0]["routes"] = json!([]), "1 to 4 routes");
    broken(
        &|v| v["sizes"][0]["routes"][0]["provider"] = json!("daytona"),
        "not in providers",
    );
    broken(
        &|v| v["sizes"][0]["usd_per_hour"] = json!(-1),
        "0 to 100 dollars",
    );
    broken(
        &|v| v["providers"][0]["base_url"] = json!("http://example.com"),
        "need HTTPS",
    );
    broken(
        &|v| v["sleep_after_minutes"] = json!(0),
        "sleep after 1-120 minutes",
    );
    broken(
        &|v| v["sizes"][0]["extra"] = json!(1),
        "Invalid computer providers file",
    );
}

#[tokio::test]
async fn a_computer_is_created_used_slept_woken_and_deleted_through_the_adapter() {
    let fake = fake::start().await;
    let dir = tempfile::tempdir().unwrap();
    let (routed, key) = routed(&fake, dir.path());
    let call = Call {
        api: &routed.api,
        computers: &routed.computers,
        key: &key,
    };
    let provider = routed.provider;
    let only = Internet::Only(vec!["portal.sharmatraders.in".into()]);
    let computer = provider
        .create(
            &call,
            &Spec {
                image: "cuckoo-standard",
                awake_seconds: 600,
                internet: &only,
                label: "test",
            },
        )
        .await
        .unwrap();
    assert_eq!(
        fake.network(&computer.id)["allowOut"],
        json!(["portal.sharmatraders.in"])
    );
    assert_eq!(fake.network(&computer.id)["denyOut"], json!(["0.0.0.0/0"]));

    let home = format!("{HOME}/requests/r1");
    let ran = provider
        .run(
            &call,
            &computer,
            "echo hello; echo oops >&2; exit 3",
            &home,
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            ran.exit_code,
            ran.stdout.as_str(),
            ran.stderr.as_str(),
            ran.timed_out
        ),
        (3, "hello\n", "oops\n", false)
    );
    assert_eq!(ran.output_bytes, 11);
    let slow = provider
        .run(&call, &computer, "sleep 5", &home, Duration::from_secs(1))
        .await
        .unwrap();
    assert!(
        slow.timed_out,
        "the command is stopped inside the computer: {slow:?}"
    );

    provider
        .write(
            &call,
            &computer,
            &format!("{home}/note.txt"),
            b"kept".to_vec(),
        )
        .await
        .unwrap();
    assert_eq!(
        provider
            .read(&call, &computer, &format!("{home}/note.txt"))
            .await
            .unwrap(),
        b"kept"
    );
    assert!(
        provider
            .read(&call, &computer, &format!("{home}/missing.txt"))
            .await
            .is_err()
    );

    provider.sleep(&call, &computer.id).await.unwrap();
    provider.sleep(&call, &computer.id).await.unwrap();
    assert_eq!(fake.asleep(&computer.id), Some(true));
    assert!(
        provider
            .run(&call, &computer, "true", &home, Duration::from_secs(5))
            .await
            .is_err(),
        "asleep computers don't run"
    );
    let woken = provider.wake(&call, &computer.id, 600).await.unwrap();
    assert_eq!(
        provider
            .read(&call, &woken, &format!("{home}/note.txt"))
            .await
            .unwrap(),
        b"kept"
    );

    provider
        .set_internet(&call, &computer.id, &Internet::None)
        .await
        .unwrap();
    assert_eq!(fake.network(&computer.id)["allowOut"], json!([]));
    provider.delete(&call, &computer.id).await.unwrap();
    provider.delete(&call, &computer.id).await.unwrap();
    assert!(!fake.exists(&computer.id));
    assert!(
        provider
            .wake(&call, &computer.id, 600)
            .await
            .unwrap_err()
            .to_string()
            .contains("no longer exists")
    );
    assert_eq!(
        fake.log().first().map(String::as_str),
        Some("create cuckoo-standard")
    );
}

#[tokio::test]
async fn a_wrong_key_is_refused() {
    let fake = fake::start().await;
    let call = Call {
        api: &fake.url,
        computers: &fake.url,
        key: "wrong",
    };
    let error = adapter("e2b")
        .unwrap()
        .create(
            &call,
            &Spec {
                image: "x",
                awake_seconds: 60,
                internet: &Internet::Any,
                label: "t",
            },
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("401"), "{error}");
}

#[test]
fn output_is_clipped_on_character_boundaries() {
    assert_eq!(clip("नमस्ते", 3), "नमस\n[… output shortened]");
    assert_eq!(clip("short", 10), "short");
}

/// Against the real E2B, with `E2B_API_KEY` and the `AYATI_COMPUTER_TEST_IMAGE` template
/// (for example cuckoo-computer-dev). Run before a release; costs a few cents.
#[tokio::test]
#[ignore = "needs E2B_API_KEY and network access to E2B"]
async fn the_real_e2b_runs_the_same_lifecycle() {
    let key = std::env::var("E2B_API_KEY").expect("E2B_API_KEY");
    let image =
        std::env::var("AYATI_COMPUTER_TEST_IMAGE").unwrap_or_else(|_| "cuckoo-computer-dev".into());
    let provider = adapter("e2b").unwrap();
    let call = Call {
        api: provider.api_url(),
        computers: provider.computer_url(),
        key: &key,
    };
    let internet = Internet::Only(vec!["example.com".into()]);
    let computer = provider
        .create(
            &call,
            &Spec {
                image: &image,
                awake_seconds: 300,
                internet: &internet,
                label: "cuckoo-provider-check",
            },
        )
        .await
        .unwrap();
    let result = async {
        let home = format!("{HOME}/requests/live");
        let ran = provider.run(&call, &computer, "mkdir -p . && echo hi && curl -s -o /dev/null -w '%{http_code}' --max-time 8 https://www.google.com || true", &home, Duration::from_secs(30)).await?;
        anyhow::ensure!(ran.stdout.starts_with("hi\n") && ran.stdout.ends_with("000"), "{ran:?}");
        provider.write(&call, &computer, &format!("{home}/a.txt"), b"live".to_vec()).await?;
        provider.sleep(&call, &computer.id).await?;
        let woken = provider.wake(&call, &computer.id, 300).await?;
        anyhow::ensure!(provider.read(&call, &woken, &format!("{home}/a.txt")).await? == b"live");
        provider.set_internet(&call, &computer.id, &Internet::Any).await?;
        let any = provider.run(&call, &woken, "curl -s -o /dev/null -w '%{http_code}' --max-time 8 https://www.google.com", &home, Duration::from_secs(30)).await?;
        anyhow::ensure!(any.stdout != "000", "any website after the change: {any:?}");
        Ok(())
    }
    .await;
    provider.delete(&call, &computer.id).await.unwrap();
    result.unwrap();
}

/// The browser helper on a real E2B computer: one action, then the live picture streamed
/// out with a person's input sent in by tag, as the controller does it.
#[tokio::test]
#[ignore = "needs E2B_API_KEY and spends a little E2B time"]
async fn the_real_e2b_runs_the_browser_helper_and_its_live_picture() {
    let key = std::env::var("E2B_API_KEY").expect("E2B_API_KEY");
    let image =
        std::env::var("AYATI_COMPUTER_TEST_IMAGE").unwrap_or_else(|_| "cuckoo-computer-dev".into());
    let provider = adapter("e2b").unwrap();
    let call = Call {
        api: provider.api_url(),
        computers: provider.computer_url(),
        key: &key,
    };
    let spec = Spec {
        image: &image,
        awake_seconds: 300,
        internet: &Internet::Only(vec!["example.com".into()]),
        label: "cuckoo-provider-check",
    };
    let computer = provider.create(&call, &spec).await.unwrap();
    let result = async {
        provider
            .write(
                &call,
                &computer,
                super::BROWSER_HELPER_PATH,
                super::BROWSER_HELPER.as_bytes().to_vec(),
            )
            .await?;
        let open = format!(
            "node {} open '{{\"url\":\"https://example.com\"}}'",
            super::BROWSER_HELPER_PATH
        );
        let page = provider
            .run(&call, &computer, &open, HOME, Duration::from_secs(60))
            .await?;
        anyhow::ensure!(
            page.stdout.contains("\"title\":\"Example Domain\""),
            "{page:?}"
        );
        let tag = super::live::tag();
        let live = format!("node {} live '{{}}'", super::BROWSER_HELPER_PATH);
        let mut output = provider.stream(&call, &computer, &live, &tag).await?;
        let mut seen = String::new();
        while !seen.contains("\"frame\"") {
            let Some(chunk) = tokio::time::timeout(Duration::from_secs(20), output.recv()).await?
            else {
                anyhow::bail!("the live picture ended early: {seen}");
            };
            seen.push_str(&String::from_utf8_lossy(&chunk));
        }
        provider
            .send(&call, &computer, &tag, b"{\"key\":\"PageDown\"}\n")
            .await?;
        provider
            .send(&call, &computer, &tag, b"{\"stop\":true}\n")
            .await?;
        // The program ends after `stop`, closing the stream.
        while tokio::time::timeout(Duration::from_secs(20), output.recv())
            .await?
            .is_some()
        {}
        Ok(())
    }
    .await;
    provider.delete(&call, &computer.id).await.unwrap();
    result.unwrap();
}
