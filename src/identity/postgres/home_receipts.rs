use super::*;
use serde_json::json;
pub(super) async fn retry(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
    input: &Value,
) -> Result<Option<Value>> {
    uuid::Uuid::parse_str(id)?;
    let hash = crate::identity::events::model::fingerprint(input)?;
    let saved=query(db,"SELECT input_hash,result FROM relay_home_receipts WHERE workspace=$1 AND owner_id=$2 AND request=$3",vec![w.into(),user.id.clone().into(),id.into()]).await?;
    saved
        .map(|r| {
            ensure!(
                r.try_get::<String>("", "input_hash")? == hash,
                "Home request ID belongs to different content"
            );
            r.try_get("", "result").map_err(Into::into)
        })
        .transpose()
}
pub(super) async fn remember(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
    input: &Value,
    result: &Value,
) -> Result<()> {
    execute(
        db,
        "INSERT INTO relay_home_receipts VALUES($1,$2,$3,$4,$5)",
        vec![
            w.into(),
            user.id.clone().into(),
            id.into(),
            crate::identity::events::model::fingerprint(input)?.into(),
            result.clone().into(),
        ],
    )
    .await?;
    Ok(())
}
pub(super) async fn layout(db: &impl ConnectionTrait, user: &User, w: &str) -> Result<Value> {
    Ok(query(db,"SELECT jsonb_build_object('revision',revision,'order',data->'order','wide',data->'wide','hidden',data->'hidden') AS data FROM relay_home_layouts WHERE workspace=$1 AND owner_id=$2",vec![w.into(),user.id.clone().into()]).await?.map(|r|r.try_get("","data")).transpose()?.unwrap_or_else(||json!({"revision":0,"order":[],"wide":[],"hidden":[]})))
}
