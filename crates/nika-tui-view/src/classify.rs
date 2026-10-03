// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The format registry: what an object is, by its declared media type,
//! then its declared format word or extension, then its magic bytes, and
//! whether the bytes agree with the declaration.
//!
//! A declaration is a claim, never a proof: a binary format is verified by
//! its signature, and a declared text format that turns out to be binary is
//! said so. An extension is not a verified format; the bytes decide when
//! they carry a signature, and the note names the disagreement.

use super::{Meta, Note, media, opaque};

/// What a viewer takes an object for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// A Nika workflow (`.nika`).
    Workflow,
    /// JSON.
    Json,
    /// A JSON Patch (RFC 6902): a list of operations.
    JsonPatch,
    /// A JSON merge patch (RFC 7396): `null` removes a key.
    MergePatch,
    /// YAML.
    Yaml,
    /// TOML.
    Toml,
    /// CSV or TSV.
    Csv,
    /// Markdown.
    Markdown,
    /// Plain text.
    Text,
    /// Source code or configuration.
    Code,
    /// HTML.
    Html,
    /// A unified diff or patch.
    Diff,
    /// An SVG vector image (a chart among them).
    Svg,
    /// A PNG image.
    Png,
    /// A JPEG image.
    Jpeg,
    /// A GIF image.
    Gif,
    /// A `WebP` image.
    Webp,
    /// MP3 audio.
    Mp3,
    /// WAV audio.
    Wav,
    /// A PDF document.
    Pdf,
    /// A Word document (DOCX).
    Docx,
    /// An Excel workbook (XLSX).
    Xlsx,
    /// A `PowerPoint` deck (PPTX).
    Pptx,
    /// A ZIP archive.
    Zip,
    /// A video container (MP4, `QuickTime`, `WebM`, Matroska, AVI).
    Video,
    /// Bytes no viewer here decodes.
    Opaque,
}

impl Format {
    /// Every format, in the order a legend lists them.
    pub const ALL: [Self; 26] = [
        Self::Workflow,
        Self::Json,
        Self::JsonPatch,
        Self::MergePatch,
        Self::Yaml,
        Self::Toml,
        Self::Csv,
        Self::Markdown,
        Self::Text,
        Self::Code,
        Self::Html,
        Self::Diff,
        Self::Svg,
        Self::Png,
        Self::Jpeg,
        Self::Gif,
        Self::Webp,
        Self::Mp3,
        Self::Wav,
        Self::Pdf,
        Self::Docx,
        Self::Xlsx,
        Self::Pptx,
        Self::Zip,
        Self::Video,
        Self::Opaque,
    ];

    /// The format in words, as a fact under a title.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Workflow => "Nika workflow",
            Self::Json => "JSON",
            Self::JsonPatch => "JSON Patch",
            Self::MergePatch => "JSON merge patch",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Csv => "CSV table",
            Self::Markdown => "Markdown",
            Self::Text => "text",
            Self::Code => "code",
            Self::Html => "HTML",
            Self::Diff => "diff",
            Self::Svg => "SVG vector image",
            Self::Png => "PNG image",
            Self::Jpeg => "JPEG image",
            Self::Gif => "GIF image",
            Self::Webp => "WebP image",
            Self::Mp3 => "MP3 audio",
            Self::Wav => "WAV audio",
            Self::Pdf => "PDF document",
            Self::Docx => "Word document",
            Self::Xlsx => "Excel workbook",
            Self::Pptx => "PowerPoint deck",
            Self::Zip => "ZIP archive",
            Self::Video => "video",
            Self::Opaque => "opaque bytes",
        }
    }

    /// Whether the format is binary: its declaration is verified by a
    /// signature in the bytes.
    #[must_use]
    pub const fn is_binary(self) -> bool {
        matches!(
            self,
            Self::Png
                | Self::Jpeg
                | Self::Gif
                | Self::Webp
                | Self::Mp3
                | Self::Wav
                | Self::Pdf
                | Self::Docx
                | Self::Xlsx
                | Self::Pptx
                | Self::Zip
                | Self::Video
                | Self::Opaque
        )
    }
}

