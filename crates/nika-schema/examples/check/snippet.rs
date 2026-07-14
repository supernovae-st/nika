// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source-excerpt rendering — rustc-grade diagnostics for workflow YAML.
//!
//! A finding that carries a byte span renders the offending source line
//! with a caret run under the exact tokens, so the operator sees WHERE
//! without opening the file:
//!
//! ```text
//!     ┌─ demo.nika.yaml:9:19
//!     │   after: { extarct: succeeded }
//!     │                ^^^^^^^
//! ```
//!
//! Deterministic + themed (the frame glyphs follow the unicode/ASCII
//! theme; the caret line is painted in the finding's severity role).
//! Alignment honesty: YAML forbids tabs in INDENTATION (parser-enforced)
//! and any tab inside a quoted scalar is normalized to one space in the
//! DISPLAY line, so column = char count holds for tabs; wide glyphs
//! (CJK · emoji) in pre-span content can still drift the caret left —
//! accepted for this surface (rustc-grade needs unicode-width; the real
//! CLI can take that dep).

use std::fmt::Write as _;

use nika_schema::ByteSpan;

use crate::theme::Theme;

/// Render the source excerpt for `span` into `out`, indented to sit
/// under its finding line. No-op when the span is out of bounds
/// (defensive: a span never lies, but a renderer must never panic).
pub(crate) fn render_snippet(
    out: &mut String,
    source: &str,
    file_label: &str,
    span: ByteSpan,
    t: Theme,
) {
    let start = span.start as usize;
    let end = (span.end as usize).max(start);
    // the never-panic contract: bail on out-of-bounds AND on a span that
    // lands mid-UTF-8 (a lying span must not crash the renderer — string
    // slicing panics off char boundaries)
    if start > source.len() || !source.is_char_boundary(start) {
        return;
    }

    // the line containing the span start
    let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |i| line_start + i);
    let line = &source[line_start..line_end];

    // CRLF sources: the trailing `\r` is invisible-but-real — strip it
    // from the DISPLAY (offsets/carets are unaffected: a span never
    // points at the EOL terminator).
    let line = line.strip_suffix('\r').unwrap_or(line);
    // tabs inside quoted scalars: 1 tab = 1 char in our col math but N
    // columns in a terminal — normalize to one space in the display so
    // the caret stays char-exact
    let line = line.replace('\t', " ");
    let line = line.as_str();

    let line_no = source[..start].matches('\n').count() + 1;
    let col = source[line_start..start].chars().count() + 1;

    // caret run: span width clipped to the line, ≥1 caret (a zero-width
    // or end-of-file span still points somewhere) — end snaps BACK to a
    // boundary so a lying end can't panic the slice either
    let mut span_end_in_line = end.clamp(start, line_end);
    while span_end_in_line > start && !source.is_char_boundary(span_end_in_line) {
        span_end_in_line -= 1;
    }
    let caret_pad = " ".repeat(col - 1);
    let carets = "^".repeat(source[start..span_end_in_line].chars().count().max(1));

    let (corner, bar) = if t.unicode_glyphs() {
        ("┌─", "│")
    } else {
        (",-", "|")
    };

    let _ = writeln!(
        out,
        "          {} {}",
        t.dim(corner),
        t.dim(&format!("{file_label}:{line_no}:{col}"))
    );
    let _ = writeln!(out, "          {}   {}", t.dim(bar), line);
    let _ = writeln!(
        out,
        "          {}   {caret_pad}{}",
        t.dim(bar),
        t.err(&carets)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str =
        "nika: v1\nworkflow:\n  id: w\ntasks:\n  a:\n    after: { ghost: succeeded }\n";

    fn snip(span: ByteSpan, unicode: bool) -> String {
        let mut out = String::new();
        render_snippet(
            &mut out,
            SRC,
            "w.nika.yaml",
            span,
            Theme::new(false, unicode),
        );
        out
    }

    #[test]
    fn caret_lands_under_the_exact_token() {
        // span of `ghost` (bytes 48..53 in SRC)
        let ghost = SRC.find("ghost").expect("ghost");
        let s = snip(ByteSpan::new(ghost as u32, (ghost + 5) as u32), true);
        let expected = concat!(
            "          ┌─ w.nika.yaml:6:14\n",
            "          │       after: { ghost: succeeded }\n",
            "          │                ^^^^^\n",
        );
        assert_eq!(s, expected);
    }

    #[test]
    fn ascii_frame_is_pure_ascii() {
        let ghost = SRC.find("ghost").expect("ghost");
        let s = snip(ByteSpan::new(ghost as u32, (ghost + 5) as u32), false);
        assert!(s.is_ascii(), "{s:?}");
        assert!(s.contains(",- w.nika.yaml:6:14"));
        assert!(s.contains("^^^^^"));
    }

    #[test]
    fn out_of_bounds_span_is_a_noop_never_a_panic() {
        let s = snip(ByteSpan::new(10_000, 10_005), true);
        assert!(s.is_empty());
    }

    #[test]
    fn crlf_line_displays_without_the_carriage_return() {
        let src = "nika: v1\r\nworkflow:\r\n  id: w\r\ntasks: [x]\r\n";
        let tasks = src.find("tasks").expect("tasks");
        let mut out = String::new();
        render_snippet(
            &mut out,
            src,
            "w.nika.yaml",
            ByteSpan::new(tasks as u32, (tasks + 5) as u32),
            Theme::new(false, true),
        );
        assert!(!out.contains('\r'), "CR leaked into the display: {out:?}");
        assert!(out.contains("tasks: [x]\n"), "{out:?}");
    }

    #[test]
    fn lying_mid_utf8_span_is_a_noop_never_a_panic() {
        let src = "nika: v1\nworkflow:\n  id: w\u{e9}\u{e9}\ntasks: []\n";
        let inside = src.find('\u{e9}').expect("é") + 1; // mid-é byte
        let s = snip_src(src, ByteSpan::new(inside as u32, (inside + 1) as u32));
        assert!(s.is_empty(), "mid-boundary span must bail: {s:?}");
    }

    #[test]
    fn tab_in_scalar_normalizes_so_the_caret_stays_aligned() {
        let src = "nika: v1\nworkflow: \"a\tb\"\ntasks: [ghost]\n";
        let ghost = src.find("ghost").expect("ghost");
        let s = snip_src(src, ByteSpan::new(ghost as u32, (ghost + 5) as u32));
        assert!(!s.contains('\t'), "tab leaked into display: {s:?}");
    }

    fn snip_src(src: &str, span: ByteSpan) -> String {
        let mut out = String::new();
        render_snippet(&mut out, src, "w.nika.yaml", span, Theme::new(false, true));
        out
    }

    #[test]
    fn zero_width_span_still_draws_one_caret() {
        let s = snip(ByteSpan::new(0, 0), true);
        assert!(s.contains('^'));
    }
}
