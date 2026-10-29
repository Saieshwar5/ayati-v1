use super::*;
use axum::{body::Bytes, extract::Query};
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Upload {
    name: String,
}
pub(super) async fn upload(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    Query(input): Query<Upload>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?.check_chat_accepting(&user, &w, &chat).await?;
    check(
        !bytes.is_empty() && bytes.len() <= 10 * 1024 * 1024,
        "Files must contain between 1 byte and 10 MiB",
    )?;
    check(
        !input.name.trim().is_empty()
            && input.name.len() <= 180
            && !input
                .name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\'),
        "Invalid file name",
    )?;
    // Only sniffed passive formats render inline. Other bytes are downloads.
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "audio/wav"
    } else if bytes.starts_with(b"\x1a\x45\xdf\xa3")
        && headers
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|v| v.starts_with("audio/webm"))
    {
        "audio/webm"
    } else {
        "application/octet-stream"
    };
    let object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        name: input.name,
        mime: mime.into(),
        size: bytes.len() as i64,
        checksum: checksum(&bytes),
    };
    if hosting::hosted(&app) {
        shared_storage(&app)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            app.library.put(&format!("chat/{w}/{}", object.id), bytes),
        )
        .await
        .context("File upload not confirmed; check stored records before retrying")??;
    } else {
        let root = app.runtimes.object_root(&w);
        tokio::fs::create_dir_all(&root).await?;
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).await?;
        let path = root.join(&object.id);
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        std::fs::File::open(&root)?.sync_all()?;
    }
    // Retain orphan bytes on uncertain DB failure; reconciliation must not destroy evidence.
    pg(&app)?.save_object(&user, &w, &chat, &object).await?;
    Ok(Json(json!(object)))
}
pub(super) async fn download(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<crate::media::Read>,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    check(uuid::Uuid::parse_str(&id).is_ok(), "Invalid file ID")?;
    let object = pg(&app)?.object(&user, &w, &id).await?;
    if input.access {
        return Ok(Json(json!({"available":true})).into_response());
    }
    input.version(&object.checksum)?;
    if input.metadata {
        return Ok(Json(json!(object)).into_response());
    }
    let permit = if input.thumbnail {
        Some(crate::media::preview_permit()?)
    } else {
        None
    };
    let bytes = if hosting::hosted(&app) {
        shared_storage(&app)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            app.library.get(&format!("chat/{w}/{id}")),
        )
        .await
        .context("File download timed out")??
    } else {
        tokio::fs::read(app.runtimes.object_root(&w).join(&id))
            .await
            .context("Stored object unavailable")?
            .into()
    };
    check(
        bytes.len() as i64 == object.size && checksum(&bytes) == object.checksum,
        "Stored object failed its integrity check",
    )?;
    let data = if let Some(permit) = permit {
        crate::media::thumbnail(bytes.to_vec(), permit).await?
    } else {
        bytes.to_vec()
    };
    // Object storage can take time. Recheck membership/publication before releasing bytes.
    pg(&app)?.object(&user, &w, &id).await?;
    Ok(crate::media::deliver(
        data,
        &object.name,
        &object.mime,
        &input,
    )?)
}
fn shared_storage(app: &App) -> ApiResult<()> {
    if matches!(app.library, library::Storage::S3(_)) {
        Ok(())
    } else {
        Err(Error(
            anyhow::anyhow!("Hosted chat attachments require private S3 storage."),
            StatusCode::SERVICE_UNAVAILABLE,
        ))
    }
}
fn checksum(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
