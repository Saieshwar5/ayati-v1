//! The email transport (`email`, server `ses`). Incoming mail arrives on the
//! SQS queue (`queue`), so `fetch` reads nothing; replies go out through SES
//! v2 `SendEmail` as raw MIME from the agent's address, tagged with the
//! workspace and attempt so SES's delivery events name the reply they are about.
use super::super::adapters::{Batch, Outgoing, Route, Sent, Server, Source, TransportAdapter};
use super::compose::{Reply, compose};
use super::settings::{self, SERVER, Settings};
use anyhow::{Context, Result, ensure};
use aws_credential_types::{Credentials, provider::ProvideCredentials};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use tokio::sync::OnceCell;

const REASON_CHARS: usize = 300;

pub(crate) struct EmailTransport {
    pub(super) client: reqwest::Client,
    credentials: OnceCell<aws_credential_types::provider::SharedCredentialsProvider>,
    /// Tests read their own file; the controller reads the configured one.
    file: Option<std::path::PathBuf>,
}
impl EmailTransport {
    pub(crate) fn new() -> Self {
        Self {
            file: None,
            credentials: OnceCell::new(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("HTTP client"),
        }
    }
    #[cfg(test)]
    pub(crate) fn with_file(file: std::path::PathBuf) -> Self {
        Self {
            file: Some(file),
            ..Self::new()
        }
    }
    pub(crate) fn settings(&self) -> Result<Option<Settings>> {
        match &self.file {
            Some(file) => settings::read(file).map(Some),
            None => settings::current(),
        }
    }
    /// The controller's AWS role (the standard provider chain). Loopback test
    /// servers get fixed dummy credentials.
    pub(super) async fn credentials(&self, s: &Settings) -> Result<Credentials> {
        if s.api.is_some() {
            return Ok(Credentials::new(
                "AKIDTEST",
                "test-secret",
                None,
                None,
                "test",
            ));
        }
        let provider = self
            .credentials
            .get_or_try_init(|| async {
                aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(aws_config::Region::new(s.region.clone()))
                    .load()
                    .await
                    .credentials_provider()
                    .context("The controller has no AWS credentials for email")
            })
            .await?;
        provider
            .provide_credentials()
            .await
            .context("The controller's AWS credentials are unavailable")
    }
    /// One signed JSON request to an AWS API; the response status and body.
    pub(super) async fn call(
        &self,
        s: &Settings,
        service: &str,
        url: &str,
        mut headers: BTreeMap<String, String>,
        body: String,
    ) -> Result<std::result::Result<(u16, Vec<u8>), reqwest::Error>> {
        let credentials = self.credentials(s).await?;
        let parsed = reqwest::Url::parse(url)?;
        let host = match parsed.port() {
            Some(port) => format!("{}:{port}", parsed.host_str().unwrap_or_default()),
            None => parsed.host_str().unwrap_or_default().to_owned(),
        };
        headers.insert("host".into(), host);
        let signed = crate::identity::aws_signing::sign(
            &credentials,
            &s.region,
            service,
            "POST",
            url,
            headers,
            body.as_bytes(),
        )?;
        let mut request = self.client.post(url).body(body);
        for (name, value) in signed.iter().filter(|(name, _)| *name != "host") {
            request = request.header(name, value);
        }
        Ok(match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                Ok((status, response.bytes().await.unwrap_or_default().to_vec()))
            }
            Err(e) => Err(e),
        })
    }
}

fn reason(text: String) -> String {
    text.chars().take(REASON_CHARS).collect()
}

