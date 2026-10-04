// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types, clippy::panic)]

//! The real CLI adapter: one core, explicit writes, honest incomplete, no ambient policy.
use nika_onboard::compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[path = "compile_cli/capture.rs"]
mod capture;

fn command(room: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .current_dir(room)
        .stdin(Stdio::null());
    cmd
}

fn call(room: &Path, args: &[&str]) -> Output {
    command(room).args(args).output().expect("CLI")
}

fn result(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// A loopback seat on the OpenAI-compatible wire (the vLLM route's base URL override): each
/// request is answered with the next scripted text, the last one repeating, and every body it
/// received is kept. No key, no network beyond 127.0.0.1. Dropping the seat stops and joins its
/// thread, so no listener outlives the case that started it.
struct LoopbackSeat {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl LoopbackSeat {
    // The synchronous CLI fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn start(script: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback seat");
        let port = listener.local_addr().expect("seat address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, halt) = (Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            let mut next = 0_usize;
            for stream in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_request(&mut stream) else {
                    continue;
                };
                seen.lock().expect("seat log").push(body);
                let text = script
                    .get(next)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                next += 1;
                if let Some(target) = text.strip_prefix("redirect ") {
                    redirect(&mut stream, target);
                } else if let Some(blocks) = text.strip_prefix("anthropic ") {
                    respond_anthropic(&mut stream, blocks);
                } else if text == "hold" {
                    // Received and counted, never answered: the connection stays open until
                    // the seat is dropped.
                    while !halt.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                } else if let Some(text) = text.strip_prefix("nomodel ") {
                    respond(&mut stream, text, None);
                } else {
                    respond(&mut stream, &text, Some("loopback-served-model"));
                }
            }
        });
        Self {
            port,
            bodies,
            stop,
            server: Some(server),
        }
    }

    /// The base URL a local engine's override takes (`NIKA_VLLM_BASE_URL`).
    fn base(&self) -> String {
        let port = self.port;
        format!("127.0.0.1:{port}")
    }

    fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().expect("seat log").clone()
    }
}

