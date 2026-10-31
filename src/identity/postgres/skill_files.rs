//! Checks a skill's files against the Agent Skills format (agentskills.io) and
//! Cuckoo's limits (D234). The browser unzips; the server only ever receives
//! text and never writes skill files to disk, so links and paths can't escape.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_FILES: usize = 100;
pub(crate) const MAX_BYTES: usize = 5 * 1024 * 1024;
const MAX_SKILL_LINES: usize = 500;
/// Text files only for now; script sources are kept as text and never run.
const TEXT: [&str; 7] = ["md", "txt", "json", "csv", "yaml", "yml", "tsv"];
const SCRIPTS: [&str; 7] = ["py", "sh", "js", "ts", "rb", "ps1", "bash"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillFile {
    pub path: String,
    pub text: String,
}

/// What the files say about the skill.
#[derive(Debug, Serialize)]
pub(crate) struct Checked {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub has_scripts: bool,
    pub hash: String,
    pub bytes: usize,
}

/// Validates a whole skill: `SKILL.md` at the root plus optional folders.
pub(crate) fn check(files: &[SkillFile]) -> Result<Checked> {
    ensure!(!files.is_empty(), "Add a SKILL.md file");
    ensure!(
        files.len() <= MAX_FILES,
        "A skill can have at most {MAX_FILES} files"
    );
    let bytes: usize = files.iter().map(|f| f.path.len() + f.text.len()).sum();
    ensure!(bytes <= MAX_BYTES, "A skill can be at most 5 MB");
    let mut paths = std::collections::HashSet::new();
    let mut has_scripts = false;
    for file in files {
        path(&file.path)?;
        ensure!(
            paths.insert(file.path.as_str()),
            "{} appears twice",
            file.path
        );
        ensure!(
            !file.text.contains('\0'),
            "{} is not a text file",
            file.path
        );
        let extension = file
            .path
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        let script = SCRIPTS.contains(&extension.as_str()) || file.path.starts_with("scripts/");
        ensure!(
            script || TEXT.contains(&extension.as_str()),
            "{} isn't supported yet: skills can hold text files (.md, .txt, .json, .csv, .yaml) and script sources",
            file.path
        );
        has_scripts |= script;
    }
    let skill = files
        .iter()
        .find(|f| f.path == "SKILL.md")
        .ok_or_else(|| anyhow::anyhow!("Add a SKILL.md file at the top of the skill"))?;
    ensure!(
        skill.text.lines().count() <= MAX_SKILL_LINES,
        "SKILL.md can be at most {MAX_SKILL_LINES} lines"
    );
    let front = frontmatter(&skill.text)?;
    let name = front
        .name
        .ok_or_else(|| anyhow::anyhow!("SKILL.md needs a name"))?;
    ensure!(
        spec_name(&name),
        "Use a name of up to 64 lowercase letters, digits and single hyphens, like quote-builder"
    );
    let description = front
        .description
        .ok_or_else(|| anyhow::anyhow!("SKILL.md needs a description"))?;
    ensure!(
        !description.is_empty() && description.chars().count() <= 1024,
        "The description (\"Use it when…\") must be 1 to 1,024 characters"
    );
    Ok(Checked {
        name,
        description,
        license: front.license,
        has_scripts,
        hash: hash(files),
        bytes,
    })
}

/// A relative path of plain segments: no absolute paths, `..`, `.`, backslashes
/// or control characters.
fn path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 200,
        "File paths must be 1 to 200 characters"
    );
    ensure!(
        !path.starts_with('/') && !path.contains('\\'),
        "{path}: use a path inside the skill"
    );
    for segment in path.split('/') {
        ensure!(
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._- ".contains(c)),
            "{path}: use letters, digits, '.', '-' and '_' in file names"
        );
    }
    Ok(())
}

/// The spec's name rule: 1–64 of a-z, 0-9 and single inner hyphens.
pub(crate) fn spec_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

#[derive(Default)]
struct Front {
    name: Option<String>,
    description: Option<String>,
    license: Option<String>,
}

