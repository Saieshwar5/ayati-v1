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
        if self.computers.is_none() || !settings.on {
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
        let seconds = input
            .seconds
            .unwrap_or(120)
            .clamp(1, LONGEST_COMMAND.as_secs());
        let started = std::time::Instant::now();
        let cwd = format!("{HOME}/{}", folder(job));
        let ran = ready
            .routed
            .provider
            .run(
                &ready.call(),
                &ready.awake,
                command,
                &cwd,
                Duration::from_secs(seconds),
            )
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
        Ok(json!({
            "state": "completed",
            "exit_code": ran.exit_code,
            "timed_out": ran.timed_out,
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
        let ran = ready
            .routed
            .provider
            .run(
                &ready.call(),
                &ready.awake,
                &listing,
                HOME,
                Duration::from_secs(30),
            )
            .await?;
        self.computer_used(job).await?;
        let files: Vec<Value> = ran.stdout.lines().filter_map(|line| {
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
        let bytes = ready
            .routed
            .provider
            .read(&ready.call(), &ready.awake, &path)
            .await;
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
        ready
            .routed
            .provider
            .write(
                &ready.call(),
                &ready.awake,
                &path,
                input.content.clone().into_bytes(),
            )
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
        let name: String = query(
            &self.db,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), job.agent.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "name"))
        .transpose()?
        .unwrap_or_else(|| "the agent".into());
        let digest = crate::identity::provider::digest(command);
        let key = format!("computer-{}", &digest[..20.min(digest.len())]);
        let specification = json!({
            "version": 1,
            "kind": "single_choice",
            "title": format!("May {name} run this on its computer?"),
            "description": command.chars().take(1000).collect::<String>(),
            "fallback": "Reply yes or no.",
            "options": [{"id": "yes", "label": "Yes, run it"}, {"id": "no", "label": "No"}],
        });
        let tx = self.read_tx(&job.workspace).await?;
        let answer = super::built_apps::asking::answered(&tx, job, &key, &specification).await?;
        tx.commit().await?;
        Ok(match answer {
            Some(true) => None,
            Some(false) => Some(super::computers::refused(
                "The person said no to this command. Don't run it; tell them what you'll do instead.",
            )),
            None => {
                self.computer_step(job, "command", command, "pending", None, "", 0, 0)
                    .await?;
                Some(
                    json!({"state": "needs_confirmation", "question": {"key": key, "specification": specification},
                    "message": "Running programs asks first. Call ask_question with this question unchanged, then run the same command again after the person answers."}),
                )
            }
        })
    }

    /// Records one step for Activity and the live view.
    #[allow(clippy::too_many_arguments)]
    async fn computer_step(
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
