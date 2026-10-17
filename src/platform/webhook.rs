use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::hmac;

/// Composio V3 signs the exact bytes with the literal webhook secret. Verify
/// before decoding JSON; bounded timestamp tolerance also rejects future replays.
pub fn verify(
    secret: &str,
    id: &str,
    timestamp: &str,
    signature: &str,
    body: &[u8],
    now: i64,
) -> Result<()> {
    ensure!(
        secret.len() >= 16 && body.len() <= 64000,
        "Webhook verification is unavailable or oversized"
    );
    super::identifier(id)?;
    let time: i64 = timestamp.parse()?;
    ensure!(
        (now as i128 - time as i128).abs() <= 300,
        "Webhook timestamp is outside the accepted window"
    );
    let mut bytes = format!("{id}.{timestamp}.").into_bytes();
    bytes.extend_from_slice(body);
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let valid = signature
        .split_whitespace()
        .take(8)
        .filter_map(|v| v.strip_prefix("v1,"))
        .filter_map(|v| STANDARD.decode(v).ok())
        .any(|v| hmac::verify(&key, &bytes, &v).is_ok());
    ensure!(valid, "Invalid webhook signature");
    Ok(())
}