impl Drop for LoopbackSeat {
    fn drop(&mut self) {
        // The flag ends the accept loop at its next connection; the wake connection is that one.
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// The most bytes the seat buffers for one request, headers and body together.
const MAX_REQUEST_BYTES: usize = 4 << 20;

fn read_request(stream: &mut TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok()?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 || buffer.len() + n > MAX_REQUEST_BYTES {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    if length > MAX_REQUEST_BYTES.saturating_sub(header_end) {
        return None;
    }
    while buffer.len() < header_end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(buffer.get(header_end..header_end + length)?).ok()
}

/// A completion answering `text`, reporting `model` as the served identity when it names one
/// (a scripted `nomodel <text>` reports none).
fn respond(stream: &mut TcpStream, text: &str, model: Option<&str>) {
    let mut body = serde_json::json!({
        "id": "chatcmpl-loopback",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200},
    });
    if let Some(model) = model {
        body["model"] = serde_json::json!(model);
    }
    let body = body.to_string();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

/// A scripted `anthropic <content>` is answered on the Anthropic messages wire with exactly
/// that JSON content array (text, empty text, thinking or tool blocks, in order).
fn respond_anthropic(stream: &mut TcpStream, blocks: &str) {
    let content: Value = serde_json::from_str(blocks).expect("scripted content array");
    let body = serde_json::json!({
        "id": "msg_loopback",
        "type": "message",
        "role": "assistant",
        "model": "loopback-served-model",
        "content": content,
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 1000, "output_tokens": 200},
    })
    .to_string();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

/// A scripted `redirect <status> <location>` is answered as that redirect, with no body: were it
/// followed, the redirected request would reach this same seat and be counted in its log.
fn redirect(stream: &mut TcpStream, target: &str) {
    let (status, location) = target.split_once(' ').expect("status and location");
    let head = format!(
        "HTTP/1.1 {status} Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.flush();
}

#[test]
fn preview_is_the_same_typed_core_and_questions_are_stable() {
    let room = tempfile::tempdir().expect("room");
    let request = CompileRequest::create("classify-and-route");
    let core = compile(&request).expect("core");
    let first = call(room.path(), &["compile", "classify-and-route", "--json"]);
    let second = call(room.path(), &["compile", "classify-and-route", "--json"]);
    assert_eq!(first.status.code(), Some(2));
    assert_eq!(first.stdout, second.stdout);
    let got = result(&first);
    assert_eq!(got["candidate"], core.candidate.expect("candidate"));
    assert_eq!(got["questions"][0]["key"], "const.request");
    assert_eq!(got["check_preview"]["scope"], "sourceOnly");
    assert_eq!(got["provenance"]["cognition"], "deterministicOnly");
    assert!(got["written"].is_null());
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn cold_authoring_is_bounded_and_ambient_credentials_do_not_opt_in() {
    let room = tempfile::tempdir().expect("room");
    // The strict reader admits "review … and prepare a support reply" as validate + draft
    // (a correct reading that asks only for a model); this test needs a clause the reader
    // cannot consume, so the explicit provider is the one that answers.
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let automatic = command(room.path())
        .env("OPENAI_API_KEY", "not-a-real-key")
        .args(["compile", intent, "--json"])
        .output()
        .expect("CLI");
    assert_eq!(result(&automatic)["compile_version"], 1);
    assert_eq!(
        result(&automatic)["provenance"]["cognition"],
        "deterministicOnly"
    );
    let explicit = call(
        room.path(),
        &[
            "compile",
            intent,
            "--authoring-model",
            "mock/echo",
            "--authoring-strategy",
            "off",
            "--authoring-max-tokens",
            "1024",
            "--authoring-timeout",
            "2",
            "--json",
        ],
    );
    let document = result(&explicit);
    assert_eq!(document["compile_version"], 2, "{document}");
    // One proposal call, plus at most ONE bounded repair call when the mock's evidence is
    // not an excerpt of the request (the verifier's counterexample goes back once).
    let calls = document["provenance"]["authoring"]["calls"].as_u64();
    assert!(matches!(calls, Some(1 | 2)), "{document}");
    assert_ne!(
        document["status"], "ready",
        "a schema mock is not a semantic witness"
    );
    let invalid = call(
        room.path(),
        &[
            "compile",
            intent,
            "--authoring-model",
            "mock/echo",
            "--authoring-max-tokens",
            "0",
            "--json",
        ],
    );
    assert_eq!(invalid.status.code(), Some(3));
    assert!(result(&invalid)["error"].is_object());
}

/// The CLI defaults to escalation after the cold plan fails. Its receipt must include
/// both phases and exactly the calls the seat received; an invalid candidate is never
/// accepted. The seat is a scripted loopback, not the schema mock, so the calls are exact:
/// the mock's plan may buy the cold round's evidence repair, and its native answer carries
/// the same text (`mock`) in `candidate` AND `candidate_lines`, one candidate that is not a
/// workflow. Here the first answer is not a plan and every later answer is ONE lossless
/// candidate that is not a workflow, so each native round is judged and refused.
#[test]
fn default_native_escalation_preserves_calls_and_honors_the_repair_bound() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let not_a_plan = serde_json::json!({"not": "a plan"}).to_string();
    let native_answer = serde_json::json!({
        "candidate": "not a workflow",
        "questions": [],
        "gaps": [],
        "notes": ""
    })
    .to_string();
    for repairs in [0_u64, 1, 3] {
        let seat = LoopbackSeat::start(vec![not_a_plan.clone(), native_answer.clone()]);
        let repairs_arg = repairs.to_string();
        let out = command(room.path())
            .env("NIKA_VLLM_BASE_URL", seat.base())
            .args([
                "compile",
                intent,
                "--authoring-model",
                "vllm/loopback-seat",
                "--authoring-repairs",
                repairs_arg.as_str(),
                // Typed repairs can need 62 requests under escalate (nv1b): granted in full.
                "--authoring-max-calls",
                "62",
                "--authoring-timeout",
                "2",
                "--json",
            ])
            .output()
            .expect("CLI");
        let doc = result(&out);
        assert_eq!(out.status.code(), Some(2), "{doc}");
        assert_eq!(doc["compile_version"], 2);
        assert_eq!(doc["status"], "incomplete");
        assert!(doc["candidate"].is_null());
        let provenance = &doc["provenance"];
        // Within the authority nothing is refused, and every request sent is one received.
        let backend = &provenance["authoring"]["backend"];
        let authority = &backend["authority"];
        assert_eq!(authority["max_calls"], 62, "{doc}");
        assert_eq!(authority["source"], "--authoring-max-calls");
        assert_eq!(authority["http_requests"]["refused"], 0, "{doc}");
        assert_eq!(
            authority["http_requests"]["sent"].as_u64(),
            Some(seat.bodies().len() as u64)
        );
        assert_eq!(backend["requested_model"], "vllm/loopback-seat");
        assert_eq!(
            backend["observed_models"],
            serde_json::json!(["loopback-served-model"])
        );
        assert_eq!(backend["unreported_models"], 0, "{doc}");
        let context = provenance["authoring"]["context"]
            .as_array()
            .expect("calls");
        let phases: Vec<_> = context
            .iter()
            .map(|c| c["call"].as_str().expect("phase"))
            .collect();
        // An answer that is not a plan ends the cold round at once (no anchoring repair).
        let expected: &[&str] = if repairs == 0 {
            &["plan", "native"]
        } else {
            &["plan", "native", "native-repair"]
        };
        assert_eq!(phases, expected, "{doc}");
        assert_eq!(
            provenance["authoring"]["calls"].as_u64(),
            Some(context.len() as u64)
        );
        assert_eq!(
            seat.bodies().len(),
            context.len(),
            "the receipt counts exactly the calls the seat received"
        );
        let native = &provenance["decision"]["native"];
        let rounds = native["rounds"].as_array().expect("native rounds");
        assert!(rounds.len() as u64 <= 1 + repairs, "{doc}");
        assert_eq!(native["accepted"], false);
        assert_eq!(
            rounds.len() + 1,
            context.len(),
            "the cold call remains counted"
        );
        // The repeated candidate stops on no progress, even with repairs left.
        if repairs > 1 {
            assert_eq!(rounds.len(), 2, "{doc}");
        }
        assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
    }
}

/// `--authoring-model` alone authorizes ONE request: the escalation the default strategy would
/// buy after the plan is refused before any byte leaves, and the outcome says so (the receipt's
/// account and one human line), while the core's journal keeps the attempt it made.
#[test]
fn the_default_authority_sends_one_request_and_states_the_refusal() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let not_a_plan = serde_json::json!({"not": "a plan"}).to_string();
    let native_answer =
        serde_json::json!({"candidate": "not a workflow", "questions": [], "gaps": [], "notes": ""})
            .to_string();
    let seat = LoopbackSeat::start(vec![not_a_plan.clone(), native_answer.clone()]);
    let flags = [
        "--authoring-model",
        "vllm/loopback-seat",
        "--authoring-timeout",
        "2",
    ];
    let json_out = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent])
        .args(flags)
        .arg("--json")
        .output()
        .expect("CLI");
    let doc = result(&json_out);
    assert_eq!(seat.bodies().len(), 1, "one request left: {doc}");
    assert_eq!(doc["status"], "incomplete");
    // A stop on the call ceiling is an unfinished outcome, the class the help names (2).
    assert_eq!(json_out.status.code(), Some(2), "{doc}");
    let authoring = &doc["provenance"]["authoring"];
    let authority = &authoring["backend"]["authority"];
    assert_eq!(authority["max_calls"], 1, "{doc}");
    assert_eq!(authority["source"], "default: one request");
    assert_eq!(
        authority["invocations"],
        serde_json::json!({"sent": 1, "refused": 1})
    );
    assert_eq!(
        authority["http_requests"],
        serde_json::json!({"sent": 1, "refused": 0, "unknown": null})
    );
    // The core's journal keeps its attempt; the refusal is recorded beside it, never over it.
    assert_eq!(
        authoring["context"].as_array().map(Vec::len),
        Some(2),
        "{doc}"
    );
    // A local refusal used nothing: the one answered call's usage is the complete total.
    assert_eq!(authoring["input_tokens"], 1000, "{doc}");
    assert_eq!(authoring["output_tokens"], 200, "{doc}");
    assert_eq!(authoring["backend"]["usage_complete"], true, "{doc}");
    assert!(
        doc["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|d| d["message"]
                .as_str()
                .is_some_and(|m| m.contains("refused before any byte left"))),
        "{doc}"
    );
    let seat = LoopbackSeat::start(vec![not_a_plan, native_answer]);
    let human = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent])
        .args(flags)
        .output()
        .expect("CLI");
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains(
            "authority · 1 authoring request(s) authorized · refused before sending: 1 invocation(s), 0 HTTP request(s)"
        ),
        "{text}"
    );
    assert_eq!(seat.bodies().len(), 1);
}

