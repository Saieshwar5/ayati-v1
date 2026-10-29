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
    for key in ["OPENAI_API_KEY", "FIREWORKS_API_KEY"] {
        let error = controller(&[(key, "x")]).err().unwrap().to_string();
        assert!(error.contains(key), "{error}");
    }
}

#[test]
fn the_drain_delay_fits_inside_the_shutdown_limit() {
    let config = controller(&[]).unwrap();
    assert_eq!((config.drain_seconds, config.shutdown_seconds), (0, 60));
    assert!(controller(&[("AYATI_DRAIN_SECONDS", "15")]).is_ok());
    assert!(controller(&[("AYATI_DRAIN_SECONDS", "60")]).is_err());
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

#[test]
fn worker_control_uses_tls_exactly_for_private_network_urls() {
    let https = [
        ("AYATI_WORKER_CONTROL_URL", "https://worker.internal:8766"),
        ("AYATI_WORKER_CONTROL_LISTEN", "0.0.0.0:8766"),
        ("AYATI_WORKER_CONTROL_TLS_CERT", "/private/server.pem"),
        ("AYATI_WORKER_CONTROL_TLS_KEY", "/private/server.key"),
    ];
    let config = controller(&https).unwrap();
    assert_eq!(config.worker_listen, Some("0.0.0.0:8766".parse().unwrap()));
    assert!(config.worker_tls.is_some());
    // An https URL without TLS files, TLS on a plain URL, or plain HTTP beyond loopback.
    assert!(controller(&https[..2]).is_err());
    assert!(controller(&[https[2], https[3]]).is_err());
    assert!(controller(&[("AYATI_WORKER_CONTROL_LISTEN", "0.0.0.0:8766")]).is_err());
    assert!(controller(&[("AYATI_WORKER_CONTROL_CA", "/private/ca.pem")]).is_err());

    let executor = |extra: &[(&str, &str)]| {
        let pairs: Vec<_> = [
            ("AYATI_MODEL", "fixture"),
            ("AYATI_WORKER_CONTROL_URL", "https://worker.internal:8766"),
            (
                "AYATI_WORKER_CONTROL_KEY_FILE",
                "/private/worker-control.key",
            ),
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .collect();
        Executor::read(&mut Reader::from_pairs(&pairs))
    };
    assert!(executor(&[]).is_err(), "https needs the private authority");
    assert!(executor(&[("AYATI_WORKER_CONTROL_CA", "/private/ca.pem")]).is_ok());
    let error = executor(&[
        ("AYATI_WORKER_CONTROL_CA", "/private/ca.pem"),
        ("AYATI_WORKER_CONTROL_TLS_KEY", "/private/server.key"),
    ])
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("AYATI_WORKER_CONTROL_TLS_KEY"), "{error}");
}

#[test]
fn aws_identity_needs_no_key_and_the_test_endpoint_stays_on_loopback() {
    let aws = [
        ("AYATI_MODEL", "fixture"),
        ("AYATI_WORKER_CONTROL_URL", "https://worker.internal:8766"),
        ("AYATI_WORKER_CONTROL_CA", "/private/ca.pem"),
        ("AYATI_EXECUTOR_IDENTITY", "aws"),
    ];
    let executor = Executor::read(&mut Reader::from_pairs(&aws)).unwrap();
    assert_eq!(executor.identity, crate::config::Identity::Aws);
    let with_key: Vec<_> = aws
        .iter()
        .copied()
        .chain([(
            "AYATI_WORKER_CONTROL_KEY_FILE",
            "/private/worker-control.key",
        )])
        .collect();
    assert!(Executor::read(&mut Reader::from_pairs(&with_key)).is_err());
    let cloud = [
        ("AYATI_CLOUD_WORKERS", "1"),
        (
            "AYATI_AWS_EXECUTOR_CLUSTER_ARN",
            "arn:aws:ecs:ap-south-1:123456789012:cluster/ayati-test",
        ),
        (
            "AYATI_AWS_EXECUTOR_RELEASE_ARN",
            "arn:aws:ecs:ap-south-1:123456789012:task-definition/ayati-test:1",
        ),
        (
            "AYATI_AWS_EXECUTOR_ROLE_ARN",
            "arn:aws:iam::123456789012:role/ayati-test-executor",
        ),
    ];
    let with = |endpoint: &str| {
        let pairs: Vec<_> = cloud
            .iter()
            .copied()
            .chain([("AYATI_AWS_TEST_ENDPOINT", endpoint)])
            .collect();
        controller(&pairs)
    };
    assert!(with("http://127.0.0.1:9000").is_ok());
    assert!(with("https://sts.amazonaws.com").is_err());
    assert!(controller(&[("AYATI_AWS_TEST_ENDPOINT", "http://127.0.0.1:9000")]).is_err());
}
