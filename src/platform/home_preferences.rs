//! Owner-selected business clock, shared by Home and schedule entry.
use super::Platform;
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HomePreferences {
    pub location: String,
    pub timezone: String,
    pub hour12: bool,
}
impl HomePreferences {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.location.trim().is_empty() && self.location.len() <= 100,
            "Choose a location label of 1–100 bytes"
        );
        ensure!(
            !self.timezone.is_empty()
                && self.timezone.len() <= 100
                && self.timezone.split('/').all(|part| !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_+-".contains(&c))),
            "Invalid timezone"
        );
        // Ayati runs on Linux; use the installed IANA zoneinfo database, not offsets.
        let bytes =
            std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(&self.timezone))?;
        ensure!(
            bytes.starts_with(b"TZif"),
            "Choose an installed IANA timezone"
        );
        Ok(())
    }
}
impl Platform {
    pub fn home_preferences(&self) -> Result<Option<HomePreferences>> {
        let saved: Option<String> = self
            .database()?
            .query_row("SELECT value FROM home_preferences WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        saved
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn save_home_preferences(&self, value: &HomePreferences) -> Result<()> {
        value.validate()?;
        self.database()?.execute("INSERT INTO home_preferences VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value", [serde_json::to_string(value)?])?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_survives_restart_and_rejects_paths_and_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let p = Platform::open(dir.path(), None).unwrap();
        assert!(p.home_preferences().unwrap().is_none());
        let mut value = HomePreferences {
            location: "Hyderabad".into(),
            timezone: "Asia/Kolkata".into(),
            hour12: true,
        };
        p.save_home_preferences(&value).unwrap();
        drop(p);
        let p = Platform::open(dir.path(), None).unwrap();
        assert_eq!(
            p.home_preferences().unwrap().unwrap().timezone,
            "Asia/Kolkata"
        );
        for zone in [
            "../../etc/passwd",
            "/etc/passwd",
            "UTC+05:30",
            "Unknown/City",
        ] {
            value.timezone = zone.into();
            assert!(p.save_home_preferences(&value).is_err());
        }
        assert_eq!(p.home_preferences().unwrap().unwrap().location, "Hyderabad");
    }
}
