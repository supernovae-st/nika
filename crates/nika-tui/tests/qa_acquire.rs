// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed PTY proof reports its exact screen"
)]
//! Input submitted while a read-only journal acquisition is held remains a fresh input.

#[path = "qa_support/child.rs"]
mod child;
mod qa_support;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use nika_display::run_story::ExecutionId;
use nika_session::KeptRun;
use nika_tui::app::{self, Options};
use nika_tui::model::{
    Beat, Committed, Conversation, Handoff, Kind, Presentation, Script, Turn, Waiting,
};
use nika_tui::session::acquire::Proven;
use qa_support::{FREE, Term, assert_restored, exit_code};

const REQUEST: &str = "Relance le même workflow, sans le modifier.";
const ACCEPTED: &str = "qa accepted request exactly once";

struct Reading {
    release: PathBuf,
    submitted: Vec<String>,
}

impl Conversation for Reading {
    fn open(&mut self) -> Vec<Beat> {
        vec![
            Beat::Say(Committed::new(Kind::Banner, "qa earlier run")),
            Beat::Wait(Waiting::Free),
        ]
    }

    fn project(&self) -> Option<nika_tui::workspace::project::ProjectView> {
        Some(nika_tui::model::demo_project())
    }

    fn kept_run(&self) -> Option<Result<KeptRun, String>> {
        let mut run = KeptRun::new();
        run.workflow = Some("saved.nika".to_owned());
        run.execution = Some("01a0ef11-0212-70de-a8b3-99de9427fccc".to_owned());
        run.exit = Some(0);
        Some(Ok(run))
    }

    fn prove(&mut self, _: &ExecutionId) -> Option<Proven> {
        let start = Instant::now();
        while !self.release.exists() && start.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    fn submit(&mut self, line: &str) -> Turn {
        self.submitted.push(line.to_owned());
        let bytes = serde_json::to_vec(&self.submitted).expect("serialize exact submitted lines");
        std::fs::write(self.release.with_extension("submissions.json"), bytes)
            .expect("record input");
        let mut script = Script::new(
            Vec::new(),
            vec![vec![
                Beat::Say(Committed::new(Kind::Reply, ACCEPTED)),
                Beat::Wait(Waiting::Free),
            ]],
        );
        <Script as Conversation>::submit(&mut script, line)
    }

    fn perform(&mut self, _: &Handoff) -> Vec<Beat> {
        panic!("reading the earlier run must never hand work off")
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "private PTY harness switches, never credentials"
)]
fn release_from_env() -> Option<PathBuf> {
    std::env::var(child::MODE).ok()?;
    Some(PathBuf::from(
        std::env::var(child::RELEASE).expect("release file"),
    ))
}

#[test]
fn qa_child_host() {
    let Some(release) = release_from_env() else {
        return;
    };
    let host = Reading {
        release,
        submitted: Vec::new(),
    };
    let mut options = Options::new(Presentation::Workspace);
    options.term = Some("xterm-256color".to_owned());
    let exit = app::run(host, options).expect("real shell");
    std::process::exit(i32::from(exit.code()));
}

fn submitted(release: &child::Release) -> Vec<String> {
    let path = release.path().with_extension("submissions.json");
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).expect("recorded exact input"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("cannot read submitted input: {error}"),
    }
}

fn held(tag: &str) -> (child::Release, Term) {
    let release = child::Release::new(tag);
    let _ = std::fs::remove_file(release.path().with_extension("submissions.json"));
    let mut term = child::spawn("acquire", Some(release.path()), 80, 24);
    term.wait_prompt(FREE);
    (release, term)
}

fn leave(term: &mut Term, release: &child::Release) {
    term.send("\x03");
    term.wait_text("Ctrl+C again leaves");
    term.send("\x03");
    assert_eq!(exit_code(term.finish()), Some(130), "{}", term.dump());
    assert_restored(term);
    let _ = std::fs::remove_file(release.path().with_extension("submissions.json"));
}

/// The old busy hint and the repaired retained-event hint both acknowledge that
/// the shell heard Enter before the test releases the read; no scheduling sleep.
fn enter_heard(term: &mut Term) {
    term.wait_until("Enter heard while the evidence read is held", |screen| {
        screen.contains("Nika is working") || screen.contains("Enter kept")
    });
}

