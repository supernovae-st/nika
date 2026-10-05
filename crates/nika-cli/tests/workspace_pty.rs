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
//! source, plan, graph and check are drawn from ONE read of its bytes, named
//! by their witness. Changed or invalid bytes need a new look (`r`) and never
//! borrow the old check. The workspace follows every size, gives way to the
//! focus view below its minimum and comes back whole; ASCII, `NO_COLOR` and
//! reduced motion keep every face readable; the keys follow the focus and a
//! typed-ahead draft is never sent; an interruption or `SIGTERM` gives the
//! terminal back. Inline is an explicit door (`NIKA_TUI=inline`), plain and
//! pipe stay as they are, opening a viewer sends nothing to a model and
//! grants nothing, and Save is never Run.
//!
//! Keyless: the kept choice is « no AI in this conversation », the PATH holds
//! no harness executable, the keychain is off. The screen is read through the
//! VT model of the TUI reception suites (`nika-tui/tests/qa_support/vt.rs`),
//! which answers the renderer's terminal queries itself. With
//! `NIKA_TUI01_EVIDENCE=<dir>` every proof keeps its raw stream, its labelled
//! screens and the SHA-256 of the binary it drove there.

#[path = "../../nika-tui/tests/qa_support/vt.rs"]
#[allow(
    dead_code,
    reason = "the workspace suite reads part of the shared VT screen"
)]
mod vt;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use expectrl::process::unix::{Signal, WaitStatus};
use expectrl::session::OsSession;
use sha2::{Digest as _, Sha256};

/// The longest any single wait may take before the proof fails with a dump.
const WAIT: Duration = Duration::from_secs(30);
/// A pause long enough for the renderer to read a key and draw.
const SETTLE: Duration = Duration::from_millis(400);

const F6: &str = "\x1b[17~";
const SHIFT_F6: &str = "\x1b[17;2~";
const END: &str = "\x1b[F";
const PAGE_UP: &str = "\x1b[5~";
const DOWN: &str = "\x1b[B";
const UP: &str = "\x1b[A";
const RIGHT: &str = "\x1b[C";
const ESC: &str = "\x1b";

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

/// A project with the two workflows and a home whose kept choice is « no AI
/// in this conversation ».
struct Rig {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Rig {
    fn new(tag: &str) -> Self {
        let project = tempfile::Builder::new()
            .prefix(&format!("nika-ws-{tag}-"))
            .tempdir()
            .expect("project dir");
        let home = tempfile::Builder::new()
            .prefix(&format!("nika-ws-home-{tag}-"))
            .tempdir()
            .expect("home dir");
        std::fs::create_dir_all(home.path().join(".nika")).expect("home .nika");
        std::fs::write(
            home.path().join(".nika").join("session-intelligence.json"),
            "{\"kind\":{\"kind\":\"none\"},\"model\":null,\"chosen_at\":\"2026-09-20T00:00:00Z\"}",
        )
        .expect("kept choice");
        std::fs::write(project.path().join("diamond.nika"), DIAMOND).expect("diamond");
        std::fs::write(project.path().join("single.nika"), SINGLE).expect("single");
        Self { project, home }
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
            .env("NIKA_KEYCHAIN", "off");
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
    eof: bool,
    shots: Vec<(String, String)>,
    started: Instant,
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
            eof: false,
            shots: Vec::new(),
            started: Instant::now(),
        }
    }

