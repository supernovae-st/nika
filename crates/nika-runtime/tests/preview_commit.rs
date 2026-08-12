// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! F-P6 · PREVIEW→COMMIT — the positive + regression fixtures, end to
//! end (design §4 a/d · the negative fixtures live in-crate, next to the
//! cfg(test)-only mutation seam): an honest run FIRES the judged request
//! and the terminal frame swears it (`preview_digest == commit_digest`),
//! with ZERO `divergence` finding in the journal. The digests are
//! recomputed INDEPENDENTLY by the fixture — the receipt's word is
//! checked against the canonical form, never trusted.

use std::sync::Arc;

use nika_check::check;
use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::{FileId, ParseMode, parse};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};

/// Run one check-clean workflow on the mock plane.
async fn run(yaml: &str, shell: MockShell, tools: MockToolExecutor) -> (RunOutcome, VecSink) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(
        report.is_clean(),
        "the fixtures are honest workflows: {:?}",
        report.findings
    );
    let tools = Arc::new(tools);
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink)
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            FieldValue::String(s) => Some(s.as_str()),
            _ => None,
        })
}

/// The `task_completed` frame of one task.
fn completed_frame<'a>(sink: &'a VecSink, task: &str) -> &'a Event {
    sink.events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some(task))
        .expect("the task_completed frame")
}

/// The receipt family's digest law, replicated fixture-side (JCS
/// RFC 8785 + the number-fold + blake3): the INDEPENDENT recomputation
/// the design's regression fixture demands — the runtime's digests must
/// equal THIS, not merely each other.
fn fixture_digest(request: &Value) -> String {
    let folded = fold_numbers(request.clone());
    let bytes = serde_json_canonicalizer::to_vec(&folded).expect("the shape canonicalizes");
    blake3::hash(&bytes).to_hex().to_string()
}

/// The number-fold law (a u64 beyond 2^53 must never collide with its
/// ES6-double spelling under JCS alone).
fn fold_numbers(value: Value) -> Value {
    const MARK: char = '\u{f8ff}';
    match value {
        Value::Number(n) => Value::String(format!("{MARK}num:{n}{MARK}")),
        Value::Array(items) => Value::Array(items.into_iter().map(fold_numbers).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, fold_numbers(v))).collect())
        }
        scalar => scalar,
    }
}

/// Design §4 (a) — a step previewed then committed UNCHANGED fires, and
/// the receipt carries `preview_digest == commit_digest`, both equal to
/// the fixture's independent recomputation of the canonical request.
#[tokio::test]
async fn an_unchanged_request_fires_and_attests() {
    let yaml = "nika: fp6-positive\npermits:\n  exec: [\"echo\"]\n  tools: [\"mcp:docs/read\"]\ntasks:\n  say:\n    exec: { command: [\"echo\", \"hello-fp6\"] }\n  read:\n    after: { say: success }\n    invoke:\n      tool: \"mcp:docs/read\"\n      args: { path: \"/readme\" }\n";
    let shell = MockShell::new().enqueue_ok("hello-fp6\n");
    let tools = MockToolExecutor::new().enqueue_ok(ToolResult::success("r1", "doc contents"));
    let (outcome, sink) = run(yaml, shell, tools).await;
    assert!(outcome.ok, "an honest run fires: {:?}", outcome.records);
    assert_eq!(outcome.records["say"].status, TaskStatus::Success);
    assert_eq!(outcome.records["read"].status, TaskStatus::Success);

    // The exec step's digests — equal to the fixture's OWN recompute of
    // the canonical request (argv ordered · cwd/env/stdin judged).
    let say = completed_frame(&sink, "say");
    let expected_exec = fixture_digest(&json!({
        "v": 1,
        "exec": {
            "command": { "argv": ["echo", "hello-fp6"] },
            "cwd": null,
            "env": {},
            "stdin": null,
        },
    }));
    let preview = str_field(say, "preview_digest").expect("the preview rides");
    let commit = str_field(say, "commit_digest").expect("the commit rides");
    assert_eq!(preview, commit, "the judged request IS the fired request");
    assert_eq!(
        preview, expected_exec,
        "the digest recomputes independently (the fixture does not trust)"
    );
    assert_eq!(preview.len(), 64, "blake3 hex");

    // The invoke step's digests — args triés JCS over the rendered map.
    let read = completed_frame(&sink, "read");
    let expected_invoke = fixture_digest(&json!({
        "v": 1,
        "invoke": {
            "tool": "mcp:docs/read",
            "args": { "path": "/readme" },
        },
    }));
    let preview = str_field(read, "preview_digest").expect("the preview rides");
    let commit = str_field(read, "commit_digest").expect("the commit rides");
    assert_eq!(preview, commit);
    assert_eq!(preview, expected_invoke);
    assert_ne!(
        expected_exec, expected_invoke,
        "two different requests never collide"
    );
}

/// Design §4 (d) — the regression green: an honest workflow (with a
/// RENDERED interpolation, so the digested form is the resolved one)
/// carries ZERO `divergence` finding and zero NIKA-SEC-011 anywhere.
#[tokio::test]
async fn an_honest_run_carries_zero_divergence_finding() {
    let yaml = "nika: fp6-green\ninputs:\n  msg: { type: string, default: \"rendered-fp6\" }\npermits:\n  exec: [\"echo\"]\ntasks:\n  say:\n    with: { msg: \"${{ inputs.msg }}\" }\n    exec: { command: [\"echo\", \"${{ with.msg }}\"] }\n";
    let shell = MockShell::new().enqueue_ok("rendered-fp6\n");
    let (outcome, sink) = run(yaml, shell, MockToolExecutor::new()).await;
    assert!(outcome.ok);

    for event in sink.events() {
        assert!(
            str_field(event, "divergence").is_none(),
            "no finding rides an honest journal: {event:?}"
        );
    }
    for record in outcome.records.values() {
        if let Some(err) = &record.error {
            assert_ne!(err.code, "NIKA-SEC-011", "no commit refusal fired");
        }
    }
    // …and the resolved (rendered) argv is what the digest names.
    let say = completed_frame(&sink, "say");
    let expected = fixture_digest(&json!({
        "v": 1,
        "exec": {
            "command": { "argv": ["echo", "rendered-fp6"] },
            "cwd": null,
            "env": {},
            "stdin": null,
        },
    }));
    assert_eq!(
        str_field(say, "preview_digest"),
        Some(expected.as_str()),
        "the digest names the RESOLVED form (the judged bytes)"
    );
    assert_eq!(
        str_field(say, "preview_digest"),
        str_field(say, "commit_digest")
    );
}
