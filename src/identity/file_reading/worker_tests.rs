//! What the local readers make of a file's bytes, without storage or a database.
use super::outcome;
use crate::identity::postgres::objects::contents::Outcome;
use axum::body::Bytes;

fn read(outcome: Outcome) -> Option<(&'static str, String, bool)> {
    match outcome {
        Outcome::Read {
            kind,
            text,
            truncated,
        } => Some((kind, text, truncated)),
        _ => None,
    }
}
fn unreadable(outcome: Outcome) -> Option<Option<&'static str>> {
    match outcome {
        Outcome::Unreadable { kind } => Some(kind),
        _ => None,
    }
}
fn word(text: &str) -> Bytes {
    use std::io::Write;
    let mut buffer = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buffer);
    zip.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    let body = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
    );
    zip.write_all(body.as_bytes()).unwrap();
    zip.finish().unwrap();
    Bytes::from(buffer.into_inner())
}

#[tokio::test]
async fn text_and_word_files_are_read_whole() {
    let csv = Bytes::from_static(b"invoice,amount\nAcme 42,48000\n");
    let (kind, text, truncated) = read(outcome(&csv).await).expect("text is read");
    assert_eq!((kind, truncated), ("text", false));
    assert!(text.contains("Acme 42,48000"));

    let (kind, text, truncated) =
        read(outcome(&word("Returns within 30 days.")).await).expect("Word is read");
    assert_eq!((kind, truncated), ("word", false));
    assert!(text.contains("Returns within 30 days."), "{text}");
}

#[tokio::test]
async fn long_text_says_it_was_cut_short() {
    let long = Bytes::from("a line of the report\n".repeat(4000));
    let (kind, text, truncated) = read(outcome(&long).await).expect("text is read");
    assert_eq!((kind, truncated), ("text", true));
    assert_eq!(text.chars().count(), 64_000);
}

#[tokio::test]
async fn files_no_reader_handles_are_unsupported_not_retried() {
    for bytes in [
        Bytes::from_static(b"\0\x01\x02\x03binary"),
        Bytes::from_static(b"\x7fELF\x02\x01\x01\0"),
        Bytes::from_static(b"   \n\t  \n"),
    ] {
        assert!(
            unreadable(outcome(&bytes).await).is_some(),
            "{bytes:?} is not readable"
        );
    }
    // An image without readable text (or without a recogniser here) stays an
    // image the agent is told it can't read.
    let broken = Bytes::from_static(b"\x89PNG\r\n\x1a\nbroken");
    assert_eq!(unreadable(outcome(&broken).await), Some(Some("image")));
}

#[tokio::test]
#[ignore = "requires bwrap and pdftotext"]
async fn pdfs_are_read_in_the_sandbox() {
    let pdf = Bytes::from(std::fs::read("tests/fixtures/library/travel.pdf").unwrap());
    let (kind, text, truncated) = read(outcome(&pdf).await).expect("PDF is read");
    assert_eq!((kind, truncated), ("pdf", false));
    assert!(text.contains("Hotels are capped at 150 a night."), "{text}");
}
