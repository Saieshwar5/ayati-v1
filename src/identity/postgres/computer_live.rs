//! Watching an agent's browser and taking it over (designs/agent-computers 3, 11 and 12).
//! The agent's owner and admins may watch, and so may the person whose request holds the
//! computer. Only that person controls the browser, and only while a hand-over they were
//! asked for is open. Watching never wakes a computer.
use super::*;
use crate::identity::computers::BROWSER_HELPER_PATH;
use crate::identity::computers::live::{Reach, Watch};
use serde::Deserialize;
use serde_json::json;

/// A live picture joined for one person.
pub(crate) struct Watching {
    /// The computer's feed, to leave when they stop watching.
    pub id: String,
    pub watch: Watch,
    /// The first event: whether they control the browser now, and for which request.
    pub hello: Value,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Mouse {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Wheel {
    pub x: f64,
    pub y: f64,
    pub delta_y: f64,
}
/// One thing a person does in the browser: a mouse event, a scroll, text or a key.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct LiveInput {
    #[serde(default)]
    pub mouse: Option<Mouse>,
    #[serde(default)]
    pub wheel: Option<Wheel>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
}
const KEYS: [&str; 13] = [
    "Enter",
    "Tab",
    "Escape",
    "Backspace",
    "ArrowDown",
    "ArrowUp",
    "ArrowLeft",
    "ArrowRight",
    "PageDown",
    "PageUp",
    "Home",
    "End",
    "Space",
];
impl LiveInput {
    /// The helper's input line, built from checked fields only.
    fn line(&self) -> Result<String> {
        let place = |x: f64, y: f64| (0.0..=4000.0).contains(&x) && (0.0..=4000.0).contains(&y);
        let line = match (&self.mouse, &self.wheel, &self.text, &self.key) {
            (Some(m), None, None, None) => {
                ensure!(
                    ["mousePressed", "mouseReleased", "mouseMoved"].contains(&m.kind.as_str())
                        && place(m.x, m.y),
                    "Invalid mouse event"
                );
                let button = if m.kind == "mouseMoved" {
                    "none"
                } else {
                    "left"
                };
                json!({"mouse": {"type": m.kind, "x": m.x, "y": m.y, "button": button, "clickCount": 1}})
            }
            (None, Some(w), None, None) => {
                ensure!(
                    place(w.x, w.y) && w.delta_y.abs() <= 5000.0,
                    "Invalid scroll"
                );
                json!({"wheel": {"x": w.x, "y": w.y, "deltaY": w.delta_y}})
            }
            (None, None, Some(text), None) => {
                ensure!(
                    !text.is_empty() && text.chars().count() <= 2000,
                    "Type up to 2,000 characters at a time"
                );
                json!({"text": text})
            }
            (None, None, None, Some(key)) => {
                ensure!(KEYS.contains(&key.as_str()), "Unsupported key");
                json!({"key": key})
            }
            _ => anyhow::bail!("Send one mouse event, scroll, text or key"),
        };
        Ok(line.to_string())
    }
}

/// The computer as a person sees it live.
struct Target {
    provider: String,
    id: String,
    reply: Option<String>,
    control: bool,
}

impl PgStore {
    async fn live_target(&self, user: &User, w: &str, agent: &str) -> Result<Target> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let manager = super::agent_profiles::managed_access(&tx, &user.id, w, agent, true)
            .await
            .is_ok();
        let row = query(&tx, "SELECT c.provider,c.provider_id,c.state,c.held_by,
              (SELECT j.payload->>'requester' FROM relay_chat_jobs j WHERE j.workspace=c.workspace AND j.reply=c.held_by) AS requester,
              EXISTS(SELECT 1 FROM relay_task_questions q WHERE q.workspace=c.workspace AND q.reply=c.held_by
                AND q.request_key LIKE 'browser-hand-over-%' AND q.state='pending') AS handing_over
            FROM relay_computers c WHERE c.workspace=$1 AND c.agent=$2", vec![w.into(), agent.into()]).await?;
        tx.commit().await?;
        let row = row.context("This agent has no computer yet")?;
        let requester: Option<String> = row.try_get("", "requester")?;
        let asker = requester.as_deref() == Some(user.id.as_str());
        ensure!(
            manager || asker,
            "Only the person whose request is using the computer, the agent's owner and admins can watch it"
        );
        let state: String = row.try_get("", "state")?;
        let id: Option<String> = row.try_get("", "provider_id")?;
        let id = id
            .filter(|_| state == "awake")
            .context("The computer isn't working now")?;
        Ok(Target {
            provider: row.try_get("", "provider")?,
            id,
            reply: row.try_get("", "held_by")?,
            control: asker && row.try_get::<bool>("", "handing_over")?,
        })
    }

