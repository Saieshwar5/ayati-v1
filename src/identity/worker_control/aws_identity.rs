//! Executors on AWS prove their ECS task identity when they register. The
//! executor signs an STS GetCallerIdentity request with its task-role
//! credentials, covering a digest of its registration, and hands the signed
//! headers to the controller without sending them. The controller sends that
//! exact request to an STS endpoint it chooses, so AWS itself names the caller,
//! then confirms the task is running the expected release and role through ECS.
use super::{cloud::CloudRegistration, model::Registration};
use crate::{config::CloudWorkers, identity::provider::digest};
use anyhow::{Context, Result, bail, ensure};
use aws_credential_types::{Credentials, provider::ProvideCredentials};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

pub(crate) const REGION: &str = "ap-south-1";
const STS_HOST: &str = "sts.ap-south-1.amazonaws.com";
const ECS_HOST: &str = "ecs.ap-south-1.amazonaws.com";
const STS_BODY: &str = "Action=GetCallerIdentity&Version=2011-06-15";
const FORM: &str = "application/x-www-form-urlencoded; charset=utf-8";
/// Signed header binding the proof to one exact registration. STS rejects a
/// signature older than 15 minutes; a replay inside that window only repeats the
/// identical registration, which is idempotent.
const BINDING: &str = "x-ayati-registration";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Proof {
    pub headers: BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AwsRegistration {
    pub registration: Registration,
    pub proof: Proof,
}

fn binding(registration: &Registration) -> Result<String> {
    Ok(digest(&serde_json::to_string(registration)?))
}

/// The executor's task-role credentials from the standard AWS provider chain
/// (the ECS container credential endpoint on Fargate).
pub(crate) async fn credentials() -> Result<Credentials> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(REGION))
        .load()
        .await;
    config
        .credentials_provider()
        .context("No AWS credentials are available to this executor")?
        .provide_credentials()
        .await
        .context("Could not load this executor's AWS task credentials")
}

/// SigV4-sign a POST with the official signer; returns the headers to send.
fn sign(
    credentials: &Credentials,
    service: &str,
    url: &str,
    mut headers: BTreeMap<String, String>,
    body: &[u8],
) -> Result<BTreeMap<String, String>> {
    use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
    let identity: aws_smithy_runtime_api::client::identity::Identity = credentials.clone().into();
    let params = aws_sigv4::sign::v4::SigningParams::builder()
        .identity(&identity)
        .region(REGION)
        .name(service)
        .time(std::time::SystemTime::now())
        .settings(SigningSettings::default())
        .build()?
        .into();
    let request = SignableRequest::new(
        "POST",
        url,
        headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        SignableBody::Bytes(body),
    )?;
    let (instructions, _) = sign(request, &params)?.into_parts();
    for (name, value) in instructions.headers() {
        headers.insert(name.to_ascii_lowercase(), value.to_owned());
    }
    Ok(headers)
}

/// Sign, but do not send, a GetCallerIdentity request bound to `registration`.
pub(crate) fn prove(registration: &Registration, credentials: &Credentials) -> Result<Proof> {
    let headers = BTreeMap::from([
        ("host".to_owned(), STS_HOST.to_owned()),
        ("content-type".to_owned(), FORM.to_owned()),
        (BINDING.to_owned(), binding(registration)?),
    ]);
    Ok(Proof {
        headers: sign(
            credentials,
            "sts",
            &format!("https://{STS_HOST}/"),
            headers,
            STS_BODY.as_bytes(),
        )?,
    })
}

/// The controller's verifier: its own STS and ECS destinations, and its own
/// credentials (task role) for the two read-only ECS calls.
pub(crate) struct Verifier {
    cloud: CloudWorkers,
    sts: String,
    ecs: String,
    http: reqwest::Client,
    credentials: aws_credential_types::provider::SharedCredentialsProvider,
}

