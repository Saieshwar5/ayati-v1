use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub request: String,
    pub revision: i64,
}
#[derive(Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub request: String,
    pub revision: i64,
    pub order: Vec<String>,
    pub wide: Vec<String>,
    pub hidden: Vec<String>,
}
impl Layout {
    pub fn validate(&self) -> Result<()> {
        uuid::Uuid::parse_str(&self.request)?;
        ensure!(
            self.revision >= 0 && self.revision < i64::MAX,
            "Invalid layout revision"
        );
        for values in [&self.order, &self.wide, &self.hidden] {
            ensure!(
                values.len() <= 32,
                "Personal Home supports at most 32 panel placements"
            );
            let mut seen = std::collections::HashSet::new();
            for id in values {
                ensure!(
                    uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == *id)
                        && seen.insert(id),
                    "Panel placement IDs must be unique canonical UUIDs"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_is_bounded_and_cannot_select_another_owner() {
        let id = uuid::Uuid::new_v4().to_string();
        let mut layout = Layout {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            order: vec![id.clone()],
            wide: vec![],
            hidden: vec![],
        };
        assert!(layout.validate().is_ok());
        layout.order.push(id);
        assert!(layout.validate().is_err());
        assert!(serde_json::from_value::<Layout>(serde_json::json!({"request":layout.request,"revision":0,"order":[],"wide":[],"hidden":[],"owner":"forged"})).is_err());
    }
}

/// Who sees a panel (team panels, Phase 3 HP4): `personal`, `channel` (with `chat`)
/// or `workspace`. Only its owner chooses.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Share {
    pub request: String,
    pub revision: i64,
    pub audience: String,
    #[serde(default)]
    pub chat: Option<String>,
}
