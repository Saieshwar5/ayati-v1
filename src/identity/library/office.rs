//! Text from Word (.docx) and Excel (.xlsx) uploads (Library LB2a), in pure Rust with
//! size limits; nothing in the file is executed. Headings become Markdown heading lines
//! ("# Returns") so passages can name their section; each Excel sheet is a section
//! with one line per row, labelled by the sheet's first row.
use anyhow::{Context, Result, ensure};
use std::io::{Cursor, Read};

pub const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const MAX_ENTRIES: usize = 4000;
const MAX_UNPACKED: u64 = 96 * 1024 * 1024;
const MAX_ROWS: usize = 2000;
pub const MAX_TEXT: usize = 64000;

/// A Word or Excel file, by the parts its zip declares.
pub fn kind(bytes: &[u8]) -> Option<&'static str> {
    let archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let has = |name: &str| archive.file_names().any(|n| n == name);
    if has("word/document.xml") {
        Some(DOCX)
    } else if has("xl/workbook.xml") {
        Some(XLSX)
    } else {
        None
    }
}

/// Refuse archives whose declared contents are too many or too large to unpack.
fn bounded(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("Not a readable file")?;
    ensure!(archive.len() <= MAX_ENTRIES, "Too many parts");
    let mut total = 0u64;
    for index in 0..archive.len() {
        total += archive.by_index_raw(index)?.size();
    }
    ensure!(total <= MAX_UNPACKED, "File unpacks too large");
    Ok(archive)
}

