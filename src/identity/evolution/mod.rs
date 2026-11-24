//! Agents that get better on their own (schema 143): a controller task that,
//! every ten minutes, gives finished answers their outcome, checks new memory
//! notes, gives notes their meaning, sorts requests into job types, and runs each
//! due agent's three-day skill review. Plain signals (refused approvals,
//! reactions, silence) always work; the rest needs `AYATI_CLASSIFIER_MODEL`, the
//! notes' meanings need `AYATI_LIBRARY_EMBEDDING_MODEL`, and skill reviews also
//! need `AYATI_LEARNING_MODEL` to draft and rehearse. See runtime/evolution.md.
pub(crate) mod classify;
pub(crate) mod review;

use crate::identity::Shared;
use crate::identity::library::models::Settings;
use crate::identity::postgres::evolution::jobs::Sorted;
use crate::identity::postgres::evolution::notes::Verdict;
use crate::identity::postgres::evolution::outcomes::by_rules;
use crate::identity::postgres::evolution::{ACT, ASK};
use crate::identity::postgres::memory::KINDS;
use crate::identity::postgres::{ModelCall, PgStore};
use anyhow::Result;
use classify::{CORRECTION, JOB, NOTE, OUTCOME, ask, choice, confidence};
use serde_json::{Value, json};
use std::time::Duration;

/// The models this task may use, from the controller's configuration.
#[derive(Clone, Default)]
pub(crate) struct Models {
    pub file: Option<std::path::PathBuf>,
    pub classifier: Option<(String, String)>,
    pub learning: Option<(String, String)>,
}
impl Models {
    fn settings<'a>(&'a self, model: &'a Option<(String, String)>) -> Option<Settings<'a>> {
        let (provider, model) = model.as_ref()?;
        Some(Settings {
            file: self.file.as_deref()?,
            provider,
            model,
        })
    }
    pub(crate) fn classifier(&self) -> Option<Settings<'_>> {
        self.settings(&self.classifier)
    }
    pub(crate) fn learning(&self) -> Option<Settings<'_>> {
        self.settings(&self.learning)
    }
}

pub(in crate::identity) fn start(app: Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let models = Models {
            file: app.config.model_providers_file.clone(),
            classifier: app.config.classifier_model.clone(),
            learning: app.config.learning_model.clone(),
        };
        loop {
            round(&app.store, &models, now_ms()).await;
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(600)) => {}
            }
        }
    });
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One round of every step. A step that fails is tried again next round; the
/// reasons logged name no conversation content.
pub(crate) async fn round(store: &PgStore, models: &Models, now_ms: i64) {
    if let Err(error) = outcomes(store, models, now_ms).await {
        tracing::warn!(reason = %error, "Outcomes were not saved");
    }
    if let Err(error) = meanings(store).await {
        tracing::warn!(reason = %error, "Notes' meanings were not saved");
    }
    if let Some(settings) = models.classifier() {
        if let Err(error) = notes(store, &settings).await {
            tracing::warn!(reason = %error, "Notes were not checked");
        }
        if let Err(error) = jobs(store, &settings).await {
            tracing::warn!(reason = %error, "Requests were not sorted into job types");
        }
        if let Some(learning) = models.learning() {
            match store.skill_reviews_due(now_ms, 3).await {
                Ok(due) => {
                    for (w, agent) in due {
                        if let Err(error) =
                            review::review(store, &settings, &learning, &w, &agent, now_ms).await
                        {
                            tracing::warn!(reason = %error, "A skill review did not finish");
                        }
                    }
                }
                Err(error) => tracing::warn!(reason = %error, "Skill reviews could not be listed"),
            }
        }
    }
}

/// Records one model call's tokens against the agent, with no catalog charge.
pub(crate) async fn usage(
    store: &PgStore,
    w: &str,
    agent: &str,
    assignment: &str,
    settings: &Settings<'_>,
    input: i64,
    output: i64,
) {
    let _ = store
        .record_model_usage(&ModelCall {
            workspace: w,
            assignment,
            agent,
            job: None,
            provider: settings.provider,
            model: settings.model,
            provider_model: settings.model,
            attempts: 1,
            person: None,
            charge_micro: 0,
            cost_micro: 0,
            latency_ms: None,
            input_tokens: input,
            output_tokens: output,
        })
        .await;
}

/// Short names (n1, n2…) for items shown to the classifier, and back.
fn numbered<T: Clone>(items: &[T], prefix: &str) -> Vec<(String, T)> {
    items
        .iter()
        .enumerate()
        .map(|(i, t)| (format!("{prefix}{}", i + 1), t.clone()))
        .collect()
}

