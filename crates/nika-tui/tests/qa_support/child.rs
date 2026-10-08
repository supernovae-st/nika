// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A conversation the QA proofs control, run by the real shell
//! (`nika_tui::app::run`) inside this test executable.
//!
//! The demo fixture answers every turn at once, so it cannot show what a
//! human does WHILE a turn runs. This child can: its first turn stays busy
//! (the spinner turns) until the proof releases it, and it ends on a gate or
//! a free prompt; it can open on N transcript lines; it can flood the busy
//! row. The proof re-invokes this very executable on a PTY with
//! `--exact qa_child_host --nocapture` and [`MODE`] set, the precedent of
//! nika-cli's `tests/tui_run_cost.rs` (`native_tui_fixture_parent`); in an
//! ordinary run the host test returns at once.

#![allow(
    dead_code,
    reason = "each suite that hosts the child uses a part of it"
)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use nika_tui::app::{self, Options};
use nika_tui::model::{
    Beat, Committed, Conversation, Handoff, Kind, Presentation, Script, Stopper, Stopping, Turn,
    Waiting, demo_project,
};

use crate::qa_support::{Term, sized};

/// The environment switch that turns the host test into the child.
pub(crate) const MODE: &str = "NIKA_TUI_QA_CHILD";
/// The file whose appearance releases the busy turn.
pub(crate) const RELEASE: &str = "NIKA_TUI_QA_RELEASE";
/// The busy label of the held turn.
pub(crate) const BUSY: &str = "qa busy turn";
/// The gate the held turn ends on, and what answering it says.
pub(crate) const GATE_SAYS: &str = "qa gate · approve the write?";
pub(crate) const AFTER_GATE: &str = "qa gate answered";
/// What the held turn says when it ends on a free prompt, and the next turn.
pub(crate) const DONE: &str = "qa turn done";
pub(crate) const SECOND: &str = "qa second turn";
/// The held preparation settled after a real Stop request, counted exactly.
pub(crate) const STOPPED: &str = "qa Stop observed · calls=1";
/// The opening banner.
pub(crate) const BANNER: &str = "qa child · ready";
/// The longest a held turn waits for its release.
const HOLD_MAX: Duration = Duration::from_secs(30);

/// The controlled conversation.
pub(crate) struct Child {
    script: Script,
    release: Option<PathBuf>,
    flood: usize,
    fresh_at_gate: bool,
    submitted: usize,
    gate_waits: bool,
    /// The workspace mode lends the demo project ([`demo_project`]).
    lends_project: bool,
    /// Only `slow-stop` lends a stoppable preparation. Settlement still waits
    /// for the proof's release, so requesting and observing Stop stay distinct.
    stop_calls: Option<Arc<AtomicUsize>>,
}