/// The format a declared media type names; `None` when it names none (or
/// only `application/octet-stream`, which declares nothing).
fn from_mime(mime: &str) -> Option<Format> {
    let essence = mime.split(';').next().unwrap_or_default().trim();
    let essence = essence.to_ascii_lowercase();
    let office = "application/vnd.openxmlformats-officedocument.";
    Some(match essence.as_str() {
        "application/json" | "text/json" => Format::Json,
        "application/json-patch+json" => Format::JsonPatch,
        "application/merge-patch+json" => Format::MergePatch,
        "application/yaml" | "application/x-yaml" | "text/yaml" | "text/x-yaml" => Format::Yaml,
        "application/toml" | "text/toml" => Format::Toml,
        "text/csv" | "text/tab-separated-values" => Format::Csv,
        "text/markdown" | "text/x-markdown" => Format::Markdown,
        "text/plain" => Format::Text,
        "text/html" | "application/xhtml+xml" => Format::Html,
        "text/x-diff" | "text/x-patch" | "application/x-patch" | "application/x-diff" => {
            Format::Diff
        }
        "image/svg+xml" => Format::Svg,
        "image/png" => Format::Png,
        "image/jpeg" | "image/jpg" => Format::Jpeg,
        "image/gif" => Format::Gif,
        "image/webp" => Format::Webp,
        "audio/mpeg" | "audio/mp3" => Format::Mp3,
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => Format::Wav,
        "application/pdf" => Format::Pdf,
        "application/zip" => Format::Zip,
        m if m.strip_prefix(office) == Some("wordprocessingml.document") => Format::Docx,
        m if m.strip_prefix(office) == Some("spreadsheetml.sheet") => Format::Xlsx,
        m if m.strip_prefix(office) == Some("presentationml.presentation") => Format::Pptx,
        m if m.starts_with("video/") => Format::Video,
        m if m.ends_with("+json") => Format::Json,
        m if m.ends_with("+xml") || m == "application/xml" || m == "text/xml" => Format::Code,
        m if m.starts_with("text/") => Format::Text,
        _ => return None,
    })
}

/// The format a format word or an extension names (case-insensitive,
/// leading dot ignored).
fn from_word(word: &str) -> Option<Format> {
    let word = word.trim().trim_start_matches('.').to_ascii_lowercase();
    Some(match word.as_str() {
        "nika" => Format::Workflow,
        "json" | "geojson" => Format::Json,
        "yaml" | "yml" => Format::Yaml,
        "toml" => Format::Toml,
        "csv" | "tsv" => Format::Csv,
        "md" | "markdown" | "mdx" => Format::Markdown,
        "txt" | "text" | "log" | "ansi" => Format::Text,
        "html" | "htm" | "xhtml" => Format::Html,
        "diff" | "patch" => Format::Diff,
        "svg" => Format::Svg,
        "png" => Format::Png,
        "jpg" | "jpeg" => Format::Jpeg,
        "gif" => Format::Gif,
        "webp" => Format::Webp,
        "mp3" => Format::Mp3,
        "wav" | "wave" => Format::Wav,
        "pdf" => Format::Pdf,
        "docx" => Format::Docx,
        "xlsx" => Format::Xlsx,
        "pptx" => Format::Pptx,
        "zip" => Format::Zip,
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" => Format::Video,
        "rs" | "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "go" | "rb" | "java" | "kt" | "c"
        | "h" | "cc" | "cpp" | "hpp" | "cs" | "swift" | "sh" | "bash" | "zsh" | "fish" | "sql"
        | "css" | "scss" | "xml" | "jq" | "lua" | "php" | "pl" | "r" | "scala" | "dart" | "vue"
        | "svelte" | "ini" | "cfg" | "conf" | "env" | "proto" | "graphql" | "jsonl" | "ndjson"
        | "jsonc" | "json5" | "dockerfile" | "makefile" => Format::Code,
        _ => return None,
    })
}

