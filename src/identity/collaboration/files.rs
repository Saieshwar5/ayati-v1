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
    app.store.check_chat_accepting(&user, &w, &chat).await?;
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
    let declared = headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok());
    let mime = sniff(&bytes, declared);
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
    let transcribe = app.config.transcription_model.is_some();
    app.store
        .save_object(&user, &w, &chat, &object, transcribe)
        .await?;
    if transcribe && object.mime.starts_with("audio/") {
        crate::identity::transcription::wake();
    }
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
    let object = app.store.object(&user, &w, &id).await?;
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
    let bytes = stored(&app, &w, &object).await?;
    let data = if let Some(permit) = permit {
        crate::media::thumbnail(bytes.to_vec(), permit)
            .await?
            .into()
    } else {
        bytes
    };
    // Object storage can take time. Recheck membership/publication before releasing bytes.
    app.store.object(&user, &w, &id).await?;
    Ok(crate::media::deliver(
        data,
        &object.name,
        &object.mime,
        &input,
        &headers,
    )?)
}
/// A chat file's bytes from private S3 (hosted) or the local object root,
/// checked against the size and checksum saved at upload.
pub(crate) async fn stored(app: &App, w: &str, object: &Object) -> ApiResult<Bytes> {
    let bytes = if hosting::hosted(app) {
        shared_storage(app)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            app.library.get(&format!("chat/{w}/{}", object.id)),
        )
        .await
        .context("File download timed out")??
    } else {
        tokio::fs::read(app.runtimes.object_root(w).join(&object.id))
            .await
            .context("Stored object unavailable")?
            .into()
    };
    check(
        bytes.len() as i64 == object.size && checksum(&bytes) == object.checksum,
        "Stored object failed its integrity check",
    )?;
    Ok(bytes)
}
/// Only sniffed passive formats render inline; other bytes are downloads.
/// WebM and MP4 can also hold video, so they count as audio only when the
/// uploader declared audio. Ogg also counts when its stream is Opus or Vorbis.
fn sniff(bytes: &[u8], declared: Option<&str>) -> &'static str {
    // Compare the declared type without parameters such as `codecs=opus`.
    let declared = declared
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "audio/wav"
    } else if bytes.starts_with(b"\x1a\x45\xdf\xa3") && declared == "audio/webm" {
        "audio/webm"
    } else if bytes.starts_with(b"OggS") && (declared == "audio/ogg" || ogg_audio(bytes)) {
        "audio/ogg"
    } else if mp4_audio(bytes) && matches!(declared.as_str(), "audio/mp4" | "audio/x-m4a") {
        "audio/mp4"
    } else {
        "application/octet-stream"
    }
}
/// An Ogg stream's first page holds just its identification header, which
/// follows the 27-byte page header and its segment table.
fn ogg_audio(bytes: &[u8]) -> bool {
    let Some(&segments) = bytes.get(26) else {
        return false;
    };
    let packet = bytes.get(27 + usize::from(segments)..).unwrap_or_default();
    packet.starts_with(b"OpusHead") || packet.starts_with(b"\x01vorbis")
}
/// MP4 (ISO BMFF) opens with an `ftyp` box: size, `ftyp`, major brand, minor
/// version, then compatible brands. Safari and M4A files use these brands.
fn mp4_audio(bytes: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 8] = [
        b"M4A ", b"M4B ", b"isom", b"iso2", b"iso5", b"iso6", b"mp41", b"mp42",
    ];
    if bytes.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let fields = bytes.get(8..size.min(bytes.len())).unwrap_or_default();
    let [major, _minor, compatible @ ..] = fields.as_chunks::<4>().0 else {
        return false;
    };
    std::iter::once(major)
        .chain(compatible)
        .any(|brand| BRANDS.contains(&brand))
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

#[cfg(test)]
mod tests {
    use super::*;
    /// A first Ogg page: 26 header bytes, one segment, then the packet.
    fn ogg(packet: &[u8]) -> Vec<u8> {
        let mut page = b"OggS\0\x02".to_vec();
        page.extend_from_slice(&[0; 20]);
        page.extend_from_slice(&[1, packet.len() as u8]);
        page.extend_from_slice(packet);
        page
    }
    fn mp4(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let mut file = (16 + 4 * compatible.len() as u32).to_be_bytes().to_vec();
        file.extend_from_slice(b"ftyp");
        file.extend_from_slice(major);
        file.extend_from_slice(&[0; 4]);
        for brand in compatible {
            file.extend_from_slice(*brand);
        }
        file.extend_from_slice(b"\0\0\0\x08free");
        file
    }
    #[test]
    fn browser_voice_recordings_are_stored_as_playable_audio() {
        let opus = ogg(b"OpusHead\x01\x01\x38\x01\x80\xbb\0\0\0\0\0");
        let vorbis = ogg(b"\x01vorbis\0\0\0\0\x01");
        let theora = ogg(b"\x80theora\x03\x02\x01");
        let webm = b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01";
        let safari = mp4(b"iso5", &[b"isom", b"iso5", b"hlsf"]);
        let m4a = mp4(b"M4A ", &[b"M4A ", b"mp42", b"isom"]);
        let quicktime = mp4(b"qt  ", &[b"qt  "]);
        let cases: [(&[u8], Option<&str>, &str); 18] = [
            // Firefox records Ogg/Opus; the codec header alone is enough.
            (&opus, Some("audio/ogg; codecs=opus"), "audio/ogg"),
            (&opus, Some("application/octet-stream"), "audio/ogg"),
            (&vorbis, None, "audio/ogg"),
            (&theora, Some("Audio/Ogg"), "audio/ogg"),
            (&theora, Some("video/ogg"), "application/octet-stream"),
            (b"OggS", Some("video/ogg"), "application/octet-stream"),
            // Safari records MP4/AAC; MP4 needs a declared audio type.
            (&safari, Some("audio/mp4"), "audio/mp4"),
            (&m4a, Some("audio/x-m4a"), "audio/mp4"),
            (&safari, Some("video/mp4"), "application/octet-stream"),
            (&safari, None, "application/octet-stream"),
            (&quicktime, Some("audio/mp4"), "application/octet-stream"),
            (&safari[..12], Some("audio/mp4"), "application/octet-stream"),
            (
                b"<html><script>",
                Some("audio/mp4"),
                "application/octet-stream",
            ),
            // Chrome records WebM/Opus; WebM also needs a declared audio type.
            (webm, Some("audio/webm;codecs=opus"), "audio/webm"),
            (webm, Some("video/webm"), "application/octet-stream"),
            (
                b"<html><script>",
                Some("audio/ogg"),
                "application/octet-stream",
            ),
            (b"RIFF\0\0\0\0WAVEfmt ", None, "audio/wav"),
            (b"\x89PNG\r\n\x1a\n", Some("text/html"), "image/png"),
        ];
        for (index, (bytes, declared, expected)) in cases.into_iter().enumerate() {
            assert_eq!(sniff(bytes, declared), expected, "case {index}");
        }
    }
}
