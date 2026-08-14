// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / resume_e2e.rs).
#![allow(clippy::disallowed_types)]

//! ADR-111 conformance — outbound pause delivery at the BINARY plane:
//!
//! - **default OFF** — a pausing run with no `NIKA_NOTIFY_URL` journals
//!   no `notify_*` kind (and opens no socket — nothing listens, nothing
//!   fails).
//! - **delivery** — a local listener receives ONE structured `CloudEvents`
//!   POST whose required attributes and `data` match the pause payload;
//!   the trace gains `notify_delivered`, journaled AFTER the pause frame
//!   and BEFORE any seal (the chain covers the delivery claim). With a
//!   `whsec_` secret configured the Standard Webhooks headers ride:
//!   `webhook-id` (= the deterministic `CloudEvents` id) ·
//!   `webhook-timestamp` · `webhook-signature` (`v1,`-prefixed).
//! - **SSRF refusal** — a metadata-range target yields no POST, a
//!   `notify_failed` carrying the `ssrf_blocked` class, and an unchanged
//!   `paused` exit (the boundary admits the host; the always-on floor
//!   still refuses the range — only an EXACT loopback literal is carved
//!   out, per `permits.net.http` semantics).
//! - **failure is not control flow** — an unreachable target still exits
//!   `paused` with the same code, `notify_failed` journaled.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    // Env hygiene: the suite controls the notify surface explicitly —
    // an operator's real config must never leak into a verdict.
    c.env_remove("NIKA_NOTIFY_URL");
    c.env_remove("NIKA_NOTIFY_SECRET");
    c
}

/// A per-test working dir (own `.nika/traces` store — no cross-talk).
fn workdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-notify-e2e").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("workdir");
    dir
}

const GATE: &str = "nika: gate-probe\n\ntasks:\n  \
                    ask_operator:\n    invoke:\n      tool: \"nika:prompt\"\n      \
                    args:\n        mode: confirm\n        \
                    message: \"Deploy to production?\"\n";

fn write_gate(dir: &std::path::Path) {
    std::fs::write(dir.join("gate.nika.yaml"), GATE).expect("fixture");
}

/// The single trace the run wrote.
fn trace_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let store = dir.join(".nika").join("traces");
    let entry = std::fs::read_dir(&store)
        .expect("trace store exists")
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|x| x == "ndjson"))
        .expect("one trace file");
    let text = std::fs::read_to_string(entry.path()).expect("trace reads");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("journal line is JSON"))
        .collect()
}

fn kind_index(lines: &[serde_json::Value], kind: &str) -> Option<usize> {
    lines
        .iter()
        .position(|l| l.get("kind").and_then(|k| k.as_str()) == Some(kind))
}

fn field<'a>(line: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    line.get("fields")?
        .as_array()?
        .iter()
        .find(|kv| kv.get("key").and_then(|k| k.as_str()) == Some(key))?
        .get("value")?
        .as_str()
}

#[test]
fn no_config_no_notify_kinds() {
    let dir = workdir("default-off");
    write_gate(&dir);
    let status = bin()
        .current_dir(&dir)
        .args(["run", "gate.nika.yaml", "--json"])
        .status()
        .expect("binary runs");
    assert_eq!(status.code(), Some(4), "the run pauses");
    let lines = trace_lines(&dir);
    assert!(kind_index(&lines, "workflow_paused").is_some());
    assert!(
        kind_index(&lines, "notify_delivered").is_none()
            && kind_index(&lines, "notify_failed").is_none(),
        "no URL configured ⇒ no notify event at all"
    );
}

/// Accept ONE request, capture it whole, answer 200.
///
/// A plain OS thread by design: the suite's tests are sync `#[test]`
/// fns spawning the real binary — there is no tokio runtime to spawn
/// on, and the listener's whole life is one blocking accept+read.
#[allow(clippy::disallowed_methods)]
fn one_shot_listener() -> (u16, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        // Read headers, then exactly content-length body bytes.
        let (mut header_end, mut content_len) = (None, 0usize);
        loop {
            let Ok(n) = stream.read(&mut chunk) else {
                break;
            };
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if header_end.is_none()
                && let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n")
            {
                header_end = Some(pos + 4);
                let head = String::from_utf8_lossy(&buf[..pos]);
                content_len = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().ok())?
                    })
                    .unwrap_or(0);
            }
            if let Some(h) = header_end
                && buf.len() >= h + content_len
            {
                break;
            }
        }
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
        let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
    });
    (port, rx)
}

