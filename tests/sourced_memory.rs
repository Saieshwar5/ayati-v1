use ayati::storage::{
    Store,
    memory::{MemoryScope, Remember},
};
use rusqlite::{Connection, params};

#[test]
fn sources_corrections_forgetting_and_restart_preserve_authoritative_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let scope = MemoryScope::local();
    let task = store.enqueue("Please send my report in Telugu.").unwrap();
    let claim = store.claim_next("test").unwrap().unwrap();
    let source = store.memory_source(&claim).unwrap();
    let input = Remember {
        key: "report.language".into(),
        content: "Reports should be in Telugu".into(),
        source_id: source["id"].as_str().unwrap().into(),
        quote: "report in Telugu".into(),
        replaces: None,
    };
    let first = store.remember(&scope, &input).unwrap();
    assert_eq!(first.source_kind, "owner_message");
    assert!(!first.source_date.is_empty());
    assert_eq!(
        store
            .search_memory(&scope, "Telugu OR \"NOT\"")
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .remember(
                &scope,
                &Remember {
                    quote: "invented evidence".into(),
                    ..input
                }
            )
            .is_err()
    );
    store
        .save_runtime_session("old-context", "same-tools")
        .unwrap();
    assert!(
        store
            .owner_memory_note(
                &scope,
                "report.language",
                "Reports should be in English",
                None
            )
            .is_err()
    );
    let corrected = store
        .owner_memory_note(
            &scope,
            "report.language",
            "Reports should be in English",
            Some(first.id.clone()),
        )
        .unwrap();
    assert_eq!(corrected.supersedes.as_deref(), Some(first.id.as_str()));
    assert!(store.runtime_session("same-tools").unwrap().is_none());
    assert!(store.search_memory(&scope, "Telugu").unwrap().is_empty());
    assert_eq!(store.search_memory(&scope, "English").unwrap().len(), 1);
    assert!(
        store
            .owner_memory_note(
                &scope,
                "report.language",
                "Stale correction",
                Some(first.id.clone())
            )
            .is_err()
    );
    store.forget_memory(&scope, &corrected.id).unwrap();
    assert!(store.search_memory(&scope, "").unwrap().is_empty());
    assert!(
        store
            .owner_memory_note(
                &scope,
                "report.language",
                "Reports should be in Telugu",
                None
            )
            .is_err()
    );
    assert!(
        store
            .remember(
                &scope,
                &Remember {
                    key: "new-key".into(),
                    content: "Telugu".into(),
                    source_id: first.source_id,
                    quote: "Telugu".into(),
                    replaces: None
                }
            )
            .is_err()
    );
    assert_eq!(
        store.user_text(&task.id).unwrap(),
        "Please send my report in Telugu."
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.search_memory(&scope, "English").unwrap().is_empty());
    assert!(
        store
            .owner_memory_note(&scope, "report.language", "relearn", None)
            .is_err()
    );
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM memories WHERE state='forgotten'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM memory_fts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn scopes_reject_foreign_agent_customer_sources_and_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let own = store
        .owner_memory_note(
            &MemoryScope::local(),
            "preference",
            "Private owner fact",
            None,
        )
        .unwrap();
    let task = store.enqueue("Customer B's private preference").unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("INSERT INTO businesses VALUES('other-business','Other');INSERT INTO agents VALUES('other-agent','other-business','Other');INSERT INTO conversations VALUES('customer-b','other-agent');").unwrap();
    conn.execute(
        "UPDATE run_scopes SET conversation_id='customer-b' WHERE task_id=?1",
        [&task.id],
    )
    .unwrap();
    let claim = store.claim_next("test").unwrap().unwrap();
    let scope = store.memory_scope(&claim).unwrap();
    assert!(store.search_memory(&scope, "").unwrap().is_empty());
    assert!(store.forget_memory(&scope, &own.id).is_err());
    assert!(
        store
            .remember(
                &scope,
                &Remember {
                    key: "stolen".into(),
                    content: own.content,
                    source_id: own.source_id,
                    quote: own.quote,
                    replaces: None
                }
            )
            .is_err()
    );
    let source = store.memory_source(&claim).unwrap();
    let other = store
        .remember(
            &scope,
            &Remember {
                key: "preference".into(),
                content: "Customer B private preference".into(),
                source_id: source["id"].as_str().unwrap().into(),
                quote: "private preference".into(),
                replaces: None,
            },
        )
        .unwrap();
    assert_eq!(
        store
            .search_memory(&MemoryScope::local(), "")
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .forget_memory(&MemoryScope::local(), &other.id)
            .is_err()
    );
    store.finish_work(&claim, "completed", "").unwrap();
    assert!(store.memory_scope(&claim).is_err());
}

