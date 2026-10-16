use ayati::workspace::registry::{AgentProfile, NewAgent, Registry};

#[test]
fn registry_preserves_ready_roles_and_marks_interrupted_preparation_failed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.sqlite");
    let registry = Registry::open(&path).unwrap();
    let mut ready = AgentProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Operations".into(),
        role: "Report progress".into(),
        model: "test".into(),
        memory: true,
        browser: false,
        state: "ready".into(),
        detail: String::new(),
        attempt: "0123456789ab".into(),
    };
    registry.save(&ready).unwrap();
    let ready_id = ready.id.clone();
    ready.id = uuid::Uuid::new_v4().to_string();
    ready.state = "preparing".into();
    registry.save(&ready).unwrap();
    drop(registry);
    let restored = Registry::open(&path).unwrap().list().unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].id, ready_id);
    assert_eq!(restored[0].state, "ready");
    assert_eq!(restored[0].role, "Report progress");
    assert_eq!(restored[1].state, "failed");
    assert!(restored[1].detail.contains("interrupted"));
}

#[test]
fn setup_validates_receipt_identity_and_model_boundaries() {
    let mut request = NewAgent {
        request_id: uuid::Uuid::new_v4().to_string(),
        name: "Reports".into(),
        role: "Summarize\nexpenses".into(),
        model: "provider/model".into(),
        memory: false,
        browser: false,
    };
    assert!(request.validate().is_ok());
    request.model = "model\n--args".into();
    assert!(request.validate().is_err());
    request.model = "model".into();
    request.request_id = "../../other".into();
    assert!(request.validate().is_err());
    request.request_id = uuid::Uuid::new_v4().to_string();
    request.role = " ".into();
    assert!(request.validate().is_err());
}