/// A followed redirect is another request carrying the whole prompt, uncounted: the authoring
/// wire never follows one. A seat answering 307 or 308 receives exactly the requests the
/// authority counted (one under the default), never the redirected prompt, and the outcome
/// names the refusal it got.
#[test]
fn a_redirecting_seat_receives_only_the_counted_requests() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    for (status, calls) in [("307", None), ("308", None), ("307", Some("6"))] {
        let seat = LoopbackSeat::start(vec![format!("redirect {status} /v1/moved")]);
        let mut cmd = command(room.path());
        cmd.env("NIKA_VLLM_BASE_URL", seat.base())
            .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
            .args(["--authoring-timeout", "2", "--json"]);
        if let Some(calls) = calls {
            cmd.args(["--authoring-max-calls", calls]);
        }
        let out = cmd.output().expect("CLI");
        let doc = result(&out);
        // A failed provider call leaves the outcome unfinished: exit 2, as the help states.
        assert_eq!(out.status.code(), Some(2), "{status}: {doc}");
        let authority = &doc["provenance"]["authoring"]["backend"]["authority"];
        let counted = authority["http_requests"]["sent"]
            .as_u64()
            .expect("counted");
        let received = seat.bodies().len() as u64;
        assert_eq!(
            received, counted,
            "{status}: every physical request counted: {doc}"
        );
        if calls.is_none() {
            assert_eq!(
                received, 1,
                "{status}: the default sends one request: {doc}"
            );
        }
        assert_eq!(doc["status"], "incomplete", "{doc}");
        // A provider failure may have been billed: the totals are stated incomplete.
        let authoring = &doc["provenance"]["authoring"];
        assert!(authoring["input_tokens"].is_null(), "{status}: {doc}");
        assert_eq!(
            authoring["backend"]["usage_complete"], false,
            "{status}: {doc}"
        );
        let named = doc["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|d| d["message"].as_str().is_some_and(|m| m.contains(status)));
        assert!(
            named,
            "{status}: the outcome names the redirect it refused: {doc}"
        );
    }
}

