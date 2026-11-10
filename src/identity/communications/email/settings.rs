//! The operator's email file (`AYATI_EMAIL_FILE`, controller only): the mail
//! domain Cuckoo receives for, the Postmark server token and the credentials
//! Postmark presents when it calls Cuckoo's webhooks. Read on every use, so the
//! token and credentials can be rotated without a restart.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use std::{path::PathBuf, sync::OnceLock};

/// The provider server name saved on email endpoints.
pub(crate) const SERVER: &str = "postmark";
/// Where Postmark posts incoming mail and delivery events.
pub(crate) const INCOMING_PATH: &str = "/hooks/email/postmark";
pub(crate) const EVENTS_PATH: &str = "/hooks/email/postmark/events";
const API: &str = "https://api.postmarkapp.com";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    /// The domain agents' addresses use, for example `agents.cuckoo.app`. Its MX
    /// points at Postmark's inbound servers and it is a verified sending domain.
    pub domain: String,
    /// An owned 0600 file holding the Postmark server API token.
    pub server_token_file: PathBuf,
    /// The Basic credentials written into the webhook URLs in Postmark.
    pub webhook_user: String,
    pub webhook_password_file: PathBuf,
    /// The outbound message stream replies use.
    #[serde(default = "stream")]
    pub message_stream: String,
    /// The name people see next to an agent's address when none is saved.
    #[serde(default = "sender_name")]
    pub sender_name: String,
    /// The Postmark API base. Only for local tests: a loopback address.
    #[serde(default)]
    pub api: Option<String>,
}
fn stream() -> String {
    "outbound".into()
}
fn sender_name() -> String {
    "Cuckoo agent".into()
}

static FILE: OnceLock<Option<PathBuf>> = OnceLock::new();

pub(crate) fn configure(file: Option<PathBuf>) {
    let _ = FILE.set(file);
}

/// The current settings, or `None` when this server offers no email.
pub(crate) fn current() -> Result<Option<Settings>> {
    match FILE.get() {
        Some(Some(file)) => read(file).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn read(file: &std::path::Path) -> Result<Settings> {
    let mut settings: Settings =
        serde_json::from_str(&crate::identity::private_config::read(file)?)
            .map_err(|_| anyhow::anyhow!("Invalid email configuration"))?;
    settings.domain = settings.domain.trim().to_ascii_lowercase();
    settings.validate()?;
    Ok(settings)
}

impl Settings {
    fn validate(&self) -> Result<()> {
        ensure!(valid_domain(&self.domain), "Invalid email domain");
        ensure!(
            self.server_token_file.is_absolute() && self.webhook_password_file.is_absolute(),
            "Use absolute paths for the email secrets"
        );
        ensure!(
            !self.webhook_user.is_empty()
                && self.webhook_user.len() <= 64
                && self
                    .webhook_user
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "Invalid webhook user"
        );
        crate::identity::identifier(&self.message_stream)?;
        ensure!(
            !self.sender_name.trim().is_empty()
                && self.sender_name.len() <= 64
                && !self
                    .sender_name
                    .chars()
                    .any(|c| c.is_control() || "<>\"".contains(c)),
            "Invalid sender name"
        );
        if let Some(api) = &self.api {
            ensure!(
                api.starts_with("http://127.0.0.1:"),
                "A Postmark API override must be a loopback test server"
            );
        }
        Ok(())
    }
    pub fn token(&self) -> Result<String> {
        secret(&self.server_token_file, "Invalid Postmark server token")
    }
    pub fn api(&self) -> &str {
        self.api.as_deref().unwrap_or(API)
    }
    /// Whether `address` is on this server's mail domain.
    pub fn serves(&self, address: &str) -> bool {
        address
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain == self.domain)
    }
    /// Checks the `Authorization` header Postmark sent with a webhook.
    pub fn authorized(&self, header: Option<&str>) -> bool {
        let Some(encoded) = header.and_then(|h| h.strip_prefix("Basic ")) else {
            return false;
        };
        let Ok(password) = secret(&self.webhook_password_file, "Invalid webhook password") else {
            return false;
        };
        let expected = STANDARD.encode(format!("{}:{password}", self.webhook_user));
        // Equal digests: the comparison reveals nothing about the password.
        let digest = crate::identity::provider::digest;
        digest(encoded.trim()) == digest(&expected)
    }
}

fn secret(path: &std::path::Path, problem: &str) -> Result<String> {
    let value = crate::identity::private_config::read(path)?;
    let value = value.trim();
    ensure!(
        value.len() >= 16 && !value.chars().any(char::is_whitespace),
        "{problem}"
    );
    Ok(value.to_owned())
}

/// Lowercase DNS labels, at least two of them.
pub(crate) fn valid_domain(value: &str) -> bool {
    let labels: Vec<&str> = value.split('.').collect();
    value.len() <= 200
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// A plain mailbox address, lowercased: what Cuckoo saves and compares.
/// Quoted local parts and IP-literal domains are not accepted.
pub(crate) fn normal_address(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let (local, domain) = value.rsplit_once('@')?;
    let local_ok = !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+/=?^_`{|}~.-".contains(&b));
    (local_ok && valid_domain(domain)).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_plain_and_lowercased() {
        assert_eq!(
            normal_address(" Priya.K+work@Acme.COM ").as_deref(),
            Some("priya.k+work@acme.com")
        );
        for bad in [
            "priya",
            "@acme.com",
            "priya@acme",
            "\"p r\"@acme.com",
            "priya@[10.0.0.1]",
            "priya..k@acme.com",
            "priya@-acme.com",
            "priya@ac\nme.com",
        ] {
            assert_eq!(normal_address(bad), None, "{bad}");
        }
    }

    #[test]
    fn webhooks_need_postmarks_exact_credentials() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let private = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            path
        };
        let file = private(
            "email.json",
            &serde_json::json!({"domain": "Agents.Cuckoo.TEST", "webhook_user": "postmark",
                "server_token_file": private("token", "fixture-postmark-token-0001"),
                "webhook_password_file": private("password", "fixture-webhook-password-01\n")})
            .to_string(),
        );
        let s = read(&file).unwrap();
        assert_eq!(s.domain, "agents.cuckoo.test");
        assert!(s.serves("research-k7q2m@agents.cuckoo.test"));
        assert!(!s.serves("research@other.test"));
        let basic = |pair: &str| format!("Basic {}", STANDARD.encode(pair));
        assert!(s.authorized(Some(&basic("postmark:fixture-webhook-password-01"))));
        for wrong in [
            basic("postmark:fixture-webhook-password-02"),
            basic("other:fixture-webhook-password-01"),
            "Bearer fixture-webhook-password-01".into(),
        ] {
            assert!(!s.authorized(Some(&wrong)), "{wrong}");
        }
        assert!(!s.authorized(None));
        // A secret readable by others is refused, so no request is authorized.
        std::fs::set_permissions(
            dir.path().join("password"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(!s.authorized(Some(&basic("postmark:fixture-webhook-password-01"))));
    }

    #[test]
    fn domains_need_two_lowercase_labels() {
        assert!(valid_domain("agents.cuckoo.app"));
        for bad in ["localhost", "Agents.cuckoo.app", "a..b", "-a.b", "a_b.com"] {
            assert!(!valid_domain(bad), "{bad}");
        }
    }
}
