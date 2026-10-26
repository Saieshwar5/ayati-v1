//! A sample's selected knowledge is personal; selection never changes sharing.
use super::*;
use crate::identity::rehearsals::model::Source;

pub(in crate::identity::postgres) async fn check(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    expected: &Source,
) -> Result<Value> {
    let sql = format!(
        "SELECT i.revision,i.access_revision,jsonb_build_object('id',i.id,'title',i.title,'revision',i.revision,'access_revision',i.access_revision,'kind',i.kind) AS data FROM relay_library_items i WHERE i.workspace=$1 AND {ACCESS} AND i.id=$3 AND NOT i.trashed AND i.readiness IN ('ready','partial') AND length(trim(i.text))>0 AND EXISTS(SELECT 1 FROM relay_library_agents a WHERE a.workspace=i.workspace AND a.item=i.id AND a.agent=$4)"
    );
    let row = query(
        db,
        &sql,
        vec![
            w.into(),
            user.id.clone().into(),
            expected.id.clone().into(),
            agent.into(),
        ],
    )
    .await?
    .context("Sample knowledge unavailable")?;
    ensure!(
        row.try_get::<i64>("", "revision")? == expected.revision
            && row.try_get::<i64>("", "access_revision")? == expected.access_revision,
        "Sample knowledge changed; start a new sample with the current version"
    );
    Ok(row.try_get("", "data")?)
}

pub(in crate::identity::postgres) async fn source(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    expected: &Source,
) -> Result<Value> {
    let current = item(db, user, w, &expected.id, false).await?;
    ensure!(
        current["revision"] == expected.revision
            && current["access_revision"] == expected.access_revision,
        "Sample knowledge changed; choose the current version for a new sample"
    );
    ensure!(
        query(
            db,
            "SELECT agent FROM relay_library_agents WHERE workspace=$1 AND item=$2 AND agent=$3",
            vec![w.into(), expected.id.clone().into(), agent.into()]
        )
        .await?
        .is_some(),
        "This source is not assigned to the agent"
    );
    ensure!(
        (current["readiness"] == "ready" || current["readiness"] == "partial")
            && current["text"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
        "This source has no prepared text for a sample"
    );
    Ok(
        json!({"id":expected.id,"revision":expected.revision,"access_revision":expected.access_revision,
        "title":current["title"],"kind":current["kind"],
        "text":current["text"].as_str().unwrap().chars().take(12000).collect::<String>()}),
    )
}
pub(in crate::identity::postgres) async fn options(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    search: &str,
) -> Result<Vec<Value>> {
    ensure!(search.len() <= 180, "Use a shorter knowledge search");
    let sql = format!(
        "SELECT jsonb_build_object('id',i.id,'revision',i.revision,'access_revision',i.access_revision,'title',i.title,'kind',i.kind,'visibility',i.visibility,'readiness',i.readiness) AS data FROM relay_library_items i WHERE i.workspace=$1 AND {ACCESS} AND NOT i.trashed AND i.readiness IN ('ready','partial') AND length(trim(i.text))>0 AND EXISTS(SELECT 1 FROM relay_library_agents a WHERE a.workspace=i.workspace AND a.item=i.id AND a.agent=$3) AND ($4='' OR strpos(lower(i.title),lower($4))>0) ORDER BY i.updated DESC,i.id LIMIT 51"
    );
    json_rows(
        db,
        &sql,
        vec![
            w.into(),
            user.id.clone().into(),
            agent.into(),
            search.into(),
        ],
    )
    .await
}
