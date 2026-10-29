use super::*;
use crate::config::Executor;

const LOCAL: &[(&str, &str)] = &[
    (
        "RELAY_DATABASE_URL",
        "postgresql://relay_app@localhost/relay",
    ),
    ("WORKOS_CLIENT_ID", "client_fixture"),
    ("WORKOS_API_KEY", "fixture-key"),
    ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
    (
        "AYATI_WORKER_CONTROL_KEY_FILE",
        "/private/worker-control.key",
    ),
];

fn controller(extra: &[(&str, &str)]) -> Result<Controller> {
    let pairs: Vec<_> = LOCAL.iter().chain(extra).copied().collect();
    Controller::read(&mut Reader::from_pairs(&pairs), false)
}

#[test]
fn a_complete_local_controller_uses_loopback_and_pooled_defaults() {
    let config = controller(&[]).unwrap();
    assert_eq!(config.origin, "http://127.0.0.1:8765");
    assert_eq!(config.execution, Execution::Pooled);
    assert_eq!(config.pool.capacity, 2);
    assert_eq!(config.pool.workspace_limit, 2);
    assert!(!config.hosted());
}

#[test]
fn every_problem_is_reported_together() {
    let error = Controller::read(
        &mut Reader::from_pairs(&[
            ("AYATI_PORT", "0"),
            ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
            ("AYATI_EXECUTOR_CAPACITY", "lots"),
        ]),
        false,
    )
    .err()
    .unwrap()
    .to_string();
    for expected in [
        "AYATI_PORT must be a number",
        "Set RELAY_DATABASE_URL",
        "Set WORKOS_CLIENT_ID and WORKOS_API_KEY",
        "Set both AYATI_WORKER_CONTROL_URL and AYATI_WORKER_CONTROL_KEY_FILE",
        "AYATI_EXECUTOR_CAPACITY must be a number",
    ] {
        assert!(error.contains(expected), "{expected} missing from: {error}");
    }
}

#[test]
fn hosted_controllers_require_https_and_private_storage() {
    let hosted = controller(&[
        ("AYATI_PUBLIC_ORIGIN", "https://workspace.example.test/"),
        ("AYATI_BIND_ADDRESS", "0.0.0.0"),
    ])
    .unwrap();
    assert_eq!(hosted.origin, "https://workspace.example.test");
    assert!(hosted.hosted());
    assert!(controller(&[("AYATI_BIND_ADDRESS", "0.0.0.0")]).is_err());
    for invalid in [
        "http://example.test",
        "https://user:secret@example.test",
        "https://example.test/path",
        "https://example.test/?token=secret",
        "https://example.test/#fragment",
        "not a URL",
    ] {
        assert!(
            controller(&[("AYATI_PUBLIC_ORIGIN", invalid)]).is_err(),
            "{invalid}"
        );
    }
    assert!(controller(&[("AYATI_BIND_ADDRESS", "hostname")]).is_err());
    assert!(
        controller(&[
            ("AYATI_PUBLIC_ORIGIN", "https://workspace.example.test"),
            ("CUCKOO_LIBRARY_LOCAL_ROOT", "/private/library"),
        ])
        .is_err()
    );
}

#[test]
fn retired_modes_and_executor_settings_are_refused_on_the_controller() {
    assert!(controller(&[("AYATI_CHAT_EXECUTION", "external")]).is_err());
    assert!(controller(&[("AYATI_CHAT_EXECUTION", "scripted")]).is_err());
    assert!(controller(&[("AYATI_EXECUTOR_SLOTS", "2")]).is_err());
}

#[test]
fn executors_refuse_controller_secrets_and_need_worker_control() {
    let base = [
        ("AYATI_MODEL", "fixture"),
        ("AYATI_WORKER_CONTROL_URL", "http://127.0.0.1:8766"),
        (
            "AYATI_WORKER_CONTROL_KEY_FILE",
            "/private/worker-control.key",
        ),
    ];
    assert_eq!(
        Executor::read(&mut Reader::from_pairs(&base))
            .unwrap()
            .slots,
        2
    );
    for secret in [
        "RELAY_DATABASE_URL",
        "WORKOS_API_KEY",
        "AYATI_MCP_SERVERS_FILE",
    ] {
        let pairs: Vec<_> = base.iter().copied().chain([(secret, "x")]).collect();
        let error = Executor::read(&mut Reader::from_pairs(&pairs))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(secret), "{error}");
    }
    assert!(Executor::read(&mut Reader::from_pairs(&[("AYATI_MODEL", "fixture")])).is_err());
}

#[test]
fn controllers_only_run_pooled_and_simulations_default_to_scripted() {
    assert_eq!(Execution::parse(None, false), Ok(Execution::Pooled));
    assert_eq!(
        Execution::parse(Some("pooled"), true),
        Ok(Execution::Pooled)
    );
    assert_eq!(Execution::parse(None, true), Ok(Execution::Scripted));
    assert!(Execution::parse(Some("scripted"), false).is_err());
    for retired in ["embedded", "external", "assigned"] {
        assert!(Execution::parse(Some(retired), false).is_err());
    }
}
