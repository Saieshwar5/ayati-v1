use super::tests::fixture;
use crate::identity::{
    provider::{digest, random_token},
    worker_control::{cloud::CloudRegistration, model::Registration},
};

fn registration(task: &str) -> CloudRegistration {
    CloudRegistration {
        registration: Registration {
            executor: uuid::Uuid::new_v4().to_string(),
            incarnation: uuid::Uuid::new_v4().to_string(),
            token: random_token(),
            pooled: true,
        },
        task_arn: format!("arn:aws:ecs:ap-south-1:123456789012:task/ayati-test/{task}"),
        principal_arn: format!("arn:aws:sts::123456789012:assumed-role/ayati-test-executor/{task}"),
        release_arn: "arn:aws:ecs:ap-south-1:123456789012:task-definition/ayati-test:1".into(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn cloud_task_binding_denies_other_tasks_and_legacy_rebinding() {
    let db = fixture().await;
    let first = registration(&"a".repeat(32));
    let second = registration(&"b".repeat(32));
    db.register_cloud_executor(&first).await.unwrap();
    db.register_cloud_executor(&second).await.unwrap();
    db.register_cloud_executor(&first).await.unwrap();
    let hash = digest(&first.registration.token);
    db.cloud_credential_allowed(&hash, Some(&first.task_arn))
        .await
        .unwrap();
    assert!(
        db.cloud_credential_allowed(&hash, Some(&second.task_arn))
            .await
            .is_err()
    );
    assert!(db.cloud_credential_allowed(&hash, None).await.is_err());
    assert!(db.register_executor(&first.registration).await.is_err());
    let mut cloned_token = registration(&"d".repeat(32));
    cloned_token.registration.token = first.registration.token.clone();
    let denied = db.register_cloud_executor(&cloned_token).await.unwrap_err();
    assert!(denied.is::<super::worker_control::ControlDenied>());
    let mut stolen = registration(&"a".repeat(32));
    assert!(db.register_cloud_executor(&stolen).await.is_err());
    stolen.registration = first.registration.clone();
    stolen.task_arn = second.task_arn.clone();
    assert!(db.register_cloud_executor(&stolen).await.is_err());
    let local = registration(&"c".repeat(32));
    db.register_executor(&local.registration).await.unwrap();
    db.cloud_credential_allowed(&digest(&local.registration.token), None)
        .await
        .unwrap();
    assert!(
        db.cloud_credential_allowed(&digest(&local.registration.token), Some(&first.task_arn))
            .await
            .is_err()
    );
    assert!(db.register_cloud_executor(&local).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn concurrent_registration_cannot_change_the_authentication_boundary() {
    let db = fixture().await;
    for id in 0..8 {
        let input = registration(&format!("{id:032x}"));
        let (local, cloud) = tokio::join!(
            db.register_executor(&input.registration),
            db.register_cloud_executor(&input),
        );
        assert_ne!(local.is_ok(), cloud.is_ok());
        let hash = digest(&input.registration.token);
        assert_eq!(
            db.cloud_credential_allowed(&hash, None).await.is_ok(),
            local.is_ok()
        );
        assert_eq!(
            db.cloud_credential_allowed(&hash, Some(&input.task_arn))
                .await
                .is_ok(),
            cloud.is_ok()
        );
    }
}
