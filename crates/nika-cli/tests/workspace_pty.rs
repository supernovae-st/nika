// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as tui_pty: this suite's WHOLE JOB is to drive the real
// binary through a PTY, because bare `nika` opens the workspace only on a
// terminal — unreachable from every piped harness.
#![allow(clippy::disallowed_types)]
//! Bare `nika` on a terminal opens the workspace over the real project: no
//! `Ctrl+T`, no fixture. Two ordinary workflows sit on disk, a four-task
//! diamond and a single task; the human selects one in the aside, and its
//! source, plan, graph and check are drawn from ONE read of its bytes, each
//! face saying it shows the file as last read, never the hex of their
//! witness. Changed or invalid bytes need a new look (`r`) and never borrow
//! the old check. The workspace follows every size, gives way to the
//! focus view below its minimum and comes back whole; ASCII, `NO_COLOR` and
//! reduced motion keep every face readable; the keys follow the focus and a
//! typed-ahead draft is never sent; an interruption or `SIGTERM` gives the
//! terminal back. Inline is an explicit door (`NIKA_TUI=inline`), plain and
//! pipe stay as they are, opening a viewer sends nothing to a model and
//! grants nothing, and Save is never Run.
//!
//! Keyless: the workspace needs an intelligence (it never opts into the
//! offline choice), so the kept choice is a local engine (`vllm`) whose base
//! URL is this proof's loopback model (`support/loopback_model.rs`): nothing
//! leaves 127.0.0.1 and every request it receives is kept, so a proof that
//! claims no model call reads that log. The round driver these proofs were
//! written for is kept (`NIKA_SESSION_DRIVER=rounds`), the PATH holds no
//! harness executable, the keychain is off. A decision the Session waits for
//! holds the line as a finite action (`action ›`). The screen is read through
//! the VT model of the TUI reception suites (`nika-tui/tests/qa_support/vt.rs`),
//! which answers the renderer's terminal queries itself. With
//! `NIKA_TUI01_EVIDENCE=<dir>` every proof keeps its raw stream, its labelled
//! screens and the SHA-256 of the binary it drove there.

#[path = "../../nika-tui/tests/qa_support/vt.rs"]
#[allow(
    dead_code,
    reason = "the workspace suite reads part of the shared VT screen"
)]
mod vt;

#[path = "support/loopback_model.rs"]
mod loopback_model;

#[path = "support/scripted_author.rs"]
mod scripted_author;

#[path = "workspace_pty/choice.rs"]
mod choice;

#[path = "workspace_pty/picker.rs"]
mod picker;

#[path = "workspace_pty/inspect.rs"]
mod inspect;

#[path = "workspace_pty/language.rs"]
mod language;

#[path = "workspace_pty/run.rs"]
mod run;

#[path = "workspace_pty/question.rs"]
mod question;

#[path = "workspace_pty/surface.rs"]
mod surface;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use expectrl::process::unix::{Signal, WaitStatus};
use expectrl::session::OsSession;
use loopback_model::{Asked, Model};
use sha2::{Digest as _, Sha256};

/// The longest any single wait may take before the proof fails with a dump.
const WAIT: Duration = Duration::from_secs(30);
/// A pause long enough for the renderer to read a key and draw.
const SETTLE: Duration = Duration::from_millis(400);

const F6: &str = "\x1b[17~";
const F4: &str = "\x1b[14~";
const SHIFT_F6: &str = "\x1b[17;2~";
const END: &str = "\x1b[F";
const PAGE_UP: &str = "\x1b[5~";
const DOWN: &str = "\x1b[B";
const UP: &str = "\x1b[A";
const RIGHT: &str = "\x1b[C";
const LEFT: &str = "\x1b[D";
const ESC: &str = "\x1b";
const F2: &str = "\x1bOQ";
const PAGE_DOWN: &str = "\x1b[6~";

/// The row each face of a file reads above it: the file as last read, never its witness's hex.
const AS_READ: &str = "as last read";

/// The graph face's own form of that row.
const GRAPH_READ: &str = "This file as last read";

/// A four-task diamond: one root, two branches, one join.
const DIAMOND: &str = r#"nika: diamond
permits: {}
tasks:
  fetch:
    invoke: { tool: "nika:log", args: { message: fetch } }
  left:
    with: { x: "${{ tasks.fetch.output }}" }
    invoke: { tool: "nika:log", args: { message: left } }
  right:
    with: { x: "${{ tasks.fetch.output }}" }
    invoke: { tool: "nika:log", args: { message: right } }
  join:
    with: { l: "${{ tasks.left.output }}", r: "${{ tasks.right.output }}" }
    invoke: { tool: "nika:log", args: { message: join } }
"#;

/// A second, distinct workflow: one task.
const SINGLE: &str = r#"nika: single
permits: {}
tasks:
  only:
    invoke: { tool: "nika:log", args: { message: only } }
"#;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

/// The first twelve hex digits of the witness the Session stamps on bytes.
fn short_witness(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().chars().take(12).collect()
}

/// A project with the two workflows and a home whose kept choice is a local
/// engine served by `model`, which lives as long as the rig.
struct Rig {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
    /// The kept intelligence, on 127.0.0.1: every request it received.
    model: Model,
}

impl Rig {
    fn new(tag: &str) -> Self {
        Self::with_model(tag, Model::start())
    }

    /// The same rig, its kept intelligence scripted ([`Model::scripted`]): each line the Session
    /// routes to it receives the next of `labels`.
    fn scripted(tag: &str, labels: &[&str]) -> Self {
        Self::with_model(tag, Model::scripted(labels))
    }

    fn with_model(tag: &str, model: Model) -> Self {
        let project = tempfile::Builder::new()
            .prefix(&format!("nika-ws-{tag}-"))
            .tempdir()
            .expect("project dir");
        let home = tempfile::Builder::new()
            .prefix(&format!("nika-ws-home-{tag}-"))
            .tempdir()
            .expect("home dir");
        // « No AI » no longer opens the workspace: the host opens the
        // required intelligence chooser over it. A ready local engine does.
        Model::chosen_in(home.path());
        std::fs::write(project.path().join("diamond.nika"), DIAMOND).expect("diamond");
        std::fs::write(project.path().join("single.nika"), SINGLE).expect("single");
        Self {
            project,
            home,
            model,
        }
    }

    /// `what` reached no model: the kept intelligence received no request.
    fn no_model_call(&self, what: &str) {
        let bodies = self.model.bodies();
        assert!(
            bodies.is_empty(),
            "{what}: {} request(s) reached the model\n{bodies:#?}",
            bodies.len()
        );
    }

