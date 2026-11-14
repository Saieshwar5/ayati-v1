//! SigV4 signing for Cuckoo's few direct AWS calls (STS and ECS for executor
//! identity, SES and SQS for agent email) with the official signer, so no
//! large per-service SDK crates are needed.
use anyhow::Result;
use aws_credential_types::Credentials;
use std::collections::BTreeMap;

/// Signs one request; returns `headers` with the signature headers added.
pub(crate) fn sign(
    credentials: &Credentials,
    region: &str,
    service: &str,
    method: &str,
    url: &str,
    mut headers: BTreeMap<String, String>,
    body: &[u8],
) -> Result<BTreeMap<String, String>> {
    use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
    let identity: aws_smithy_runtime_api::client::identity::Identity = credentials.clone().into();
    let params = aws_sigv4::sign::v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name(service)
        .time(std::time::SystemTime::now())
        .settings(SigningSettings::default())
        .build()?
        .into();
    let request = SignableRequest::new(
        method,
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
