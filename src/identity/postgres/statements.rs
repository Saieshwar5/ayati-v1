//! Running SQL with bound values: one statement, one row, all rows, or each row's
//! `data` JSON column. Every store module uses these through `super::*`.
use anyhow::Result;
use sea_orm::{ConnectionTrait, QueryResult, Statement, Value as SqlValue};
use serde_json::Value;

fn statement(sql: &str, values: Vec<SqlValue>) -> Statement {
    Statement::from_sql_and_values(sea_orm::DbBackend::Postgres, sql, values)
}
pub(super) async fn execute(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<u64> {
    Ok(db
        .execute_raw(statement(sql, values))
        .await?
        .rows_affected())
}
pub(super) async fn query(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<Option<QueryResult>> {
    Ok(db.query_one_raw(statement(sql, values)).await?)
}
pub(super) async fn rows(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<Vec<QueryResult>> {
    Ok(db.query_all_raw(statement(sql, values)).await?)
}
pub(super) async fn json_rows(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<Vec<Value>> {
    rows(db, sql, values)
        .await?
        .into_iter()
        .map(|r| Ok(r.try_get("", "data")?))
        .collect()
}
