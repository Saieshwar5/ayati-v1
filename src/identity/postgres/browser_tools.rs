//! The agent's browser on its computer (designs/agent-computers 8 and 11): Cuckoo's
//! helper drives Chromium and the agent reads pages as text with numbered elements.
//! Browsing and pressing buttons follow the owner's controls; the computer's network
//! allows only the websites its Internet setting allows. Passwords and one-time codes
//! are typed by a person in a hand-over. See runtime/agent-computers.md.
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::computers::{BROWSER_HELPER, BROWSER_HELPER_PATH, HOME, clip};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

/// Characters of a page's text returned to the agent at once.
const PAGE_CHARS: usize = 8_000;
/// How long a person has for a hand-over before others may use the computer again.
pub(super) const HAND_OVER_SECONDS: i64 = 15 * 60;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Open {
    pub url: String,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Element {
    #[serde(rename = "ref")]
    pub element: u32,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Type {
    #[serde(rename = "ref")]
    pub element: u32,
    pub text: String,
    #[serde(default)]
    pub submit: bool,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Choose {
    #[serde(rename = "ref")]
    pub element: u32,
    pub option: String,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Press {
    pub key: String,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Screenshot {
    #[serde(default)]
    pub name: String,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct HandOver {
    /// sign_in, code, captcha, check or other.
    pub need: String,
    pub message: String,
}

/// The host of an address the agent gave, for questions and steps.
fn host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

fn short(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

impl PgStore {
    pub(crate) async fn browser_open(&self, job: &Job, input: &Open) -> Result<Value> {
        let url = input.url.trim();
        let site = host(url);
        if site.is_empty() || !(url.starts_with("https://") || url.starts_with("http://")) {
            return Ok(super::computers::refused(
                "Open an http or https address, such as https://portal.example.in/invoices.",
            ));
        }
        if let Some(stop) = self.browser_allowed(job, Some(&site)).await? {
            return Ok(stop);
        }
        self.browse(
            job,
            "open",
            json!({"url": url}),
            &format!("Opened {}", short(url, 300)),
        )
        .await
    }

    pub(crate) async fn browser_read(&self, job: &Job) -> Result<Value> {
        if let Some(stop) = self.browser_allowed(job, None).await? {
            return Ok(stop);
        }
        self.browse(job, "read", json!({}), "Read the page").await
    }

    pub(crate) async fn browser_click(&self, job: &Job, input: &Element) -> Result<Value> {
        self.pressing(
            job,
            "click",
            json!({"ref": input.element}),
            &format!("Clicked element {}", input.element),
        )
        .await
    }

    pub(crate) async fn browser_type(&self, job: &Job, input: &Type) -> Result<Value> {
        if input.text.chars().count() > 2000 {
            return Ok(super::computers::refused("Type up to 2,000 characters."));
        }
        let args = json!({"ref": input.element, "text": input.text, "submit": input.submit});
        // Typed text isn't kept: it may be private.
        let detail = format!(
            "Typed {} characters into element {}{}",
            input.text.chars().count(),
            input.element,
            if input.submit { " and sent it" } else { "" }
        );
        if input.submit {
            return self.pressing(job, "type", args, &detail).await;
        }
        if let Some(stop) = self.browser_allowed(job, None).await? {
            return Ok(stop);
        }
        self.browse(job, "type", args, &detail).await
    }

    pub(crate) async fn browser_choose(&self, job: &Job, input: &Choose) -> Result<Value> {
        if let Some(stop) = self.browser_allowed(job, None).await? {
            return Ok(stop);
        }
        let detail = format!(
            "Chose “{}” in element {}",
            short(&input.option, 60),
            input.element
        );
        self.browse(
            job,
            "choose",
            json!({"ref": input.element, "option": input.option}),
            &detail,
        )
        .await
    }

    pub(crate) async fn browser_press(&self, job: &Job, input: &Press) -> Result<Value> {
        let args = json!({"key": input.key});
        let detail = format!("Pressed {}", short(&input.key, 20));
        if input.key == "Enter" {
            return self.pressing(job, "press", args, &detail).await;
        }
        if let Some(stop) = self.browser_allowed(job, None).await? {
            return Ok(stop);
        }
        self.browse(job, "press", args, &detail).await
    }

    pub(crate) async fn browser_screenshot(&self, job: &Job, input: &Screenshot) -> Result<Value> {
        if let Some(stop) = self.browser_allowed(job, None).await? {
            return Ok(stop);
        }
        let name = if input.name.trim().is_empty() {
            format!("screenshot-{}.png", now())
        } else {
            format!("{}.png", input.name.trim().trim_end_matches(".png"))
        };
        let path = match super::computer_tools::resolve(job, &name) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        let shown = path.strip_prefix(&format!("{HOME}/")).unwrap_or(&path);
        self.browse(
            job,
            "screenshot",
            json!({"path": path}),
            &format!("Saved a screenshot {shown}"),
        )
        .await
    }

    /// Asks the person who asked to use the browser themselves: to sign in, type a code
    /// or check something. The computer stays with this request meanwhile.
    pub(crate) async fn browser_hand_over(&self, job: &Job, input: &HandOver) -> Result<Value> {
        let needs = [
            ("sign_in", "Sign in"),
            ("code", "Type a one-time code"),
            ("captcha", "Prove you're a person"),
            ("check", "Check the page"),
            ("other", "Help in the browser"),
        ];
        let Some((_, need)) = needs.iter().find(|(id, _)| *id == input.need) else {
            return Ok(super::computers::refused(
                "need is sign_in, code, captcha, check or other.",
            ));
        };
        let message = input.message.trim();
        if message.is_empty() || message.chars().count() > 500 {
            return Ok(super::computers::refused(
                "Say in up to 500 characters what the person should do.",
            ));
        }
        let name = self.computer_agent_name(job).await?;
        let digest = crate::identity::provider::digest(&format!("{}\n{message}", input.need));
        let key = format!("browser-hand-over-{}", &digest[..16.min(digest.len())]);
        let specification = json!({
            "version": 1,
            "kind": "single_choice",
            "title": format!("{name} needs you in its browser: {}", need.to_lowercase()),
            "description": format!("{message}\n\nTake over the browser, then choose Done. {name} sees nothing while you are in control."),
            "fallback": "Open Cuckoo to use the browser, then reply done or can't.",
            "options": [{"id": "yes", "label": "Done, give back"}, {"id": "no", "label": "I can't do it"}],
        });
        let waiting = self.asked(job, &key, specification, ("browser", &format!("Asked for a hand-over: {}", short(message, 300))),
            "Call ask_question with this question unchanged and finish. You continue after the person gives the browser back; then read the page.",
            "The person couldn't do it in the browser. Tell them what you'll do instead.").await?;
        if let Some(waiting) = waiting {
            if waiting["state"] == "needs_confirmation" {
                self.hand_over_picture(job, &key).await;
                // The computer stays with this request while the person uses it.
                let at = now();
                execute(&self.db, "UPDATE relay_computers SET held_until=GREATEST(held_until,$4),last_used=$5 WHERE workspace=$1 AND agent=$2 AND held_by=$3",
                    vec![job.workspace.clone().into(), job.agent.clone().into(), job.reply.clone().into(), (at + HAND_OVER_SECONDS).into(), at.into()]).await?;
            }
            return Ok(waiting);
        }
        Ok(
            json!({"state": "completed", "message": "The person finished in the browser and gave it back. Read the page to continue."}),
        )
    }

    /// A picture of the page for the person asked to help; the hand-over works without it.
    async fn hand_over_picture(&self, job: &Job, key: &str) {
        let picture = async {
            let ready = match self.computer_ready(job).await? {
                Ok(ready) => ready,
                Err(_) => return anyhow::Ok(()),
            };
            let ran = ready
                .routed
                .provider
                .run(
                    &ready.call(),
                    &ready.awake,
                    &format!("node {BROWSER_HELPER_PATH} screenshot '{{\"jpeg\":true}}'"),
                    HOME,
                    Duration::from_secs(30),
                )
                .await?;
            let shot: Value = serde_json::from_str(ran.stdout.trim()).unwrap_or(Value::Null);
            if let Some(data) = shot["jpeg"].as_str() {
                use base64::Engine as _;
                let image = base64::engine::general_purpose::STANDARD.decode(data)?;
                if image.len() <= 600_000 {
                    self.computer_shot(job, key, &image).await?;
                }
            }
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = picture {
            tracing::warn!("A hand-over's picture couldn't be taken: {error}");
        }
    }

    /// The owner's browsing control: blocked, asked first per website, or allowed.
    async fn browser_allowed(&self, job: &Job, site: Option<&str>) -> Result<Option<Value>> {
        let Some(settings) =
            super::computers::agent_computer(&self.db, &job.workspace, &job.agent).await?
        else {
            return Ok(Some(super::computers::refused("Agent unavailable")));
        };
        if self.computers.is_none() || !settings.on {
            return Ok(Some(
                self.computer_ready(job).await?.err().unwrap_or_default(),
            ));
        }
        match (settings.browser.as_str(), site) {
            ("block", _) => Ok(Some(super::computers::refused(
                "Browsing is blocked for you. Tell the person a manager can allow it in your Tools tab.",
            ))),
            ("ask", Some(site)) => {
                let name = self.computer_agent_name(job).await?;
                let digest = crate::identity::provider::digest(site);
                self.ask_first(
                    job,
                    &format!("browser-site-{}", &digest[..16.min(digest.len())]),
                    &format!("May {name} open {site} in its browser?"),
                    &format!("{name} wants to read pages on {site} for this request."),
                    ("browser", &format!("Asked to open {site}")),
                )
                .await
            }
            _ => Ok(None),
        }
    }

    /// Clicks and sending: links are browsing; pressing a button or sending a form
    /// follows the owner's "press buttons" control, asked per element when it is Ask first.
    async fn pressing(
        &self,
        job: &Job,
        action: &str,
        mut args: Value,
        detail: &str,
    ) -> Result<Value> {
        if let Some(stop) = self.browser_allowed(job, None).await? {
            return Ok(stop);
        }
        let Some(settings) =
            super::computers::agent_computer(&self.db, &job.workspace, &job.agent).await?
        else {
            return Ok(super::computers::refused("Agent unavailable"));
        };
        args["may_submit"] = json!(settings.submit == "allow");
        let tried = self.browse(job, action, args.clone(), detail).await?;
        let Some(what) = tried["needs_submit"].as_str().map(str::to_owned) else {
            return Ok(tried);
        };
        let site = host(tried["url"].as_str().unwrap_or(""));
        if settings.submit == "block" {
            return Ok(super::computers::refused(
                "Pressing buttons and sending forms on websites is blocked for you. Tell the person a manager can allow it in your Tools tab.",
            ));
        }
        let name = self.computer_agent_name(job).await?;
        let digest = crate::identity::provider::digest(&format!("{site}\n{what}"));
        if let Some(waiting) = self
            .ask_first(
                job,
                &format!("browser-press-{}", &digest[..16.min(digest.len())]),
                &format!("May {name} {what} on {site}?"),
                &format!(
                    "{name} wants to {what} on {} for this request.",
                    short(tried["url"].as_str().unwrap_or(""), 300)
                ),
                ("browser", &format!("Asked to {what} on {site}")),
            )
            .await?
        {
            return Ok(waiting);
        }
        args["may_submit"] = json!(true);
        self.browse(job, action, args, detail).await
    }

    /// Runs one helper action on the agent's computer, records it and returns the page.
    async fn browse(&self, job: &Job, action: &str, args: Value, detail: &str) -> Result<Value> {
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        let computers = self.computers.clone().context("Computers unavailable")?;
        if !computers.has_helper(&ready.awake.id) {
            ready
                .routed
                .provider
                .write(
                    &ready.call(),
                    &ready.awake,
                    BROWSER_HELPER_PATH,
                    BROWSER_HELPER.as_bytes().to_vec(),
                )
                .await?;
            computers.helper_uploaded(&ready.awake.id);
        }
        let quoted = format!("'{}'", args.to_string().replace('\'', "'\\''"));
        let started = std::time::Instant::now();
        let ran = ready
            .routed
            .provider
            .run(
                &ready.call(),
                &ready.awake,
                &format!("node {BROWSER_HELPER_PATH} {action} {quoted}"),
                HOME,
                Duration::from_secs(60),
            )
            .await;
        self.computer_used(job).await?;
        let milliseconds = started.elapsed().as_millis() as i64;
        let page: Value = match &ran {
            Ok(ran) => serde_json::from_str(ran.stdout.trim()).unwrap_or_else(|_| {
                json!({"error": format!("The browser didn't answer: {}", clip(ran.stderr.trim(), 300))})
            }),
            Err(error) => json!({"error": format!("The browser couldn't be reached: {error}")}),
        };
        if page.get("needs_submit").is_some() {
            return Ok(page);
        }
        if let Some(error) = page["error"].as_str() {
            self.computer_step(
                job,
                "browser",
                detail,
                "failed",
                None,
                error,
                0,
                milliseconds,
            )
            .await?;
            return Ok(json!({"state": "failed", "message": error}));
        }
        let title = page["title"].as_str().unwrap_or("");
        let url = page["url"].as_str().unwrap_or("");
        let output = format!("{}\n{}", short(title, 200), short(url, 500));
        self.computer_step(
            job,
            "browser",
            detail,
            "completed",
            None,
            output.trim(),
            0,
            milliseconds,
        )
        .await?;
        let mut answer = page;
        answer["state"] = json!("completed");
        if let Some(text) = answer["text"].as_str() {
            answer["text"] = json!(clip(text, PAGE_CHARS));
        }
        Ok(answer)
    }
}
