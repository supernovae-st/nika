// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media facts read from header bytes: an image's dimensions, a sound's
//! duration when its header states one, an SVG root's declared size.
//! Nothing is decoded past the header, no pixel is drawn, no terminal image
//! protocol is guessed, and a duration the header does not state is never
//! estimated from a bitrate: it stays unknown, in words.

use nika_display::theme::Role;

use super::Format;
use super::cells::{Row, floor_boundary, size_words_u64};

/// A big-endian `u16` at `at`, when the bytes hold it.
pub(crate) fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

/// A big-endian `u32` at `at`.
pub(crate) fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// A little-endian `u16` at `at`.
pub(crate) fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

/// A little-endian `u32` at `at`.
pub(crate) fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// A little-endian 24-bit value at `at`.
fn le24(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        0,
    ]))
}

/// What an image header states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    /// Width in pixels.
    pub(crate) width: u32,
    /// Height in pixels.
    pub(crate) height: u32,
    /// A detail the header states (colour, encoding), in words.
    pub(crate) detail: Option<String>,
}

/// The PNG header: IHDR is the first chunk.
fn png(b: &[u8]) -> Option<Image> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    let depth = *b.get(24)?;
    let colour = match *b.get(25)? {
        0 => "grey",
        2 => "RGB",
        3 => "indexed colour",
        4 => "grey with alpha",
        6 => "RGBA",
        _ => "an unknown colour type",
    };
    Some(Image {
        width: be32(b, 16)?,
        height: be32(b, 20)?,
        detail: Some(format!("{depth}-bit {colour}")),
    })
}

/// The GIF logical screen.
fn gif(b: &[u8]) -> Option<Image> {
    Some(Image {
        width: u32::from(le16(b, 6)?),
        height: u32::from(le16(b, 8)?),
        detail: None,
    })
}

/// The JPEG frame header, found by walking the marker segments.
fn jpeg(b: &[u8]) -> Option<Image> {
    let mut at = 2;
    for _ in 0..4096 {
        if *b.get(at)? != 0xFF {
            return None;
        }
        let marker = *b.get(at + 1)?;
        match marker {
            0xFF => at += 1,
            0x01 | 0xD0..=0xD8 => at += 2,
            0xD9 | 0xDA => return None,
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let encoding = if matches!(marker, 0xC2 | 0xC6 | 0xCA | 0xCE) {
                    "progressive"
                } else {
                    "baseline"
                };
                let components = match *b.get(at + 9)? {
                    1 => "grey",
                    3 => "colour",
                    4 => "CMYK",
                    _ => "an unusual component count",
                };
                return Some(Image {
                    height: u32::from(be16(b, at + 5)?),
                    width: u32::from(be16(b, at + 7)?),
                    detail: Some(format!("{encoding} · {components}")),
                });
            }
            _ => {
                let length = usize::from(be16(b, at + 2)?);
                if length < 2 {
                    return None;
                }
                at = at.checked_add(2 + length)?;
            }
        }
    }
    None
}

/// The `WebP` bitstream header of its first chunk.
fn webp(b: &[u8]) -> Option<Image> {
    match b.get(12..16)? {
        b"VP8 " => {
            if b.get(23..26)? != b"\x9D\x01\x2A" {
                return None;
            }
            Some(Image {
                width: u32::from(le16(b, 26)? & 0x3FFF),
                height: u32::from(le16(b, 28)? & 0x3FFF),
                detail: Some("lossy".to_owned()),
            })
        }
        b"VP8L" => {
            if *b.get(20)? != 0x2F {
                return None;
            }
            let bits = le32(b, 21)?;
            Some(Image {
                width: (bits & 0x3FFF) + 1,
                height: ((bits >> 14) & 0x3FFF) + 1,
                detail: Some("lossless".to_owned()),
            })
        }
        b"VP8X" => {
            let flags = *b.get(20)?;
            let animated = if flags & 0x02 == 0 {
                ""
            } else {
                " · animated"
            };
            Some(Image {
                width: le24(b, 24)? + 1,
                height: le24(b, 27)? + 1,
                detail: Some(format!("extended{animated}")),
            })
        }
        _ => None,
    }
}

/// What the header of an image in `format` states, if it states it.
pub(crate) fn image(format: Format, b: &[u8]) -> Option<Image> {
    match format {
        Format::Png => png(b),
        Format::Gif => gif(b),
        Format::Jpeg => jpeg(b),
        Format::Webp => webp(b),
        _ => None,
    }
}

