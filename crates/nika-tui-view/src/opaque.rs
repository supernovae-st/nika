// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bytes no viewer here decodes: PDF, office documents, archives, video
//! and anything unknown. They are shown by what their first bytes state (a
//! version, a container brand, the first entry names) and a short hex head,
//! never by a guess at their content; a protected object keeps even its
//! hex head to itself.

use std::fmt::Write as _;

use nika_display::theme::Role;

use super::Format;
use super::cells::{Body, Row, paint, plain};
use super::media::{le16, le32};

/// The entry names of the first local file headers of a ZIP (at most
/// `max`, within the bytes held). The walk stops where an entry defers its
/// sizes to after its data: finding the next header would mean inflating.
pub(crate) fn zip_names(b: &[u8], max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while out.len() < max && b.get(at..at + 4) == Some(b"PK\x03\x04".as_slice()) {
        let (Some(flags), Some(size), Some(name), Some(extra)) = (
            le16(b, at + 6),
            le32(b, at + 18),
            le16(b, at + 26),
            le16(b, at + 28),
        ) else {
            break;
        };
        let (name, extra) = (usize::from(name), usize::from(extra));
        let Some(bytes) = b.get(at + 30..at + 30 + name) else {
            break;
        };
        out.push(String::from_utf8_lossy(bytes).chars().take(120).collect());
        let Ok(size) = usize::try_from(size) else {
            break;
        };
        if flags & 0x08 != 0 {
            break;
        }
        at = match at.checked_add(30 + name + extra + size) {
            Some(next) => next,
            None => break,
        };
    }
    out
}

/// The office document a ZIP holds, by its first entry names; a plain ZIP
/// when they name none.
pub(crate) fn zip_kind(head: &[u8]) -> Format {
    let names = zip_names(head, 32);
    let under = |dir: &str| names.iter().any(|n| n.starts_with(dir));
    if under("word/") {
        Format::Docx
    } else if under("xl/") {
        Format::Xlsx
    } else if under("ppt/") {
        Format::Pptx
    } else {
        Format::Zip
    }
}

/// The brand of an ISO media file (`ftyp` box), when the bytes hold it.
fn brand(head: &[u8]) -> Option<String> {
    let raw = head.get(8..12)?;
    Some(String::from_utf8_lossy(raw).trim().to_owned())
}

/// What an ISO media file is by its brand: a video, or opaque bytes for
/// the audio and still-image brands no viewer here reads.
pub(crate) fn ftyp_kind(head: &[u8]) -> Format {
    match brand(head).as_deref() {
        Some("M4A" | "M4B" | "M4P" | "heic" | "heix" | "mif1" | "msf1" | "avif" | "avis") => {
            Format::Opaque
        }
        _ => Format::Video,
    }
}

/// The container a video's first bytes name.
fn container(head: &[u8]) -> String {
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        let brand = brand(head).unwrap_or_default();
        return match brand.as_str() {
            "qt" => "QuickTime (brand qt)".to_owned(),
            other => format!("MP4 (brand {other})"),
        };
    }
    if head.starts_with(b"\x1A\x45\xDF\xA3") {
        let window = &head[..head.len().min(64)];
        let webm = window.windows(4).any(|w| w == b"webm");
        return if webm { "WebM" } else { "Matroska" }.to_owned();
    }
    if head.starts_with(b"RIFF") {
        return "AVI".to_owned();
    }
    "not recognised in the first bytes".to_owned()
}

/// The fact rows of a format no viewer here decodes.
pub(crate) fn rows(format: Format, head: &[u8]) -> Vec<Row> {
    let mut out = Vec::new();
    match format {
        Format::Pdf => {
            let version: String = head
                .get(5..)
                .unwrap_or_default()
                .iter()
                .take_while(|b| b.is_ascii_digit() || **b == b'.')
                .map(|b| char::from(*b))
                .collect();
            out.push(Row::new("version", format!("PDF {version}")));
            out.push(Row::toned(
                "pages",
                "not counted: no PDF decoder here".to_owned(),
                Role::Dim,
            ));
        }
        Format::Docx | Format::Xlsx | Format::Pptx | Format::Zip => {
            let names = zip_names(head, 32);
            out.push(Row::new("container", "ZIP".to_owned()));
            let shown: Vec<&str> = names.iter().take(4).map(String::as_str).collect();
            let more = if names.len() > shown.len() {
                ", …"
            } else {
                ""
            };
            let seen = if names.is_empty() {
                "none readable in the bytes held".to_owned()
            } else {
                format!(
                    "{} in the first bytes: {}{more}",
                    names.len(),
                    shown.join(", ")
                )
            };
            out.push(Row::new("entries", seen));
        }
        Format::Video => {
            out.push(Row::new("container", container(head)));
            out.push(Row::toned(
                "duration",
                "not read: no container walk here".to_owned(),
                Role::Dim,
            ));
        }
        _ => out.push(Row::new("kind", "bytes no viewer here decodes".to_owned())),
    }
    let open = if format == Format::Video {
        "not played in the terminal: open the file outside it"
    } else {
        "open the file with an application that reads it"
    };
    out.push(Row::toned("preview", open.to_owned(), Role::Dim));
    out
}

