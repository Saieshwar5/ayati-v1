//! AWS identity proofs against a loopback fake. The fake STS recomputes the
//! SigV4 signature from the synthetic secret, as STS would, so any change to a
//! signed header is refused; the fake ECS answers from the test's state.
use super::{
    aws_identity::{AwsRegistration, Verifier, prove},
    model::Registration,
};
use crate::{config::CloudWorkers, identity::provider::random_token};
use aws_credential_types::Credentials;
use axum::{Router, body::Bytes, extract::State, http::HeaderMap, routing::post};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const ACCOUNT: &str = "123456789012";
const TASK: &str = "0123456789abcdef0123456789abcdef";

fn cloud() -> CloudWorkers {
    CloudWorkers {
        cluster_arn: format!("arn:aws:ecs:ap-south-1:{ACCOUNT}:cluster/ayati-test"),
        release_arn: format!("arn:aws:ecs:ap-south-1:{ACCOUNT}:task-definition/ayati-test:1"),
        role_arn: format!("arn:aws:iam::{ACCOUNT}:role/ayati-test-executor"),
    }
}
fn task_arn() -> String {
    format!("arn:aws:ecs:ap-south-1:{ACCOUNT}:task/ayati-test/{TASK}")
}
fn credentials() -> Credentials {
    Credentials::new(
        "AKIDTESTEXECUTOR",
        "synthetic-secret",
        Some("synthetic-session".into()),
        None,
        "test",
    )
}
fn registration() -> Registration {
    Registration {
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
        pooled: true,
    }
}

/// What the fake AWS currently reports.
struct Aws {
    session: String,
    status: &'static str,
    release: String,
    role: String,
}

/// `YYYYMMDDTHHMMSSZ` to a time, so the fake can re-sign at the signer's instant.
fn amz_time(value: &str) -> std::time::SystemTime {
    let n = |range: std::ops::Range<usize>| value[range].parse::<i64>().unwrap();
    let (y, m, d) = (n(0..4), n(4..6), n(6..8));
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    let seconds = days * 86_400 + n(9..11) * 3600 + n(11..13) * 60 + n(13..15);
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds as u64)
}

/// Recompute the signature over the received request with the known secret.
fn signature_valid(headers: &HeaderMap, body: &[u8]) -> bool {
    use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
    };
    let identity: aws_smithy_runtime_api::client::identity::Identity = credentials().into();
    let params = aws_sigv4::sign::v4::SigningParams::builder()
        .identity(&identity)
        .region("ap-south-1")
        .name("sts")
        .time(amz_time(get("x-amz-date")))
        .settings(SigningSettings::default())
        .build()
        .unwrap()
        .into();
    let signed: Vec<(&str, &str)> = ["host", "content-type", "x-ayati-registration"]
        .into_iter()
        .map(|name| (name, get(name)))
        .collect();
    let Ok(request) = SignableRequest::new(
        "POST",
        format!("https://{}/", get("host")),
        signed.into_iter(),
        SignableBody::Bytes(body),
    ) else {
        return false;
    };
    let Ok(output) = sign(request, &params) else {
        return false;
    };
    let (instructions, _) = output.into_parts();
    instructions.headers().any(|(name, value)| {
        name.eq_ignore_ascii_case("authorization") && value == get("authorization")
    })
}

async fn fake(
    State(aws): State<Arc<Mutex<Aws>>>,
    headers: HeaderMap,
    body: Bytes,
) -> (axum::http::StatusCode, String) {
    let aws = aws.lock().unwrap();
    let target = headers
        .get("x-amz-target")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let cloud = cloud();
    let reply = |value: Value| (axum::http::StatusCode::OK, value.to_string());
    match target.rsplit('.').next().unwrap_or_default() {
        "DescribeTasks" => reply(
            json!({"failures":[],"tasks":[{"taskArn":task_arn(),"clusterArn":cloud.cluster_arn,"lastStatus":aws.status,"taskDefinitionArn":aws.release}]}),
        ),
        "DescribeTaskDefinition" => reply(json!({"taskDefinition":{"taskRoleArn":aws.role}})),
        _ if signature_valid(&headers, &body) => (
            axum::http::StatusCode::OK,
            format!(
                "<GetCallerIdentityResponse><GetCallerIdentityResult><Arn>arn:aws:sts::{ACCOUNT}:assumed-role/ayati-test-executor/{}</Arn></GetCallerIdentityResult></GetCallerIdentityResponse>",
                aws.session
            ),
        ),
        _ => (
            axum::http::StatusCode::FORBIDDEN,
            "<Error>SignatureDoesNotMatch</Error>".into(),
        ),
    }
}