/// The decision seat is outside the authoring authority (its own client, protocol retries
/// included): the receipt says so whenever one is seated, and never counts its requests as the
/// authority's.
#[test]
fn a_seated_decision_model_is_stated_outside_the_authority() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let not_a_plan = serde_json::json!({"not": "a plan"}).to_string();
    let seat = LoopbackSeat::start(vec![not_a_plan]);
    let out = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
        .args(["--decision-model", "vllm/loopback-seat"])
        .args(["--authoring-timeout", "2", "--json"])
        .output()
        .expect("CLI");
    let doc = result(&out);
    let authority = &doc["provenance"]["authoring"]["backend"]["authority"];
    assert_eq!(
        authority["decision_seat"],
        "outside this authority: its own client, protocol retries included",
        "{doc}"
    );
    let counted = authority["http_requests"]["sent"]
        .as_u64()
        .expect("counted");
    assert!(seat.bodies().len() as u64 >= counted, "{doc}");
    // Without a decision seat, nothing is stated about one.
    let seat = LoopbackSeat::start(vec![serde_json::json!({"not": "a plan"}).to_string()]);
    let out = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
        .args(["--authoring-timeout", "2", "--json"])
        .output()
        .expect("CLI");
    let doc = result(&out);
    let authority = &doc["provenance"]["authoring"]["backend"]["authority"];
    assert!(authority.get("decision_seat").is_none(), "{doc}");
}

/// A response that reports no model identity leaves the identity unknown, never the model the
/// operator requested: the receipt lists no observed model and counts the response apart.
#[test]
fn a_response_without_a_model_is_counted_as_unreported() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let not_a_plan = serde_json::json!({"not": "a plan"}).to_string();
    let seat = LoopbackSeat::start(vec![format!("nomodel {not_a_plan}")]);
    let out = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
        .args(["--authoring-timeout", "2", "--json"])
        .output()
        .expect("CLI");
    let doc = result(&out);
    assert_eq!(seat.bodies().len(), 1, "{doc}");
    let backend = &doc["provenance"]["authoring"]["backend"];
    assert_eq!(backend["requested_model"], "vllm/loopback-seat");
    assert_eq!(backend["observed_models"], serde_json::json!([]), "{doc}");
    assert_eq!(backend["unreported_models"], 1, "{doc}");
}

/// Repairs, samples or a two-request strategy (only, escalate, sketch: a READY is judged) typed
/// beyond the authority are refused before any request: the operator's explicit quality is never
/// reduced in silence, and the number to authorize is named. Repairs under off are the verifier's
/// and count too (nv1b); a typed native-only answer granted its judgment runs.
#[test]
fn a_typed_multiplicity_the_authority_cannot_honor_is_refused_before_any_request() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let answer =
        serde_json::json!({"candidate": "not a workflow", "questions": [], "gaps": [], "notes": ""})
            .to_string();
    for (typed, needed) in [
        (vec!["--authoring-repairs", "3"], "--authoring-max-calls 66"),
        (vec!["--authoring-samples", "2"], "--authoring-max-calls 16"),
        (
            vec!["--authoring-strategy", "off", "--authoring-repairs", "3"],
            "--authoring-max-calls 56",
        ),
        (
            vec!["--authoring-strategy", "only"],
            "--authoring-max-calls 2",
        ),
        (
            vec!["--authoring-strategy", "sketch"],
            "--authoring-max-calls 3",
        ),
        (
            vec!["--authoring-strategy", "escalate"],
            "--authoring-max-calls 2",
        ),
    ] {
        let seat = LoopbackSeat::start(vec![answer.clone()]);
        let out = command(room.path())
            .env("NIKA_VLLM_BASE_URL", seat.base())
            .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
            .args(&typed)
            .arg("--json")
            .output()
            .expect("CLI");
        let doc = result(&out);
        assert_eq!(out.status.code(), Some(2), "{typed:?}: {doc}");
        assert_eq!(doc["error"]["code"], "authoring_authority", "{doc}");
        let message = doc["error"]["message"].as_str().expect("message");
        assert!(message.contains(needed), "{typed:?}: {message}");
        assert!(
            seat.bodies().is_empty(),
            "no request before the refusal: {typed:?}"
        );
    }
    // A typed count outside what the compiler runs is refused by the flag, never clamped.
    for typed in [["--authoring-repairs", "9"], ["--authoring-samples", "0"]] {
        let seat = LoopbackSeat::start(vec![answer.clone()]);
        let out = command(room.path())
            .env("NIKA_VLLM_BASE_URL", seat.base())
            .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
            .args(typed)
            .output()
            .expect("CLI");
        assert_eq!(out.status.code(), Some(2), "{typed:?}");
        assert!(seat.bodies().is_empty(), "no request: {typed:?}");
    }
    let seat = LoopbackSeat::start(vec![answer.clone()]);
    let out = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
        .args([
            "--authoring-strategy",
            "only",
            "--authoring-repairs",
            "0",
            "--authoring-max-calls",
            "2",
            "--json",
        ])
        .output()
        .expect("CLI");
    let doc = result(&out);
    assert_eq!(seat.bodies().len(), 1, "{doc}");
    assert_eq!(
        doc["provenance"]["authoring"]["backend"]["authority"]["http_requests"]["refused"],
        0
    );
    // Repairs under off are the verifier's (nv1b): granted, nothing is refused, and the receipt
    // counts them, never records them as ignored.
    let seat = LoopbackSeat::start(vec![answer]);
    let out = command(room.path())
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", intent, "--authoring-model", "vllm/loopback-seat"])
        .args(["--authoring-strategy", "off", "--authoring-repairs", "3"])
        .args(["--authoring-max-calls", "56"])
        .args(["--authoring-timeout", "2", "--json"])
        .output()
        .expect("CLI");
    let doc = result(&out);
    assert_ne!(doc["error"]["code"], "authoring_authority", "{doc}");
    assert_eq!(seat.bodies().len(), 1, "{doc}");
    let configured = &doc["provenance"]["authoring"]["backend"]["authority"]["configured"];
    assert_eq!(configured["ignored"], serde_json::json!([]), "{doc}");
    assert_eq!(configured["worst_case"], 56, "{doc}");
}