/// The format a name's extension names.
fn from_name(name: &str) -> Option<Format> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match file.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => from_word(extension),
        _ => from_word(file).filter(|f| *f == Format::Code),
    }
}

/// The declaration, if any, as the caller made it: the format and the
/// words that declared it.
fn declared(meta: &Meta) -> Option<(Format, String)> {
    if let Some(mime) = &meta.mime_type
        && let Some(format) = from_mime(mime)
    {
        return Some((format, mime.clone()));
    }
    if let Some(word) = &meta.format
        && let Some(format) = from_word(word)
    {
        return Some((format, word.clone()));
    }
    let format = from_name(&meta.name)?;
    let extension = meta.name.rsplit_once('.').map_or("", |(_, e)| e);
    Some((format, format!("the extension .{extension}")))
}

/// A plain JSON declaration refined by what its producer makes: the
/// operations `json_diff` returns are a JSON Patch whatever their name.
fn refined(want: Format, meta: &Meta) -> Format {
    let made = meta.producer.as_deref().and_then(for_builtin);
    match (want, made) {
        (Format::Json, Some(Format::JsonPatch)) => Format::JsonPatch,
        _ => want,
    }
}

/// The format the facts declare (type, format word, extension) or the
/// producer usually makes, without looking at any byte.
pub(crate) fn declared_format(meta: &Meta) -> Option<Format> {
    declared(meta)
        .map(|(format, _)| refined(format, meta))
        .or_else(|| meta.producer.as_deref().and_then(for_builtin))
}

/// The format a signature in the first bytes proves, if any.
pub(crate) fn sniff(head: &[u8]) -> Option<Format> {
    let riff = |kind: &[u8]| head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == kind;
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if head.starts_with(b"\xFF\xD8\xFF") {
        Some(Format::Jpeg)
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if riff(b"WEBP") {
        Some(Format::Webp)
    } else if riff(b"WAVE") {
        Some(Format::Wav)
    } else if riff(b"AVI ") || head.starts_with(b"\x1A\x45\xDF\xA3") {
        Some(Format::Video)
    } else if head.len() >= 12 && &head[4..8] == b"ftyp" {
        Some(opaque::ftyp_kind(head))
    } else if head.starts_with(b"%PDF-") {
        Some(Format::Pdf)
    } else if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        Some(opaque::zip_kind(head))
    } else if head.starts_with(b"ID3") || media::mp3_frame(head).is_some() {
        Some(Format::Mp3)
    } else if media::svg_root(head).is_some() {
        Some(Format::Svg)
    } else {
        None
    }
}

/// Whether `head` reads as binary rather than text: a NUL byte, or more
/// than one byte in ten that is neither valid UTF-8 nor text layout.
pub(crate) fn looks_binary(head: &[u8]) -> bool {
    let window = &head[..head.len().min(8192)];
    if window.contains(&0) {
        return true;
    }
    let mut odd = 0usize;
    for chunk in window.utf8_chunks() {
        odd += chunk.invalid().len();
        odd += chunk
            .valid()
            .bytes()
            .filter(|b| b.is_ascii_control() && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0c | 0x1b))
            .count();
    }
    odd * 10 > window.len()
}

/// The text formats a first glance at the words can name.
fn glance(head: &[u8]) -> Option<Format> {
    let text = String::from_utf8_lossy(&head[..head.len().min(4096)]);
    let start = text.trim_start_matches('\u{FEFF}').trim_start();
    let lower = start.get(..16).unwrap_or(start).to_ascii_lowercase();
    if start.starts_with('{') || start.starts_with('[') {
        return Some(Format::Json);
    }
    if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        return Some(Format::Html);
    }
    let mut lines = start.lines();
    let first = lines.next().unwrap_or_default();
    if first.starts_with("diff --git ")
        || (first.starts_with("--- ") && lines.next().is_some_and(|l| l.starts_with("+++ ")))
    {
        return Some(Format::Diff);
    }
    let program = start
        .lines()
        .map(str::trim_end)
        .find(|l| !l.is_empty() && !l.starts_with('#'));
    if program.is_some_and(|l| l.starts_with("nika:")) {
        return Some(Format::Workflow);
    }
    None
}

