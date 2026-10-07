// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]
//! Real PTY proofs: the sole broker carries SGR mouse input, fullscreen owns
//! capture, inline returns it, and pointer input during work never submits.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use crossterm::event::{MouseButton, MouseEventKind};
use nika_tui::events::{Broker, UiEvent};
use nika_tui::model::Presentation;
use nika_tui::terminal;
use qa_support::{Term, exit_code, sized};
use std::io::Write as _;
use std::time::{Duration, Instant};

const MOUSE_ON: &str = "\x1b[?1006h";
const MOUSE_OFF: &str = "\x1b[?1006l";
const CHILD: &str = "NIKA_TUI_MOUSE_CHILD";

fn say(text: &str) {
    write!(std::io::stdout(), "\r\n{text}\r\n").expect("marker");
    std::io::stdout().flush().expect("flush marker");
}

#[test]
#[allow(clippy::disallowed_methods, reason = "a test-only child switch")]
fn mouse_broker_child() {
    if std::env::var(CHILD).is_err() {
        return;
    }
    let (mut owner, _screen) =
        terminal::enter(Presentation::Focus, Some("xterm-256color")).expect("enter");
    let mut broker = Broker::start();
    say("mouse reader ready");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut received = Vec::new();
    while received.len() < 2 {
        assert!(Instant::now() < deadline, "mouse never reached the broker");
        if let Some(UiEvent::Mouse(mouse)) = broker.try_recv() {
            received.push(mouse);
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert_eq!(
        (received[0].kind, received[0].column, received[0].row),
        (MouseEventKind::ScrollUp, 6, 4)
    );
    assert_eq!(
        (received[1].kind, received[1].column, received[1].row),
        (MouseEventKind::Down(MouseButton::Left), 11, 8)
    );
    broker.stop();
    terminal::set_alternate_screen(false).expect("inline releases mouse");
    say("inline released");
    terminal::set_alternate_screen(true).expect("fullscreen takes mouse");
    say("fullscreen resumed");
    owner.restore().expect("owner restores mouse");
    say("mouse lifecycle complete");
}

#[test]
fn one_broker_decodes_mouse_and_owner_restores_capture_across_inline() {
    let exe = std::env::current_exe().expect("test executable");
    let mut command = sized(exe.to_str().expect("executable path"), 100, 32);
    command
        .args([
            "--exact",
            "mouse_broker_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1");
    let mut term = Term::spawn(command, 100, 32);
    term.wait_text("mouse reader ready");
    assert!(term.raw_text().contains(MOUSE_ON));
    term.send("\x1b[<64;7;5M\x1b[<0;12;9M");
    term.wait_text("mouse lifecycle complete");
    assert_eq!(exit_code(term.finish()), Some(0));
    let raw = term.raw_text();
    let first_on = raw.find(MOUSE_ON).expect("first capture");
    let first_off = raw.find(MOUSE_OFF).expect("release for inline");
    let last_on = raw.rfind(MOUSE_ON).expect("capture after inline");
    let last_off = raw.rfind(MOUSE_OFF).expect("release on restore");
    assert!(
        first_on < first_off && first_off < last_on && last_on < last_off,
        "{raw:?}"
    );
}

#[test]
fn qa_child_host() {
    child::host();
}

#[test]
fn busy_wheel_and_click_keep_the_draft_unsent_after_resize() {
    let release = child::Release::new("mouse-busy");
    let mut term = child::spawn("slow-free:120:workspace", Some(release.path()), 120, 40);
    term.wait_text(qa_support::FREE); // Workspace prompt is inset after the project pane.
    term.send("work\r");
    term.wait_text(child::BUSY);
    let g = nika_tui::workspace::geometry::Geometry::of(
        ratatui::layout::Rect::new(0, 0, 120, 40),
        true,
    )
    .expect("geometry");
    let x = g.conversation.x + 3;
    let y = g.conversation.y + 5;
    term.send(&format!("\x1b[<64;{};{}M", x + 1, y + 1));
    term.send("draft-mouse");
    term.settle(Duration::from_millis(200));
    assert!(
        !term.screen.seen(child::SECOND),
        "a wheel submitted the draft"
    );
    term.resize(59, 15);
    term.send("\x1b[<64;3;3M");
    term.resize(120, 40);
    term.send(&format!("\x1b[<0;{};{}M", x + 1, y + 1));
    term.send("\x1b[F"); // Return to latest rows; a wheel deliberately kept the old reading position.
    release.open();
    term.wait_text(child::DONE);
    term.settle(Duration::from_millis(200));
    assert!(
        !term.screen.seen(child::SECOND),
        "a busy click became a queued submission"
    );
    assert!(
        term.screen.contains("draft-mouse"),
        "draft lost: {}",
        term.dump()
    );
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    assert_eq!(exit_code(term.finish()), Some(130));
    assert!(term.raw_text().contains(MOUSE_OFF));
}
