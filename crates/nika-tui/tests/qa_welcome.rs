// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a native reception proof fails when it cannot read its screen"
)]
//! Reception proofs of the welcome's brand on a real PTY, through
//! `nika-tui-proto --workspace` over the hermetic demo: the butterfly wears
//! the brand's ice where `COLORTERM` proves 24-bit colour, what a reveal
//! frame has just lit glows until the mark settles, a 256- or 16-colour
//! palette gives its nearest slots, and nothing is hued without colour. The
//! wordmark `nika.` sits under the mark and opens the header, its dot in the
//! exec hue, and the welcome keeps every word it said. They read the stream's
//! colours and the screen's cells; they qualify no Session, provider or model.

mod qa_support;

use std::time::Duration;

use nika_tui::visual::logomark::{Glyphs, REVEAL_AT, Size, WORDMARK};
use nika_tui::workspace::geometry::Geometry;
use qa_support::vt::Screen;
use qa_support::{FREE, FREE_HINT, Term, assert_restored, exit_code};
use ratatui::layout::Rect;

/// The brand's tones as the stream sets them, at rest and newly lit.
struct Tones {
    ice: &'static str,
    glow: &'static str,
}

/// 24-bit colour: the tokens' own hues.
const RGB: Tones = Tones {
    ice: "38;2;159;208;255",
    glow: "38;2;207;230;255",
};
/// The nearest xterm-256 indices.
const XTERM_256: Tones = Tones {
    ice: "38;5;153",
    glow: "38;5;189",
};
/// The 16 slots: bright cyan at rest, bright white newly lit.
const ANSI_16: Tones = Tones {
    ice: "38;5;14",
    glow: "38;5;15",
};

const WIDE: (u16, u16) = (120, 40);

/// `nika-tui-proto --workspace` with `args` on a `size` PTY.
fn welcome(args: &[&str], (cols, rows): (u16, u16), env: &[(&str, &str)]) -> Term {
    let mut all = vec!["--workspace"];
    all.extend_from_slice(args);
    Term::proto_with(&all, cols, rows, env)
}

/// The preview's cells, row by row (none below the workspace's minimum).
fn preview(screen: &Screen) -> Vec<String> {
    let (cols, rows) = screen.size();
    let size = Rect::new(
        0,
        0,
        u16::try_from(cols).expect("cols"),
        u16::try_from(rows).expect("rows"),
    );
    let Some(object) = Geometry::of(size, false).map(|geometry| geometry.object) else {
        return Vec::new();
    };
    screen
        .lines()
        .into_iter()
        .skip(usize::from(object.y))
        .take(usize::from(object.height))
        .map(|row| {
            row.chars()
                .skip(usize::from(object.x))
                .take(usize::from(object.width))
                .collect()
        })
        .collect()
}

/// Where reveal frame `index` of some size, in either glyph set, stands whole
/// in the preview (each row compared without its margins): that size, its
/// glyphs and the preview row it starts on.
fn frame_in(screen: &Screen, index: usize) -> Option<(Size, Glyphs, usize)> {
    let cells = preview(screen);
    let renditions = Size::ALL
        .into_iter()
        .rev()
        .flat_map(|size| [(size, Glyphs::Braille), (size, Glyphs::Ramp)]);
    renditions.into_iter().find_map(|(size, glyphs)| {
        let frame = size.frame(glyphs, index);
        cells
            .windows(frame.len())
            .position(|rows| {
                rows.iter()
                    .zip(&frame)
                    .all(|(row, mark)| row.trim() == mark.trim())
            })
            .map(|top| (size, glyphs, top))
    })
}

/// Whether reveal frame `index` of some size stands whole in the preview.
fn shows_frame(screen: &Screen, index: usize) -> bool {
    frame_in(screen, index).is_some()
}

/// The last frame, the settled mark.
const SETTLED: usize = REVEAL_AT.len() - 1;