pub fn docx_text(bytes: &[u8]) -> Result<String> {
    use quick_xml::events::Event;
    let mut archive = bounded(bytes)?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")?
        .take(MAX_UNPACKED)
        .read_to_string(&mut xml)?;
    let mut reader = quick_xml::Reader::from_str(&xml);
    let (mut out, mut paragraph, mut style) = (String::new(), String::new(), None::<usize>);
    let (mut cells, mut cell, mut in_text) = (Vec::<String>::new(), String::new(), false);
    let mut depth = 0usize; // table cells being read
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"pStyle" => {
                let value = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.local_name().as_ref() == b"val")
                    .map(|a| String::from_utf8_lossy(&a.value).to_lowercase());
                style = value.and_then(|v| {
                    if v == "title" {
                        Some(1)
                    } else {
                        v.strip_prefix("heading")
                            .and_then(|n| n.parse::<usize>().ok())
                            .map(|n| n.clamp(1, 3))
                    }
                });
            }
            Event::Start(e) => match e.local_name().as_ref() {
                b"t" => in_text = true,
                b"tc" => depth += 1,
                _ => {}
            },
            Event::Empty(e) if matches!(e.local_name().as_ref(), b"tab" | b"br") => {
                paragraph.push(' ')
            }
            Event::Text(t) if in_text => paragraph.push_str(&t.decode()?),
            Event::GeneralRef(r) if in_text => {
                if let Some(c) = r.resolve_char_ref()? {
                    paragraph.push(c);
                } else if let Some(s) = quick_xml::escape::resolve_predefined_entity(&r.decode()?) {
                    paragraph.push_str(s);
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"p" => {
                    let text = paragraph.trim().to_owned();
                    paragraph.clear();
                    if depth > 0 {
                        if !text.is_empty() {
                            cell.push_str(if cell.is_empty() { "" } else { " " });
                            cell.push_str(&text);
                        }
                    } else if !text.is_empty() {
                        if let Some(level) = style {
                            out.push_str(&format!("\n{} ", "#".repeat(level)));
                        }
                        out.push_str(&text);
                        out.push('\n');
                    }
                    style = None;
                }
                b"tc" => {
                    depth = depth.saturating_sub(1);
                    cells.push(std::mem::take(&mut cell));
                }
                b"tr" => {
                    let row: Vec<String> = std::mem::take(&mut cells)
                        .into_iter()
                        .filter(|c| !c.is_empty())
                        .collect();
                    if !row.is_empty() {
                        out.push_str(&row.join(" | "));
                        out.push('\n');
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        if out.len() > MAX_TEXT * 4 {
            break;
        }
    }
    Ok(out)
}

/// An Excel date serial as a calendar date (and time when it has one).
fn excel_date(serial: f64) -> String {
    // Days since 1899-12-30, which absorbs Excel's 1900 leap-year quirk for real dates.
    let days = serial.floor() as i64 - 25569; // to the Unix epoch
    let (z, mut minutes) = (days + 719468, ((serial.fract() * 1440.0).round()) as i64);
    if minutes == 1440 {
        minutes = 0;
    }
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    if minutes == 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!(
            "{year:04}-{month:02}-{day:02} {:02}:{:02}",
            minutes / 60,
            minutes % 60
        )
    }
}

fn cell(value: &calamine::Data) -> String {
    match value {
        calamine::Data::DateTime(d) if d.is_datetime() => excel_date(d.as_f64()),
        calamine::Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        other => other.to_string().trim().to_owned(),
    }
}

pub fn xlsx_text(bytes: &[u8]) -> Result<String> {
    use calamine::Reader;
    bounded(bytes)?;
    let mut workbook: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(Cursor::new(bytes)).context("Not a readable workbook")?;
    let mut out = String::new();
    for name in workbook.sheet_names() {
        let Ok(range) = workbook.worksheet_range(&name) else {
            continue;
        };
        let mut rows = range
            .rows()
            .map(|row| row.iter().map(cell).collect::<Vec<_>>())
            .filter(|row| row.iter().any(|c| !c.is_empty()));
        let Some(header) = rows.next() else { continue };
        out.push_str(&format!("\n# {name}\n"));
        out.push_str(
            &header
                .iter()
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" | "),
        );
        out.push('\n');
        for row in rows.take(MAX_ROWS) {
            let line = row
                .iter()
                .enumerate()
                .filter(|(_, value)| !value.is_empty())
                .map(|(i, value)| match header.get(i).filter(|h| !h.is_empty()) {
                    Some(label) => format!("{label}: {value}"),
                    None => value.clone(),
                })
                .collect::<Vec<_>>()
                .join(" · ");
            out.push_str(&line);
            out.push('\n');
            if out.len() > MAX_TEXT * 4 {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    /// A minimal zip with the given parts, for Word and Excel fixtures.
    pub(crate) fn package(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buffer);
        for (name, body) in parts {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        buffer.into_inner()
    }
    pub(crate) fn docx(body: &str) -> Vec<u8> {
        package(&[
            ("[Content_Types].xml", "<Types/>"),
            (
                "word/document.xml",
                &format!(
                    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
                ),
            ),
        ])
    }

    #[test]
    fn word_headings_paragraphs_and_tables_become_text() {
        let file = docx(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Returns</w:t></w:r></w:p>
            <w:p><w:r><w:t xml:space="preserve">Refunds within </w:t></w:r><w:r><w:t>30 days &amp; free.</w:t></w:r></w:p>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Item</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Days</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        );
        assert_eq!(super::kind(&file), Some(super::DOCX));
        assert_eq!(
            super::docx_text(&file).unwrap(),
            "\n# Returns\nRefunds within 30 days & free.\nItem | Days\n"
        );
    }

    pub(crate) fn xlsx() -> Vec<u8> {
        let cell =
            |r: &str, v: &str| format!(r#"<c r="{r}" t="inlineStr"><is><t>{v}</t></is></c>"#);
        let sheet = format!(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1">{}{}{}</row><row r="2">{}<c r="B2"><v>1250</v></c><c r="C2" s="1"><v>45966</v></c></row></sheetData></worksheet>"#,
            cell("A1", "Product"),
            cell("B1", "Price"),
            cell("C1", "Updated"),
            cell("A2", "Desk")
        );
        package(&[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Price list" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/styles.xml",
                r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><cellXfs count="2"><xf numFmtId="0"/><xf numFmtId="14" applyNumberFormat="1"/></cellXfs></styleSheet>"#,
            ),
            ("xl/worksheets/sheet1.xml", &sheet),
        ])
    }

    #[test]
    fn excel_sheets_are_sections_with_rows_labelled_by_the_first_row() {
        let file = xlsx();
        assert_eq!(super::kind(&file), Some(super::XLSX));
        assert_eq!(
            super::xlsx_text(&file).unwrap(),
            "\n# Price list\nProduct | Price | Updated\nProduct: Desk · Price: 1250 · Updated: 2025-11-05\n"
        );
    }

    #[test]
    fn excel_serial_dates_become_calendar_dates() {
        assert_eq!(super::excel_date(45966.0), "2025-11-05");
        assert_eq!(super::excel_date(45966.5), "2025-11-05 12:00");
        assert_eq!(super::excel_date(1.0), "1899-12-31");
    }

    #[test]
    fn unrelated_zips_and_bad_bytes_are_not_office_files() {
        assert_eq!(super::kind(&package(&[("a.txt", "x")])), None);
        assert_eq!(super::kind(b"PK not really"), None);
        assert!(super::docx_text(b"nope").is_err());
        assert!(super::xlsx_text(&package(&[("xl/workbook.xml", "<bad")])).is_err());
    }
}
