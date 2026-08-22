// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Painted source frames — rustc-grade span rendering for findings that
//! carry a [`ByteSpan`] (spec: « renderers show the offending source
//! line with a caret »).
//!
//! The renderer is `annotate-snippets` (rust-lang org — the crate cargo
//! itself renders diagnostics with), driven entirely through the ONE
//! colour seam: every painted element maps from [`super::theme::Role`]-equivalent
//! styles below, `theme.color` picks plain vs styled, `theme.ascii`
//! picks the decor family. Nothing decorative escapes the mapping
//! (CLI presentation canon).

use annotate_snippets::renderer::DecorStyle;
use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet};
use anstyle::{AnsiColor, Style};
use nika_check::ByteSpan;

use super::theme::Theme;

/// The Role palette expressed as [`anstyle`] styles — the SAME SGR
/// numbers `Role::sgr()` emits (31 red · 33 yellow · 36 cyan · 2 dim ·
/// 1 bold), so the frame and the verdict table read as one voice.
const BAD: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)));
const WARN: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Yellow)));
const ACCENT: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));
const DIM: Style = Style::new().dimmed();
const STRONG: Style = Style::new().bold();

/// Build the theme-faithful renderer: `color` picks styled vs plain,
/// `ascii` picks the decor family, and the styled palette is OUR Role
/// mapping — annotate-snippets' own defaults never reach the terminal.
fn renderer(theme: Theme) -> Renderer {
    let base = if theme.color {
        Renderer::styled()
            .error(BAD)
            .warning(WARN)
            .info(ACCENT)
            .note(ACCENT)
            .help(ACCENT)
            .line_num(DIM)
            .emphasis(STRONG)
            .context(DIM)
    } else {
        Renderer::plain()
    };
    base.decor_style(if theme.ascii {
        DecorStyle::Ascii
    } else {
        DecorStyle::Unicode
    })
}

/// Floor a byte offset onto a UTF-8 char boundary. A parser span that
/// lands inside a decorative glyph (`⛨` · `◇`) must not be handed to
/// annotate-snippets — that crate panics (`byte index N is not a char
/// boundary`) instead of painting the line (persona 8 · 2026-08-22).
fn floor_char(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Widen a point span by one CHAR, not one BYTE. `start + 1` walked
/// into the middle of `⛨` (3 bytes) and the renderer panicked at
/// byte 1 of `⛨permits: {}`.
fn char_end_after(s: &str, start: usize) -> usize {
    s[start..]
        .chars()
        .next()
        .map_or(start, |c| start + c.len_utf8())
}

/// Paint one span-carrying finding as a source frame: the offending
/// line(s) with a caret run under the exact byte range, rustc-style.
/// The caller has already printed the `CONFORM [CODE] message` row + the
/// docs-url line (the grep-stable contract), so the frame is HEADERLESS
/// (`no_name` + empty title): origin `path:line:col`, the source line,
/// the caret run — nothing duplicated.
#[must_use]
pub fn paint_span(source: &str, path: &str, span: ByteSpan, theme: Theme) -> String {
    let start = floor_char(source, span.start as usize);
    let mut end = ceil_char(source, span.end as usize);
    // A point span (start == end — flow-scalar tokens) still deserves a
    // visible caret: widen to one char, clamped to the source end.
    if end <= start {
        end = char_end_after(source, start);
    }
    let group = Group::with_title(Level::ERROR.no_name().primary_title("")).element(
        Snippet::source(source)
            .path(path)
            .fold(true)
            .annotation(AnnotationKind::Primary.span(start..end)),
    );
    renderer(theme).render(&[group])
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "nika: demo\ntasks:\n  a:\n    after:\n      ghost: success\n";

    fn span_of(needle: &str) -> ByteSpan {
        let start = YAML.find(needle).expect("needle present");
        ByteSpan::new(
            u32::try_from(start).expect("small fixture"),
            u32::try_from(start + needle.len()).expect("small fixture"),
        )
    }

    #[test]
    fn frame_paints_the_offending_line_with_a_caret() {
        let out = paint_span(
            YAML,
            "demo.nika.yaml",
            span_of("ghost"),
            Theme::new(false, true, false),
        );
        assert!(
            out.contains("demo.nika.yaml:5:7"),
            "origin carries path:line:col:\n{out}"
        );
        assert!(
            out.contains("ghost: success"),
            "offending line shown:\n{out}"
        );
        assert!(
            out.contains("^^^^^"),
            "caret run under the 5-byte span:\n{out}"
        );
        assert!(
            !out.contains("error"),
            "headerless frame — the CONFORM row above is the header:\n{out}"
        );
    }

    #[test]
    fn point_spans_still_show_one_caret() {
        let start = u32::try_from(YAML.find("ghost").expect("present")).expect("small");
        let out = paint_span(
            YAML,
            "demo.nika.yaml",
            ByteSpan::new(start, start),
            Theme::new(false, true, false),
        );
        assert!(
            out.contains('^'),
            "a caret survives the empty range:\n{out}"
        );
    }

    #[test]
    fn plain_theme_emits_zero_ansi() {
        let out = paint_span(
            YAML,
            "demo.nika.yaml",
            span_of("ghost"),
            Theme::new(false, true, false),
        );
        assert!(
            !out.contains('\u{1b}'),
            "colour OFF emits zero ANSI:\n{out}"
        );
    }

    /// Copied from nika.sh: `⛨permits: {}`. A point span at byte 0
    /// used to widen to `0..1` and panic inside the 3-byte glyph.
    #[test]
    fn a_shield_glyph_point_span_does_not_panic() {
        let yaml = "⛨permits: {}\n";
        let out = paint_span(
            yaml,
            "hello.nika.yaml",
            ByteSpan::new(0, 0),
            Theme::new(false, true, false),
        );
        assert!(
            out.contains('^'),
            "a caret survives a multibyte glyph:\n{out}"
        );
        assert!(out.contains("permits"), "the line is still painted:\n{out}");
    }

    /// Copied from nika.sh: `    ◇infer:`. Span at the diamond (byte 4);
    /// widen-by-one-byte landed at byte 5, inside the glyph.
    #[test]
    fn a_diamond_infer_point_span_does_not_panic() {
        let yaml = "    ◇infer:\n";
        let start = u32::try_from(yaml.find('◇').expect("diamond")).expect("small");
        let out = paint_span(
            yaml,
            "hello.nika.yaml",
            ByteSpan::new(start, start),
            Theme::new(false, true, false),
        );
        assert!(out.contains('^'), "a caret survives the diamond:\n{out}");
    }

    /// A parser that reports a mid-glyph span (byte 1 of `⛨`) must
    /// snap, not crash the operator's `nika check`.
    #[test]
    fn a_mid_glyph_span_snaps_to_a_char_boundary() {
        let yaml = "⛨permits: {}\n";
        let out = paint_span(
            yaml,
            "hello.nika.yaml",
            ByteSpan::new(1, 2),
            Theme::new(false, true, false),
        );
        assert!(out.contains('^'), "mid-char span still paints:\n{out}");
    }
}
