//! Model provider API keys added in Cuckoo Control are sealed at rest with
//! AES-256-GCM (ring). The 32-byte key comes from `AYATI_SECRETS_KEY_FILE`: an
//! owned 0600 file holding 64 hex characters (`openssl rand -hex 32`). It is
//! read on every use, like the other private key files, and never logged.
//!
//! The provider name is bound as associated data, so a sealed key cannot be
//! moved from one provider's row to another's.
use anyhow::{Context, Result, ensure};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct Sealer {
    file: PathBuf,
}

impl Sealer {
    pub(crate) fn new(file: &Path) -> Self {
        Self {
            file: file.to_owned(),
        }
    }

    fn key(&self) -> Result<LessSafeKey> {
        let text = crate::identity::private_config::read(&self.file)
            .context("AYATI_SECRETS_KEY_FILE is unavailable")?;
        let bytes =
            hex(text.trim()).context("AYATI_SECRETS_KEY_FILE must hold 64 hex characters")?;
        let key = UnboundKey::new(&AES_256_GCM, &bytes)
            .map_err(|_| anyhow::anyhow!("AYATI_SECRETS_KEY_FILE must hold 64 hex characters"))?;
        Ok(LessSafeKey::new(key))
    }

    /// A random nonce followed by the ciphertext and tag.
    pub(crate) fn seal(&self, label: &str, secret: &str) -> Result<Vec<u8>> {
        let key = self.key()?;
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| anyhow::anyhow!("OS randomness unavailable"))?;
        let mut data = secret.as_bytes().to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(label.as_bytes()),
            &mut data,
        )
        .map_err(|_| anyhow::anyhow!("Could not seal the key"))?;
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&data);
        Ok(sealed)
    }

    pub(crate) fn open(&self, label: &str, sealed: &[u8]) -> Result<String> {
        ensure!(sealed.len() > NONCE_LEN, "Sealed key is damaged");
        let key = self.key()?;
        let (nonce, data) = sealed.split_at(NONCE_LEN);
        let mut data = data.to_vec();
        let nonce = Nonce::try_assume_unique_for_key(nonce)
            .map_err(|_| anyhow::anyhow!("Sealed key is damaged"))?;
        let plain = key
            .open_in_place(nonce, Aad::from(label.as_bytes()), &mut data)
            // Wrong secrets key, a moved row or tampering: never say which.
            .map_err(|_| {
                anyhow::anyhow!("The stored key cannot be opened with this AYATI_SECRETS_KEY_FILE")
            })?;
        String::from_utf8(plain.to_vec()).context("Sealed key is damaged")
    }
}

fn hex(text: &str) -> Option<Vec<u8>> {
    if text.len() != 64 {
        return None;
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn sealer(dir: &Path, hex_key: &str) -> Sealer {
        let file = dir.join(format!("secrets-{}", &hex_key[..4]));
        std::fs::write(&file, hex_key).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        Sealer::new(&file)
    }

    #[test]
    fn keys_open_only_with_the_same_secret_and_provider() {
        let dir = tempfile::tempdir().unwrap();
        let one = sealer(dir.path(), &"ab".repeat(32));
        let sealed = one.seal("fireworks", "fw_live_123").unwrap();
        assert!(!String::from_utf8_lossy(&sealed).contains("fw_live_123"));
        assert_eq!(one.open("fireworks", &sealed).unwrap(), "fw_live_123");
        // Moved to another provider's row, or opened with another deployment's key.
        assert!(one.open("openai", &sealed).is_err());
        let other = sealer(dir.path(), &"cd".repeat(32));
        assert!(other.open("fireworks", &sealed).is_err());
        // Two seals of the same key differ (random nonce).
        assert_ne!(one.seal("fireworks", "fw_live_123").unwrap(), sealed);
    }

    #[test]
    fn the_secrets_file_must_be_private_and_well_formed() {
        let dir = tempfile::tempdir().unwrap();
        let short = sealer(dir.path(), "abcd");
        assert!(short.seal("x", "y").is_err());
        let open = dir.path().join("open");
        std::fs::write(&open, "ab".repeat(32)).unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Sealer::new(&open).seal("x", "y").is_err());
    }
}
