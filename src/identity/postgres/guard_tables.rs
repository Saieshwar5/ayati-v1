//! Company rules whose limit comes from a Library table (LB4): "if the discount is over
//! Max discount in the Price list for this product, ask first". The value is
//! `{table, match, column}`: the table's row whose key equals the request's `match`
//! field, and its number in `column`. No row or no number means a person decides.
use super::*;
use serde_json::json;

pub(super) fn is_table(value: &Value) -> bool {
    value.get("table").is_some()
}

/// The structure of a table limit, checked with the action's fields (no database).
pub(super) fn validate_shape(
    value: &Value,
    kind: &str,
    comparator: &str,
    fields: &[&Value],
) -> Result<()> {
    let limit = value.as_object().context("Unsupported rule value")?;
    ensure!(
        limit
            .keys()
            .all(|k| ["table", "match", "column", "title"].contains(&k.as_str())),
        "Unsupported table limit"
    );
    ensure!(
        kind == "field" && [">", ">=", "<", "<="].contains(&comparator),
        "A table limit compares one number field"
    );
    let field = |name: &str| fields.iter().any(|f| f["name"] == name);
    let matched = limit
        .get("match")
        .and_then(Value::as_str)
        .context("Choose the request field that names the row")?;
    ensure!(field(matched), "This action has no field {matched}");
    ensure!(
        limit
            .get("column")
            .and_then(Value::as_str)
            .is_some_and(|c| !c.is_empty()),
        "Choose the table's column"
    );
    Ok(())
}

/// Checks the table and column exist and adds the table's title for the read-back.
pub(super) async fn checked(db: &impl ConnectionTrait, w: &str, value: &Value) -> Result<Value> {
    if !is_table(value) {
        return Ok(value.clone());
    }
    let row = query(db, "SELECT title,document FROM relay_library_items WHERE workspace=$1 AND id=$2 AND kind='table' AND NOT trashed", vec![w.into(), value["table"].as_str().unwrap_or("").into()]).await?.context("Choose a Library table")?;
    let table: Value = row.try_get("", "document")?;
    let column = value["column"].as_str().unwrap_or("");
    ensure!(
        table["columns"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|c| c["name"] == column && c["type"] == "number"),
        "Choose a number column of the table"
    );
    let mut value = value.clone();
    value["title"] = json!(row.try_get::<String>("", "title")?);
    Ok(value)
}

/// The limit for this request and the facts it came from; Null when the table has no
/// such row or number, so a person decides.
pub(super) async fn limit(
    db: &impl ConnectionTrait,
    w: &str,
    value: &Value,
    arguments: &Value,
) -> Result<(Value, Option<Value>)> {
    if !is_table(value) {
        return Ok((value.clone(), None));
    }
    let document = document(db, w, value).await?;
    let found = from(document.as_ref(), value, arguments);
    let key = arguments
        .get(value["match"].as_str().unwrap_or(""))
        .cloned()
        .unwrap_or(Value::Null);
    Ok((
        found.clone(),
        Some(json!({"table":value["title"],"row":key,"column":value["column"],"limit":found})),
    ))
}

/// The current table a limit reads, if it still exists.
pub(super) async fn document(
    db: &impl ConnectionTrait,
    w: &str,
    value: &Value,
) -> Result<Option<Value>> {
    Ok(query(db, "SELECT document FROM relay_library_items WHERE workspace=$1 AND id=$2 AND kind='table' AND NOT trashed", vec![w.into(), value["table"].as_str().unwrap_or("").into()]).await?.map(|r| r.try_get("", "document")).transpose()?)
}

/// The number in the request's row, or Null.
pub(super) fn from(document: Option<&Value>, value: &Value, arguments: &Value) -> Value {
    let key = arguments
        .get(value["match"].as_str().unwrap_or(""))
        .cloned()
        .unwrap_or(Value::Null);
    document
        .and_then(|table| crate::identity::library::tables::row(table, &key))
        .and_then(|row| row.get(value["column"].as_str().unwrap_or("")).cloned())
        .filter(Value::is_number)
        .unwrap_or(Value::Null)
}

/// How a table limit reads in the rule's plain words.
pub(super) fn words(value: &Value) -> String {
    format!(
        "{} in {} for its {}",
        value["column"].as_str().unwrap_or("the limit"),
        value["title"].as_str().unwrap_or("a table"),
        value["match"].as_str().unwrap_or("row").replace('_', " ")
    )
}

/// Tables a person can open, with their number columns, to choose a limit from.
pub(super) async fn choices(db: &impl ConnectionTrait, w: &str, user: &str) -> Result<Vec<Value>> {
    let present: bool = query(db, "SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conname='relay_library_items_kind_check' AND pg_get_constraintdef(oid) LIKE '%table%') AS present", vec![]).await?.context("Schema check")?.try_get("", "present")?;
    if !present {
        return Ok(vec![]);
    }
    json_rows(db, "SELECT jsonb_build_object('id',i.id,'title',i.title,'columns',(SELECT COALESCE(jsonb_agg(c->>'name'),'[]'::jsonb) FROM jsonb_array_elements(i.document->'columns') c WHERE c->>'type'='number')) AS data FROM relay_library_items i WHERE i.workspace=$1 AND i.kind='table' AND NOT i.trashed AND relay_library_access(i.workspace,i.id,$2) IS NOT NULL ORDER BY i.title,i.id LIMIT 200", vec![w.into(), user.into()]).await
}
