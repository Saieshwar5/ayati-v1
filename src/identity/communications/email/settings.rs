//! The operator's email file (`AYATI_EMAIL_FILE`, controller only): the mail
//! domain agents get addresses on and the AWS resources that carry their mail.
//! Amazon SES receives mail for the domain, stores each message in a private S3
//! bucket and notifies an SQS queue; Cuckoo reads that queue and sends replies
//! through SES. Credentials come from the controller's AWS role, never this
//! file. Read on every use, so the domain can change without a restart.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{path::PathBuf, sync::OnceLock};

/// The provider server name saved on email endpoints.
pub(crate) const SERVER: &str = "ses";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    /// The mail domain, for example `mail.cuckoo.onl`. Agents' addresses end in it.
    pub domain: String,
    /// The AWS region of SES, the queue and the bucket, for example `ap-south-1`.
    pub region: String,
    /// The SQS queue SES's notifications arrive on.
    pub queue_url: String,
    /// The private bucket SES stores incoming mail in.
    pub bucket: String,
    /// The SES configuration set whose delivery events reach the same queue.
    pub configuration_set: String,
    /// Only for local tests: loopback SES and SQS stand-ins.
    #[serde(default)]
    pub api: Option<TestApi>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TestApi {
    pub ses: String,
    pub sqs: String,
}

static FILE: OnceLock<Option<PathBuf>> = OnceLock::new();

pub(crate) fn configure(file: Option<PathBuf>) {
    let _ = FILE.set(file);
}

/// The current settings, or `None` when this server offers no agent email.
pub(crate) fn current() -> Result<Option<Settings>> {
    match FILE.get() {
        Some(Some(file)) => read(file).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn read(file: &std::path::Path) -> Result<Settings> {
    let settings: Settings = serde_json::from_str(&crate::identity::private_config::read(file)?)
        .map_err(|_| anyhow::anyhow!("Invalid email configuration"))?;
    settings.validate()?;
    Ok(settings)
}

impl Settings {
    fn validate(&self) -> Result<()> {
        ensure!(valid_domain(&self.domain), "Invalid mail domain");
        ensure!(
            !self.region.is_empty()
                && self.region.len() <= 32
                && self
                    .region
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "Invalid AWS region"
        );
        let queue = reqwest::Url::parse(&self.queue_url).context("Invalid queue URL")?;
        ensure!(
            queue.scheme() == "https" || self.api.is_some(),
            "The queue URL must be HTTPS"
        );
        ensure!(
            (3..=63).contains(&self.bucket.len())
                && self
                    .bucket
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'),
            "Invalid bucket name"
        );
        ensure!(
            (1..=64).contains(&self.configuration_set.len())
                && self
                    .configuration_set
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Invalid configuration set name"
        );
        if let Some(api) = &self.api {
            for url in [&api.ses, &api.sqs] {
                ensure!(
                    url.starts_with("http://127.0.0.1:"),
                    "An email API override must be a loopback test server"
                );
            }
        }
        Ok(())
    }
    /// SES v2's API base.
    pub fn ses(&self) -> String {
        match &self.api {
            Some(api) => api.ses.clone(),
            None => format!("https://email.{}.amazonaws.com", self.region),
        }
    }
    /// SQS's JSON API endpoint.
    pub fn sqs(&self) -> String {
        match &self.api {
            Some(api) => api.sqs.clone(),
            None => format!("https://sqs.{}.amazonaws.com/", self.region),
        }
    }
    /// Whether `address` is on this server's mail domain.
    pub fn serves(&self, address: &str) -> bool {
        address
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain.eq_ignore_ascii_case(&self.domain))
    }
}

/// A lowercase DNS name with at least two labels.
fn valid_domain(value: &str) -> bool {
    value.len() <= 200
        && value.split('.').count() >= 2
        && value.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(json: &str) -> Result<Settings> {
        let settings: Settings = serde_json::from_str(json)?;
        settings.validate()?;
        Ok(settings)
    }

    #[test]
    fn the_operator_file_names_a_domain_and_its_aws_resources() {
        let ok = settings(r#"{"domain":"mail.cuckoo.onl","region":"ap-south-1","queue_url":"https://sqs.ap-south-1.amazonaws.com/1/ayati-mail","bucket":"ayati-mail-1","configuration_set":"ayati-mail"}"#).unwrap();
        assert_eq!(ok.ses(), "https://email.ap-south-1.amazonaws.com");
        assert!(ok.serves("nova.acme@MAIL.cuckoo.onl"));
        assert!(!ok.serves("nova.acme@cuckoo.onl"));
        for bad in [
            r#"{"domain":"Mail.cuckoo.onl","region":"ap-south-1","queue_url":"https://q.example/1","bucket":"b-1","configuration_set":"c"}"#,
            r#"{"domain":"mail.cuckoo.onl","region":"ap-south-1","queue_url":"http://q.example/1","bucket":"b-1","configuration_set":"c"}"#,
            r#"{"domain":"mail.cuckoo.onl","region":"ap-south-1","queue_url":"https://q.example/1","bucket":"b-1","configuration_set":"c","api":{"ses":"http://example.com","sqs":"http://127.0.0.1:1"}}"#,
            r#"{"domain":"mail.cuckoo.onl","region":"ap-south-1","queue_url":"https://q.example/1","bucket":"b-1","configuration_set":"c","token":"x"}"#,
        ] {
            assert!(settings(bad).is_err(), "{bad}");
        }
    }
}