async fn verifier() -> (Verifier, Arc<Mutex<Aws>>, tokio::task::JoinHandle<()>) {
    let cloud = cloud();
    let aws = Arc::new(Mutex::new(Aws {
        session: TASK.into(),
        status: "RUNNING",
        release: cloud.release_arn.clone(),
        role: cloud.role_arn.clone(),
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/", post(fake)).with_state(aws.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let verifier = Verifier::new(&cloud, Some(&endpoint), Some(credentials()))
        .await
        .unwrap();
    (verifier, aws, server)
}

#[tokio::test]
async fn aws_names_the_executor_and_ecs_confirms_its_running_release_and_role() {
    let (verifier, aws, server) = verifier().await;
    let registration = registration();
    let input = AwsRegistration {
        proof: prove(&registration, &credentials()).unwrap(),
        registration: registration.clone(),
    };
    let bound = verifier.verify(&input).await.unwrap();
    assert_eq!(bound.task_arn, task_arn());
    assert_eq!(bound.release_arn, cloud().release_arn);
    assert!(bound.principal_arn.ends_with(TASK));

    for change in [
        |a: &mut Aws| a.status = "STOPPED",
        |a: &mut Aws| {
            a.release = format!("arn:aws:ecs:ap-south-1:{ACCOUNT}:task-definition/ayati-test:2")
        },
        |a: &mut Aws| a.role = format!("arn:aws:iam::{ACCOUNT}:role/ayati-other"),
        |a: &mut Aws| a.session = "chosen-session-name".into(),
    ] {
        // One guard: several locks in one expression would deadlock.
        let saved = {
            let a = aws.lock().unwrap();
            (
                a.status,
                a.release.clone(),
                a.role.clone(),
                a.session.clone(),
            )
        };
        change(&mut aws.lock().unwrap());
        assert!(verifier.verify(&input).await.is_err());
        *aws.lock().unwrap() = Aws {
            status: saved.0,
            release: saved.1,
            role: saved.2,
            session: saved.3,
        };
    }
    assert!(verifier.verify(&input).await.is_ok());
    server.abort();
}

#[tokio::test]
async fn a_proof_works_only_for_its_own_registration_and_exact_signed_request() {
    let (verifier, _, server) = verifier().await;
    let registration = registration();
    let proof = prove(&registration, &credentials()).unwrap();
    // Reusing a proof for another executor or token fails the binding check.
    let other = AwsRegistration {
        registration: Registration {
            token: random_token(),
            ..registration.clone()
        },
        proof: prove(&registration, &credentials()).unwrap(),
    };
    assert!(verifier.verify(&other).await.is_err());
    // Rewriting the binding to match breaks the signature, so STS refuses it.
    let mut forged = prove(&registration, &credentials()).unwrap();
    let target = Registration {
        executor: uuid::Uuid::new_v4().to_string(),
        ..registration.clone()
    };
    forged.headers.insert(
        "x-ayati-registration".into(),
        crate::identity::provider::digest(&serde_json::to_string(&target).unwrap()),
    );
    assert!(
        verifier
            .verify(&AwsRegistration {
                registration: target,
                proof: forged
            })
            .await
            .is_err()
    );
    // Another host, an extra header or a missing signature is refused before STS.
    for edit in [
        |p: &mut std::collections::BTreeMap<String, String>| {
            p.insert("host".into(), "sts.amazonaws.com".into());
        },
        |p: &mut std::collections::BTreeMap<String, String>| {
            p.insert("x-forwarded-for".into(), "1.2.3.4".into());
        },
        |p: &mut std::collections::BTreeMap<String, String>| {
            p.remove("authorization");
        },
    ] {
        let mut headers = proof.headers.clone();
        edit(&mut headers);
        let input = AwsRegistration {
            registration: registration.clone(),
            proof: super::aws_identity::Proof { headers },
        };
        assert!(verifier.verify(&input).await.is_err());
    }
    let wrong_secret = Credentials::new(
        "AKIDTESTEXECUTOR",
        "not-the-secret",
        Some("synthetic-session".into()),
        None,
        "test",
    );
    let input = AwsRegistration {
        proof: prove(&registration, &wrong_secret).unwrap(),
        registration: registration.clone(),
    };
    assert!(verifier.verify(&input).await.is_err());
    assert!(
        verifier
            .verify(&AwsRegistration {
                proof,
                registration
            })
            .await
            .is_ok()
    );
    server.abort();
}