#[test]
fn delivery_is_a_signed_cloudevents_post_journaled_before_the_seal() {
    let dir = workdir("delivery");
    write_gate(&dir);
    let (port, rx) = one_shot_listener();
    let status = bin()
        .current_dir(&dir)
        .env("NIKA_NOTIFY_URL", format!("http://127.0.0.1:{port}/hook"))
        .env(
            "NIKA_NOTIFY_SECRET",
            "whsec_bmlrYS10ZXN0LXNlY3JldC0zMi1ieXRlcy1sb25nISE=",
        )
        .args(["run", "gate.nika.yaml", "--json"])
        .status()
        .expect("binary runs");
    assert_eq!(status.code(), Some(4), "delivery never changes the verdict");

    let request = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the listener received the POST");
    let head = request.split("\r\n\r\n").next().expect("request head");
    assert!(request.starts_with("POST /hook HTTP/1.1"), "one POST");
    let header = |name: &str| -> Option<String> {
        head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
        })
    };
    assert_eq!(
        header("content-type").as_deref(),
        Some("application/cloudevents+json"),
        "CloudEvents structured mode"
    );
    let webhook_id = header("webhook-id").expect("webhook-id always rides");
    assert_eq!(webhook_id.len(), 64, "the deterministic sha256-hex id");
    let ts = header("webhook-timestamp").expect("webhook-timestamp always rides");
    assert!(ts.parse::<i64>().is_ok(), "unix seconds");
    let sig = header("webhook-signature").expect("secret configured ⇒ signed");
    assert!(sig.starts_with("v1,"), "Standard Webhooks v1 scheme");

    let body: serde_json::Value =
        serde_json::from_str(request.split("\r\n\r\n").nth(1).expect("request body"))
            .expect("body is JSON");
    assert_eq!(body["specversion"], "1.0");
    assert_eq!(body["type"], "sh.nika.run.paused");
    assert_eq!(body["id"], serde_json::Value::String(webhook_id));
    assert_eq!(body["subject"], "task:ask_operator");
    assert_eq!(body["data"]["workflow"], "gate-probe");
    assert_eq!(body["data"]["task"], "ask_operator");
    assert_eq!(body["data"]["mode"], "confirm");
    assert!(
        body["data"]["resume_hint"]
            .as_str()
            .expect("resume hint rides")
            .contains("--resume"),
        "the teaching line travels with the question"
    );

    let lines = trace_lines(&dir);
    let paused = kind_index(&lines, "workflow_paused").expect("pause journaled");
    let delivered = kind_index(&lines, "notify_delivered").expect("delivery journaled");
    assert!(paused < delivered, "the outcome narrates the pause");
    assert_eq!(
        field(&lines[delivered], "target_host"),
        Some("127.0.0.1"),
        "the journal names the target"
    );
    if let Some(sealed) = kind_index(&lines, "run_sealed") {
        assert!(
            delivered < sealed,
            "the delivery claim lands under the chain the seal covers"
        );
    }
}

#[test]
fn metadata_range_refuses_at_the_ssrf_floor() {
    let dir = workdir("ssrf");
    write_gate(&dir);
    let status = bin()
        .current_dir(&dir)
        .env("NIKA_NOTIFY_URL", "http://169.254.169.254/latest/meta-data")
        .args(["run", "gate.nika.yaml", "--json"])
        .status()
        .expect("binary runs");
    assert_eq!(status.code(), Some(4), "refusal never changes the verdict");
    let lines = trace_lines(&dir);
    let failed = kind_index(&lines, "notify_failed").expect("the refusal is journaled");
    assert_eq!(
        field(&lines[failed], "error"),
        Some("ssrf_blocked"),
        "the floor's refusal is named, not hidden"
    );
    assert!(kind_index(&lines, "notify_delivered").is_none());
}

#[test]
fn unreachable_target_is_journaled_never_fatal() {
    let dir = workdir("unreachable");
    write_gate(&dir);
    // Port 9 (discard) on loopback — the carve-out admits the host, the
    // connection refuses. Nothing listens on it in any sane environment.
    let status = bin()
        .current_dir(&dir)
        .env("NIKA_NOTIFY_URL", "http://127.0.0.1:9/hook")
        .args(["run", "gate.nika.yaml", "--json"])
        .status()
        .expect("binary runs");
    assert_eq!(
        status.code(),
        Some(4),
        "delivery failure is never the run's failure"
    );
    let lines = trace_lines(&dir);
    assert!(kind_index(&lines, "workflow_paused").is_some());
    let failed = kind_index(&lines, "notify_failed").expect("the failure is journaled");
    assert!(
        matches!(
            field(&lines[failed], "error"),
            Some("transport" | "timeout")
        ),
        "a coarse, stable error class"
    );
}