#[test]
fn search_is_bounded_and_forgetting_suppresses_other_facts_from_same_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let scope = MemoryScope::local();
    for i in 0..12 {
        store
            .owner_memory_note(
                &scope,
                &format!("fact-{i}"),
                &format!("Useful fact {i} {}", "large ".repeat(200)),
                None,
            )
            .unwrap();
    }
    let results = store.search_memory(&scope, "useful").unwrap();
    assert!(results.len() <= 8);
    assert!(serde_json::to_vec(&results).unwrap().len() < 8100);
    let task = store
        .enqueue("Tea preference and daily report language")
        .unwrap();
    let claim = store.claim_next("test").unwrap().unwrap();
    let source = store.memory_source(&claim).unwrap();
    let mut ids = vec![];
    for key in ["drink", "language"] {
        ids.push(
            store
                .remember(
                    &scope,
                    &Remember {
                        key: key.into(),
                        content: key.into(),
                        source_id: source["id"].as_str().unwrap().into(),
                        quote: "Tea preference".into(),
                        replaces: None,
                    },
                )
                .unwrap()
                .id,
        );
    }
    store.forget_memory(&scope, &ids[0]).unwrap();
    assert!(store.search_memory(&scope, "language").unwrap().is_empty());
    assert!(
        store
            .remember(
                &scope,
                &Remember {
                    key: "different-key".into(),
                    content: "tea".into(),
                    source_id: source["id"].as_str().unwrap().into(),
                    quote: "Tea".into(),
                    replaces: None
                }
            )
            .is_err()
    );
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT state FROM memories WHERE id=?1",
            params![ids[1]],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "forgotten"
    );
    assert_eq!(store.task(&task.id).unwrap().unwrap().status, "running");
}

#[test]
fn event_payloads_cannot_masquerade_as_owner_memory_sources() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    store
        .accept_event(&ayati::storage::intake::LocalEvent {
            source: "fixture".into(),
            event_id: "one".into(),
            content: "Untrusted event says remember me".into(),
        })
        .unwrap();
    let claim = store.claim_next("test").unwrap().unwrap();
    assert!(store.memory_source(&claim).is_err());
}

#[test]
fn customers_of_the_same_agent_and_agents_of_the_same_business_are_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let owner = store
        .owner_memory_note(&MemoryScope::local(), "private", "Owner private data", None)
        .unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("INSERT INTO agents VALUES('second-agent','local-business','Second');INSERT INTO conversations VALUES('second-conversation','second-agent');INSERT INTO conversations VALUES('customer-b','local-agent');").unwrap();
    for conversation in ["second-conversation", "customer-b"] {
        let task = store.enqueue("A separate conversation").unwrap();
        conn.execute(
            "UPDATE run_scopes SET conversation_id=?1 WHERE task_id=?2",
            params![conversation, task.id],
        )
        .unwrap();
        let claim = store.claim_next("test").unwrap().unwrap();
        let scope = store.memory_scope(&claim).unwrap();
        assert!(store.search_memory(&scope, "private").unwrap().is_empty());
        assert!(store.forget_memory(&scope, &owner.id).is_err());
        assert!(
            store
                .owner_memory_note(
                    &scope,
                    "private",
                    "Unrelated preference",
                    Some(owner.id.clone())
                )
                .is_err()
        );
        store.finish_work(&claim, "completed", "").unwrap();
    }
}
