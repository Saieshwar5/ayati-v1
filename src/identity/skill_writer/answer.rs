//! Reads the writer's answer into drafts that pass the same checks as uploaded
//! skills. Names are made valid and free in the workspace; anything that can't
//! be used is left out with a note people can read.
use super::{Request, WriterContext};
use crate::identity::agents::config::INSTRUCTIONS_LIMIT;
use crate::identity::postgres::skills::files::{SkillFile, check};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Skills one answer may add or change.
const MAX_SKILLS: usize = 8;
/// Step lines kept, so SKILL.md stays under the 500-line limit with its header.
const MAX_STEP_LINES: usize = 480;
const UNREADABLE: &str = "The answer couldn't be read. Try again.";

#[derive(Deserialize, Default)]
#[serde(default)]
struct Raw {
    reply: String,
    ground_rules: Option<String>,
    skills: Vec<RawSkill>,
    remove: Vec<String>,
    reuse: Vec<String>,
    missing_apps: Vec<String>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSkill {
    name: String,
    description: String,
    steps: String,
}

/// A skill ready to show and save: its fields and the SKILL.md made from them.
#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct Draft {
    pub name: String,
    pub description: String,
    pub steps: String,
    pub text: String,
}

#[derive(Serialize, Debug)]
pub(crate) struct Answer {
    pub reply: String,
    /// New ground rules, or none when unchanged.
    pub ground_rules: Option<String>,
    /// New or changed skills, in full.
    pub skills: Vec<Draft>,
    /// Drafts to drop, by name; only the request's own drafts.
    pub remove: Vec<String>,
    /// Library skills to add instead of copies: `{id, name, description}`.
    pub reuse: Vec<Value>,
    pub missing_apps: Vec<String>,
    /// What Cuckoo changed or left out, in plain words.
    pub notes: Vec<String>,
}

/// The JSON object in an answer, even when wrapped in a code fence or prose.
fn object(content: &str) -> Result<Raw> {
    let start = content.find('{').context(UNREADABLE)?;
    let end = content.rfind('}').context(UNREADABLE)?;
    ensure!(end > start, UNREADABLE);
    serde_json::from_str(&content[start..=end]).context(UNREADABLE)
}

/// A name in the Agent Skills format: lowercase letters, digits and single
/// hyphens, at most 64 characters. Other characters become hyphens.
pub(crate) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut out: String = out.chars().take(64).collect();
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn one_line(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn within_bytes(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// SKILL.md for a written skill.
pub(crate) fn skill_md(name: &str, description: &str, steps: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: {description}\n---\n{}\n",
        steps.trim()
    )
}

/// A free name close to `wanted`: with the agent's name, then a number.
fn free_name(wanted: &str, agent: &str, taken: &HashSet<String>) -> String {
    let fit = |suffix: &str| {
        let room = 64 - suffix.len() - 1;
        let base: String = wanted.chars().take(room).collect();
        slug(&format!("{base}-{suffix}"))
    };
    std::iter::once(agent.to_owned())
        .filter(|a| !a.is_empty())
        .chain((2..100).map(|n| n.to_string()))
        .map(|suffix| fit(&suffix))
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| fit(&uuid::Uuid::new_v4().simple().to_string()[..8]))
}

/// Turns the model's answer into checked drafts for this request.
pub(crate) fn read(content: &str, request: &Request, context: &WriterContext) -> Result<Answer> {
    let raw = object(content)?;
    let own: HashSet<&str> = request.skills.iter().map(|s| s.name.as_str()).collect();
    let mut taken: HashSet<String> = context.names.iter().cloned().collect();
    let agent = slug(&request.name);
    let mut notes = vec![];
    let mut skills: Vec<Draft> = vec![];
    for (index, skill) in raw.skills.into_iter().take(MAX_SKILLS).enumerate() {
        let mut name = slug(&skill.name);
        if name.is_empty() {
            name = format!("skill-{}", index + 1);
        }
        let description = one_line(&skill.description, 1024);
        let steps = skill
            .steps
            .trim()
            .lines()
            .take(MAX_STEP_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        if description.is_empty() || steps.is_empty() {
            notes.push(format!(
                "{name} was left out: it had no description or no steps."
            ));
            continue;
        }
        let mine = own.contains(name.as_str()) || skills.iter().any(|d| d.name == name);
        if !mine && taken.contains(&name) {
            let wanted = name;
            name = free_name(&wanted, &agent, &taken);
            notes.push(format!("A skill named {wanted} already exists in this workspace, so this one is called {name}."));
        }
        let text = skill_md(&name, &description, &steps);
        match check(&[SkillFile {
            path: "SKILL.md".into(),
            text: text.clone(),
        }]) {
            Ok(checked) => {
                taken.insert(name.clone());
                skills.retain(|d| d.name != name);
                skills.push(Draft {
                    name,
                    description: checked.description,
                    steps,
                    text,
                });
            }
            Err(error) => notes.push(format!("{name} was left out: {error}.")),
        }
    }
    let ground_rules = raw
        .ground_rules
        .map(|text| within_bytes(text.trim(), INSTRUCTIONS_LIMIT))
        .filter(|text| !text.is_empty());
    let mut remove: Vec<String> = raw
        .remove
        .iter()
        .map(|n| slug(n))
        .filter(|n| own.contains(n.as_str()))
        .collect();
    remove.dedup();
    let mut reuse = vec![];
    for name in raw.reuse.iter().map(|n| slug(n)) {
        if let Some(skill) = context.library.iter().find(|s| s.name == name)
            && !reuse.iter().any(|r: &Value| r["id"] == skill.id.as_str())
        {
            reuse.push(
                json!({"id": skill.id, "name": skill.name, "description": skill.description}),
            );
        }
    }
    let missing_apps = raw
        .missing_apps
        .iter()
        .map(|a| one_line(a, 80))
        .filter(|a| !a.is_empty())
        .take(5)
        .collect();
    let mut reply: String = raw.reply.trim().chars().take(2000).collect();
    if reply.is_empty() {
        reply = if skills.is_empty() && ground_rules.is_none() {
            "I couldn't write anything from that. Tell me more about the job.".into()
        } else {
            "Here are the drafts. Tell me what to change.".into()
        };
    }
    Ok(Answer {
        reply,
        ground_rules,
        skills,
        remove,
        reuse,
        missing_apps,
        notes,
    })
}
