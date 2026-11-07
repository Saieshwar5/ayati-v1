//! Passive file delivery and image derivatives. Callers select and authorize the
//! source first; original bytes and source permissions remain separate.
use anyhow::{Context, Result, ensure};
use axum::{
    Json,
    body::{Body, Bytes},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::Cursor,
    sync::{Arc, LazyLock},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
/// Largest stored file the client may read or preview.
pub(crate) const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
static PREVIEWS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(2)));
pub(crate) fn preview_permit() -> Result<OwnedSemaphorePermit> {
    PREVIEWS
        .clone()
        .try_acquire_owned()
        .context("Image previews are busy. Try again or download the original.")
}
pub(crate) fn mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}
pub(crate) async fn thumbnail(bytes: Vec<u8>, permit: OwnedSemaphorePermit) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        decode_thumbnail(&bytes)
    })
    .await
    .context("Image preview could not finish")?
}
fn decode_thumbnail(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() <= MAX_FILE_BYTES as usize,
        "Image exceeds preview byte limit"
    );
    let format = match mime(bytes) {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        _ => anyhow::bail!("Preview is unavailable for this format. Download the original."),
    };
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .context("Image preview could not be decoded")?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width > 0
            && height > 0
            && u64::from(width) * u64::from(height) <= 16_777_216
            && decoder.total_bytes() <= 64 * 1024 * 1024,
        "Image dimensions exceed the supported preview size. Download the original."
    );
    let orientation = decoder.orientation()?;
    let decoded = DynamicImage::from_decoder(decoder)?;
    let edge = 256.min(width.max(height));
    let mut small = decoded.thumbnail(edge, edge);
    // Apply camera orientation after shrinking to avoid a second full-size buffer.
    small.apply_orientation(orientation);
    let mut output = Cursor::new(Vec::new());
    small.write_to(&mut output, ImageFormat::Png)?;
    Ok(output.into_inner())
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    // Clients may still name the selected agent; reading never depends on it.
    #[serde(rename = "agent")]
    _agent: Option<String>,
    #[serde(default)]
    pub metadata: bool,
    #[serde(default)]
    pub access: bool,
    #[serde(default)]
    pub thumbnail: bool,
    #[serde(default)]
    pub inline: bool,
    #[serde(default)]
    pub preview: bool,
    pub version: Option<String>,
}
impl Read {
    pub(crate) fn version(&self, checksum: &str) -> Result<()> {
        ensure!(
            self.version.as_ref().is_none_or(|v| v == checksum),
            "Stored file version changed. Reopen the file."
        );
        Ok(())
    }
}
/// What a request's `Range` header selects from a complete file. Media
/// players (Safari and iOS especially) fetch audio in byte ranges to play and
/// seek. Only one `bytes` range is served; several ranges are refused.
#[derive(Debug, PartialEq, Eq)]
enum ByteRange {
    /// No range was requested, or the header was not readable text.
    Whole,
    /// One satisfiable range; `end` is inclusive, as in `Content-Range`.
    Part { start: usize, end: usize },
    /// A malformed, multiple or out-of-file range.
    Unsatisfiable,
}
impl ByteRange {
    fn requested(request: &HeaderMap, size: usize) -> Self {
        let Some(range) = request.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
            return Self::Whole;
        };
        let parsed = range
            .strip_prefix("bytes=")
            .and_then(|r| r.split_once('-'))
            .filter(|(first, last)| !first.contains(',') && !last.contains(','))
            .zip(size.checked_sub(1))
            .and_then(|((first, last), final_byte)| {
                if first.is_empty() {
                    // `bytes=-N` asks for the final N bytes.
                    let suffix = last.parse::<usize>().ok()?;
                    (suffix > 0).then(|| (size.saturating_sub(suffix), final_byte))
                } else {
                    let start = first.parse::<usize>().ok()?;
                    let end = if last.is_empty() {
                        final_byte
                    } else {
                        last.parse::<usize>().ok()?.min(final_byte)
                    };
                    Some((start, end))
                }
            });
        match parsed {
            Some((start, end)) if start <= end => Self::Part { start, end },
            _ => Self::Unsatisfiable,
        }
    }
}
/// Answers with the whole of `bytes` (200), the one range the request asks
/// for (206), or 416. `body` builds the response body from the selected bytes
/// so callers may stream them. Callers add the content headers.
pub(crate) fn ranged(
    request: &HeaderMap,
    bytes: Bytes,
    body: impl FnOnce(Bytes) -> Body,
) -> Result<Response> {
    let size = bytes.len();
    let (status, selected, content_range) = match ByteRange::requested(request, size) {
        ByteRange::Whole => (StatusCode::OK, bytes, None),
        ByteRange::Part { start, end } => (
            StatusCode::PARTIAL_CONTENT,
            bytes.slice(start..=end),
            Some(format!("bytes {start}-{end}/{size}")),
        ),
        ByteRange::Unsatisfiable => (
            StatusCode::RANGE_NOT_SATISFIABLE,
            Bytes::new(),
            Some(format!("bytes */{size}")),
        ),
    };
    let length = selected.len();
    let mut response = Response::new(body(selected));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_LENGTH, length.into());
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Some(content_range) = content_range {
        headers.insert(header::CONTENT_RANGE, content_range.parse()?);
    }
    Ok(response)
}
pub(crate) fn deliver(
    bytes: Bytes,
    name: &str,
    mime: &str,
    input: &Read,
    request: &HeaderMap,
) -> Result<Response> {
    if input.preview {
        let supported = name.rsplit('.').next().is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "txt"
                    | "md"
                    | "csv"
                    | "json"
                    | "log"
                    | "yaml"
                    | "yml"
                    | "toml"
                    | "xml"
                    | "html"
                    | "svg"
                    | "rs"
                    | "py"
                    | "js"
                    | "ts"
                    | "css"
            )
        });
        let end = bytes.len().min(64 * 1024);
        let text = if supported && !bytes[..end].contains(&0) {
            Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
        } else {
            None
        };
        return Ok(Json(json!({"text":text,"truncated":bytes.len()>end})).into_response());
    }
    let passive = matches!(
        mime,
        "image/png" | "image/jpeg" | "audio/wav" | "audio/webm" | "audio/ogg" | "audio/mp4"
    );
    let mime = if input.thumbnail {
        "image/png"
    } else if passive {
        mime
    } else {
        "application/octet-stream"
    };
    let encoded: String = name.bytes().map(|b| format!("%{b:02X}")).collect();
    let disposition = if input.thumbnail || input.inline && passive {
        "inline"
    } else {
        "attachment"
    };
    let mut response = ranged(request, bytes, Body::from)?;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, mime.parse()?);
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("{disposition}; filename=\"download\"; filename*=UTF-8''{encoded}").parse()?,
    );
    headers.insert(header::CACHE_CONTROL, "private, no-store".parse()?);
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse()?);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_png_and_jpeg_previews_are_bounded_and_camera_orientation_is_applied() {
        let mut png = Cursor::new(Vec::new());
        DynamicImage::new_rgba8(1200, 600)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let preview = decode_thumbnail(png.get_ref()).unwrap();
        let result = image::load_from_memory(&preview).unwrap();
        assert_eq!((result.width(), result.height()), (256, 128));
        let mut jpeg = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(80, 40)
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
        let mut oriented = jpeg.get_ref()[..2].to_vec();
        oriented.extend_from_slice(b"\xff\xe1");
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(exif);
        oriented.extend_from_slice(&jpeg.get_ref()[2..]);
        let preview = decode_thumbnail(&oriented).unwrap();
        let result = image::load_from_memory(&preview).unwrap();
        assert_eq!((result.width(), result.height()), (40, 80));
    }
    #[test]
    fn active_malformed_and_oversized_images_do_not_become_previews() {
        assert!(decode_thumbnail(b"<svg onload='alert(1)'></svg>").is_err());
        assert!(decode_thumbnail(b"\x89PNG\r\n\x1a\ntruncated").is_err());
        let mut png = Cursor::new(Vec::new());
        DynamicImage::new_luma8(9000, 1)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        assert!(decode_thumbnail(png.get_ref()).is_err());
    }

    #[test]
    fn active_content_downloads_as_an_opaque_attachment() {
        let html = Bytes::from_static(b"<script>alert(1)</script>");
        let response = deliver(
            html,
            "<script>.html",
            mime(b"<script>"),
            &Read::default(),
            &HeaderMap::new(),
        )
        .unwrap();
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/octet-stream"
        );
        let disposition = response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap();
        assert!(disposition.starts_with("attachment;"));
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
    }

    #[test]
    fn a_single_byte_range_is_selected_and_others_are_refused() {
        let requested = |range: &str, size| {
            let mut headers = HeaderMap::new();
            headers.insert(header::RANGE, range.parse().unwrap());
            ByteRange::requested(&headers, size)
        };
        let part = |start, end| ByteRange::Part { start, end };
        assert_eq!(
            ByteRange::requested(&HeaderMap::new(), 10),
            ByteRange::Whole
        );
        assert_eq!(requested("bytes=0-", 10), part(0, 9));
        assert_eq!(requested("bytes=2-4", 10), part(2, 4));
        assert_eq!(requested("bytes=5-100", 10), part(5, 9));
        assert_eq!(requested("bytes=-3", 10), part(7, 9));
        assert_eq!(requested("bytes=-30", 10), part(0, 9));
        for refused in [
            "bytes=-0",
            "bytes=10-",
            "bytes=4-2",
            "bytes=0-1,4-5",
            "items=0-1",
            "bytes=x-",
            "bytes=-",
        ] {
            assert_eq!(
                requested(refused, 10),
                ByteRange::Unsatisfiable,
                "{refused}"
            );
        }
        assert_eq!(requested("bytes=0-", 0), ByteRange::Unsatisfiable);
        assert_eq!(ByteRange::requested(&HeaderMap::new(), 0), ByteRange::Whole);
    }

    #[tokio::test]
    async fn passive_audio_plays_inline_and_seeks_in_byte_ranges() {
        let audio = Bytes::from_static(b"OggS voice note bytes");
        let inline = Read {
            inline: true,
            ..Read::default()
        };
        let send = |mime: &str, range: Option<&str>| {
            let mut request = HeaderMap::new();
            if let Some(range) = range {
                request.insert(header::RANGE, range.parse().unwrap());
            }
            deliver(audio.clone(), "voice-message.ogg", mime, &inline, &request).unwrap()
        };
        let whole = send("audio/ogg", None);
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.headers()[header::CONTENT_TYPE], "audio/ogg");
        assert_eq!(whole.headers()[header::ACCEPT_RANGES], "bytes");
        assert!(
            whole.headers()[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .starts_with("inline;")
        );
        let part = send("audio/ogg", Some("bytes=0-3"));
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.headers()[header::CONTENT_RANGE], "bytes 0-3/21");
        assert_eq!(part.headers()[header::CONTENT_LENGTH], "4");
        assert_eq!(part.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let body = axum::body::to_bytes(part.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"OggS");
        let refused = send("audio/ogg", Some("bytes=21-"));
        assert_eq!(refused.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(refused.headers()[header::CONTENT_RANGE], "bytes */21");
        assert_eq!(
            send("audio/mp4", None).headers()[header::CONTENT_TYPE],
            "audio/mp4"
        );
    }
}
