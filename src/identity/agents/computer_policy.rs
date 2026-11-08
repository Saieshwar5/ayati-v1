//! A workspace's limits on agent computers, set by its admins in Settings → Computers
//! (designs/agent-computers 5). An agent's own settings can only be stricter: the
//! stricter of the two applies whenever the computer is used.
use super::computer_settings::{ComputerSettings, host};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ComputerPolicy {
    /// Agents in this workspace may have computers.
    pub allowed: bool,
    /// The sizes agents may use.
    pub sizes: Vec<String>,
    /// The loosest internet: none, listed (only `websites`) or any.
    pub internet: String,
    /// The websites agents' computers may reach when `internet` is listed.
    pub websites: Vec<String>,
    /// The loosest rules for programs, browsing and pressing buttons: allow, ask or block.
    pub programs: String,
    pub browser: String,
    pub submit: String,
    /// How many of the workspace's computers may be awake at once; 0 is no limit.
    pub max_awake: u32,
}
impl Default for ComputerPolicy {
    fn default() -> Self {
        Self {
            allowed: true,
            sizes: crate::identity::computers::SIZES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            internet: "any".into(),
            websites: vec![],
            programs: "allow".into(),
            browser: "allow".into(),
            submit: "allow".into(),
            max_awake: 0,
        }
    }
}

const MODES: [&str; 3] = ["allow", "ask", "block"];
/// The stricter of two Allow / Ask first / Block rules.
fn stricter(a: &str, b: &str) -> String {
    let rank = |m: &str| MODES.iter().position(|x| *x == m).unwrap_or(2);
    MODES[rank(a).max(rank(b))].to_owned()
}
/// Whether `site` is inside an allowed website, such as `portal.x.in` inside `*.x.in`.
fn within(site: &str, allowed: &str) -> bool {
    match allowed.strip_prefix("*.") {
        Some(domain) => {
            let site = site.strip_prefix("*.").unwrap_or(site);
            site == domain || site.ends_with(&format!(".{domain}"))
        }
        None => site == allowed,
    }
}

impl ComputerPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.sizes.len() <= 3
                && self
                    .sizes
                    .iter()
                    .all(|s| crate::identity::computers::SIZES.contains(&s.as_str())),
            "Choose sizes from small, standard and large"
        );
        ensure!(
            ["none", "listed", "any"].contains(&self.internet.as_str()),
            "Internet is none, listed websites or any website"
        );
        ensure!(
            self.websites.len() <= 100 && self.websites.iter().all(|w| host(w)),
            "List up to 100 websites as host names, such as portal.example.in or *.example.in"
        );
        ensure!(
            self.max_awake <= 1000,
            "Allow at most 1,000 computers awake at once"
        );
        for rule in [&self.programs, &self.browser, &self.submit] {
            ensure!(
                MODES.contains(&rule.as_str()),
                "Rules are allowed, asked first or blocked"
            );
        }
        Ok(())
    }

    /// The agent's settings as they apply in this workspace: never looser than here.
    pub fn apply(&self, own: &ComputerSettings) -> ComputerSettings {
        let mut next = own.clone();
        next.on = own.on && self.allowed;
        next.programs = stricter(&own.programs, &self.programs);
        next.browser = stricter(&own.browser, &self.browser);
        next.submit = stricter(&own.submit, &self.submit);
        match self.internet.as_str() {
            "none" => {
                next.internet = "none".into();
                next.websites.clear();
            }
            "listed" => {
                let sites: Vec<String> = if own.internet == "any" {
                    self.websites.clone()
                } else {
                    own.websites
                        .iter()
                        .filter(|s| self.websites.iter().any(|a| within(s, a)))
                        .cloned()
                        .collect()
                };
                next.internet = if own.internet == "none" {
                    "none"
                } else {
                    "listed"
                }
                .into();
                next.websites = if own.internet == "none" {
                    vec![]
                } else {
                    sites
                };
            }
            _ => {}
        }
        next
    }

    pub fn size_allowed(&self, size: &str) -> bool {
        self.sizes.iter().any(|s| s == size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(internet: &str, websites: &[&str]) -> ComputerSettings {
        ComputerSettings {
            on: true,
            internet: internet.into(),
            websites: websites.iter().map(|s| (*s).to_owned()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_workspace_limits_win_and_only_make_agents_stricter() {
        let open = ComputerPolicy::default();
        open.validate().unwrap();
        let own = agent("any", &[]);
        assert_eq!(open.apply(&own), own, "the default limits change nothing");

        let policy = ComputerPolicy {
            internet: "listed".into(),
            websites: vec!["*.zoho.in".into(), "portal.sharmatraders.in".into()],
            programs: "ask".into(),
            submit: "block".into(),
            ..Default::default()
        };
        policy.validate().unwrap();
        // Any website becomes the workspace's list; a listed agent keeps what's inside it.
        assert_eq!(policy.apply(&own).websites, policy.websites);
        let listed = policy.apply(&agent(
            "listed",
            &["books.zoho.in", "evil.example", "portal.sharmatraders.in"],
        ));
        assert_eq!(
            (listed.internet.as_str(), listed.websites.clone()),
            (
                "listed",
                vec![
                    "books.zoho.in".to_owned(),
                    "portal.sharmatraders.in".to_owned()
                ]
            )
        );
        assert_eq!(policy.apply(&agent("none", &[])).internet, "none");
        // Rules take the stricter side; an agent stricter than the workspace stays so.
        let mut strict = agent("any", &[]);
        strict.programs = "block".into();
        let applied = policy.apply(&strict);
        assert_eq!(
            (
                applied.programs.as_str(),
                applied.browser.as_str(),
                applied.submit.as_str()
            ),
            ("block", "allow", "block")
        );
        assert_eq!(policy.apply(&agent("any", &[])).programs, "ask");
        // Switched off for the workspace: no agent has a computer.
        let off = ComputerPolicy {
            allowed: false,
            ..Default::default()
        };
        assert!(!off.apply(&own).on);
        let small = ComputerPolicy {
            sizes: vec!["small".into()],
            ..Default::default()
        };
        assert!(small.size_allowed("small") && !small.size_allowed("large"));
        for bad in [
            ComputerPolicy {
                sizes: vec!["huge".into()],
                ..Default::default()
            },
            ComputerPolicy {
                internet: "some".into(),
                ..Default::default()
            },
            ComputerPolicy {
                websites: vec!["https://x.in".into()],
                ..Default::default()
            },
            ComputerPolicy {
                submit: "often".into(),
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
    }
}