/// Whether bytes found as `found` honour a declaration of `want`.
fn agrees(want: Format, found: Format) -> bool {
    let office = |f: Format| matches!(f, Format::Docx | Format::Xlsx | Format::Pptx);
    want == found
        || (office(want) && found == Format::Zip)
        || (want == Format::Zip && office(found))
}

/// Classify an object from its first bytes and its facts: the format a
/// viewer shows it as, and the disagreement between the declaration and
/// the bytes when there is one.
pub(crate) fn judge(head: &[u8], meta: &Meta) -> (Format, Option<Note>) {
    let sniffed = sniff(head);
    let mismatch = |said: String, found: Format| {
        Some(Note::Mismatch {
            declared: said,
            found,
        })
    };
    match declared(meta) {
        Some((want, said)) if want.is_binary() => match sniffed {
            Some(found) if agrees(want, found) => {
                (if found == Format::Zip { want } else { found }, None)
            }
            Some(found) => (found, mismatch(said, found)),
            None if head.is_empty() => (want, None),
            None => (Format::Opaque, mismatch(said, Format::Opaque)),
        },
        Some((want, said)) => match sniffed {
            Some(found) if found.is_binary() => (found, mismatch(said, found)),
            _ if looks_binary(head) => (Format::Opaque, mismatch(said, Format::Opaque)),
            None if want == Format::Svg && !head.is_empty() => {
                (Format::Text, mismatch(said, Format::Text))
            }
            _ => (refined(want, meta), None),
        },
        None => match sniffed {
            Some(found) => (found, None),
            None if looks_binary(head) => (Format::Opaque, None),
            None => (by_words(head, meta), None),
        },
    }
}

/// An undeclared text object's format: what its first words show, else
/// what its producer usually makes (a text format only: a media format
/// is proven by its signature or not at all).
fn by_words(head: &[u8], meta: &Meta) -> Format {
    let hint = meta
        .producer
        .as_deref()
        .and_then(for_builtin)
        .filter(|f| !f.is_binary() && *f != Format::Svg);
    match (glance(head), hint) {
        (Some(Format::Json), Some(patch @ (Format::JsonPatch | Format::MergePatch))) => patch,
        (Some(seen), _) => seen,
        (None, Some(hint)) => hint,
        (None, None) => Format::Text,
    }
}

/// The format an object is shown as, by its declared type, its extension,
/// then its magic bytes (`head`: its first bytes, all of them or at least
/// the first few kilobytes).
#[must_use]
pub fn classify(head: &[u8], meta: &Meta) -> Format {
    judge(head, meta).0
}