    fn pump(&mut self) -> usize {
        let mut total = 0;
        let mut buf = [0u8; 16 * 1024];
        while !self.eof {
            match self.pty.try_read(&mut buf) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    total += n;
                    self.raw.extend_from_slice(&buf[..n]);
                    self.screen.feed(&buf[..n]);
                    for reply in self.screen.take_replies() {
                        let _ = self.pty.write_all(&reply);
                        let _ = self.pty.flush();
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

    fn settle(&mut self) {
        let deadline = Instant::now() + SETTLE;
        while Instant::now() < deadline && !self.eof {
            if self.pump() == 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn send(&mut self, text: &str) {
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

    /// Two `Ctrl+C` while idle leave with 130 and the terminal restored.
    fn leave(&mut self) {
        self.keys("\x03");
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
            && screen.contains("describe work")
    });
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
    term.wait_text("2 workflows · 2 clean");
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
    assert!(shown.contains("2 workflows · 2 clean"), "{shown}");
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
}

/// 2 · The selected workflow shows its source, plan, graph and check, every
/// face from the one read of its bytes (their witness on every face); the
/// second workflow shows its own bytes.
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
    assert!(term.text().contains(&diamond), "{}", term.text());
    to_object(&mut term);
    term.keys(RIGHT);
    term.wait_until("the diamond's plan", |s| {
        s.contains("[plan]") && s.contains("wave 2 · 2 side by side")
    });
    assert!(term.text().contains(&diamond));
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
        assert!(graph.contains(edge), "{edge}\n{graph}");
    }
    assert!(
        graph
            .lines()
            .any(|line| line.contains("◆ left") && line.contains("◆ right")),
        "the parallel tasks share a row\n{graph}"
    );
    assert!(!graph.contains("left → right"), "{graph}");
    assert!(graph.contains(&diamond));
    term.keys(RIGHT);
    term.wait_until("the diamond's check", |s| {
        s.contains("[check]") && s.contains("IMPORTS")
    });
    let check = term.text();
    assert!(check.contains("VALID"), "{check}");
    assert!(check.contains("RUN READY    unknown"), "{check}");
    assert!(!check.contains("nothing known blocks a run"), "{check}");
    assert!(check.contains(&diamond));
    // The second workflow: F6 from the object to the aside, one row down.
    term.keys(F6);
    term.keys(DOWN);
    term.keys("\r");
    let single = short_witness(SINGLE.as_bytes());
    term.wait_until("the single workflow", |s| {
        s.contains("[source] plan graph check · single") && s.contains("message: only")
    });
    let shown = term.text();
    assert!(shown.contains(&single), "{shown}");
    assert!(
        !shown.contains(&diamond) && !shown.contains("message: join"),
        "{shown}"
    );
    term.leave();
}

/// 3 · Changed bytes need a new look: until `r` the object names the bytes it
/// read; after it, the new bytes and their own check, never the old one. An
/// invalid source stays visible beside its refusal.
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
    assert!(
        term.text().contains(&short_witness(SINGLE.as_bytes())),
        "until r, the read bytes"
    );
    term.keys("r");
    term.wait_until("the new bytes", |s| s.contains("message: changed"));
    let shown = term.text();
    assert!(
        shown.contains(&short_witness(changed.as_bytes())),
        "{shown}"
    );
    assert!(
        !shown.contains(&short_witness(SINGLE.as_bytes())),
        "{shown}"
    );
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
        check.contains(&short_witness(invalid.as_bytes())),
        "{check}"
    );
    assert!(
        !check.contains("IMPORTS"),
        "a refused file has no layers to qualify\n{check}"
    );
    term.keys(UP);
    term.leave();
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
        s.contains("[graph]") && s.contains("◆ join")
    });
    term.resize(80, 24);
    // Wait for the resized frame before End uses its object viewport.
    term.wait_until("the stacked frame at 80x24", |s| {
        s.lines()[12].contains("this conversation")
    });
    term.keys(END);
    term.wait_until("the graph at 80x24", |s| {
        s.contains("[graph]") && s.contains("◆ join") && s.contains("this conversation")
    });
    term.resize(60, 18);
    term.wait_until("the stacked frame at 60x18", |s| {
        s.lines()[9].contains("this conversation")
    });
    term.keys(END);
    term.wait_until("the graph at 60x18", |s| {
        s.contains("[graph]") && s.contains("◆ join")
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
    term.resize(120, 36);
    term.wait_until("the workspace back, object kept", |s| {
        s.contains("[graph]") && s.contains("diamond.nika") && s.contains("◆ join")
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
        s.contains("[source] plan graph check · single")
    });
    term.keys(ESC);
    term.leave();
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
    let raw = String::from_utf8_lossy(&term.raw).into_owned();
    for hue in ["\x1b[38;", "\x1b[48;", "\x1b[31m", "\x1b[32m", "\x1b[33m"] {
        assert!(!raw.contains(hue), "a colour under NO_COLOR: {hue:?}");
    }
    term.leave();
}

/// The renderer's own glyphs take their ASCII twins (the session's words,
/// such as its banner, stay as the session wrote them).
fn assert_renderer_ascii(shown: &str) {
    // The lifecycle rail (`Draft ○ · Saved ○ …`) is the Session's own words,
    // drawn as given under --ascii in every presentation: not checked here.
    for glyph in [
        '◆', '─', '│', '┬', '╰', '▶', '⑂', '◌', '▱', '⌄', '›', '✔', '…',
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

/// 6 · Words typed before the first frame land in the draft and are never
/// sent; a region holding the keys reads them instead of the composer; `Esc`
/// gives the keys back; `Ctrl+L` repaints with the draft intact.
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
    term.keys("xyz");
    term.keys(ESC);
    term.keys("\x0c");
    term.wait_until("the draft after the aside held the keys", |s| {
        s.lines()
            .iter()
            .any(|l| l.contains("nika ›") && l.contains("hello") && !l.contains("xyz"))
    });
    let shown = term.text();
    assert_eq!(
        shown.matches("hello").count(),
        1,
        "the draft is in the box only, never echoed as sent\n{shown}"
    );
    term.leave();
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
/// the lifecycle rail still says nothing was drafted, saved or run.
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
    assert!(shown.contains("Draft ○ · Saved ○ · Checked ○"), "{shown}");
    term.leave();
    assert_eq!(rig.tree(), before, "the project is untouched");
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
    term.wait_until("the proposal", |s| s.contains("Save?"));
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
}

/// The candidate identity the object region shows (`proposal <12 hex> · what
/// a yes answers`), when one is shown.
fn shown_identity(screen: &str) -> Option<String> {
    let tail = " · what a yes answers";
    screen.lines().find_map(|line| {
        let at = line.find(tail)?;
        let id = line[..at].rsplit("proposal ").next()?;
        (id.len() == 12 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_owned())
    })
}

/// The witness of the candidate's pending bytes the object region shows
/// (`these bytes <12 hex>, the proposal's own`), when it shows one.
fn shown_bytes(screen: &str) -> Option<String> {
    let head = "these bytes ";
    screen.lines().find_map(|line| {
        let at = line.find(head)? + head.len();
        let rest = line.get(at..)?;
        let witness = rest.get(..12)?;
        let own = rest.get(12..)?.starts_with(", the proposal");
        (own && witness.chars().all(|c| c.is_ascii_hexdigit())).then(|| witness.to_owned())
    })
}

/// 12 · A sentence becomes a candidate the workspace shows before any Save:
/// the identity a yes answers, what it creates, what it reaches, the witness
/// of its exact pending bytes, and its four faces, while nothing is written. A
/// revision of its ceiling is a new identity over the same bytes and the old
/// one leaves the object; `yes` lands exactly those bytes (the saved file then
/// shows the same witness) and nothing runs until a separate line asks.
#[test]
fn the_candidate_is_inspected_and_revised_before_a_separate_save() {
    let rig = Rig::new("candidate");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\n").expect("brief");
    let before = rig.tree();
    let mut term = rig.spawn("12-candidate", 120, 36);
    wait_workspace(&mut term);
    term.send("Read ./notes/brief.md and write it to ./out/copy.md\r");
    term.wait_until("the candidate in view", |s| {
        s.contains("Save?") && s.contains("what a yes answers")
    });
    let shown = term.text();
    let a = shown_identity(&shown).expect("an identity is shown");
    let preview = preview_text(&term.screen);
    for said in [
        "not saved",
        "creates compiled-workflow.nika",
        "when it runs",
        "rehearsal",
        "[graph]",
    ] {
        assert!(preview.contains(said), "{said}\n{}", term.dump());
    }
    assert_eq!(rig.tree(), before, "a proposal writes nothing");
    // The observed graph opens first. Read Source, then all the other faces;
    // none of these navigation keys is a consent.
    term.keys(F6);
    term.keys(LEFT);
    term.keys(LEFT);
    term.wait_text("[source]");
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
    term.keys(ESC);
    // A revision of the proposal's money: a new identity, A gone from the object.
    term.send("budget 0.10 USD\r");
    term.wait_until("the revised candidate", |s| {
        shown_identity(&s.lines().join("\n")).is_some_and(|id| id != a)
    });
    let b = shown_identity(&term.text()).expect("B");
    let witness_row = shown_bytes(&term.text()).expect("the pending bytes' witness");
    assert_eq!(rig.tree(), before, "a revision writes nothing");
    term.send("yes\r");
    let landed = rig.path("compiled-workflow.nika");
    term.wait_until("the exact bytes saved", |s| {
        landed.exists() && s.contains("as last read")
    });
    let saved = short_witness(&std::fs::read(&landed).expect("landed"));
    assert!(
        witness_row == saved,
        "the saved bytes are the ones shown before the yes: {witness_row} vs {saved}\n{}",
        term.dump()
    );
    assert!(term.text().contains(&saved), "{}", term.dump());
    assert_ne!(
        shown_identity(&term.text()),
        Some(b),
        "a saved candidate is no longer one"
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
}

/// Two keyless tasks a second apart: the child's frames arrive over time.
const SLOW: &str = r#"nika: slow
permits:
  tools: ["nika:wait"]
tasks:
  first:
    invoke: { tool: "nika:wait", args: { duration: "1s" } }
  second:
    with: { x: "${{ tasks.first.output }}" }
    invoke: { tool: "nika:wait", args: { duration: "1s" } }
"#;

/// 13 · A run asked in the workspace is followed from its own frames: the
/// leg names the execution its first frame carries, binds the graph to the
/// bytes the run names (their sha256), and shows each task as the child
/// reports it while the run has not settled; the settlement and the
/// stream's wholeness come last, apart from the proof. The runtime reports a
/// task's start with its end, so a task shows done or not yet, never a
/// guessed « running ».
#[test]
fn a_run_is_followed_from_its_frames_before_it_settles() {
    let rig = Rig::new("live");
    std::fs::write(rig.path("slow.nika"), SLOW).expect("slow");
    let mut term = rig.spawn("13-live", 120, 36);
    wait_workspace(&mut term);
    term.send("run slow.nika\r");
    term.wait_until("the leg bound to the bytes it runs", |s| {
        s.contains("graph · the bytes") && s.contains("it was asked over")
    });
    term.wait_until("the first task done, the run not settled", |s| {
        s.contains("✔ first") && s.contains("○ second") && !s.contains("settled · succeeded")
    });
    term.wait_until("the settlement", |s| s.contains("settled · succeeded"));
    let shown = term.text();
    for said in [
        "events and the settlement, whole",
        "evidence · unsealed, as the run declared it",
        "✔ second",
    ] {
        assert!(shown.contains(said), "{said}\n{}", term.dump());
    }
    assert!(!shown.contains("graph not bound"), "{}", term.dump());
    term.leave();
}

/// A read and a write with an output: what the run left is found from its own frames.
const COPY: &str = r#"nika: copy
permits:
  fs: { read: ["./notes/brief.md"], write: ["./out/copy.md"] }
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke: { tool: "nika:read", args: { path: "./notes/brief.md" } }
  write_output:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/copy.md", content: "${{ with.text }}" } }
outputs:
  written: ${{ tasks.write_output.output }}
"#;

/// The run's label the object region shows (`run <12 hex>`), when it shows one.
fn run_label(screen: &str) -> Option<String> {
    screen.lines().find_map(|line| {
        line.match_indices("run ").find_map(|(at, _)| {
            let hex = line.get(at + 4..at + 16)?;
            hex.chars()
                .all(|c| c.is_ascii_hexdigit())
                .then(|| format!("run {hex}"))
        })
    })
}

/// Visit the reopened conversation's history, then return to its latest row.
fn visit_reopened_history(term: &mut Term, label: &str) {
    // History may span several cards: prove both markers are reachable,
    // without requiring them to occupy the same page or stay pinned forever.
    let (mut saw_history, mut saw_run) = (false, false);
    for page in 0..=8 {
        saw_history |= term.screen.contains("earlier in this conversation");
        saw_run |= term.screen.contains("(run)");
        assert!(
            term.text().contains(label),
            "scrolling history keeps the same run\n{}",
            term.dump()
        );
        term.shot(&format!("reopened conversation page {page}"));
        if saw_history && saw_run {
            break;
        }
        if page < 8 {
            term.keys(PAGE_UP);
        }
    }
    assert!(
        saw_history && saw_run,
        "the history marker and its run must be accessible within eight pages\n{}",
        term.dump()
    );
    term.keys(END);
}

const LEFT: &str = "\x1b[D";

/// 14 · What a run left, then the same run after a reopen: its outputs as
/// the settlement carried them, the file it reported writing as read now
/// (edited after the run, read again: other bytes, never called the run's),
/// and its Proof bound to its execution, source and receipt. Closed and
/// reopened, the conversation is repainted as history and the same run is
/// offered as evidence whose Proof is read again; nothing replays.
#[test]
fn a_run_result_its_file_and_proof_are_found_again_after_a_reopen() {
    let rig = Rig::new("result");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\nline two\n").expect("brief");
    std::fs::write(rig.path("copy.nika"), COPY).expect("copy");
    let mut term = rig.spawn("14-result", 120, 40);
    wait_workspace(&mut term);
    term.send("run copy.nika\r");
    term.wait_until("the settlement", |s| s.contains("settled · succeeded"));
    let label = run_label(&term.text()).expect("the run names its execution");
    term.keys(F6);
    term.keys(RIGHT);
    term.wait_until("the outputs the settlement carried", |s| {
        s.contains("[outputs]") && s.contains("written")
    });
    term.keys(RIGHT);
    term.wait_until("the file read now", |s| {
        s.contains("[files]") && s.contains("reported written by") && s.contains("# Brief")
    });
    term.keys(RIGHT);
    term.wait_until("the proof bound to the run", |s| {
        s.contains("[proof]") && s.contains("this run's journal") && s.contains("receipt match")
    });
    std::fs::write(rig.path("out/copy.md"), "# Edited after the run\n").expect("edit");
    term.keys(LEFT);
    term.keys("r");
    term.wait_until("today's bytes, read again", |s| {
        s.contains("[files]") && s.contains("# Edited after the run")
    });
    assert!(!term.text().contains("unchanged"), "{}", term.dump());
    term.leave();
    let (kept, files) = (journal_bytes(&rig), rig.tree());
    let mut again = rig.spawn("14-reopen", 120, 40);
    wait_workspace(&mut again);
    visit_reopened_history(&mut again, &label);
    // One line of a wrapped notice: a needle never spans a wrap.
    again.wait_text("last run, observed in an earlier");
    again.wait_until("the same run, as evidence", |s| {
        s.contains(&label) && s.contains("earlier session")
    });
    again.keys(F6);
    // Before any Proof: the run's tasks, one task, its outputs and its file,
    // from the journal it left (captured once, the same bytes the Proof reads).
    again.wait_until("its tasks, from its journal", |s| {
        s.contains("› ✔ read_source") && s.contains("write_output")
    });
    let witness = captured(&again.text()).expect("the captured journal's witness");
    again.keys("\r");
    again.wait_until("one task, as its journal recorded it", |s| {
        let preview = preview_text(s);
        preview.contains("task read_source") && preview.contains("as its journal recorded it")
    });
    again.keys(BACKSPACE);
    again.keys(RIGHT);
    again.wait_until("the outputs its journal recorded", |s| {
        s.contains("[outputs]") && s.contains("written") && s.contains("./out/copy.md")
    });
    again.keys(RIGHT);
    again.wait_until("the file it wrote, read now", |s| {
        s.contains("[files]")
            && s.contains("reported written by")
            && s.contains("# Edited after the run")
    });
    again.keys(RIGHT);
    again.wait_until("its proof read again, bound to it", |s| {
        s.contains("[proof]") && s.contains("this run's journal")
    });
    assert_eq!(
        captured(&again.text()),
        Some(witness),
        "the Proof reads the same bytes"
    );
    assert!(again.text().contains(&label), "{}", again.dump());
    // Narrowed, folded below the minimum and widened again: the same run.
    again.resize(80, 24);
    again.wait_until("the proof at 80x24", |s| {
        s.contains("[proof]") && s.contains("verdict")
    });
    again.resize(50, 14);
    again.wait_until("the focus view below the minimum", |s| {
        !s.contains("[proof]") && s.contains("nika ›")
    });
    again.resize(120, 40);
    again.wait_until("the proof back at 120x40", |s| {
        s.contains("[proof]") && s.contains("receipt")
    });
    // Consulting it ran, wrote and resumed nothing: the same journals and
    // the same files.
    assert_eq!(journal_bytes(&rig), kept, "no journal written or changed");
    assert_eq!(rig.tree(), files, "no file written or changed");
    again.leave();
}

/// Every journal under the project's traces, with its bytes.
fn journal_bytes(rig: &Rig) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out: Vec<_> = (std::fs::read_dir(rig.path(".nika/traces")).expect("traces"))
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).expect("journal");
            (path, bytes)
        })
        .collect();
    out.sort();
    out
}