/// Reads `key: value` lines between the opening and closing `---`. Values may be
/// quoted; folded (`>`) and literal (`|`) blocks are joined into one line.
fn frontmatter(text: &str) -> Result<Front> {
    let mut lines = text.lines();
    ensure!(
        lines.next().map(str::trim) == Some("---"),
        "SKILL.md must start with --- and a name and description"
    );
    let mut front = Front::default();
    let mut block: Option<(String, Vec<String>)> = None;
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((key, parts)) = &mut block {
            if line.starts_with(' ') || line.trim().is_empty() {
                parts.push(line.trim().to_string());
                continue;
            }
            let value = parts
                .iter()
                .filter(|p| !p.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            set(&mut front, key, value);
            block = None;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if line.starts_with(' ') {
            continue;
        }
        let value = value.trim();
        if value == ">" || value == "|" || value == ">-" || value == "|-" {
            block = Some((key.trim().to_string(), vec![]));
        } else {
            set(&mut front, key.trim(), unquote(value));
        }
    }
    if let Some((key, parts)) = block {
        set(
            &mut front,
            &key,
            parts
                .iter()
                .filter(|p| !p.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    ensure!(
        closed,
        "SKILL.md must close its name and description with ---"
    );
    Ok(front)
}

fn set(front: &mut Front, key: &str, value: String) {
    let value = Some(value).filter(|v| !v.is_empty());
    match key {
        "name" => front.name = value,
        "description" => front.description = value,
        "license" => front.license = value.map(|v| v.chars().take(200).collect()),
        _ => {}
    }
}

fn unquote(value: &str) -> String {
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

/// The same files give the same hash, whatever order they arrive in, so a
/// downloaded skill uploads back identically.
pub(crate) fn hash(files: &[SkillFile]) -> String {
    let mut sorted: Vec<&SkillFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    for file in sorted {
        digest.update(file.path.as_bytes());
        digest.update(&[0]);
        digest.update(file.text.len().to_string().as_bytes());
        digest.update(&[0]);
        digest.update(file.text.as_bytes());
    }
    digest
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(front: &str) -> SkillFile {
        SkillFile {
            path: "SKILL.md".into(),
            text: format!("---\n{front}\n---\n# Steps\n1. Check the price list.\n"),
        }
    }

    #[test]
    fn a_valid_skill_reads_its_name_description_and_license() {
        let files = vec![
            skill(
                "name: quote-builder\ndescription: \"Use when a customer asks for a quote\"\nlicense: Apache-2.0",
            ),
            SkillFile {
                path: "references/prices.csv".into(),
                text: "item,price\nchair,4500\n".into(),
            },
        ];
        let checked = check(&files).unwrap();
        assert_eq!(
            (
                checked.name.as_str(),
                checked.license.as_deref(),
                checked.has_scripts
            ),
            ("quote-builder", Some("Apache-2.0"), false)
        );
        assert_eq!(checked.description, "Use when a customer asks for a quote");
        let reversed: Vec<SkillFile> = files.iter().rev().cloned().collect();
        assert_eq!(
            hash(&reversed),
            checked.hash,
            "order doesn't change the hash"
        );
        let folded = check(&[skill("name: x\ndescription: >\n  Use when\n  asked")]).unwrap();
        assert_eq!(folded.description, "Use when asked");
    }

    #[test]
    fn names_paths_types_and_sizes_are_refused_clearly() {
        for bad in [
            "Quote",
            "-quote",
            "quote-",
            "quote--builder",
            "quote_builder",
        ] {
            assert!(
                check(&[skill(&format!("name: {bad}\ndescription: d"))]).is_err(),
                "{bad}"
            );
        }
        assert!(check(&[skill("description: d")]).is_err());
        assert!(
            check(&[skill(&format!(
                "name: a\ndescription: {}",
                "x".repeat(1025)
            ))])
            .is_err()
        );
        let ok = skill("name: a\ndescription: d");
        for path in [
            "../etc/passwd",
            "/abs.md",
            "a/../b.md",
            "a\\b.md",
            "./a.md",
            "a//b.md",
        ] {
            assert!(
                check(&[
                    ok.clone(),
                    SkillFile {
                        path: path.into(),
                        text: "x".into()
                    }
                ])
                .is_err(),
                "{path}"
            );
        }
        assert!(
            check(&[
                ok.clone(),
                SkillFile {
                    path: "logo.png".into(),
                    text: "x".into()
                }
            ])
            .is_err()
        );
        assert!(
            check(&[
                ok.clone(),
                SkillFile {
                    path: "a.md".into(),
                    text: "a\0b".into()
                }
            ])
            .is_err()
        );
        assert!(check(&[ok.clone(), ok.clone()]).is_err(), "duplicates");
        let many: Vec<SkillFile> = (0..MAX_FILES)
            .map(|n| SkillFile {
                path: format!("r/{n}.md"),
                text: "x".into(),
            })
            .chain([ok.clone()])
            .collect();
        assert!(check(&many).is_err());
        // A "zip bomb" arrives already expanded; its size is what counts.
        let huge = SkillFile {
            path: "references/big.txt".into(),
            text: "0".repeat(MAX_BYTES),
        };
        assert!(check(&[ok.clone(), huge]).is_err());
        let long = SkillFile {
            path: "SKILL.md".into(),
            text: format!(
                "---\nname: a\ndescription: d\n---\n{}",
                "line\n".repeat(MAX_SKILL_LINES)
            ),
        };
        assert!(check(&[long]).is_err());
        assert!(
            check(&[
                ok.clone(),
                SkillFile {
                    path: "scripts/run.py".into(),
                    text: "print(1)".into()
                }
            ])
            .unwrap()
            .has_scripts
        );
    }
}