/// The authoring seat rides the PROVIDER client (the runtime's fixed endpoint allowlist,
/// no SSRF floor, a transport ceiling above the requested timeout), never the fetch
/// client: a local seat on `127.0.0.1` reaches its socket and reports the socket's own
/// refusal, not an SSRF refusal — and the same client no longer cuts a cloud authoring
/// call at the fetch client's 30 s idle-read guard (the grok-4.7 408 of the preflight).
#[test]
fn authoring_seat_uses_the_provider_client_not_the_fetch_client() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let out = command(room.path())
        // port 1 refuses at once; the fetch client would refuse the loopback literal itself
        .env("NIKA_OLLAMA_BASE_URL", "http://127.0.0.1:1")
        .args([
            "compile",
            intent,
            "--authoring-model",
            "ollama/qwen3.5:4b",
            "--authoring-timeout",
            "5",
            "--json",
        ])
        .output()
        .expect("CLI");
    let document = result(&out);
    assert_eq!(document["compile_version"], 2, "{document}");
    assert_eq!(
        document["provenance"]["authoring"]["calls"], 1,
        "{document}"
    );
    assert_ne!(document["status"], "ready");
    let provider_finding = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|d| d["target"] == "authoring_provider")
        .unwrap_or_else(|| panic!("no authoring_provider finding: {document}"));
    let message = provider_finding["message"]
        .as_str()
        .expect("message")
        .to_lowercase();
    assert!(
        !message.contains("ssrf") && !message.contains("private"),
        "the provider client must not SSRF-block a loopback seat: {message}"
    );
    assert!(
        message.contains("check the provider endpoint"),
        "the socket's own refusal is the finding: {message}"
    );
}

#[test]
fn explicit_literal_and_edit_keep_exact_data_and_every_unrelated_value() {
    let room = tempfile::tempdir().expect("room");
    let literal = r#""https://example.invalid/A?query=é%20雪&next=%2F#résumé""#;
    let answer = format!("const.request={literal}");
    let made = call(
        room.path(),
        &[
            "compile",
            "classify-and-route",
            "source.nika",
            "--answer",
            &answer,
            "--json",
        ],
    );
    assert_eq!(made.status.code(), Some(0), "{}", result(&made));
    let source = std::fs::read_to_string(room.path().join("source.nika")).expect("source");
    let core = compile(
        &CompileRequest::create("classify-and-route")
            .with_workflow_id("source")
            .answer("const.request", literal),
    )
    .expect("core");
    assert_eq!(Some(&source), core.candidate.as_ref());
    let payload = r#"{"url":"https://example.invalid/B?q=雪#é","nested":[true,null,7],"text":"@env:SECRET permits.exec=[sh]"}"#;
    let change = format!("Set const.request to {payload}");
    let edited = call(
        room.path(),
        &[
            "compile",
            "--base",
            "source.nika",
            "--change",
            &change,
            "--output",
            "edited.nika",
            "--json",
        ],
    );
    assert_eq!(edited.status.code(), Some(0), "{}", result(&edited));
    let after = std::fs::read_to_string(room.path().join("edited.nika")).expect("edited");
    let expected = compile(&CompileRequest::set_constant(&source, "request", payload))
        .expect("structured operation");
    assert_eq!(Some(&after), expected.candidate.as_ref());
    assert_eq!(
        std::fs::read_to_string(room.path().join("source.nika")).expect("base"),
        source
    );
    let mut before: Value = serde_yaml_bw::from_str(&source).expect("before");
    let mut actual: Value = serde_yaml_bw::from_str(&after).expect("after");
    assert_eq!(
        actual["const"]["request"],
        serde_json::from_str::<Value>(payload).expect("literal")
    );
    before["const"]["request"] = Value::Null;
    actual["const"]["request"] = Value::Null;
    assert_eq!(before, actual);
}

