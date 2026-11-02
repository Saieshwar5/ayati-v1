//! Bindings: how a panel's numbers come from its reviewed source reads, so the
//! controller can refresh them without a model call (people-and-agents-first,
//! Phase 3 HP3). The agent proposes them with an update; the controller keeps them
//! only if they reproduce exactly what the agent published from the same reads.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    /// The number or text at `path`.
    #[default]
    Value,
    /// How many items the list at `path` has.
    Count,
    /// The sum of `field` over the list at `path`.
    Sum,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// Which of the panel's sections (0 to 2); the first by default.
    #[serde(default, skip_serializing_if = "is_first")]
    pub section: usize,
    /// Where in that section: `value` (metric), `items.N.value` (metrics),
    /// `items.N.actual` (progress), `stages.N.count` (pipeline) or `rows` (table).
    pub target: String,
    /// Which reviewed read, by its position in the update's reads.
    pub read: usize,
    /// Dot path into the read's result (`orders`, `totals.revenue`, `items.0.count`);
    /// empty for the result itself.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub op: Op,
    /// For `sum`: the field of each item to add up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// For `rows`: a path within each item for every column.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    /// Decimal places for numbers (0–2); whole numbers show none by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    /// Digit grouping: `thousands` (48,200) or `indian` (4,82,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
}

const MAX_ROWS: usize = 50;

fn is_first(section: &usize) -> bool {
    *section == 0
}

fn of(bindings: &[Binding], section: usize) -> Vec<Binding> {
    bindings
        .iter()
        .filter(|b| b.section == section)
        .cloned()
        .collect()
}

/// `validate` for a panel with several sections (HP6).
pub fn validate_all(bindings: &[Binding], reads: usize, sections: &[Value]) -> Result<()> {
    ensure!((1..=16).contains(&bindings.len()), "Give 1 to 16 bindings");
    ensure!(
        bindings.iter().all(|b| b.section < sections.len()),
        "Bind only the panel's own sections"
    );
    for (i, section) in sections.iter().enumerate() {
        let list = of(bindings, i);
        if !list.is_empty() {
            validate(&list, reads, section)?;
        }
    }
    Ok(())
}

/// `apply` for every section.
pub fn apply_all(
    sections: &[Value],
    bindings: &[Binding],
    results: &[Value],
) -> Result<Vec<Value>> {
    sections
        .iter()
        .enumerate()
        .map(|(i, section)| {
            let list = of(bindings, i);
            if list.is_empty() {
                Ok(section.clone())
            } else {
                apply(section, &list, results)
            }
        })
        .collect()
}

/// `check` for every section.
pub fn check_all(published: &[Value], bindings: &[Binding], results: &[Value]) -> Result<()> {
    for (i, section) in published.iter().enumerate() {
        let list = of(bindings, i);
        if !list.is_empty() {
            check(section, &list, results)?;
        }
    }
    Ok(())
}

/// `meaningful` for any section.
pub fn meaningful_all(old: &[Value], new: &[Value], bindings: &[Binding], share: f64) -> bool {
    old.len() != new.len()
        || old.iter().zip(new).enumerate().any(|(i, (a, z))| {
            let list = of(bindings, i);
            !list.is_empty() && meaningful(a, z, &list, share)
        })
}

pub fn validate(bindings: &[Binding], reads: usize, section: &Value) -> Result<()> {
    ensure!((1..=16).contains(&bindings.len()), "Give 1 to 16 bindings");
    let mut targets = std::collections::HashSet::new();
    for b in bindings {
        ensure!(
            b.read < reads,
            "Binding read {} is not one of the reviewed reads",
            b.read
        );
        ensure!(targets.insert(b.target.as_str()), "Bind {} once", b.target);
        ensure!(
            b.path.len() <= 200 && b.prefix.chars().count() <= 8 && b.suffix.chars().count() <= 8,
            "Keep paths to 200 characters and prefixes or suffixes to 8"
        );
        ensure!(b.decimals.is_none_or(|d| d <= 2), "Use 0 to 2 decimals");
        ensure!(
            b.group
                .as_deref()
                .is_none_or(|g| ["thousands", "indian"].contains(&g)),
            "Group digits by thousands or indian"
        );
        ensure!(
            (b.op == Op::Sum) == b.field.is_some(),
            "A sum needs the field to add, and only a sum has one"
        );
        target(section, &b.target, b)?;
    }
    Ok(())
}

/// Where a binding writes, checked against the section's kind.
fn target<'a>(section: &'a Value, target: &str, b: &Binding) -> Result<&'a str> {
    let kind = section["kind"].as_str().unwrap_or_default();
    let parts: Vec<&str> = target.split('.').collect();
    match (kind, parts.as_slice()) {
        ("metric", ["value"]) => Ok("text"),
        ("metrics", ["items", n, "value"]) | ("progress", ["items", n, "actual"]) => {
            let n: usize = n.parse().context("Item number")?;
            ensure!(
                section["items"].get(n).is_some(),
                "No item {n} in this panel"
            );
            Ok(if kind == "progress" { "number" } else { "text" })
        }
        ("pipeline", ["stages", n, "count"]) => {
            let n: usize = n.parse().context("Stage number")?;
            ensure!(
                section["stages"].get(n).is_some(),
                "No stage {n} in this panel"
            );
            Ok("number")
        }
        ("table", ["rows"]) => {
            ensure!(
                b.columns.len() == section["columns"].as_array().map_or(0, Vec::len),
                "Give one path per table column"
            );
            Ok("rows")
        }
        _ => bail!("A {kind} panel can't bind {target}"),
    }
}

