// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Every format against adversarial inputs: empty, a megabyte on one line,
//! invalid UTF-8, binary junk, truncated headers, deep JSON, escape
//! sequences, secrets. The laws: nothing panics, no line passes the width,
//! no control character reaches a cell, every cut is said, four states stay
//! four, a protected secret never shows, the ASCII column stays ASCII, no
//! hue without colour, and a megabyte renders within a time bound.

#![allow(clippy::expect_used)]

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::{Duration, Instant};

use ratatui::text::Line;

use super::{Availability, Canvas, Content, Format, Limits, Meta, Note, Rendered, artifact, cells};

/// A declared object: its name and media type.
fn meta(name: &str, mime: Option<&str>) -> Meta {
    let mut meta = Meta::new(name);
    meta.mime_type = mime.map(str::to_owned);
    meta
}

/// One declaration per format the registry names.
fn declarations() -> Vec<Meta> {
    vec![
        meta("flow.nika", None),
        meta("data.json", Some("application/json")),
        meta("ops.json", Some("application/json-patch+json")),
        meta("merge.json", Some("application/merge-patch+json")),
        meta("conf.yaml", None),
        meta("conf.toml", None),
        meta("table.csv", None),
        meta("table.tsv", None),
        meta("notes.md", None),
        meta("notes.txt", None),
        meta("main.rs", None),
        meta("page.html", None),
        meta("change.diff", None),
        meta("chart.svg", None),
        meta("cover.png", None),
        meta("cover.jpg", None),
        meta("cover.gif", None),
        meta("cover.webp", None),
        meta("voice.mp3", None),
        meta("voice.wav", None),
        meta("doc.pdf", None),
        meta("doc.docx", None),
        meta("sheet.xlsx", None),
        meta("deck.pptx", None),
        meta("bundle.zip", None),
        meta("clip.mp4", None),
        meta("blob.bin", None),
        meta("unnamed", None),
    ]
}

/// A deterministic stream of junk bytes.
fn junk(len: usize) -> Vec<u8> {
    let mut state: u32 = 0x2545_F491;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect()
}

/// The adversarial inputs, by name.
fn inputs() -> Vec<(&'static str, Vec<u8>)> {
    let megabyte = 1 << 20;
    let mut deep = b"[".repeat(100_000);
    deep.extend(b"]".repeat(100_000));
    let escapes =
        "a\x1b[31mred\x1b[0m \u{202e}bidi\x07bell\r\n\ttab\u{85}next\0nul\n".repeat(2_000);
    vec![
        ("empty", Vec::new()),
        ("one megabyte line", vec![b'a'; megabyte]),
        ("many lines", b"line of text\n".repeat(megabyte / 13)),
        (
            "invalid utf-8",
            [0xC3, 0x28, 0xFF, b'x'].repeat(megabyte / 4),
        ),
        ("binary junk", junk(megabyte)),
        ("deep json", deep),
        ("unbalanced json", b"[".repeat(megabyte)),
        ("escapes", escapes.into_bytes()),
        ("cut png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0".to_vec()),
        ("cut jpeg", b"\xFF\xD8\xFF\xE0\0\x10JFIF".to_vec()),
        ("cut gif", b"GIF89a\x40".to_vec()),
        ("cut webp", b"RIFF\0\0\0\0WEBPVP8 ".to_vec()),
        ("cut wav", b"RIFF\0\0\0\0WAVEfmt \x10\0".to_vec()),
        ("cut mp3", b"ID3\x04\0\0\0\0\x7f\x7f".to_vec()),
        ("cut pdf", b"%PDF-".to_vec()),
        ("cut zip", b"PK\x03\x04\x14\0".to_vec()),
        ("cut mp4", b"\0\0\0\x18ftyp".to_vec()),
    ]
}

fn text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// The laws every rendering keeps, on its body and on its head.
fn laws(rendered: &Rendered, canvas: Canvas, context: &str) {
    assert!(
        rendered.lines.len() <= canvas.limits.lines,
        "{context}: {} lines",
        rendered.lines.len()
    );
    for line in rendered.lines.iter().chain(rendered.head(canvas).iter()) {
        let row = text(line);
        assert!(
            cells::width(&row) <= usize::from(canvas.width),
            "{context}: wider than {}: {row:?}",
            canvas.width
        );
        assert!(
            !row.chars().any(char::is_control),
            "{context}: a control character in {row:?}"
        );
        if !canvas.color {
            assert!(
                line.spans
                    .iter()
                    .all(|s| s.style.fg.is_none() && s.style.bg.is_none()),
                "{context}: a hue without colour"
            );
        }
    }
}

