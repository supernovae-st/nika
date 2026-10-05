// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    reason = "a hermetic child on a real PTY proves the shell's timed paints"
)]

//! The real Workspace loop reveals its welcome without input, remains
//! responsive during the wait, and goes silent after the one-shot reveal.

mod qa_support;

use std::time::{Duration, Instant};

use nika_tui::app::{self, Options};
use nika_tui::model::{Beat, Presentation, Script, Waiting};
use nika_tui::visual::logomark::Size;
use nika_tui::workspace::geometry::Geometry;
use qa_support::vt::Screen;
use qa_support::{Term, assert_restored, exit_code, sized};
use ratatui::layout::Rect;

const CHILD: &str = "NIKA_TUI_WELCOME_CHILD";
const COLS: u16 = 160;
const ROWS: u16 = 48;

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "a test-only child switch, not a secret"
)]
fn welcome_child_host() {
    let Ok(mode) = std::env::var(CHILD) else {
        return;
    };
    let mut options = Options::new(Presentation::Workspace);
    options.term = Some("xterm-256color".to_owned());
    options.ascii = true;
    options.reduced_motion = mode == "reduced";
    let conversation = Script::new(vec![Beat::Wait(Waiting::Free)], Vec::new());
    let exit = app::run(conversation, options).expect("the real shell");
    std::process::exit(i32::from(exit.code()));
}

fn welcome(reduced: bool) -> Term {
    let exe = std::env::current_exe().expect("this test executable");
    let mut command = sized(exe.to_str().expect("UTF-8 executable"), COLS, ROWS);
    command
        .args([
            "--exact",
            "welcome_child_host",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, if reduced { "reduced" } else { "motion" });
    Term::spawn(command, COLS, ROWS)
}

/// Read only the preview cells. Each known frame must appear as the complete
/// 48x20 butterfly, not as fragments elsewhere in the screen or ANSI stream.
fn logo_is(screen: &Screen, index: usize) -> bool {
    let object = Geometry::of(Rect::new(0, 0, COLS, ROWS), false)
        .expect("wide workspace")
        .object;
    let rows: Vec<String> = screen
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
        .collect();
    let mark = Size::Board.frame(index);
    rows.windows(mark.len()).any(|rows| {
        rows.iter()
            .zip(&mark)
            .all(|(row, mark)| row.trim() == mark.trim())
    })
}

/// Whether `raw` stops where a frame ends. The renderer ends every frame on
/// its cursor: hidden (`?25l`), or shown (`?25h`) then placed (`CSI r;c H`).
/// A stream that stops anywhere else stopped inside a frame.
fn ends_a_frame(raw: &[u8]) -> bool {
    if raw.ends_with(b"\x1b[?25l") {
        return true;
    }
    let Some(head) = raw.strip_suffix(b"H") else {
        return false;
    };
    let digits = head
        .iter()
        .rev()
        .take_while(|byte| byte.is_ascii_digit() || **byte == b';')
        .count();
    head[..head.len() - digits].ends_with(b"\x1b[?25h\x1b[")
}

/// Pump until the frame that drew the wordmark is whole. The shell writes a
/// frame through a line-buffered stdout in pieces and a PTY hands each
/// piece over as it comes, so the wordmark can be read before the rows
/// after it; those rows belong to the first paint, not to a new one.
fn first_frame_whole(term: &mut Term) {
    term.wait_text("N I K A");
    let deadline = Instant::now() + qa_support::WAIT;
    while !ends_a_frame(term.bytes_since(0)) {
        assert!(
            Instant::now() < deadline,
            "the first frame never ended\n{}",
            term.dump()
        );
        if term.pump() == 0 {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn leave(term: &mut Term) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", term.dump());
    assert_restored(term);
}

#[test]
fn the_real_workspace_reveals_all_frames_without_input_then_stops_painting() {
    let mut term = welcome(false);
    // No keys, mouse events or resize are sent between these five frames.
    for index in 0..5 {
        term.wait_until(&format!("welcome frame {index}"), |screen| {
            logo_is(screen, index)
        });
    }
    // The exact mark is held from 1.4s; pass the 1.8s end before measuring
    // silence. Continuous repainting writes cursor/backend bytes even when
    // the picture is unchanged, so inspect the actual PTY output as well.
    term.settle(Duration::from_millis(600));
    assert_eq!(
        term.settle(Duration::from_millis(450)),
        0,
        "idle repaint loop"
    );
    assert!(logo_is(&term.screen, 4), "{}", term.dump());
    assert_eq!(term.screen.beyond(), 0);
    leave(&mut term);
}

#[test]
fn typing_and_interruptions_cancel_the_animation_wait() {
    let mut term = welcome(false);
    term.wait_until("first welcome frame", |screen| logo_is(screen, 0));
    let started = Instant::now();
    term.send("draft while the logo opens");
    term.wait_text("draft while the logo opens");
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "input waited for the reveal"
    );
    assert!(
        !logo_is(&term.screen, 4),
        "typing was held until the final mark"
    );
    leave(&mut term);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "interruption waited for the reveal"
    );
}

#[test]
fn reduced_motion_draws_the_final_mark_at_once_and_stays_silent() {
    let mut term = welcome(true);
    first_frame_whole(&mut term);
    assert!(
        logo_is(&term.screen, 4),
        "first paint was not final\n{}",
        term.dump()
    );
    let mark = term.mark();
    assert_eq!(
        term.settle(Duration::from_millis(2100)),
        0,
        "reduced motion scheduled a paint: {:?}\n{}",
        term.raw_since(mark),
        term.dump()
    );
    assert!(logo_is(&term.screen, 4));
    leave(&mut term);
}
