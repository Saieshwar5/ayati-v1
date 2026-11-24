//! Skills an agent proposes, as its owners and admins see and decide them.
//! Evidence text is shown only from conversations the viewer can open. Approving
//! a new skill saves it as an owner's own skill, marked as created by the agent,
//! and puts it on the agent; a new version is written by the agent too; retiring
//! takes the skill off the agent. Nothing is used before a person approves.
use super::super::agent_profiles::managed_access;
use super::super::skills::files::{SkillFile, check};
use super::super::skills::{AGENT_SKILLS, LISTING_CHARS, insert_version};
use super::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillDecision {
    pub approve: bool,
    /// The files as the person edited them; the proposal's own when absent.
    #[serde(default)]
    pub files: Option<Vec<SkillFile>>,
}

impl PgStore {
    /// An agent's proposed skills (newest first) and its recent decided ones.
    pub(crate) async fn skill_proposals(
        &self,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, agent, true).await?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(vec![]);
        }
        let mut items = json_rows(&tx, "SELECT jsonb_build_object('id',p.id,'kind',p.kind,'name',p.name,'description',p.description,'files',p.files,
                'why',p.why,'evidence',p.evidence,'rehearsals',p.rehearsals,'state',p.state,'created_ms',p.created_ms,'decided_ms',p.decided_ms,
                'decided_by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=p.decided_by),
                'job_type',(SELECT j.name FROM relay_job_types j WHERE j.workspace=p.workspace AND j.id=p.job_type),
                'skill',(SELECT jsonb_build_object('id',s.id,'name',s.name,'version',s.version) FROM relay_skills s WHERE s.workspace=p.workspace AND s.id=p.skill)) AS data
            FROM relay_skill_proposals p WHERE p.workspace=$1 AND p.agent=$2
            ORDER BY p.state='proposed' DESC,p.created_ms DESC LIMIT 30", vec![w.into(), agent.into()]).await?;
        for item in &mut items {
            let mut shown = vec![];
            for e in item["evidence"].as_array().cloned().unwrap_or_default() {
                let job = e["job"].as_str().unwrap_or_default();
                // The request's words only when the viewer can open its conversation.
                let text = query(&tx, "SELECT left(r.content,300) AS text,c.name FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
                    JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat LEFT JOIN relay_chat_messages r ON r.workspace=j.workspace AND r.id=j.request_message
                    WHERE j.workspace=$1 AND j.reply=$2 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3))",
                    vec![w.into(), job.into(), user.id.clone().into()]).await?;
                let (said, place) = match text {
                    Some(r) => (
                        r.try_get::<Option<String>>("", "text")?,
                        r.try_get::<Option<String>>("", "name")?,
                    ),
                    None => (None, None),
                };
                shown.push(json!({"at_ms":e["at_ms"],"outcome":e["outcome"],"text":said,"conversation":place}));
            }
            item["evidence"] = json!(shown);
        }
        tx.commit().await?;
        Ok(items)
    }

    /// One of the agent's owners or an admin approves (as proposed or edited) or
    /// rejects a proposal. A repeated decision changes nothing.
    pub(crate) async fn decide_skill_proposal(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &SkillDecision,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let a = managed_access(&tx, &user.id, w, agent, true).await?;
        ensure!(ready(&tx).await?, "Skill proposals are not available yet");
        let p = json_rows(&tx, "SELECT to_jsonb(p) AS data FROM relay_skill_proposals p WHERE workspace=$1 AND agent=$2 AND id=$3 FOR UPDATE",
            vec![w.into(), agent.into(), id.into()]).await?.into_iter().next().context("That proposal is not on this agent")?;
        let wanted = if input.approve {
            "approved"
        } else {
            "rejected"
        };
        if p["state"] != "proposed" {
            ensure!(p["state"] == wanted, "This proposal was already decided");
            tx.commit().await?;
            return Ok(json!({"state":wanted,"skill":p["created_skill"]}));
        }
        let agent_name = a["name"].as_str().unwrap_or_default().to_owned();
        let mut created = None;
        if input.approve {
            let kind = p["kind"].as_str().unwrap_or_default();
            if kind == "retire" {
                let skill = p["skill"].as_str().context("No skill to retire")?;
                execute(
                    &tx,
                    "DELETE FROM relay_agent_skills WHERE workspace=$1 AND agent=$2 AND skill=$3",
                    vec![w.into(), agent.into(), skill.into()],
                )
                .await?;
                audit_with_summary(
                    &tx,
                    &user.id,
                    Some(w),
                    "skill_assigned",
                    agent,
                    &format!(
                        "retired the skill {} from {agent_name}, as it proposed",
                        p["name"].as_str().unwrap_or_default()
                    ),
                )
                .await?;
            } else {
                let files: Vec<SkillFile> = match &input.files {
                    Some(files) => files.clone(),
                    None => serde_json::from_value(p["files"].clone())?,
                };
                let checked = check(&files)?;
                ensure!(
                    !checked.has_scripts,
                    "A skill an agent writes can't have scripts; add them by uploading a skill"
                );
                if kind == "new" {
                    created =
                        Some(new_skill(&tx, user, w, agent, &agent_name, &files, &checked).await?);
                } else {
                    let skill = p["skill"].as_str().context("No skill to improve")?;
                    created = Some(skill.to_owned());
                    next_version(&tx, user, w, agent, &agent_name, skill, &files, &checked).await?;
                }
            }
        }
        execute(&tx, "UPDATE relay_skill_proposals SET state=$4,decided_by=$5,decided_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,created_skill=$6
            WHERE workspace=$1 AND agent=$2 AND id=$3", vec![w.into(), agent.into(), id.into(), wanted.into(), user.id.clone().into(), created.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"state":wanted,"skill":created}))
    }
}