#[test]
fn every_format_survives_every_adversarial_input() {
    let limits = Limits::new(16 * 1024, 120, 1024);
    let canvases = [
        Canvas::new(80, false, true).with_limits(limits),
        Canvas::new(40, true, false).with_limits(limits),
        Canvas::new(1, false, false).with_limits(limits),
    ];
    let inputs = inputs();
    for meta in declarations() {
        for (name, input) in &inputs {
            for canvas in canvases {
                let context = format!("{} / {name} / {} cols", meta.name, canvas.width);
                let rendered = artifact(Content::Bytes(input), &meta, canvas);
                laws(&rendered, canvas, &context);
                if input.len() > limits.bytes
                    && !rendered.format.is_binary()
                    && rendered.format != Format::Svg
                {
                    assert!(
                        rendered
                            .notes
                            .iter()
                            .any(|n| matches!(n, Note::BytesCut { .. })),
                        "{context}: a cut read is not said: {:?}",
                        rendered.notes
                    );
                }
            }
        }
    }
}

/// A megabyte in each format's own shape.
fn megabytes() -> Vec<(Meta, Vec<u8>)> {
    let megabyte = 1 << 20;
    let row = |text: &str| text.repeat(megabyte / text.len()).into_bytes();
    let mut json_line = b"[".to_vec();
    json_line.extend(
        "{\"id\":12345,\"name\":\"a name\",\"ok\":true},"
            .repeat(megabyte / 40)
            .into_bytes(),
    );
    json_line.extend(b"{}]");
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x04\0\0\0\x04\0\x08\x06\0\0\0".to_vec();
    png.extend(junk(megabyte));
    vec![
        (meta("one-line.json", None), json_line),
        (
            meta("pretty.json", None),
            row("{\n  \"key\": [1, 2, 3],\n  \"more\": {\"a\": null}\n}\n"),
        ),
        (meta("deep.json", None), b"[".repeat(megabyte)),
        (
            meta("rows.csv", None),
            row("alpha,1,\"quoted, cell\",2026-09-29\n"),
        ),
        (
            meta("notes.md", None),
            row("# Title\n\n- a **bold** item with `code` and [a link](https://nika.sh)\n"),
        ),
        (
            meta("notes.txt", None),
            row("plain words of a long paragraph that wraps across the width\n"),
        ),
        (meta("single.txt", None), vec![b'x'; megabyte]),
        (
            meta("conf.yaml", None),
            row("key: value # comment\n  - item: ${{ inputs.x }}\n"),
        ),
        (
            meta("conf.toml", None),
            row("[table]\nkey = \"value\" # c\n"),
        ),
        (
            meta("change.diff", None),
            row("@@ -1 +1 @@\n-old line\n+new line\n"),
        ),
        (
            meta("main.rs", None),
            row("fn main() { println!(\"hello\"); }\n"),
        ),
        (
            meta("flow.nika", None),
            row("tasks:\n  t:\n    infer:\n      prompt: \"${{ inputs.x }}\"\n"),
        ),
        (meta("junk.bin", None), junk(megabyte)),
        (meta("big.png", None), png),
    ]
}

#[test]
fn a_megabyte_renders_within_the_time_bound() {
    let bound = Duration::from_secs(2);
    let canvas = Canvas::new(80, false, true);
    let mut receipt = String::new();
    for (meta, bytes) in megabytes() {
        let started = Instant::now();
        let rendered = artifact(Content::Bytes(&bytes), &meta, canvas);
        let took = started.elapsed();
        laws(&rendered, canvas, &meta.name);
        assert!(took < bound, "{} took {took:?}", meta.name);
        let _ = writeln!(
            receipt,
            "view-timing {:<14} {:>8} bytes {:>6} lines {:>9.3} ms {}",
            meta.name,
            bytes.len(),
            rendered.lines.len(),
            took.as_secs_f64() * 1e3,
            rendered.format.label()
        );
    }
    let _ = std::io::stderr().write_all(receipt.as_bytes());
}