/// The bytes a hex head shows at most.
const HEAD: usize = 64;

/// Keep a short hex head of `head`: offset, bytes and their printable
/// ASCII, 16, 8 or 4 bytes a row, whichever the width holds; withheld,
/// in words, when the object is protected.
pub(crate) fn hex_head(body: &mut Body, head: &[u8], protected: bool) {
    let canvas = body.canvas();
    if protected {
        body.push(
            vec![paint(
                "hex head withheld: the object is protected",
                Role::Dim,
                canvas.color,
            )],
            false,
        );
        return;
    }
    let per_row = [16usize, 8, 4]
        .into_iter()
        .find(|n| 11 + n * 4 <= body.width())
        .unwrap_or(4);
    let shown = &head[..head.len().min(HEAD)];
    for (row, chunk) in shown.chunks(per_row).enumerate() {
        let mut hex = String::with_capacity(per_row * 3);
        for byte in chunk {
            let _ = write!(hex, "{byte:02x} ");
        }
        let text: String = chunk
            .iter()
            .map(|b| {
                if b.is_ascii_graphic() || *b == b' ' {
                    char::from(*b)
                } else {
                    '.'
                }
            })
            .collect();
        let offset = format!("{:08x}  ", row * per_row);
        let pad = " ".repeat((per_row - chunk.len()) * 3);
        let spans = vec![
            paint(offset, Role::Dim, canvas.color),
            plain(format!("{hex}{pad} ")),
            paint(text, Role::Dim, canvas.color),
        ];
        if !body.push(spans, false) {
            return;
        }
    }
    if head.len() > HEAD {
        body.push(
            vec![paint(
                format!("the first {HEAD} bytes"),
                Role::Dim,
                canvas.color,
            )],
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Canvas;

    fn local(name: &str) -> Vec<u8> {
        let mut b = b"PK\x03\x04".to_vec();
        b.extend([20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        b.extend(0_u32.to_le_bytes());
        b.extend(0_u32.to_le_bytes());
        b.extend(u16::try_from(name.len()).unwrap_or(0).to_le_bytes());
        b.extend(0_u16.to_le_bytes());
        b.extend(name.as_bytes());
        b
    }

    #[test]
    fn a_zip_names_the_office_document_its_entries_hold() {
        let mut docx = local("[Content_Types].xml");
        docx.extend(local("word/document.xml"));
        assert_eq!(
            zip_names(&docx, 8),
            ["[Content_Types].xml", "word/document.xml"]
        );
        assert_eq!(zip_kind(&docx), Format::Docx);
        assert_eq!(zip_kind(&local("xl/workbook.xml")), Format::Xlsx);
        assert_eq!(zip_kind(&local("notes.txt")), Format::Zip);
        assert_eq!(zip_names(b"PK\x03\x04\x14", 8), Vec::<String>::new());
    }

    #[test]
    fn video_brands_and_containers_are_named() {
        assert_eq!(ftyp_kind(b"\0\0\0\x18ftypisom\0\0\0\0"), Format::Video);
        assert_eq!(ftyp_kind(b"\0\0\0\x18ftypM4A \0\0\0\0"), Format::Opaque);
        assert_eq!(
            container(b"\0\0\0\x18ftypqt  \0\0\0\0"),
            "QuickTime (brand qt)"
        );
        assert_eq!(
            container(b"\x1A\x45\xDF\xA3\x01\x00\x42\x82\x84webm"),
            "WebM"
        );
    }

    #[test]
    fn the_hex_head_fits_the_width_and_respects_protection() {
        let bytes: Vec<u8> = (0..=255).collect();
        let mut body = Body::new(Canvas::new(80, true, false));
        hex_head(&mut body, &bytes, false);
        let mut notes = Vec::new();
        let lines = body.finish(&mut notes);
        assert_eq!(lines.len(), 5, "4 rows of 16 and the bound");
        assert!(notes.is_empty(), "{notes:?}");
        let first: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.starts_with("00000000  00 01 02"), "{first}");
        let mut narrow = Body::new(Canvas::new(44, true, false));
        hex_head(&mut narrow, &bytes, false);
        assert_eq!(narrow.len(), 9, "8 rows of 8 and the bound");
        let mut hidden = Body::new(Canvas::new(80, true, false));
        hex_head(&mut hidden, b"secret bytes", true);
        assert_eq!(hidden.len(), 1);
    }
}