    /// `what` asked the kept intelligence to route exactly `lines`, in order (each typed reply
    /// the Session reads is one authoring call), and asked it nothing else than those routes
    /// and the judge's closed choices.
    fn routed(&self, lines: &[&str], what: &str) {
        let asked = self.model.asked();
        let routes: Vec<&str> = (asked.iter())
            .filter_map(|asked| match asked {
                Asked::Route(line) => Some(line.as_str()),
                Asked::Choice(_) | Asked::Other => None,
            })
            .collect();
        assert_eq!(routes, lines, "{what}: the routed lines\n{asked:#?}");
        assert!(
            (asked.iter()).all(|asked| matches!(asked, Asked::Route(_) | Asked::Choice(_))),
            "{what}: only routes and the judge's choices reached the model\n{asked:#?}"
        );
    }

    /// `what` ran nothing: the project holds no Run journal (counted apart from the model's
    /// authoring calls).
    fn no_run(&self, what: &str) {
        assert!(
            !self.path(".nika/traces").exists(),
            "{what}: a Run journal exists"
        );
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.project.path().join(rel)
    }

    /// Bare `nika` (plus `args`) in the project, sized before its first paint,
    /// with a minimal environment and `env` on top.
    fn command(&self, args: &[&str], cols: u16, rows: u16, env: &[(&str, &str)]) -> Command {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(format!(
                "stty cols {cols} rows {rows} && exec \"$0\" \"$@\""
            ))
            .arg(bin())
            .args(args)
            .current_dir(self.project.path())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .env("HOME", self.home.path())
            .env("NIKA_KEYCHAIN", "off")
            .env("NIKA_VLLM_BASE_URL", self.model.base())
            // The round driver, read once when the Session opens: with no
            // intelligence the Session kept it, and these proofs read it.
            .env("NIKA_SESSION_DRIVER", "rounds");
        for (key, value) in env {
            command.env(key, value);
        }
        command
    }

    fn spawn(&self, tag: &str, cols: u16, rows: u16) -> Term {
        self.spawn_with(tag, &[], cols, rows, &[])
    }

    fn spawn_with(
        &self,
        tag: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        env: &[(&str, &str)],
    ) -> Term {
        Term::spawn(tag, self.command(args, cols, rows, env), cols, rows)
    }

    /// Every file under the project with its bytes, `.nika/` state excluded.
    fn tree(&self) -> Vec<(String, Vec<u8>)> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let mut entries: Vec<_> = std::fs::read_dir(dir)
                .expect("read dir")
                .map(|e| e.expect("entry").path())
                .collect();
            entries.sort();
            for path in entries {
                let rel = path
                    .strip_prefix(root)
                    .expect("below")
                    .display()
                    .to_string();
                if path.is_dir() {
                    if rel != ".nika" {
                        walk(&path, root, out);
                    }
                } else {
                    out.push((rel, std::fs::read(&path).expect("read")));
                }
            }
        }
        let mut out = Vec::new();
        walk(self.project.path(), self.project.path(), &mut out);
        out
    }
}

/// The binary on a PTY, its raw output and the screen it painted.
struct Term {
    tag: String,
    pty: OsSession,
    screen: vt::Screen,
    raw: Vec<u8>,
    input_mark: usize,
    eof: bool,
    shots: Vec<(String, String)>,
    replay: Vec<serde_json::Value>,
    started: Instant,
    /// At most this many bytes per read ([`Term::reading_at_most`]).
    read_limit: Option<usize>,
}

impl Term {
    fn spawn(tag: &str, command: Command, cols: u16, rows: u16) -> Self {
        let mut pty = OsSession::spawn(command).expect("pty spawn");
        pty.get_process_mut()
            .set_window_size(cols, rows)
            .expect("size the pty");
        let mut screen = vt::Screen::new(cols, rows);
        screen.park_at_bottom();
        Self {
            tag: tag.to_owned(),
            pty,
            screen,
            raw: Vec::new(),
            input_mark: 0,
            eof: false,
            shots: Vec::new(),
            replay: vec![serde_json::json!({
                "type": "size", "offset": 0, "cols": cols, "rows": rows
            })],
            started: Instant::now(),
            read_limit: None,
        }
    }

    /// This terminal read adversarially, as nika-tui's reception suites do:
    /// each pump takes at most `bytes` of what the process wrote, so every
    /// wait is judged inside the frames too and only a complete frame can
    /// satisfy a wait that needs one, whatever the scheduling.
    fn reading_at_most(mut self, bytes: usize) -> Self {
        self.read_limit = Some(bytes.max(1));
        self
    }

    fn pump(&mut self) -> usize {
        let mut total = 0;
        let mut buf = [0u8; 16 * 1024];
        let limit = self.read_limit.unwrap_or(buf.len()).min(buf.len());
        while !self.eof {
            match self.pty.try_read(&mut buf[..limit]) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    total += n;
                    self.raw.extend_from_slice(&buf[..n]);
                    self.screen.feed(&buf[..n]);
                    for reply in self.screen.take_replies() {
                        let _ = self.pty.write_all(&reply);
                        let _ = self.pty.flush();
                    }
                    if self.read_limit.is_some() {
                        // One slice per pump: the caller judges the screen now.
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => self.eof = true,
            }
        }
        total
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&vt::Screen) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            let read = self.pump();
            if done(&self.screen) {
                self.shot(what);
                return;
            }
            assert!(
                !self.eof && Instant::now() <= deadline,
                "{what}: never reached ({}).\n{}",
                if self.eof {
                    "the process ended"
                } else {
                    "timeout"
                },
                self.dump()
            );
            if read == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn wait_text(&mut self, needle: &str) {
        self.wait_until(needle, |screen| screen.contains(needle));
    }

