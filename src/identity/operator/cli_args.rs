//! A small argument reader for `ayati admin`: positional words, `--name value`
//! options and a few `--switch` flags. Unknown options are errors, so a typo
//! never changes the platform silently.
use anyhow::{Result, bail};
use std::collections::HashMap;

const SWITCHES: &[&str] = &["json", "blocked"];

pub(super) struct Args {
    pub words: Vec<String>,
    options: HashMap<String, String>,
    switches: Vec<String>,
}

impl Args {
    pub(super) fn parse(raw: &[String]) -> Result<Self> {
        let (mut words, mut options, mut switches) = (vec![], HashMap::new(), vec![]);
        let mut items = raw.iter();
        while let Some(item) = items.next() {
            let Some(name) = item.strip_prefix("--") else {
                words.push(item.clone());
                continue;
            };
            if let Some((name, value)) = name.split_once('=') {
                options.insert(name.to_owned(), value.to_owned());
            } else if SWITCHES.contains(&name) {
                switches.push(name.to_owned());
            } else {
                let Some(value) = items.next() else {
                    bail!("--{name} needs a value")
                };
                options.insert(name.to_owned(), value.clone());
            }
        }
        Ok(Self {
            words,
            options,
            switches,
        })
    }

    pub(super) fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }

    pub(super) fn required(&self, name: &str) -> Result<&str> {
        match self.option(name) {
            Some(value) => Ok(value),
            None => bail!("Add --{name}"),
        }
    }

    pub(super) fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    /// Fails on any option the command does not use.
    pub(super) fn only(&self, allowed: &[&str]) -> Result<()> {
        for name in self.options.keys().chain(&self.switches) {
            if !allowed.contains(&name.as_str()) && name != "as" && name != "json" {
                bail!("Unknown option --{name}");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Args {
        Args::parse(&text.split(' ').map(str::to_owned).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn words_options_and_switches() {
        let a = args("provider switch fireworks --off-reason x --reason=slow --json");
        assert_eq!(a.words, ["provider", "switch", "fireworks"]);
        assert_eq!(a.option("off-reason"), Some("x"));
        assert_eq!(a.option("reason"), Some("slow"));
        assert!(a.switch("json"));
        assert!(a.only(&["reason"]).is_err());
        assert!(a.only(&["reason", "off-reason"]).is_ok());
        assert!(Args::parse(&["--reason".to_owned()]).is_err());
    }
}
