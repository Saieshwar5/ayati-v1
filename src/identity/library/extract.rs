use axum::body::Bytes;
use std::process::Stdio;

pub fn mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"%PDF-") {
        "application/pdf"
    } else if bytes.starts_with(b"PK\x03\x04") {
        super::office::kind(bytes).unwrap_or("application/octet-stream")
    } else if std::str::from_utf8(bytes).is_ok_and(|s| {
        !s.chars()
            .any(|c| c.is_control() && !['\n', '\r', '\t'].contains(&c))
    }) {
        "text/plain"
    } else {
        "application/octet-stream"
    }
}
/// The kind of readable file a detected type is, as agents are told it, or None
/// when no reader handles it.
pub fn kind(mime: &str) -> Option<&'static str> {
    match mime {
        "text/plain" => Some("text"),
        "application/pdf" => Some("pdf"),
        "image/png" | "image/jpeg" => Some("image"),
        super::office::DOCX => Some("word"),
        super::office::XLSX => Some("excel"),
        _ => None,
    }
}
pub async fn text(bytes: &Bytes, mime: &str) -> (String, &'static str) {
    if mime == "text/plain" {
        return (
            std::str::from_utf8(bytes)
                .unwrap_or("")
                .chars()
                .take(64000)
                .collect(),
            if bytes.len() > 64000 {
                "partial"
            } else {
                "ready"
            },
        );
    }
    if mime == super::office::DOCX || mime == super::office::XLSX {
        return office(bytes.clone(), mime).await;
    }
    if mime == "image/png" || mime == "image/jpeg" {
        return image_text(bytes).await;
    }
    if mime != "application/pdf" {
        return (String::new(), "unsupported");
    }
    let pdf = [
        "/usr/bin/pdftotext",
        "-f",
        "1",
        "-l",
        "50",
        "-enc",
        "UTF-8",
        "/source",
        "/output/text.txt",
    ];
    match sandboxed(
        bytes,
        &pdf,
        Limits {
            memory: 268435456,
            cpu: 5,
            seconds: 7,
        },
    )
    .await
    {
        Ok(text) => (text, "partial"),
        Err(_) => (String::new(), "unsupported"),
    }
}

struct Limits {
    memory: u64,
    cpu: u64,
    seconds: u64,
}

/// Runs a fixed local reader over the uploaded bytes (mounted at /source) without
/// network, home directory or a shell, with memory, CPU, output-size and wall-clock
/// limits, and returns what it wrote to /output/text.txt. Never runs uploaded content.
async fn sandboxed(bytes: &Bytes, program: &[&str], limits: Limits) -> anyhow::Result<String> {
    let root = std::env::temp_dir().join(format!("cuckoo-library-{}", uuid::Uuid::new_v4()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    tokio::fs::create_dir(&root).await?;
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).await?;
    let input = root.join("source");
    let output = root.join("text.txt");
    tokio::fs::write(&input, bytes).await?;
    // The program loader's path: /lib64 is usr/lib64 on Debian and Ubuntu, usr/lib on
    // distributions that merged them.
    let lib64 = if std::path::Path::new("/usr/lib64").is_dir() {
        "usr/lib64"
    } else {
        "usr/lib"
    };
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(limits.seconds),
        tokio::process::Command::new("bwrap")
            .args([
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--ro-bind",
                "/usr",
                "/usr",
                "--symlink",
                "usr/lib",
                "/lib",
                "--symlink",
                lib64,
                "/lib64",
                "--symlink",
                "usr/bin",
                "/bin",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--clearenv",
                "--setenv",
                "PATH",
                "/usr/bin",
                "--setenv",
                "OMP_THREAD_LIMIT",
                "1",
                "--ro-bind",
            ])
            .arg(&input)
            .arg("/source")
            .arg("--bind")
            .arg(&root)
            .arg("/output")
            .arg("--")
            .arg("/usr/bin/prlimit")
            .arg(format!("--as={}", limits.memory))
            .arg(format!("--cpu={}", limits.cpu))
            .arg("--fsize=262144")
            .arg("--")
            .args(program)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await??;
    anyhow::ensure!(status.success(), "Text unavailable");
    let text = tokio::fs::read_to_string(&output).await?;
    anyhow::ensure!(!text.trim().is_empty(), "No readable text");
    Ok(text.chars().take(64000).collect::<String>())
}

/// Images are read by text recognition (Tesseract, English), up to 40 megapixels.
/// The text becomes one "Text in the image" section.
async fn image_text(bytes: &Bytes) -> (String, &'static str) {
    let small_enough = image::ImageReader::new(std::io::Cursor::new(bytes.as_ref()))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
        .is_some_and(|(w, h)| u64::from(w) * u64::from(h) <= 40_000_000);
    if !small_enough {
        return (String::new(), "unsupported");
    }
    let ocr = [
        "/usr/bin/tesseract",
        "/source",
        "/output/text",
        "-l",
        "eng",
        "--psm",
        "3",
    ];
    match sandboxed(
        bytes,
        &ocr,
        Limits {
            memory: 1073741824,
            cpu: 20,
            seconds: 25,
        },
    )
    .await
    {
        Ok(text) => (format!("# Text in the image\n{}", text.trim()), "partial"),
        Err(_) => (String::new(), "unsupported"),
    }
}

/// Word and Excel text, off the async threads and within a time limit.
async fn office(bytes: Bytes, mime: &str) -> (String, &'static str) {
    let word = mime == super::office::DOCX;
    let work = tokio::task::spawn_blocking(move || {
        if word {
            super::office::docx_text(&bytes)
        } else {
            super::office::xlsx_text(&bytes)
        }
    });
    match tokio::time::timeout(std::time::Duration::from_secs(7), work).await {
        Ok(Ok(Ok(text))) if !text.trim().is_empty() => {
            let limit = super::office::MAX_TEXT;
            let partial = text.chars().count() > limit;
            (
                text.chars().take(limit).collect(),
                if partial { "partial" } else { "ready" },
            )
        }
        _ => (String::new(), "unsupported"),
    }
}

#[cfg(test)]
mod tests {
    /// Needs bwrap, prlimit and tesseract (tesseract-ocr, tesseract-ocr-eng).
    #[tokio::test]
    #[ignore = "requires bwrap and tesseract"]
    async fn text_in_images_and_pdfs_is_read_in_the_sandbox() {
        let bytes =
            axum::body::Bytes::from(std::fs::read("tests/fixtures/library/receipt.png").unwrap());
        assert_eq!(super::mime(&bytes), "image/png");
        let (text, readiness) = super::text(&bytes, "image/png").await;
        assert_eq!(readiness, "partial");
        assert!(text.starts_with("# Text in the image\n"));
        assert!(text.contains("Standing desk"), "{text}");
        let pdf =
            axum::body::Bytes::from(std::fs::read("tests/fixtures/library/travel.pdf").unwrap());
        assert_eq!(super::mime(&pdf), "application/pdf");
        let (text, _) = super::text(&pdf, "application/pdf").await;
        assert!(
            text.contains("Hotels are capped at 150 a night."),
            "PDF text in the same sandbox: {text}"
        );
        let (none, unsupported) = super::text(
            &axum::body::Bytes::from_static(b"\x89PNG\r\n\x1a\nbroken"),
            "image/png",
        )
        .await;
        assert_eq!((none.as_str(), unsupported), ("", "unsupported"));
    }
}
