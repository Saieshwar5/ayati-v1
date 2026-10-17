use ayati::{files::Files, storage::Store};
use std::sync::Arc;

fn files(root: &std::path::Path) -> Files {
    std::fs::create_dir_all(root).unwrap();
    Files::new(root, Arc::new(Store::open(&root.join("state.db")).unwrap())).unwrap()
}

#[tokio::test]
async fn attachments_are_scoped_bounded_and_stable() {
    let dir = tempfile::tempdir().unwrap();
    let a = files(&dir.path().join("a"));
    let b = files(&dir.path().join("b"));
    let (artifact, _) = a
        .upload("stock.txt", "stock: 42 🍎".repeat(9000).as_bytes())
        .await
        .unwrap();
    let ids = vec![artifact.id.clone()];
    let first = a.attach_to_message("Review", &ids).await.unwrap();
    assert!(first.contains("stock: 42"));
    assert!(first.contains("\"truncated\":true"));
    assert!(first.len() < 16000);
    assert_eq!(first, a.attach_to_message("Review", &ids).await.unwrap());
    assert!(b.preview(&artifact.id).await.is_err());
    assert!(b.attach_to_message("Review", &ids).await.is_err());
    assert!(
        a.attach_to_message("Review", &vec![artifact.id; 9])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn binary_and_symlink_files_are_not_read_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let a = files(dir.path());
    let (pdf, _) = a
        .upload("report.pdf", b"private binary content")
        .await
        .unwrap();
    assert!(a.preview(&pdf.id).await.unwrap().text.is_none());
    let (bad, _) = a.upload("invalid.txt", &[0xff, 0]).await.unwrap();
    assert!(a.preview(&bad.id).await.unwrap().text.is_none());
    let (text, _) = a.upload("file.txt", b"safe").await.unwrap();
    let path = a.archives.join(text.stored_name);
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", path).unwrap();
    assert!(a.preview(&text.id).await.is_err());
}
