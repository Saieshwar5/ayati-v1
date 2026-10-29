//! Passive file delivery and image derivatives. Callers select and authorize the
//! source first; original bytes and source permissions remain separate.
use anyhow::{Context, Result, ensure};
use axum::{
    Json,
    http::{HeaderMap, header},
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
pub(crate) fn deliver(bytes: Vec<u8>, name: &str, mime: &str, input: &Read) -> Result<Response> {
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
        "image/png" | "image/jpeg" | "audio/wav" | "audio/webm"
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
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, mime.parse()?);
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("{disposition}; filename=\"download\"; filename*=UTF-8''{encoded}").parse()?,
    );
    headers.insert(header::CACHE_CONTROL, "private, no-store".parse()?);
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse()?);
    Ok((headers, bytes).into_response())
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
        let html = b"<script>alert(1)</script>".to_vec();
        let response = deliver(html, "<script>.html", mime(b"<script>"), &Read::default()).unwrap();
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
}
