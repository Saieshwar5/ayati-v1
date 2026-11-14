//! What agents do on their computers: run commands and work with files, each request in
//! its own folder. Every step is recorded for Activity. See runtime/agent-computers.md.
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::computers::{HOME, KEPT_BYTES, LONGEST_COMMAND, SHOWN_CHARS, clip};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

/// Longest text file returned to the agent at once, and the most it may write.
const READ_CHARS: usize = 30_000;
const WRITE_BYTES: usize = 1024 * 1024;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Run {
    pub command: String,
    /// The longest it may run, 1-600 seconds; two minutes unless given.
    #[serde(default)]
    pub seconds: Option<u64>,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct FilePath {
    #[serde(default)]
    pub path: String,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Write {
    pub path: String,
    pub content: String,
}

/// "Used its computer: 3 commands and 4 steps in the browser", linking to the steps in
/// Activity, under a reply that used the computer (designs/agent-computers 10). Made by
/// Cuckoo from its records, not by the model.
pub(super) async fn used_line(db: &impl ConnectionTrait, job: &Job) -> Result<String> {
    if !super::model_budgets::present(db, "relay_computer_steps").await? {
        return Ok(String::new());
    }
    let row = query(
        db,
        "SELECT count(*) FILTER (WHERE kind='command')::bigint AS commands,
          count(*) FILTER (WHERE kind='browser')::bigint AS pages,
          count(*) FILTER (WHERE kind IN ('list','read','write'))::bigint AS files
        FROM relay_computer_steps WHERE workspace=$1 AND reply=$2 AND state<>'pending'",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?
    .context("Steps unavailable")?;
    let count = |name: &str| row.try_get::<i64>("", name);
    let mut parts = vec![];
    for (n, one, many) in [
        (count("commands")?, "1 command", "commands"),
        (
            count("pages")?,
            "1 step in the browser",
            "steps in the browser",
        ),
        (count("files")?, "1 file step", "file steps"),
    ] {
        if n == 1 {
            parts.push(one.to_owned());
        } else if n > 1 {
            parts.push(format!("{n} {many}"));
        }
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    let listed = match parts.len() {
        1 => parts[0].clone(),
        _ => format!(
            "{} and {}",
            parts[..parts.len() - 1].join(", "),
            parts[parts.len() - 1]
        ),
    };
    Ok(format!(
        "\n\nUsed its computer: {listed}. [See the steps](/w/{}/?view=activity&tab=history&run={})",
        job.workspace, job.reply
    ))
}

/// This request's own folder, relative to the agent's home.
fn folder(job: &Job) -> String {
    format!("requests/{}", job.reply)
}

/// A path the agent gave: inside this request's folder, or the agent's shared folder
/// when it starts with `shared/`. Never absolute and never `..`.
pub(super) fn resolve(job: &Job, path: &str) -> std::result::Result<String, Value> {
    let path = path.trim().trim_start_matches("./");
    let bad = path.starts_with('/')
        || path.len() > 500
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| part == "..");
    if bad {
        return Err(super::computers::refused(
            "Use a path inside this request's folder, such as invoices/may.csv, or in shared/ for files kept for every request.",
        ));
    }
    let relative = if path == "shared" || path.starts_with("shared/") {
        path.to_owned()
    } else if path.is_empty() {
        folder(job)
    } else {
        format!("{}/{path}", folder(job))
    };
    Ok(format!("{HOME}/{}", relative.trim_end_matches('/')))
}

/// What a person would recognise: the path within the agent's home.
fn shown(path: &str) -> &str {
    path.strip_prefix(HOME)
        .unwrap_or(path)
        .trim_start_matches('/')
}

impl PgStore {
    pub(crate) async fn computer_run(&self, job: &Job, input: &Run) -> Result<Value> {
        let command = input.command.trim();
        if command.is_empty() || command.len() > 4000 || command.contains('\0') {
            return Ok(super::computers::refused(
                "Give one command of up to 4,000 characters.",
            ));
        }
        let Some(settings) =
            super::computers::agent_computer(&self.db, &job.workspace, &job.agent).await?
        else {
            return Ok(super::computers::refused("Agent unavailable"));
        };
        // Nobody is asked about a command that couldn't run anyway.
        if !self.computer_usable(job, &settings).await? {
            return Ok(self.computer_ready(job).await?.err().unwrap_or_default());
        }
        match settings.programs.as_str() {
            "block" => {
                return Ok(super::computers::refused(
                    "Running programs on your computer is blocked. Work with files only, or tell the person a manager can allow programs in your Tools tab.",
                ));
            }
            "ask" => {
                if let Some(waiting) = self.computer_permission(job, command).await? {
                    return Ok(waiting);
                }
            }
            _ => {}
        }
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        let asked = input
            .seconds
            .unwrap_or(60)
            .clamp(1, LONGEST_COMMAND.as_secs());
        // A command never outlasts the request it runs for.
        let left = self.request_seconds_left(job).await?;
        let seconds = left.map_or(asked, |left| asked.min(left.saturating_sub(20).max(5)));
        let started = std::time::Instant::now();
        let cwd = format!("{HOME}/{}", folder(job));
        let ran = self
            .computer_exec(&ready, command, &cwd, Duration::from_secs(seconds))
            .await;
        self.computer_used(job).await?;
        let milliseconds = started.elapsed().as_millis() as i64;
        let ran = match ran {
            Ok(ran) => ran,
            Err(error) => {
                self.computer_step(
                    job,
                    "command",
                    command,
                    "failed",
                    None,
                    &error.to_string(),
                    0,
                    milliseconds,
                )
                .await?;
                // A workspace machine may have run it without answering.
                if ready.shared.is_some() {
                    return Ok(
                        json!({"state": "unavailable", "message": format!("{error}. The command may have run anyway: check before running it again.")}),
                    );
                }
                return Ok(
                    json!({"state": "unavailable", "message": "The command could not be run on your computer just now. Try again in a minute."}),
                );
            }
        };
        let mut kept = ran.stdout.clone();
        if !ran.stderr.is_empty() {
            kept.push_str("\n[stderr]\n");
            kept.push_str(&ran.stderr);
        }
        let state = if ran.exit_code == 0 {
            "completed"
        } else {
            "failed"
        };
        self.computer_step(
            job,
            "command",
            command,
            state,
            Some(ran.exit_code),
            &clip(&kept, 2 * KEPT_BYTES),
            ran.output_bytes as i64,
            milliseconds,
        )
        .await?;
        let mut stopped = Value::Null;
        if ran.timed_out && seconds < asked {
            stopped = json!(
                "This request's time was nearly up, so the command was stopped early. Save work to files and continue in smaller steps."
            );
        }
        Ok(json!({
            "state": "completed",
            "exit_code": ran.exit_code,
            "timed_out": ran.timed_out,
            "note": stopped,
            "stdout": clip(&ran.stdout, SHOWN_CHARS),
            "stderr": clip(&ran.stderr, 2_000),
            "folder": folder(job),
        }))
    }

    pub(crate) async fn computer_list_files(&self, job: &Job, input: &FilePath) -> Result<Value> {
        let path = match resolve(job, &input.path) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        let quoted = format!("'{}'", path.replace('\'', "'\\''"));
        let listing = format!(
            "mkdir -p {quoted} && find {quoted} -mindepth 1 -maxdepth 1 -printf '%y\\t%s\\t%TY-%Tm-%Td %TH:%TM\\t%f\\n' | sort -t \"$(printf '\\t')\" -k4 | head -200"
        );
        let listed = self.computer_ls(&ready, &path, &listing).await?;
        self.computer_used(job).await?;
        let files: Vec<Value> = listed.lines().filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let (kind, size, changed, name) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            Some(json!({"name": name, "folder": kind == "d", "bytes": size.parse::<u64>().unwrap_or(0), "changed": changed}))
        }).collect();
        self.computer_step(job, "list", shown(&path), "completed", None, "", 0, 0)
            .await?;
        Ok(json!({"state": "completed", "path": shown(&path), "files": files}))
    }

    pub(crate) async fn computer_read_file(&self, job: &Job, input: &FilePath) -> Result<Value> {
        let path = match resolve(job, &input.path) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        let bytes = self.computer_get(&ready, &path).await;
        self.computer_used(job).await?;
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(_) => {
                return Ok(
                    json!({"state": "invalid", "message": format!("There is no file {} on your computer.", shown(&path))}),
                );
            }
        };
        self.computer_step(
            job,
            "read",
            shown(&path),
            "completed",
            None,
            "",
            bytes.len() as i64,
            0,
        )
        .await?;
        match String::from_utf8(bytes.clone()) {
            Ok(text) => Ok(
                json!({"state": "completed", "path": shown(&path), "bytes": bytes.len(), "text": clip(&text, READ_CHARS)}),
            ),
            Err(_) => Ok(
                json!({"state": "completed", "path": shown(&path), "bytes": bytes.len(), "text": null,
                "message": "This is not a text file. Use a command to read it, for example pdftotext for a PDF."}),
            ),
        }
    }

    pub(crate) async fn computer_write_file(&self, job: &Job, input: &Write) -> Result<Value> {
        let path = match resolve(job, &input.path) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        if path.ends_with(&folder(job)) || input.content.len() > WRITE_BYTES {
            return Ok(super::computers::refused(
                "Write a named file of up to 1 MB.",
            ));
        }
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        self.computer_put(&ready, &path, input.content.clone().into_bytes())
            .await?;
        self.computer_used(job).await?;
        self.computer_step(
            job,
            "write",
            shown(&path),
            "completed",
            None,
            "",
            input.content.len() as i64,
            0,
        )
        .await?;
        Ok(json!({"state": "completed", "path": shown(&path), "bytes": input.content.len()}))
    }

    /// For "Ask first": none once the person said yes to this exact command; otherwise
    /// the question to ask, or the refusal when they said no.
    async fn computer_permission(&self, job: &Job, command: &str) -> Result<Option<Value>> {
        let name = self.computer_agent_name(job).await?;
        let digest = crate::identity::provider::digest(command);
        self.ask_first(
            job,
            &format!("computer-{}", &digest[..20.min(digest.len())]),
            &format!("May {name} run this on its computer?"),
            &command.chars().take(1000).collect::<String>(),
            ("command", command),
        )
        .await
    }

    /// Seconds before this request's attempt must end, when it runs on an executor.
    async fn request_seconds_left(&self, job: &Job) -> Result<Option<u64>> {
        let Some(claim) = &job.claim else {
            return Ok(None);
        };
        let row = query(&self.db, "SELECT (a.started_ms/1000 + j.run_seconds - extract(epoch FROM clock_timestamp())::bigint)::bigint AS left_seconds
            FROM relay_worker_assignments a JOIN relay_chat_jobs j ON j.workspace=a.workspace AND j.reply=a.reply
            WHERE a.id=$1 AND a.started_ms IS NOT NULL", vec![claim.clone().into()]).await?;
        Ok(row
            .map(|r| r.try_get::<i64>("", "left_seconds"))
            .transpose()?
            .map(|left| left.max(0) as u64))
    }

    pub(super) async fn computer_agent_name(&self, job: &Job) -> Result<String> {
        Ok(query(
            &self.db,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), job.agent.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "name"))
        .transpose()?
        .unwrap_or_else(|| "the agent".into()))
    }

    /// A yes/no question to the person who asked, under `key`: none once they said yes,
    /// the refusal when they said no, otherwise the question for the agent to ask. A
    /// waiting step (`kind`, `detail`) is recorded for Activity.
    pub(super) async fn ask_first(
        &self,
        job: &Job,
        key: &str,
        title: &str,
        description: &str,
        (kind, detail): (&str, &str),
    ) -> Result<Option<Value>> {
        let specification = json!({
            "version": 1,
            "kind": "single_choice",
            "title": title,
            "description": description,
            "fallback": "Reply yes or no.",
            "options": [{"id": "yes", "label": "Yes"}, {"id": "no", "label": "No"}],
        });
        self.asked(job, key, specification, (kind, detail),
            "This asks first. Call ask_question with this question unchanged, then try again after the person answers.",
            "The person said no. Don't do it; tell them what you'll do instead.").await
    }

    /// Whether the person answered the question under `key` yes; otherwise the question.
    pub(super) async fn asked(
        &self,
        job: &Job,
        key: &str,
        specification: Value,
        (kind, detail): (&str, &str),
        ask: &str,
        refused: &str,
    ) -> Result<Option<Value>> {
        let tx = self.read_tx(&job.workspace).await?;
        let answer = super::built_apps::asking::answered(&tx, job, key, &specification).await?;
        tx.commit().await?;
        Ok(match answer {
            Some(true) => None,
            Some(false) => Some(super::computers::refused(refused)),
            None => {
                self.computer_step(job, kind, detail, "pending", None, "", 0, 0)
                    .await?;
                Some(
                    json!({"state": "needs_confirmation", "question": {"key": key, "specification": specification}, "message": ask}),
                )
            }
        })
    }

    /// Records one step for Activity and the live view.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn computer_step(
        &self,
        job: &Job,
        kind: &str,
        detail: &str,
        state: &str,
        exit_code: Option<i32>,
        output: &str,
        bytes: i64,
        milliseconds: i64,
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        query(
            &tx,
            "SELECT pg_advisory_xact_lock(hashtext('relay_computer_steps:'||$1||':'||$2))",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?;
        execute(&tx, "INSERT INTO relay_computer_steps(workspace,reply,position,agent,kind,detail,state,exit_code,output,output_bytes,milliseconds,at)
            SELECT $1,$2,COALESCE(max(position),0)+1,$3,$4,$5,$6,$7,$8,$9,$10,$11 FROM relay_computer_steps WHERE workspace=$1 AND reply=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into(), job.agent.clone().into(), kind.into(), detail.chars().take(4000).collect::<String>().into(), state.into(), exit_code.into(), output.chars().take(140_000).collect::<String>().into(), bytes.into(), milliseconds.into(), now().into()]).await?;
        tx.commit().await?;
        Ok(())
    }
}