/// The twelve hex digits of the captured journal a screen names
/// (`captured bytes <hex>`), when it names one.
fn captured(screen: &str) -> Option<String> {
    screen.lines().find_map(|line| {
        let at = line.find("captured bytes ")? + "captured bytes ".len();
        let hex = line.get(at..at + 12)?;
        hex.chars()
            .all(|c| c.is_ascii_hexdigit())
            .then(|| hex.to_owned())
    })
}

/// A valid chained journal just under 8 MiB (of another execution: never
/// bound to the run), written over the run's own journal at `path`.
fn heavy_journal(path: &std::path::Path) {
    use sha2::{Digest as _, Sha256};
    let hex = |bytes: &[u8]| -> String {
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut out, b| {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{b:02x}"));
                out
            })
    };
    let mut chain = hex(b"nika-trace-v1");
    let mut out = String::new();
    let mut n = 0_u64;
    while out.len() < 8 * 1024 * 1024 - 4096 {
        let kind = if n == 0 {
            "workflow_started"
        } else {
            "task_completed"
        };
        let line = format!(
            r#"{{"chain":"{chain}","correlation":null,"execution":{{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}},"fields":[{{"key":"task","value":"t{n}"}},{{"key":"note","value":"{}"}}],"id":{{"uuid":"01a0ef11-03a1-73d9-a2bc-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#,
            "x".repeat(300)
        );
        chain = hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
        n += 1;
    }
    std::fs::write(path, out).expect("heavy journal");
}

