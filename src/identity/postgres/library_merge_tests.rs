//! A person's change to another section of a page, or another row of a table, is kept
//! together with the agent's when the agent saves; the same section or row is refused.
use super::super::*;
use super::folders::page;
use super::workbench::{asked, bench, level, run, tool};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_persons_change_to_another_section_or_row_is_kept_with_the_agents() {
    let b = bench().await;
    let db = &b.s.db;
    level(&b, "edit").await;
    // A page with two sections; the agent opens it at revision 2.
    let mut sections = page("Returns policy", "x", None);
    sections.id = b.s.returns.clone();
    sections.revision = 1;
    sections.document = crate::identity::library::page_markdown::from_markdown(
        "# Returns\n\n30 days.\n\n# Shipping\n\nTwo days.\n",
    );
    db.library_save(&b.s.owner, &b.s.w, &sections)
        .await
        .unwrap();
    let job = asked(&b, &b.s.owner, "Make returns 45 days").await;
    tool(&b, &job, "open_document", json!({"id": b.s.returns})).await;
    // Meanwhile a person changes Shipping.
    sections.request = uuid::Uuid::new_v4().to_string();
    sections.revision = 2;
    sections.document = crate::identity::library::page_markdown::from_markdown(
        "# Returns\n\n30 days.\n\n# Shipping\n\nThree days.\n",
    );
    db.library_save(&b.s.owner, &b.s.w, &sections)
        .await
        .unwrap();
    run(
        &b,
        &job,
        "sed -i 's/30 days/45 days/' 'library/Returns policy.md'",
    )
    .await;
    let saved = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Returns policy.md"}),
    )
    .await;
    assert_eq!(
        (saved["state"].as_str(), saved["revision"].as_i64()),
        (Some("edited"), Some(4)),
        "{saved}"
    );
    assert!(saved["merged"].is_string());
    let page_now = db
        .library_get(&b.s.owner, &b.s.w, &b.s.returns)
        .await
        .unwrap();
    let words = crate::identity::library::page_markdown::to_markdown(&page_now["document"]);
    assert!(
        words.contains("45 days") && words.contains("Three days"),
        "{words}"
    );
    // The same section changed by both: the agent opens it again.
    sections.request = uuid::Uuid::new_v4().to_string();
    sections.revision = 4;
    sections.document = crate::identity::library::page_markdown::from_markdown(
        "# Returns\n\n60 days.\n\n# Shipping\n\nThree days.\n",
    );
    db.library_save(&b.s.owner, &b.s.w, &sections)
        .await
        .unwrap();
    run(
        &b,
        &job,
        "sed -i 's/45 days/50 days/' 'library/Returns policy.md'",
    )
    .await;
    assert_eq!(
        tool(
            &b,
            &job,
            "save_file",
            json!({"path": "library/Returns policy.md"})
        )
        .await["state"],
        "changed"
    );
    // A table: a person changes one row, the agent another.
    let table = |rows: Value| json!({"type":"table","columns":[{"name":"Product","type":"text"},{"name":"Price","type":"number"}],"rows":rows});
    let mut prices = crate::identity::postgres::SaveTable {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Price list".into(),
        table: table(json!([["Desk", 1200], ["Chair", 300]])),
        folder: Some(b.policies.clone()),
    };
    db.library_table_save(&b.s.owner, &b.s.w, &prices)
        .await
        .unwrap();
    tool(&b, &job, "open_document", json!({"id": prices.id})).await;
    prices.request = uuid::Uuid::new_v4().to_string();
    prices.revision = 1;
    prices.folder = None;
    prices.table = table(json!([["Desk", 1200], ["Chair", 320]]));
    db.library_table_save(&b.s.owner, &b.s.w, &prices)
        .await
        .unwrap();
    run(
        &b,
        &job,
        "printf 'Product,Price\\nDesk,1250\\nChair,300\\n' > 'library/Price list.csv'",
    )
    .await;
    let saved = tool(
        &b,
        &job,
        "save_file",
        json!({"path": "library/Price list.csv"}),
    )
    .await;
    assert_eq!(
        (saved["state"].as_str(), saved["revision"].as_i64()),
        (Some("saved"), Some(3)),
        "{saved}"
    );
    assert_eq!(
        db.library_get(&b.s.owner, &b.s.w, &prices.id)
            .await
            .unwrap()["document"]["rows"],
        json!([["Desk", 1250], ["Chair", 320]])
    );
}
