//! Actions a built app offers agents (Apps AP10): each is one thing an agent may do in
//! the app for a person — create a record, change listed fields of one, or read — named
//! in `cuckoo-app.json`. An agent runs an action only if the app's builder or an admin
//! allowed it, and never with more access than the person it works for.
use super::manifest::Collection;
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MAX_ACTIONS: usize = 20;
const DOES: [&str; 3] = ["create", "update", "read"];

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub id: String,
    pub title: String,
    pub collection: String,
    /// create, update or read.
    pub does: String,
    /// The fields an agent may set (create, update); empty for read.
    pub fields: Vec<String>,
}

/// Reads `actions` from the manifest: each names a declared collection and fields.
pub fn parse(value: &Value, collections: &BTreeMap<String, Collection>) -> Result<Vec<Action>> {
    let list = match &value["actions"] {
        Value::Null => return Ok(vec![]),
        Value::Array(list) => list,
        _ => bail!("actions in cuckoo-app.json is a list"),
    };
    ensure!(
        list.len() <= MAX_ACTIONS,
        "An app offers at most {MAX_ACTIONS} actions"
    );
    let mut actions: Vec<Action> = vec![];
    for spec in list {
        let id = spec["id"].as_str().unwrap_or("");
        super::manifest::name("Action", id)?;
        ensure!(
            actions.iter().all(|a| a.id != id),
            "Two actions are called \"{id}\""
        );
        let title = spec["title"].as_str().unwrap_or("").trim();
        ensure!(
            !title.is_empty() && title.chars().count() <= 80,
            "Action \"{id}\" has a title of up to 80 characters"
        );
        let collection = spec["collection"].as_str().unwrap_or("");
        let declared = collections.get(collection).ok_or_else(|| {
            anyhow::anyhow!(
                "Action \"{id}\" uses \"{collection}\", which isn't one of the app's collections"
            )
        })?;
        let does = spec["does"].as_str().unwrap_or("");
        ensure!(
            DOES.contains(&does),
            "Action \"{id}\" does create, update or read"
        );
        let fields: Vec<String> = spec["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str().map(str::to_owned))
            .collect();
        for field in &fields {
            ensure!(
                declared.fields.contains_key(field),
                "Action \"{id}\" sets \"{field}\", which \"{collection}\" doesn't have"
            );
        }
        ensure!(
            does == "read" || !fields.is_empty(),
            "Action \"{id}\" lists the fields it sets"
        );
        actions.push(Action {
            id: id.into(),
            title: title.into(),
            collection: collection.into(),
            does: does.into(),
            fields: if does == "read" { vec![] } else { fields },
        });
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::super::manifest::Manifest;

    #[test]
    fn actions_name_a_collection_and_the_fields_they_set() {
        let base = r#"{"name":"Leave","roles":["user"],"collections":{"requests":{"fields":{"days":"number","approved":"boolean"},"access":{"user":["create","read_own"]}}},"actions":"#;
        let with = |actions: &str| Manifest::parse(&format!("{base}{actions}}}"));
        let m = with(r#"[{"id":"file_leave","title":"File a leave request","collection":"requests","does":"create","fields":["days"]},{"id":"my_requests","title":"See my requests","collection":"requests","does":"read"}]"#).unwrap();
        assert_eq!(m.actions.len(), 2);
        assert_eq!(m.actions[0].fields, ["days"]);
        let said = |actions: &str| with(actions).unwrap_err().to_string();
        assert!(
            said(r#"[{"id":"x","title":"X","collection":"other","does":"read"}]"#)
                .contains("isn't one of the app's collections")
        );
        assert!(said(r#"[{"id":"x","title":"X","collection":"requests","does":"create","fields":["salary"]}]"#).contains("doesn't have"));
        assert!(
            said(r#"[{"id":"x","title":"X","collection":"requests","does":"delete"}]"#)
                .contains("create, update or read")
        );
        assert!(
            said(r#"[{"id":"x","title":"X","collection":"requests","does":"create"}]"#)
                .contains("lists the fields")
        );
        assert!(said(r#"[{"id":"x","title":"X","collection":"requests","does":"read"},{"id":"x","title":"Y","collection":"requests","does":"read"}]"#).contains("Two actions"));
    }
}