/// 15 · Reading what a run left never freezes the shell: while the proof of
/// a journal near the 8 MiB cap is verified on a worker, the busy row says
/// what is being read and words typed land in the composer at once; the
/// verdict follows, the words stay in the draft, nothing is sent.
#[test]
fn the_shell_stays_live_while_a_run_proof_is_verified() {
    let rig = Rig::new("acquire");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\n").expect("brief");
    std::fs::write(rig.path("copy.nika"), COPY).expect("copy");
    let mut term = rig.spawn("15-acquire", 120, 40);
    wait_workspace(&mut term);
    term.send("run copy.nika\r");
    term.wait_until("the settlement", |s| s.contains("settled · succeeded"));
    let traces = std::fs::read_dir(rig.path(".nika/traces")).expect("traces");
    let journal = (traces.filter_map(Result::ok))
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|e| e == "ndjson"))
        .expect("the run's journal");
    heavy_journal(&journal);
    term.send(&format!("{F6}{RIGHT}{RIGHT}{RIGHT}{SHIFT_F6}zz"));
    term.wait_until("words typed while the proof is read", |s| {
        s.contains("zz") && s.contains("reading what the run left") && !s.contains("verdict ·")
    });
    term.wait_until("the verdict, then", |s| {
        s.contains("verdict ·") && s.contains("zz")
    });
    // The words are the draft alone: never echoed into the conversation.
    assert_eq!(term.text().matches("zz").count(), 1, "{}", term.dump());
    term.leave();
}

