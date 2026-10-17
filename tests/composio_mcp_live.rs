use ayati::platform::composio::Composio;

/// Explicit opt-in: reads catalog/account metadata, never executes an app tool.
#[tokio::test]
#[ignore = "requires the owner's Composio Connect key and network"]
async fn consumer_mcp_live_metadata_contract() {
    dotenvy::dotenv().ok();
    let client = Composio::from_env()
        .unwrap()
        .expect("Configure COMPOSIO_API_KEY");
    assert!(
        client.is_consumer(),
        "This check requires a Connect consumer key"
    );
    let tools = client
        .search_tools("gmail", "List Gmail messages")
        .await
        .unwrap();
    let definition = tools["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["slug"] == "GMAIL_FETCH_EMAILS")
        .expect("Gmail read operation not discovered");
    let pinned = client
        .tool_definition(
            "GMAIL_FETCH_EMAILS",
            definition["version"].as_str().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(definition["version"], pinned["version"]);
    let accounts = client.consumer_accounts("gmail").await.unwrap();
    for account in accounts.as_array().unwrap() {
        assert!(account.get("user_info").is_none());
        assert!(account.get("state").is_none());
        assert!(account["id"].is_string());
    }
}
