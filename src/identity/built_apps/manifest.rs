//! A built app's manifest (Apps BA2): the roles people can hold in the app, the
//! collections of records it keeps, each collection's fields, and what each role may do
//! with them. The server enforces it on every data request; the app's own code is never
//! trusted to.
use anyhow::{Result, bail, ensure};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const MAX_COLLECTIONS: usize = 20;
pub const MAX_FIELDS: usize = 40;
pub const MAX_RECORDS: i64 = 5000;
pub const MAX_RECORD: usize = 16 * 1024;
const TYPES: [&str; 4] = ["text", "number", "date", "boolean"];
/// What a role may do with a collection: everything, or only records they created.
pub const PERMISSIONS: [&str; 7] = [
    "create",
    "read",
    "read_own",
    "update",
    "update_own",
    "delete",
    "delete_own",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    pub fields: BTreeMap<String, String>,
    pub access: BTreeMap<String, Vec<String>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub roles: Vec<String>,
    pub collections: BTreeMap<String, Collection>,
    /// What agents may be allowed to do in the app (AP10).
    pub actions: Vec<super::actions::Action>,
}

pub(super) fn name(kind: &str, text: &str) -> Result<()> {
    ensure!(
        !text.is_empty()
            && text.len() <= 40
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !text.starts_with(|c: char| c.is_ascii_digit()),
        "{kind} \"{text}\" uses lowercase letters, numbers and _ only"
    );
    Ok(())
}

/// A calendar date written 2026-11-05.
fn date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return false;
    };
    let numbers = (y.parse::<u32>(), m.parse::<u32>(), d.parse::<u32>());
    let (Ok(y), Ok(m), Ok(d)) = numbers else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    text.len() == 10 && (1..=12).contains(&m) && d >= 1 && d <= days[m as usize - 1]
}

