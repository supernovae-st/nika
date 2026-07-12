// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Structural chrome — the rail (wizard step grammar), the panel box,
//! the segment bar, the banner, the dither pulse.
//!
//! The vocab module owns WORDS (arrow · hint); this one owns SHAPES. Same
//! two laws as every glyph surface: an ASCII twin for every mark
//! (CI logs and legacy terminals are first-class), and colour through
//! the semantic [`Role`] slots only — the shapes carry structure, the
//! roles carry meaning, nothing here is decorative.
//!
//! The rail is the clack-school step grammar (`◆` a step · `│` its
//! continuation · `└` the close) WITHOUT the raw-mode event loop:
//! answers stay line-based (`ask` in the CLI), so the grammar works
//! over any `BufRead` — a transcript, a test cursor, an ssh session.

use crate::theme::{Role, Theme};

/// Dither pulse frames (low → high density) — the "thinking" suffix on a
/// live sweep line. TTY+animate surfaces only; ASCII theme renders none
/// (CI-stable by construction, same stance as the braille spinner).
pub const PULSE: [char; 3] = ['░', '▒', '▓'];

/// One rail step head: `◆ label` (`+` under ASCII). The step marker is
/// the accent — the conversation's "you are here", one per question.
#[must_use]
pub fn rail_head(theme: Theme, label: &str) -> String {
    let mark = if theme.ascii { "+" } else { "◆" };
    format!(
        "{} {}",
        theme.paint(Role::Accent, mark),
        theme.paint(Role::Strong, label)
    )
}

/// One rail continuation line: `│ text` (`|` under ASCII) — detail under
/// the current step (a resolved value · a note), dim rail + plain text.
#[must_use]
pub fn rail_line(theme: Theme, text: &str) -> String {
    let mark = if theme.ascii { "|" } else { "│" };
    format!("{} {text}", theme.paint(Role::Dim, mark))
}

/// One numbered pick row inside a step: `│  3 label — note`. The number
/// is the answer the human types (line-based · no raw mode), so it
/// renders Strong; the note stays dim metadata.
#[must_use]
pub fn rail_pick(theme: Theme, n: usize, label: &str, note: &str) -> String {
    let sep = if note.is_empty() { "" } else { " — " };
    rail_line(
        theme,
        &format!(
            " {} {label}{sep}{}",
            theme.paint(Role::Strong, &n.to_string()),
            theme.paint(Role::Dim, note)
        ),
    )
}

/// The rail close: `└ text` (`` ` `` under ASCII) — the hand-back line,
/// dim like every ending that isn't a verdict.
#[must_use]
pub fn rail_close(theme: Theme, text: &str) -> String {
    let mark = if theme.ascii { "`" } else { "└" };
    format!("{} {text}", theme.paint(Role::Dim, mark))
}

/// Inner width the panel pads content to — 76 gives the 80-column
/// terminal nobody configures a 2-border + 2-pad fit.
const PANEL_INNER: usize = 72;

/// A rounded panel box around `lines`, with a title on the top border:
/// `╭─ title ─…─╮ · │ line │ · ╰─…─╯` (`+-|` under ASCII). Content wider
/// than the inner width is truncated with an honest `…` (`...` ASCII) —
/// a box that wraps would break the frame arithmetic that makes it a box.
/// Painted lines are the CALLER's job through [`Theme::paint`]; the
/// border itself stays dim structure. Width math counts chars, so pass
/// PRE-PAINT text via `(text, role)` pairs — the panel paints after
/// measuring (ANSI escapes break width arithmetic · theme.rs law).
#[must_use]
pub fn panel(theme: Theme, title: &str, lines: &[(String, Role)]) -> Vec<String> {
    let (tl, tr, bl, br, h, v) = if theme.ascii {
        ('+', '+', '+', '+', '-', '|')
    } else {
        ('╭', '╮', '╰', '╯', '─', '│')
    };
    let inner = PANEL_INNER;
    let mut out = Vec::with_capacity(lines.len() + 2);

    // Top border carries the title: `╭─ title ` then the fill run.
    let head = format!("{h} {title} ");
    let fill = inner.saturating_sub(head.chars().count());
    out.push(theme.paint(
        Role::Dim,
        &format!("{tl}{head}{}{tr}", h.to_string().repeat(fill)),
    ));

    for (text, role) in lines {
        let clipped = clip(text, inner.saturating_sub(2), theme.ascii);
        let pad = inner
            .saturating_sub(2)
            .saturating_sub(clipped.chars().count());
        out.push(format!(
            "{} {}{} {}",
            theme.paint(Role::Dim, &v.to_string()),
            theme.paint(*role, &clipped),
            " ".repeat(pad),
            theme.paint(Role::Dim, &v.to_string()),
        ));
    }

    out.push(theme.paint(
        Role::Dim,
        &format!("{bl}{}{br}", h.to_string().repeat(inner)),
    ));
    out
}

