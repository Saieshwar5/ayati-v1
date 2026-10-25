//! Passive image derivatives. Original bytes and source permissions remain separate.
use anyhow::{Context, Result, ensure};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{
    io::Cursor,
    sync::{Arc, LazyLock},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
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
        bytes.len() <= crate::files::MAX_FILE_BYTES as usize,
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
}
