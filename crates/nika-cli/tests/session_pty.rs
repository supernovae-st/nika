// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::disallowed_types, clippy::expect_used, clippy::panic)]

//! PTY e2e — ADR-125 · the one door: bare `nika` on an interactive
//! terminal opens the native session; a pipe keeps the deterministic
//! concierge (exit 0); `nika thread` is gone (no alias). The session
//! answers a Nika fact without any model, writes nothing into the
//! project, and closes on `/quit`.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

fn exit_code(session: &mut LoggedSession) -> i32 {
    match session.get_process_mut().wait().expect("wait") {
        WaitStatus::Exited(_, code) => code,
        other => panic!("process did not exit cleanly: {other:?}"),
    }
}

fn output_text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A rig: a project with one workflow and a HOME of its own.
fn rig(name: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let project = tempfile::Builder::new()
        .prefix(&format!("nika-session-pty-{name}-"))
        .tempdir()
        .expect("project dir");
    std::fs::write(
        project.path().join("alpha.nika"),
        "nika: alpha\nmodel: mock/echo\ntasks:\n  hello:\n    infer: { prompt: hi, max_tokens: 10 }\n",
    )
    .expect("workflow");
    let home = tempfile::Builder::new()
        .prefix(&format!("nika-session-home-{name}-"))
        .tempdir()
        .expect("home dir");
    (project, home)
}

/// A pipe is the concierge, never the session; the TTY is the session.
#[test]
fn a_pipe_is_the_concierge_and_the_tty_is_the_session() {
    let (project, home) = rig("door");
    let piped = Command::new(bin())
        .current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("HOME", home.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("piped nika");
    assert_eq!(
        piped.status.code(),
        Some(0),
        "a pipe is a greeting, not usage"
    );
    let pipe_text = output_text(&piped);
    assert!(
        pipe_text.contains("Local first"),
        "the pipe shows the cascade: {pipe_text}"
    );
    assert!(
        !pipe_text.contains("nika · session"),
        "the pipe never opens a session: {pipe_text}"
    );
    assert!(
        !pipe_text.contains("Choose how Nika"),
        "the pipe never asks: {pipe_text}"
    );

    // The TTY, first run: the session opens at once on the human's
    // question; a fact answers with no choice made; the first line only an
    // intelligence answers asks the first screen in context; the choice
    // « 4 » (no conversational AI) resumes that line; the close.
    let mut cmd = Command::new(bin());
    cmd.current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("NIKA_TUI", "0")
        .env("HOME", home.path());
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
        .expect("What do you want to automate?")
        .expect("the session opens on the question, no first screen");
    session.expect("nika ›").expect("the prompt");
    session
        .send_line("what workflows are here?")
        .expect("a fact");
    session
        .expect("alpha.nika")
        .expect("the workflow listed, no model asked, no choice made");
    session
        .send_line("hello there, how are you today?")
        .expect("a line only an intelligence answers");
    session
        .expect("Nika needs an intelligence for this part")
        .expect("the first screen is asked in context");
    session
        .expect("No AI in this conversation")
        .expect("the fourth path");
    // The choice is typed at its own prompt: a line typed before the prompt is drawn is
    // type-ahead, which the choice never takes as its answer.
    session.expect("› ").expect("the choice prompt");
    session.send_line("4").expect("choose");
    session
        .expect("no conversational AI")
        .expect("the choice names the path");
    session
        .expect("facts still answer")
        .expect("the waiting line resumed under the choice");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("the session closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(
        !home
            .path()
            .join(".nika")
            .join("session-intelligence.json")
            .exists(),
        "the choice holds for this conversation: the operator's kept choice is never written"
    );
    let entries: Vec<_> = std::fs::read_dir(project.path())
        .expect("project")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec!["alpha.nika"],
        "nothing written into the project: {entries:?}"
    );
}

