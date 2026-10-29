//! Profile pictures for people and agents accept only common image formats,
//! recognised from the file's own bytes rather than its name or declared type.
use axum::{
    http::header,
    response::{IntoResponse, Response},
};

pub(crate) fn picture_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

/// Serves stored picture bytes; the browser must not guess another type.
pub(crate) fn picture_response(mime: &str, bytes: Vec<u8>) -> anyhow::Result<Response> {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, mime.parse()?);
    headers.insert("x-content-type-options", "nosniff".parse()?);
    headers.insert(
        header::CACHE_CONTROL,
        "private, max-age=86400, immutable".parse()?,
    );
    Ok(response)
}
