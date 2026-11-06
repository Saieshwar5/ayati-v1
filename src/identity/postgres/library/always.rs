//! "Always read" documents (Library LB2c): up to 5 documents an agent was given that
//! its managers want in every request, at most 8,000 characters together. Each request
//! still checks "use for" against its own audience, and what it was given fences it.
use super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

/// Schema 73: the documents each agent always reads.
pub(crate) struct LibraryAlways;
impl MigrationName for LibraryAlways {
    fn name(&self) -> &str {
        "m20261105_000073_library_always"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryAlways {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("always.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Always-read documents require a forward migration".into(),
        ))
    }
}

const MOST: usize = 5;
pub(super) const BUDGET: usize = 8000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AlwaysRead {
    /// The documents, in the order the agent reads them.
    pub items: Vec<String>,
    /// The list this change was made from; a newer save wins nothing silently.
    pub expected: Vec<String>,
}

/// Documents given to the agent (directly or through their folder) that this person
/// can open: what managers may choose from.
const GIVEN: &str = "SELECT jsonb_build_object('id',i.id,'title',i.title,'kind',i.kind,'folder',CASE WHEN i.folder IS NULL THEN NULL ELSE s.title END,'use_for',g.use_for,'characters',(SELECT COALESCE(sum(char_length(p.text)),0) FROM relay_library_passages p WHERE p.workspace=i.workspace AND p.item=i.id AND p.revision=i.revision)) AS data FROM relay_library_items i JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id) JOIN relay_library_agents g ON g.workspace=s.workspace AND g.item=s.id AND g.agent=$2 WHERE i.workspace=$1 AND i.kind<>'folder' AND NOT i.trashed AND NOT s.trashed AND relay_library_access(i.workspace,i.id,$3) IS NOT NULL";

async fn current(db: &impl ConnectionTrait, w: &str, agent: &str) -> Result<Vec<String>> {
    rows(
        db,
        "SELECT item FROM relay_library_always WHERE workspace=$1 AND agent=$2 ORDER BY position",
        vec![w.into(), agent.into()],
    )
    .await?
    .into_iter()
    .map(|r| Ok(r.try_get("", "item")?))
    .collect()
}

impl PgStore {
    pub(crate) async fn library_always(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let editable =
            profile["owner_id"] == user.id.as_str() || role(&tx, w, &user.id).await? == "Admin";
        let given = json_rows(
            &tx,
            &format!("{GIVEN} ORDER BY i.title,i.id LIMIT 200"),
            vec![w.into(), agent.into(), user.id.clone().into()],
        )
        .await?;
        // Chosen documents this person can't open are counted, not named.
        let chosen = current(&tx, w, agent).await?;
        let items: Vec<Value> = chosen
            .iter()
            .filter_map(|id| given.iter().find(|g| g["id"] == id.as_str()).cloned())
            .collect();
        tx.commit().await?;
        Ok(
            json!({"items":items,"hidden":chosen.len()-items.len(),"chosen":chosen.len(),"given":given,"editable":editable,"most":MOST,"budget":BUDGET}),
        )
    }

    pub(crate) async fn library_always_save(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &AlwaysRead,
    ) -> Result<Value> {
        ensure!(input.items.len() <= MOST, "Choose up to {MOST} documents");
        let mut seen = std::collections::HashSet::new();
        ensure!(
            input.items.iter().all(|i| seen.insert(i)),
            "Each document can be chosen once"
        );
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        if current(&tx, w, agent).await? != input.expected {
            return Err(Conflict.into());
        }
        let given = json_rows(
            &tx,
            GIVEN,
            vec![w.into(), agent.into(), user.id.clone().into()],
        )
        .await?;
        let mut titles = vec![];
        for id in &input.items {
            let found = given
                .iter()
                .find(|g| g["id"] == id.as_str())
                .context("Choose documents this agent was given and you can open")?;
            titles.push(found["title"].as_str().unwrap_or("").to_owned());
        }
        execute(
            &tx,
            "DELETE FROM relay_library_always WHERE workspace=$1 AND agent=$2",
            vec![w.into(), agent.into()],
        )
        .await?;
        for (position, id) in input.items.iter().enumerate() {
            execute(&tx, "INSERT INTO relay_library_always(workspace,agent,item,position) VALUES($1,$2,$3,$4)", vec![w.into(), agent.into(), id.clone().into(), (position as i32).into()]).await?;
        }
        let name = profile["name"].as_str().unwrap_or("The agent");
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_always_read",
            agent,
            &if titles.is_empty() {
                format!("{name} no longer always reads any Library document")
            } else {
                format!("{name} always reads: {}", titles.join(", "))
            },
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"items":input.items}))
    }

    /// The always-read text for one request, within the budget, of documents usable for
    /// its audience. Each document given leaves a receipt that fences the reply.
    pub(crate) async fn library_always_context(&self, job: &Job) -> Result<String> {
        let tx = self.workspace_tx(&job.workspace).await?;
        let present: bool = query(
            &tx,
            "SELECT to_regclass('relay_library_always') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        if !present {
            tx.commit().await?;
            return Ok(String::new());
        }
        let documents = json_rows(&tx, "SELECT jsonb_build_object('id',i.id,'title',i.title,'revision',i.revision,'access_revision',i.access_revision,'text',(SELECT string_agg(CASE WHEN p.section='' THEN '' ELSE '### ' || p.section || E'\\n' END || p.text,E'\\n' ORDER BY p.position) FROM relay_library_passages p WHERE p.workspace=i.workspace AND p.item=i.id AND p.revision=i.revision)) AS data FROM relay_library_always a JOIN relay_library_items i ON i.workspace=a.workspace AND i.id=a.item WHERE a.workspace=$1 AND a.agent=$2 AND relay_library_agent_use(a.workspace,a.item,a.agent,$3) IS NOT NULL ORDER BY a.position", vec![job.workspace.clone().into(), job.agent.clone().into(), job.reply.clone().into()]).await?;
        let mut out = String::new();
        for document in documents {
            let text = document["text"].as_str().unwrap_or("");
            let room = BUDGET.saturating_sub(out.chars().count());
            if text.is_empty() || room < 200 {
                continue;
            }
            let part: String = format!("## {}\n{text}\n", document["title"].as_str().unwrap_or(""))
                .chars()
                .take(room)
                .collect();
            out.push_str(&part);
            execute(&tx,"INSERT INTO relay_library_reads(workspace,reply,item,revision,access_revision,title,at,cited,section) VALUES($1,$2,$3,$4,$5,$6,$7,false,'') ON CONFLICT(workspace,reply,item) DO NOTHING",vec![job.workspace.clone().into(),job.reply.clone().into(),document["id"].as_str().unwrap_or("").into(),document["revision"].as_i64().unwrap_or(1).into(),document["access_revision"].as_i64().unwrap_or(1).into(),document["title"].as_str().unwrap_or("").into(),now().into()]).await?;
        }
        tx.commit().await?;
        Ok(out)
    }
}