/// Truncate to `max` chars with the honest ellipsis when clipped.
fn clip(text: &str, max: usize, ascii: bool) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let ell = if ascii { "..." } else { "…" };
    let keep = max.saturating_sub(ell.chars().count());
    let mut s: String = text.chars().take(keep).collect();
    s.push_str(ell);
    s
}

/// A segment progress bar: `━━━╸───` (`==>--` ASCII), done painted
/// accent, rest dim. `total == 0` renders an all-done bar (an empty job
/// is a finished job, not a division). The half-cell `╸` marks the
/// frontier while work remains — the moving edge reads as motion even
/// on a surface that never animates.
#[must_use]
pub fn bar(theme: Theme, done: usize, total: usize, width: usize) -> String {
    let filled = if total == 0 {
        width
    } else {
        (done.min(total) * width) / total
    };
    let (full, edge, rest) = if theme.ascii {
        ("=", ">", "-")
    } else {
        ("━", "╸", "─")
    };
    let frontier = usize::from(filled < width);
    format!(
        "{}{}{}",
        theme.paint(Role::Accent, &full.repeat(filled)),
        theme.paint(Role::Accent, &edge.repeat(frontier)),
        theme.paint(
            Role::Dim,
            &rest.repeat(width.saturating_sub(filled + frontier))
        ),
    )
}

/// The identity banner — the ONE ceremonial moment (init · welcome).
/// Three lines, no figlet wall: the mark, the claim, the version. Every
/// sober register still gets its text (the banner is words, not art);
/// only the paint differs.
#[must_use]
pub fn banner(theme: Theme, title: &str, tagline: &str, version: &str) -> Vec<String> {
    vec![
        format!(
            "{} {} {}",
            theme.logo(),
            theme.paint(Role::Strong, title),
            theme.paint(Role::Dim, &format!("v{version}")),
        ),
        format!("  {}", theme.paint(Role::Dim, tagline)),
        String::new(),
    ]
}

