//! Skills (D234): workspace skills (shared; only admins add or change them) and
//! "My skills" (anyone; usable on agents they own). Every change is a new version
//! with a content hash, and creating, changing, sharing and assigning are audited.
use super::*;
use serde::Deserialize;
use serde_json::json;

#[path = "skill_files.rs"]
pub(crate) mod files;
#[path = "skills_migration.rs"]
pub(crate) mod migration;
#[path = "skill_runtime.rs"]
pub(crate) mod runtime;
#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;

use files::{SkillFile, check};

/// What an agent's instructions can carry: skills, and characters of names and
/// descriptions. Beyond this, assigning is refused with a reason.
pub(crate) const AGENT_SKILLS: i64 = 20;
pub(crate) const LISTING_CHARS: i64 = 8000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewSkill {
    /// Chosen by the browser so a retried upload saves one skill.
    pub id: String,
    pub shared: bool,
    pub files: Vec<SkillFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewVersion {
    /// The version being replaced, so two editors can't overwrite each other.
    pub version: i32,
    pub files: Vec<SkillFile>,
}

/// One skill row the person may see, and whether they may change it.
async fn visible(
    tx: &DatabaseTransaction,
    user: &str,
    w: &str,
    id: &str,
    change: bool,
) -> Result<Value> {
    let admin = role(tx, w, user).await? == "Admin";
    let row = query(tx, "SELECT jsonb_build_object('id',id,'name',name,'description',description,'owner_id',owner_id,'shared',shared,'version',version) AS data
        FROM relay_skills WHERE workspace=$1 AND id=$2 FOR UPDATE", vec![w.into(), id.into()]).await?.context("Skill unavailable")?;
    let skill: Value = row.try_get("", "data")?;
    let own = skill["owner_id"] == user;
    ensure!(skill["shared"] == true || own || admin, "Skill unavailable");
    if change {
        ensure!(
            if skill["shared"] == true {
                admin
            } else {
                own || admin
            },
            "Only admins can change workspace skills"
        );
    }
    Ok(skill)
}

const LIST: &str = "SELECT jsonb_build_object('id',s.id,'name',s.name,'description',s.description,'shared',s.shared,
    'owner_id',s.owner_id,'owner_name',COALESCE(u.display_name,u.email,'Former member'),'version',s.version,
    'license',v.license,'has_scripts',v.has_scripts,'updated_ms',v.created_ms,'files',jsonb_array_length(v.files),
    'agents',(SELECT COALESCE(jsonb_agg(a.name ORDER BY a.name),'[]'::jsonb) FROM relay_agent_skills x
      JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent WHERE x.workspace=s.workspace AND x.skill=s.id)) AS data
    FROM relay_skills s JOIN relay_skill_versions v ON v.workspace=s.workspace AND v.skill=s.id AND v.version=s.version
    LEFT JOIN relay_users u ON u.id=s.owner_id";

impl PgStore {
    /// Workspace skills, the person's own, and (for admins) everyone else's.
    pub(crate) async fn skills(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let all = json_rows(
            &tx,
            &format!(
                "{LIST} WHERE s.workspace=$1 AND (s.shared OR s.owner_id=$2 OR $3) ORDER BY s.name"
            ),
            vec![w.into(), user.id.clone().into(), admin.into()],
        )
        .await?;
        tx.commit().await?;
        let (workspace, rest): (Vec<Value>, Vec<Value>) =
            all.into_iter().partition(|s| s["shared"] == true);
        let (mine, others): (Vec<Value>, Vec<Value>) =
            rest.into_iter().partition(|s| s["owner_id"] == user.id);
        Ok(json!({"workspace": workspace, "mine": mine, "others": others, "admin": admin}))
    }

    /// One skill with its current files and every version's author and time.
    pub(crate) async fn skill(
        &self,
        user: &User,
        w: &str,
        id: &str,
        version: Option<i32>,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let skill = visible(&tx, &user.id, w, id, false).await?;
        let wanted = version.unwrap_or(skill["version"].as_i64().unwrap_or(1) as i32);
        let current = query(&tx, "SELECT files,license,has_scripts,hash FROM relay_skill_versions WHERE workspace=$1 AND skill=$2 AND version=$3",
            vec![w.into(), id.into(), wanted.into()]).await?.context("Version unavailable")?;
        let versions = json_rows(&tx, "SELECT jsonb_build_object('version',v.version,'hash',v.hash,'author',COALESCE(u.display_name,u.email,'Former member'),'created_ms',v.created_ms) AS data
            FROM relay_skill_versions v LEFT JOIN relay_users u ON u.id=v.author WHERE v.workspace=$1 AND v.skill=$2 ORDER BY v.version DESC LIMIT 50",
            vec![w.into(), id.into()]).await?;
        tx.commit().await?;
        let mut data = skill;
        data["shown_version"] = json!(wanted);
        data["files"] = current.try_get("", "files")?;
        data["license"] = json!(current.try_get::<Option<String>>("", "license")?);
        data["has_scripts"] = json!(current.try_get::<bool>("", "has_scripts")?);
        data["hash"] = json!(current.try_get::<String>("", "hash")?);
        data["versions"] = json!(versions);
        Ok(data)
    }

    pub(crate) async fn create_skill(
        &self,
        user: &User,
        w: &str,
        input: &NewSkill,
    ) -> Result<Value> {
        ensure!(uuid::Uuid::parse_str(&input.id).is_ok(), "Invalid skill");
        let checked = check(&input.files)?;
        let tx = self.workspace_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        ensure!(
            !input.shared || admin,
            "Only admins can add workspace skills"
        );
        if let Some(existing) = query(&tx, "SELECT owner_id,shared,(SELECT hash FROM relay_skill_versions v WHERE v.workspace=s.workspace AND v.skill=s.id AND v.version=1) AS hash
            FROM relay_skills s WHERE workspace=$1 AND id=$2", vec![w.into(), input.id.clone().into()]).await? {
            // An exact retry returns the saved skill.
            ensure!(existing.try_get::<String>("", "owner_id")? == user.id && existing.try_get::<Option<String>>("", "hash")?.as_deref() == Some(checked.hash.as_str()),
                "Skill ID already used");
            tx.commit().await?;
            return self.skill(user, w, &input.id, None).await;
        }
        ensure!(
            query(
                &tx,
                "SELECT 1 AS taken FROM relay_skills WHERE workspace=$1 AND name=$2",
                vec![w.into(), checked.name.clone().into()]
            )
            .await?
            .is_none(),
            "A skill named {} already exists in this workspace",
            checked.name
        );
        execute(&tx, "INSERT INTO relay_skills(workspace,id,name,description,owner_id,shared,version) VALUES($1,$2,$3,$4,$5,$6,1)",
            vec![w.into(), input.id.clone().into(), checked.name.clone().into(), checked.description.clone().into(), user.id.clone().into(), input.shared.into()]).await?;
        insert_version(&tx, w, &input.id, 1, &checked, &input.files, &user.id).await?;
        let scope = if input.shared {
            "workspace skill"
        } else {
            "skill"
        };
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "skill_created",
            &input.id,
            &format!("added the {scope} {}", checked.name),
        )
        .await?;
        tx.commit().await?;
        self.skill(user, w, &input.id, None).await
    }

    /// Saves changed files as the next version. The name can't change: agents load it by name.
    pub(crate) async fn update_skill(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &NewVersion,
    ) -> Result<Value> {
        let checked = check(&input.files)?;
        let tx = self.workspace_tx(w).await?;
        let skill = visible(&tx, &user.id, w, id, true).await?;
        ensure!(
            skill["name"] == checked.name.as_str(),
            "Keep the name {}; agents find the skill by it",
            skill["name"].as_str().unwrap_or_default()
        );
        let current = skill["version"].as_i64().unwrap_or(1) as i32;
        let latest: String = query(
            &tx,
            "SELECT hash FROM relay_skill_versions WHERE workspace=$1 AND skill=$2 AND version=$3",
            vec![w.into(), id.into(), current.into()],
        )
        .await?
        .context("Version unavailable")?
        .try_get("", "hash")?;
        if input.version == current - 1 && latest == checked.hash {
            // An exact retry after a lost response.
            tx.commit().await?;
            return self.skill(user, w, id, None).await;
        }
        ensure!(
            input.version == current,
            "Someone changed this skill; reload before saving"
        );
        ensure!(latest != checked.hash, "Nothing changed");
        let next = current + 1;
        insert_version(&tx, w, id, next, &checked, &input.files, &user.id).await?;
        execute(
            &tx,
            "UPDATE relay_skills SET version=$3,description=$4 WHERE workspace=$1 AND id=$2",
            vec![
                w.into(),
                id.into(),
                next.into(),
                checked.description.clone().into(),
            ],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "skill_changed",
            id,
            &format!("changed the skill {} (version {next})", checked.name),
        )
        .await?;
        tx.commit().await?;
        self.skill(user, w, id, None).await
    }

    /// Admins share a skill with the workspace, or make it personal again.
    pub(crate) async fn share_skill(
        &self,
        user: &User,
        w: &str,
        id: &str,
        shared: bool,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        let skill = visible(&tx, &user.id, w, id, false).await?;
        if skill["shared"] != shared {
            execute(
                &tx,
                "UPDATE relay_skills SET shared=$3 WHERE workspace=$1 AND id=$2",
                vec![w.into(), id.into(), shared.into()],
            )
            .await?;
            let verb = if shared {
                "shared the skill"
            } else {
                "made the skill personal again:"
            };
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "skill_shared",
                id,
                &format!("{verb} {}", skill["name"].as_str().unwrap_or_default()),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Deletes a skill; agents that used it stop seeing it.
    pub(crate) async fn delete_skill(&self, user: &User, w: &str, id: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        let skill = visible(&tx, &user.id, w, id, true).await?;
        execute(
            &tx,
            "DELETE FROM relay_skills WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "skill_deleted",
            id,
            &format!(
                "deleted the skill {}",
                skill["name"].as_str().unwrap_or_default()
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The skills an agent can use, with how often each was used this week.
    pub(crate) async fn agent_skills(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let a = super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let editable = a["owner_id"] == user.id || role(&tx, w, &user.id).await? == "Admin";
        let week = now() * 1000 - 7 * 24 * 3600 * 1000;
        let skills = json_rows(&tx, &format!("{LIST} JOIN relay_agent_skills x ON x.workspace=s.workspace AND x.skill=s.id AND x.agent=$2
            WHERE s.workspace=$1 ORDER BY s.name"), vec![w.into(), agent.into()]).await?;
        let mut items = vec![];
        for mut skill in skills {
            let used: i64 = query(&tx, "SELECT count(*) AS n FROM relay_skill_loads l JOIN relay_chat_jobs j ON j.workspace=l.workspace AND j.reply=l.job
                WHERE l.workspace=$1 AND l.skill=$2 AND j.payload->>'agent'=$3 AND l.at_ms>=$4 AND l.path IS NULL",
                vec![w.into(), skill["id"].as_str().unwrap_or_default().into(), agent.into(), week.into()]).await?
                .map(|r| r.try_get("", "n")).transpose()?.unwrap_or(0);
            skill["used_this_week"] = json!(used);
            items.push(skill);
        }
        tx.commit().await?;
        Ok(json!({"skills": items, "editable": editable}))
    }

    /// Adds or removes a skill on an agent its owner or an admin can change.
    pub(crate) async fn assign_skill(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        skill: &str,
        on: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let a = super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let s = visible(&tx, &user.id, w, skill, false).await?;
        let name = s["name"].as_str().unwrap_or_default().to_string();
        if on {
            ensure!(
                s["shared"] == true || s["owner_id"] == a["owner_id"],
                "A personal skill can only be used on agents its owner owns; ask an admin to share it"
            );
            let budget = query(&tx, "SELECT count(*) AS n,COALESCE(sum(length(s.name)+length(s.description)),0) AS chars FROM relay_agent_skills x
                JOIN relay_skills s ON s.workspace=x.workspace AND s.id=x.skill WHERE x.workspace=$1 AND x.agent=$2 AND x.skill<>$3",
                vec![w.into(), agent.into(), skill.into()]).await?.context("Agent unavailable")?;
            let (count, chars): (i64, i64) =
                (budget.try_get("", "n")?, budget.try_get("", "chars")?);
            let adding = (name.len() + s["description"].as_str().unwrap_or_default().len()) as i64;
            ensure!(
                count < AGENT_SKILLS,
                "An agent can use at most {AGENT_SKILLS} skills"
            );
            ensure!(
                chars + adding <= LISTING_CHARS,
                "These skills' descriptions are too long to list together; remove one first or shorten a description"
            );
            if execute(&tx, "INSERT INTO relay_agent_skills(workspace,agent,skill,added_by) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
                vec![w.into(), agent.into(), skill.into(), user.id.clone().into()]).await? == 1 {
                audit_with_summary(&tx, &user.id, Some(w), "skill_assigned", agent, &format!("let {} use the skill {name}", a["name"].as_str().unwrap_or_default())).await?;
            }
        } else if execute(
            &tx,
            "DELETE FROM relay_agent_skills WHERE workspace=$1 AND agent=$2 AND skill=$3",
            vec![w.into(), agent.into(), skill.into()],
        )
        .await?
            == 1
        {
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "skill_assigned",
                agent,
                &format!(
                    "removed the skill {name} from {}",
                    a["name"].as_str().unwrap_or_default()
                ),
            )
            .await?;
        }
        tx.commit().await?;
        self.agent_skills(user, w, agent).await
    }
}

async fn insert_version(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    version: i32,
    checked: &files::Checked,
    files: &[SkillFile],
    author: &str,
) -> Result<()> {
    execute(tx, "INSERT INTO relay_skill_versions(workspace,skill,version,hash,description,license,has_scripts,files,author) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        vec![w.into(), id.into(), version.into(), checked.hash.clone().into(), checked.description.clone().into(), checked.license.clone().into(),
            checked.has_scripts.into(), serde_json::to_value(files)?.into(), author.into()]).await?;
    Ok(())
}