impl Child {
    /// The first turn's work: the flood, then the hold until the release.
    fn hold(&self, busy: &Sender<String>) {
        for index in 0..self.flood {
            let _ = busy.send(format!("{BUSY} · event {index}"));
        }
        let Some(release) = self.release.as_deref() else {
            return;
        };
        let _ = busy.send(BUSY.to_owned());
        let started = Instant::now();
        while !release.exists() && started.elapsed() < HOLD_MAX {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Conversation for Child {
    fn stopper(&mut self) -> Option<Stopper> {
        let calls = self.stop_calls.as_ref()?.clone();
        Some(Box::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            Stopping::Requested
        }))
    }

    fn fresh_input_required(&self) -> bool {
        self.fresh_at_gate && self.gate_waits
    }

    // One insertion-only command for the fresh-answer palette proof. The
    // controlled script never qualifies real Session command availability.
    fn commands(&self) -> Vec<String> {
        if self.fresh_at_gate {
            vec!["/status".to_owned()]
        } else {
            Vec::new()
        }
    }

    fn project(&self) -> Option<nika_tui::workspace::project::ProjectView> {
        self.lends_project.then(demo_project)
    }

    fn open(&mut self) -> Vec<Beat> {
        self.script.open()
    }

    fn submit(&mut self, line: &str) -> Turn {
        if self.fresh_at_gate && self.gate_waits && !matches!(line.trim(), "yes" | "no") {
            let mut waiting = Script::new(Vec::new(), vec![vec![Beat::Wait(Waiting::Gate)]]);
            return <Script as Conversation>::submit(&mut waiting, line);
        }
        let fresh_answer = self.fresh_at_gate && self.gate_waits;
        let mut turn = <Script as Conversation>::submit(&mut self.script, line);
        if fresh_answer {
            turn.beats
                .insert(0, say(Kind::Notice, format!("qa fresh answer: {line}")));
        }
        self.gate_waits = turn
            .beats
            .iter()
            .any(|beat| matches!(beat, Beat::Wait(Waiting::Gate)));
        turn
    }

    fn submit_with(&mut self, line: &str, busy: &Sender<String>) -> Turn {
        self.submitted += 1;
        if self.submitted == 1 {
            self.hold(busy);
            if let Some(calls) = self
                .stop_calls
                .as_ref()
                .filter(|calls| calls.load(Ordering::SeqCst) > 0)
            {
                let mut cancelled = Script::new(
                    Vec::new(),
                    vec![vec![
                        Beat::Cancelled(format!(
                            "qa Stop observed · calls={}",
                            calls.load(Ordering::SeqCst)
                        )),
                        Beat::Wait(Waiting::Free),
                    ]],
                );
                return <Script as Conversation>::submit(&mut cancelled, line);
            }
        }
        self.submit(line)
    }

    fn perform(&mut self, _handoff: &Handoff) -> Vec<Beat> {
        Vec::new()
    }
}

fn say(kind: Kind, text: impl Into<String>) -> Beat {
    Beat::Say(Committed::new(kind, text))
}

/// Typed display evidence only: the strict door refuses this named release
/// before I/O. The held turn tests shell routing, never a Live/provider call.
fn diagnostic() -> Option<Beat> {
    use nika_cli_host::compile::config::AuthoringSettings;
    use nika_session::authoring::{AuthoringContext, AuthoringError};
    let mut env = AuthoringSettings::none();
    env.knowledge = Some(PathBuf::from("/srv/foundry/release-r3"));
    let context = AuthoringContext::from_settings(&AuthoringSettings::none(), &env);
    context.refusal().cloned().map(|cause| say(Kind::Refusal, format!(
        "{} · this workflow-authoring request was not sent, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again",
        AuthoringError::Context(cause)
    )))
}

/// The conversation and presentation of one mode, `<kind>[:<n>[:<inline|focus|workspace>]]`
/// (the workspace mode lends the demo project):
/// `slow-free` · `slow-gate` · `slow-gate-fresh` (the gate asks for fresh
/// input) · `slow-stop` (stoppable preparation) · `slow-diagnostic` (opening raw evidence) · `big` (no held turn)
/// open on `n` transcript lines; `flood`
/// sends `n` busy labels in its first turn.
pub(crate) fn conversation_for(mode: &str, release: Option<PathBuf>) -> (Child, Presentation) {
    let mut parts = mode.split(':');
    let kind = parts.next().unwrap_or_default();
    let count: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let presentation = match parts.next() {
        Some("focus") => Presentation::Focus,
        Some("workspace" | "workspace-ascii") => Presentation::Workspace,
        _ => Presentation::Inline,
    };
    let mut opening = vec![say(Kind::Banner, BANNER)];
    if kind == "slow-diagnostic" {
        opening.extend(diagnostic());
    }
    if kind != "flood" {
        opening.extend((0..count).map(|i| say(Kind::Run, format!("item {i:05} ✓ {} ms", i % 97))));
    }
    opening.push(Beat::Wait(Waiting::Free));
    let first = if kind.starts_with("slow-gate") {
        vec![say(Kind::Gate, GATE_SAYS), Beat::Wait(Waiting::Gate)]
    } else {
        vec![say(Kind::Reply, DONE), Beat::Wait(Waiting::Free)]
    };
    let second = if kind.starts_with("slow-gate") {
        vec![say(Kind::Reply, AFTER_GATE), Beat::Wait(Waiting::Free)]
    } else {
        vec![say(Kind::Reply, SECOND), Beat::Wait(Waiting::Free)]
    };
    let child = Child {
        script: Script::new(opening, vec![first, second]),
        release: if kind.starts_with("slow") {
            release
        } else {
            None
        },
        flood: if kind == "flood" { count } else { 0 },
        fresh_at_gate: kind == "slow-gate-fresh",
        submitted: 0,
        gate_waits: false,
        lends_project: presentation == Presentation::Workspace,
        stop_calls: (kind == "slow-stop").then(|| Arc::new(AtomicUsize::new(0))),
    };
    (child, presentation)
}

/// `std::env::var`, for the harness switches (not secrets).
#[allow(
    clippy::disallowed_methods,
    reason = "a test-harness switch, never a secret (the proto reads TERM the same way)"
)]
fn switch(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The child side: nothing in an ordinary test run; with [`MODE`] set, the
/// real shell over the controlled conversation on this process's terminal,
/// then the process leaves with the shell's exit code.
pub(crate) fn host() {
    let Some(mode) = switch(MODE) else {
        return;
    };
    let (child, presentation) = conversation_for(&mode, switch(RELEASE).map(PathBuf::from));
    let mut options = Options::new(presentation);
    options.term = Some("xterm-256color".to_owned());
    options.ascii = mode.ends_with(":workspace-ascii");
    let code = match app::run(child, options) {
        Ok(exit) => i32::from(exit.code()),
        Err(_) => 2,
    };
    std::process::exit(code);
}

/// The parent side: this test executable re-invoked on a `cols` × `rows`
/// PTY as the child of `mode`.
pub(crate) fn spawn(mode: &str, release: Option<&Path>, cols: u16, rows: u16) -> Term {
    let exe = std::env::current_exe().expect("this test executable");
    let mut command = sized(exe.to_str().expect("a UTF-8 path"), cols, rows);
    command
        .args([
            "--exact",
            "qa_child_host",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env(MODE, mode);
    if let Some(release) = release {
        command.env(RELEASE, release);
    }
    Term::spawn(command, cols, rows)
}

/// A release file private to one proof, removed when the proof ends.
pub(crate) struct Release(PathBuf);

impl Release {
    /// A fresh path under the temporary directory.
    pub(crate) fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("nika-tui-qa-{}-{tag}.release", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    /// The path the child watches.
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// Let the held turn end.
    pub(crate) fn open(&self) {
        std::fs::write(&self.0, b"release").expect("write the release file");
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