/// A connector's result: MCP structured content, JSON text content, or the value.
pub fn normalize(result: &Value) -> Value {
    if result["structuredContent"].is_object() {
        return result["structuredContent"].clone();
    }
    if let Some(text) = result["content"][0]["text"].as_str()
        && let Ok(parsed) = serde_json::from_str::<Value>(text)
    {
        return parsed;
    }
    result.clone()
}

fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    path.split('.').try_fold(value, |v, key| match v {
        Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
        _ => v.get(key),
    })
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().replace(',', "").parse().ok())
}

fn group(digits: &str, style: &str) -> String {
    let bytes: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    let len = bytes.len();
    for (i, c) in bytes.iter().enumerate() {
        let left = len - i;
        let comma = match style {
            "indian" => left >= 3 && (left - 3).is_multiple_of(2),
            _ => left.is_multiple_of(3),
        };
        if i > 0 && comma {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

pub fn format(n: f64, b: &Binding) -> String {
    let decimals = b
        .decimals
        .map(usize::from)
        .unwrap_or(if n.fract() == 0.0 { 0 } else { 2 });
    let text = format!("{:.*}", decimals, n.abs());
    let (whole, fraction) = text
        .split_once('.')
        .map_or((text.as_str(), ""), |(w, f)| (w, f));
    let whole = b
        .group
        .as_deref()
        .map_or(whole.to_string(), |g| group(whole, g));
    let sign = if n < 0.0 && text.chars().any(|c| c != '0' && c != '.') {
        "-"
    } else {
        ""
    };
    let body = if fraction.is_empty() {
        whole
    } else {
        format!("{whole}.{fraction}")
    };
    format!("{}{sign}{body}{}", b.prefix, b.suffix)
}

fn text(value: &Value, b: &Binding) -> Option<String> {
    match value {
        Value::String(s)
            if number(value).is_none() || (b.decimals.is_none() && b.group.is_none()) =>
        {
            Some(format!("{}{s}{}", b.prefix, b.suffix))
        }
        Value::Bool(v) => Some(if *v { "Yes" } else { "No" }.into()),
        Value::Null => Some(String::new()),
        _ => number(value).map(|n| format(n, b)),
    }
}

/// The value one binding produces from the reads' results.
fn produce(b: &Binding, results: &[Value], kind: &str) -> Result<Value> {
    let result = normalize(results.get(b.read).context("Missing read result")?);
    let found = at(&result, &b.path)
        .with_context(|| format!("Nothing at {} in read {}", b.path, b.read))?;
    let n = match b.op {
        Op::Value => None,
        Op::Count => Some(found.as_array().context("Count needs a list")?.len() as f64),
        Op::Sum => {
            let field = b.field.as_deref().unwrap_or_default();
            let mut total = 0.0;
            for item in found.as_array().context("Sum needs a list")? {
                total += at(item, field)
                    .and_then(number)
                    .with_context(|| format!("No number at {field}"))?;
            }
            Some(total)
        }
    };
    Ok(match kind {
        "number" => json!(n.or_else(|| number(found)).context("Not a number")?),
        "rows" => {
            let items = found.as_array().context("Rows need a list")?;
            let rows: Vec<Vec<String>> = items
                .iter()
                .take(MAX_ROWS)
                .map(|item| {
                    b.columns
                        .iter()
                        .map(|c| at(item, c).and_then(|v| text(v, b)).unwrap_or_default())
                        .collect()
                })
                .collect();
            json!(rows)
        }
        _ => json!(match n {
            Some(n) => format(n, b),
            None => text(found, b).context("Not a value")?,
        }),
    })
}

/// Fill the section's bound values from the reads' results.
pub fn apply(section: &Value, bindings: &[Binding], results: &[Value]) -> Result<Value> {
    let mut out = section.clone();
    for b in bindings {
        let kind = target(section, &b.target, b)?;
        let value = produce(b, results, kind)?;
        let mut place = &mut out;
        for key in b.target.split('.') {
            place = match key.parse::<usize>() {
                Ok(i) => place.get_mut(i).context("Missing item")?,
                Err(_) => place.get_mut(key).context("Missing field")?,
            };
        }
        *place = value;
    }
    Ok(out)
}

/// Bindings are kept only if they reproduce exactly what was published.
pub fn check(published: &Value, bindings: &[Binding], results: &[Value]) -> Result<()> {
    let produced = apply(published, bindings, results)?;
    for b in bindings {
        let path = b.target.replace('.', "/");
        let (want, got) = (
            published.pointer(&format!("/{path}")),
            produced.pointer(&format!("/{path}")),
        );
        ensure!(
            want == got,
            "{} would be {} from the read, but you published {}",
            b.target,
            got.map_or("nothing".into(), Value::to_string),
            want.map_or("nothing".into(), Value::to_string)
        );
    }
    Ok(())
}

/// Did a bound number move by more than `share` (0.10 = 10%), or a table change size?
pub fn meaningful(old: &Value, new: &Value, bindings: &[Binding], share: f64) -> bool {
    bindings.iter().any(|b| {
        let path = format!("/{}", b.target.replace('.', "/"));
        let (Some(a), Some(z)) = (old.pointer(&path), new.pointer(&path)) else {
            return true;
        };
        if b.target == "rows" {
            return a.as_array().map(Vec::len) != z.as_array().map(Vec::len);
        }
        let digits = |v: &Value| {
            number(v).or_else(|| {
                let s: String = v
                    .as_str()?
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                    .collect();
                s.parse().ok()
            })
        };
        match (digits(a), digits(z)) {
            (Some(x), Some(y)) => (y - x).abs() > share * x.abs().max(1.0),
            _ => a != z,
        }
    })
}

#[cfg(test)]
#[path = "bindings_tests.rs"]
mod tests;
