//! Skill scripts on the agent's computer (B2 of agent building blocks, D248).
//! A skill's script runs only on the agent's own computer, never on Cuckoo's
//! servers or an executor, and only under the computer's Programs rule: a
//! script counts as a program, so "Ask first" asks with the exact command,
//! which names the skill, its version and the script. The skill's files are
//! copied to `shared/skills/<name>/v<version>/` first, and the run is recorded
//! in Activity with the skill's name and version. An agent without a computer
//! runs nothing; its scripts stay text it can read.
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::computers::HOME;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunScript {
    pub skill: String,
    /// The script's path inside the skill, such as `scripts/make_invoice.py`.
    pub script: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub seconds: Option<u64>,
}

/// The program that runs a script, by its extension.
fn interpreter(script: &str) -> Option<&'static str> {
    match script.rsplit_once('.')?.1 {
        "py" => Some("python3"),
        "sh" => Some("bash"),
        "js" | "mjs" => Some("node"),
        _ => None,
    }
}

/// One shell word: `it's` becomes `'it'\''s'`.
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// Where the skill's files are kept on the computer, relative to its home.
pub(super) fn folder(name: &str, version: i32) -> String {
    format!("shared/skills/{name}/v{version}")
}

/// The command that runs `script` of a skill copied to `folder`, from the
/// request's folder, with `arguments` as separate words.
pub(super) fn command(folder: &str, script: &str, arguments: &[String]) -> Option<String> {
    let program = interpreter(script)?;
    let mut words = vec![
        program.to_owned(),
        quoted(&format!("{HOME}/{folder}/{script}")),
    ];
    words.extend(arguments.iter().map(|a| quoted(a)));
    Some(words.join(" "))
}

fn refused(message: &str) -> Value {
    super::super::computers::refused(message)
}

impl PgStore {
    pub(crate) async fn run_skill_script(&self, job: &Job, input: &RunScript) -> Result<Value> {
        if input.arguments.len() > 20
            || input
                .arguments
                .iter()
                .any(|a| a.len() > 500 || a.contains('\0'))
        {
            return Ok(refused(
                "Give at most 20 arguments of up to 500 characters each.",
            ));
        }
        // The version current when the request started, like load_skill.
        let found = query(&self.db, "SELECT s.id,s.name,v.version,v.files FROM relay_agent_skills x
            JOIN relay_skills s ON s.workspace=x.workspace AND s.id=x.skill
            JOIN LATERAL (SELECT version,files FROM relay_skill_versions v WHERE v.workspace=s.workspace AND v.skill=s.id
              AND v.created_ms<=COALESCE((SELECT created_ms FROM relay_chat_jobs j WHERE j.reply=$4),v.created_ms)
              ORDER BY version DESC LIMIT 1) v ON TRUE
            WHERE x.workspace=$1 AND x.agent=$2 AND s.name=$3",
            vec![job.workspace.clone().into(), job.agent.clone().into(), input.skill.clone().into(), job.reply.clone().into()]).await?;
        let Some(row) = found else {
            return Ok(
                json!({"state":"not_found","message":"No skill with that name is available to you. Use only the skills listed in your instructions."}),
            );
        };
        let (id, name, version): (String, String, i32) = (
            row.try_get("", "id")?,
            row.try_get("", "name")?,
            row.try_get("", "version")?,
        );
        let files: Vec<super::files::SkillFile> =
            serde_json::from_value(row.try_get("", "files")?)?;
        if !files.iter().any(|f| f.path == input.script) || !input.script.starts_with("scripts/") {
            return Ok(
                json!({"state":"not_found","message":"That script isn't in this skill. Scripts are in its scripts/ folder.",
                "scripts":files.iter().filter(|f| f.path.starts_with("scripts/")).map(|f| &f.path).collect::<Vec<_>>()}),
            );
        }
        let folder = folder(&name, version);
        let Some(command) = command(&folder, &input.script, &input.arguments) else {
            return Ok(refused(
                "Cuckoo runs .py, .sh and .js scripts. Follow the skill's written steps instead.",
            ));
        };
        // Without a working computer nothing runs; the script stays text to read.
        let Some(settings) =
            super::super::computers::agent_computer(&self.db, &job.workspace, &job.agent).await?
        else {
            return Ok(refused("Agent unavailable"));
        };
        if !self.computer_usable(job, &settings).await? {
            return Ok(refused(
                "You have no computer, so skill scripts can't run. Follow the skill's written steps instead.",
            ));
        }
        if settings.programs == "block" {
            return Ok(refused(
                "Running programs on your computer is blocked, so this script can't run. Follow the skill's written steps, or tell the person a manager can allow programs in your Tools tab.",
            ));
        }
        // The skill's files go to the computer first; the command then runs under
        // the Programs rule, asking first with this exact command when set to.
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        for file in &files {
            self.computer_put(
                &ready,
                &format!("{HOME}/{folder}/{}", file.path),
                file.text.clone().into_bytes(),
            )
            .await?;
        }
        let run = super::super::computer_tools::Run {
            command,
            seconds: input.seconds,
        };
        let result = self.computer_run(job, &run).await?;
        if result["state"] == "completed" {
            execute(&self.db, "INSERT INTO relay_skill_loads(workspace,job,skill,version,path) VALUES($1,$2,$3,$4,$5)",
                vec![job.workspace.clone().into(), job.reply.clone().into(), id.into(), version.into(), format!("run:{}", input.script).into()]).await?;
        }
        let mut result = result;
        result["skill"] = json!(name);
        result["version"] = json!(version);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scripts_run_with_their_interpreter_and_arguments_stay_separate_words() {
        assert_eq!(
            command(
                "shared/skills/invoice-pdf/v3",
                "scripts/make.py",
                &["1042".into(), "it's; rm -rf ~".into()]
            )
            .unwrap(),
            format!(
                "python3 '{HOME}/shared/skills/invoice-pdf/v3/scripts/make.py' '1042' 'it'\\''s; rm -rf ~'"
            )
        );
        assert!(
            command("f", "scripts/run.sh", &[])
                .unwrap()
                .starts_with("bash ")
        );
        assert!(
            command("f", "scripts/run.mjs", &[])
                .unwrap()
                .starts_with("node ")
        );
        assert_eq!(command("f", "scripts/run.exe", &[]), None);
        assert_eq!(command("f", "scripts/README", &[]), None);
    }
}