/// The second run never asks again: the kept choice opens the session.
#[test]
fn the_kept_choice_opens_the_session_without_asking() {
    let (project, home) = rig("kept");
    std::fs::create_dir_all(home.path().join(".nika")).expect("home dir");
    std::fs::write(
        home.path().join(".nika").join("session-intelligence.json"),
        "{\"kind\":{\"kind\":\"none\"},\"model\":null,\"chosen_at\":\"2026-09-03T00:00:00Z\"}",
    )
    .expect("preference");
    let mut cmd = Command::new(bin());
    cmd.current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("NIKA_TUI", "0")
        .env("HOME", home.path());
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
        .expect("What do you want to automate?")
        .expect("the session opens at once");
    session.send_line("/help").expect("help");
    session.expect("/intelligence").expect("the card");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
}

/// `nika thread` is gone: no alias, the parser's own refusal.
#[test]
fn nika_thread_is_an_unrecognized_subcommand() {
    let (project, home) = rig("thread");
    let out = Command::new(bin())
        .arg("thread")
        .current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("HOME", home.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("nika thread");
    assert_eq!(
        out.status.code(),
        Some(2),
        "a mistyped verb is the parser's usage error"
    );
    let text = output_text(&out);
    assert!(
        text.contains("unrecognized subcommand") || text.contains("unexpected argument"),
        "{text}"
    );
    assert!(
        !text.contains("nika · thread") && !text.contains("nika · session"),
        "{text}"
    );
}

/// One gesture: bare `nika` on a real terminal opens the renderer (its
/// probe asks the terminal's attributes, then the workspace takes the
/// alternate screen); `NIKA_TUI=0` keeps the plain loop, which asks the
/// terminal nothing.
#[test]
fn bare_nika_opens_the_renderer_and_nika_tui_zero_keeps_the_plain_loop() {
    let (project, home) = rig("bare-door");
    let mut cmd = Command::new(bin());
    cmd.current_dir(project.path())
        .env_remove("NIKA_TUI")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home.path());
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
        .expect("\x1b[c")
        .expect("the renderer's probe asks the terminal's attributes: the renderer opened");
    session.send("\x1b[?62;22c").expect("answer the attributes");
    session
        .expect("\x1b[?1049h")
        .expect("bare nika opens the workspace on the alternate screen");
    // The status row is the workspace's own: the banner may already have
    // scrolled out of a small transcript region.
    session
        .expect("workspace · F6 panel")
        .expect("the workspace drawn");
    session.send("/quit\r").expect("quit in raw mode");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);

    let mut cmd = Command::new(bin());
    cmd.current_dir(project.path())
        .env("NIKA_TUI", "0")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home.path());
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
        .expect("What do you want to automate?")
        .expect("the plain loop's banner, whole");
    session.expect("nika ›").expect("the plain prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
}

// ── Typeahead (C11): only a line typed after a question is shown answers it ─────────────────

/// A gated workflow whose run first waits `wait`: a line typed then precedes its gate.
fn gated(project: &Path, wait: &str) {
    std::fs::write(project.join("draft.md"), "the draft\n").expect("draft");
    let settle = format!(
        "  settle:\n    invoke: {{ tool: \"nika:wait\", args: {{ duration: \"{wait}\" }} }}\n"
    );
    let workflow = format!(
        "nika: waited-gate\npermits: {{ fs: {{ read: [\"./draft.md\"], write: [\"./final.md\"] }}, tools: [\"nika:wait\", \"nika:read\", \"nika:prompt\", \"nika:write\"] }}\ntasks:\n{settle}  read_draft:\n    after: {{ settle: success }}\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"./draft.md\" }} }}\n  approve:\n    after: {{ read_draft: success }}\n    invoke: {{ tool: \"nika:prompt\", args: {{ mode: confirm, message: \"Write final.md?\" }} }}\n  write_final:\n    after: {{ approve: success }}\n    with: {{ go: \"${{{{ tasks.approve.output }}}}\", text: \"${{{{ tasks.read_draft.output }}}}\" }}\n    when: \"${{{{ with.go == true }}}}\"\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"./final.md\", content: \"${{{{ with.text }}}}\" }} }}\n"
    );
    std::fs::write(project.join("gate.nika"), workflow).expect("gate");
}

/// The plain loop on a PTY in `project`: an environment of its own (nothing inherited, so a
/// host's model or decision-seat settings never reach it), an isolated HOME, no key, the run
/// keys absent.
fn plain(project: &Path, home: &Path, env: &[(String, String)]) -> LoggedSession {
    let mut cmd = Command::new(bin());
    cmd.current_dir(project)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NIKA_TUI", "0")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", home.join("absent-run-key"))
        .env("NIKA_RUN_PUB_FILE", home.join("absent-run-pub"));
    for (key, value) in env {
        cmd.env(key, value);
    }
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(90)));
    session.expect("nika ›").expect("the plain prompt");
    session
}