impl Verifier {
    /// `endpoint` replaces both AWS endpoints with one loopback fake in tests.
    pub(crate) async fn new(
        cloud: &CloudWorkers,
        endpoint: Option<&str>,
        credentials: Option<Credentials>,
    ) -> Result<Self> {
        let credentials = match credentials {
            Some(credentials) => {
                aws_credential_types::provider::SharedCredentialsProvider::new(credentials)
            }
            None => aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(REGION))
                .load()
                .await
                .credentials_provider()
                .context("The controller has no AWS credentials for ECS")?,
        };
        let fixed = |host: &str| format!("https://{host}/");
        Ok(Self {
            cloud: cloud.clone(),
            sts: endpoint.map_or_else(|| fixed(STS_HOST), |e| format!("{e}/")),
            ecs: endpoint.map_or_else(|| fixed(ECS_HOST), |e| format!("{e}/")),
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(5))
                .build()?,
            credentials,
        })
    }

    /// AWS names the caller; the result is a cloud registration bound to that task.
    pub(crate) async fn verify(&self, input: &AwsRegistration) -> Result<CloudRegistration> {
        input.registration.validate()?;
        let headers = &input.proof.headers;
        check_headers(headers, &binding(&input.registration)?)?;
        let mut request = self.http.post(&self.sts).body(STS_BODY);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let response = request.send().await.context("STS unavailable")?;
        ensure!(
            response.status().is_success(),
            "AWS rejected the identity proof"
        );
        let principal = caller_arn(&response.text().await?)?;
        let task_arn = task_arn(&self.cloud, &principal)?;
        let registration = CloudRegistration {
            registration: input.registration.clone(),
            task_arn,
            principal_arn: principal,
            release_arn: self.cloud.release_arn.clone(),
        };
        registration.validate(Some(&self.cloud))?;
        self.running(&registration.task_arn).await?;
        Ok(registration)
    }

    /// The task is running the configured release with the configured role.
    async fn running(&self, task_arn: &str) -> Result<()> {
        let cloud = &self.cloud;
        let described = self
            .ecs(
                "DescribeTasks",
                json!({"cluster":cloud.cluster_arn,"tasks":[task_arn]}),
            )
            .await?;
        let tasks = described["tasks"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        ensure!(
            described["failures"].as_array().is_none_or(Vec::is_empty),
            "Task identity unavailable"
        );
        let [task] = tasks else {
            bail!("Task identity unavailable")
        };
        ensure!(
            task["taskArn"] == task_arn
                && task["clusterArn"] == cloud.cluster_arn.as_str()
                && task["lastStatus"] == "RUNNING"
                && task["taskDefinitionArn"] == cloud.release_arn.as_str(),
            "Wrong task, state or release"
        );
        let definition = self
            .ecs(
                "DescribeTaskDefinition",
                json!({"taskDefinition":cloud.release_arn}),
            )
            .await?;
        ensure!(
            definition["taskDefinition"]["taskRoleArn"] == cloud.role_arn.as_str(),
            "Wrong packaged task role"
        );
        Ok(())
    }

    /// One read-only ECS JSON 1.1 call signed with the controller's credentials.
    async fn ecs(&self, operation: &str, body: Value) -> Result<Value> {
        let credentials = self
            .credentials
            .provide_credentials()
            .await
            .context("Controller AWS credentials unavailable")?;
        let url = reqwest::Url::parse(&self.ecs)?;
        let host = match url.port() {
            Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
            None => url.host_str().unwrap_or_default().to_owned(),
        };
        let body = body.to_string();
        let headers = sign(
            &credentials,
            "ecs",
            &self.ecs,
            BTreeMap::from([
                ("host".to_owned(), host),
                (
                    "content-type".to_owned(),
                    "application/x-amz-json-1.1".to_owned(),
                ),
                (
                    "x-amz-target".to_owned(),
                    format!("AmazonEC2ContainerServiceV20141113.{operation}"),
                ),
            ]),
            body.as_bytes(),
        )?;
        let mut request = self.http.post(&self.ecs).body(body);
        for (name, value) in headers.iter().filter(|(name, _)| *name != "host") {
            request = request.header(name, value);
        }
        let response = request.send().await.context("ECS unavailable")?;
        ensure!(
            response.status().is_success(),
            "ECS refused the task lookup"
        );
        Ok(response.json().await?)
    }
}

/// Exactly the signed request shape `prove` produces, for STS, bound to this
/// registration and signed recently.
fn check_headers(headers: &BTreeMap<String, String>, expected_binding: &str) -> Result<()> {
    let allowed = [
        "authorization",
        "content-type",
        "host",
        "x-amz-date",
        "x-amz-security-token",
        BINDING,
    ];
    ensure!(
        headers.keys().all(|name| allowed.contains(&name.as_str()))
            && headers.get("host").map(String::as_str) == Some(STS_HOST)
            && headers.get("content-type").map(String::as_str) == Some(FORM)
            && headers.get(BINDING).map(String::as_str) == Some(expected_binding),
        "Identity proof does not match this registration"
    );
    let authorization = headers
        .get("authorization")
        .context("Unsigned identity proof")?;
    let scope = format!("/{REGION}/sts/aws4_request,");
    let signed = authorization
        .split_once("SignedHeaders=")
        .and_then(|(_, rest)| rest.split(',').next())
        .unwrap_or_default();
    ensure!(
        authorization.starts_with("AWS4-HMAC-SHA256 Credential=")
            && authorization.contains(&scope)
            && signed.split(';').any(|name| name == "host")
            && signed.split(';').any(|name| name == BINDING),
        "Identity proof must sign the STS host and this registration"
    );
    ensure!(headers.contains_key("x-amz-date"), "Undated identity proof");
    Ok(())
}

/// The single `<Arn>` in a GetCallerIdentity response.
fn caller_arn(xml: &str) -> Result<String> {
    let mut parts = xml.split("<Arn>");
    let (Some(_), Some(rest), None) = (parts.next(), parts.next(), parts.next()) else {
        bail!("Unexpected STS response")
    };
    let arn = rest.split("</Arn>").next().unwrap_or_default();
    ensure!(
        arn.len() <= 256 && !arn.contains('<'),
        "Unexpected STS response"
    );
    Ok(arn.to_owned())
}

/// `arn:aws:sts::<account>:assumed-role/<role>/<task id>` → the task's ECS ARN.
/// ECS names task-role sessions after the task (confirm in the first AWS window);
/// any other session name fails closed in `CloudRegistration::validate`.
fn task_arn(cloud: &CloudWorkers, principal: &str) -> Result<String> {
    let id = principal.rsplit('/').next().unwrap_or_default();
    let (base, name) = cloud
        .cluster_arn
        .rsplit_once(":cluster/")
        .context("Invalid cloud cluster")?;
    Ok(format!("{base}:task/{name}/{id}"))
}
