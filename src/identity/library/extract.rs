use axum::body::Bytes;
use std::process::Stdio;

pub fn mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"%PDF-") {
        "application/pdf"
    } else if std::str::from_utf8(bytes).is_ok_and(|s| {
        !s.chars()
            .any(|c| c.is_control() && !['\n', '\r', '\t'].contains(&c))
    }) {
        "text/plain"
    } else {
        "application/octet-stream"
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
    if mime != "application/pdf" {
        return (String::new(), "unsupported");
    }
    // Never execute a shell or uploaded content. The optional local parser has
    // fixed CPU, address-space, output-size, page and wall-clock limits.
    let root = std::env::temp_dir().join(format!("cuckoo-library-{}", uuid::Uuid::new_v4()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let result = async {
        tokio::fs::create_dir(&root).await?;
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).await?;
        let input = root.join("source.pdf");
        let output = root.join("text.txt");
        tokio::fs::write(&input, bytes).await?;
        let status = tokio::time::timeout(
            std::time::Duration::from_secs(7),
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
                    "usr/lib",
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
                    "--ro-bind",
                ])
                .arg(&input)
                .arg("/source.pdf")
                .arg("--bind")
                .arg(&root)
                .arg("/output")
                .args([
                    "--",
                    "/usr/bin/prlimit",
                    "--as=268435456",
                    "--cpu=5",
                    "--fsize=262144",
                    "--",
                    "/usr/bin/pdftotext",
                    "-f",
                    "1",
                    "-l",
                    "50",
                    "-enc",
                    "UTF-8",
                    "/source.pdf",
                    "/output/text.txt",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await??;
        anyhow::ensure!(status.success(), "PDF text unavailable");
        let text = tokio::fs::read_to_string(&output).await?;
        anyhow::ensure!(!text.trim().is_empty(), "No readable text");
        Ok::<_, anyhow::Error>(text.chars().take(64000).collect::<String>())
    }
    .await;
    match result {
        Ok(text) => (text, "partial"),
        Err(_) => (String::new(), "unsupported"),
    }
}
