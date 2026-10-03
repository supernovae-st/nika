// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One artifact, shown honestly: first its state (null, missing, empty,
//! unknown, or bytes), then its facts (format, size, what was declared,
//! where it comes from, the recorded digest), then its body through the
//! viewer its classification names, every decoder bounded.

use nika_display::theme::Role;

use super::cells::{self, Row, Sheet, paint};
use super::classify::{self, Format};
use super::{
    Availability, Canvas, Content, Meta, Note, Rendered, data, diff, json, markdown, mask, media,
    opaque, source, table, text,
};

/// The byte-order mark a JSON text may start with.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// The facts every artifact carries after its format: size, producer,
/// provenance, digest.
fn facts(sheet: &mut Sheet, meta: &Meta, held: Option<usize>) {
    let ascii = sheet.body.canvas().ascii;
    let clean = |text: &str| cells::clean(text, ascii).0;
    match (held, meta.size_bytes) {
        (Some(held), Some(declared)) if cells::wide(held) < declared => sheet.facts.push(format!(
            "{} held of {}",
            cells::size_words(held),
            cells::size_words_u64(declared)
        )),
        (Some(held), _) => sheet.facts.push(cells::size_words(held)),
        (None, Some(declared)) => sheet
            .facts
            .push(format!("{} declared", cells::size_words_u64(declared))),
        (None, None) => {}
    }
    if let Some(producer) = &meta.producer {
        sheet.facts.push(format!("made by {}", clean(producer)));
    }
    if let Some(provenance) = &meta.provenance {
        sheet.facts.push(clean(provenance));
    }
    if let Some(digest) = &meta.sha256 {
        let short: String = clean(digest).chars().take(12).collect();
        sheet.facts.push(format!("sha256 {short} (recorded)"));
    }
    if meta.availability == Availability::Missing && held.is_some() {
        sheet
            .facts
            .push("missing now: these are the bytes the caller kept".to_owned());
    }
}

/// An object shown by its state alone: null, empty, missing, changed,
/// present but not loaded, or unknown. Four of them never read alike.
fn state(content: Content<'_>, meta: &Meta, canvas: Canvas, title: String) -> Rendered {
    let declared = classify::declared_format(meta);
    let mut sheet = Sheet::new(canvas, declared.unwrap_or(Format::Opaque));
    let (word, sentence, tone) = match (content, meta.availability) {
        (Content::Null, _) => (
            "null",
            "the value is null: not empty, not missing",
            Role::Dim,
        ),
        (Content::Bytes(_) | Content::Text(_), _) => {
            ("empty", "the object exists and holds no byte", Role::Dim)
        }
        (Content::Unread, Availability::Missing) => (
            "missing",
            "nothing is at the recorded place now",
            Role::Warn,
        ),
        (Content::Unread, Availability::Changed) => (
            "changed",
            "something is there and differs from the record; its bytes are not loaded here",
            Role::Warn,
        ),
        (Content::Unread, Availability::Present) => (
            "not loaded",
            "the object is there; its bytes are not loaded here",
            Role::Dim,
        ),
        (Content::Unread, Availability::Unknown) => (
            "unknown",
            "no byte was observed and nobody looked",
            Role::Dim,
        ),
    };
    sheet.facts.push(word.to_owned());
    if let Some(format) = declared {
        sheet.facts.push(format!("declared {}", format.label()));
    }
    facts(
        &mut sheet,
        meta,
        matches!(content, Content::Bytes(_) | Content::Text(_)).then_some(0),
    );
    let dot = cells::sep(canvas.ascii);
    sheet.body.wrap(
        &[paint(format!("{word}{dot}{sentence}"), tone, canvas.color)],
        &[],
        &[],
        false,
    );
    let mut declared_rows = Vec::new();
    if let (Some(width), Some(height)) = (meta.width, meta.height) {
        declared_rows.push(Row::new(
            "declared",
            media::dimensions(width, height, canvas.ascii),
        ));
    }
    if let Some(ms) = meta.duration_ms {
        declared_rows.push(Row::new("declared", media::duration_words(ms)));
    }
    cells::rows(&mut sheet.body, &declared_rows);
    if meta.availability == Availability::Changed {
        sheet.notes.push(Note::Changed);
    }
    sheet.finish(title)
}

/// Whether a CSV object is tab-separated by its declaration.
fn tab_separated(meta: &Meta) -> bool {
    let named = |s: &str| s.trim_start_matches('.').eq_ignore_ascii_case("tsv");
    meta.format.as_deref().is_some_and(named)
        || meta.name.rsplit_once('.').is_some_and(|(_, e)| named(e))
        || meta
            .mime_type
            .as_deref()
            .is_some_and(|m| m.contains("tab-separated"))
}