/// What SES's answer to `SendEmail` means for the reply.
fn outcome(answer: std::result::Result<(u16, Vec<u8>), reqwest::Error>) -> Sent {
    let (status, body) = match answer {
        Ok(answer) => answer,
        // A refused connection never reached SES; anything later might have.
        Err(e) if e.is_connect() => return Sent::Unreached("SES could not be reached".into()),
        Err(_) => return Sent::Uncertain("SES did not answer in time".into()),
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    match status {
        200..=299 => match body["MessageId"].as_str() {
            Some(id) => Sent::Accepted(Some(reason(id.to_owned()))),
            None => Sent::Uncertain("SES's answer could not be read".into()),
        },
        // Signing or permission problems: SES sent nothing, and the operator
        // can fix the role, so the reply waits instead of failing.
        403 => Sent::Unreached(reason(format!(
            "SES refused Cuckoo's credentials: {}",
            body["message"].as_str().unwrap_or("access denied")
        ))),
        408 | 429 => Sent::Uncertain(format!("SES answered HTTP {status}")),
        400..=499 => Sent::Refused(reason(format!(
            "SES refused it: {}",
            body["message"].as_str().unwrap_or("bad request")
        ))),
        // 5xx: SES may or may not have taken it. Never resent.
        _ => Sent::Uncertain(format!("SES answered HTTP {status}")),
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[async_trait::async_trait]
impl TransportAdapter for EmailTransport {
    fn name(&self) -> &'static str {
        "email"
    }
    fn servers(&self, _workspace: &str) -> Result<Vec<Server>> {
        Ok(match self.settings()? {
            Some(_) => vec![Server {
                id: SERVER.into(),
                name: "Email".into(),
            }],
            None => vec![],
        })
    }
    async fn check(&self, route: &Route) -> Result<()> {
        let s = self
            .settings()?
            .ok_or_else(|| anyhow::anyhow!("Email is not offered"))?;
        ensure!(
            route.server == SERVER && s.serves(&route.address),
            "This address is not on Cuckoo's mail domain"
        );
        Ok(())
    }
    /// Incoming mail arrives on the queue; there is nothing to read here.
    async fn fetch(&self, _source: &Source, _cursor: Option<&str>) -> Result<Batch> {
        Ok(Batch {
            events: vec![],
            next: None,
            skipped: 0,
        })
    }
    async fn send(&self, route: &Route, outgoing: &Outgoing) -> Sent {
        let s = match self.settings() {
            Ok(Some(s)) if route.server == SERVER && s.serves(&route.address) => s,
            _ => return Sent::Unreached("Email is not configured on this server".into()),
        };
        let Some(mail) = &outgoing.mail else {
            return Sent::Refused("The reply has no mail to answer".into());
        };
        if !outgoing.to.contains('@')
            || outgoing
                .to
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '<' || c == '>')
        {
            return Sent::Refused("The recipient is not an email address".into());
        }
        let raw = compose(&Reply {
            from_name: &mail.from_name,
            from: &route.address,
            to: &outgoing.to,
            subject: &mail.subject,
            in_reply_to: mail.in_reply_to.as_deref(),
            references: &mail.references,
            key: &outgoing.key,
            domain: &s.domain,
            markdown: &outgoing.text,
            now: now(),
        });
        let body = json!({
            "FromEmailAddress": route.address,
            "Destination": {"ToAddresses": [outgoing.to]},
            "Content": {"Raw": {"Data": STANDARD.encode(raw.as_bytes())}},
            "ConfigurationSetName": s.configuration_set,
            "EmailTags": [
                {"Name": "workspace", "Value": route.workspace},
                {"Name": "attempt", "Value": outgoing.key},
            ],
        });
        let url = format!("{}/v2/email/outbound-emails", s.ses());
        let headers = BTreeMap::from([("content-type".to_owned(), "application/json".to_owned())]);
        match self.call(&s, "ses", &url, headers, body.to_string()).await {
            Ok(answer) => outcome(answer),
            Err(e) => Sent::Unreached(reason(format!("Email could not be signed: {e}"))),
        }
    }
    /// SES can't be asked about a send by Cuckoo's key, and an unclear send is
    /// never resent. A later delivery event still settles it.
    async fn status(&self, _route: &Route, _key: &str) -> Sent {
        Sent::Uncertain("Waiting for SES's delivery event".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ses_answers_map_to_what_became_of_the_reply() {
        let ok = |status: u16, body: &str| outcome(Ok((status, body.as_bytes().to_vec())));
        assert_eq!(
            ok(200, r#"{"MessageId":"m-1"}"#),
            Sent::Accepted(Some("m-1".into()))
        );
        assert!(matches!(ok(200, "{}"), Sent::Uncertain(_)));
        assert!(
            matches!(ok(400, r#"{"message":"Email address is not verified."}"#), Sent::Refused(r) if r.contains("not verified"))
        );
        assert!(matches!(ok(403, "{}"), Sent::Unreached(_)));
        assert!(matches!(ok(429, "{}"), Sent::Uncertain(_)));
        assert!(matches!(ok(503, "{}"), Sent::Uncertain(_)));
    }
}