/// Wait, within the harness's usual bound, until the settled welcome stands
/// whole: the settled mark and every fact of the welcome, read only at the
/// end of a frame drawn after the last input or resize (each frame ends
/// hiding the cursor). A frame still arriving in pieces, its mark drawn but
/// its words not yet, cannot satisfy it.
fn wait_settled(term: &mut Term) {
    term.wait_workspace_frame("the whole settled welcome", |screen| {
        shows_frame(screen, SETTLED) && FACTS.iter().all(|fact| screen.contains(fact))
    });
}

/// Two `Ctrl+C` from the idle prompt: the terminal comes back with 130.
fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

/// Every foreground colour the `SGR` sequences of `raw` set (`38;5;n`,
/// `38;2;r;g;b`, or a 16-slot code), in order.
fn foregrounds(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for body in raw.split("\x1b[").skip(1) {
        let Some(body) = body.split_once('m').map(|(body, _)| body) else {
            continue;
        };
        if !body.bytes().all(|b| b.is_ascii_digit() || b == b';') {
            continue;
        }
        let tokens: Vec<&str> = body.split(';').collect();
        let mut index = 0;
        while let Some(token) = tokens.get(index) {
            let take = match (*token, tokens.get(index + 1)) {
                ("38" | "48", Some(&"5")) => 3,
                ("38" | "48", Some(&"2")) => 5,
                _ => 1,
            };
            let end = (index + take).min(tokens.len());
            let colour = tokens[index..end].join(";");
            if colour.starts_with("38;") || matches!(token.parse(), Ok(30..=37 | 90..=97)) {
                out.push(colour);
            }
            index = end;
        }
    }
    out
}

/// The reveal under `env`: every foreground it set, then those the frame that
/// settled the mark set (the stream is read at that frame's end, where each
/// paint ends hiding the cursor; nothing is painted after it until a key
/// arrives).
fn tones_under(env: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let mut term = welcome(&["--color"], WIDE, env);
    wait_settled(&mut term);
    let raw = term.raw_text();
    let last = raw
        .rsplit("\x1b[?25l")
        .find(|paint| !paint.is_empty())
        .unwrap_or_default();
    let settling = foregrounds(last);
    let all = foregrounds(&raw);
    leave(&mut term);
    (all, settling)
}

fn assert_tones(env: &[(&str, &str)], wanted: &Tones, others: &[&Tones]) {
    let (all, settling) = tones_under(env);
    let has = |set: &[String], tone: &str| set.iter().any(|hue| hue == tone);
    for tone in [wanted.ice, wanted.glow] {
        assert!(has(&all, tone), "{tone} missing: {all:?}");
    }
    for other in others {
        for tone in [other.ice, other.glow] {
            assert!(!has(&all, tone), "{tone} set: {all:?}");
        }
    }
    // The settling frame paints ice and lights nothing more.
    assert!(has(&settling, wanted.ice), "{settling:?}");
    assert!(!has(&settling, wanted.glow), "{settling:?}");
}

#[test]
fn a_24_bit_terminal_reveals_the_mark_in_glow_and_settles_it_in_ice() {
    assert_tones(&[("COLORTERM", "truecolor")], &RGB, &[&XTERM_256, &ANSI_16]);
}

#[test]
fn a_256_colour_terminal_gives_the_nearest_indices() {
    assert_tones(&[], &XTERM_256, &[&RGB]);
}

#[test]
fn a_16_colour_terminal_gives_its_bright_slots() {
    assert_tones(&[("TERM", "xterm")], &ANSI_16, &[&RGB, &XTERM_256]);
}

/// What the welcome says besides the brand, every word of it kept.
const FACTS: [&str; 6] = [
    "Turn an intention into a workflow.",
    "Describe the outcome in the conversation.",
    "Answer questions; inspect the proposed plan.",
    "Save, then Run with the workflow's models.",
    "Try: aggregate-by-key",
    "no project is known to this conversation",
];

/// The exec hue of the wordmark's dot in 24-bit colour.
const DOT_RGB: &str = "38;2;255;122;60";

