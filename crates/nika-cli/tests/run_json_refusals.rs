// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! #1650: one compact JSON object per line on every run machine path.
//! Subprocesses are the CLI under test, in an isolated home without keys.

use std::net::TcpListener;
use std::process::{Command, Output};

use serde_json::Value;

const HELLO: &str = "nika: framing-hello\nmodel: mock/echo\npermits: {}\ntasks:\n  greet:\n    infer: { prompt: hello, max_tokens: 32 }\noutputs:\n  greeting: ${{ tasks.greet.output }}\n";

fn execute(source: &str, extra: &[&str]) -> (tempfile::TempDir, Output) {
    execute_on(source, extra, true)
}

/// `canary_route` confines provider traffic to an owned listener that must stay silent. That
/// override is an unpriced route, which run cost admission refuses before the budget floor; the
/// floor (NIKA-1709) judges only the provider's priced default route, which no canary can observe
/// because the HTTP client ignores proxies.
fn execute_on(source: &str, extra: &[&str], canary_route: bool) -> (tempfile::TempDir, Output) {
    let dir = tempfile::tempdir().expect("isolated room");
    let canary = TcpListener::bind("127.0.0.1:0").expect("owned endpoint");
    canary.set_nonblocking(true).expect("nonblocking canary");
    let endpoint = format!(
        "http://{}/v1/messages",
        canary.local_addr().expect("address")
    );
    std::fs::write(dir.path().join("case.nika"), source).expect("fixture");
    let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .args(["run", "case.nika", "--json", "--no-gc", "--color", "never"])
        .args(extra)
        .env_clear()
        .env("HOME", dir.path())
        .env("TERM", "dumb")
        .env("NIKA_KEYCHAIN", "off")
        // Synthetic readiness, with all provider traffic confined to our canary.
        .env(
            "ANTHROPIC_API_KEY",
            "sk-ant-api03-framing-fixture-not-a-real-key",
        )
        .current_dir(dir.path());
    if canary_route {
        command.env("NIKA_ANTHROPIC_BASE_URL", endpoint);
    }
    let result = command.output().expect("binary runs");
    assert_eq!(
        canary
            .accept()
            .expect_err("no provider I/O on any fixture")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    (dir, result)
}

fn frames(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("each line must parse: {e}: {line}"));
            assert!(value.is_object(), "each frame is an object: {value}");
            value
        })
        .collect()
}

fn refused(source: &str, flags: &[&str], expected: &str) {
    refused_on(source, flags, expected, true);
}

fn refused_on(source: &str, flags: &[&str], expected: &str, canary_route: bool) {
    let (dir, output) = execute_on(source, flags, canary_route);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let values = frames(&output);
    assert_eq!(
        values.len(),
        1,
        "pre-admission emits one object: {values:?}"
    );
    let report = &values[0];
    let code_found = report["error"]["code"] == expected
        || report["findings"]
            .as_array()
            .is_some_and(|findings| findings.iter().any(|finding| finding["code"] == expected));
    assert!(code_found, "the actual refusal code survives: {report}");
    assert!(
        report.get("receipt").is_none(),
        "no execution proof before admission"
    );
    assert!(
        !dir.path().join(".nika/traces").exists(),
        "refusal never starts a trace"
    );
}

#[test]
fn check_refusals_keep_their_real_findings_on_one_line() {
    refused(
        "nika: denied\nmodel: mock/echo\npermits: {}\ntasks:\n  pwn:\n    exec: { command: [echo, pwn] }\n",
        &[],
        "NIKA-SEC-004",
    );
    refused(
        &HELLO.replace("${{ tasks.greet.output }}", "{ from: greet }"),
        &[],
        "NIKA-PARSE-005",
    );
    refused(
        "nika: absent-boundary\ntasks:\n  pwn:\n    exec: { command: [echo, pwn] }\n",
        &[],
        "NIKA-AUTH-006",
    );
}

#[test]
fn a_cost_refusal_is_json_before_any_provider_call() {
    // Synthetic access is configured; the positive floor must refuse
    // before provider I/O, independently of missing-access diagnostics.
    // The floor judges the priced default route.
    let source = HELLO
        .replace("mock/echo", "anthropic/claude-sonnet-5")
        .replace("max_tokens: 32", "max_tokens: 1000");
    refused_on(&source, &["--max-cost-usd", "0"], "NIKA-1709", false);
    // The canary override is unpriced: run cost admission refuses it before
    // the floor, still as one JSON object and before any provider I/O.
    let (dir, output) = execute(&source, &["--max-cost-usd", "0"]);
    assert!(!output.status.success(), "{output:?}");
    let values = frames(&output);
    assert_eq!(
        values.len(),
        1,
        "pre-admission emits one object: {values:?}"
    );
    assert!(
        values[0]["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown-cost admission")),
        "the unpriced route is refused by cost admission: {values:?}"
    );
    assert!(
        values[0].get("receipt").is_none(),
        "no execution proof before admission"
    );
    assert!(
        !dir.path().join(".nika/traces").exists(),
        "refusal never starts a trace"
    );
}

#[test]
fn hello_keeps_its_six_compact_lifecycle_events() {
    let (_dir, output) = execute(HELLO, &[]);
    assert!(output.status.success(), "{output:?}");
    let values = frames(&output);
    let kinds: Vec<_> = values
        .iter()
        .map(|v| v["kind"].as_str().expect("event kind"))
        .collect();
    assert_eq!(
        kinds,
        [
            "workflow_started",
            "task_scheduled",
            "task_started",
            "task_completed",
            "workflow_completed",
            "run_settled"
        ]
    );
    assert_eq!(values.last().expect("terminal")["status"], "succeeded");
}

#[test]
fn admitted_failure_keeps_a_terminal_execution_frame() {
    let (_dir, output) = execute(
        "nika: failed\npermits: { tools: [nika:assert] }\ntasks:\n  fail:\n    invoke: { tool: nika:assert, args: { condition: false, message: intentional } }\n",
        &[],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let values = frames(&output);
    assert_eq!(values.first().expect("started")["kind"], "workflow_started");
    assert_eq!(values.last().expect("terminal")["kind"], "run_settled");
    assert_eq!(values.last().expect("terminal")["status"], "failed");
}
