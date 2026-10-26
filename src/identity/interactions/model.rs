//! Bounded declarative questions. Values are task data, never action authority.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OptionItem {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Field {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub options: Vec<OptionItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Specification {
    pub version: u8,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fallback: String,
    #[serde(default)]
    pub options: Vec<OptionItem>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub allow_other: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_choices: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_choices: Option<usize>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ask {
    pub key: String,
    pub specification: Specification,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Submission {
    pub id: String,
    pub revision: i64,
    pub decision: String,
    pub values: Value,
}
pub(super) fn key(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Use a stable key up to 64 letters, numbers, underscores or hyphens"
    );
    Ok(())
}
fn text(value: &str, max: usize, required: bool) -> Result<()> {
    ensure!(
        value.chars().count() <= max
            && !value.contains('\0')
            && (!required || !value.trim().is_empty()),
        "Text is empty or exceeds its supported length"
    );
    Ok(())
}
fn options(values: &[OptionItem]) -> Result<()> {
    ensure!(
        !values.is_empty() && values.len() <= 12,
        "Choose 1–12 options"
    );
    let mut ids = HashSet::new();
    for option in values {
        key(&option.id)?;
        text(&option.label, 160, true)?;
        ensure!(ids.insert(&option.id), "Option IDs must be unique");
    }
    Ok(())
}
impl Ask {
    pub fn validate(&self) -> Result<()> {
        key(&self.key)?;
        self.specification.validate()
    }
}
impl Specification {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && serde_json::to_vec(self)?.len() <= 16384,
            "Unsupported question version or size"
        );
        text(&self.title, 240, true)?;
        text(&self.description, 1000, false)?;
        text(&self.fallback, 1200, false)?;
        match self.kind.as_str() {
            "single_choice" | "multiple_choice" => {
                options(&self.options)?;
                ensure!(
                    self.fields.is_empty(),
                    "Choice questions cannot contain form fields"
                );
                let (min, max) = self.selection_bounds();
                ensure!(
                    min > 0
                        && min <= max
                        && max <= self.options.len() + usize::from(self.allow_other),
                    "Invalid selection bounds"
                );
                ensure!(
                    self.kind == "multiple_choice"
                        || (self.min_choices.is_none() && self.max_choices.is_none()),
                    "Only multiple choice uses selection bounds"
                );
            }
            "form" => {
                ensure!(
                    self.options.is_empty()
                        && !self.allow_other
                        && self.min_choices.is_none()
                        && self.max_choices.is_none(),
                    "Forms use fields rather than question options"
                );
                ensure!(
                    !self.fields.is_empty() && self.fields.len() <= 8,
                    "Use 1–8 form fields"
                );
                let mut ids = HashSet::new();
                for field in &self.fields {
                    ensure!(ids.insert(&field.id), "Field IDs must be unique");
                    field.validate()?;
                }
            }
            _ => anyhow::bail!("Unsupported question kind"),
        }
        Ok(())
    }
    pub fn selection_bounds(&self) -> (usize, usize) {
        if self.kind == "single_choice" {
            (1, 1)
        } else {
            (
                self.min_choices.unwrap_or(1),
                self.max_choices
                    .unwrap_or(self.options.len() + usize::from(self.allow_other)),
            )
        }
    }
    pub fn answer(&self, values: &Value) -> Result<Value> {
        ensure!(
            serde_json::to_vec(values)?.len() <= 8192,
            "Answer exceeds supported size"
        );
        let data = values
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Answer must be an object"))?;
        if self.kind == "form" {
            ensure!(
                data.keys()
                    .all(|id| self.fields.iter().any(|field| &field.id == id)),
                "Answer contains unknown fields"
            );
            for field in &self.fields {
                if let Some(value) = data.get(&field.id) {
                    field
                        .value(value)
                        .map_err(|e| anyhow::anyhow!("{}: {e}", field.label))?;
                } else {
                    ensure!(!field.required, "{} is required", field.label);
                }
            }
            return Ok(values.clone());
        }
        ensure!(
            data.keys()
                .all(|id| ["choices", "other"].contains(&id.as_str())),
            "Answer contains unknown fields"
        );
        let selected = data
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Choose options using their saved IDs"))?;
        let mut ids = std::collections::BTreeSet::new();
        for value in selected {
            let id = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid option ID"))?;
            ensure!(
                self.options.iter().any(|option| option.id == id) && ids.insert(id),
                "Unknown or repeated option"
            );
        }
        let other = data
            .get("other")
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| anyhow::anyhow!("Alternative must be text"))
            })
            .transpose()?;
        if let Some(other) = other {
            ensure!(self.allow_other, "This question uses closed choices");
            text(other, 1000, true)?;
        }
        let count = ids.len() + usize::from(other.is_some());
        let (min, max) = self.selection_bounds();
        ensure!(
            (min..=max).contains(&count),
            "Choose between {min} and {max} answers"
        );
        let mut result = serde_json::json!({"choices":ids});
        if let Some(other) = other {
            result["other"] = Value::String(other.into());
        }
        Ok(result)
    }
}
impl Field {
    fn validate(&self) -> Result<()> {
        key(&self.id)?;
        text(&self.label, 160, true)?;
        text(&self.description, 500, false)?;
        ensure!(
            ["text", "multiline", "number", "date", "boolean", "choice"]
                .contains(&self.kind.as_str()),
            "Unsupported form field"
        );
        if self.kind == "choice" {
            options(&self.options)?;
        } else {
            ensure!(self.options.is_empty(), "Only a choice field has options");
        }
        ensure!(
            self.kind == "number" || (self.min.is_none() && self.max.is_none()),
            "Only numbers have numeric bounds"
        );
        ensure!(
            self.min
                .into_iter()
                .chain(self.max)
                .all(|n| n.is_finite() && n.abs() <= 1e12)
                && self.min.zip(self.max).is_none_or(|(min, max)| min <= max),
            "Invalid numeric bounds"
        );
        ensure!(
            self.max_length
                .is_none_or(|n| ["text", "multiline"].contains(&self.kind.as_str())
                    && n > 0
                    && n <= self.text_limit()),
            "Invalid text limit"
        );
        if let Some(value) = &self.suggested {
            self.value(value)?;
        }
        Ok(())
    }
    fn text_limit(&self) -> usize {
        if self.kind == "multiline" { 4000 } else { 1000 }
    }
    fn value(&self, value: &Value) -> Result<()> {
        match self.kind.as_str() {
            "boolean" => ensure!(value.is_boolean(), "Choose Yes or No"),
            "number" => {
                let n = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("Enter a number"))?;
                ensure!(
                    n.is_finite()
                        && n.abs() <= 1e12
                        && self.min.is_none_or(|min| n >= min)
                        && self.max.is_none_or(|max| n <= max),
                    "Number is outside the allowed bounds"
                );
            }
            "text" | "multiline" => text(
                value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Enter text"))?,
                self.max_length.unwrap_or(self.text_limit()),
                self.required,
            )?,
            "date" => ensure!(
                value.as_str().is_some_and(valid_date),
                "Enter a valid calendar date (YYYY-MM-DD)"
            ),
            "choice" => ensure!(
                self.options
                    .iter()
                    .any(|option| value.as_str() == Some(&option.id)),
                "Choose a saved option"
            ),
            _ => anyhow::bail!("Unsupported field"),
        }
        Ok(())
    }
}
fn valid_date(text: &str) -> bool {
    if text.len() != 10 || text.as_bytes()[4] != b'-' || text.as_bytes()[7] != b'-' {
        return false;
    }
    if !text.bytes().enumerate().all(|(i, b)| {
        if i == 4 || i == 7 {
            b == b'-'
        } else {
            b.is_ascii_digit()
        }
    }) {
        return false;
    }
    let parts = text
        .split('-')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>();
    let Ok(parts) = parts else { return false };
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    };
    (1..=9999).contains(&year) && day > 0 && day <= max
}
