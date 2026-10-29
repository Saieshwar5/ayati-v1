use super::*;
use axum::http::HeaderMap;
static TRANSFERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
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
    let _slot = TRANSFERS.try_acquire().map_err(|_| {
        Error(
            anyhow::anyhow!("Four file transfers are active. Retry shortly."),
            StatusCode::TOO_MANY_REQUESTS,
        )
    })?;
    let user = identity(&app, request.headers()).await?;
    let session = app.store.library_upload_session(&user, &w, &id).await?;
    let bytes = axum::body::to_bytes(request.into_body(), MAX_FILE)
        .await
        .context("File exceeds the 25 MiB limit or transfer was interrupted")?;
    check(
        bytes.len() as i64 == session["size"].as_i64().unwrap_or(0)
            && checksum(&bytes) == session["checksum"],
        "Selected file does not match this upload; reselect the original file",
    )?;
    if session["state"] == "stored" {
        return Ok(Json(app.store.library_get(&user, &w, &id).await?));
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
}
pub(super) async fn download(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<Download>,
) -> ApiResult<Response> {
    let slot = TRANSFERS.try_acquire().map_err(|_| {
        Error(
            anyhow::anyhow!("File transfers busy. Retry shortly."),
            StatusCode::TOO_MANY_REQUESTS,
        )
    })?;
    let user = identity(&app, &headers).await?;
    let item = app.store.library_file(&user, &w, &id).await?;
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
    let size = bytes.len();
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let (start, end) = if let Some(range) = range {
        let selected = range
            .strip_prefix("bytes=")
            .and_then(|r| r.split_once('-'))
            .filter(|(a, b)| !a.contains(',') && !b.contains(','));
        let parsed = selected.and_then(|(a, b)| {
            if a.is_empty() {
                let last = b.parse::<usize>().ok()?;
                (last > 0).then_some((size.saturating_sub(last), size - 1))
            } else {
                Some((
                    a.parse::<usize>().ok()?,
                    if b.is_empty() {
                        size - 1
                    } else {
                        b.parse::<usize>().ok()?.min(size - 1)
                    },
                ))
            }
        });
        let Some((start, end)) = parsed.filter(|(a, b)| a <= b && *a < size) else {
            return Ok((
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{size}"))],
            )
                .into_response());
        };
        (start, end)
    } else {
        (0, size - 1)
    };
    // Keep the transfer slot with a bounded-chunk body so a slow reader cannot
    // release capacity while its entire file is still queued for transmission.
    let selected = bytes.slice(start..=end);
    let length = selected.len();
    let stream =
        futures_util::stream::unfold((selected, 0, slot), |(bytes, offset, slot)| async move {
            if offset >= bytes.len() {
                return None;
            }
            let end = (offset + 64 * 1024).min(bytes.len());
            Some((
                Ok::<_, std::io::Error>(bytes.slice(offset..end)),
                (bytes, end, slot),
            ))
        });
    let mut response = Response::new(Body::from_stream(stream));
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, length.to_string().parse()?);
    if range.is_some() {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{size}").parse()?,
        );
    }
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
    h.insert(header::ACCEPT_RANGES, "bytes".parse()?);
    h.insert("x-content-type-options", "nosniff".parse()?);
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        "sandbox; default-src 'none'".parse()?,
    );
    Ok(response)
}