/// What a sound's header states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sound {
    /// The encoding in words (`MPEG-1 Layer III`, `PCM 16-bit`).
    pub(crate) encoding: String,
    /// Samples per second.
    pub(crate) rate: u32,
    /// Channels.
    pub(crate) channels: u16,
    /// Bitrate in kilobits per second, when the header states one.
    pub(crate) kbps: Option<u32>,
    /// The duration the header states, and which header states it.
    pub(crate) duration: Option<(u64, &'static str)>,
    /// Audio bytes the header states beyond the bytes held.
    pub(crate) short_by: Option<u64>,
}

/// One MPEG audio frame header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    version: u8,
    layer: u8,
    kbps: u32,
    rate: u32,
    mono: bool,
    samples: u32,
    length: usize,
}

/// The bitrate, in kbps, of index `index` for an MPEG `version` (1, 2, or
/// 25 for 2.5) and `layer`.
fn bitrate(version: u8, layer: u8, index: usize) -> Option<u32> {
    const V1L1: [u32; 14] = [
        32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
    ];
    const V1L2: [u32; 14] = [
        32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
    ];
    const V1L3: [u32; 14] = [
        32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const V2L1: [u32; 14] = [
        32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
    ];
    const V2L23: [u32; 14] = [8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let table = match (version, layer) {
        (1, 1) => &V1L1,
        (1, 2) => &V1L2,
        (1, _) => &V1L3,
        (_, 1) => &V2L1,
        _ => &V2L23,
    };
    table.get(index.checked_sub(1)?).copied()
}

/// The MPEG audio frame header at the start of `b`, when it is one.
pub(crate) fn mp3_frame(b: &[u8]) -> Option<Frame> {
    let h = be32(b, 0)?;
    if h >> 21 != 0x7FF {
        return None;
    }
    let version = match (h >> 19) & 3 {
        0 => 25,
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let layer = match (h >> 17) & 3 {
        1 => 3,
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let kbps = bitrate(version, layer, usize::try_from((h >> 12) & 0xF).ok()?)?;
    let base: u32 = match (h >> 10) & 3 {
        0 => 44_100,
        1 => 48_000,
        2 => 32_000,
        _ => return None,
    };
    let rate = match version {
        1 => base,
        2 => base / 2,
        _ => base / 4,
    };
    let padding = (h >> 9) & 1;
    let samples = match (layer, version) {
        (1, _) => 384,
        (3, 2 | 25) => 576,
        _ => 1152,
    };
    let length = if layer == 1 {
        (12 * kbps * 1000 / rate + padding) * 4
    } else {
        samples / 8 * kbps * 1000 / rate + padding
    };
    Some(Frame {
        version,
        layer,
        kbps,
        rate,
        mono: (h >> 6) & 3 == 3,
        samples,
        length: usize::try_from(length).ok()?,
    })
}

/// The end of an `ID3v2` tag at the start of `b` and the `TLEN` it states.
fn id3(b: &[u8]) -> Option<(usize, Option<u64>)> {
    if !b.starts_with(b"ID3") {
        return None;
    }
    let major = *b.get(3)?;
    let flags = *b.get(5)?;
    let syncsafe = |at: usize| -> Option<usize> {
        let mut value = 0usize;
        for byte in b.get(at..at + 4)? {
            value = (value << 7) | usize::from(byte & 0x7F);
        }
        Some(value)
    };
    let footer = if flags & 0x10 == 0 { 0 } else { 10 };
    let end = 10 + syncsafe(6)? + footer;
    let mut tlen = None;
    let mut at = 10;
    for _ in 0..256 {
        let Some(id) = b.get(at..at + 4) else { break };
        if id[0] == 0 || major < 3 || at + 10 > end {
            break;
        }
        let size = if major >= 4 {
            syncsafe(at + 4)?
        } else {
            usize::try_from(be32(b, at + 4)?).ok()?
        };
        if id == b"TLEN" {
            let text = b.get(at + 11..at + 10 + size).unwrap_or_default();
            let digits: String = text
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .map(|c| char::from(*c))
                .collect();
            tlen = digits.parse().ok();
        }
        at = at.checked_add(10 + size)?;
    }
    Some((end, tlen))
}

/// The frame count an Xing/Info or VBRI header states, in the first frame.
fn frame_count(frame: &[u8], f: Frame) -> Option<u32> {
    let side = match (f.version, f.mono) {
        (1, false) => 32,
        (1, true) | (_, false) => 17,
        (_, true) => 9,
    };
    let xing = 4 + side;
    if matches!(frame.get(xing..xing + 4), Some(b"Xing" | b"Info"))
        && be32(frame, xing + 4)? & 1 == 1
    {
        return be32(frame, xing + 8);
    }
    if frame.get(36..40) == Some(b"VBRI".as_slice()) {
        return be32(frame, 36 + 14);
    }
    None
}

/// What an MP3's headers state: the first frame, and the duration an ID3
/// `TLEN` or an Xing/Info/VBRI frame count states.
fn mp3(b: &[u8]) -> Option<Sound> {
    let (start, tlen) = id3(b).unwrap_or((0, None));
    let window = b.get(start..)?;
    let offset = (0..window.len().min(4096)).find(|at| mp3_frame(&window[*at..]).is_some())?;
    let frame_bytes = &window[offset..];
    let f = mp3_frame(frame_bytes)?;
    let counted = frame_count(frame_bytes, f).map(|frames| {
        (
            u64::from(frames) * u64::from(f.samples) * 1000 / u64::from(f.rate),
            "Xing/VBRI frame count",
        )
    });
    let version = if f.version == 25 {
        "2.5".to_owned()
    } else {
        f.version.to_string()
    };
    let layer = ["I", "II", "III"][usize::from(f.layer.clamp(1, 3) - 1)];
    Some(Sound {
        encoding: format!("MPEG-{version} Layer {layer}"),
        rate: f.rate,
        channels: if f.mono { 1 } else { 2 },
        kbps: Some(f.kbps),
        duration: tlen.map(|ms| (ms, "ID3 TLEN")).or(counted),
        short_by: None,
    })
}

/// What a WAV's chunks state: the `fmt ` chunk and the `data` size.
fn wav(b: &[u8], total: usize) -> Option<Sound> {
    let mut at = 12usize;
    let mut format = None;
    for _ in 0..64 {
        let id = b.get(at..at + 4)?;
        let size = usize::try_from(le32(b, at + 4)?).ok()?;
        if id == b"fmt " {
            format = Some((
                le16(b, at + 8)?,
                le16(b, at + 10)?,
                le32(b, at + 12)?,
                le32(b, at + 16)?,
                le16(b, at + 22)?,
            ));
        } else if id == b"data" {
            let (codec, channels, rate, byte_rate, bits) = format?;
            let encoding = match codec {
                1 => format!("PCM {bits}-bit"),
                3 => format!("float {bits}-bit"),
                6 => "A-law".to_owned(),
                7 => "mu-law".to_owned(),
                0xFFFE => format!("extensible {bits}-bit"),
                other => format!("codec 0x{other:04X}"),
            };
            let stated = u64::try_from(size).ok()?;
            let streamed = size == 0xFFFF_FFFF || byte_rate == 0;
            let held = u64::try_from(total.saturating_sub(at + 8)).ok()?;
            return Some(Sound {
                encoding,
                rate,
                channels,
                kbps: byte_rate.checked_mul(8).map(|bits| bits / 1000),
                duration: (!streamed)
                    .then(|| (stated * 1000 / u64::from(byte_rate), "WAV data chunk")),
                short_by: if streamed {
                    None
                } else {
                    stated.checked_sub(held).filter(|s| *s > 0)
                },
            });
        }
        at = at.checked_add(8 + size + (size & 1))?;
    }
    None
}

/// What the header of a sound in `format` states; `total` is the size of
/// the whole object (the bytes read may be fewer).
pub(crate) fn sound(format: Format, b: &[u8], total: usize) -> Option<Sound> {
    match format {
        Format::Mp3 => mp3(b),
        Format::Wav => wav(b, total),
        _ => None,
    }
}

/// What an SVG root element declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SvgRoot {
    /// The `width` attribute as written.
    pub(crate) width: Option<String>,
    /// The `height` attribute as written.
    pub(crate) height: Option<String>,
    /// The `viewBox` attribute as written.
    pub(crate) view_box: Option<String>,
    /// The first `<title>` text.
    pub(crate) title: Option<String>,
}

/// The value of attribute `name` in one tag's text, as written.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let body = &value[1..];
        let end = body.find(quote)?;
        return Some(body[..end].chars().take(64).collect());
    }
    None
}

/// The SVG root in the first bytes, when they are an SVG document: only
/// a declaration, comments or a doctype may come before `<svg`.
pub(crate) fn svg_root(head: &[u8]) -> Option<SvgRoot> {
    let window = &head[..head.len().min(16 * 1024)];
    let text = String::from_utf8_lossy(window);
    let at = text.find("<svg")?;
    let before = text[..at].trim_start_matches('\u{FEFF}').trim();
    if !(before.is_empty()
        || before.starts_with("<?xml")
        || before.starts_with("<!--")
        || before.starts_with("<!DOCTYPE"))
    {
        return None;
    }
    let tag = &text[at..];
    let tag = &tag[..floor_boundary(tag, tag.find('>').unwrap_or(tag.len()).min(4096))];
    let title = text.find("<title>").and_then(|start| {
        let rest = &text[start + 7..];
        rest.find("</title>")
            .map(|end| rest[..end].chars().take(120).collect())
    });
    Some(SvgRoot {
        width: attribute(tag, "width"),
        height: attribute(tag, "height"),
        view_box: attribute(tag, "viewBox"),
        title,
    })
}

/// A duration in words: `850 ms`, `12.4 s`, `3 min 12 s`, `1 h 02 min`.
pub(crate) fn duration_words(ms: u64) -> String {
    match ms {
        ms if ms < 1_000 => format!("{ms} ms"),
        ms if ms < 60_000 => format!("{}.{} s", ms / 1_000, ms % 1_000 / 100),
        ms if ms < 3_600_000 => format!("{} min {:02} s", ms / 60_000, ms % 60_000 / 1_000),
        ms => format!("{} h {:02} min", ms / 3_600_000, ms % 3_600_000 / 60_000),
    }
}

/// `w × h` in the glyph column in use.
pub(crate) fn dimensions(width: u32, height: u32, ascii: bool) -> String {
    let by = if ascii { "x" } else { "×" };
    format!("{width} {by} {height}")
}

/// The fact rows of an image: what its header states, what was declared,
/// and the honest preview line.
pub(crate) fn image_rows(
    format: Format,
    b: &[u8],
    declared: Option<(u32, u32)>,
    ascii: bool,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let stated = image(format, b);
    match &stated {
        Some(found) => {
            rows.push(Row::new(
                "dimensions",
                format!(
                    "{} (stated by the header)",
                    dimensions(found.width, found.height, ascii)
                ),
            ));
            if let Some(detail) = &found.detail {
                rows.push(Row::new("encoding", detail.clone()));
            }
        }
        None => rows.push(Row::toned(
            "dimensions",
            "unknown: the header is cut or unreadable".to_owned(),
            Role::Warn,
        )),
    }
    if let Some((width, height)) = declared {
        let agree = stated
            .as_ref()
            .is_none_or(|s| (s.width, s.height) == (width, height));
        let words = dimensions(width, height, ascii);
        rows.push(if agree {
            Row::new("declared", words)
        } else {
            Row::toned(
                "declared",
                format!("{words}: the header disagrees"),
                Role::Warn,
            )
        });
    }
    rows.push(Row::toned(
        "preview",
        "not drawn in the terminal: open the file outside it".to_owned(),
        Role::Dim,
    ));
    rows
}

/// The fact rows of a sound: what its header states and what was declared.
pub(crate) fn sound_rows(
    format: Format,
    b: &[u8],
    total: usize,
    declared_ms: Option<u64>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let Some(found) = sound(format, b, total) else {
        rows.push(Row::toned(
            "header",
            "unreadable: no frame or format chunk in the bytes read".to_owned(),
            Role::Warn,
        ));
        return rows;
    };
    rows.push(Row::new("encoding", found.encoding.clone()));
    let channels = match found.channels {
        1 => "mono".to_owned(),
        2 => "stereo".to_owned(),
        n => format!("{n} channels"),
    };
    let (whole, part) = (found.rate / 1000, found.rate % 1000);
    let rate = match part {
        0 => format!("{whole} kHz"),
        part if part % 100 == 0 => format!("{whole}.{} kHz", part / 100),
        part => format!("{whole}.{:02} kHz", part / 10),
    };
    let kbps = found
        .kbps
        .map(|k| format!(" · {k} kbps"))
        .unwrap_or_default();
    rows.push(Row::new("signal", format!("{rate} · {channels}{kbps}")));
    rows.push(match found.duration {
        Some((ms, source)) => Row::new(
            "duration",
            format!("{} (stated by the {source})", duration_words(ms)),
        ),
        None => Row::toned("duration", "not stated by the header".to_owned(), Role::Dim),
    });
    if let Some(short) = found.short_by {
        rows.push(Row::toned(
            "held",
            format!(
                "the header states {} more audio than the file holds",
                size_words_u64(short)
            ),
            Role::Warn,
        ));
    }
    if let Some(ms) = declared_ms {
        rows.push(Row::new("declared", duration_words(ms)));
    }
    rows.push(Row::toned(
        "playback",
        "not played in the terminal".to_owned(),
        Role::Dim,
    ));
    rows
}

/// The fact rows of an SVG: its declared size and title, never a raster.
pub(crate) fn svg_rows(head: &[u8]) -> Vec<Row> {
    let root = svg_root(head).unwrap_or_default();
    let mut rows = vec![Row::new("kind", "vector image (SVG)".to_owned())];
    let size = match (&root.width, &root.height) {
        (Some(w), Some(h)) => Row::new("size", format!("{w} by {h} (declared by the root)")),
        _ => Row::toned("size", "not declared by the root".to_owned(), Role::Dim),
    };
    rows.push(size);
    if let Some(view_box) = root.view_box {
        rows.push(Row::new("viewBox", view_box));
    }
    if let Some(title) = root.title {
        rows.push(Row::new("title", title));
    }
    rows.push(Row::toned(
        "preview",
        "not drawn in the terminal: open the file outside it".to_owned(),
        Role::Dim,
    ));
    rows
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(width.to_be_bytes());
        b.extend(height.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn image_headers_state_their_dimensions() {
        let png = png_header(1024, 768);
        let found = image(Format::Png, &png).expect("png header");
        assert_eq!((found.width, found.height), (1024, 768));
        assert_eq!(found.detail.as_deref(), Some("8-bit RGBA"));
        assert_eq!(
            image(Format::Png, &png[..20]),
            None,
            "a cut header states nothing"
        );
        let gif = b"GIF89a\x40\x01\xF0\x00";
        assert_eq!(
            image(Format::Gif, gif).map(|i| (i.width, i.height)),
            Some((320, 240))
        );
        let jpeg = b"\xFF\xD8\xFF\xE0\x00\x04ab\xFF\xC0\x00\x11\x08\x01\xE0\x02\x80\x03";
        let found = image(Format::Jpeg, jpeg).expect("jpeg sof0");
        assert_eq!((found.width, found.height), (640, 480));
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\x10\0\0\0".to_vec();
        webp.extend([0x3F, 0x01, 0x00, 0xEF, 0x00, 0x00]);
        assert_eq!(
            image(Format::Webp, &webp).map(|i| (i.width, i.height)),
            Some((320, 240))
        );
    }

    fn wav_header(seconds: u32) -> Vec<u8> {
        let rate: u32 = 8_000;
        let data = rate * 2 * seconds;
        let mut b = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
        b.extend(rate.to_le_bytes());
        b.extend((rate * 2).to_le_bytes());
        b.extend([2, 0, 16, 0]);
        b.extend(b"data");
        b.extend(data.to_le_bytes());
        b
    }

    #[test]
    fn a_wav_states_its_duration_and_a_short_file_is_named() {
        let header = wav_header(3);
        let full = header.len() + 48_000;
        let found = sound(Format::Wav, &header, full).expect("wav");
        assert_eq!(found.duration, Some((3_000, "WAV data chunk")));
        assert_eq!(found.short_by, None);
        let cut = sound(Format::Wav, &header, header.len() + 10).expect("wav");
        assert_eq!(cut.short_by, Some(47_990));
    }

    #[test]
    fn an_mp3_duration_is_stated_or_unknown_never_estimated() {
        // MPEG-1 Layer III, 128 kbps, 44.1 kHz, stereo: no Xing header.
        let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
        frame.resize(417, 0);
        let bare = sound(Format::Mp3, &frame, frame.len()).expect("mp3");
        assert_eq!(bare.duration, None);
        assert_eq!(bare.encoding, "MPEG-1 Layer III");
        let mut xing = frame.clone();
        xing[36..40].copy_from_slice(b"Xing");
        xing[40..44].copy_from_slice(&1_u32.to_be_bytes());
        xing[44..48].copy_from_slice(&100_u32.to_be_bytes());
        let stated = sound(Format::Mp3, &xing, xing.len()).expect("mp3");
        assert_eq!(stated.duration, Some((2_612, "Xing/VBRI frame count")));
    }

    #[test]
    fn an_svg_root_declares_its_size_and_nothing_else_is_guessed() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="640" height="480" viewBox="0 0 640 480"><title>Revenue</title></svg>"#;
        let root = svg_root(svg).expect("svg");
        assert_eq!(root.width.as_deref(), Some("640"));
        assert_eq!(root.view_box.as_deref(), Some("0 0 640 480"));
        assert_eq!(root.title.as_deref(), Some("Revenue"));
        assert!(svg_root(b"<html><svg></svg></html>").is_none());
    }

    #[test]
    fn durations_read_as_words() {
        assert_eq!(duration_words(850), "850 ms");
        assert_eq!(duration_words(12_400), "12.4 s");
        assert_eq!(duration_words(192_000), "3 min 12 s");
        assert_eq!(duration_words(3_720_000), "1 h 02 min");
    }
}
