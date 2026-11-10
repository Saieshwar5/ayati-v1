//! The operator's WhatsApp file (`AYATI_WHATSAPP_FILE`, controller only): the
//! provider account, where its auth token is kept and the numbers already
//! registered as WhatsApp senders. Twilio calls Cuckoo's public origin. Read on every
//! use, so numbers can be added and the token rotated without a restart.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::hmac;
use serde::Deserialize;
use std::{path::PathBuf, sync::OnceLock};

/// The provider server name saved on WhatsApp endpoints.
pub(crate) const SERVER: &str = "twilio";
/// Where Twilio posts incoming messages and delivery updates.
pub(crate) const INCOMING_PATH: &str = "/hooks/whatsapp/twilio";
pub(crate) const STATUS_PATH: &str = "/hooks/whatsapp/twilio/status";
const API: &str = "https://api.twilio.com";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    /// The Twilio account SID, `AC` and 32 hex digits.
    pub account: String,
    /// An owned 0600 file holding the account's auth token.
    pub auth_token_file: PathBuf,
    /// Cuckoo's public origin (`AYATI_PUBLIC_ORIGIN`), the base of the webhooks.
    #[serde(skip)]
    pub origin: String,
    /// The Twilio API base. Only for local tests: a loopback address.
    #[serde(default)]
    pub api: Option<String>,
    /// Numbers registered as WhatsApp senders, in international format.
    pub numbers: Vec<String>,
    /// How long a released number rests before another agent gets it.
    #[serde(default = "cooling_days")]
    pub cooling_days: i64,
}
fn cooling_days() -> i64 {
    90
}

static FILE: OnceLock<(Option<PathBuf>, String)> = OnceLock::new();

pub(crate) fn configure(file: Option<PathBuf>, origin: String) {
    let _ = FILE.set((file, origin));
}

/// The current settings, or `None` when this server offers no WhatsApp numbers.
pub(crate) fn current() -> Result<Option<Settings>> {
    match FILE.get() {
        Some((Some(file), origin)) => read(file, origin).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn read(file: &std::path::Path, origin: &str) -> Result<Settings> {
    let mut settings: Settings =
        serde_json::from_str(&crate::identity::private_config::read(file)?)
            .map_err(|_| anyhow::anyhow!("Invalid WhatsApp configuration"))?;
    settings.origin = origin.trim_end_matches('/').to_owned();
    settings.validate()?;
    Ok(settings)
}

impl Settings {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.account.len() == 34
                && self.account.starts_with("AC")
                && self.account[2..].chars().all(|c| c.is_ascii_hexdigit()),
            "Invalid Twilio account SID"
        );
        ensure!(
            self.auth_token_file.is_absolute(),
            "Use an absolute auth token path"
        );
        origin(&self.origin)?;
        if let Some(api) = &self.api {
            ensure!(
                api.starts_with("http://127.0.0.1:"),
                "A Twilio API override must be a loopback test server"
            );
        }
        ensure!(self.numbers.len() <= 1000, "Too many WhatsApp numbers");
        for number in &self.numbers {
            ensure!(valid_number(number), "Invalid WhatsApp number {number}");
        }
        ensure!(
            (1..=365).contains(&self.cooling_days),
            "Cooling days must be between 1 and 365"
        );
        Ok(())
    }
    pub fn token(&self) -> Result<String> {
        let token = crate::identity::private_config::read(&self.auth_token_file)?;
        let token = token.trim();
        ensure!(
            token.len() >= 16 && !token.chars().any(char::is_whitespace),
            "Invalid Twilio auth token"
        );
        Ok(token.to_owned())
    }
    pub fn api(&self) -> &str {
        self.api.as_deref().unwrap_or(API)
    }
    pub fn offers(&self, number: &str) -> bool {
        self.numbers.iter().any(|n| n == number)
    }
    /// The exact URL Twilio called, which its signature covers.
    pub fn url(&self, path_and_query: &str) -> String {
        format!("{}{path_and_query}", self.origin)
    }
}

fn origin(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("Invalid public origin")?;
    let local = url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    ensure!(
        (url.scheme() == "https" || local) && url.path() == "/" && url.query().is_none(),
        "WhatsApp needs an HTTPS public origin (AYATI_PUBLIC_ORIGIN)"
    );
    Ok(())
}

/// International format: `+`, a country code and up to 15 digits in all.
pub(crate) fn valid_number(value: &str) -> bool {
    let digits = value.strip_prefix('+').unwrap_or("");
    (7..=15).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
        && !digits.starts_with('0')
}

/// Twilio's request signature: HMAC-SHA1 with the auth token over the full URL
/// followed by every POST parameter, sorted by name, as name then value; base64.
/// Constant-time comparison (`ring::hmac::verify`).
pub(crate) fn signed(token: &str, url: &str, params: &[(String, String)], given: &str) -> bool {
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, token.as_bytes());
    let Ok(given) = STANDARD.decode(given) else {
        return false;
    };
    hmac::verify(&key, signing_data(url, params).as_bytes(), &given).is_ok()
}
fn signing_data(url: &str, params: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = params.iter().collect();
    sorted.sort();
    let mut data = url.to_owned();
    for (name, value) in sorted {
        data.push_str(name);
        data.push_str(value);
    }
    data
}

#[cfg(test)]
pub(crate) fn sign(token: &str, url: &str, params: &[(String, String)]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, token.as_bytes());
    STANDARD.encode(hmac::sign(&key, signing_data(url, params).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn twilio_signatures_cover_the_url_and_every_parameter() {
        // Twilio's documented example (docs/usage/security, 2026-11-10).
        let url = "https://example.com/myapp.php?foo=1&bar=2";
        let params = pairs(&[
            ("Digits", "1234"),
            ("To", "+18005551212"),
            ("From", "+14158675310"),
            ("Caller", "+14158675310"),
            ("CallSid", "CA1234567890ABCDE"),
        ]);
        let signature = sign("12345", url, &params);
        assert_eq!(signature, "L/OH5YylLD5NRKLltdqwSvS0BnU=");
        assert!(signed("12345", url, &params, &signature));
        let mut reordered = params.clone();
        reordered.reverse();
        assert!(signed("12345", url, &reordered, &signature));
        let mut changed = params.clone();
        changed[2].1 = "9999".into();
        assert!(!signed("12345", url, &changed, &signature));
        assert!(!signed(
            "12345",
            "https://example.com/other",
            &params,
            &signature
        ));
        assert!(!signed("54321", url, &params, &signature));
        assert!(!signed("12345", url, &params, "not base64!"));
    }

    #[test]
    fn numbers_are_international() {
        assert!(valid_number("+14155550142"));
        assert!(valid_number("+919812345678"));
        for bad in [
            "14155550142",
            "+0123456789",
            "+12",
            "+1415555014x",
            "+1234567890123456",
        ] {
            assert!(!valid_number(bad), "{bad}");
        }
    }
}