async fn outcomes(store: &PgStore, models: &Models, now_ms: i64) -> Result<()> {
    let classifier = models.classifier();
    for c in store.outcome_candidates(60).await? {
        if let Some((outcome, confidence, source)) =
            by_rules(&c, now_ms / 1000, classifier.is_some())
        {
            store
                .save_outcome(&c, outcome, confidence, source, None)
                .await?;
            continue;
        }
        let (Some(settings), Some((_, next))) = (&classifier, &c.next) else {
            continue;
        };
        let (answer, input, output) = ask(
            settings,
            OUTCOME,
            &json!({"answer": c.answer, "next_message": next}),
        )
        .await?;
        usage(
            store,
            &c.workspace,
            &c.agent,
            "classify",
            settings,
            input,
            output,
        )
        .await;
        let outcome = match choice(
            &answer,
            "outcome",
            &["accepted", "corrected", "followed_up", "unrelated"],
        ) {
            Some("unrelated") | None => "unknown",
            Some(found) => found,
        };
        let sure = confidence(&answer);
        let used = store
            .save_outcome(&c, outcome, sure, "classifier", Some(settings.model))
            .await?;
        if outcome == "corrected" && sure >= ACT && !used.is_empty() {
            // Only the notes the correction shows to be wrong go on hold.
            let named = numbered(&used, "n");
            let shown: Vec<Value> = named
                .iter()
                .map(|(n, (_, text))| json!({"id": n, "text": text}))
                .collect();
            let (answer, input, output) = ask(
                settings,
                CORRECTION,
                &json!({"correction": next, "notes": shown}),
            )
            .await?;
            usage(
                store,
                &c.workspace,
                &c.agent,
                "classify",
                settings,
                input,
                output,
            )
            .await;
            if confidence(&answer) >= ACT {
                let wrong: Vec<String> = answer["wrong"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|n| {
                        named
                            .iter()
                            .find(|(name, _)| Some(name.as_str()) == n.as_str())
                            .map(|(_, (id, _))| id.clone())
                    })
                    .collect();
                store.hold_notes(&c.workspace, &c.job, &wrong).await?;
            }
        }
    }
    Ok(())
}

async fn notes(store: &PgStore, settings: &Settings<'_>) -> Result<()> {
    for note in store.notes_to_check(20).await? {
        let named = numbered(&note.nearest, "n");
        let nearest: Vec<Value> = named
            .iter()
            .map(|(n, near)| json!({"id": n, "text": near.1}))
            .collect();
        let (answer, input, output) = ask(
            settings,
            NOTE,
            &json!({"note": note.text, "kind": note.kind, "nearest": nearest}),
        )
        .await?;
        let agent = store
            .memory_author(&note.workspace, &note.id)
            .await?
            .unwrap_or_default();
        if !agent.is_empty() {
            usage(
                store,
                &note.workspace,
                &agent,
                "classify",
                settings,
                input,
                output,
            )
            .await;
        }
        let sure = confidence(&answer) >= ACT;
        let verdict = Verdict {
            kind: choice(&answer, "kind", &KINDS)
                .map(str::to_owned)
                .filter(|_| sure),
            one_off: sure && answer["lasting"] == false,
            private: sure && answer["private"] == true,
            instruction: sure && answer["instruction"] == true,
            relations: answer["relations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| {
                    let id = named
                        .iter()
                        .find(|(n, _)| Some(n.as_str()) == r["id"].as_str())?
                        .1
                        .0
                        .clone();
                    let relation = choice(r, "relation", &["same", "contradicts"])?;
                    Some((id, relation.to_owned(), confidence(r)))
                })
                .filter(|(_, _, c)| *c >= ASK)
                .collect(),
            model: settings.model.to_owned(),
        };
        store.save_note_check(&note, &verdict).await?;
    }
    Ok(())
}

async fn meanings(store: &PgStore) -> Result<()> {
    let Some(configured) = store.library_models.clone() else {
        return Ok(());
    };
    let Some(settings) = configured.embedding() else {
        return Ok(());
    };
    let waiting = store.notes_to_embed(settings.model, 50).await?;
    if waiting.is_empty() {
        return Ok(());
    }
    let texts: Vec<String> = waiting.iter().map(|(_, _, text)| text.clone()).collect();
    let (vectors, _) = crate::identity::library::models::embed(&settings, &texts).await?;
    let embedded: Vec<(String, String, String)> = waiting
        .into_iter()
        .zip(vectors)
        .map(|((w, id, _), v)| {
            (
                w,
                id,
                format!(
                    "[{}]",
                    v.iter().map(f32::to_string).collect::<Vec<_>>().join(",")
                ),
            )
        })
        .collect();
    store.store_note_embeddings(settings.model, &embedded).await
}

async fn jobs(store: &PgStore, settings: &Settings<'_>) -> Result<()> {
    for item in store.requests_to_sort(30).await? {
        let named = numbered(&item.types, "t");
        let types: Vec<Value> = named.iter().map(|(n, (_, name, description))| json!({"id": n, "name": name, "description": description})).collect();
        let (answer, input, output) = ask(
            settings,
            JOB,
            &json!({"request": item.request, "job_types": types}),
        )
        .await?;
        usage(
            store,
            &item.workspace,
            &item.agent,
            "classify",
            settings,
            input,
            output,
        )
        .await;
        let sorted = match answer["type"].as_str() {
            Some("new") => match answer["name"].as_str().filter(|n| !n.trim().is_empty()) {
                Some(name) => Sorted::New {
                    name: name.to_owned(),
                    description: answer["description"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                },
                None => continue,
            },
            Some(short) => match named.iter().find(|(n, _)| n == short) {
                Some((_, (id, _, _))) => Sorted::Existing(id.clone()),
                None => continue,
            },
            None => continue,
        };
        store
            .save_job_type(&item, &sorted, confidence(&answer))
            .await?;
    }
    Ok(())
}
