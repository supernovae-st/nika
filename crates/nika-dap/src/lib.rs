// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        clippy::unreachable
    )
)]

//! `nika dap` — the Debug Adapter Protocol server (replay sessions).
//!
//! The MVP is a READ-ONLY replay debugger over a recorded run journal —
//! launch args carry `{workflow, replay}`, breakpoints map to task
//! lines, stepping walks task settles, `stepBack` is free because the
//! log is total (replay = re-render, NEVER re-execute). Live sessions
//! (breakpoint gates on the durable-pause substrate) come after replay
//! proves the wiring.
//!
//! Lifecycle (the spec's launch sequencing): initialize → capabilities
//! → `initialized` event → launch → setBreakpoints → configurationDone
//! → stopped/continue loop → disconnect.

pub mod anchor;
pub mod bounded;
pub mod chain;
pub mod cost_replay;
pub mod drift;
pub mod evidence;
pub mod flow;
pub mod inventory;
pub mod journal;
pub mod liveness;
pub mod memory;
pub mod otel;
pub mod quarantine;
pub mod recover;
pub mod reproduce;
pub mod resume;
pub mod retention;
pub mod seal;
pub mod sign;
pub mod stats;
pub mod store;

mod protocol;
mod replay;

/// Escape an artifact-originated string before it reaches a terminal
/// (NEP-0012 law 2 · the promoted `sanitize()` law) — strips every
/// control character (C0 · C1 · DEL · and therefore ESC, which kills the
/// OSC52 clipboard class). Applied at birth (where the untrusted field
/// enters a user-facing string), so the render layer never sees a live
/// sequence. The verify surface carries NO machine consumer of these
/// reason strings, so the escape is unconditional — the receipt's
/// byte-exact fields never pass through here.
#[must_use]
pub fn escape_tty(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod escape_tty_tests {
    #[test]
    fn control_chars_die_and_text_survives() {
        assert_eq!(super::escape_tty("plain-key_1234"), "plain-key_1234");
        assert_eq!(super::escape_tty("a\u{1b}]52;;evil\u{7}b"), "a]52;;evilb");
        assert_eq!(super::escape_tty("x\ny\tz"), "xyz");
        assert_eq!(super::escape_tty("\u{7f}\u{9b}"), "");
    }
}

use protocol::{Request, Wire};
use replay::ReplaySession;

/// Serve one DAP session over stdio. Like `nika lsp`, stdout belongs to
/// the protocol — diagnostics go to stderr only.
#[must_use]
pub fn run_stdio() -> u8 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let wire = Wire::new(stdin.lock(), stdout.lock());
    serve(wire)
}

/// The session loop — generic over the wire so tests drive exact bytes.
fn serve<R: std::io::BufRead, W: std::io::Write>(mut wire: Wire<R, W>) -> u8 {
    let mut session: Option<ReplaySession> = None;
    while let Some(req) = wire.read_request() {
        match req.command.as_str() {
            "initialize" => {
                // Capabilities: absence = unsupported (the spec's rule) —
                // claim ONLY what the replay session actually serves.
                wire.respond(
                    &req,
                    serde_json::json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsStepBack": true,
                    }),
                );
                // `initialized` waits for launch: the adapter is only
                // ready for setBreakpoints once a session exists (a
                // config-first client would lose its breakpoints to
                // "before launch" rejections — the M1 finding).
            }
            "launch" => match launch(&req) {
                Ok(s) => {
                    // The #210 identity check speaks BEFORE the first stop:
                    // a drifted source means snapped breakpoint lines may
                    // not match what actually ran.
                    if s.chain_broken {
                        wire.emit(
                            "output",
                            serde_json::json!({
                                "category": "console",
                                "output": "⚠ this journal FAILS verification (nika trace verify) — its recorded outputs are unverified\n",
                            }),
                        );
                    }
                    if let Some(note) = &s.truncated_note {
                        wire.emit(
                            "output",
                            serde_json::json!({
                                "category": "console",
                                "output": format!("⚠ {note} — this replay shows the run's valid prefix only\n"),
                            }),
                        );
                    }
                    if s.drifted == Some(true) {
                        wire.emit(
                            "output",
                            serde_json::json!({
                                "category": "console",
                                "output": "⚠ workflow changed since this run was recorded — breakpoint lines may be off (re-run to refresh the journal)\n",
                            }),
                        );
                    }
                    session = Some(s);
                    wire.respond(&req, serde_json::Value::Null);
                    wire.emit("initialized", serde_json::Value::Null);
                }
                Err(e) => wire.reject(&req, &e),
            },
            "setBreakpoints" => set_breakpoints(&mut wire, session.as_mut(), &req),
            "configurationDone" => {
                wire.respond(&req, serde_json::Value::Null);
                // The replay opens standing on the first settle — but a
                // failed launch must not announce a phantom stop.
                if session.is_some() {
                    stopped(&mut wire, "entry");
                }
            }
            "threads" => {
                let name = session
                    .as_ref()
                    .map_or("workflow", |s| s.workflow_name.as_str());
                wire.respond(
                    &req,
                    serde_json::json!({ "threads": [{ "id": 1, "name": name }] }),
                );
            }
            "stackTrace" => stack_trace(&mut wire, session.as_ref(), &req),
            "scopes" => {
                // One scope; its reference is stable (state lives in the
                // session cursor, not in a per-stop arena).
                wire.respond(
                    &req,
                    serde_json::json!({ "scopes": [
                        { "name": "Outputs", "variablesReference": 1, "expensive": false },
                    ]}),
                );
            }
            "variables" => variables(&mut wire, session.as_ref(), &req),
            "continue" | "next" | "stepBack" | "reverseContinue" => {
                step(&mut wire, session.as_mut(), &req);
            }
            "terminate" | "disconnect" => {
                wire.respond(&req, serde_json::Value::Null);
                return 0;
            }
            _ => wire.reject(&req, "nika dap: not supported in a replay session"),
        }
    }
    0
}