/// The viewer a builtin's primary result is shown with when its caller
/// declares no type: its structured result (JSON) for most, the file it
/// produces for the media builtins, the operations for `json_diff`.
/// `None` for a name the catalog did not ship when this table was written:
/// the caller then falls back to the structured result, and the
/// exhaustiveness test turns red.
#[must_use]
pub fn for_builtin(name: &str) -> Option<Format> {
    let bare = name.strip_prefix("nika:").unwrap_or(name);
    Some(match bare {
        "chart" => Format::Svg,
        "image_generate" | "image_fx" => Format::Png,
        "tts_generate" => Format::Mp3,
        "json_diff" => Format::JsonPatch,
        "fetch" => Format::Markdown,
        "read" | "write" | "edit" => Format::Text,
        "log" | "emit" | "assert" | "prompt" | "done" | "wait" | "jq" | "json_merge_patch"
        | "validate" | "convert" | "uuid" | "date" | "hash" | "decide" | "glob" | "grep"
        | "notify" | "compose" | "inspect" => Format::Json,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str, mime: Option<&str>) -> Meta {
        let mut meta = Meta::new(name);
        meta.mime_type = mime.map(str::to_owned);
        meta
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x08\x08\x06\0\0\0";

    #[test]
    fn the_declared_type_comes_first_then_the_extension_then_the_bytes() {
        assert_eq!(
            classify(b"a,b\n1,2\n", &meta("x.txt", Some("text/csv"))),
            Format::Csv
        );
        assert_eq!(classify(b"a,b\n1,2\n", &meta("x.csv", None)), Format::Csv);
        assert_eq!(classify(PNG, &meta("blob", None)), Format::Png);
        assert_eq!(classify(b"{\"a\": 1}", &meta("out", None)), Format::Json);
        assert_eq!(
            classify(b"# notes\n", &meta("notes.md", None)),
            Format::Markdown
        );
        assert_eq!(
            classify(b"nika: demo\ntasks: {}\n", &meta("x", None)),
            Format::Workflow
        );
        assert_eq!(classify(b"hello", &meta("x", None)), Format::Text);
        assert_eq!(
            classify(b"\0\x01\x02junk", &meta("x", None)),
            Format::Opaque
        );
    }

    #[test]
    fn a_declaration_the_bytes_contradict_is_named() {
        let (format, note) = judge(PNG, &meta("photo.jpg", None));
        assert_eq!(format, Format::Png);
        assert_eq!(
            note,
            Some(Note::Mismatch {
                declared: "the extension .jpg".to_owned(),
                found: Format::Png
            })
        );
        let (format, note) = judge(b"just words", &meta("x.png", Some("image/png")));
        assert_eq!(format, Format::Opaque);
        assert!(matches!(
            note,
            Some(Note::Mismatch {
                found: Format::Opaque,
                ..
            })
        ));
        let (format, note) = judge(PNG, &meta("x.csv", Some("text/csv")));
        assert_eq!(format, Format::Png);
        assert!(note.is_some(), "a text declaration over PNG bytes");
    }

    #[test]
    fn a_producer_refines_a_plain_json_declaration_only() {
        let ops = b"[{\"op\":\"remove\",\"path\":\"/a\"}]";
        let mut diff = meta("v7-to-v8.json", Some("application/json"));
        diff.producer = Some("nika:json_diff".to_owned());
        assert_eq!(judge(ops, &diff), (Format::JsonPatch, None));
        assert_eq!(declared_format(&diff), Some(Format::JsonPatch));
        let mut named = meta("ops.json", None);
        named.producer = Some("json_diff".to_owned());
        assert_eq!(classify(ops, &named), Format::JsonPatch);
        named.producer = Some("nika:jq".to_owned());
        assert_eq!(classify(ops, &named), Format::Json);
        let mut merge = meta("m.json", Some("application/merge-patch+json"));
        merge.producer = Some("nika:json_diff".to_owned());
        assert_eq!(
            classify(b"{}", &merge),
            Format::MergePatch,
            "only a plain JSON declaration is refined"
        );
        let mut text = meta("ops.txt", None);
        text.producer = Some("nika:json_diff".to_owned());
        assert_eq!(classify(ops, &text), Format::Text);
    }

    #[test]
    fn every_format_has_distinct_words() {
        let mut labels: Vec<&str> = Format::ALL.iter().map(|f| f.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Format::ALL.len());
    }

    #[test]
    fn every_builtin_the_catalog_ships_has_a_viewer() {
        let names = nika_session::guard::builtin_names();
        assert_eq!(names.len(), 28, "the catalog grew or shrank: map it here");
        for name in &names {
            assert!(
                for_builtin(name).is_some(),
                "{name} has no viewer or fallback"
            );
        }
        assert_eq!(for_builtin("nika:chart"), Some(Format::Svg));
        assert_eq!(for_builtin("mcp:server/tool"), None);
    }
}
