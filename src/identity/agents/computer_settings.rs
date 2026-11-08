//! An agent's computer, as its owner sets it (runtime/agent-computers.md): on or off,
//! its size, whether it may run programs, and which websites it may reach.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ComputerSettings {
    /// Off for every agent until its owner or an admin turns it on.
    pub on: bool,
    /// small, standard or large.
    pub size: String,
    /// Running programs and commands: allow, ask (first) or block.
    pub programs: String,
    /// Opening and reading websites in its browser: allow, ask (first, per website) or block.
    pub browser: String,
    /// Pressing buttons and sending forms on websites: allow, ask (first) or block.
    pub submit: String,
    /// The computer's internet: none, listed (only `websites`) or any.
    pub internet: String,
    /// Host names the computer may reach when `internet` is listed.
    pub websites: Vec<String>,
}
impl Default for ComputerSettings {
    fn default() -> Self {
        Self {
            on: false,
            size: "standard".into(),
            programs: "allow".into(),
            browser: "allow".into(),
            submit: "ask".into(),
            internet: "none".into(),
            websites: vec![],
        }
    }
}

/// A host name such as `portal.sharmatraders.in`, or a whole domain as `*.zoho.in`.
fn host(name: &str) -> bool {
    let name = name.strip_prefix("*.").unwrap_or(name);
    !name.is_empty()
        && name.len() <= 253
        && name.contains('.')
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

impl ComputerSettings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            crate::identity::computers::SIZES.contains(&self.size.as_str()),
            "Choose a small, standard or large computer"
        );
        for (control, name) in [
            (&self.programs, "Programs"),
            (&self.browser, "Browsing"),
            (&self.submit, "Pressing buttons on websites"),
        ] {
            ensure!(
                ["allow", "ask", "block"].contains(&control.as_str()),
                "{name} is allowed, asked first or blocked"
            );
        }
        ensure!(
            ["none", "listed", "any"].contains(&self.internet.as_str()),
            "The computer's internet is none, listed websites or any website"
        );
        ensure!(
            self.websites.len() <= 30 && self.websites.iter().all(|w| host(w)),
            "List up to 30 websites as host names, such as portal.example.in or *.example.in"
        );
        Ok(())
    }

    /// The rule the provider enforces on the computer's network.
    pub(crate) fn internet(&self) -> crate::identity::computers::Internet {
        use crate::identity::computers::Internet;
        match self.internet.as_str() {
            "any" => Internet::Any,
            "listed" if !self.websites.is_empty() => Internet::Only(self.websites.clone()),
            _ => Internet::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::computers::Internet;

    #[test]
    fn settings_are_checked_and_turned_into_network_rules() {
        let mut settings = ComputerSettings::default();
        settings.validate().unwrap();
        assert_eq!((settings.on, settings.internet()), (false, Internet::None));
        settings.internet = "listed".into();
        settings.websites = vec!["portal.sharmatraders.in".into(), "*.zoho.in".into()];
        settings.validate().unwrap();
        assert_eq!(
            settings.internet(),
            Internet::Only(settings.websites.clone())
        );
        settings.websites.clear();
        assert_eq!(
            settings.internet(),
            Internet::None,
            "an empty list reaches nothing"
        );
        for (field, value) in [
            ("size", "huge"),
            ("programs", "sometimes"),
            ("browser", "maybe"),
            ("submit", "often"),
            ("internet", "some"),
        ] {
            let mut bad = ComputerSettings::default();
            match field {
                "size" => bad.size = value.into(),
                "programs" => bad.programs = value.into(),
                "browser" => bad.browser = value.into(),
                "submit" => bad.submit = value.into(),
                _ => bad.internet = value.into(),
            }
            assert!(bad.validate().is_err(), "{field}={value}");
        }
        for site in ["https://x.in", "x", "UPPER.in", "a..b", "-a.in", "a b.in"] {
            let bad = ComputerSettings {
                websites: vec![site.into()],
                ..Default::default()
            };
            assert!(bad.validate().is_err(), "{site}");
        }
    }
}