impl Manifest {
    /// Reads and checks a manifest; the error says, in plain words, what is wrong.
    pub fn parse(text: &str) -> Result<Self> {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            bail!("cuckoo-app.json is not valid JSON")
        };
        ensure!(
            value["name"].as_str().is_some_and(|n| !n.is_empty()),
            "cuckoo-app.json needs a name"
        );
        let roles: Vec<String> = match value["roles"].as_array() {
            Some(list) if !list.is_empty() && list.len() <= 10 => list
                .iter()
                .map(|r| r.as_str().map(str::to_owned))
                .collect::<Option<_>>()
                .unwrap_or_default(),
            _ => vec![],
        };
        ensure!(!roles.is_empty(), "cuckoo-app.json lists one to ten roles");
        for role in &roles {
            name("Role", role)?;
        }
        let empty = Map::new();
        let declared = match &value["collections"] {
            Value::Null => &empty,
            Value::Object(map) => map,
            _ => bail!("collections in cuckoo-app.json is an object"),
        };
        ensure!(
            declared.len() <= MAX_COLLECTIONS,
            "An app keeps at most {MAX_COLLECTIONS} collections"
        );
        let mut collections = BTreeMap::new();
        for (key, spec) in declared {
            name("Collection", key)?;
            let fields_value = spec["fields"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Collection \"{key}\" lists its fields"))?;
            ensure!(
                !fields_value.is_empty() && fields_value.len() <= MAX_FIELDS,
                "Collection \"{key}\" has 1 to {MAX_FIELDS} fields"
            );
            let mut fields = BTreeMap::new();
            for (field, kind) in fields_value {
                name("Field", field)?;
                let kind = kind.as_str().filter(|k| TYPES.contains(k)).ok_or_else(|| {
                    anyhow::anyhow!("Field \"{key}.{field}\" is text, number, date or boolean")
                })?;
                fields.insert(field.clone(), kind.to_owned());
            }
            let mut access = BTreeMap::new();
            for (role, allowed) in spec["access"].as_object().into_iter().flatten() {
                ensure!(
                    roles.contains(role),
                    "Collection \"{key}\" gives access to \"{role}\", which isn't one of the app's roles"
                );
                let list: Vec<String> = allowed
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p.as_str().map(str::to_owned))
                    .collect();
                for permission in &list {
                    ensure!(
                        PERMISSIONS.contains(&permission.as_str()),
                        "\"{permission}\" isn't a permission; use {}",
                        PERMISSIONS.join(", ")
                    );
                }
                access.insert(role.clone(), list);
            }
            collections.insert(key.clone(), Collection { fields, access });
        }
        let actions = super::actions::parse(&value, &collections)?;
        Ok(Self {
            roles,
            collections,
            actions,
        })
    }

    /// Everyone who may open the app holds its first role.
    pub fn default_role(&self) -> &str {
        &self.roles[0]
    }

    /// Whether these roles may do `action` (create, read, update, delete) on the
    /// collection: `Some(true)` for any record, `Some(false)` only for their own records.
    pub fn allows(&self, roles: &[String], collection: &str, action: &str) -> Option<bool> {
        let spec = self.collections.get(collection)?;
        let granted: Vec<&String> = roles
            .iter()
            .filter_map(|r| spec.access.get(r))
            .flatten()
            .collect();
        if granted.iter().any(|p| *p == action) {
            return Some(true);
        }
        (action != "create" && granted.iter().any(|p| **p == format!("{action}_own")))
            .then_some(false)
    }

    /// A record checked against its collection's fields: no unknown fields, the right
    /// types, bounded size. Missing fields are allowed.
    pub fn record(&self, collection: &str, data: &Value) -> Result<()> {
        let spec = self
            .collections
            .get(collection)
            .ok_or_else(|| anyhow::anyhow!("The app has no collection \"{collection}\""))?;
        let map = data
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("A record is an object of fields"))?;
        ensure!(
            serde_json::to_string(data)?.len() <= MAX_RECORD,
            "A record is at most 16 KiB"
        );
        for (field, value) in map {
            let kind = spec
                .fields
                .get(field)
                .ok_or_else(|| anyhow::anyhow!("\"{collection}\" has no field \"{field}\""))?;
            let fits = match (kind.as_str(), value) {
                (_, Value::Null) => true,
                ("text", Value::String(s)) => s.chars().count() <= 5000,
                ("number", Value::Number(_)) => true,
                ("boolean", Value::Bool(_)) => true,
                ("date", Value::String(s)) => date(s),
                _ => false,
            };
            ensure!(
                fits,
                "\"{collection}.{field}\" is a {kind}{}",
                if kind == "date" {
                    " like 2026-11-05"
                } else {
                    ""
                }
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Manifest;
    use serde_json::json;

    const LEAVE: &str = r#"{"name":"Leave","roles":["user","approver"],"collections":{"requests":{"fields":{"start":"date","days":"number","reason":"text","approved":"boolean"},
        "access":{"user":["create","read_own","update_own"],"approver":["read","update"]}}}}"#;

    #[test]
    fn roles_grant_everything_or_only_their_own_records() {
        let m = Manifest::parse(LEAVE).unwrap();
        let user = vec!["user".to_owned()];
        let both = vec!["user".to_owned(), "approver".to_owned()];
        assert_eq!(m.default_role(), "user");
        assert_eq!(m.allows(&user, "requests", "create"), Some(true));
        assert_eq!(m.allows(&user, "requests", "read"), Some(false));
        assert_eq!(m.allows(&user, "requests", "delete"), None);
        assert_eq!(m.allows(&both, "requests", "read"), Some(true));
        assert_eq!(m.allows(&user, "other", "read"), None);
        assert!(
            m.record(
                "requests",
                &json!({"start":"2026-11-10","days":2,"reason":"Trip"})
            )
            .is_ok()
        );
        assert!(
            m.record("requests", &json!({"start":"10/11/2026"}))
                .is_err()
        );
        assert!(
            m.record("requests", &json!({"start":"2026-02-30"}))
                .is_err()
        );
        assert!(m.record("requests", &json!({"salary":1})).is_err());
        assert!(m.record("requests", &json!({"days":"two"})).is_err());
    }

    #[test]
    fn a_broken_manifest_says_what_is_wrong() {
        let said = |text: &str| Manifest::parse(text).unwrap_err().to_string();
        assert_eq!(said("{"), "cuckoo-app.json is not valid JSON");
        assert_eq!(
            said(r#"{"name":"A","roles":[]}"#),
            "cuckoo-app.json lists one to ten roles"
        );
        assert!(
            said(r#"{"name":"A","roles":["user"],"collections":{"x":{"fields":{"a":"money"}}}}"#)
                .contains("text, number, date or boolean")
        );
        assert!(said(r#"{"name":"A","roles":["user"],"collections":{"x":{"fields":{"a":"text"},"access":{"boss":["read"]}}}}"#).contains("isn't one of the app's roles"));
        assert!(said(r#"{"name":"A","roles":["user"],"collections":{"x":{"fields":{"a":"text"},"access":{"user":["write"]}}}}"#).contains("isn't a permission"));
        assert!(Manifest::parse(r#"{"name":"A","roles":["user"],"collections":{}}"#).is_ok());
    }
}