/// Launch arguments: `{workflow, replay}` — both required (replay-only).
fn launch(req: &Request) -> Result<ReplaySession, String> {
    let workflow = req.arguments["workflow"]
        .as_str()
        .ok_or("launch needs `workflow` (the YAML path)")?;
    let replay = req.arguments["replay"]
        .as_str()
        .ok_or("launch needs `replay` (a recorded .ndjson journal) — live sessions land later")?;
    ReplaySession::load(workflow, replay)
}

fn set_breakpoints<R: std::io::BufRead, W: std::io::Write>(
    wire: &mut Wire<R, W>,
    session: Option<&mut ReplaySession>,
    req: &Request,
) {
    let Some(session) = session else {
        wire.reject(req, "setBreakpoints before launch");
        return;
    };
    // The session debugs ONE source: breakpoints from any other file
    // must not clobber the set (VS Code sends one setBreakpoints per
    // file — the H1 finding). Mismatch → all unverified, set untouched.
    let requested = req.arguments["source"]["path"].as_str();
    if let Some(path) = requested
        && path != session.workflow_path
    {
        let n = req.arguments["breakpoints"].as_array().map_or(0, Vec::len);
        let body: Vec<serde_json::Value> = (0..n)
            .map(|_| {
                serde_json::json!({
                    "verified": false,
                    "message": "Breakpoints only bind in the replayed workflow file.",
                })
            })
            .collect();
        wire.respond(req, serde_json::json!({ "breakpoints": body }));
        return;
    }
    // `map`, never `filter_map`: the response array must stay 1:1 with
    // the request (clients align by index — the L2 finding). A bad line
    // becomes 0, which snaps to nothing → an honest unverified verdict.
    let lines: Vec<u32> = req.arguments["breakpoints"]
        .as_array()
        .map(|bps| {
            bps.iter()
                .map(|b| u32::try_from(b["line"].as_i64().unwrap_or(0)).unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    let verdicts = session.set_breakpoints(&lines);
    let body: Vec<serde_json::Value> = verdicts
        .iter()
        .map(|(verified, line)| {
            serde_json::json!({
                "verified": verified,
                "line": line,
                "message": if *verified { serde_json::Value::Null }
                           else { "No task at or above this line.".into() },
            })
        })
        .collect();
    wire.respond(req, serde_json::json!({ "breakpoints": body }));
}

fn stack_trace<R: std::io::BufRead, W: std::io::Write>(
    wire: &mut Wire<R, W>,
    session: Option<&ReplaySession>,
    req: &Request,
) {
    let Some(session) = session else {
        wire.reject(req, "stackTrace before launch");
        return;
    };
    let stop = session.current();
    // Lexical containment, not a call stack (the ansibug precedent):
    // the settled task, then the workflow itself.
    wire.respond(
        req,
        serde_json::json!({ "stackFrames": [
            {
                "id": 1,
                "name": format!("{} ({})", stop.task, stop.kind),
                "source": { "path": session.workflow_path },
                "line": stop.line.max(1),
                "column": 1,
            },
            {
                "id": 2,
                "name": session.workflow_name,
                "source": { "path": session.workflow_path },
                "line": 1,
                "column": 1,
            },
        ], "totalFrames": 2 }),
    );
}

fn variables<R: std::io::BufRead, W: std::io::Write>(
    wire: &mut Wire<R, W>,
    session: Option<&ReplaySession>,
    req: &Request,
) {
    let Some(session) = session else {
        wire.reject(req, "variables before launch");
        return;
    };
    let vars: Vec<serde_json::Value> = session
        .variables()
        .into_iter()
        .map(|(name, value)| {
            serde_json::json!({ "name": name, "value": value, "variablesReference": 0 })
        })
        .collect();
    wire.respond(req, serde_json::json!({ "variables": vars }));
}

/// The four movement verbs share one shape: mutate the cursor, respond,
/// then either announce the stop or terminate (forward off the end).
fn step<R: std::io::BufRead, W: std::io::Write>(
    wire: &mut Wire<R, W>,
    session: Option<&mut ReplaySession>,
    req: &Request,
) {
    let Some(session) = session else {
        wire.reject(req, "no session — launch first");
        return;
    };
    let (still_running, reason) = match req.command.as_str() {
        "continue" => (session.run_forward(), "breakpoint"),
        "next" => (session.step(), "step"),
        "stepBack" => {
            session.step_back();
            (true, "step")
        }
        // reverseContinue — floors at the first settle.
        _ => {
            session.run_backward();
            (true, "breakpoint")
        }
    };
    wire.respond(req, serde_json::json!({ "allThreadsContinued": true }));
    if still_running {
        stopped(wire, reason);
    } else {
        // The log is total: running off its end is the run's end.
        wire.emit("terminated", serde_json::Value::Null);
    }
}

fn stopped<R: std::io::BufRead, W: std::io::Write>(wire: &mut Wire<R, W>, reason: &str) {
    wire.emit(
        "stopped",
        serde_json::json!({
            "reason": reason,
            "threadId": 1,
            "allThreadsStopped": true,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::protocol::frame;
    use super::*;

    fn drive(messages: &[serde_json::Value]) -> String {
        let mut input: Vec<u8> = Vec::new();
        for m in messages {
            input.extend(frame(m));
        }
        let mut out: Vec<u8> = Vec::new();
        let code = serve(Wire::new(std::io::Cursor::new(input), &mut out));
        assert_eq!(code, 0);
        String::from_utf8(out).expect("utf8")
    }

    fn req(seq: i64, command: &str, arguments: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({"seq": seq, "type": "request", "command": command,
                           "arguments": arguments})
    }

    #[test]
    fn handshake_then_disconnect() {
        let out = drive(&[
            req(1, "initialize", &serde_json::json!({"adapterID": "nika"})),
            req(2, "disconnect", &serde_json::Value::Null),
            // Anything after disconnect must go UNANSWERED — the session
            // returned, it did not fall through to the reject arm.
            req(3, "threads", &serde_json::Value::Null),
        ]);
        assert!(out.contains(r#""supportsStepBack":true"#));
        // `initialized` waits for a session (M1): no launch → no event —
        // a config-first client must never lose breakpoints to
        // "before launch" rejections.
        assert!(!out.contains(r#""event":"initialized""#));
        assert!(out.contains(r#""request_seq":2"#));
        assert!(
            !out.contains(r#""request_seq":3"#),
            "disconnect ENDS the session"
        );
    }

    #[test]
    fn threads_and_scopes_answer_even_before_launch() {
        // Clients probe both during configuration: the thread answers
        // with the generic name (no session yet) and the one scope is
        // fixed — neither may fall to the reject arm.
        let out = drive(&[
            req(1, "threads", &serde_json::Value::Null),
            req(2, "scopes", &serde_json::json!({"frameId": 1})),
            req(3, "disconnect", &serde_json::Value::Null),
        ]);
        assert!(out.contains(r#""name":"workflow""#), "{out}");
        assert!(out.contains(r#""name":"Outputs""#), "{out}");
    }

    #[test]
    fn requests_before_launch_are_refused_not_ignored() {
        let out = drive(&[
            req(1, "stackTrace", &serde_json::json!({"threadId": 1})),
            req(2, "attach", &serde_json::Value::Null),
            req(3, "disconnect", &serde_json::Value::Null),
        ]);
        assert!(out.contains("stackTrace before launch"));
        assert!(out.contains("not supported in a replay session"));
    }

    /// Journey fixture on disk: the two-task workflow + its recorded
    /// journal, hand-authored in the journal's REAL serde shape
    /// (untagged Value · flat ns — pinned from a live line, not
    /// imagined; the `floor_deep` lesson). Returns (dir, wf, journal) —
    /// the tempdir rides along so it outlives the session.
    fn journey_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let wf = dir.path().join("w.nika.yaml");
        std::fs::write(
            &wf,
            "nika: journey\ntasks:\n  alpha:\n    exec:\n      command: [\"true\"]\n  beta:\n    after:\n      alpha: success\n    exec:\n      command: [\"true\"]\n",
        )
        .expect("write wf");
        let journal = dir.path().join("run.ndjson");
        let lines = [
            serde_json::json!({"id": {"uuid": "01912345-0000-7000-8000-000000000001"},
                "timestamp": 1000, "kind": "workflow_started", "run": null, "correlation": null,
                "fields": [{"key": "workflow", "value": "journey"}]}),
            serde_json::json!({"id": {"uuid": "01912345-0000-7000-8000-000000000002"},
                "timestamp": 2000, "kind": "task_completed", "run": null, "correlation": null,
                "fields": [{"key": "task", "value": "alpha"},
                           {"key": "output", "value": "\"a\""}]}),
            serde_json::json!({"id": {"uuid": "01912345-0000-7000-8000-000000000003"},
                "timestamp": 3000, "kind": "task_completed", "run": null, "correlation": null,
                "fields": [{"key": "task", "value": "beta"}]}),
            serde_json::json!({"id": {"uuid": "01912345-0000-7000-8000-000000000004"},
                "timestamp": 4000, "kind": "workflow_completed", "run": null, "correlation": null,
                "fields": [{"key": "workflow", "value": "journey"}]}),
        ];
        let mut ndjson = String::new();
        for l in &lines {
            use std::fmt::Write as _;
            let _ = writeln!(ndjson, "{l}");
        }
        std::fs::write(&journal, ndjson).expect("write journal");
        (dir, wf, journal)
    }

    /// The full journey against REAL files — launch → breakpoints →
    /// configurationDone → threads → stackTrace → variables → next →
    /// stepBack → continue-off-the-end → terminated.
    #[test]
    fn replay_journey_over_real_files() {
        let (_dir, wf, journal) = journey_fixture();
        let out = drive(&[
            req(1, "initialize", &serde_json::json!({"adapterID": "nika"})),
            req(
                2,
                "launch",
                &serde_json::json!({"workflow": wf.to_string_lossy(),
                                   "replay": journal.to_string_lossy()}),
            ),
            // Line 8 sits INSIDE beta's block — snaps back to beta's start.
            req(
                3,
                "setBreakpoints",
                &serde_json::json!({"source": {"path": wf.to_string_lossy()},
                                   "breakpoints": [{"line": 8}]}),
            ),
            req(4, "configurationDone", &serde_json::Value::Null),
            req(5, "threads", &serde_json::Value::Null),
            req(6, "stackTrace", &serde_json::json!({"threadId": 1})),
            req(
                7,
                "variables",
                &serde_json::json!({"variablesReference": 1}),
            ),
            req(8, "next", &serde_json::json!({"threadId": 1})),
            // The frame AFTER next proves the cursor moved FORWARD —
            // a `next` that walked backward would still say "step".
            req(9, "stackTrace", &serde_json::json!({"threadId": 1})),
            req(10, "stepBack", &serde_json::json!({"threadId": 1})),
            req(11, "continue", &serde_json::json!({"threadId": 1})),
            req(12, "continue", &serde_json::json!({"threadId": 1})),
            req(13, "disconnect", &serde_json::Value::Null),
        ]);

        assert!(out.contains(r#""verified":true"#), "breakpoint verifies");
        assert!(
            out.contains(r#""name":"journey""#),
            "the thread is the workflow"
        );
        assert!(
            out.contains(r#""name":"alpha (completed)""#),
            "frame at the cursor"
        );
        assert!(
            out.contains(r#""value":"\"a\"""#),
            "alpha's recorded output"
        );
        assert!(
            out.contains(r#""reason":"entry""#),
            "configurationDone stops at entry"
        );
        assert!(
            out.contains(r#""name":"beta (completed)""#),
            "`next` moved the cursor FORWARD to beta"
        );
        assert_eq!(
            out.matches(r#""reason":"step""#).count(),
            2,
            "next AND stepBack each announce their own step stop"
        );
        assert!(
            out.contains(r#""reason":"breakpoint""#),
            "continue lands on beta"
        );
        assert!(
            out.contains(r#""event":"terminated""#),
            "running off the total log ends the session"
        );
        assert!(
            !out.contains("workflow changed since this run was recorded"),
            "a pre-#210 journal (no recorded sha) must not cry drift"
        );
    }
}