    /// The workspace paints its own caret. Ratatui emits Hide after the cell
    /// diff and flushes it: a matching header alone can be a partial frame.
    /// Capture the matching state only after that native frame boundary.
    fn wait_workspace_frame(&mut self, what: &str, done: impl Fn(&vt::Screen) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            let read = self.pump();
            if self.raw.len() > self.input_mark
                && self.raw.ends_with(b"\x1b[?25l")
                && done(&self.screen)
            {
                self.shot(what);
                return;
            }
            assert!(
                !self.eof && Instant::now() <= deadline,
                "{what}: complete workspace frame never reached.\n{}",
                self.dump()
            );
            if read == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn settle(&mut self) {
        let deadline = Instant::now() + SETTLE;
        while Instant::now() < deadline && !self.eof {
            if self.pump() == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn send(&mut self, text: &str) {
        if !text.is_empty() {
            self.input_mark = self.raw.len();
        }
        self.pty.write_all(text.as_bytes()).expect("write");
        self.pty.flush().expect("flush");
    }

    /// Send `keys` and let the renderer draw.
    fn keys(&mut self, keys: &str) {
        self.send(keys);
        self.settle();
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.pump();
        self.input_mark = self.raw.len();
        self.replay.push(serde_json::json!({
            "type": "resize", "offset": self.raw.len(), "cols": cols, "rows": rows
        }));
        self.screen.resize(cols, rows);
        self.pty
            .get_process_mut()
            .set_window_size(cols, rows)
            .expect("resize the pty");
    }

    fn signal(&mut self, signal: Signal) {
        self.pty.get_process_mut().kill(signal).expect("signal");
    }

    fn finish(&mut self) -> WaitStatus {
        let deadline = Instant::now() + WAIT;
        while !self.eof {
            if self.pump() == 0 {
                assert!(
                    Instant::now() < deadline,
                    "the process never ended.\n{}",
                    self.dump()
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        self.pty.get_process().wait().expect("wait")
    }

    /// The visible rows, joined.
    fn text(&self) -> String {
        self.screen.lines().join("\n")
    }

    /// Keep the current screen under `label` for the evidence.
    fn shot(&mut self, label: &str) {
        let stamp = self.started.elapsed().as_millis();
        let (cols, rows) = self.screen.size();
        let head = format!(
            "{label} · +{stamp} ms · {cols}x{rows} · alt {}",
            self.screen.on_alt()
        );
        // Preserve byte offsets and sizes so a terminal emulator can replay
        // the actual colors and cell positions, without restyling plain text.
        self.replay.push(serde_json::json!({
            "type": "frame", "offset": self.raw.len(), "cols": cols, "rows": rows,
            "label": label, "text": self.text(),
            "complete": self.raw.len() > self.input_mark && self.raw.ends_with(b"\x1b[?25l")
        }));
        self.shots.push((head, self.text()));
    }

    fn dump(&self) -> String {
        let screen: Vec<String> = self
            .screen
            .lines()
            .iter()
            .enumerate()
            .map(|(row, line)| format!("{row:>3}|{line}"))
            .collect();
        let raw = String::from_utf8_lossy(&self.raw);
        let from = raw.char_indices().rev().nth(1999).map_or(0, |(at, _)| at);
        format!(
            "--- screen {:?} cursor {:?} alt {} ---\n{}\n--- stream tail ---\n{:?}",
            self.screen.size(),
            self.screen.cursor(),
            self.screen.on_alt(),
            screen.join("\n"),
            &raw[from..]
        )
    }

    /// The terminal handed back: paste and focus reports off, the cursor
    /// shown, the main screen on.
    fn assert_restored(&self) {
        for (mode, on, what) in [
            (2004, false, "bracketed paste left on"),
            (1004, false, "focus reporting left on"),
            (25, true, "cursor left hidden"),
        ] {
            assert_eq!(self.screen.mode(mode), Some(on), "{what}.\n{}", self.dump());
        }
        assert!(
            !self.screen.on_alt(),
            "left on the alternate screen.\n{}",
            self.dump()
        );
    }

    /// Two `Ctrl+C` while idle leave with 130 and the terminal restored: the
    /// second goes once the screen acknowledges the first at the idle prompt
    /// (« Ctrl+C again leaves · any key stays », in French where the person
    /// writes French), never after a fixed wait.
    fn leave(&mut self) {
        self.send("\x03");
        self.wait_workspace_frame("the first Ctrl+C acknowledged at the idle prompt", |s| {
            (s.contains("Ctrl+C again leaves") && s.contains("any key stays"))
                || (s.contains("Ctrl+C encore quitte") && s.contains("autre touche reste"))
        });
        self.send("\x03");
        let status = self.finish();
        assert_eq!(exit_code(status), Some(130), "{status:?}\n{}", self.dump());
        self.assert_restored();
    }
}

impl Drop for Term {
    /// Keep the evidence when asked; never leave the binary running.
    fn drop(&mut self) {
        if !self.eof {
            let _ = self.pty.get_process_mut().kill(Signal::SIGKILL);
        }
        let Some(dir) = evidence_dir() else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("{}.raw", self.tag)), &self.raw);
        if let Ok(events) = serde_json::to_vec_pretty(&self.replay) {
            let _ = std::fs::write(dir.join(format!("{}.events.json", self.tag)), events);
        }
        let shots: Vec<String> = self
            .shots
            .iter()
            .map(|(head, body)| format!("=== {head}\n{body}\n"))
            .collect();
        let _ = std::fs::write(
            dir.join(format!("{}.screens.txt", self.tag)),
            shots.join("\n"),
        );
        let digest = std::fs::read(bin()).map(Sha256::digest);
        if let Ok(digest) = digest {
            let hex = digest.iter().fold(String::new(), |mut hex, b| {
                let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{b:02x}"));
                hex
            });
            let _ = std::fs::write(dir.join("binary.sha256"), format!("{hex}  {}\n", bin()));
        }
    }
}

/// Where the evidence goes, when asked (a test switch, never a secret).
#[allow(clippy::disallowed_methods)]
fn evidence_dir() -> Option<PathBuf> {
    std::env::var_os("NIKA_TUI01_EVIDENCE").map(PathBuf::from)
}

fn exit_code(status: WaitStatus) -> Option<i32> {
    match status {
        WaitStatus::Exited(_, code) => Some(code),
        _ => None,
    }
}

/// The workspace is up over the rig's project: both workflows listed.
fn wait_workspace(term: &mut Term) {
    term.wait_until("the workspace over the project", |screen| {
        screen.on_alt()
            && screen.contains("diamond.nika")
            && screen.contains("single.nika")
            && screen.contains("Ctrl+O")
            && (screen.contains("nika ›") || screen.contains("nika >"))
    });
}

/// A row of `screen` carries the finite action's prompt, `action ›` (its
/// ASCII twin `action >`), as a word of its own: the composer's prompt while
/// an action holds the line, or the echo of a line sent at one. The window
/// title's « action required » is no screen row and has no prompt mark.
fn action_prompt_shown(screen: &vt::Screen) -> bool {
    screen.lines().iter().any(|row| {
        ["action ›", "action >"].into_iter().any(|prompt| {
            row.match_indices(prompt).any(|(at, _)| {
                !row[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
        })
    })
}

/// A finite Session action holds the line now: its prompt is painted and the
/// hint names its keys (`Enter acts`, in every form a narrow row takes; `Entrée
/// valide` where the person writes French). Its band covers the conversation
/// down to the composer, so the echo of an earlier answer is out of view
/// whenever this holds.
fn action_holds(screen: &vt::Screen) -> bool {
    action_prompt_shown(screen)
        && (screen.contains("Enter acts") || screen.contains("Entrée valide"))
}

/// Open the aside entry `downs` rows below this conversation (wide layout).
fn open_entry(term: &mut Term, downs: usize) {
    term.keys(SHIFT_F6);
    for _ in 0..downs {
        term.keys(DOWN);
    }
    term.keys("\r");
}

/// Shift+F6 wraps from the aside to the object region.
fn to_object(term: &mut Term) {
    term.keys(SHIFT_F6);
}

/// 1 · Bare `nika` opens the workspace directly: no key is sent before the
/// first frame, which already names the project and lists both workflows.
#[test]
fn bare_nika_opens_the_workspace_on_the_real_project() {
    let rig = Rig::new("open");
    let mut term = rig.spawn("01-open", 120, 36);
    wait_workspace(&mut term);
    let first = term.started.elapsed();
    let _ = writeln!(
        std::io::stderr(),
        "[workspace_pty] first workspace frame after {first:?}"
    );
    assert!(
        first < Duration::from_secs(15),
        "first frame after {first:?}"
    );
    let shown = term.text();
    // Inventory has one home: the aside lists each observed workflow and
    // its check, rather than repeating its count in the welcome object.
    for name in ["diamond.nika", "single.nika"] {
        let entries: Vec<_> = term
            .screen
            .lines()
            .into_iter()
            .filter(|row| row.contains(name))
            .collect();
        assert_eq!(entries.len(), 1, "{name}: {shown}");
        assert!(entries[0].contains("ok"), "{name}: {shown}");
    }
    assert!(!shown.contains("2 workflows · 2 clean"), "{shown}");
    let project = rig
        .project
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .expect("name")
        .to_owned();
    assert!(
        shown.contains(&project),
        "the header names the project\n{shown}"
    );
    assert!(shown.contains("this conversation"), "{shown}");
    assert!(!shown.contains("nika-tui-proto"), "{shown}");
    term.leave();
    rig.no_model_call("opening the workspace");
}

/// 2 · The selected workflow shows its source, plan, graph and check, every
/// face from the one read of its bytes (each says it shows the file as last
/// read, and none the hex of their witness); the second workflow shows its
/// own bytes.
#[test]
fn every_face_of_the_selected_workflow_reads_the_same_bytes() {
    let rig = Rig::new("faces");
    let mut term = rig.spawn("02-faces", 160, 36);
    wait_workspace(&mut term);
    let diamond = short_witness(DIAMOND.as_bytes());
    open_entry(&mut term, 1);
    term.wait_until("the diamond's source", |s| {
        s.contains("[source]") && s.contains("message: join")
    });
    let read = |text: &str| text.contains(AS_READ) && !text.contains(&diamond);
    assert!(read(&term.text()), "{}", term.text());
    to_object(&mut term);
    term.keys(RIGHT);
    term.wait_until("the diamond's plan", |s| {
        s.contains("[plan]") && s.contains("wave 2 · 2 side by side")
    });
    assert!(read(&term.text()), "{}", term.text());
    term.keys(RIGHT);
    term.wait_until("the diamond's graph", |s| {
        s.contains("[graph]") && s.contains("4 edges")
    });
    let graph = term.text();
    for edge in [
        "fetch → left · value",
        "fetch → right · value",
        "left → join · value",
        "right → join · value",
    ] {
        assert!(
            !graph.contains(edge),
            "the wired diamond repeats its edges: {edge}\n{graph}"
        );
    }
    assert_eq!(
        graph.matches('▼').count(),
        3,
        "one entry per dependent task\n{graph}"
    );
    assert!(
        graph.contains('┴') && graph.contains('┬'),
        "the split and join\n{graph}"
    );
    assert!(
        graph
            .lines()
            .any(|line| line.contains("│ left") && line.contains("│ right")),
        "the parallel tasks share a row\n{graph}"
    );
    assert!(!graph.contains("left → right"), "{graph}");
    assert!(
        graph.contains(GRAPH_READ) && !graph.contains(&diamond),
        "{graph}"
    );
    term.keys(RIGHT);
    term.wait_until("the diamond's check", |s| {
        s.contains("[check]") && s.contains("IMPORTS")
    });
    let check = term.text();
    assert!(check.contains("VALID"), "{check}");
    assert!(check.contains("RUN READY    unknown"), "{check}");
    assert!(!check.contains("nothing known blocks a run"), "{check}");
    assert!(read(&check), "{check}");
    // The second workflow: F6 from the object to the aside, one row down.
    term.keys(F6);
    term.keys(DOWN);
    term.keys("\r");
    let single = short_witness(SINGLE.as_bytes());
    term.wait_until("the single workflow", |s| {
        s.contains("single · [source] plan graph check") && s.contains("message: only")
    });
    let shown = term.text();
    assert!(
        shown.contains(AS_READ) && !shown.contains(&single),
        "{shown}"
    );
    assert!(!shown.contains("message: join"), "{shown}");
    term.leave();
    rig.no_model_call("reading every face");
}

/// The exact unsent fixture draft occupies only its composer row.
fn contextual_draft(screen: &vt::Screen) -> bool {
    screen.lines().iter().any(|row| {
        let Some((before, after)) = row.split_once("nika › unchanged draft") else {
            return false;
        };
        before.chars().all(|c| c.is_whitespace() || c == '│')
            && after
                .split('│')
                .next()
                .is_some_and(|text| text.trim().is_empty())
    })
}

/// Measure the painted conversation separator in display cells.
fn contextual_edge(screen: &vt::Screen) -> Option<usize> {
    use unicode_width::UnicodeWidthStr as _;
    screen.lines().iter().find_map(|row| {
        let prompt = "nika › unchanged draft";
        let start = row.find(prompt)? + prompt.len();
        let edge = start + row[start..].find('│')?;
        Some(row[..edge].width())
    })
}

/// The local action ends at the actual viewport's last display cell.
fn action_at_width(screen: &vt::Screen, action: &str, cols: u16) -> bool {
    use unicode_width::UnicodeWidthStr as _;
    screen.lines().iter().any(|row| {
        row.find(action)
            .is_some_and(|byte| row[..byte].width() + action.width() == usize::from(cols))
    })
}

/// Contextual expansion through bare `nika`, over one real, witnessed file.
/// The saved source, graph face and exact unsent composer survive pointer,
/// keyboard and native sizes. This calls no provider and starts no Run.
#[test]
fn the_selected_object_expands_without_leaving_the_real_conversation() {
    use unicode_width::UnicodeWidthStr as _;
    let rig = Rig::new("contextual");
    let before = rig.tree();
    let mut term = rig.spawn_with(
        "20-contextual",
        &[],
        180,
        48,
        &[("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    open_entry(&mut term, 1);
    to_object(&mut term);
    term.keys(RIGHT);
    term.keys(RIGHT);
    term.wait_workspace_frame("selected graph before expansion", |screen| {
        screen.contains("[graph]")
            && screen.contains(GRAPH_READ)
            && screen.contains("[+] Expand · F4")
    });
    term.keys(ESC);
    term.send("unchanged draft");
    for (cols, rows) in [(180, 48), (120, 40), (80, 24)] {
        term.resize(cols, rows);
        term.wait_workspace_frame(
            &format!("selected graph restored {cols}x{rows}"),
            |screen| {
                action_at_width(screen, "[+] Expand · F4", cols)
                    && screen.contains("[graph]")
                    && contextual_draft(screen)
                    && (screen.lines()[0].contains("[Conversation]") == (cols < 120))
            },
        );
        let restored_rule = term.screen.row_of("── ◌ this conversation");
        let restored_edge = contextual_edge(&term.screen);
        let restored_title = term.screen.row_of("this conversation");
        term.send(F4);
        term.wait_workspace_frame(
            &format!("selected graph expanded {cols}x{rows}"),
            |screen| {
                action_at_width(screen, "[-] Restore · F4", cols)
                    && screen.contains("[graph]")
                    && screen.contains(GRAPH_READ)
                    && contextual_draft(screen)
                    && if cols < 100 {
                        screen.row_of("── ◌ this conversation") > restored_rule
                    } else {
                        screen.row_of("── ◌ this conversation").is_none()
                            && screen.row_of("this conversation") == restored_title
                            && contextual_edge(screen)
                                .zip(restored_edge)
                                .is_some_and(|(expanded, restored)| expanded < restored)
                    }
            },
        );
        if cols < 100 {
            assert!(term.screen.row_of("── ◌ this conversation") > restored_rule);
        }
        let lines = term.screen.lines();
        let row = lines
            .iter()
            .position(|line| line.contains("[-] Restore · F4"))
            .expect("restore action");
        let byte = lines[row].find("[-] Restore · F4").expect("painted action");
        let column = lines[row][..byte].width() + 2;
        term.send(&format!(
            "\x1b[<0;{column};{}M\x1b[<0;{column};{}m",
            row + 1,
            row + 1
        ));
        term.wait_workspace_frame(
            &format!("pointer restored same graph {cols}x{rows}"),
            |screen| {
                action_at_width(screen, "[+] Expand · F4", cols)
                    && contextual_draft(screen)
                    && screen.row_of("── ◌ this conversation") == restored_rule
                    && contextual_edge(screen) == restored_edge
            },
        );
        assert!(!term.text().contains("Workbench"));
        assert!(term.screen.hues().is_empty());
    }
    assert_eq!(
        rig.tree(),
        before,
        "inspection never writes workflow or business files"
    );
    term.leave();
    for mode in [1000, 1002, 1003, 1006] {
        assert_eq!(term.screen.mode(mode), Some(false), "mouse mode {mode}");
    }
    rig.no_model_call("contextual expansion calls no provider");
}

/// Native colored output is recorded as bytes, with frame and resize offsets.
/// Opening and resizing this graph stays read-only and preserves its source.
#[test]
fn the_native_workflow_uses_the_approved_palette_without_changing_source() {
    let rig = Rig::new("colored-graph");
    let before = rig.tree();
    let mut term = rig.spawn_with(
        "21-colored-graph",
        &[],
        180,
        48,
        &[("NIKA_REDUCED_MOTION", "1")],
    );
    wait_workspace(&mut term);
    open_entry(&mut term, 1);
    to_object(&mut term);
    term.keys(RIGHT);
    term.keys(RIGHT);
    term.wait_workspace_frame("colored workflow at 180x48", |screen| {
        screen.contains("[graph]") && screen.contains(GRAPH_READ) && screen.contains("join")
    });
    assert!(
        term.screen.hues().contains("38;2;76;163;255"),
        "the approved electric-blue accent is emitted by the native renderer: {:?}",
        term.screen.hues()
    );
    assert!(
        term.screen.hues().contains("48;2;13;17;25"),
        "the approved base surface is native, not added by a capture viewer"
    );
    for (cols, rows) in [(120, 40), (80, 24), (180, 48)] {
        term.resize(cols, rows);
        term.wait_workspace_frame(&format!("colored workflow at {cols}x{rows}"), |screen| {
            screen.size() == (usize::from(cols), usize::from(rows))
                && screen.contains("[graph]")
                && screen.contains("nika ›")
                && (screen.lines()[0].contains("[Object]") == (cols < 120))
                && (cols != 80
                    || (screen.contains("Compact graph")
                        && !screen.contains("┌")
                        && [
                            "fetch · invoke",
                            "left ← fetch · invoke",
                            "right ← fetch · invoke",
                            "join ← left, right · invoke",
                        ]
                        .iter()
                        .all(|row| screen.lines().iter().any(|line| line.trim() == *row))
                        && screen.contains("Structure checked · this file only")
                        && screen.contains(GRAPH_READ)))
        });
    }
    term.keys(ESC);
    term.send("Make the brief more concise");
    term.wait_workspace_frame("unsent draft with Conversation focus", |screen| {
        screen.contains("Make the brief more concise") && screen.contains("[graph]")
    });
    term.keys(F6);
    term.wait_workspace_frame("same unsent draft with Object focus", |screen| {
        screen.contains("Make the brief more concise")
            && screen.contains("[graph]")
            && screen.contains(GRAPH_READ)
    });
    assert_eq!(
        rig.tree(),
        before,
        "viewing the graph changed project files"
    );
    term.leave();
    rig.no_model_call("viewing the graph with an unsent draft");
}

/// 3 · Changed bytes need a new look: until `r` the object shows the bytes it
/// read; after it, the new bytes and their own check, never the old one. An
/// invalid source stays visible beside its refusal. No face names bytes by
/// the hex of their witness.
#[test]
fn changed_and_invalid_bytes_need_a_new_look() {
    let rig = Rig::new("stale");
    let mut term = rig.spawn("03-stale", 160, 36);
    wait_workspace(&mut term);
    open_entry(&mut term, 2);
    term.wait_until("the single workflow", |s| s.contains("message: only"));
    let changed = SINGLE.replace("message: only", "message: changed");
    std::fs::write(rig.path("single.nika"), &changed).expect("change the bytes");
    to_object(&mut term);
    term.settle();
    let until = term.text();
    assert!(
        until.contains("message: only") && !until.contains("message: changed"),
        "until r, the read bytes\n{until}"
    );
    term.keys("r");
    term.wait_until("the new bytes", |s| s.contains("message: changed"));
    let shown = term.text();
    assert!(
        shown.contains(AS_READ) && !shown.contains("message: only"),
        "{shown}"
    );
    for bytes in [SINGLE, changed.as_str()] {
        assert!(!shown.contains(&short_witness(bytes.as_bytes())), "{shown}");
    }
    let invalid = "nika: single\nbogus: 1\ntasks: {}\n";
    std::fs::write(rig.path("single.nika"), invalid).expect("invalid bytes");
    term.keys("r");
    term.wait_until("the invalid source", |s| s.contains("bogus: 1"));
    term.keys(RIGHT);
    term.keys(RIGHT);
    term.keys(RIGHT);
    term.wait_until("the refusal", |s| {
        s.contains("[check]") && s.contains("NIKA-PARSE")
    });
    let check = term.text();
    assert!(
        check.contains(AS_READ) && !check.contains(&short_witness(invalid.as_bytes())),
        "{check}"
    );
    assert!(
        !check.contains("IMPORTS"),
        "a refused file has no layers to qualify\n{check}"
    );
    term.keys(UP);
    term.leave();
    rig.no_model_call("a new look at changed and invalid bytes");
}

/// 4 · The workspace follows 120x36, 80x24 and 60x18 (the folded aside is
/// reachable over the object), gives way to the focus view below its minimum
/// and comes back whole with the object and the focus kept.
#[test]
fn the_workspace_follows_every_size_and_comes_back() {
    let rig = Rig::new("sizes");
    let mut term = rig.spawn("04-sizes", 120, 36);
    wait_workspace(&mut term);
    open_entry(&mut term, 1);
    to_object(&mut term);
    term.keys(RIGHT);
    term.keys(RIGHT);
    term.wait_until("the graph at 120x36", |s| {
        s.contains("[graph]") && s.contains("│ join")
    });
    term.resize(80, 24);
    // Wait for the resized frame before End uses its object viewport.
    term.wait_until("the stacked frame at 80x24", |s| {
        s.lines()[12].contains("this conversation")
    });
    term.keys(END);
    term.wait_until("the graph at 80x24", |s| {
        s.contains("[graph]")
            && s.contains("join ← left, right · invoke")
            && s.contains("this conversation")
    });
    term.resize(60, 18);
    term.wait_until("the stacked frame at 60x18", |s| {
        s.lines()[9].contains("this conversation")
    });
    term.keys(END);
    term.wait_until("the graph at 60x18", |s| {
        s.contains("[graph]") && s.contains("join ← left, right · invoke")
    });
    for line in term.screen.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(line.as_str()) <= 60,
            "{line}"
        );
    }
    term.resize(50, 14);
    term.wait_until("the focus view below the minimum", |s| {
        !s.contains("[graph]") && s.contains("nika ›")
    });
    term.resize(40, 14);
    let redraw_from = term.raw.len();
    term.send("\x0c");
    let redraw_started = Instant::now();
    while !term.raw[redraw_from..]
        .windows(4)
        .any(|bytes| bytes == b"\x1b[2J")
    {
        assert!(redraw_started.elapsed() < WAIT, "{}", term.dump());
        term.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    term.wait_until("a fresh focus frame at 40 columns", |s| {
        !s.contains("[graph]") && s.contains("nika ›")
    });
    term.shot("narrow focus frame");
    term.resize(120, 36);
    term.wait_until("the workspace back, object kept", |s| {
        s.contains("[graph]") && s.contains("diamond.nika") && s.contains("│ join")
    });
    // The folded aside at 80x24 is reachable over the object and opens.
    term.resize(80, 24);
    term.wait_text("[graph]");
    term.keys(F6);
    term.wait_until("the aside over the object", |s| {
        s.contains("single.nika") && !s.contains("[graph]")
    });
    term.keys(DOWN);
    term.keys("\r");
    term.wait_until("single opened at 80x24", |s| {
        s.contains("single · [source] plan graph check")
    });
    term.keys(ESC);
    term.leave();
    rig.no_model_call("following every size");
}

/// 5 · ASCII, `NO_COLOR` and reduced motion keep every face readable: the
/// workspace rows are ASCII and no colour reaches the terminal.
#[test]
fn ascii_no_color_and_reduced_motion_keep_every_face_readable() {
    let rig = Rig::new("ascii");
    let env = [("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")];
    let mut term = rig.spawn_with("05-ascii", &["--ascii"], 120, 36, &env);
    wait_workspace(&mut term);
    open_entry(&mut term, 1);
    to_object(&mut term);
    term.keys(RIGHT);
    term.keys(RIGHT);
    term.wait_until("the ASCII graph", |s| {
        s.contains("[graph]") && s.contains("join")
    });
    let shown = term.text();
    assert_renderer_ascii(&shown);
    term.keys(RIGHT);
    term.wait_until("the ASCII check", |s| {
        s.contains("[check]") && s.contains("IMPORTS")
    });
    assert_renderer_ascii(&term.text());
    term.resize(80, 24);
    term.keys(LEFT);
    term.wait_workspace_frame("the compact ASCII graph", |screen| {
        screen.size() == (80, 24)
            && screen.contains("Compact graph")
            && screen.contains("[graph]")
            && screen.contains("[Object]")
            && screen.contains("Structure checked - this file only")
            && screen.contains(GRAPH_READ)
            && [
                "fetch - invoke",
                "left <- fetch - invoke",
                "right <- fetch - invoke",
                "join <- left, right - invoke",
            ]
            .iter()
            .all(|row| screen.lines().iter().any(|line| line.trim() == *row))
    });
    assert_renderer_ascii(&term.text());
    let raw = String::from_utf8_lossy(&term.raw).into_owned();
    for hue in ["\x1b[38;", "\x1b[48;", "\x1b[31m", "\x1b[32m", "\x1b[33m"] {
        assert!(!raw.contains(hue), "a colour under NO_COLOR: {hue:?}");
    }
    term.leave();
    rig.no_model_call("reading the ASCII faces");
}

/// The renderer's own glyphs take their ASCII twins (the session's words,
/// such as its banner, stay as the session wrote them).
fn assert_renderer_ascii(shown: &str) {
    // The lifecycle rail (`Draft ○ · Saved ○ …`) is the Session's own words,
    // drawn as given under --ascii in every presentation: not checked here.
    for glyph in [
        '◆', '─', '│', '┬', '╰', '▶', '⑂', '◌', '▱', '⌄', '›', '✔', '…', '←',
    ] {
        assert!(!shown.contains(glyph), "{glyph:?} under --ascii\n{shown}");
    }
}

/// The right preview at 120 columns (21 navigation + 47 conversation).
/// Count display cells: Unicode in the conversation must not shift the slice.
fn right_preview(screen: &vt::Screen) -> Vec<String> {
    assert_eq!(screen.size().0, 120);
    screen
        .lines()
        .into_iter()
        .map(|line| {
            let mut cells = 0;
            line.chars()
                .skip_while(|c| {
                    if cells >= 68 {
                        return false;
                    }
                    cells += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                    true
                })
                .collect()
        })
        .collect()
}

/// Read wrapped preview prose without including the adjacent conversation.
fn preview_text(screen: &vt::Screen) -> String {
    right_preview(screen)
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The centre conversation at the same 120-column geometry. This reads the
/// actual decision region, so duplicate facts in the object cannot mask a
/// missing review. Card edges are not part of the words being checked.
fn review_text(screen: &vt::Screen) -> String {
    review_rows(screen)
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Read each row in that same conversation region, without its frame edges.
fn review_rows(screen: &vt::Screen) -> Vec<String> {
    assert_eq!(screen.size().0, 120);
    screen
        .lines()
        .into_iter()
        .map(|line| {
            let mut cell = 0;
            line.chars()
                .filter(|glyph| {
                    let at = cell;
                    cell += unicode_width::UnicodeWidthChar::width(*glyph).unwrap_or(0);
                    (21..68).contains(&at)
                })
                .collect::<String>()
                .replace('│', " ")
        })
        .collect()
}

/// 6 · Words typed before the first frame land in the draft and are never
/// sent. While the aside holds the keys the hint row names it; `Esc` gives
/// the keys back, and so does a letter typed there: the composer takes it, so
/// no typed word is lost. `Ctrl+L` repaints with the draft intact.
#[test]
fn typed_ahead_words_stay_in_the_draft_and_the_keys_follow_the_focus() {
    let rig = Rig::new("keys");
    let mut term = rig.spawn("06-keys", 120, 36);
    term.send("hello");
    wait_workspace(&mut term);
    term.wait_until("the typed-ahead draft", |s| {
        s.lines()
            .iter()
            .any(|l| l.contains("nika ›") && l.contains("hello"))
    });
    term.keys(SHIFT_F6);
    term.wait_text("keys: the project list");
    term.keys(ESC);
    term.wait_until("Esc gave the keys back", |s| {
        !s.contains("keys: the project list")
    });
    term.keys(SHIFT_F6);
    term.wait_text("keys: the project list");
    term.keys("xyz");
    term.keys("\x0c");
    term.wait_until("the letters typed in the aside, in the draft", |s| {
        !s.contains("keys: the project list")
            && s.lines()
                .iter()
                .any(|l| l.contains("nika ›") && l.contains("helloxyz"))
    });
    let shown = term.text();
    assert_eq!(
        shown.matches("hello").count(),
        1,
        "the draft is in the box only, never echoed as sent\n{shown}"
    );
    term.leave();
    rig.no_model_call("a typed-ahead draft is never sent");
}

/// 7 · `SIGTERM` leaves at once with 143 and the terminal handed back.
#[test]
fn terminate_restores_the_terminal() {
    let rig = Rig::new("term");
    let mut term = rig.spawn("07-sigterm", 120, 36);
    wait_workspace(&mut term);
    open_entry(&mut term, 1);
    term.wait_text("[source]");
    term.signal(Signal::SIGTERM);
    let status = term.finish();
    assert_eq!(exit_code(status), Some(143), "{status:?}\n{}", term.dump());
    term.assert_restored();
    rig.no_model_call("opening a source before SIGTERM");
}

/// 8 · Inline stays an explicit door: `NIKA_TUI=inline` opens it on the main
/// screen; `Ctrl+T` reaches the workspace and `Esc` comes back.
#[test]
fn inline_stays_an_explicit_door() {
    let rig = Rig::new("inline");
    let mut term = rig.spawn_with("08-inline", &[], 120, 36, &[("NIKA_TUI", "inline")]);
    term.wait_until("the inline door", |s| !s.on_alt() && s.contains("nika ›"));
    term.keys("\x14");
    wait_workspace(&mut term);
    term.keys(ESC);
    term.wait_until("back inline", |s| !s.on_alt() && s.contains("nika ›"));
    term.leave();
    rig.no_model_call("switching presentations");
}

/// 9 · Plain stays plain (`NIKA_TUI=0`: no alternate screen) and a pipe keeps
/// the concierge (exit 0, no terminal mode).
#[test]
fn plain_and_pipe_stay_as_they_are() {
    let rig = Rig::new("plain");
    let mut term = rig.spawn_with("09-plain", &[], 120, 36, &[("NIKA_TUI", "0")]);
    term.wait_until("the plain prompt", |s| s.contains("nika ›"));
    assert!(!term.screen.on_alt(), "{}", term.dump());
    assert!(!String::from_utf8_lossy(&term.raw).contains("\x1b[?1049h"));
    term.send("/quit\r");
    let status = term.finish();
    assert_eq!(exit_code(status), Some(0), "{status:?}\n{}", term.dump());
    let out = Command::new(bin())
        .current_dir(rig.project.path())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", rig.home.path())
        .env("NIKA_KEYCHAIN", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("piped nika");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\x1b[?1049h") && !stdout.contains("\x1b[?2004h"),
        "{stdout}"
    );
}

/// 10 · Opening and reading every face sends nothing and grants nothing: the
/// project's files are byte-identical, no proposal, save or run is said, and
/// the inline lifecycle rail still says nothing was drafted, saved or run.
#[test]
fn opening_a_viewer_sends_nothing_and_grants_nothing() {
    let rig = Rig::new("authority");
    let before = rig.tree();
    let mut term = rig.spawn("10-authority", 120, 36);
    wait_workspace(&mut term);
    open_entry(&mut term, 1);
    to_object(&mut term);
    for _ in 0..4 {
        term.keys(RIGHT);
    }
    term.keys("r");
    term.wait_text("[source]");
    let shown = term.text();
    for said in ["proposes", "saved", "Save?", "run it", "observed"] {
        assert!(!shown.contains(said), "{said}\n{shown}");
    }
    // A decision's prompt is `action ›` now (`Save?` was): none is painted.
    assert!(
        !action_prompt_shown(&term.screen),
        "no decision holds the line\n{shown}"
    );
    // The workspace hides a rail whose five stages are all pending. Inspect
    // the same retained state inline without sending a Session line.
    term.keys("\x14");
    term.wait_until("all five stages remain pending", |screen| {
        !screen.on_alt()
            && screen.contains("Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○")
            && screen.contains("nika ›")
    });
    assert!(
        !rig.path(".nika/traces").exists(),
        "inspection starts no Run"
    );
    term.leave();
    assert_eq!(rig.tree(), before, "the project is untouched");
    rig.no_model_call("opening a viewer sends nothing");
}

/// 11 · Save is never Run in the workspace: a sentence becomes a proposal,
/// `oui` saves its exact bytes, and nothing runs until a separate line asks.
#[test]
fn save_is_never_run_in_the_workspace() {
    let rig = Rig::new("save");
    let brief = "# Brief\n\nLe lancement passe en octobre.\n";
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), brief).expect("brief");
    let mut term = rig.spawn("11-save", 120, 36);
    wait_workspace(&mut term);
    term.send("Lis ./notes/brief.md et écris-le dans ./out/copie.md\r");
    // The consent holds the line as an action; it still reads typed words.
    term.wait_until("the proposal's decision", action_holds);
    term.send("oui\r");
    let landed = rig.path("compiled-workflow.nika");
    term.wait_until("the exact bytes saved", |s| {
        landed.exists() && s.contains("nika ›")
    });
    for _ in 0..4 {
        term.settle();
    }
    assert!(
        rig.path("compiled-workflow.nika").exists(),
        "{}",
        term.dump()
    );
    assert!(
        !rig.path("out/copie.md").exists(),
        "a save ran the workflow\n{}",
        term.dump()
    );
    term.leave();
    assert!(
        !rig.path("out/copie.md").exists(),
        "nothing ran before the door closed"
    );
    rig.no_model_call("a deterministic copy proposed and saved");
}

/// The identity a `yes` answers, as the Session's words name it (`identity <12 hex> · …`),
/// when `text` holds them: read word by word, across the reader's wrapped rows and edges.
fn said_identity(text: &str) -> Option<String> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '│')
        .filter(|word| !word.is_empty())
        .collect();
    words.windows(2).find_map(|pair| {
        let id = pair[1];
        (pair[0] == "identity" && id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| id.to_owned())
    })
}

/// The current proposal's whole words, read where a person reads them (`F2`), page by page
/// to their end, then `Esc` back to the decision. The card names the proposal by where it
/// lands; its identity stays in those words.
fn read_proposal(term: &mut Term) -> String {
    term.send(F2);
    term.wait_workspace_frame("the proposal's whole words", |s| {
        s.lines()[0].contains("Full proposal")
    });
    let mut read = term.text();
    let mut last = read.clone();
    for _ in 0..12 {
        term.keys(PAGE_DOWN);
        let page = term.text();
        if page == last {
            break;
        }
        read.push('\n');
        read.push_str(&page);
        last = page;
    }
    term.send(ESC);
    term.wait_workspace_frame("the decision again", |s| {
        !s.lines()[0].contains("Full proposal") && action_holds(s)
    });
    read
}

/// The whole words of the proposal a revision brought, once they say `marker`: the decision
/// may still hold the earlier proposal while the Session revises it.
fn revised_proposal(term: &mut Term, marker: &str) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        term.wait_workspace_frame("a decision", action_holds);
        let words = read_proposal(term);
        if words.contains(marker) {
            return words;
        }
        assert!(
            Instant::now() <= deadline,
            "the revision never said {marker}\n{}",
            term.dump()
        );
        term.settle();
    }
}

/// The source rows the object shows (`<n> <line>`, a verb's line marked `◆`, a long one cut
/// on `…`), each by its number and its words.
fn source_rows(screen: &vt::Screen) -> Vec<(usize, String)> {
    (right_preview(screen).iter())
        .filter_map(|row| {
            let (number, rest) = row.trim().split_once(' ')?;
            let number: usize = number.parse().ok()?;
            let words = rest.trim_start().trim_start_matches('◆').trim();
            Some((number, words.to_owned()))
        })
        .collect()
}

/// Whether every source row a face showed (at least `least` of them) is the line of `saved`
/// at that number, a cut row its start: the bytes a person read before the yes are the ones
/// saved.
fn shows_source(shown: &[(usize, String)], saved: &str, least: usize) -> bool {
    let lines: Vec<&str> = saved.lines().collect();
    shown.len() >= least
        && shown.iter().all(|(number, words)| {
            let line = number.checked_sub(1).and_then(|at| lines.get(at));
            line.is_some_and(|line| match words.strip_suffix('…') {
                Some(start) => line.trim().starts_with(start.trim_end()),
                None => line.trim() == words,
            })
        })
}

/// The observed graph opens first: read the source, then every other face (none of these
/// navigation keys is a consent), and give the keys back to the conversation. The source
/// face's rows, by number.
fn walk_faces(term: &mut Term) -> Vec<(usize, String)> {
    term.keys(F6);
    term.keys(LEFT);
    term.keys(LEFT);
    term.wait_text("[source]");
    let source = source_rows(&term.screen);
    for face in ["[plan]", "[graph]", "[check]"] {
        term.keys(RIGHT);
        term.wait_text(face);
    }
    term.wait_text("IMPORTS");
    assert!(
        !term.text().contains("nothing known blocks a run"),
        "{}",
        term.dump()
    );
    // From the object, `Esc` gives the keys back to the conversation (never
    // to the decision, which `Esc` never answers).
    term.keys(ESC);
    source
}

/// 12 · A sentence becomes a candidate the workspace shows before any Save:
/// the proposal a yes answers by where it lands, what it creates, what it
/// reaches, whose pending bytes the object shows, and its four faces, while
/// nothing is written. A revision of its ceiling is a new identity over the
/// same bytes (read in the Session's whole words, `F2`) and the old one leaves
/// the object; `yes` lands exactly the bytes its source face showed and
/// nothing runs until a separate line asks.
#[test]
fn the_candidate_is_inspected_and_revised_before_a_separate_save() {
    let rig = Rig::new("candidate");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\n").expect("brief");
    let before = rig.tree();
    let mut term = rig.spawn("12-candidate", 120, 36);
    wait_workspace(&mut term);
    term.send("Read ./notes/brief.md and write it to ./out/copy.md\r");
    // The consent's decision holds the line; the proposal a yes answers must
    // stay in view beside it (its card's border in the conversation), by name.
    let named = "compiled-workflow.nika · F2";
    term.wait_workspace_frame("the candidate in view", |s| {
        action_holds(s) && review_text(s).contains(named)
    });
    let preview = preview_text(&term.screen);
    let review = review_text(&term.screen);
    assert!(
        review.contains("compiled-workflow.nika · F2: whole words"),
        "the proposal's name beside its reader key on the card's border\n{}",
        term.dump()
    );
    let a = said_identity(&read_proposal(&mut term)).expect("the identity a yes answers");
    assert!(
        !term.text().contains(&a),
        "the identity stays in the Session's words\n{}",
        term.dump()
    );
    for said in [
        "Not saved yet",
        "creates compiled-workflow.nika",
        "when it runs",
        "rehearsal",
    ] {
        assert!(
            review.contains(said),
            "missing review fact {said}\n{}",
            term.dump()
        );
        assert!(
            !preview.contains(said),
            "duplicated review fact {said}\n{}",
            term.dump()
        );
    }
    // The object's header is the one home of whose pending bytes these are.
    assert!(
        preview.contains("these bytes, the proposal's own") && !review.contains("these bytes"),
        "the pending bytes once, in the object\n{}",
        term.dump()
    );
    assert!(
        preview.contains("[graph]"),
        "the object retains its face\n{}",
        term.dump()
    );
    assert_eq!(rig.tree(), before, "a proposal writes nothing");
    let source = walk_faces(&mut term);
    // A revision of the proposal's money, typed at the decision (it reads
    // words): a new identity, A gone from the object.
    term.send("budget 0.10 USD\r");
    let revised = revised_proposal(&mut term, "money: $0.10 USD");
    assert!(
        !revised.contains(&a),
        "a revision is another proposal\n{revised}"
    );
    assert_eq!(rig.tree(), before, "a revision writes nothing");
    // The typed consent answers the decision painted now: B's.
    term.send("yes\r");
    let landed = rig.path("compiled-workflow.nika");
    term.wait_workspace_frame("the exact bytes saved", |s| {
        landed.exists() && s.contains("as last read")
    });
    let saved = std::fs::read_to_string(&landed).expect("landed");
    assert!(
        shows_source(&source, &saved, 20),
        "the saved bytes are the ones shown before the yes:\n{saved}\nshown: {source:#?}\n{}",
        term.dump()
    );
    assert!(
        !review_text(&term.screen).contains(named),
        "a saved candidate is no longer one\n{}",
        term.dump()
    );
    for _ in 0..4 {
        term.settle();
    }
    assert!(!rig.path("out/copy.md").exists(), "a save ran the workflow");
    term.leave();
    assert!(
        !rig.path("out/copy.md").exists(),
        "nothing ran before the door closed"
    );
    rig.no_model_call("a candidate proposed, revised and saved");
}