/// Two tasks, the second reading a file that is not there: a run that fails
/// the same way every time, with nothing to configure and no model.
const PICK: &str = r#"nika: pick
permits:
  fs: { read: ["./notes/absent.md"] }
  tools: ["nika:read"]
tasks:
  greet:
    invoke: { tool: "nika:log", args: { message: hello } }
  look:
    after: { greet: success }
    invoke: { tool: "nika:read", args: { path: "./notes/absent.md" } }
"#;

const BACKSPACE: &str = "\x7f";

/// 16 · A task of the run is picked by its id and read in detail, then the
/// list comes back: the pick survives a resize and the focus view, the
/// detail says the failure the stream carried, and reading it runs nothing
/// again (one journal, the project's files untouched).
#[test]
fn a_task_of_the_run_is_picked_read_and_left() {
    let rig = Rig::new("pick");
    std::fs::write(rig.path("pick.nika"), PICK).expect("pick");
    let mut term = rig.spawn("16-pick", 120, 40);
    wait_workspace(&mut term);
    term.send("run pick.nika\r");
    term.wait_until("the settlement", |s| s.contains("settled · failed"));
    let tree = rig.tree();
    term.keys(F6);
    term.wait_until("the task list, the first task picked", |s| {
        s.contains("tasks · ↑↓ pick · Enter details") && s.contains("› ✔ greet")
    });
    term.keys(DOWN);
    term.wait_text("› ✖ look");
    term.resize(80, 24);
    term.wait_until("the same task at 80x24", |s| s.contains("› ✖ look"));
    term.keys("\r");
    term.wait_until("its detail at 80x24", |s| {
        s.contains("task look") && s.contains("Backspace") && s.contains("failed")
    });
    assert!(term.text().contains("why ·"), "{}", term.dump());
    term.resize(50, 14);
    term.wait_until("the focus view below the minimum", |s| {
        !s.contains("task look") && s.contains("nika ›")
    });
    term.resize(120, 40);
    term.wait_until("the detail back with the workspace", |s| {
        s.contains("task look") && s.contains("why ·")
    });
    term.keys(BACKSPACE);
    term.wait_until("the list again, the same task picked", |s| {
        s.contains("› ✖ look") && !s.contains("task look")
    });
    let journals = (std::fs::read_dir(rig.path(".nika/traces")).expect("traces"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "ndjson"))
        .count();
    assert_eq!(journals, 1, "reading a task runs nothing again");
    assert_eq!(rig.tree(), tree, "the project's files are untouched");
    term.leave();
    let env = [("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")];
    let mut ascii = rig.spawn_with("16-pick-ascii", &["--ascii"], 120, 40, &env);
    wait_workspace(&mut ascii);
    ascii.send("run pick.nika\r");
    ascii.wait_until("the settlement", |s| s.contains("settled - failed"));
    ascii.keys(F6);
    ascii.wait_until("the ASCII list", |s| {
        s.contains("tasks - Up/Down pick - Enter details") && s.contains("* ok greet")
    });
    ascii.keys(DOWN);
    ascii.wait_text("* X look");
    ascii.keys("\r");
    ascii.wait_until("the ASCII detail", |s| {
        s.contains("task look") && s.contains("why -")
    });
    // The object region's own rows (the conversation keeps the run story's
    // words as the Session wrote them).
    let object = right_preview(&ascii.screen);
    assert!(
        object.iter().any(|row| row.contains("task look")),
        "{}",
        ascii.dump()
    );
    assert_renderer_ascii(&object.join("\n"));
    let raw = String::from_utf8_lossy(&ascii.raw).into_owned();
    // The colour forms the renderer writes (indexed, true colour, basic):
    // `ESC[38;<col>H` on a 40-row screen is a cursor move, not a colour.
    for hue in [
        "\x1b[38;5;",
        "\x1b[38;2;",
        "\x1b[48;5;",
        "\x1b[48;2;",
        "\x1b[31m",
        "\x1b[32m",
        "\x1b[33m",
    ] {
        assert!(!raw.contains(hue), "a colour under NO_COLOR: {hue:?}");
    }
    ascii.leave();
}

