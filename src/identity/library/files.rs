use super::*;
use axum::{body::Bytes, http::HeaderMap};
use std::time::Duration;
/// The longest an upload's body may take: 25 MiB at about 90 KB/s.
const UPLOAD_TIME: Duration = Duration::from_secs(300);
/// A download ends when its client reads nothing for this long.
const STALLED: Duration = Duration::from_secs(60);
const CHUNK: usize = 64 * 1024;

/// A transfer's place, taken only after sign-in and access are checked.
fn slot(app: &App, w: &str) -> ApiResult<transfer_slots::Slot> {
    app.runtimes
        .library_transfers
        .take(w)
        .map_err(|why| Error(anyhow::anyhow!(why), StatusCode::TOO_MANY_REQUESTS))
}

/// The response body, sent in 64 KiB pieces by a task that holds the
/// transfer's slot. A client that reads nothing for `stalled` ends the
/// transfer, so a stuck connection can't keep the slot, and the file in
/// memory, for ever.
fn streamed(bytes: Bytes, slot: transfer_slots::Slot, stalled: Duration) -> Body {
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(1);
    tokio::spawn(async move {
        let _slot = slot;
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + CHUNK).min(bytes.len());
            match tokio::time::timeout(stalled, sender.send(Ok(bytes.slice(offset..end)))).await {
                Ok(Ok(())) => offset = end,
                // The client stopped reading or left.
                _ => return,
            }
        }
    });
    Body::from_stream(futures_util::stream::unfold(receiver, |mut r| async move {
        r.recv().await.map(|piece| (piece, r))
    }))
}
pub(super) async fn start(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<UploadFile>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        app.library.enabled(),
        "File storage is not configured. You can still create pages.",
    )?;
    Ok(Json(
        app.store.library_upload_start(&user, &w, &input).await?,
    ))
}
pub(super) async fn status(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut session = app.store.library_upload_session(&user, &w, &id).await?;
    if let Some(value) = session.as_object_mut() {
        value.remove("object_key");
    }
    Ok(Json(session))
}
pub(super) async fn cancel(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_upload_cancel(&user, &w, &id).await?))
}
pub(super) async fn upload(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    request: Request<Body>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, request.headers()).await?;
    let session = app.store.library_upload_session(&user, &w, &id).await?;
    let _slot = slot(&app, &w)?;
    let bytes = tokio::time::timeout(
        UPLOAD_TIME,
        axum::body::to_bytes(request.into_body(), MAX_FILE),
    )
    .await
    .context("The upload took longer than five minutes. Retry on a faster connection.")?
    .context("File exceeds the 25 MiB limit or transfer was interrupted")?;
    check(
        bytes.len() as i64 == session["size"].as_i64().unwrap_or(0)
            && checksum(&bytes) == session["checksum"],
        "Selected file does not match this upload; reselect the original file",
    )?;
    if session["state"] == "stored" {
        let target = session["item"].as_str().unwrap_or(&id);
        return Ok(Json(app.store.library_get(&user, &w, target).await?));
    }
    let key = session["object_key"]
        .as_str()
        .context("Upload key missing")?;
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        app.library.put(key, bytes.clone()),
    )
    .await
    .context("Storage upload was not confirmed. Retry the same file.")??;
    let mime = extract::mime(&bytes);
    let (text, readiness) = extract::text(&bytes, mime).await;
    Ok(Json(
        app.store
            .library_upload_finish(&user, &w, &id, mime, &text, readiness)
            .await?,
    ))
}
#[derive(Deserialize)]
pub(super) struct Download {
    #[serde(default)]
    inline: bool,
    /// An earlier version of the file, for people who can edit it.
    version: Option<i64>,
}
pub(super) async fn download(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<Download>,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let mut item = app.store.library_file(&user, &w, &id).await?;
    let slot = slot(&app, &w)?;
    if let Some(version) = input.version {
        let held = app
            .store
            .library_file_version(&user, &w, &id, version)
            .await?;
        for field in ["title", "object_key", "mime", "size", "checksum"] {
            item[field] = held[field].clone();
        }
    }
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        app.library.get(
            item["object_key"]
                .as_str()
                .context("Stored object missing")?,
        ),
    )
    .await
    .context("Stored file transfer timed out; try again shortly")??;
    check(
        bytes.len() as i64 == item["size"].as_i64().unwrap_or(0)
            && checksum(&bytes) == item["checksum"],
        "Stored file failed its integrity check",
    )?;
    // A revocation during the storage read must fence the outgoing response too.
    app.store.library_file(&user, &w, &id).await?;
    // The slot stays with the body while the file is sent, and is freed when the
    // client finishes, leaves or stalls.
    let mut response = crate::media::ranged(&headers, bytes, |selected| {
        streamed(selected, slot, STALLED)
    })?;
    let mime = item["mime"].as_str().unwrap_or("application/octet-stream");
    let inline = input.inline
        && ["image/png", "image/jpeg", "application/pdf", "text/plain"].contains(&mime);
    let name = item["title"].as_str().unwrap_or("download");
    let encoded: String = name.bytes().map(|b| format!("%{b:02X}")).collect();
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, mime.parse()?);
    h.insert(
        header::CONTENT_DISPOSITION,
        format!(
            "{}; filename*=UTF-8''{encoded}",
            if inline { "inline" } else { "attachment" }
        )
        .parse()?,
    );
    h.insert(header::CACHE_CONTROL, "private, no-store".parse()?);
    h.insert("x-content-type-options", "nosniff".parse()?);
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        "sandbox; default-src 'none'".parse()?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_download_nobody_reads_frees_its_place() {
        let transfers = transfer_slots::Transfers::default();
        let slots: Vec<_> = (0..transfer_slots::PER_WORKSPACE)
            .map(|_| transfers.take("w").unwrap())
            .collect();
        assert!(transfers.take("w").is_err());
        // Two bodies whose client never reads past the first piece.
        let file = Bytes::from(vec![1u8; 4 * CHUNK]);
        let bodies: Vec<Body> = slots
            .into_iter()
            .map(|slot| streamed(file.clone(), slot, Duration::from_millis(50)))
            .collect();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(transfers.take("w").is_ok(), "stalled transfers ended");
        drop(bodies);
    }

    #[tokio::test]
    async fn a_download_that_is_read_arrives_whole() {
        let transfers = transfer_slots::Transfers::default();
        let file = Bytes::from((0..3 * CHUNK + 7).map(|i| i as u8).collect::<Vec<_>>());
        let body = streamed(file.clone(), transfers.take("w").unwrap(), STALLED);
        let read = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        assert_eq!(read, file);
        assert!(transfers.take("w").is_ok() && transfers.take("w").is_ok());
    }
}
