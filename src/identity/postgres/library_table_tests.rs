//! Library LB4: tables are versioned Library items; agents look rows up exactly (cited)
//! and change a row only with Edit; a company rule can take its limit from a table.
use super::super::{tests::*, *};
use super::folders::{ask, grant, setup, share};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::*;
use crate::identity::postgres::SaveTable;
use serde_json::json;

fn prices(id: &str, revision: i64, folder: Option<&str>, desk: i64) -> SaveTable {
    SaveTable {
        id: id.into(),
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        title: "Price list".into(),
        table: json!({"type":"table","columns":[{"name":"Product","type":"text"},{"name":"Price","type":"number"},{"name":"Max discount","type":"number"}],
            "rows":[["Standing desk",desk,10],["Chair",300,null]]}),
        folder: folder.map(str::to_owned),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_look_rows_up_exactly_and_change_them_only_with_edit() {
    let s = setup().await;
    let db = &s.db;
    let policies = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    let table = uuid::Uuid::new_v4().to_string();
    let mut bad = prices(&table, 0, Some(&policies), 1250);
    bad.table["rows"][1][1] = json!("cheap");
    assert!(
        db.library_table_save(&s.owner, &s.w, &bad).await.is_err(),
        "number columns hold numbers"
    );
    db.library_table_save(&s.owner, &s.w, &prices(&table, 0, Some(&policies), 1250))
        .await
        .unwrap();
    assert!(
        db.library_table_save(&s.owner, &s.w, &prices(&table, 0, None, 1))
            .await
            .is_err()
            || db.library_get(&s.owner, &s.w, &table).await.unwrap()["revision"] == 1
    );
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "What does the standing desk cost?").await;
    let found = invoke(
        db,
        &job,
        "lookup",
        json!({"table":table,"key":"  standing DESK "}),
    )
    .await
    .unwrap();
    assert_eq!(found["state"], "found");
    assert_eq!(
        found["row"],
        json!({"Product":"Standing desk","Price":1250,"Max discount":10})
    );
    assert_eq!(
        invoke(db, &job, "lookup", json!({"table":table,"key":"Lamp"}))
            .await
            .unwrap()["state"],
        "no_row"
    );
    let searched = invoke(db, &job, "search_library", json!({"query":"chair price"}))
        .await
        .unwrap();
    assert_eq!(
        searched["results"][0]["title"], "Price list",
        "a table's rows are searchable"
    );
    assert_eq!(
        invoke(
            db,
            &job,
            "set_table_row",
            json!({"table":table,"revision":1,"key":"Chair","values":{"Price":320}})
        )
        .await
        .unwrap()["state"],
        "not_allowed"
    );
    db.chat_finish(&job, "1250.", "complete").await.unwrap();
    let answer: String = query(
        &db.db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "content")
    .unwrap();
    assert!(
        answer.contains("[Price list › Standing desk · revision 1]"),
        "{answer}"
    );
    // With Edit, for a person who may edit, one row changes as a version marked with the agent.
    let folder = db.library_get(&s.owner, &s.w, &policies).await.unwrap();
    let mut g = grant(&s.agent, "staff");
    g.level = "edit".into();
    db.library_share(
        &s.owner,
        &s.w,
        &policies,
        &share(
            folder["access_revision"].as_i64().unwrap(),
            "private",
            vec![],
            vec![g],
        ),
    )
    .await
    .unwrap();
    let private = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let mine = ask(&s, &s.owner, &private, "Raise the chair to 320").await;
    assert_eq!(
        invoke(
            db,
            &mine,
            "set_table_row",
            json!({"table":table,"revision":9,"key":"Chair","values":{"Price":320}})
        )
        .await
        .unwrap()["state"],
        "changed"
    );
    assert_eq!(
        invoke(
            db,
            &mine,
            "set_table_row",
            json!({"table":table,"revision":1,"key":"Chair","values":{"Price":"cheap"}})
        )
        .await
        .unwrap()["state"],
        "invalid"
    );
    assert_eq!(
        invoke(
            db,
            &mine,
            "set_table_row",
            json!({"table":table,"revision":1,"key":"Chair","values":{"Price":320}})
        )
        .await
        .unwrap()["revision"],
        2
    );
    let saved = db.library_get(&s.owner, &s.w, &table).await.unwrap();
    assert_eq!(saved["document"]["rows"][1], json!(["Chair", 320, null]));
    assert_eq!(
        db.library_versions(&s.owner, &s.w, &table).await.unwrap()["versions"][0]["agent"],
        "Pool agent"
    );
    db.chat_finish(&mine, "Done.", "complete").await.unwrap();
    // Restoring the first version is the undo, for tables too.
    db.library_action(
        &s.owner,
        &s.w,
        &table,
        &ItemAction {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 2,
            action: "restore_version".into(),
            version: Some(1),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.library_get(&s.owner, &s.w, &table).await.unwrap()["document"]["rows"][1][1],
        300
    );
    // People edit the whole table from the revision they read.
    assert!(
        db.library_table_save(&s.member, &s.w, &prices(&table, 3, None, 1300))
            .await
            .is_err()
    );
    assert!(
        db.library_table_save(&s.owner, &s.w, &prices(&table, 2, None, 1300))
            .await
            .is_err(),
        "stale"
    );
    db.library_table_save(&s.owner, &s.w, &prices(&table, 3, None, 1300))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_company_rule_takes_its_limit_from_a_library_table() {
    let db = fixture().await;
    let owner = user(&db, "table-rule-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat_id, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"shop","name":"Shop","operations":[{"id":"discount.apply","title":"Apply a discount","tool":"discount","effect":"write","schema":{"type":"object","properties":{"product":{"type":"string"},"discount":{"type":"number"}},"required":["product"],"additionalProperties":false}}]});
    db.control_system(
        &owner,
        &w,
        &crate::identity::control::connections_model::SystemApproval {
            server: "shop".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    db.control_connect(&owner, &w, &agent, &connection, definition, "reviewed")
        .await
        .unwrap();
    super::super::tool_test_support::grant(
        &db,
        &owner,
        &w,
        &agent,
        &connection,
        "discount.apply",
        "allow",
    )
    .await;
    let table = uuid::Uuid::new_v4().to_string();
    db.library_table_save(&owner, &w, &prices(&table, 0, None, 1250))
        .await
        .unwrap();
    let rule = |value: Value| crate::identity::postgres::control_actions::guard::store::GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: Some("discount.apply".into()),
        connection: None,
        meaning: None,
        source: None,
        kind: "field".into(),
        field: "discount".into(),
        comparator: ">".into(),
        value,
        effect: "ask".into(),
        note: String::new(),
        enabled: true,
        revision: 0,
        trial: false,
    };
    assert!(
        db.guard_rule_save(
            &owner,
            &w,
            &rule(json!({"table":table,"match":"product","column":"Product"}))
        )
        .await
        .is_err(),
        "a number column"
    );
    assert!(
        db.guard_rule_save(
            &owner,
            &w,
            &rule(json!({"table":table,"match":"colour","column":"Max discount"}))
        )
        .await
        .is_err(),
        "a request field"
    );
    let saved = db
        .guard_rule_save(
            &owner,
            &w,
            &rule(json!({"table":table,"match":"product","column":"Max discount"})),
        )
        .await
        .unwrap();
    assert_eq!(
        saved["summary"],
        "Apply a discount: if discount is over Max discount in Price list for its product, ask first."
    );
    db.chat_send(&owner, &w, &chat_id, &message("Discounts", Some(&agent)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let request = |product: &str, discount: f64| crate::identity::control::model::ActionRequest {
        connection: connection.clone(),
        operation: "discount.apply".into(),
        arguments: json!({"product":product,"discount":discount}),
    };
    let within = db
        .control_propose(&job, &request("Standing desk", 5.0))
        .await
        .unwrap();
    assert_eq!(within.action["state"], "executing");
    db.control_finish(&w, &within.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    let over = db
        .control_propose(&job, &request("standing desk", 15.0))
        .await
        .unwrap();
    assert_eq!(over.action["state"], "pending");
    assert_eq!(
        over.action["guard"]["facts"]["limit"],
        json!({"table":"Price list","row":"standing desk","column":"Max discount","limit":10})
    );
    let unknown = db
        .control_propose(&job, &request("Chair", 1.0))
        .await
        .unwrap();
    assert_eq!(
        unknown.action["state"], "pending",
        "no number in the table: a person decides"
    );
    let replay = db
        .guard_rule_replay(
            &owner,
            &w,
            &rule(json!({"table":table,"match":"product","column":"Max discount"})),
        )
        .await
        .unwrap();
    assert_eq!(
        (replay["examined"].as_i64(), replay["would_hold"].as_i64()),
        (Some(3), Some(2)),
        "replay reads the table too"
    );
}
