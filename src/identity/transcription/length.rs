//! How long a recording lasts, read from its container where that is cheap
//! and reliable: WAV, Ogg (Opus or Vorbis), WebM and MP4. None when the file
//! doesn't say; a fragmented MP4, for one, carries no total.

pub(super) fn duration_ms(mime: &str, bytes: &[u8]) -> Option<i64> {
    match mime {
        "audio/wav" => wav(bytes),
        "audio/ogg" => ogg(bytes),
        "audio/webm" => webm(bytes),
        "audio/mp4" => mp4(bytes),
        _ => None,
    }
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
fn be64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}
fn ms(units: u64, per_second: u64) -> Option<i64> {
    i64::try_from(units.checked_mul(1000)? / (per_second > 0).then_some(per_second)?).ok()
}

/// RIFF chunks: the `fmt ` chunk's bytes per second, then the `data` size.
fn wav(bytes: &[u8]) -> Option<i64> {
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let (mut at, mut rate) = (12usize, None);
    loop {
        let (id, size, body) = (bytes.get(at..at + 4)?, le32(bytes, at + 4)?, at + 8);
        match id {
            b"fmt " => rate = le32(bytes, body + 8),
            // A recorder that never finished writes a size larger than the file.
            b"data" => {
                let size = u64::from(size).min(bytes.len().saturating_sub(body) as u64);
                return ms(size, u64::from(rate?));
            }
            _ => {}
        }
        at = body.checked_add(size as usize + (size as usize & 1))?;
    }
}

/// The first page names the codec; the last page's granule position counts
/// samples from the start (Opus always at 48 kHz, after its pre-skip).
fn ogg(bytes: &[u8]) -> Option<i64> {
    let packet = bytes.get(27 + usize::from(*bytes.get(26)?)..)?;
    let (rate, skip) = if packet.starts_with(b"OpusHead") {
        (
            48_000,
            u64::from(u16::from_le_bytes(packet.get(10..12)?.try_into().ok()?)),
        )
    } else if packet.starts_with(b"\x01vorbis") {
        (u64::from(le32(packet, 12)?), 0)
    } else {
        return None;
    };
    let mut at = bytes.len().checked_sub(27)?;
    loop {
        if bytes.get(at..at + 5) == Some(b"OggS\0") {
            let granule = u64::from_le_bytes(bytes.get(at + 6..at + 14)?.try_into().ok()?);
            // No packet ends on a page marked with all ones; look further back.
            if granule != u64::MAX {
                return ms(granule.saturating_sub(skip), rate);
            }
        }
        at = at.checked_sub(1)?;
    }
}

const EBML: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const CLUSTER: u32 = 0x1F43_B675;
const TIMECODE_SCALE: u32 = 0x2A_D7B1;
const DURATION: u32 = 0x4489;
const TIMECODE: u32 = 0xE7;
const SIMPLE_BLOCK: u32 = 0xA3;
const BLOCK_GROUP: u32 = 0xA0;
const BLOCK: u32 = 0xA1;
/// Segment children: one of these ends a cluster written with an unknown size.
const TOP_LEVEL: [u32; 8] = [
    CLUSTER,
    INFO,
    0x114D_9B74,
    0x1654_AE6B,
    0x1C53_BB6B,
    0x1254_C367,
    0x1941_A469,
    0x1043_A770,
];

/// An EBML element at `at`: its ID, its size (None when written as unknown,
/// as browsers do while recording) and where its contents start.
fn element(bytes: &[u8], at: usize) -> Option<(u32, Option<usize>, usize)> {
    let id_length = bytes.get(at)?.leading_zeros() as usize + 1;
    if id_length > 4 {
        return None;
    }
    let id = bytes
        .get(at..at + id_length)?
        .iter()
        .fold(0, |id, b| id << 8 | u32::from(*b));
    let first = *bytes.get(at + id_length)?;
    let size_length = first.leading_zeros() as usize + 1;
    if size_length > 8 {
        return None;
    }
    // The size's first byte keeps the bits after its length marker.
    let marker = (0xFFu16 >> size_length) as u8;
    let rest = bytes.get(at + id_length + 1..at + id_length + size_length)?;
    let unknown = first & marker == marker && rest.iter().all(|b| *b == 0xFF);
    let size = rest.iter().fold(u64::from(first & marker), |size, b| {
        size << 8 | u64::from(*b)
    });
    let size = if unknown {
        None
    } else {
        Some(usize::try_from(size).ok()?)
    };
    Some((id, size, at + id_length + size_length))
}
fn unsigned(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |value, b| value << 8 | u64::from(*b))
}
/// A block's time, in ticks after its cluster's: after the track number.
fn block_time(block: &[u8]) -> Option<i64> {
    let track = block.first()?.leading_zeros() as usize + 1;
    Some(i64::from(i16::from_be_bytes(
        block.get(track..track + 2)?.try_into().ok()?,
    )))
}

