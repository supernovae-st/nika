// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The resumed leg is the real `nika` binary (CARGO_BIN_EXE): spawning it is
// this suite's job (the resume_e2e.rs carve-out class).
#![allow(clippy::disallowed_types)]

//! NIK-245 at the binary plane: `nika run --resume <trace> --answer keep=…`
//! answers a gate whose paused leg already spent part of its ceiling.
//!
//! The paused leg is recorded in-process by the real runtime through the
//! real journal writer (`TraceFileSink`): a priced builtin (`nika:image_generate`
//! on xAI, a $0.02 catalog floor) reports its $0.015 spend on the honest-spend
//! channel, then the confirm gate parks the run under its $0.03 ceiling. The
//! resume states no ceiling, so it continues under the $0.015 left; the CLI
//! preflight priced the WHOLE workflow again ($0.02, the image a cache hit
//! included) and refused NIKA-1709 for « non » and « oui » alike. No provider
//! is reached: the image is served from the journal.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use nika_dap::journal::TraceFileSink;
use nika_kernel::tool_executor::{ToolErrorMeta, ToolResult};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{Runtime, RuntimeConfig, SystemStamper};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};

const WORKFLOW: &str = r#"nika: resume-floor-e2e
permits:
  tools: ["nika:image_generate", "nika:prompt", "nika:jq"]
  fs: { write: ["./out/**"] }
tasks:
  og:
    invoke: { tool: "nika:image_generate", args: { provider: xai, prompt: "a monarch butterfly", output_dir: "./out" } }
  keep:
    with: { image: "${{ tasks.og.output }}" }
    invoke: { tool: "nika:prompt", args: { mode: "confirm", message: "Keep ${{ with.image }}?" } }
  note:
    with: { kept: "${{ tasks.keep.output }}" }
    when: "${{ with.kept == true }}"
    invoke: { tool: "nika:jq", args: { input: { kept: "${{ with.kept }}" }, expression: ".kept" } }
"#;

/// The paused leg's ceiling: it admits the image's $0.02 floor.
const CEILING: f64 = 0.03;
/// What the image reported it cost: more than the ceiling minus the floor.
const SPENT: f64 = 0.015;

fn blocked_prompt() -> ToolResult {
    let mut result = ToolResult::error("keep", "non-interactive and no `default:`");
    result.error_meta = Some(ToolErrorMeta::new(
        Some("NIKA-BUILTIN-PROMPT-001".to_owned()),
        false,
    ));
    result
}

/// Record the paused leg into `<room>/.nika/traces` and return its journal.
fn paused_leg(room: &Path) -> PathBuf {
    let mode = nika_schema::ParseMode::Strict;
    let wf = nika_schema::parse(WORKFLOW, nika_schema::FileId::new(0), mode).expect("parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the fixture checks clean: {report:?}");
    let image =
        ToolResult::success("og", "made").with_structured(json!({ "cost_usd": SPENT, "ok": true }));
    let tools = MockToolExecutor::new()
        .enqueue_ok(image)
        .enqueue_ok(blocked_prompt());
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_max_cost_usd(Some(CEILING)),
    )
    .with_prompt_pause(true);
    let mut sink = TraceFileSink::new(room.join(".nika/traces"));
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("executor");
    let outcome = executor
        .block_on(runtime.run(&wf, &report, &mut SystemStamper::new(), &mut sink))
        .expect("the image's floor fits the paused leg's ceiling");
    assert!(
        outcome.paused.is_some(),
        "the gate parks the run: {outcome:?}"
    );
    sink.finalize();
    let journal = sink.path().expect("the journal opened").to_path_buf();
    assert!(
        sink.error().is_none(),
        "the journal is whole: {:?}",
        sink.error()
    );
    drop(sink);
    journal
}

/// The resumed leg: the real binary, an isolated HOME, no ceiling stated.
fn resume(room: &Path, journal: &Path, answer: &str) -> Output {
    let home = room.join(".home");
    std::fs::create_dir_all(&home).expect("isolated home");
    Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["run", "flow.nika", "--resume"])
        .arg(journal)
        .args([
            "--answer",
            &format!("keep={answer}"),
            "--json",
            "--color",
            "never",
        ])
        .current_dir(room)
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", room.join("absent-key"))
        .env("NIKA_RUN_PUB_FILE", room.join("absent-pub"))
        .output()
        .expect("the binary runs")
}

/// The frames the resumed leg streamed.
fn frames(output: &Output) -> Vec<Value> {
    (String::from_utf8_lossy(&output.stdout).lines())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn field<'v>(frame: &'v Value, key: &str) -> Option<&'v Value> {
    (frame["fields"].as_array()?.iter())
        .find(|kv| kv["key"] == key)
        .map(|kv| &kv["value"])
}

/// The tasks whose frame is `kind`, in stream order.
fn tasks(frames: &[Value], kind: &str) -> Vec<String> {
    (frames.iter().filter(|f| f["kind"] == kind))
        .filter_map(|f| field(f, "task")?.as_str().map(str::to_owned))
        .collect()
}

fn answered(answer: &str) -> (Output, Vec<Value>) {
    let room = tempfile::tempdir().expect("isolated room");
    std::fs::write(room.path().join("flow.nika"), WORKFLOW).expect("workflow");
    let journal = paused_leg(room.path());
    let out = resume(room.path(), &journal, answer);
    let streamed = frames(&out);
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !said.contains("NIKA-1709"),
        "the resume is admitted: {said}"
    );
    assert_eq!(out.status.code(), Some(0), "{said}");
    assert_eq!(tasks(&streamed, "task_cache_hit"), ["og"], "{said}");
    (out, streamed)
}

/// The recorded answer (`allow` · `deny`) of the gate.
fn decision(frames: &[Value]) -> Option<String> {
    (frames.iter().find(|f| f["kind"] == "approval_decided"))
        .and_then(|f| field(f, "decision")?.as_str().map(str::to_owned))
}

#[test]
fn a_no_at_a_gate_resumes_under_what_the_paused_leg_left() {
    let (_, streamed) = answered("false");
    assert_eq!(decision(&streamed).as_deref(), Some("deny"));
    assert_eq!(tasks(&streamed, "task_skipped"), ["note"]);
    assert!(tasks(&streamed, "task_completed").contains(&"keep".to_owned()));
}

#[test]
fn a_yes_at_a_gate_resumes_under_what_the_paused_leg_left() {
    let (_, streamed) = answered("true");
    assert_eq!(decision(&streamed).as_deref(), Some("allow"));
    assert_eq!(tasks(&streamed, "task_completed"), ["keep", "note"]);
}
