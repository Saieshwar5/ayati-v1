//! The Library indexer (LB2b): a controller task that embeds new and changed passages
//! and writes a one-line summary per document, a batch at a time. Saving never waits
//! for it; until it catches up, word search still finds everything.
use super::models;
use crate::identity::Shared;
use std::time::Duration;

const SUMMARIES_PER_ROUND: i64 = 4;

pub(in crate::identity) fn start(app: Shared) {
    let Some(configured) = app.store.library_models.clone() else {
        return;
    };
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        loop {
            let busy = round(&app.store, &configured).await;
            let pause = if busy {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(3)
            };
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tokio::time::sleep(pause) => {}
            }
        }
    });
}

/// One batch of embeddings and a few summaries; true when there was work.
pub(crate) async fn round(
    store: &crate::identity::postgres::PgStore,
    configured: &models::Configured,
) -> bool {
    let mut busy = false;
    if let Some(settings) = configured.embedding() {
        match store.library_pending_embeddings(settings.model).await {
            Ok(Some(pending)) => {
                busy = true;
                let texts: Vec<String> = pending.passages.iter().map(|p| p.3.clone()).collect();
                match models::embed(&settings, &texts).await {
                    Ok((vectors, tokens)) => {
                        let embedded: Vec<_> = pending
                            .passages
                            .iter()
                            .zip(vectors)
                            .map(|(p, v)| ((p.0.clone(), p.1, p.2), models::vector_text(&v)))
                            .collect();
                        let stored = store
                            .library_store_embeddings(&pending.workspace, settings.model, &embedded)
                            .await;
                        let _ = store
                            .record_model_usage(
                                &pending.workspace,
                                "library",
                                "",
                                None,
                                settings.provider,
                                settings.model,
                                tokens,
                                0,
                            )
                            .await;
                        if stored.is_err() {
                            tracing::warn!("Library embeddings could not be stored");
                        }
                    }
                    Err(_) => {
                        tracing::warn!("Library embeddings unavailable; word search continues");
                        busy = false;
                    }
                }
            }
            Ok(None) => {}
            Err(_) => tracing::warn!("Library embedding queue unavailable"),
        }
    }
    if let Some(settings) = configured.summary() {
        let pending = store
            .library_pending_summaries(SUMMARIES_PER_ROUND)
            .await
            .unwrap_or_default();
        for document in pending {
            let (Some(w), Some(item), Some(revision)) = (
                document["workspace"].as_str(),
                document["item"].as_str(),
                document["revision"].as_i64(),
            ) else {
                continue;
            };
            match models::summarize(
                &settings,
                document["title"].as_str().unwrap_or(""),
                document["text"].as_str().unwrap_or(""),
            )
            .await
            {
                Ok((line, input, output)) => {
                    busy = true;
                    let _ = store
                        .library_store_summary(w, item, revision, settings.model, &line)
                        .await;
                    let _ = store
                        .record_model_usage(
                            w,
                            "library",
                            "",
                            None,
                            settings.provider,
                            settings.model,
                            input,
                            output,
                        )
                        .await;
                }
                Err(_) => {
                    tracing::warn!("Library summaries unavailable");
                    break;
                }
            }
        }
    }
    busy
}