/// Saves an approved new skill as an owner's own, created by the agent, and puts it on the agent.
async fn new_skill(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    agent_name: &str,
    files: &[SkillFile],
    checked: &super::super::skills::files::Checked,
) -> Result<String> {
    ensure!(
        query(
            tx,
            "SELECT 1 AS taken FROM relay_skills WHERE workspace=$1 AND name=$2",
            vec![w.into(), checked.name.clone().into()]
        )
        .await?
        .is_none(),
        "A skill named {} already exists; edit the name before approving",
        checked.name
    );
    // A personal skill goes only on agents its owner owns: the approver when they
    // are an owner, otherwise the agent's first owner.
    let owners = super::super::agent_owners::listed(tx, w, agent).await?;
    let owner = if owners.iter().any(|o| o["id"] == user.id.as_str()) {
        user.id.clone()
    } else {
        owners
            .first()
            .and_then(|o| o["id"].as_str())
            .context("This agent needs an owner before it can get a skill")?
            .to_owned()
    };
    let limits = query(tx, "SELECT count(*) AS n,COALESCE(sum(length(s.name)+length(s.description)),0) AS chars FROM relay_agent_skills x
        JOIN relay_skills s ON s.workspace=x.workspace AND s.id=x.skill WHERE x.workspace=$1 AND x.agent=$2", vec![w.into(), agent.into()]).await?.context("Agent unavailable")?;
    ensure!(
        limits.try_get::<i64>("", "n")? < AGENT_SKILLS,
        "An agent can use at most {AGENT_SKILLS} skills; remove one first"
    );
    ensure!(
        limits.try_get::<i64>("", "chars")?
            + (checked.name.len() + checked.description.len()) as i64
            <= LISTING_CHARS,
        "Its skills' descriptions would be too long to list together; shorten the description"
    );
    let id = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_skills(workspace,id,name,description,owner_id,shared,version,created_by_agent) VALUES($1,$2,$3,$4,$5,false,1,$6)",
        vec![w.into(), id.clone().into(), checked.name.clone().into(), checked.description.clone().into(), owner.into(), agent.into()]).await?;
    insert_version(tx, w, &id, 1, checked, files, &user.id).await?;
    execute(
        tx,
        "UPDATE relay_skill_versions SET agent=$3 WHERE workspace=$1 AND skill=$2 AND version=1",
        vec![w.into(), id.clone().into(), agent.into()],
    )
    .await?;
    execute(
        tx,
        "INSERT INTO relay_agent_skills(workspace,agent,skill,added_by) VALUES($1,$2,$3,$4)",
        vec![
            w.into(),
            agent.into(),
            id.clone().into(),
            user.id.clone().into(),
        ],
    )
    .await?;
    audit_with_summary(
        tx,
        &user.id,
        Some(w),
        "skill_created",
        &id,
        &format!(
            "approved the skill {} created by {agent_name}",
            checked.name
        ),
    )
    .await?;
    Ok(id)
}

/// Saves an approved new version of one of the agent's skills, written by the agent.
#[allow(clippy::too_many_arguments)]
async fn next_version(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    agent_name: &str,
    skill: &str,
    files: &[SkillFile],
    checked: &super::super::skills::files::Checked,
) -> Result<()> {
    let row = query(tx, "SELECT s.name,s.version,s.shared,s.owner_id FROM relay_skills s JOIN relay_agent_skills x ON x.workspace=s.workspace AND x.skill=s.id AND x.agent=$3
        WHERE s.workspace=$1 AND s.id=$2 FOR UPDATE", vec![w.into(), skill.into(), agent.into()]).await?.context("That skill is no longer on this agent")?;
    let name: String = row.try_get("", "name")?;
    ensure!(
        name == checked.name,
        "Keep the name {name}; the agent finds the skill by it"
    );
    let admin = role(tx, w, &user.id).await? == "Admin";
    let shared: bool = row.try_get("", "shared")?;
    ensure!(
        admin || (!shared && row.try_get::<String>("", "owner_id")? == user.id),
        "Only the skill's owner or an admin can change it"
    );
    let next = row.try_get::<i32>("", "version")? + 1;
    insert_version(tx, w, skill, next, checked, files, &user.id).await?;
    execute(
        tx,
        "UPDATE relay_skill_versions SET agent=$4 WHERE workspace=$1 AND skill=$2 AND version=$3",
        vec![w.into(), skill.into(), next.into(), agent.into()],
    )
    .await?;
    execute(
        tx,
        "UPDATE relay_skills SET version=$3,description=$4 WHERE workspace=$1 AND id=$2",
        vec![
            w.into(),
            skill.into(),
            next.into(),
            checked.description.clone().into(),
        ],
    )
    .await?;
    audit_with_summary(
        tx,
        &user.id,
        Some(w),
        "skill_changed",
        skill,
        &format!("approved version {next} of the skill {name}, improved by {agent_name}"),
    )
    .await?;
    Ok(())
}
