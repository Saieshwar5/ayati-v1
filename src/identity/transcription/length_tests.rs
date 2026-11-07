use super::duration_ms;

fn wav(seconds: u32, unfinished: bool) -> Vec<u8> {
    let rate = 16_000u32 * 2;
    let data = vec![0u8; (rate * seconds) as usize];
    let mut file = b"RIFF\0\0\0\0WAVE".to_vec();
    file.extend_from_slice(b"LIST\x04\0\0\0info");
    file.extend_from_slice(b"fmt \x10\0\0\0\x01\0\x01\0");
    file.extend_from_slice(&16_000u32.to_le_bytes());
    file.extend_from_slice(&rate.to_le_bytes());
    file.extend_from_slice(b"\x02\0\x10\0data");
    let size = if unfinished {
        u32::MAX
    } else {
        data.len() as u32
    };
    file.extend_from_slice(&size.to_le_bytes());
    file.extend_from_slice(&data);
    file
}

fn ogg_page(granule: u64, packet: &[u8]) -> Vec<u8> {
    let mut page = b"OggS\0\x02".to_vec();
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&[0; 12]);
    page.extend_from_slice(&[1, packet.len() as u8]);
    page.extend_from_slice(packet);
    page
}

/// An EBML element with a one-byte size, or an unknown size.
fn element(id: &[u8], contents: &[u8], unknown: bool) -> Vec<u8> {
    let mut bytes = id.to_vec();
    if unknown {
        bytes.extend_from_slice(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    } else {
        bytes.push(0x80 | contents.len() as u8);
    }
    bytes.extend_from_slice(contents);
    bytes
}

/// What Chrome records: no Duration, a segment and clusters of unknown size.
fn webm(declared: Option<f64>, blocks: &[(u16, i16)]) -> Vec<u8> {
    let mut info = element(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40], false);
    if let Some(ticks) = declared {
        info.extend(element(&[0x44, 0x89], &ticks.to_be_bytes(), false));
    }
    let mut segment = element(&[0x15, 0x49, 0xA9, 0x66], &info, false);
    segment.extend(element(&[0x16, 0x54, 0xAE, 0x6B], b"tracks", false));
    let mut cluster = vec![];
    let mut base = None;
    for (cluster_time, offset) in blocks {
        if base != Some(*cluster_time) {
            if base.is_some() {
                segment.extend(element(&[0x1F, 0x43, 0xB6, 0x75], &cluster, true));
                cluster.clear();
            }
            base = Some(*cluster_time);
            cluster.extend(element(&[0xE7], &cluster_time.to_be_bytes(), false));
        }
        let mut block = vec![0x81];
        block.extend_from_slice(&offset.to_be_bytes());
        block.extend_from_slice(&[0x80, 1, 2, 3]);
        cluster.extend(element(&[0xA3], &block, false));
    }
    segment.extend(element(&[0x1F, 0x43, 0xB6, 0x75], &cluster, true));
    let mut file = element(&[0x1A, 0x45, 0xDF, 0xA3], b"\x42\x82\x84webm", false);
    file.extend(element(&[0x18, 0x53, 0x80, 0x67], &segment, true));
    file
}

fn mp4_box(kind: &[u8; 4], contents: &[u8]) -> Vec<u8> {
    let mut bytes = ((8 + contents.len()) as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(contents);
    bytes
}
fn mp4(timescale: u32, duration: u32) -> Vec<u8> {
    let mut header = vec![0u8; 12];
    header.extend_from_slice(&timescale.to_be_bytes());
    header.extend_from_slice(&duration.to_be_bytes());
    header.extend_from_slice(&[0; 80]);
    let mut file = mp4_box(b"ftyp", b"M4A \0\0\0\0isom");
    file.extend(mp4_box(b"mdat", &[0; 64]));
    file.extend(mp4_box(b"moov", &mp4_box(b"mvhd", &header)));
    file
}

#[test]
fn recordings_say_how_long_they_are() {
    assert_eq!(duration_ms("audio/wav", &wav(3, false)), Some(3000));
    // A WAV whose size was never written counts only the bytes present.
    assert_eq!(duration_ms("audio/wav", &wav(2, true)), Some(2000));

    let opus = b"OpusHead\x01\x01\x38\x01\x80\xbb\0\0\0\0\0";
    let mut ogg = ogg_page(0, opus);
    ogg.extend(ogg_page(0, b"OpusTags"));
    ogg.extend(ogg_page(48_000 * 4 + 312, &[0; 40]));
    // A page where no packet ends carries no position.
    ogg.extend(ogg_page(u64::MAX, &[0; 40]));
    assert_eq!(duration_ms("audio/ogg", &ogg), Some(4000));
    let mut vorbis = b"\x01vorbis\0\0\0\0\x01".to_vec();
    vorbis.extend_from_slice(&44_100u32.to_le_bytes());
    let mut ogg = ogg_page(0, &vorbis);
    ogg.extend(ogg_page(44_100 * 7, &[0; 40]));
    assert_eq!(duration_ms("audio/ogg", &ogg), Some(7000));

    // Chrome: the last block's time, over two clusters of unknown size.
    let chrome = webm(None, &[(0, 0), (0, 20), (5000, 0), (5000, 4980)]);
    assert_eq!(duration_ms("audio/webm", &chrome), Some(9980));
    let declared = webm(Some(301_500.0), &[(0, 0)]);
    assert_eq!(duration_ms("audio/webm", &declared), Some(301_500));

    assert_eq!(
        duration_ms("audio/mp4", &mp4(44_100, 44_100 * 65)),
        Some(65_000)
    );
}

#[test]
fn unknown_or_broken_files_have_no_length() {
    // Fragmented MP4 (Safari) leaves the movie's duration at zero.
    assert_eq!(duration_ms("audio/mp4", &mp4(1000, 0)), None);
    assert_eq!(duration_ms("audio/mp4", &mp4(0, 1000)), None);
    assert_eq!(duration_ms("audio/ogg", b"OggS\0\x02"), None);
    assert_eq!(duration_ms("audio/webm", b"\x1a\x45\xdf\xa3\x9f\x42"), None);
    assert_eq!(duration_ms("audio/wav", b"RIFF\0\0\0\0WAVEdata"), None);
    assert_eq!(duration_ms("image/png", &wav(1, false)), None);
    let mut truncated = webm(None, &[(0, 0), (0, 20)]);
    truncated.truncate(truncated.len() - 3);
    // A recording cut off mid-block still has the times read so far.
    assert_eq!(duration_ms("audio/webm", &truncated), Some(20));
}