    /// Joins the live picture of the agent's browser, starting it if nobody watches yet.
    pub(crate) async fn computer_watch(
        &self,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<Watching> {
        let computers = self.computers.clone().context("Computers are not set up")?;
        let target = self.live_target(user, w, agent).await?;
        let hello = json!({"watching": {"control": target.control, "reply": target.reply}});
        if let Some(watch) = computers.live.join(&target.id) {
            return Ok(Watching {
                id: target.id,
                watch,
                hello,
            });
        }
        let file = computers.file()?;
        let routed = file.routed(&target.provider)?;
        let key = routed.key()?;
        let call = crate::identity::computers::Call {
            api: &routed.api,
            computers: &routed.computers,
            key: &key,
        };
        let awake = match computers.fresh(&target.id) {
            Some(awake) => awake,
            None => {
                let seconds = (u64::from(file.sleep_after_minutes) + 5) * 60;
                let awake = routed.provider.wake(&call, &target.id, seconds).await?;
                computers.remember(&awake);
                awake
            }
        };
        if !computers.has_helper(&target.id) {
            routed
                .provider
                .write(
                    &call,
                    &awake,
                    BROWSER_HELPER_PATH,
                    crate::identity::computers::BROWSER_HELPER
                        .as_bytes()
                        .to_vec(),
                )
                .await?;
            computers.helper_uploaded(&target.id);
        }
        let tag = crate::identity::computers::live::tag();
        let output = routed
            .provider
            .stream(
                &call,
                &awake,
                &format!("node {BROWSER_HELPER_PATH} live '{{}}'"),
                &tag,
            )
            .await?;
        let reach = Reach {
            provider: routed.provider,
            api: routed.api.clone(),
            computers: routed.computers.clone(),
            key: key.clone(),
            awake,
        };
        let watch = computers
            .live
            .start(&target.id, tag, reach, output)
            .context("The live picture couldn't start")?;
        Ok(Watching {
            id: target.id,
            watch,
            hello,
        })
    }

    pub(crate) fn computer_unwatch(&self, id: &str) {
        if let Some(computers) = &self.computers {
            computers.live.leave(id);
        }
    }

    /// A person's input during their hand-over; it keeps the computer for their request.
    pub(crate) async fn computer_live_input(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &LiveInput,
    ) -> Result<Value> {
        let line = input.line()?;
        let computers = self.computers.clone().context("Computers are not set up")?;
        let target = self.live_target(user, w, agent).await?;
        ensure!(
            target.control,
            "You can use this browser only when the agent hands it over to you"
        );
        ensure!(
            computers.live.send(&target.id, line),
            "Open the browser view first"
        );
        let at = now();
        execute(&self.db, "UPDATE relay_computers SET held_until=GREATEST(held_until,$4),last_used=$5 WHERE workspace=$1 AND agent=$2 AND held_by=$3",
            vec![w.into(), agent.into(), target.reply.into(), (at + super::browser_tools::HAND_OVER_SECONDS).into(), at.into()]).await?;
        Ok(json!({"sent": true}))
    }
}
