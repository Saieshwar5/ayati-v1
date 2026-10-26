use super::*;
use serde_json::json;
fn source() -> catalog::Source {
    catalog::Source {
        id: "11111111-1111-4111-8111-111111111111".into(),
        workspace: "22222222-2222-4222-8222-222222222222".into(),
        connection: "33333333-3333-4333-8333-333333333333".into(),
        connection_fingerprint: "a".repeat(43),
        name: "Reviewed order source".into(),
        provider: "composio_v3".into(),
        user_id: "person".into(),
        connected_account_id: "account".into(),
        auth_config_id: "auth".into(),
        trigger_id: "trigger".into(),
        trigger_slug: "ORDER_CHANGED".into(),
        read_operation: "order.read".into(),
        fields: vec!["order_id".into()],
        secret_file: "/private/source.secret".into(),
    }
}
fn event() -> serde_json::Value {
    json!({"id":"event-1","type":"composio.trigger.message","metadata":{"user_id":"person","connected_account_id":"account","auth_config_id":"auth","trigger_id":"trigger","trigger_slug":"ORDER_CHANGED"},"data":{"order_id":"one","private_extra":"NEVER_COPY"}})
}
#[test]
fn event_projection_requires_exact_identity_and_copies_only_reviewed_bounded_scalars() {
    let s = source();
    s.validate().unwrap();
    assert_eq!(
        s.project("event-1", &event()).unwrap(),
        json!({"order_id":"one"})
    );
    for field in [
        "user_id",
        "connected_account_id",
        "auth_config_id",
        "trigger_id",
        "trigger_slug",
    ] {
        let mut e = event();
        e["metadata"][field] = json!("forged");
        assert!(s.project("event-1", &e).is_err());
    }
    assert!(s.project("other-id", &event()).is_err());
    for invalid in [
        json!({"nested":true}),
        json!(["one"]),
        json!("x".repeat(2001)),
        json!(9007199254740992u64),
    ] {
        let mut e = event();
        e["data"]["order_id"] = invalid;
        assert!(s.project("event-1", &e).is_err());
    }
    for valid in [json!(null), json!(true), json!(123), json!("one")] {
        let mut e = event();
        e["data"]["order_id"] = valid;
        assert!(s.project("event-1", &e).is_ok());
    }
}
#[test]
fn event_source_binding_rotates_semantics_but_not_key_paths_and_exact_filters_keep_types() {
    let s = source();
    let mut changed = s.clone();
    changed.secret_file = "/rotated/key".into();
    assert_eq!(s.fingerprint().unwrap(), changed.fingerprint().unwrap());
    changed.fields.push("status".into());
    assert_ne!(s.fingerprint().unwrap(), changed.fingerprint().unwrap());
    assert!(model::matches(
        &json!({"status":"delayed"}),
        &json!({"status":"delayed","other":true})
    ));
    assert!(!model::matches(
        &json!({"status":"1"}),
        &json!({"status":1})
    ));
    assert!(!s.public().to_string().contains("private"));
}
#[test]
fn event_csrf_exemption_is_only_the_exact_signed_uuid_hook_route() {
    assert!(routes::webhook_path(
        "/hooks/composio/workspaces/22222222-2222-4222-8222-222222222222/11111111-1111-4111-8111-111111111111"
    ));
    for p in [
        "/hooks/composio/workspaces/invalid/source",
        "/hooks/composio/workspaces",
        "/api/identity/workspaces/22222222-2222-4222-8222-222222222222/11111111-1111-4111-8111-111111111111",
        "/hooks/composio/workspaces/22222222-2222-4222-8222-222222222222/11111111-1111-4111-8111-111111111111/state",
    ] {
        assert!(!routes::webhook_path(p));
    }
}
