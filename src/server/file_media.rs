//! Passive file viewing; every caller first selects and authorizes its own source.
use super::*;
use anyhow::{Context, ensure};
use axum::extract::Query;
use tokio::io::AsyncReadExt;
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    // Selection belongs to the existing local middleware, never the file reader.
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
    pub(crate) fn version(&self, checksum: &str) -> anyhow::Result<()> {
        ensure!(
            self.version.as_ref().is_none_or(|v| v == checksum),
            "Stored file version changed. Reopen the file."
        );
        Ok(())
    }
}
pub(super) async fn read(
    axum::Extension(state): axum::Extension<WebState>,
    Path(id): Path<String>,
    Query(input): Query<Read>,
) -> ApiResult<Response> {
    let artifact = state
        .agent
        .store
        .artifact(&id)?
        .context("File does not exist")?;
    if input.access {
        return Ok(Json(json!({"available":true})).into_response());
    }
    let permit = if input.thumbnail {
        Some(crate::media::preview_permit()?)
    } else {
        None
    };
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(state.agent.files.archives.join(&artifact.stored_name))
        .await?;
    if !file.metadata().await?.is_file() {
        return Err(anyhow::anyhow!("Stored file must be a regular file").into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_FILE_BYTES || bytes.len() as u64 != artifact.size {
        return Err(anyhow::anyhow!("Stored file size changed").into());
    }
    let checksum = crate::response::checksum(&bytes);
    input.version(&checksum)?;
    let mime = crate::media::mime(&bytes);
    if input.metadata {
        return Ok(Json(json!({"id":id,"name":artifact.name,"size":artifact.size,"mime":mime,"checksum":checksum})).into_response());
    }
    let bytes = if let Some(permit) = permit {
        crate::media::thumbnail(bytes, permit).await?
    } else {
        bytes
    };
    Ok(deliver(bytes, &artifact.name, mime, &input)?)
}
pub(crate) fn deliver(
    bytes: Vec<u8>,
    name: &str,
    mime: &str,
    input: &Read,
) -> anyhow::Result<Response> {
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