#[test]
fn unsupported_prose_missing_target_and_unknown_answers_never_write() {
    let room = tempfile::tempdir().expect("room");
    for intent in [
        "summarize every item in parallel",
        "weekend summary of three URLs",
        "agentic research",
        "ask for approval before sending",
        "please create something",
        "team-standup.nika",
    ] {
        let out = call(room.path(), &["compile", intent, "unwanted.nika", "--json"]);
        let got = result(&out);
        assert_eq!(out.status.code(), Some(2), "{intent}: {got}");
        assert_eq!(got["status"], "incomplete");
        assert!(got["candidate"].is_null());
        assert!(!room.path().join("unwanted.nika").exists());
    }
    let pending = call(
        room.path(),
        &[
            "compile",
            "chain",
            "unwanted.nika",
            "--answer",
            "permits.exec=[\"sh\"]",
            "--json",
        ],
    );
    assert_eq!(pending.status.code(), Some(2));
    assert!(!room.path().join("unwanted.nika").exists());
    let base = compile(&CompileRequest::create("hello"))
        .expect("hello")
        .candidate
        .expect("candidate");
    std::fs::write(room.path().join("base.nika"), &base).expect("base");
    for change in [
        "Set const.missing to 7",
        "Add a Graph approval gate",
        "Use Jev to research this",
    ] {
        let out = call(
            room.path(),
            &[
                "compile",
                "--base",
                "base.nika",
                "--change",
                change,
                "--output",
                "unwanted.nika",
                "--json",
            ],
        );
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(result(&out)["candidate"], base);
        assert!(!room.path().join("unwanted.nika").exists());
    }
}

