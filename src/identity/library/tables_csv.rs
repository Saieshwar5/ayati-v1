//! A Library table as a CSV file and back, for agents that open a table on their
//! computer and save it again. The first line names the columns; a column that was
//! numbers stays numbers, and a new column is numbers when every value in it is.
use anyhow::{Result, ensure};
use serde_json::{Value, json};

/// The table as CSV text, its column names first.
pub fn to_csv(table: &Value) -> String {
    let field = |text: String| {
        if text.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text
        }
    };
    let columns = table["columns"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let names: Vec<String> = columns
        .iter()
        .map(|c| field(c["name"].as_str().unwrap_or("").to_owned()))
        .collect();
    let mut out = names.join(",") + "\n";
    for row in table["rows"].as_array().into_iter().flatten() {
        let cells: Vec<String> = row
            .as_array()
            .into_iter()
            .flatten()
            .map(|cell| match cell {
                Value::String(text) => field(text.clone()),
                Value::Number(number) => number.to_string(),
                _ => String::new(),
            })
            .collect();
        out.push_str(&(cells.join(",") + "\n"));
    }
    out
}

/// Rows of fields, with quoted fields holding commas, quotes and line breaks.
fn records(text: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut field) = (vec![], vec![], String::new());
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

fn number(text: &str) -> Option<Value> {
    let text = text.trim();
    text.parse::<i64>().map(Value::from).ok().or_else(|| {
        text.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Value::from)
    })
}

/// CSV text as a table. `current` is the table it replaces, whose number columns stay
/// numbers; the caller validates the result.
pub fn from_csv(text: &str, current: Option<&Value>) -> Result<Value> {
    let mut rows = records(text);
    ensure!(
        !rows.is_empty(),
        "The file has no column names on its first line"
    );
    let names: Vec<String> = rows.remove(0).iter().map(|n| n.trim().to_owned()).collect();
    let kept = |name: &str| {
        current?["columns"]
            .as_array()?
            .iter()
            .find(|c| {
                c["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .and_then(|c| c["type"].as_str())
    };
    let mut columns = vec![];
    for (at, name) in names.iter().enumerate() {
        let filled = || {
            rows.iter()
                .filter_map(|r| r.get(at))
                .filter(|f| !f.trim().is_empty())
        };
        let numbers = at > 0
            && match kept(name) {
                Some(kind) => kind == "number",
                None => filled().next().is_some() && filled().all(|f| number(f).is_some()),
            };
        columns.push(json!({"name":name,"type":if numbers { "number" } else { "text" }}));
    }
    let mut out = vec![];
    for row in &rows {
        ensure!(
            row.len() <= names.len(),
            "A row has more values than there are columns"
        );
        let mut cells = vec![];
        for (at, column) in columns.iter().enumerate() {
            let field = row.get(at).map(String::as_str).unwrap_or("").trim();
            cells.push(if field.is_empty() {
                if at == 0 { json!("") } else { Value::Null }
            } else if column["type"] == "number" {
                number(field).ok_or_else(|| {
                    anyhow::anyhow!("{} holds numbers, not \"{field}\"", names[at])
                })?
            } else {
                json!(field)
            });
        }
        out.push(Value::Array(cells));
    }
    Ok(json!({"type":"table","columns":columns,"rows":out}))
}

/// Two changes to the same table, made from `base`, as one table: rows are matched by
/// their key; none when both changed the same row, or the columns differ.
pub fn merge(base: &Value, theirs: &Value, mine: &Value) -> Option<Value> {
    if base["columns"] != theirs["columns"] || base["columns"] != mine["columns"] {
        return None;
    }
    let keyed = |table: &Value| -> Vec<(String, Value)> {
        table["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| (crate::identity::library::tables::key(&row[0]), row.clone()))
            .collect()
    };
    let (was, now, wanted) = (keyed(base), keyed(theirs), keyed(mine));
    let of =
        |rows: &[(String, Value)], key: &str| rows.iter().find(|r| r.0 == key).map(|r| r.1.clone());
    // Rows keep the table's current order; rows the change adds come last.
    let mut keys: Vec<String> = now.iter().map(|r| r.0.clone()).collect();
    for (key, _) in &wanted {
        if !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    let mut rows = vec![];
    for key in keys {
        let (b, t, m) = (of(&was, &key), of(&now, &key), of(&wanted, &key));
        let kept = if m == b || m == t {
            t
        } else if t == b {
            m
        } else {
            return None;
        };
        rows.extend(kept);
    }
    Some(json!({"type":"table","columns":base["columns"],"rows":rows}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::library::tables::validate;

    #[test]
    fn a_table_survives_the_round_trip_and_keeps_its_number_columns() {
        let table = json!({"type":"table","columns":[{"name":"Product","type":"text"},{"name":"Price","type":"number"},{"name":"Note","type":"text"}],
            "rows":[["Standing desk",1250,"Oak, \"large\""],["Chair",299.5,null],["Lamp",null,"2024"]]});
        assert!(validate(&table).is_ok());
        let text = to_csv(&table);
        assert_eq!(
            text,
            "Product,Price,Note\nStanding desk,1250,\"Oak, \"\"large\"\"\"\nChair,299.5,\nLamp,,2024\n"
        );
        assert_eq!(
            from_csv(&text, Some(&table)).unwrap(),
            table,
            "Note stays text though 2024 is a number"
        );
        // A new table guesses: a column is numbers when every value is.
        let fresh = from_csv("Item,Qty,Code\r\nDesk,2,A1\r\nChair,10,7\r\n\r\n", None).unwrap();
        assert_eq!(
            fresh["columns"],
            json!([{"name":"Item","type":"text"},{"name":"Qty","type":"number"},{"name":"Code","type":"text"}])
        );
        assert!(validate(&fresh).is_ok());
        assert!(
            from_csv("Product,Price\nDesk,cheap\n", Some(&table)).is_err(),
            "a number column refuses text"
        );
        assert!(from_csv("", None).is_err());
        assert!(from_csv("A,B\n1,2,3\n", None).is_err());
    }

    #[test]
    fn changes_to_different_rows_merge_and_to_the_same_row_do_not() {
        let table = |rows: Value| json!({"type":"table","columns":[{"name":"Product","type":"text"},{"name":"Price","type":"number"}],"rows":rows});
        let base = table(json!([["Desk", 1200], ["Chair", 300]]));
        let theirs = table(json!([["Desk", 1200], ["Chair", 320], ["Lamp", 40]]));
        let mine = table(json!([["Desk", 1250], ["Chair", 300], ["Shelf", 90]]));
        assert_eq!(
            merge(&base, &theirs, &mine).unwrap()["rows"],
            json!([["Desk", 1250], ["Chair", 320], ["Lamp", 40], ["Shelf", 90]])
        );
        assert!(
            merge(
                &base,
                &table(json!([["Desk", 1300], ["Chair", 300]])),
                &mine
            )
            .is_none(),
            "the same row"
        );
        assert_eq!(
            merge(&base, &table(json!([["Desk", 1200]])), &mine).unwrap()["rows"],
            json!([["Desk", 1250], ["Shelf", 90]]),
            "a row someone removed stays removed"
        );
        let other =
            json!({"type":"table","columns":[{"name":"Product","type":"text"}],"rows":[["Desk"]]});
        assert!(merge(&base, &other, &mine).is_none(), "different columns");
    }
}