/// A line typed while a run works never answers the gate it had not asked yet: the question
/// waits for a line typed after it, and the write waits with it.
#[test]
fn a_line_typed_during_a_run_never_answers_its_gate() {
    let (project, home) = rig("typeahead-gate");
    gated(project.path(), "5s");
    let mut session = plain(project.path(), home.path(), &[]);
    session.send_line("run gate.nika").expect("run");
    session
        .expect("running `gate.nika`")
        .expect("the run starts");
    session
        .send_line("y")
        .expect("typed while the run waits, before its gate");
    session
        .expect("Write final.md?")
        .expect("the gate is asked");
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        !project.path().join("final.md").exists(),
        "a line typed before the gate answered it"
    );
    session
        .send_line("n")
        .expect("an answer typed after the question");
    session.expect("nika ›").expect("the run settled");
    assert!(!project.path().join("final.md").exists(), "declined");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
}

/// Its neighbour: a line typed after the gate is shown answers it, once.
#[test]
fn a_line_typed_after_the_gate_is_shown_answers_it_once() {
    let (project, home) = rig("typeahead-gate-fresh");
    gated(project.path(), "1s");
    let mut session = plain(project.path(), home.path(), &[]);
    session.send_line("run gate.nika").expect("run");
    session
        .expect("Write final.md?")
        .expect("the gate is asked");
    session
        .send_line("y")
        .expect("an answer typed after the question");
    session
        .expect("produced · ./final.md")
        .expect("the approved write");
    session.expect("nika ›").expect("the run settled");
    let written = std::fs::read_to_string(project.path().join("final.md")).expect("final.md");
    assert_eq!(written, "the draft\n", "written once, exactly");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
}

/// Work the deterministic reader cannot settle: the Session asks its seat.
const UNSETTLED: &str = "Read ./a.md and do something clever with it, then write ./b.md";

/// The seat's sketch: read `./a.md`, write it to `./b.md`. No `infer` task, so no run model is
/// asked, nothing is left to fill, and the round ends READY as `clever-copy`.
fn copy_sketch() -> String {
    serde_json::json!({"name": "clever-copy", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "purpose": "read_source",
         "reads": ["./a.md"]},
        {"id": "write_result", "verb": "invoke", "tool": "nika:write", "purpose": "write_result",
         "writes": ["./b.md"], "with": [{"name": "text", "from": "read_source"}]},
    ], "questions": [], "gaps": [], "notes": "read, write"})
    .to_string()
}

/// A local seat on the loopback, keyless, on the OpenAI-compatible wire a `vllm` override
/// speaks, answering by the request's schema as the sketch door asks: a route label is new
/// work, the sketch is [`copy_sketch`], its fills are none, and the judge finds it faithful. The
/// judge's FIRST call waits until released: while it is in flight Nika is still building, and no
/// proposal exists yet.
struct LoopbackSeat {
    port: u16,
    entered: std::sync::mpsc::Receiver<()>,
    release: std::sync::mpsc::Sender<()>,
}

impl LoopbackSeat {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("seat");
        let port = listener.local_addr().expect("seat address").port();
        let (arrived, entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        // The loopback seat's accept loop: a test harness thread, never production.
        #[allow(clippy::disallowed_methods)]
        std::thread::spawn(move || {
            let mut judged = false;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(body) = seat_request(&mut stream) else {
                    continue;
                };
                let request: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                let format = &request["response_format"];
                let schema = if format["json_schema"]["schema"].is_object() {
                    &format["json_schema"]["schema"]
                } else {
                    &format["schema"]
                };
                let properties = &schema["properties"];
                let answer = if let Some(keys) = properties["choice"]["enum"].as_array() {
                    if !judged {
                        judged = true;
                        let _sent = arrived.send(());
                        let _released = released.recv();
                    }
                    let approve = ["faithful", "carried"]
                        .into_iter()
                        .find(|key| keys.iter().any(|value| value == *key))
                        .unwrap_or("none");
                    serde_json::json!({"choice": approve}).to_string()
                } else if properties.get("fills").is_some() {
                    r#"{"fills":[],"notes":"no holes"}"#.to_owned()
                } else if properties.get("tasks").is_some() {
                    copy_sketch()
                } else if body.contains("You route ONE line") {
                    "NEW_WORK".to_owned()
                } else {
                    "{}".to_owned()
                };
                let reply = serde_json::json!({
                    "id": "chatcmpl-typeahead", "object": "chat.completion",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": answer}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
                })
                .to_string();
                let _written = std::io::Write::write_all(
                    &mut stream,
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
            }
        });
        Self {
            port,
            entered,
            release,
        }
    }