/// A parent whose one task calls a child workflow: both may log (a
/// composed run grants the child's tool in both files), nothing else.
const PARENT: &str = r#"nika: parent
permits:
  tools: ["nika:log"]
tasks:
  call:
    invoke: { workflow: "./child.nika" }
"#;

/// The child the parent calls.
const CHILD: &str = r#"nika: child
permits:
  tools: ["nika:log"]
tasks:
  greet:
    invoke: { tool: "nika:log", args: { message: hello } }
"#;

/// The `.ndjson` journals under the rig's `.nika/traces`.
fn journals(rig: &Rig) -> usize {
    (std::fs::read_dir(rig.path(".nika/traces")).expect("traces"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "ndjson"))
        .count()
}

/// 17 · From a task of the run, the child run its settle frame named is
/// opened one level down: the host reads the journal that child really
/// wrote, says its identity is not recorded (the local child route binds
/// none), shows the verifier's verdict and the child's own task, and
/// Backspace returns to the parent's task, then its list, through a resize
/// and the focus view, running and writing nothing.
#[test]
fn a_child_run_is_opened_from_its_task_and_left_for_the_parent() {
    let rig = Rig::new("child");
    std::fs::write(rig.path("parent.nika"), PARENT).expect("parent");
    std::fs::write(rig.path("child.nika"), CHILD).expect("child");
    let mut term = rig.spawn("17-child", 120, 40);
    wait_workspace(&mut term);
    term.send("run parent.nika\r");
    term.wait_until("the parent settled", |s| s.contains("settled · succeeded"));
    let (tree, written) = (rig.tree(), journals(&rig));
    term.keys(F6);
    term.wait_text("› ✔ call");
    term.keys("\r");
    term.wait_until("the task names its child", |s| {
        s.contains("task call") && s.contains("child run · ./child.nika")
    });
    term.wait_text("Enter: open its journal");
    term.keys("\r");
    term.wait_until("the child's journal, read", |s| {
        s.contains("child ./child.nika") && s.contains("verdict ·") && s.contains("greet")
    });
    assert!(term.text().contains("not recorded"), "{}", term.dump());
    term.resize(80, 24);
    term.wait_until("the child at 80x24", |s| s.contains("child ./child.nika"));
    term.resize(50, 14);
    term.wait_until("the focus view below the minimum", |s| {
        !s.contains("child ./child.nika") && s.contains("nika ›")
    });
    term.resize(120, 40);
    term.wait_until("the child back with the workspace", |s| {
        s.contains("child ./child.nika") && s.contains("verdict ·")
    });
    term.keys(BACKSPACE);
    term.wait_until("the parent's task again", |s| {
        s.contains("task call") && !s.contains("child ./child.nika")
    });
    term.keys(BACKSPACE);
    term.wait_until("the parent's list again", |s| {
        s.contains("› ✔ call") && !s.contains("task call")
    });
    assert_eq!(journals(&rig), written, "opening the child runs nothing");
    assert_eq!(rig.tree(), tree, "the project's files are untouched");
    term.leave();
    let env = [("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")];
    let mut ascii = rig.spawn_with("17-child-ascii", &["--ascii"], 120, 40, &env);
    wait_workspace(&mut ascii);
    ascii.send("run parent.nika\r");
    ascii.wait_until("the parent settled", |s| s.contains("settled - succeeded"));
    ascii.keys(F6);
    ascii.wait_text("* ok call");
    ascii.keys("\r");
    ascii.wait_text("Enter: open its journal");
    ascii.keys("\r");
    ascii.wait_until("the ASCII child", |s| {
        s.contains("child ./child.nika") && s.contains("verdict -")
    });
    let object = right_preview(&ascii.screen);
    assert_renderer_ascii(&object.join("\n"));
    let raw = String::from_utf8_lossy(&ascii.raw).into_owned();
    for hue in ["\x1b[38;5;", "\x1b[38;2;", "\x1b[48;5;", "\x1b[48;2;"] {
        assert!(!raw.contains(hue), "a colour under NO_COLOR: {hue:?}");
    }
    ascii.leave();
}
