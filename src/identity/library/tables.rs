//! Library tables (LB4): a key column and up to 19 more, text or number, up to 5,000
//! rows. Agents look rows up exactly by key, and company rules can compare a request
//! with a table's value, which text search can't do reliably for numbers.
use anyhow::{Result, ensure};
use serde_json::Value;

pub const MAX_COLUMNS: usize = 20;
pub const MAX_ROWS: usize = 5000;

/// The key as it is matched: trimmed and case-insensitive.
pub fn key(value: &Value) -> String {
    match value {
        Value::String(s) => s.trim().to_lowercase(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// Checks a table and returns its text for search, reading and versions.
pub fn validate(table: &Value) -> Result<String> {
    ensure!(table["type"] == "table", "Not a table");
    ensure!(
        table.to_string().len() <= 1_048_576,
        "A table can be at most 1 MiB"
    );
    let columns = table["columns"]
        .as_array()
        .filter(|c| (1..=MAX_COLUMNS).contains(&c.len()));
    let Some(columns) = columns else {
        anyhow::bail!("A table has 1 to {MAX_COLUMNS} columns");
    };
    let mut names = std::collections::HashSet::new();
    for column in columns {
        let name = column["name"].as_str().unwrap_or("").trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 60,
            "Name every column, in up to 60 characters"
        );
        ensure!(
            names.insert(name.to_lowercase()),
            "Column names must differ"
        );
        ensure!(
            ["text", "number"].contains(&column["type"].as_str().unwrap_or("")),
            "Columns hold text or numbers"
        );
        ensure!(
            column
                .as_object()
                .is_some_and(|c| c.keys().all(|k| k == "name" || k == "type")),
            "Unsupported column"
        );
    }
    ensure!(
        columns[0]["type"] == "text",
        "The first column is the key and holds text"
    );
    let rows = table["rows"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    ensure!(rows.len() <= MAX_ROWS, "A table has up to {MAX_ROWS} rows");
    ensure!(
        table.as_object().is_some_and(|t| t
            .keys()
            .all(|k| ["type", "columns", "rows"].contains(&k.as_str()))),
        "Unsupported table"
    );
    let mut keys = std::collections::HashSet::new();
    let mut text = String::new();
    for row in rows {
        let cells = row.as_array().filter(|r| r.len() == columns.len());
        let Some(cells) = cells else {
            anyhow::bail!("Every row has one value per column");
        };
        let k = key(&cells[0]);
        ensure!(!k.is_empty(), "Every row has a key in the first column");
        ensure!(
            keys.insert(k),
            "Keys must differ: {} appears twice",
            cells[0].as_str().unwrap_or("")
        );
        let mut line = vec![];
        for (cell, column) in cells.iter().zip(columns) {
            match (column["type"].as_str(), cell) {
                (_, Value::Null) => {}
                (Some("number"), Value::Number(n)) => {
                    line.push(format!("{}: {n}", column["name"].as_str().unwrap_or("")))
                }
                (Some("text"), Value::String(s)) if s.chars().count() <= 500 => {
                    if !s.trim().is_empty() {
                        line.push(format!(
                            "{}: {}",
                            column["name"].as_str().unwrap_or(""),
                            s.trim()
                        ));
                    }
                }
                _ => anyhow::bail!(
                    "{} holds {}",
                    column["name"].as_str().unwrap_or("A column"),
                    if column["type"] == "number" {
                        "numbers"
                    } else {
                        "text up to 500 characters"
                    }
                ),
            }
        }
        text.push_str(&line.join(" · "));
        text.push('\n');
    }
    Ok(text)
}

/// The row with this key, as an object of column name → value.
pub fn row(table: &Value, wanted: &Value) -> Option<Value> {
    let wanted = key(wanted);
    let columns = table["columns"].as_array()?;
    table["rows"]
        .as_array()?
        .iter()
        .find(|r| key(&r[0]) == wanted)
        .map(|r| {
            Value::Object(
                columns
                    .iter()
                    .zip(r.as_array().into_iter().flatten())
                    .map(|(c, v)| (c["name"].as_str().unwrap_or("").to_owned(), v.clone()))
                    .collect(),
            )
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    fn prices() -> serde_json::Value {
        json!({"type":"table","columns":[{"name":"Product","type":"text"},{"name":"Price","type":"number"},{"name":"Max discount","type":"number"}],
            "rows":[["Standing desk",1250,10],["Chair",300,null]]})
    }
    #[test]
    fn tables_are_checked_and_looked_up_by_key() {
        assert_eq!(
            super::validate(&prices()).unwrap(),
            "Product: Standing desk · Price: 1250 · Max discount: 10\nProduct: Chair · Price: 300\n"
        );
        assert_eq!(
            super::row(&prices(), &json!(" standing DESK ")).unwrap(),
            json!({"Product":"Standing desk","Price":1250,"Max discount":10})
        );
        assert!(super::row(&prices(), &json!("Lamp")).is_none());
        let mut twice = prices();
        twice["rows"][1][0] = json!("standing desk");
        assert!(
            super::validate(&twice).is_err(),
            "keys differ, ignoring case"
        );
        let mut numeric_key = prices();
        numeric_key["columns"][0]["type"] = json!("number");
        assert!(super::validate(&numeric_key).is_err());
    }
}