#[test]
fn a_fresh_rerun_enter_survives_the_earlier_run_read() {
    for (tag, wire) in [
        ("acquire-typed", format!("{REQUEST}\r")),
        ("acquire-pasted", format!("\x1b[200~{REQUEST}\x1b[201~\r")),
    ] {
        let (release, mut term) = held(tag);
        term.send(&wire);
        enter_heard(&mut term);
        assert_eq!(submitted(&release), Vec::<String>::new());
        release.open();
        term.wait_text(ACCEPTED);
        assert_eq!(submitted(&release), vec![REQUEST]);
        leave(&mut term, &release);
    }
}

#[test]
fn editing_after_a_retained_enter_does_not_change_the_submitted_bytes() {
    let (release, mut term) = held("acquire-order");
    term.send(&format!("{REQUEST}\rnext draft"));
    enter_heard(&mut term);
    release.open();
    term.wait_workspace_frame(
        "first request submitted, later text still a draft",
        |screen| screen.seen(ACCEPTED) && screen.contains("next draft"),
    );
    assert_eq!(submitted(&release), vec![REQUEST]);
    leave(&mut term, &release);
}

#[test]
fn interrupt_cancels_the_retained_enter_and_preserves_following_edits() {
    let (release, mut term) = held("acquire-cancel");
    term.send("AB\r\x7fC\x03");
    term.wait_text("Ctrl+C again leaves");
    assert_eq!(submitted(&release), Vec::<String>::new());
    release.open();
    term.wait_workspace_frame("cancelled request remains an edited draft", |screen| {
        screen.contains("AC") && !screen.contains("reading what the run left")
    });
    assert_eq!(submitted(&release), Vec::<String>::new());
    term.send("\x03");
    assert_eq!(exit_code(term.finish()), Some(130), "{}", term.dump());
    assert_restored(&term);
}

#[test]
fn interrupt_keeps_editor_shortcuts_before_following_text() {
    for (tag, wire, expected) in [
        ("acquire-cancel-head", "AB\r\x01C\x03", "CAB"),
        ("acquire-cancel-cut", "AB\r\x01C\x0b\x03", "C"),
    ] {
        let (release, mut term) = held(tag);
        term.send(wire);
        term.wait_text("Ctrl+C again leaves");
        assert_eq!(submitted(&release), Vec::<String>::new());
        release.open();
        term.wait_workspace_frame("cancelled editor shortcuts keep their order", |screen| {
            screen.contains(expected) && !screen.contains("reading what the run left")
        });
        // A fresh Enter records the complete resulting draft, not just visible
        // substrings: CAB must not silently become ABC after this read.
        term.send("\r");
        term.wait_text(ACCEPTED);
        assert_eq!(submitted(&release), vec![expected]);
        leave(&mut term, &release);
    }
}

#[test]
fn navigation_enter_during_a_read_never_activates_the_composer() {
    let (release, mut term) = held("acquire-navigation");
    term.send("kept draft");
    term.wait_text("reading what the run left");
    // F6 reaches the object. Its Enter stays navigation; the next F6 must
    // still reach the folded aside while the evidence read remains held.
    term.send("\x1b[17~\r\x1b[17~");
    term.wait_text("Nika · Files");
    assert_eq!(submitted(&release), Vec::<String>::new());
    term.send("\x1b");
    term.wait_until("the composer has the keys again", |screen| {
        !screen.contains("Nika · Files")
    });
    term.send("!");
    term.wait_text("kept draft!");
    release.open();
    term.wait_workspace_frame("navigation returned without submitting", |screen| {
        screen.contains("kept draft!") && !screen.contains("reading what the run left")
    });
    assert_eq!(submitted(&release), Vec::<String>::new());
    term.send("\r");
    term.wait_text(ACCEPTED);
    assert_eq!(submitted(&release), vec!["kept draft!"]);
    leave(&mut term, &release);
}

#[test]
fn editor_shortcuts_before_enter_keep_the_submitted_bytes_in_order() {
    for (tag, wire, expected) in [
        ("acquire-edit-head", "AB\x01C\r", "CAB"),
        ("acquire-edit-cut", "AB\x01C\x0b\r", "C"),
    ] {
        let (release, mut term) = held(tag);
        term.send(wire);
        enter_heard(&mut term);
        assert_eq!(submitted(&release), Vec::<String>::new());
        release.open();
        term.wait_text(ACCEPTED);
        assert_eq!(submitted(&release), vec![expected]);
        leave(&mut term, &release);
    }
}