#[test]
fn null_missing_empty_and_unknown_never_read_alike() {
    let canvas = Canvas::new(80, true, false);
    let mut missing = meta("report.md", None);
    missing.availability = Availability::Missing;
    let unknown = meta("report.md", None);
    let states = [
        artifact(Content::Null, &unknown, canvas),
        artifact(Content::Bytes(b""), &unknown, canvas),
        artifact(Content::Unread, &missing, canvas),
        artifact(Content::Unread, &unknown, canvas),
    ];
    let words: Vec<&str> = states.iter().map(|r| r.facts[0].as_str()).collect();
    assert_eq!(words, ["null", "empty", "missing", "unknown"]);
    let bodies: Vec<String> = states.iter().map(|r| text(&r.lines[0])).collect();
    for (i, a) in bodies.iter().enumerate() {
        for b in &bodies[i + 1..] {
            assert_ne!(a, b);
        }
    }
    let mut changed = meta("report.md", None);
    changed.availability = Availability::Changed;
    let rendered = artifact(Content::Unread, &changed, canvas);
    assert_eq!(rendered.facts[0], "changed");
    assert!(rendered.notes.contains(&Note::Changed));
    let mut present = meta("report.md", None);
    present.availability = Availability::Present;
    assert_eq!(
        artifact(Content::Unread, &present, canvas).facts[0],
        "not loaded"
    );
    assert_eq!(
        artifact(Content::Null, &meta("x", None), canvas).format,
        Format::Opaque
    );
}

#[test]
fn a_protected_secret_never_reaches_a_cell() {
    let secret = "sk-live-0123456789abcdefXYZ";
    let samples = [
        ("env.txt", format!("API_KEY={secret}\nnote: fine\n")),
        ("conf.yaml", format!("service:\n  token: {secret}\n")),
        ("conf.toml", format!("password = \"{secret}\"\n")),
        (
            "data.json",
            format!("{{\"api_key\":\"{secret}\",\"user\":\"ann\"}}"),
        ),
        ("rows.csv", format!("user,secret\nann,{secret}\n")),
        ("notes.md", format!("Use `Bearer {secret}` here\n")),
        ("change.diff", format!("+token: {secret}\n")),
        ("main.rs", format!("let key = \"{secret}\";\n")),
        (
            "draft.nika",
            format!("nika: draft\nconst:\n  api_key: {secret}\n"),
        ),
        (
            "proposal",
            format!(
                "nika: proposal\ntasks:\n  call:\n    invoke:\n      args:\n        token: {secret}\n"
            ),
        ),
    ];
    for (name, body) in samples {
        let mut protected = meta(name, None);
        protected.protected = true;
        let rendered = artifact(
            Content::Text(&body),
            &protected,
            Canvas::new(120, false, false),
        );
        let all: Vec<String> = rendered.lines.iter().map(text).collect();
        assert!(
            !all.join("\n").contains("0123456789abcdef"),
            "{name} leaked: {all:?}"
        );
        assert!(
            rendered
                .notes
                .iter()
                .any(|n| matches!(n, Note::Masked { .. })),
            "{name}: {:?}",
            rendered.notes
        );
        let open = artifact(
            Content::Text(&body),
            &meta(name, None),
            Canvas::new(120, false, false),
        );
        let shown: Vec<String> = open.lines.iter().map(text).collect();
        assert!(
            shown.join("\n").contains("0123456789abcdef"),
            "{name}: unprotected text stays whole"
        );
    }
    let mut blob = meta("key.bin", None);
    blob.protected = true;
    let bytes = [&[0_u8, 1, 2][..], secret.as_bytes()].concat();
    let rendered = artifact(Content::Bytes(&bytes), &blob, Canvas::new(80, true, false));
    let all: Vec<String> = rendered.lines.iter().map(text).collect();
    assert!(all.iter().any(|l| l.contains("withheld")), "{all:?}");
    assert!(
        !all.join("\n").contains("73 6b"),
        "no hex of the key: {all:?}"
    );
}

#[test]
fn escape_sequences_and_invalid_bytes_are_marked_and_counted() {
    for name in [
        "notes.txt",
        "notes.md",
        "rows.csv",
        "conf.yaml",
        "main.rs",
        "change.diff",
    ] {
        let mut body = b"plain text on a line\n".repeat(10);
        body.extend(b"a\x1b[2Jb\xff\xfe c\x07\n");
        let rendered = artifact(
            Content::Bytes(&body),
            &meta(name, None),
            Canvas::new(80, false, false),
        );
        laws(&rendered, Canvas::new(80, false, false), name);
        assert!(
            rendered
                .notes
                .iter()
                .any(|n| matches!(n, Note::Controls { .. })),
            "{name}: {:?}",
            rendered.notes
        );
        assert!(
            rendered.notes.contains(&Note::InvalidUtf8 { sequences: 2 }),
            "{name}: {:?}",
            rendered.notes
        );
        let all: String = rendered.lines.iter().map(text).collect();
        assert!(
            all.contains('\u{241B}') && all.contains('\u{FFFD}'),
            "{name}: {all}"
        );
    }
}