    /// The kept choice of this local seat in `home`, and the environment that points it here.
    fn chosen_in(&self, home: &Path) -> Vec<(String, String)> {
        std::fs::create_dir_all(home.join(".nika")).expect("home");
        std::fs::write(
            home.join(".nika").join("session-intelligence.json"),
            r#"{"kind":{"kind":"local","provider":"vllm"},"model":"vllm/typeahead-seat","chosen_at":"2026-09-29T00:00:00Z"}"#,
        )
        .expect("preference");
        vec![
            (
                "NIKA_VLLM_BASE_URL".to_owned(),
                format!("http://127.0.0.1:{}/v1", self.port),
            ),
            ("NIKA_AUTHORING_STRATEGY".to_owned(), "sketch".to_owned()),
        ]
    }
}

/// One request's body, read whole: its head, then as many bytes as it declares.
fn seat_request(stream: &mut std::net::TcpStream) -> Option<String> {
    use std::io::Read as _;
    let mut data = Vec::new();
    let mut chunk = [0_u8; 8192];
    let end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    while data.len() < end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n]);
    }
    Some(String::from_utf8_lossy(&data[end..]).into_owned())
}

/// A line typed while Nika builds never consents to the proposal it had not shown yet:
/// `apply? ›` waits for a line typed after it, and nothing is written.
#[test]
fn a_line_typed_while_nika_builds_never_consents() {
    let (project, home) = rig("typeahead-consent");
    std::fs::write(project.path().join("a.md"), "alpha\n").expect("a.md");
    let seat = LoopbackSeat::start();
    let env = seat.chosen_in(home.path());
    let mut session = plain(project.path(), home.path(), &env);
    session.send_line(UNSETTLED).expect("the request");
    seat.entered
        .recv_timeout(Duration::from_secs(90))
        .expect("the judge's call is in flight");
    session
        .send_line("yes")
        .expect("typed before any proposal exists");
    seat.release.send(()).expect("release the judge");
    session.expect("apply? ›").expect("the proposal is shown");
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        !project.path().join("clever-copy.nika").exists(),
        "a line typed before the proposal consented to it"
    );
    session
        .send_line("no")
        .expect("an answer typed after the question");
    session.expect("nika ›").expect("the proposal discarded");
    assert!(!project.path().join("clever-copy.nika").exists());
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
}

/// Its neighbour: a `yes` typed after the proposal is shown applies it, once.
#[test]
fn a_yes_typed_after_the_proposal_is_shown_applies_it_once() {
    let (project, home) = rig("typeahead-consent-fresh");
    std::fs::write(project.path().join("a.md"), "alpha\n").expect("a.md");
    let seat = LoopbackSeat::start();
    let env = seat.chosen_in(home.path());
    let mut session = plain(project.path(), home.path(), &env);
    session.send_line(UNSETTLED).expect("the request");
    seat.entered
        .recv_timeout(Duration::from_secs(90))
        .expect("the judge's call is in flight");
    seat.release.send(()).expect("release the judge");
    session.expect("apply? ›").expect("the proposal is shown");
    session
        .send_line("yes")
        .expect("a consent typed after the question");
    session
        .expect("applied · wrote `clever-copy.nika`")
        .expect("applied");
    session.expect("nika ›").expect("the prompt again");
    let saved = std::fs::read_dir(project.path())
        .expect("project")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "nika"))
        .count();
    assert_eq!(saved, 2, "alpha.nika and the one applied workflow");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
}