/// The lockup on `term`'s settled welcome: the mark in `glyphs`, the
/// wordmark alone on the row after the blank row under it, the header
/// opening with it, and every fact of the welcome kept.
fn assert_lockup(term: &Term, glyphs: Glyphs) {
    let (size, drawn, top) = frame_in(&term.screen, SETTLED).expect("the settled mark");
    assert_eq!(drawn, glyphs, "{}", term.dump());
    let cells = preview(&term.screen);
    let under = top + usize::from(size.rows());
    assert_eq!(cells[under].trim(), "", "{}", term.dump());
    assert_eq!(cells[under + 1].trim(), WORDMARK, "{}", term.dump());
    let lines = term.screen.lines();
    assert!(lines[0].starts_with("nika.  "), "{}", lines[0]);
    for fact in FACTS {
        assert!(term.screen.contains(fact), "{fact}\n{}", term.dump());
    }
}

#[test]
fn the_wordmark_locks_up_under_the_mark_and_opens_the_header() {
    let mut term = welcome(&["--color"], WIDE, &[("COLORTERM", "truecolor")]);
    wait_settled(&mut term);
    assert_lockup(&term, Glyphs::Braille);
    let hues = foregrounds(&term.raw_text());
    assert!(hues.iter().any(|hue| hue == DOT_RGB), "{hues:?}");
    leave(&mut term);
    // The ASCII column draws the same characters; without colour, no hue.
    let mut plain = welcome(&["--ascii"], WIDE, &[("NO_COLOR", "1")]);
    wait_settled(&mut plain);
    assert_lockup(&plain, Glyphs::Ramp);
    assert!(plain.screen.hues().is_empty(), "{:?}", plain.screen.hues());
    leave(&mut plain);
}

#[test]
fn without_colour_the_mark_is_drawn_and_nothing_is_hued() {
    let mut term = welcome(&[], WIDE, &[("NO_COLOR", "1")]);
    wait_settled(&mut term);
    assert!(term.screen.hues().is_empty(), "{:?}", term.screen.hues());
    capture(&term, "welcome-120x40-no-color");
    leave(&mut term);
    let mut ascii = welcome(&["--ascii"], WIDE, &[("NO_COLOR", "1")]);
    wait_settled(&mut ascii);
    assert_lockup(&ascii, Glyphs::Ramp);
    capture(&ascii, "welcome-120x40-ascii");
    leave(&mut ascii);
}

/// Keep `term`'s screen as `<label>.txt` in the directory
/// `NIKA_TUI_WELCOME_CAPTURES` names, when it names one.
#[allow(
    clippy::disallowed_methods,
    reason = "a test-only evidence switch, not a secret"
)]
fn capture(term: &Term, label: &str) {
    let Some(dir) = std::env::var_os("NIKA_TUI_WELCOME_CAPTURES") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the capture directory");
    let screen = term.screen.lines().join("\n");
    std::fs::write(dir.join(format!("{label}.txt")), screen + "\n").expect("write the capture");
}

/// Under reduced motion the first whole welcome frame already holds the
/// settled mark, in ice; it stays, and nothing glows (the shell's silence is
/// `welcome_pty`'s proof).
#[test]
fn reduced_motion_draws_the_settled_mark_at_once_in_ice() {
    let args = ["--color", "--reduced-motion"];
    let mut term = welcome(&args, WIDE, &[("COLORTERM", "truecolor")]);
    term.wait_workspace_frame("the first whole welcome", |screen| {
        FACTS.iter().all(|fact| screen.contains(fact))
            && (0..REVEAL_AT.len()).any(|index| shows_frame(screen, index))
    });
    assert!(shows_frame(&term.screen, SETTLED), "{}", term.dump());
    term.settle(Duration::from_millis(2100));
    assert!(shows_frame(&term.screen, SETTLED), "{}", term.dump());
    let hues = foregrounds(&term.raw_text());
    assert!(hues.iter().any(|hue| hue == RGB.ice), "{hues:?}");
    assert!(!hues.iter().any(|hue| hue == RGB.glow), "{hues:?}");
    leave(&mut term);
}