/// The dither-pulse frame for one tick — the "thinking" suffix on a live
/// line (`check workflows/…  ▒`). Empty under ASCII (CI-stable) and when
/// motion is off: a pulse that can't move must not exist.
#[must_use]
pub fn pulse(theme: Theme, tick: usize) -> String {
    if theme.ascii || !theme.animate {
        return String::new();
    }
    theme.paint(Role::Accent, &PULSE[tick % PULSE.len()].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);
    const ASCII: Theme = Theme::new(false, true, false);

    /// Every structural mark carries its ASCII twin — the parity law
    /// vocab.rs pins for words, extended to shapes.
    #[test]
    fn rail_marks_have_ascii_twins() {
        assert!(rail_head(PLAIN, "recipe").starts_with("◆ "));
        assert!(rail_head(ASCII, "recipe").starts_with("+ "));
        assert!(rail_line(PLAIN, "x").starts_with("│ "));
        assert!(rail_line(ASCII, "x").starts_with("| "));
        assert!(rail_close(PLAIN, "done").starts_with("└ "));
        assert!(rail_close(ASCII, "done").starts_with("` "));
    }

    #[test]
    fn rail_pick_carries_number_label_and_note() {
        let row = rail_pick(PLAIN, 2, "agentic", "the 4-pattern curriculum");
        assert!(
            row.contains(" 2 agentic — the 4-pattern curriculum"),
            "{row}"
        );
        // No separator dangles when the note is empty.
        let bare = rail_pick(PLAIN, 1, "starter", "");
        assert!(bare.ends_with("1 starter"), "{bare}");
    }

    /// The panel is a real box: aligned borders, padded rows, title on
    /// the top run — in BOTH glyph themes (frame arithmetic is the test).
    #[test]
    fn panel_borders_align_in_both_themes() {
        for theme in [PLAIN, ASCII] {
            let lines = vec![
                ("short".to_owned(), Role::Strong),
                ("a second line".to_owned(), Role::Dim),
            ];
            let out = panel(theme, "ready", &lines);
            assert_eq!(out.len(), 4);
            let widths: Vec<usize> = out.iter().map(|l| l.chars().count()).collect();
            assert!(
                widths.windows(2).all(|w| w[0] == w[1]),
                "every row same width: {widths:?}\n{out:#?}"
            );
            assert!(out[0].contains("ready"), "title rides the top border");
        }
    }

    /// Overlong content clips with the honest ellipsis instead of
    /// breaking the frame.
    #[test]
    fn panel_clips_overlong_lines() {
        let long = "x".repeat(200);
        let out = panel(PLAIN, "t", &[(long, Role::Dim)]);
        let widths: Vec<usize> = out.iter().map(|l| l.chars().count()).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
        assert!(out[1].contains('…'));
        let ascii_out = panel(ASCII, "t", &[("y".repeat(200), Role::Dim)]);
        assert!(ascii_out[1].contains("..."));
    }

    /// The bar's frontier edge exists exactly while work remains; the
    /// zero-total job renders full (finished, not a division).
    #[test]
    fn bar_geometry_is_stable() {
        // 3/6 over width 12 → 6 full + 1 edge + 5 rest = 12 cells.
        let half = bar(PLAIN, 3, 6, 12);
        assert_eq!(half.chars().count(), 12, "{half}");
        assert!(half.contains('╸'), "frontier while unfinished");
        let done = bar(PLAIN, 6, 6, 12);
        assert_eq!(done.chars().count(), 12);
        assert!(!done.contains('╸'), "no frontier at completion");
        assert_eq!(bar(PLAIN, 0, 0, 8).chars().count(), 8, "empty job = full");
        let ascii = bar(ASCII, 1, 4, 8);
        assert!(ascii.contains('>') && ascii.contains('='), "{ascii}");
    }

    /// Overshoot clamps (done > total must not overflow the width).
    #[test]
    fn bar_clamps_overshoot() {
        assert_eq!(bar(PLAIN, 9, 4, 10).chars().count(), 10);
    }

    #[test]
    fn banner_is_three_lines_no_wall() {
        let b = banner(PLAIN, "nika", "the workflow language for AI", "0.99.0");
        assert_eq!(b.len(), 3);
        assert!(b[0].contains("nika") && b[0].contains("v0.99.0"));
        assert!(b[2].is_empty(), "breathing room, not art");
    }

    /// The pulse obeys the motion law: ASCII and motion-off render
    /// NOTHING (CI-stable), animated unicode cycles the dither ramp.
    #[test]
    fn pulse_only_exists_where_motion_does() {
        assert_eq!(pulse(PLAIN, 0), "", "no animate → no pulse");
        let anim = Theme {
            animate: true,
            ..PLAIN
        };
        assert_eq!(pulse(anim, 0), "░");
        assert_eq!(pulse(anim, 2), "▓");
        let ascii_anim = Theme {
            animate: true,
            ..ASCII
        };
        assert_eq!(pulse(ascii_anim, 1), "", "ASCII never pulses");
    }

    /// Colour off = zero escapes across every chrome shape (the sober
    /// registers keep byte-clean structure).
    #[test]
    fn colour_off_means_zero_escapes_everywhere() {
        let all = [
            rail_head(PLAIN, "x"),
            rail_line(PLAIN, "x"),
            rail_pick(PLAIN, 1, "x", "y"),
            rail_close(PLAIN, "x"),
            bar(PLAIN, 1, 2, 8),
            banner(PLAIN, "t", "g", "1").join("\n"),
            panel(PLAIN, "t", &[("l".to_owned(), Role::Dim)]).join("\n"),
        ];
        for s in all {
            assert!(!s.contains('\x1b'), "{s:?}");
        }
    }
}