/// The body of a text format, decoded within the bounds.
fn text_body(sheet: &mut Sheet, bytes: &[u8], total: usize, meta: &Meta) {
    let canvas = sheet.body.canvas();
    let decoded = cells::decode(bytes, canvas.limits.bytes, canvas.ascii);
    if decoded.read < total {
        sheet.notes.push(Note::BytesCut {
            read: decoded.read,
            total,
        });
    }
    if decoded.invalid > 0 {
        sheet.notes.push(Note::InvalidUtf8 {
            sequences: decoded.invalid,
        });
    }
    let protected = meta.protected;
    match sheet.format {
        Format::Json | Format::JsonPatch | Format::MergePatch => {
            let window = &bytes[..decoded.read];
            let window = window.strip_prefix(BOM).unwrap_or(window);
            if let Err(at) = json::show(sheet, window, decoded.read < total, protected) {
                sheet.notes.push(Note::Fallback {
                    to: Format::Text,
                    why: "the bytes are not valid JSON",
                });
                sheet.facts.push(format!("the JSON breaks at byte {at}"));
                sheet.format = Format::Text;
                text::prose(sheet, &decoded.text, protected);
            }
        }
        Format::Yaml => data::yaml(sheet, &decoded.text, protected),
        Format::Toml => data::toml(sheet, &decoded.text, protected),
        Format::Csv => table::show(sheet, &decoded.text, tab_separated(meta), protected),
        Format::Markdown => markdown::show(sheet, &decoded.text, protected),
        Format::Diff => diff::show(sheet, &decoded.text, protected),
        Format::Workflow => source::show(sheet, &decoded.text, protected),
        Format::Code | Format::Html => text::code(sheet, &decoded.text, protected),
        _ => text::prose(sheet, &decoded.text, protected),
    }
}

/// Show one artifact.
pub(crate) fn show(content: Content<'_>, meta: &Meta, canvas: Canvas) -> Rendered {
    let (mut title, _) = cells::clean(&meta.name, canvas.ascii);
    if title.is_empty() {
        "(unnamed)".clone_into(&mut title);
    }
    let bytes: &[u8] = match content {
        Content::Bytes(bytes) => bytes,
        Content::Text(text) => text.as_bytes(),
        Content::Null | Content::Unread => return state(content, meta, canvas, title),
    };
    if bytes.is_empty() {
        return state(content, meta, canvas, title);
    }
    let window = &bytes[..bytes.len().min(canvas.limits.bytes)];
    let (format, mismatch) = classify::judge(window, meta);
    let mut sheet = Sheet::new(canvas, format);
    sheet.notes.extend(mismatch);
    if meta.availability == Availability::Changed {
        sheet.notes.push(Note::Changed);
    }
    let total = usize::try_from(meta.size_bytes.unwrap_or(0))
        .unwrap_or(usize::MAX)
        .max(bytes.len());
    sheet.facts.push(format.label().to_owned());
    facts(&mut sheet, meta, Some(bytes.len()));
    let declared = meta.width.zip(meta.height);
    let mut rows = match format {
        Format::Png | Format::Jpeg | Format::Gif | Format::Webp => {
            media::image_rows(format, window, declared, canvas.ascii)
        }
        Format::Mp3 | Format::Wav => media::sound_rows(format, window, total, meta.duration_ms),
        Format::Svg => media::svg_rows(window),
        Format::Pdf
        | Format::Docx
        | Format::Xlsx
        | Format::Pptx
        | Format::Zip
        | Format::Video
        | Format::Opaque => opaque::rows(format, window),
        _ => {
            text_body(&mut sheet, bytes, total, meta);
            return sheet.finish(title);
        }
    };
    if meta.protected {
        let mut hidden = 0;
        for row in &mut rows {
            let (value, count) = mask::text(&row.value, canvas.ascii);
            row.value = value;
            hidden += count;
        }
        if hidden > 0 {
            sheet.notes.push(Note::Masked { count: hidden });
        }
    }
    cells::rows(&mut sheet.body, &rows);
    let undecoded = matches!(
        format,
        Format::Pdf
            | Format::Docx
            | Format::Xlsx
            | Format::Pptx
            | Format::Zip
            | Format::Video
            | Format::Opaque
    );
    if undecoded {
        sheet.body.push(Vec::new(), false);
        opaque::hex_head(&mut sheet.body, window, meta.protected);
    }
    sheet.finish(title)
}