/// At each qualified size the welcome settles whole: the braille mark, the
/// lockup and every fact where the preview holds them. At 80×24 the preview
/// gives its rows to the words (they come before decoration): the wordmark
/// opens them and no mark is drawn.
#[test]
fn the_welcome_settles_whole_at_every_qualified_size() {
    for (cols, rows, expected) in [
        (80, 24, None),
        (120, 40, Some(Size::Board)),
        (180, 48, Some(Size::Board)),
    ] {
        let mut term = welcome(&[], (cols, rows), &[]);
        // Read at a frame's end, as `wait_settled`: never a frame in pieces.
        term.wait_workspace_frame("the whole welcome", |screen| {
            FACTS.iter().all(|fact| screen.contains(fact))
                && expected.is_none_or(|_| shows_frame(screen, SETTLED))
        });
        capture(&term, &format!("welcome-{cols}x{rows}"));
        let drawn = frame_in(&term.screen, SETTLED);
        assert_eq!(
            drawn.map(|(size, glyphs, _)| (size, glyphs)),
            expected.map(|size| (size, Glyphs::Braille)),
            "{cols}x{rows}\n{}",
            term.dump()
        );
        if expected.is_some() {
            assert_lockup(&term, Glyphs::Braille);
        } else {
            let words = preview(&term.screen);
            assert!(
                words.iter().any(|row| row.trim() == WORDMARK),
                "{}",
                term.dump()
            );
        }
        assert_eq!(
            term.screen.beyond(),
            0,
            "{cols}x{rows}: addressed past the edge"
        );
        leave(&mut term);
    }
}

/// After the first exchange the conversation takes the welcome's space; `/welcome` brings the
/// welcome back beside it, its words whole, and sends nothing to the conversation (live gauntlet
/// 5, r08 B16: no way back to the example the welcome offered).
#[test]
fn slash_welcome_brings_the_welcome_back_and_sends_nothing() {
    let args = ["--demo-surface", "working", "--reduced-motion"];
    let mut term = welcome(&args, WIDE, &[]);
    term.wait_workspace_frame("the welcome", |screen| {
        screen.contains("Try: aggregate-by-key")
    });
    term.send("digest my notes\r");
    term.wait_text("received: digest my notes");
    term.wait_workspace_frame("the exchange in the welcome's space", |screen| {
        !screen.contains("Try: aggregate-by-key") && screen.contains("received: digest my notes")
    });
    term.send("/welcome\r");
    term.wait_workspace_frame("the welcome again", |screen| {
        FACTS.iter().take(5).all(|fact| screen.contains(fact))
    });
    assert!(!term.screen.seen("received: /welcome"), "{}", term.dump());
    leave(&mut term);
}

/// Below the workspace's minimum the focus view stands in, with no mark left
/// behind; back at a workspace size the welcome returns, settled and locked
/// up. The stream is read in 64-byte slices, so every wait is judged inside
/// the frames too: only a whole frame satisfies it, whatever the scheduling.
#[test]
fn below_the_minimum_the_welcome_gives_way_and_returns_settled() {
    let mut term = welcome(&[], WIDE, &[]).reading_at_most(64);
    wait_settled(&mut term);
    term.resize(59, 15);
    // The focus view's prompt and, on its last row, its hint: the whole view.
    term.wait_until("the whole focus view", |screen| {
        screen.row_starting(FREE).is_some()
            && screen.contains(FREE_HINT)
            && !screen.text().contains('⣿')
    });
    capture(&term, "welcome-59x15-below-minimum");
    term.resize(WIDE.0, WIDE.1);
    // Only a frame drawn after this resize, read at its end, counts.
    wait_settled(&mut term);
    assert_lockup(&term, Glyphs::Braille);
    capture(&term, "welcome-120x40-returned");
    leave(&mut term);
}
