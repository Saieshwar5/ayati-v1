use super::*;
use crate::identity::chat::model::Job;
use serde_json::json;
async fn operation(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Value> {
    let row=query(db,"SELECT jsonb_build_object('id',a.id,'agent',a.agent,'connection',a.connection,'operation',a.operation,'chat',a.chat,'rule',to_jsonb(r),'connection_revision',c.revision) AS data FROM relay_control_actions a JOIN relay_control_effective_rules r ON r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation JOIN relay_control_connections c ON c.workspace=a.workspace AND c.id=a.connection WHERE a.workspace=$1 AND a.id=$2 AND a.state='completed' AND a.rule_revision=r.revision AND a.connection_revision=c.revision",vec![w.into(),id.into()]).await?.context("Panel operation source is no longer authorized")?;
    row.try_get("", "data").map_err(Into::into)
}
pub(super) async fn capture(
    db: &impl ConnectionTrait,
    job: &Job,
    previous: Option<&Value>,
    declared: &[String],
) -> Result<Value> {
    let mut provenance = previous
        .cloned()
        .unwrap_or_else(|| json!({"tasks":[],"operations":[]}));
    let tasks = provenance["tasks"]
        .as_array_mut()
        .context("Invalid saved panel provenance")?;
    if !tasks.iter().any(|v| v == &job.reply) {
        tasks.push(json!(job.reply));
    }
    ensure!(
        tasks.len() <= 32,
        "Panel source history reached its supported limit; create a separately reviewed view"
    );
    let tree = super::handoff_access::tree(db, job).await?;
    let mut task_ids = vec![json!(super::handoff_access::root(db, job).await?)];
    task_ids.extend(tree.iter().map(|h| h["child"].clone()));
    let records=rows(db,"SELECT id FROM relay_control_actions WHERE workspace=$1 AND job IN (SELECT jsonb_array_elements_text($2::jsonb)) AND state='completed' ORDER BY created,id LIMIT 33",vec![job.workspace.clone().into(),json!(task_ids).into()]).await?;
    ensure!(
        records.len() <= 32,
        "Panel operation sources exceed the supported bound"
    );
    let mut observed = std::collections::HashSet::new();
    for row in records {
        let id: String = row.try_get("", "id")?;
        observed.insert(id.clone());
        let source = operation(db, &job.workspace, &id).await?;
        let refs = provenance["operations"].as_array_mut().unwrap();
        if !refs.contains(&source) {
            refs.push(source);
        }
    }
    ensure!(
        declared.iter().all(|id| observed.contains(id)),
        "Panel sources must be completed operations from this logical task"
    );
    ensure!(
        provenance["operations"].as_array().unwrap().len() <= 32
            && serde_json::to_vec(&provenance)?.len() <= 65536,
        "Panel provenance exceeds the supported bound"
    );
    ensure!(
        allowed(db, &job.workspace, &job.requester, &provenance).await?,
        "Panel source unavailable"
    );
    Ok(provenance)
}
pub(super) async fn allowed(
    db: &impl ConnectionTrait,
    w: &str,
    owner: &str,
    provenance: &Value,
) -> Result<bool> {
    let user = User {
        id: owner.into(),
        email: String::new(),
        display_name: None,
    };
    for task in provenance["tasks"]
        .as_array()
        .context("Invalid panel task provenance")?
    {
        let mut job =
            super::handoff_access::job(db, w, task.as_str().context("Invalid panel source")?)
                .await?;
        ensure!(
            job.workspace == w && job.requester == owner,
            "Panel principal changed"
        );
        if super::chat::access(db, &user, w, &job.chat).await.is_err()
            || super::approval_authority::agent_visible(db, owner, w, &job.agent)
                .await
                .is_err()
        {
            return Ok(false);
        }
        job.requester = owner.into();
        if !super::source_access::allowed(db, &job).await?
            || !super::context_attempts::knowledge_allowed(db, &job).await?
        {
            return Ok(false);
        }
    }
    for saved in provenance["operations"]
        .as_array()
        .context("Invalid panel operation provenance")?
    {
        match operation(
            db,
            w,
            saved["id"].as_str().context("Invalid panel receipt")?,
        )
        .await
        {
            Ok(current) if current == *saved => {}
            Err(e) if e.is::<sea_orm::DbErr>() => return Err(e),
            _ => return Ok(false),
        }
    }
    Ok(true)
}