/// The segment's declared duration, or else the time of its last block.
fn webm(bytes: &[u8]) -> Option<i64> {
    let (id, size, data) = element(bytes, 0)?;
    if id != EBML {
        return None;
    }
    let (id, size, start) = element(bytes, data.checked_add(size?)?)?;
    if id != SEGMENT {
        return None;
    }
    let end = size.map_or(bytes.len(), |size| {
        start.saturating_add(size).min(bytes.len())
    });
    let (mut scale, mut declared, mut last) = (1_000_000u64, None::<f64>, None::<i64>);
    let mut at = start;
    while at < end {
        let (id, size, data) = element(bytes, at)?;
        if id == CLUSTER {
            let stop = size.map_or(end, |size| data.saturating_add(size).min(end));
            let (mut inside, mut base) = (data, 0i64);
            while inside < stop {
                let Some((child, length, contents)) = element(bytes, inside) else {
                    break;
                };
                if size.is_none() && TOP_LEVEL.contains(&child) {
                    break;
                }
                let value =
                    bytes.get(contents..contents.saturating_add(length?).min(bytes.len()))?;
                match child {
                    TIMECODE => base = i64::try_from(unsigned(value)).ok()?,
                    SIMPLE_BLOCK => last = last.max(block_time(value).map(|t| base + t)),
                    BLOCK_GROUP => {
                        if let Some((BLOCK, Some(n), block)) = element(value, 0) {
                            let time = value.get(block..block + n).and_then(block_time);
                            last = last.max(time.map(|t| base + t));
                        }
                    }
                    _ => {}
                }
                inside = contents + length?;
            }
            at = if size.is_none() { inside } else { stop };
            continue;
        }
        let size = size?;
        if id == INFO {
            let (mut inside, stop) = (data, data.saturating_add(size).min(end));
            while inside < stop {
                let (child, length, contents) = element(bytes, inside)?;
                let value = bytes.get(contents..contents + length?)?;
                match child {
                    TIMECODE_SCALE => scale = unsigned(value),
                    DURATION => {
                        declared = match value.len() {
                            4 => Some(f64::from(f32::from_be_bytes(value.try_into().ok()?))),
                            8 => Some(f64::from_be_bytes(value.try_into().ok()?)),
                            _ => None,
                        }
                    }
                    _ => {}
                }
                inside = contents + length?;
            }
        }
        at = data.checked_add(size)?;
    }
    let ticks = declared
        .filter(|ticks| ticks.is_finite() && *ticks >= 0.0)
        .map(|ticks| ticks as u64)
        .or_else(|| last.and_then(|t| u64::try_from(t).ok()))?;
    i64::try_from(ticks.checked_mul(scale)? / 1_000_000).ok()
}

/// The boxes of an MP4: four-character type and contents.
fn boxes(bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let (header, size) = match be32(bytes, at)? {
            1 => (16, usize::try_from(be64(bytes, at + 8)?).ok()?),
            0 => (8, bytes.len() - at),
            size => (8, size as usize),
        };
        let kind = bytes.get(at + 4..at + 8)?;
        let contents = bytes.get(at + header..at.checked_add(size)?.min(bytes.len()))?;
        at = at.checked_add(size.max(header))?;
        Some((kind, contents))
    })
}

/// The movie header's duration and timescale; fragmented files leave them 0.
fn mp4(bytes: &[u8]) -> Option<i64> {
    let (_, moov) = boxes(bytes).find(|(kind, _)| *kind == b"moov")?;
    let (_, header) = boxes(moov).find(|(kind, _)| *kind == b"mvhd")?;
    let (scale, duration) = match header.first()? {
        0 => (be32(header, 12)?, u64::from(be32(header, 16)?)),
        1 => (be32(header, 20)?, be64(header, 24)?),
        _ => return None,
    };
    if duration == 0 || duration == u64::from(u32::MAX) || duration == u64::MAX {
        return None;
    }
    ms(duration, u64::from(scale))
}

#[cfg(test)]
#[path = "length_tests.rs"]
mod tests;