#[test]
fn a_header_states_what_it_holds_and_nothing_more() {
    let canvas = Canvas::new(80, true, false);
    let rows = |rendered: &Rendered| {
        rendered
            .lines
            .iter()
            .map(text)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x40\0\0\0\x20\x08\x02\0\0\0";
    let mut declared = meta("cover.png", None);
    declared.width = Some(100);
    declared.height = Some(100);
    let shown = rows(&artifact(Content::Bytes(png), &declared, canvas));
    assert!(shown.contains("64 x 32 (stated by the header)"), "{shown}");
    assert!(shown.contains("100 x 100: the header disagrees"), "{shown}");
    assert!(shown.contains("not drawn in the terminal"), "{shown}");
    let cut = rows(&artifact(
        Content::Bytes(&png[..20]),
        &meta("cover.png", None),
        canvas,
    ));
    assert!(
        cut.contains("unknown: the header is cut or unreadable"),
        "{cut}"
    );
    let lying = artifact(
        Content::Bytes(png),
        &meta("photo.jpg", Some("image/jpeg")),
        canvas,
    );
    assert_eq!(lying.format, Format::Png);
    assert!(lying.notes.iter().any(|n| matches!(
        n,
        Note::Mismatch {
            found: Format::Png,
            ..
        }
    )));
    let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
    frame.resize(600, 0);
    let mut voice = meta("voice.mp3", None);
    voice.duration_ms = Some(12_000);
    let sound = rows(&artifact(Content::Bytes(&frame), &voice, canvas));
    assert!(sound.contains("not stated by the header"), "{sound}");
    assert!(sound.contains("declared  12.0 s"), "{sound}");
    let pdf = rows(&artifact(
        Content::Bytes(b"%PDF-1.7\n%\xe2\xe3"),
        &meta("doc.pdf", None),
        canvas,
    ));
    assert!(
        pdf.contains("PDF 1.7") && pdf.contains("00000000  25 50 44 46"),
        "{pdf}"
    );
}

#[test]
fn the_ascii_column_stays_ascii_and_wide_text_stays_within_the_width() {
    let ascii_bodies = [
        ("data.json", "{\"a\": [1, 2, {\"b\": null}], \"c\": \"d\"}"),
        ("rows.csv", "a,b\n1,2\n"),
        ("notes.md", "# T\n\n- item\n> quote\n```\ncode\n```\n---\n"),
        ("change.diff", "--- a\n+++ b\n@@ -1 +1 @@\n-x\n+y\n"),
        ("conf.yaml", "a: 1\n"),
        ("conf.toml", "[t]\na = 1\n"),
        (
            "flow.nika",
            "nika: x\ntasks:\n  t:\n    infer:\n      prompt: hi\n",
        ),
    ];
    for (name, body) in ascii_bodies {
        for width in [1_u16, 7, 20, 80] {
            let canvas = Canvas::new(width, true, false);
            let rendered = artifact(Content::Text(body), &meta(name, None), canvas);
            laws(&rendered, canvas, name);
            let all: Vec<String> = rendered
                .lines
                .iter()
                .chain(rendered.head(canvas).iter())
                .map(text)
                .collect();
            assert!(
                all.iter().all(|l| l.is_ascii()),
                "{name} @ {width}: {all:?}"
            );
        }
    }
    let wide = "日本語のテキスト 👩‍👩‍👧 e\u{301}\u{301} ﷽ tail\n";
    for name in ["notes.txt", "notes.md", "rows.csv", "data.json", "main.rs"] {
        let body = if name == "data.json" {
            format!("[\"{}\"]", wide.trim())
        } else {
            wide.repeat(3)
        };
        for width in 1..=24_u16 {
            let canvas = Canvas::new(width, false, true);
            laws(
                &artifact(Content::Text(&body), &meta(name, None), canvas),
                canvas,
                name,
            );
        }
    }
}

#[test]
fn a_producer_names_the_viewer_of_undeclared_bytes() {
    let mut patch = meta("result", None);
    patch.producer = Some("nika:json_diff".to_owned());
    let rendered = artifact(
        Content::Text("[{\"op\":\"remove\",\"path\":\"/a\"}]"),
        &patch,
        Canvas::new(80, true, false),
    );
    assert_eq!(rendered.format, Format::JsonPatch);
    assert_eq!(text(&rendered.lines[0]), "- remove  /a");
    let mut chart = meta("out", None);
    chart.producer = Some("nika:chart".to_owned());
    let plain_words = artifact(
        Content::Text("not an svg at all"),
        &chart,
        Canvas::new(80, true, false),
    );
    assert_eq!(
        plain_words.format,
        Format::Text,
        "a producer never proves a media format"
    );
}