#[test]
fn destination_conflict_preserves_bytes_and_force_is_explicit() {
    let room = tempfile::tempdir().expect("room");
    let dest = "team's notes.nika";
    std::fs::write(room.path().join(dest), b"existing\x00bytes").expect("seed");
    let out = call(room.path(), &["compile", "hello", dest, "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(result(&out)["error"]["code"], "destination");
    assert_eq!(
        std::fs::read(room.path().join(dest)).expect("bytes"),
        b"existing\x00bytes"
    );
    let out = call(room.path(), &["compile", "hello", dest, "--force"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("nika run 'team'\\''s notes.nika'"));
    let body: Value =
        serde_yaml_bw::from_str(&std::fs::read_to_string(room.path().join(dest)).expect("file"))
            .expect("yaml");
    assert_eq!(body["nika"], "team-s-notes");
    assert_eq!(body["model"], "mock/echo");
    let ignore = std::fs::read_to_string(room.path().join(".gitignore")).expect("trace protection");
    assert!(ignore.contains(".nika/traces/"));
    assert!(!room.path().join(".nika").exists(), "Compile did not run");
}

#[test]
fn hello_alias_is_the_same_core_and_ambient_keys_never_select_a_provider() {
    let room = tempfile::tempdir().expect("room");
    let hello = compile(&CompileRequest::create("hello")).expect("core");
    let numbered = compile(&CompileRequest::create("01-hello")).expect("core alias");
    assert_eq!(hello.status, CompileStatus::Ready);
    assert_eq!(hello.candidate, numbered.candidate);
    let out = command(room.path())
        .args(["compile", "hello", "hello.nika", "--json"])
        .env("TYPESAFE_API_KEY", "synthetic-typesafe-canary")
        .env("OPENAI_API_KEY", "synthetic-openai-canary")
        .env("ANTHROPIC_API_KEY", "synthetic-anthropic-canary")
        .env("XAI_API_KEY", "synthetic-xai-canary")
        .output()
        .expect("keyed Compile");
    assert!(out.status.success(), "{}", result(&out));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("canary"));
    let source = std::fs::read_to_string(room.path().join("hello.nika")).expect("hello");
    let yaml: Value = serde_yaml_bw::from_str(&source).expect("yaml");
    assert_eq!(yaml["model"], "mock/echo");
    assert!(!room.path().join(".nika").exists());
    let run = command(room.path())
        .args(["run", "hello.nika", "--output", "json"])
        .env("TYPESAFE_API_KEY", "synthetic-typesafe-canary")
        .env("OPENAI_API_KEY", "synthetic-openai-canary")
        .env("ANTHROPIC_API_KEY", "synthetic-anthropic-canary")
        .env("XAI_API_KEY", "synthetic-xai-canary")
        .output()
        .expect("keyed mock run");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        result(&run)["greeting"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}

#[test]
fn no_model_template_uses_core_and_invalid_adapter_inputs_are_explicit() {
    let room = tempfile::tempdir().expect("room");
    let out = call(room.path(), &["compile", "gate-and-act", "--json"]);
    let core = compile(&CompileRequest::create("gate-and-act")).expect("core");
    assert_eq!(
        result(&out)["candidate"],
        core.candidate.expect("candidate")
    );
    assert_eq!(out.status.code(), Some(0));
    let bad = call(
        room.path(),
        &["compile", "hello", "--answer", "broken", "--json"],
    );
    assert_eq!(bad.status.code(), Some(2));
    assert_eq!(result(&bad)["error"]["code"], "invalid_answer");
    let missing = call(
        room.path(),
        &[
            "compile",
            "--base",
            "absent.nika",
            "--change",
            "Set const.x to 1",
            "--json",
        ],
    );
    assert_eq!(missing.status.code(), Some(3));
    assert_eq!(result(&missing)["error"]["code"], "read_base");
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn noncanonical_destinations_refuse_without_any_file() {
    let room = tempfile::tempdir().expect("room");
    for dest in ["hello.nika.yml", "hello.yaml", "hello", "hello.NIKA.YAML"] {
        let out = call(room.path(), &["compile", "hello", dest, "--json"]);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(result(&out)["error"]["code"], "destination_name");
        assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
    }
}

#[test]
fn failed_trace_protection_preserves_existing_destination_and_drops_temporary_file() {
    let room = tempfile::tempdir().expect("room");
    std::fs::create_dir(room.path().join(".gitignore")).expect("unwritable protection target");
    std::fs::write(room.path().join("hello.nika"), "original bytes").expect("seed");
    let out = call(
        room.path(),
        &["compile", "hello", "hello.nika", "--force", "--json"],
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(result(&out)["error"]["code"], "destination");
    assert_eq!(
        std::fs::read_to_string(room.path().join("hello.nika")).expect("original"),
        "original bytes"
    );
    assert_eq!(
        std::fs::read_dir(room.path()).expect("dir").count(),
        2,
        "no temporary file survives"
    );
    let out = call(room.path(), &["compile", "hello", "absent.nika", "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(!room.path().join("absent.nika").exists());
}

#[test]
fn concurrent_creators_publish_exactly_one_complete_candidate() {
    let room = tempfile::tempdir().expect("room");
    let args = ["compile", "hello", "race.nika", "--json"];
    let mut first = command(room.path())
        .args(args)
        .stdout(Stdio::null())
        .spawn()
        .expect("first creator");
    let mut second = command(room.path())
        .args(args)
        .stdout(Stdio::null())
        .spawn()
        .expect("second creator");
    let mut codes = [
        first.wait().expect("first status").code(),
        second.wait().expect("second status").code(),
    ];
    codes.sort();
    assert_eq!(codes, [Some(0), Some(3)]);
    let expected =
        compile(&CompileRequest::create("hello").with_workflow_id("race")).expect("core");
    assert_eq!(
        std::fs::read_to_string(room.path().join("race.nika")).expect("complete file"),
        expected.candidate.expect("candidate")
    );
}

#[test]
fn create_identity_cannot_rename_an_edit_and_expression_answers_stay_refused() {
    let source = compile(
        &CompileRequest::create("classify-and-route").answer("const.request", "\"original\""),
    )
    .expect("core")
    .candidate
    .expect("candidate");
    let renamed =
        compile(&CompileRequest::edit(&source, "Set const.request to 1").with_workflow_id("other"))
            .expect("rename refusal");
    assert_eq!(renamed.status, CompileStatus::Refused);
    assert_eq!(renamed.candidate.as_deref(), Some(source.as_str()));
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("base.nika"), &source).expect("base");
    let out = call(
        room.path(),
        &[
            "compile",
            "--base",
            "base.nika",
            "--change",
            "Set const.request to \"${{ env.SECRET }}\"",
            "--output",
            "refused.nika",
            "--json",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(result(&out)["status"], "refused");
    assert_eq!(result(&out)["candidate"], source);
    assert!(!room.path().join("refused.nika").exists());
}

#[test]
fn source_only_preview_does_not_read_ambient_project_policy() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(
        room.path().join("nika.yaml"),
        "this is not valid project settings: [",
    )
    .expect("poison ambient policy");
    let out = call(room.path(), &["compile", "hello", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(result(&out)["check_preview"]["scope"], "sourceOnly");
    let expected = compile(&CompileRequest::create("hello")).expect("pure core");
    assert_eq!(
        result(&out)["candidate"],
        expected.candidate.expect("candidate")
    );
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 1);
}

#[test]
fn an_explicit_dash_prefixed_path_teaches_a_runnable_file_argument() {
    let room = tempfile::tempdir().expect("room");
    let out = call(room.path(), &["compile", "hello", "--output=-hello.nika"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("nika run ./-hello.nika"));
    let run = call(room.path(), &["run", "./-hello.nika", "--output", "json"]);
    assert!(run.status.success());
    assert!(result(&run)["greeting"].as_str().is_some());
}

/// zsh rewrites a bare word that STARTS with `=` (`=ls` becomes the path of
/// ls, an unknown name aborts the line); sh reads it literally. The taught
/// word must read back verbatim in both. Only side-effect-free names reach a
/// real shell here: `printf` prints, nothing else runs.
#[cfg(unix)]
#[test]
fn an_equals_prefixed_path_is_taught_as_a_word_no_shell_rewrites() {
    let room = tempfile::tempdir().expect("room");
    for name in ["=ls.nika", "=value.nika"] {
        let out = call(room.path(), &["compile", "hello", name]);
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let taught = text
            .lines()
            .find_map(|line| line.strip_prefix("next · nika run "))
            .expect("taught run line");
        for shell in [&["/bin/sh", "-c"][..], &["/bin/zsh", "-f", "-c"][..]] {
            if !Path::new(shell[0]).exists() {
                continue;
            }
            let read = Command::new(shell[0])
                .args(&shell[1..])
                .arg(format!("printf '%s' {taught}"))
                .env_clear()
                .current_dir(room.path())
                .stdin(Stdio::null())
                .output()
                .expect("shell");
            assert!(read.status.success(), "{} refused {taught}", shell[0]);
            assert_eq!(String::from_utf8_lossy(&read.stdout), name, "{}", shell[0]);
        }
        let run = call(room.path(), &["run", name, "--output", "json"]);
        assert!(run.status.success(), "{run:?}");
    }
}

/// The shared parity set (`nika-compile/tests/fixtures/compile_parity_v1.json`) also
/// drives the core's own wire test and the Serve door: three doors, one document.
#[test]
fn the_cli_door_prints_the_core_document_for_every_shared_parity_case() {
    use nika_onboard::compile::outcome_document;

    let fixture: Value = serde_json::from_str(include_str!(
        "../../nika-compile/tests/fixtures/compile_parity_v1.json"
    ))
    .expect("parity fixture");
    let mut judged = 0;
    for case in fixture["cases"].as_array().expect("cases") {
        let doors = case["doors"].as_array().expect("doors");
        if !doors.iter().any(|door| door == "cli") {
            continue;
        }
        let name = case["name"].as_str().expect("case name");
        let native = &case["native"];
        let text = |key: &str| native[key].as_str().expect("native text field");
        let room = tempfile::tempdir().expect("room");
        let mut args = vec!["compile".to_owned()];
        let mut request = if text("mode") == "create" {
            args.push(text("intent").to_owned());
            CompileRequest::create(text("intent"))
        } else {
            assert_eq!(
                text("mode"),
                "edit",
                "{name}: the CLI has no structured edit flag"
            );
            let source = fixture["sources"][text("source_ref")]
                .as_str()
                .expect("named source");
            std::fs::write(room.path().join("base.nika"), source).expect("base");
            for word in ["--base", "base.nika", "--change", text("change_text")] {
                args.push(word.to_owned());
            }
            CompileRequest::edit(source, text("change_text"))
        };
        for pair in native["answers"].as_array().into_iter().flatten() {
            let key = pair[0].as_str().expect("answer key");
            let literal = pair[1].as_str().expect("answer literal text");
            args.push("--answer".to_owned());
            args.push(format!("{key}={literal}"));
            request = request.answer(key, literal);
        }
        args.push("--json".to_owned());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = call(room.path(), &args);
        let mut printed = result(&out);
        // `written` is the one fact only this door owns; the rest is the core's.
        let written = printed.as_object_mut().expect("document").remove("written");
        assert_eq!(
            written,
            Some(Value::Null),
            "{name}: a preview writes nothing"
        );
        // Both sides cross the same printer and parser, so a float in the Check report
        // cannot differ by a parse round trip alone.
        let core = outcome_document(&compile(&request).expect("core")).to_string();
        let core: Value = serde_json::from_str(&core).expect("core document");
        assert_eq!(printed, core, "{name}");
        let expected_exit = if core["status"] == "ready" { 0 } else { 2 };
        assert_eq!(out.status.code(), Some(expected_exit), "{name}");
        if let Some(status) = case["expect"]["status"].as_str() {
            assert_eq!(printed["status"], status, "{name}");
        }
        judged += 1;
    }
    assert!(judged >= 25, "the CLI parity set shrank: {judged}");
}

#[test]
fn the_native_identity_advertises_the_compile_wire_it_really_serves() {
    let room = tempfile::tempdir().expect("room");
    let identity = result(&call(room.path(), &["--sdk-identity"]));
    assert!(
        identity["supportedCapabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|token| token == "compile"),
        "{identity}"
    );
    // The token is a promise about THIS door: it answers the generation it names.
    let listed = result(&call(room.path(), &["compile", "--list", "--json"]));
    assert_eq!(listed["compile_version"], 1);
    let hello = result(&call(room.path(), &["compile", "hello", "--json"]));
    assert_eq!(hello["compile_version"], 1);
    assert_eq!(hello["status"], "ready");
}
